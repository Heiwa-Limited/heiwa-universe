use crate::adapter::{Message, ProviderAdapter, StreamEvent, TokenUsage};
use anyhow::Result;
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// CLI subprocess adapter for Ollama.
///
/// This wraps `ollama run <model> <prompt>` as a subprocess.  For BYOK API
/// key users, the HTTP API adapter (detect/ollama.rs handles detection;
/// a future OllamaHttpAdapter will handle execution) is preferred.
pub struct OllamaCliAdapter {
    default_model: String,
}

impl OllamaCliAdapter {
    pub fn new() -> Self {
        Self {
            default_model: "llama3".to_string(),
        }
    }

    pub fn with_model(model: &str) -> Self {
        Self {
            default_model: model.to_string(),
        }
    }
}

#[async_trait]
impl ProviderAdapter for OllamaCliAdapter {
    async fn send(
        &self,
        model: &str,
        messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        let model = if model.is_empty() {
            &self.default_model
        } else {
            model
        };

        // Flatten messages into a single prompt for the CLI
        let prompt: String = messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let mut child = Command::new("ollama")
            .arg("run")
            .arg(model)
            .arg(&prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout).lines();

        while let Some(line) = reader.next_line().await? {
            if stream_tx.send(StreamEvent::Token(line)).await.is_err() {
                // Consumer dropped — interrupt
                child.kill().await.ok();
                return Ok(());
            }
        }

        let _ = stream_tx
            .send(StreamEvent::Done(TokenUsage::default()))
            .await;
        Ok(())
    }

    async fn interrupt(&self) -> Result<()> {
        // Subprocess-based — interrupt is handled by dropping the child
        Ok(())
    }

    fn supported_models(&self) -> Vec<String> {
        vec![self.default_model.clone()]
    }
}
