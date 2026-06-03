use crate::adapter::{Message, ProviderAdapter, StreamEvent, TokenUsage};
use anyhow::Result;
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// CLI subprocess adapter for Gemini CLI.
///
/// Wraps `gemini -p <prompt> --output-format stream-json --model <model>`.
/// This adapter is for users who have Gemini CLI installed and authenticated.
pub struct GeminiCliAdapter;

impl GeminiCliAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProviderAdapter for GeminiCliAdapter {
    async fn send(
        &self,
        model: &str,
        messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        let prompt: String = messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let mut cmd = Command::new(crate::resolve_command_or_name("gemini"));
        cmd.arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("stream-json")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if !model.is_empty() {
            cmd.arg("--model").arg(model);
        }

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout).lines();

        while let Some(line) = reader.next_line().await? {
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) {
                // Gemini CLI stream-json format:
                // { "type": "message", "content": "..." }
                // { "type": "result", "stats": { ... } }
                match obj.get("type").and_then(|t| t.as_str()) {
                    Some("message") => {
                        if let Some(content) = obj.get("content").and_then(|c| c.as_str()) {
                            if stream_tx
                                .send(StreamEvent::Token(content.to_string()))
                                .await
                                .is_err()
                            {
                                child.kill().await.ok();
                                return Ok(());
                            }
                        }
                    }
                    Some("result") => {
                        let usage = extract_usage(&obj);
                        let _ = stream_tx.send(StreamEvent::Done(usage)).await;
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }

        let _ = stream_tx
            .send(StreamEvent::Done(TokenUsage::default()))
            .await;
        Ok(())
    }

    async fn interrupt(&self) -> Result<()> {
        Ok(())
    }

    fn supported_models(&self) -> Vec<String> {
        vec!["gemini-3.1-pro".to_string(), "gemini-3-flash".to_string()]
    }
}

fn extract_usage(result: &serde_json::Value) -> TokenUsage {
    // Expected: { "stats": { "input_tokens": 123, "output_tokens": 456, "total_cost_usd": 0.0 } }
    let stats = result.get("stats").unwrap_or(result);
    TokenUsage {
        input_tokens: stats
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        output_tokens: stats
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        cost_usd: stats
            .get("total_cost_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    }
}
