//! HTTP API adapter for OpenRouter.
//!
//! First BYOK HTTP adapter (the other adapters wrap provider CLIs).
//! Streams chat completions from the OpenAI-compatible endpoint at
//! `https://openrouter.ai/api/v1`, authenticating with the API key stored
//! in the OS keychain under the account's `account_id`.

use crate::adapter::{Message, ProviderAdapter, Role, StreamEvent, TokenUsage};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;

const OPENROUTER_CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

pub struct OpenRouterAdapter {
    account_id: String,
    models: Vec<String>,
}

impl OpenRouterAdapter {
    /// Build from the first connected OpenRouter account in the registry.
    /// Returns `None` when no OpenRouter account is registered.
    pub fn from_registry() -> Option<Self> {
        let registry = crate::AccountRegistry::load();
        let account = registry
            .accounts
            .iter()
            .find(|a| a.provider == "openrouter")?;
        Some(Self {
            account_id: account.account_id.clone(),
            models: account
                .models
                .iter()
                .map(|m| m.provider_model_id.clone())
                .collect(),
        })
    }

    fn role_str(role: &Role) -> &'static str {
        match role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
        }
    }
}

/// Parse one SSE `data:` payload from an OpenAI-compatible stream.
///
/// Returns the text delta (if any) and usage (if this is the final chunk
/// that carries it). `[DONE]` is handled by the caller.
fn parse_stream_payload(payload: &str) -> (Option<String>, Option<TokenUsage>) {
    let Ok(v) = serde_json::from_str::<Value>(payload) else {
        return (None, None);
    };
    let delta = v["choices"][0]["delta"]["content"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let usage = v.get("usage").filter(|u| !u.is_null()).map(|u| TokenUsage {
        input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
        output_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        cost_usd: u["cost"].as_f64().unwrap_or(0.0),
    });
    (delta, usage)
}

#[async_trait]
impl ProviderAdapter for OpenRouterAdapter {
    async fn send(
        &self,
        model: &str,
        messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        let api_key = crate::registry::resolve_secret(&self.account_id)
            .ok_or_else(|| anyhow!("no OpenRouter key in keychain for {}", self.account_id))?;

        let body = json!({
            "model": model,
            "messages": messages.iter().map(|m| json!({
                "role": Self::role_str(&m.role),
                "content": m.content,
            })).collect::<Vec<_>>(),
            "stream": true,
            "usage": { "include": true },
        });

        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()?;

        // Free-tier models 429 routinely; retry a bounded number of times,
        // honoring Retry-After (capped so a hostile header can't stall us).
        const MAX_ATTEMPTS: u32 = 3;
        const MAX_BACKOFF_SECS: u64 = 30;
        let mut resp = None;
        for attempt in 1..=MAX_ATTEMPTS {
            let r = client
                .post(OPENROUTER_CHAT_URL)
                .bearer_auth(&api_key)
                .header("HTTP-Referer", "https://heiwa.ltd")
                .header("X-Title", "Heiwa")
                .json(&body)
                .send()
                .await?;

            if r.status().as_u16() == 429 && attempt < MAX_ATTEMPTS {
                let wait = r
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(10)
                    .min(MAX_BACKOFF_SECS);
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                continue;
            }
            resp = Some(r);
            break;
        }
        let resp = resp.expect("loop always sets resp on final attempt");

        if !resp.status().is_success() {
            let status = resp.status();
            let detail = resp.text().await.unwrap_or_default();
            let msg = format!("OpenRouter HTTP {status}: {detail}");
            let _ = stream_tx.send(StreamEvent::Error(msg.clone())).await;
            return Err(anyhow!(msg));
        }

        let mut stream = resp.bytes_stream();
        let mut carry = String::new();
        let mut usage = TokenUsage::default();

        'outer: while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            carry.push_str(&String::from_utf8_lossy(&chunk));

            // SSE events are separated by a blank line.
            while let Some(sep) = carry.find("\n\n") {
                let event: String = carry.drain(..sep + 2).collect();
                for line in event.lines() {
                    let Some(payload) = line.strip_prefix("data: ") else {
                        continue; // comments / keep-alives
                    };
                    if payload == "[DONE]" {
                        break 'outer;
                    }
                    let (delta, chunk_usage) = parse_stream_payload(payload);
                    if let Some(u) = chunk_usage {
                        usage = u;
                    }
                    if let Some(text) = delta {
                        if stream_tx.send(StreamEvent::Token(text)).await.is_err() {
                            return Ok(()); // consumer went away
                        }
                    }
                }
            }
        }

        let _ = stream_tx.send(StreamEvent::Done(usage)).await;
        Ok(())
    }

    async fn interrupt(&self) -> Result<()> {
        // Dropping the response stream cancels the HTTP request; nothing
        // further to signal server-side.
        Ok(())
    }

    fn supported_models(&self) -> Vec<String> {
        self.models.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_delta() {
        let (delta, usage) = parse_stream_payload(r#"{"choices":[{"delta":{"content":"hello"}}]}"#);
        assert_eq!(delta.as_deref(), Some("hello"));
        assert!(usage.is_none());
    }

    #[test]
    fn parses_final_usage_chunk() {
        let (delta, usage) = parse_stream_payload(
            r#"{"choices":[{"delta":{}}],"usage":{"prompt_tokens":12,"completion_tokens":34,"cost":0.0}}"#,
        );
        assert!(delta.is_none());
        let usage = usage.expect("usage present");
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 34);
    }

    #[test]
    fn ignores_malformed_payload() {
        let (delta, usage) = parse_stream_payload("not json");
        assert!(delta.is_none());
        assert!(usage.is_none());
    }

    #[test]
    fn empty_delta_yields_no_token() {
        let (delta, _) = parse_stream_payload(r#"{"choices":[{"delta":{"content":""}}]}"#);
        assert!(delta.is_none());
    }
}
