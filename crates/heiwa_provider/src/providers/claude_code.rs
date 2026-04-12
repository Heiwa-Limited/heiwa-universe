use crate::adapter::{Message, ProviderAdapter, StreamEvent, TokenUsage};
use anyhow::Result;
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// CLI subprocess adapter for Claude Code.
///
/// Wraps `claude -p <prompt> --output-format stream-json --verbose --model <model>`.
/// For BYOK API key users, the Anthropic HTTP API adapter is preferred.
/// This adapter is for subscription users who have Claude Code installed.
pub struct ClaudeCodeCliAdapter;

impl ClaudeCodeCliAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProviderAdapter for ClaudeCodeCliAdapter {
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

        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if !model.is_empty() {
            cmd.arg("--model").arg(model);
        }

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout).lines();

        while let Some(line) = reader.next_line().await? {
            // Claude Code stream-json emits JSON objects per line.
            // Extract text content from assistant messages.
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) {
                if obj.get("type").and_then(|t| t.as_str()) == Some("assistant") {
                    if let Some(msg) = obj.get("message") {
                        if let Some(content) = msg.get("content") {
                            if let Some(arr) = content.as_array() {
                                for block in arr {
                                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                                        if let Some(text) =
                                            block.get("text").and_then(|t| t.as_str())
                                        {
                                            if stream_tx
                                                .send(StreamEvent::Token(text.to_string()))
                                                .await
                                                .is_err()
                                            {
                                                child.kill().await.ok();
                                                return Ok(());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Extract usage from result events
                if obj.get("type").and_then(|t| t.as_str()) == Some("result") {
                    let usage = extract_usage(&obj);
                    let _ = stream_tx.send(StreamEvent::Done(usage)).await;
                    return Ok(());
                }
            }
        }

        // If we get here without a result event, send Done anyway
        let _ = stream_tx
            .send(StreamEvent::Done(TokenUsage::default()))
            .await;
        Ok(())
    }

    async fn interrupt(&self) -> Result<()> {
        Ok(())
    }

    fn supported_models(&self) -> Vec<String> {
        vec![
            "claude-sonnet-4-6".to_string(),
            "claude-opus-4-6".to_string(),
            "claude-haiku-4-5".to_string(),
        ]
    }
}

fn extract_usage(result: &serde_json::Value) -> TokenUsage {
    let usage = result.get("usage").unwrap_or(result);
    TokenUsage {
        input_tokens: usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        output_tokens: usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        cache_read_tokens: usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        cache_write_tokens: usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        cost_usd: result
            .get("total_cost_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    }
}
