//! Direct-API adapter for the Anthropic Messages API.
//!
//! The BYOK path for the `anthropic` provider: a user with an API key gets a
//! working assistant without Claude Code installed. The CLI adapter
//! (`claude_code.rs`) stays alongside it — a subscription seat carries the
//! provider's own auth, quota, and session behavior, which this cannot.
//!
//! Scope is transport plus translation into `StreamEvent`. Routing, quota,
//! and cost stay with DREX and `heiwa_quota`, so `TokenUsage.cost_usd` is
//! left at zero here rather than priced against a table this crate would
//! have to keep current.

use crate::adapter::{Message, ProviderAdapter, Role, StreamEvent, TokenUsage};
use crate::providers::sse::{SseDecoder, SseEvent};
use crate::registry::{DetectedModel, InventoryTruth};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Ceiling for a single response. Anthropic requires `max_tokens`; the
/// provider caps it per model, so an over-large value is clamped upstream.
const DEFAULT_MAX_TOKENS: u32 = 16_000;

pub struct AnthropicApiAdapter {
    account_id: String,
    base_url: String,
    models: Vec<String>,
    /// Caller-supplied credential. When absent the keychain is consulted.
    api_key: Option<String>,
}

impl AnthropicApiAdapter {
    /// Build from the first API-key Anthropic account in the registry.
    pub fn from_registry() -> Option<Self> {
        let registry = crate::AccountRegistry::load();
        let account = registry.accounts.iter().find(|account| {
            account.provider == "anthropic"
                && matches!(account.credential, crate::Credential::ApiKey)
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

    /// Explicit construction. `base_url` is a constructor argument rather
    /// than an env lookup so the fresh-install harness can point the adapter
    /// at a loopback mock without the adapter knowing it is under test.
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

/// Split Heiwa's flat message list into the Messages API shape: system
/// content is a top-level field, and `messages` carries only user/assistant
/// turns.
fn build_request_body(model: &str, messages: &[Message], max_tokens: u32) -> Value {
    let system: Vec<&str> = messages
        .iter()
        .filter(|message| matches!(message.role, Role::System))
        .map(|message| message.content.as_str())
        .collect();

    let turns: Vec<Value> = messages
        .iter()
        .filter(|message| !matches!(message.role, Role::System))
        .map(|message| {
            json!({
                "role": match message.role {
                    Role::Assistant => "assistant",
                    _ => "user",
                },
                "content": message.content,
            })
        })
        .collect();

    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": turns,
        "stream": true,
    });
    if !system.is_empty() {
        body["system"] = Value::String(system.join("\n\n"));
    }
    body
}

/// Translates the Messages API event stream into `StreamEvent`s.
///
/// Kept as a pure state machine so the wire vocabulary is testable without a
/// server: `consume` is the whole translation.
#[derive(Default)]
struct AnthropicStream {
    usage: TokenUsage,
    /// Tool block being assembled, if the open block is a tool_use.
    pending_tool: Option<PendingTool>,
}

#[derive(Default)]
struct PendingTool {
    name: String,
    input: String,
}

impl AnthropicStream {
    fn consume(&mut self, event: &SseEvent) -> Vec<StreamEvent> {
        let Ok(payload) = serde_json::from_str::<Value>(&event.data) else {
            return Vec::new(); // keep-alive or malformed frame
        };
        let kind = event
            .name
            .as_deref()
            .or_else(|| payload["type"].as_str())
            .unwrap_or_default();

        match kind {
            "message_start" => {
                let usage = &payload["message"]["usage"];
                self.usage.input_tokens = usage["input_tokens"].as_u64().unwrap_or(0) as u32;
                self.usage.cache_read_tokens =
                    usage["cache_read_input_tokens"].as_u64().unwrap_or(0) as u32;
                self.usage.cache_write_tokens =
                    usage["cache_creation_input_tokens"].as_u64().unwrap_or(0) as u32;
                Vec::new()
            }
            "content_block_start" => {
                if payload["content_block"]["type"] == "tool_use" {
                    self.pending_tool = Some(PendingTool {
                        name: payload["content_block"]["name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        input: String::new(),
                    });
                }
                Vec::new()
            }
            "content_block_delta" => match payload["delta"]["type"].as_str() {
                Some("text_delta") => payload["delta"]["text"]
                    .as_str()
                    .filter(|text| !text.is_empty())
                    .map(|text| vec![StreamEvent::Token(text.to_string())])
                    .unwrap_or_default(),
                Some("input_json_delta") => {
                    if let (Some(tool), Some(fragment)) = (
                        self.pending_tool.as_mut(),
                        payload["delta"]["partial_json"].as_str(),
                    ) {
                        tool.input.push_str(fragment);
                    }
                    Vec::new()
                }
                // thinking_delta and signature_delta are model-internal.
                _ => Vec::new(),
            },
            "content_block_stop" => self
                .pending_tool
                .take()
                .map(|tool| {
                    vec![StreamEvent::ToolUse {
                        name: tool.name,
                        input: tool.input,
                    }]
                })
                .unwrap_or_default(),
            "message_delta" => {
                if let Some(output) = payload["usage"]["output_tokens"].as_u64() {
                    self.usage.output_tokens = output as u32;
                }
                Vec::new()
            }
            "message_stop" => vec![StreamEvent::Done(self.usage.clone())],
            "error" => {
                let error = &payload["error"];
                vec![StreamEvent::Error(format!(
                    "Anthropic {}: {}",
                    error["type"].as_str().unwrap_or("error"),
                    error["message"].as_str().unwrap_or("unknown error"),
                ))]
            }
            _ => Vec::new(),
        }
    }
}

/// Map `GET /v1/models` into detected models.
///
/// The provider is authoritative for its own inventory, so nothing here
/// invents an entry: an id absent from the response is absent from Heiwa.
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
            Some(DetectedModel {
                model_id: id.to_string(),
                provider_model_id: id.to_string(),
                provider: "anthropic".to_string(),
                account_id: account_id.to_string(),
                rate_group: rate_group.to_string(),
                capability_class: capability_class(id),
                context_window: entry
                    .get("max_input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(200_000) as u32,
                supports_streaming: true,
                supports_tools: true,
                supports_vision: true,
                supports_audio: false,
                // Per-token prices are not on the models endpoint; the cost
                // layer owns pricing.
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                inventory_truth: InventoryTruth::Verified,
            })
        })
        .collect()
}

/// Rough tier from the model family in the id. Routing needs an ordering,
/// and the provider does not publish one.
fn capability_class(id: &str) -> u8 {
    let id = id.to_ascii_lowercase();
    if id.contains("fable") || id.contains("mythos") || id.contains("opus") {
        5
    } else if id.contains("sonnet") {
        4
    } else if id.contains("haiku") {
        2
    } else {
        3
    }
}

/// Probe `GET /v1/models` with an API key: verifies the key and returns the
/// account's inventory in one call.
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
        .get(format!(
            "{}/v1/models?limit=100",
            base_url.trim_end_matches('/')
        ))
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        return Err(anyhow!("Anthropic models HTTP {status}: {detail}"));
    }
    let body: Value = response.json().await?;
    Ok(models_from_list_response(&body, account_id, rate_group))
}

#[async_trait]
impl ProviderAdapter for AnthropicApiAdapter {
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
            .ok_or_else(|| anyhow!("no Anthropic API key for account {}", self.account_id))?;

        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()?;
        let response = client
            .post(format!(
                "{}/v1/messages",
                self.base_url.trim_end_matches('/')
            ))
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&build_request_body(model, messages, DEFAULT_MAX_TOKENS))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            let message = format!("Anthropic HTTP {status}: {detail}");
            let _ = stream_tx.send(StreamEvent::Error(message.clone())).await;
            return Err(anyhow!(message));
        }

        let mut decoder = SseDecoder::new();
        let mut state = AnthropicStream::default();
        let mut body = response.bytes_stream();

        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            for event in decoder.push(&String::from_utf8_lossy(&chunk)) {
                for stream_event in state.consume(&event) {
                    let terminal =
                        matches!(stream_event, StreamEvent::Done(_) | StreamEvent::Error(_));
                    if stream_tx.send(stream_event).await.is_err() {
                        return Ok(()); // consumer went away
                    }
                    if terminal {
                        return Ok(());
                    }
                }
            }
        }

        // Stream ended without message_stop: report what was counted rather
        // than leaving the consumer waiting on a terminal event.
        let _ = stream_tx.send(StreamEvent::Done(state.usage.clone())).await;
        Ok(())
    }

    async fn interrupt(&self) -> Result<()> {
        // Dropping the response stream cancels the request.
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

    fn drain(state: &mut AnthropicStream, name: &str, data: &str) -> Vec<StreamEvent> {
        state.consume(&SseEvent::new(Some(name), data))
    }

    #[test]
    fn emits_a_token_per_text_delta() {
        let mut state = AnthropicStream::default();
        let events = drain(
            &mut state,
            "content_block_delta",
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello"}}"#,
        );
        assert!(matches!(&events[..], [StreamEvent::Token(t)] if t == "hello"));
    }

    #[test]
    fn ignores_thinking_deltas() {
        // Thinking is not assistant output; surfacing it as a Token would put
        // reasoning into the operator transcript.
        let mut state = AnthropicStream::default();
        let events = drain(
            &mut state,
            "content_block_delta",
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hmm"}}"#,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn accumulates_tool_use_across_partial_json_deltas() {
        let mut state = AnthropicStream::default();
        assert!(drain(
            &mut state,
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tu_1","name":"read_file","input":{}}}"#
        )
        .is_empty());
        assert!(drain(
            &mut state,
            "content_block_delta",
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}"#
        )
        .is_empty());
        assert!(drain(
            &mut state,
            "content_block_delta",
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"\"a.rs\"}"}}"#
        )
        .is_empty());
        let events = drain(
            &mut state,
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#,
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
    fn closing_a_text_block_emits_nothing() {
        let mut state = AnthropicStream::default();
        drain(
            &mut state,
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        );
        assert!(drain(
            &mut state,
            "content_block_stop",
            r#"{"type":"content_block_stop","index":0}"#
        )
        .is_empty());
    }

    #[test]
    fn totals_usage_across_message_start_and_delta_then_reports_on_stop() {
        let mut state = AnthropicStream::default();
        drain(
            &mut state,
            "message_start",
            r#"{"type":"message_start","message":{"usage":{"input_tokens":120,"output_tokens":1,"cache_read_input_tokens":40,"cache_creation_input_tokens":10}}}"#,
        );
        drain(
            &mut state,
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":57}}"#,
        );
        let events = drain(&mut state, "message_stop", r#"{"type":"message_stop"}"#);
        match &events[..] {
            [StreamEvent::Done(usage)] => {
                assert_eq!(usage.input_tokens, 120);
                assert_eq!(usage.output_tokens, 57);
                assert_eq!(usage.cache_read_tokens, 40);
                assert_eq!(usage.cache_write_tokens, 10);
                // Cost is DREX/heiwa_quota's concern, not the adapter's.
                assert_eq!(usage.cost_usd, 0.0);
            }
            other => panic!("expected one Done, got {other:?}"),
        }
    }

    #[test]
    fn surfaces_a_mid_stream_error_event() {
        let mut state = AnthropicStream::default();
        let events = drain(
            &mut state,
            "error",
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
        );
        match &events[..] {
            [StreamEvent::Error(message)] => {
                assert!(message.contains("overloaded_error"));
                assert!(message.contains("Overloaded"));
            }
            other => panic!("expected one Error, got {other:?}"),
        }
    }

    #[test]
    fn ignores_ping_and_unparseable_payloads() {
        let mut state = AnthropicStream::default();
        assert!(drain(&mut state, "ping", r#"{"type":"ping"}"#).is_empty());
        assert!(drain(&mut state, "content_block_delta", "not json").is_empty());
    }

    #[test]
    fn request_body_splits_system_messages_into_the_system_field() {
        // The Messages API rejects a `system` role inside `messages`; system
        // content belongs in a top-level field.
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
        let body = build_request_body("claude-opus-5", &messages, 4096);
        assert_eq!(body["system"], "be terse");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn request_body_joins_multiple_system_messages() {
        let messages = vec![
            Message {
                role: Role::System,
                content: "one".to_string(),
            },
            Message {
                role: Role::System,
                content: "two".to_string(),
            },
            Message {
                role: Role::User,
                content: "go".to_string(),
            },
        ];
        let body = build_request_body("claude-opus-5", &messages, 1024);
        assert_eq!(body["system"], "one\n\ntwo");
    }

    #[test]
    fn request_body_omits_system_when_no_system_message_is_present() {
        let messages = vec![Message {
            role: Role::User,
            content: "hi".to_string(),
        }];
        let body = build_request_body("claude-opus-5", &messages, 1024);
        assert!(body.get("system").is_none());
    }

    #[test]
    fn discovery_maps_the_models_endpoint_payload() {
        let body = serde_json::json!({
            "data": [
                {"id": "claude-opus-5", "display_name": "Claude Opus 5",
                 "max_input_tokens": 1000000, "max_tokens": 128000},
                {"id": "claude-haiku-4-5", "display_name": "Claude Haiku 4.5",
                 "max_input_tokens": 200000, "max_tokens": 64000}
            ],
            "has_more": false
        });
        let models = models_from_list_response(&body, "anthropic-api-1", "anthropic_api");
        assert_eq!(models.len(), 2);
        let opus = &models[0];
        assert_eq!(opus.provider, "anthropic");
        assert_eq!(opus.provider_model_id, "claude-opus-5");
        assert_eq!(opus.account_id, "anthropic-api-1");
        assert_eq!(opus.rate_group, "anthropic_api");
        assert_eq!(opus.context_window, 1_000_000);
        // Discovered from the provider's own endpoint, so Verified — never
        // Inferred from a list this crate carries.
        assert_eq!(opus.inventory_truth, InventoryTruth::Verified);
        assert!(opus.supports_streaming);
    }

    #[test]
    fn discovery_returns_empty_for_a_malformed_payload() {
        let models = models_from_list_response(&serde_json::json!({}), "a", "g");
        assert!(models.is_empty());
    }
}
