//! Direct-API adapter for the Google Generative Language API.
//!
//! The BYOK path for the `google` provider. It matters more than the other
//! two: the Gemini CLI's free individual tier ended, so on this machine the
//! CLI adapter has no working auth at all. A user with an API key gets a
//! working Google route without it.

use crate::adapter::{Message, ProviderAdapter, Role, StreamEvent, TokenUsage};
use crate::providers::sse::{SseDecoder, SseEvent};
use crate::registry::{DetectedModel, InventoryTruth};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
const API_VERSION: &str = "v1beta";

pub struct GeminiApiAdapter {
    account_id: String,
    base_url: String,
    models: Vec<String>,
    /// Caller-supplied credential. When absent the keychain is consulted.
    api_key: Option<String>,
}

impl GeminiApiAdapter {
    pub fn from_registry() -> Option<Self> {
        let registry = crate::AccountRegistry::load();
        let account = registry.accounts.iter().find(|account| {
            account.provider == "google" && matches!(account.credential, crate::Credential::ApiKey)
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

/// Map Heiwa messages onto `contents` + `systemInstruction`. Gemini names
/// the assistant role "model" and has no system role inside `contents`.
fn build_request_body(messages: &[Message]) -> Value {
    let system: Vec<&str> = messages
        .iter()
        .filter(|message| matches!(message.role, Role::System))
        .map(|message| message.content.as_str())
        .collect();

    let contents: Vec<Value> = messages
        .iter()
        .filter(|message| !matches!(message.role, Role::System))
        .map(|message| {
            json!({
                "role": match message.role {
                    Role::Assistant => "model",
                    _ => "user",
                },
                "parts": [{ "text": message.content }],
            })
        })
        .collect();

    let mut body = json!({ "contents": contents });
    if !system.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": system.join("\n\n") }] });
    }
    body
}

/// Translates `streamGenerateContent` chunks into `StreamEvent`s.
#[derive(Default)]
struct GeminiStream {
    usage: TokenUsage,
    done_emitted: bool,
}

impl GeminiStream {
    fn consume(&mut self, event: &SseEvent) -> Vec<StreamEvent> {
        let Ok(payload) = serde_json::from_str::<Value>(&event.data) else {
            return Vec::new();
        };

        if let Some(error) = payload.get("error").filter(|value| !value.is_null()) {
            return vec![StreamEvent::Error(format!(
                "Gemini {}: {}",
                error["status"].as_str().unwrap_or("error"),
                error["message"].as_str().unwrap_or("unknown error"),
            ))];
        }

        if let Some(usage) = payload.get("usageMetadata").filter(|v| !v.is_null()) {
            self.usage.input_tokens = usage["promptTokenCount"].as_u64().unwrap_or(0) as u32;
            self.usage.output_tokens = usage["candidatesTokenCount"].as_u64().unwrap_or(0) as u32;
            self.usage.cache_read_tokens =
                usage["cachedContentTokenCount"].as_u64().unwrap_or(0) as u32;
        }

        let mut events = Vec::new();
        if let Some(parts) = payload["candidates"][0]["content"]["parts"].as_array() {
            for part in parts {
                if let Some(text) = part["text"].as_str().filter(|text| !text.is_empty()) {
                    events.push(StreamEvent::Token(text.to_string()));
                }
                if let Some(call) = part.get("functionCall").filter(|value| !value.is_null()) {
                    events.push(StreamEvent::ToolUse {
                        name: call["name"].as_str().unwrap_or_default().to_string(),
                        input: call
                            .get("args")
                            .map(|args| args.to_string())
                            .unwrap_or_else(|| "{}".to_string()),
                    });
                }
            }
        }
        events
    }

    fn finish(&mut self) -> Vec<StreamEvent> {
        if self.done_emitted {
            return Vec::new();
        }
        self.done_emitted = true;
        vec![StreamEvent::Done(self.usage.clone())]
    }
}

fn capability_class(id: &str) -> u8 {
    let id = id.to_ascii_lowercase();
    if id.contains("ultra") {
        5
    } else if id.contains("pro") {
        4
    } else if id.contains("flash-lite") || id.contains("nano") {
        2
    } else {
        // flash and anything unrecognized sit in the middle tier.
        3
    }
}

fn models_from_list_response(
    body: &Value,
    account_id: &str,
    rate_group: &str,
) -> Vec<DetectedModel> {
    let Some(models) = body.get("models").and_then(Value::as_array) else {
        return Vec::new();
    };
    models
        .iter()
        .filter_map(|entry| {
            // Names are returned fully qualified ("models/gemini-3-pro").
            let id = entry.get("name")?.as_str()?.trim_start_matches("models/");
            let methods: Vec<&str> = entry
                .get("supportedGenerationMethods")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if !methods.contains(&"streamGenerateContent") {
                return None;
            }
            Some(DetectedModel {
                model_id: id.to_string(),
                provider_model_id: id.to_string(),
                provider: "google".to_string(),
                account_id: account_id.to_string(),
                rate_group: rate_group.to_string(),
                capability_class: capability_class(id),
                context_window: entry
                    .get("inputTokenLimit")
                    .and_then(Value::as_u64)
                    .unwrap_or(1_048_576) as u32,
                supports_streaming: true,
                supports_tools: true,
                supports_vision: true,
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
        .get(format!(
            "{}/{API_VERSION}/models?pageSize=200",
            base_url.trim_end_matches('/')
        ))
        // Header rather than a query parameter: a key in the URL lands in
        // proxy and server logs.
        .header("x-goog-api-key", api_key)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        return Err(anyhow!("Gemini models HTTP {status}: {detail}"));
    }
    let body: Value = response.json().await?;
    Ok(models_from_list_response(&body, account_id, rate_group))
}

#[async_trait]
impl ProviderAdapter for GeminiApiAdapter {
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
            .ok_or_else(|| anyhow!("no Google API key in keychain for {}", self.account_id))?;

        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()?;
        let response = client
            .post(format!(
                "{}/{API_VERSION}/models/{model}:streamGenerateContent?alt=sse",
                self.base_url.trim_end_matches('/')
            ))
            .header("x-goog-api-key", api_key)
            .json(&build_request_body(messages))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            let message = format!("Gemini HTTP {status}: {detail}");
            let _ = stream_tx.send(StreamEvent::Error(message.clone())).await;
            return Err(anyhow!(message));
        }

        let mut decoder = SseDecoder::new();
        let mut state = GeminiStream::default();
        let mut body = response.bytes_stream();

        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            for event in decoder.push(&String::from_utf8_lossy(&chunk)) {
                for stream_event in state.consume(&event) {
                    let terminal =
                        matches!(stream_event, StreamEvent::Done(_) | StreamEvent::Error(_));
                    if stream_tx.send(stream_event).await.is_err() {
                        return Ok(());
                    }
                    if terminal {
                        return Ok(());
                    }
                }
            }
        }

        for stream_event in state.finish() {
            if stream_tx.send(stream_event).await.is_err() {
                return Ok(());
            }
        }
        Ok(())
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

    fn drain(state: &mut GeminiStream, data: &str) -> Vec<StreamEvent> {
        state.consume(&SseEvent::new(None, data))
    }

    #[test]
    fn emits_a_token_per_text_part() {
        let mut state = GeminiStream::default();
        let events = drain(
            &mut state,
            r#"{"candidates":[{"content":{"parts":[{"text":"hello"}],"role":"model"}}]}"#,
        );
        assert!(matches!(&events[..], [StreamEvent::Token(t)] if t == "hello"));
    }

    #[test]
    fn emits_one_token_per_part_in_a_multi_part_chunk() {
        let mut state = GeminiStream::default();
        let events = drain(
            &mut state,
            r#"{"candidates":[{"content":{"parts":[{"text":"a"},{"text":"b"}]}}]}"#,
        );
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn maps_a_function_call_part_to_tool_use() {
        let mut state = GeminiStream::default();
        let events = drain(
            &mut state,
            r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"read_file","args":{"path":"a.rs"}}}]}}]}"#,
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
    fn tracks_usage_metadata_and_reports_it_on_finish() {
        let mut state = GeminiStream::default();
        drain(
            &mut state,
            r#"{"candidates":[{"content":{"parts":[{"text":"x"}]}}],"usageMetadata":{"promptTokenCount":19,"candidatesTokenCount":4,"cachedContentTokenCount":3}}"#,
        );
        match &state.finish()[..] {
            [StreamEvent::Done(usage)] => {
                assert_eq!(usage.input_tokens, 19);
                assert_eq!(usage.output_tokens, 4);
                assert_eq!(usage.cache_read_tokens, 3);
                assert_eq!(usage.cost_usd, 0.0);
            }
            other => panic!("expected one Done, got {other:?}"),
        }
    }

    #[test]
    fn surfaces_an_error_payload() {
        let mut state = GeminiStream::default();
        let events = drain(
            &mut state,
            r#"{"error":{"status":"PERMISSION_DENIED","message":"API key not valid"}}"#,
        );
        match &events[..] {
            [StreamEvent::Error(message)] => assert!(message.contains("API key not valid")),
            other => panic!("expected one Error, got {other:?}"),
        }
    }

    #[test]
    fn request_body_maps_roles_and_lifts_system_into_system_instruction() {
        let messages = vec![
            Message {
                role: Role::System,
                content: "be terse".to_string(),
            },
            Message {
                role: Role::User,
                content: "hi".to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "hello".to_string(),
            },
        ];
        let body = build_request_body(&messages);
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "be terse");
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0]["role"], "user");
        // Gemini names the assistant role "model".
        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[1]["parts"][0]["text"], "hello");
    }

    #[test]
    fn discovery_strips_the_models_prefix_and_keeps_generation_capable_entries() {
        let body = serde_json::json!({
            "models": [
                {"name": "models/gemini-3-pro", "inputTokenLimit": 1048576,
                 "supportedGenerationMethods": ["generateContent", "streamGenerateContent"]},
                {"name": "models/text-embedding-004", "inputTokenLimit": 2048,
                 "supportedGenerationMethods": ["embedContent"]}
            ]
        });
        let models = models_from_list_response(&body, "google-api-1", "google_api");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].provider_model_id, "gemini-3-pro");
        assert_eq!(models[0].provider, "google");
        assert_eq!(models[0].context_window, 1_048_576);
        assert_eq!(models[0].inventory_truth, InventoryTruth::Verified);
    }
}
