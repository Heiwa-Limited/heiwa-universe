//! HTTP API adapter for OpenRouter.
//!
//! Streams chat completions from the OpenAI-compatible endpoint at
//! `https://openrouter.ai/api/v1`, authenticating with the API key stored
//! in the OS keychain under the account's `account_id`. The stream
//! vocabulary is shared with `openai_api` — OpenRouter's differences are the
//! endpoint, the attribution headers, and free-tier 429 retries.

use crate::adapter::{Message, ProviderAdapter, StreamEvent};
use crate::providers::openai_api::{pump_openai_stream, role_str};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::json;
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
                "role": role_str(&m.role),
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

        // Framing, tool-call assembly, usage, and terminal-event handling are
        // shared with the OpenAI adapter — OpenRouter speaks the same wire
        // format, so the vocabulary is implemented once.
        pump_openai_stream(resp, stream_tx).await
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
