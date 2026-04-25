use crate::adapter::{Message, ProviderAdapter, StreamEvent, TokenUsage};
use anyhow::Result;
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// CLI subprocess adapter for OpenAI Codex.
///
/// Wraps `codex exec --json [-m <model>] <prompt>`.
/// For ChatGPT-Plus subscription users authenticated via `codex login`.
pub struct CodexCliAdapter;

impl CodexCliAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProviderAdapter for CodexCliAdapter {
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

        let mut cmd = Command::new("codex");
        cmd.arg("exec")
            .arg("--json")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if !model.is_empty() {
            cmd.arg("-m").arg(model);
        }

        cmd.arg(&prompt);

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout).lines();

        while let Some(line) = reader.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            match obj.get("type").and_then(|t| t.as_str()) {
                Some("agent_message_delta") | Some("message_delta") => {
                    if let Some(delta) = obj.get("delta").and_then(|d| d.as_str()) {
                        if stream_tx
                            .send(StreamEvent::Token(delta.to_string()))
                            .await
                            .is_err()
                        {
                            child.kill().await.ok();
                            return Ok(());
                        }
                    }
                }
                Some("agent_message") | Some("message") => {
                    if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                        let _ = stream_tx.send(StreamEvent::Token(text.to_string())).await;
                    }
                }
                Some("task_complete") | Some("turn_complete") | Some("result") => {
                    let usage = extract_usage(&obj);
                    let _ = stream_tx.send(StreamEvent::Done(usage)).await;
                    return Ok(());
                }
                _ => {}
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
        vec![
            "gpt-5".to_string(),
            "gpt-5-codex".to_string(),
            "o3".to_string(),
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
            .get("cached_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        cache_write_tokens: 0,
        cost_usd: usage
            .get("total_cost_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    }
}
