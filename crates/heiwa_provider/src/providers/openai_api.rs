//! Direct-API adapter for OpenAI-compatible Chat Completions endpoints.
//!
//! The BYOK path for the `openai` provider, alongside the Codex CLI adapter.
//! The stream translation here is shared with OpenRouter, which speaks the
//! same wire format — one implementation of the Chat Completions vocabulary
//! rather than one per provider that happens to use it.

use crate::adapter::{Message, ProviderAdapter, Role, StreamEvent, TokenUsage};
use crate::providers::sse::SseEvent;
use crate::providers::stream::{pump, SseConsumer};
use crate::registry::{DetectedModel, InventoryTruth};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tokio::sync::mpsc;

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com";

pub struct OpenAiApiAdapter {
    account_id: String,
    base_url: String,
    models: Vec<String>,
    /// Caller-supplied credential. When absent the keychain is consulted.
    api_key: Option<String>,
}

impl OpenAiApiAdapter {
    pub fn from_registry() -> Option<Self> {
        let registry = crate::AccountRegistry::load();
        let account = registry.accounts.iter().find(|account| {
            account.provider == "openai" && matches!(account.credential, crate::Credential::ApiKey)
        })?;
        Some(Self {
            account_id: account.account_id.clone(),
            base_url: DEFAULT_BASE_URL.to_string(),
            models: account
                .models
                .iter()
                .map(|model| model.provider_model_id.clone())
                .collect(),
            api_key: None,
        })
    }

    /// `base_url` is a constructor argument so the fresh-install harness can
    /// point the adapter at a loopback mock.
    pub fn new(account_id: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            base_url: base_url.into(),
            models: Vec::new(),
            api_key: None,
        }
    }

    /// Supply the credential directly instead of reading the OS keychain.
    ///
    /// Not every deployment has a keychain: a headless server or container
    /// holds its secrets elsewhere, and the embedder is the one that knows
    /// where. The keychain stays the default for desktop installs.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn with_models(mut self, models: Vec<String>) -> Self {
        self.models = models;
        self
    }
}

pub(crate) fn role_str(role: &Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
    }
}

fn build_request_body(model: &str, messages: &[Message]) -> Value {
    json!({
        "model": model,
        "messages": messages.iter().map(|message| json!({
            "role": role_str(&message.role),
            "content": message.content,
        })).collect::<Vec<_>>(),
        "stream": true,
        "stream_options": { "include_usage": true },
    })
}

/// Translates the Chat Completions event stream into `StreamEvent`s.
///
/// Shared with OpenRouter. Kept as a pure state machine so the wire
/// vocabulary is testable without a server.
#[derive(Default)]
pub(crate) struct OpenAiStream {
    usage: TokenUsage,
    /// Tool calls arrive as fragments keyed by index; BTreeMap keeps the
    /// emission order stable regardless of fragment arrival order.
    tool_calls: BTreeMap<u64, PendingToolCall>,
    done_emitted: bool,
}

#[derive(Default)]
struct PendingToolCall {
    name: String,
    arguments: String,
}

impl SseConsumer for OpenAiStream {
    fn consume(&mut self, event: &SseEvent) -> Vec<StreamEvent> {
        if event.is_done() {
            return self.finish();
        }
        let Ok(payload) = serde_json::from_str::<Value>(&event.data) else {
            return Vec::new();
        };

        if let Some(error) = payload.get("error").filter(|value| !value.is_null()) {
            return vec![StreamEvent::Error(format!(
                "OpenAI {}: {}",
                error["type"].as_str().unwrap_or("error"),
                error["message"].as_str().unwrap_or("unknown error"),
            ))];
        }

        if let Some(usage) = payload.get("usage").filter(|value| !value.is_null()) {
            self.usage.input_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0) as u32;
            self.usage.output_tokens = usage["completion_tokens"].as_u64().unwrap_or(0) as u32;
            self.usage.cache_read_tokens = usage["prompt_tokens_details"]["cached_tokens"]
                .as_u64()
                .unwrap_or(0) as u32;
            // OpenRouter reports the charged cost on the final chunk; OpenAI
            // itself does not send this field, so it stays zero there and the
            // cost layer prices the turn from tokens.
            if let Some(cost) = usage.get("cost").and_then(Value::as_f64) {
                self.usage.cost_usd = cost;
            }
        }

        let mut events = Vec::new();
        let choice = &payload["choices"][0];

        if let Some(text) = choice["delta"]["content"]
            .as_str()
            .filter(|text| !text.is_empty())
        {
            events.push(StreamEvent::Token(text.to_string()));
        }

        if let Some(calls) = choice["delta"]["tool_calls"].as_array() {
            for call in calls {
                let index = call["index"].as_u64().unwrap_or(0);
                let entry = self.tool_calls.entry(index).or_default();
                if let Some(name) = call["function"]["name"].as_str() {
                    // Assign, not append: OpenAI sends the name once, but
                    // several OpenAI-compatible servers repeat it on every
                    // fragment, which appending turns into "readread…".
                    entry.name = name.to_string();
                }
                if let Some(fragment) = call["function"]["arguments"].as_str() {
                    entry.arguments.push_str(fragment);
                }
            }
        }

        if choice["finish_reason"].as_str() == Some("tool_calls") {
            events.extend(self.flush_tool_calls());
        }
        events
    }

    /// Terminal event. Any tool calls still buffered are flushed first so a
    /// stream that ends without an explicit finish_reason does not drop them.
    fn finish(&mut self) -> Vec<StreamEvent> {
        if self.done_emitted {
            return Vec::new();
        }
        self.done_emitted = true;
        let mut events = self.flush_tool_calls();
        events.push(StreamEvent::Done(self.usage.clone()));
        events
    }
}

impl OpenAiStream {
    fn flush_tool_calls(&mut self) -> Vec<StreamEvent> {
        std::mem::take(&mut self.tool_calls)
            .into_values()
            .map(|call| StreamEvent::ToolUse {
                name: call.name,
                input: call.arguments,
            })
            .collect()
    }
}

/// Chat-capable model ids. The list endpoint returns embeddings, audio, and
/// image models too, and routing must not offer those as chat routes.
fn is_chat_model(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    if id.contains("embedding")
        || id.contains("whisper")
        || id.contains("tts")
        || id.contains("dall-e")
        || id.contains("moderation")
        || id.contains("realtime")
        || id.contains("audio")
        || id.contains("image")
    {
        return false;
    }
    id.starts_with("gpt-") || id.starts_with("o1") || id.starts_with("o3") || id.starts_with("o4")
}

fn capability_class(id: &str) -> u8 {
    let id = id.to_ascii_lowercase();
    if id.starts_with("o3") || id.starts_with("o4") || id.starts_with("gpt-5") {
        5
    } else if id.starts_with("gpt-4") || id.starts_with("o1") {
        4
    } else if id.contains("mini") || id.contains("nano") {
        2
    } else {
        3
    }
}

fn models_from_list_response(
    body: &Value,
    account_id: &str,
    rate_group: &str,
) -> Vec<DetectedModel> {
    let Some(data) = body.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    data.iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?;
            if !is_chat_model(id) {
                return None;
            }
            Some(DetectedModel {
                model_id: id.to_string(),
                provider_model_id: id.to_string(),
                provider: "openai".to_string(),
                account_id: account_id.to_string(),
                rate_group: rate_group.to_string(),
                capability_class: capability_class(id),
                // The list endpoint does not publish context windows.
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: true,
                supports_vision: id.contains("gpt-4") || id.contains("gpt-5"),
                supports_audio: false,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                inventory_truth: InventoryTruth::Verified,
            })
        })
        .collect()
}

pub async fn discover_models(
    api_key: &str,
    base_url: &str,
    account_id: &str,
    rate_group: &str,
) -> Result<Vec<DetectedModel>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let response = client
        .get(format!("{}/v1/models", base_url.trim_end_matches('/')))
        .bearer_auth(api_key)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        // Carry the status as a value. Classifying by substring-matching the
        // message body once marked a working key invalid because an upstream
        // 500 body happened to contain a request id with "401" in it.
        return Err(crate::detect::DiscoveryError {
            status: status.as_u16(),
            detail,
        }
        .into());
    }
    let body: Value = response.json().await?;
    Ok(models_from_list_response(&body, account_id, rate_group))
}

/// Drive an OpenAI-compatible SSE response into `StreamEvent`s.
///
/// Shared by this adapter and OpenRouter so the wire vocabulary exists once.
pub(crate) async fn pump_openai_stream(
    response: reqwest::Response,
    stream_tx: mpsc::Sender<StreamEvent>,
    provider: &str,
) -> Result<()> {
    pump(response, OpenAiStream::default(), stream_tx, provider).await
}

#[async_trait]
impl ProviderAdapter for OpenAiApiAdapter {
    async fn send(
        &self,
        model: &str,
        messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        let api_key = self
            .api_key
            .clone()
            .or_else(|| crate::registry::resolve_secret(&self.account_id))
            .ok_or_else(|| anyhow!("no OpenAI API key in keychain for {}", self.account_id))?;

        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()?;
        let response = client
            .post(format!(
                "{}/v1/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&api_key)
            .json(&build_request_body(model, messages))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            let message = format!("OpenAI HTTP {status}: {detail}");
            let _ = stream_tx.send(StreamEvent::Error(message.clone())).await;
            return Err(anyhow!(message));
        }
        pump_openai_stream(response, stream_tx, "OpenAI").await
    }

    async fn interrupt(&self) -> Result<()> {
        Ok(())
    }

    fn supported_models(&self) -> Vec<String> {
        self.models.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::StreamEvent;
    use crate::providers::sse::SseEvent;

    fn drain(state: &mut OpenAiStream, data: &str) -> Vec<StreamEvent> {
        state.consume(&SseEvent::new(None, data))
    }

    #[test]
    fn emits_a_token_per_content_delta() {
        let mut state = OpenAiStream::default();
        let events = drain(&mut state, r#"{"choices":[{"delta":{"content":"hello"}}]}"#);
        assert!(matches!(&events[..], [StreamEvent::Token(t)] if t == "hello"));
    }

    #[test]
    fn ignores_empty_and_absent_content_deltas() {
        let mut state = OpenAiStream::default();
        assert!(drain(&mut state, r#"{"choices":[{"delta":{"content":""}}]}"#).is_empty());
        assert!(drain(
            &mut state,
            r#"{"choices":[{"delta":{"role":"assistant"}}]}"#
        )
        .is_empty());
    }

    #[test]
    fn accumulates_tool_calls_across_argument_fragments() {
        let mut state = OpenAiStream::default();
        assert!(drain(
            &mut state,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"read_file","arguments":"{\"path\":"}}]}}]}"#
        )
        .is_empty());
        assert!(drain(
            &mut state,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"a.rs\"}"}}]}}]}"#
        )
        .is_empty());
        let events = drain(
            &mut state,
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        );
        match &events[..] {
            [StreamEvent::ToolUse { name, input }] => {
                assert_eq!(name, "read_file");
                assert_eq!(input, r#"{"path":"a.rs"}"#);
            }
            other => panic!("expected one ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn emits_parallel_tool_calls_in_index_order() {
        let mut state = OpenAiStream::default();
        drain(
            &mut state,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"name":"b","arguments":"{}"}}]}}]}"#,
        );
        drain(
            &mut state,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"a","arguments":"{}"}}]}}]}"#,
        );
        let events = drain(
            &mut state,
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        );
        let names: Vec<&str> = events
            .iter()
            .filter_map(|event| match event {
                StreamEvent::ToolUse { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn reads_usage_from_the_final_chunk_and_reports_on_done() {
        let mut state = OpenAiStream::default();
        drain(
            &mut state,
            r#"{"choices":[],"usage":{"prompt_tokens":31,"completion_tokens":12,"prompt_tokens_details":{"cached_tokens":8}}}"#,
        );
        let events = state.finish();
        match &events[..] {
            [StreamEvent::Done(usage)] => {
                assert_eq!(usage.input_tokens, 31);
                assert_eq!(usage.output_tokens, 12);
                assert_eq!(usage.cache_read_tokens, 8);
                assert_eq!(usage.cost_usd, 0.0);
            }
            other => panic!("expected one Done, got {other:?}"),
        }
    }

    #[test]
    fn surfaces_an_error_payload() {
        let mut state = OpenAiStream::default();
        let events = drain(
            &mut state,
            r#"{"error":{"type":"invalid_request_error","message":"bad model"}}"#,
        );
        match &events[..] {
            [StreamEvent::Error(message)] => assert!(message.contains("bad model")),
            other => panic!("expected one Error, got {other:?}"),
        }
    }

    #[test]
    fn request_body_keeps_system_messages_inline() {
        // Chat Completions takes system as an ordinary message, unlike the
        // Anthropic Messages API.
        let messages = vec![
            Message {
                role: Role::System,
                content: "be terse".to_string(),
            },
            Message {
                role: Role::User,
                content: "hi".to_string(),
            },
        ];
        let body = build_request_body("gpt-5", &messages);
        let turns = body["messages"].as_array().unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0]["role"], "system");
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn discovery_keeps_only_chat_capable_models() {
        let body = serde_json::json!({
            "data": [
                {"id": "gpt-5"},
                {"id": "text-embedding-3-large"},
                {"id": "o4-mini"},
                {"id": "dall-e-3"},
                {"id": "whisper-1"}
            ]
        });
        let models = models_from_list_response(&body, "openai-api-1", "openai_api");
        let ids: Vec<&str> = models
            .iter()
            .map(|model| model.provider_model_id.as_str())
            .collect();
        assert_eq!(ids, vec!["gpt-5", "o4-mini"]);
        assert!(models
            .iter()
            .all(|model| model.inventory_truth == InventoryTruth::Verified));
        assert!(models.iter().all(|model| model.provider == "openai"));
    }
}
