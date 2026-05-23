use crate::adapter::{Message, ProviderAdapter, StreamEvent, TokenUsage};
use anyhow::Result;
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;

/// CLI subprocess adapter for Ollama.
///
/// Wraps `ollama run <model> <prompt>`, streaming stdout as text tokens
/// while filtering model "thinking" traces that Qwen3/Gemma/etc. emit
/// inline. Preserves newlines so downstream markdown renders correctly.
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

        let mut stdout = child.stdout.take().unwrap();
        let mut buf = [0u8; 4096];
        let mut carry = String::new();
        let mut filter = ThinkFilter::new();

        loop {
            let n = stdout.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            carry.push_str(&String::from_utf8_lossy(&buf[..n]));

            // Emit complete segments (split on newline, keep the newline).
            while let Some(nl) = carry.find('\n') {
                let mut line: String = carry.drain(..=nl).collect();
                // Normalise carriage returns that Ollama uses for spinner wipe.
                line = line.replace('\r', "");
                if let Some(clean) = filter.feed(&line) {
                    if !clean.is_empty() && stream_tx.send(StreamEvent::Token(clean)).await.is_err()
                    {
                        child.kill().await.ok();
                        return Ok(());
                    }
                }
            }
        }

        // Flush any trailing unterminated segment.
        if !carry.is_empty() {
            if let Some(clean) = filter.feed(&carry.replace('\r', "")) {
                if !clean.is_empty() {
                    let _ = stream_tx.send(StreamEvent::Token(clean)).await;
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
        vec![self.default_model.clone()]
    }
}

/// Strips reasoning traces that Ollama models emit inline.
/// Handles: `<think>...</think>` (Qwen3) and `Thinking...done thinking.`
/// / `Thinking Process:...done thinking.` (Gemma).
struct ThinkFilter {
    in_block: bool,
}

impl ThinkFilter {
    fn new() -> Self {
        Self { in_block: false }
    }

    fn feed(&mut self, input: &str) -> Option<String> {
        let mut out = String::with_capacity(input.len());
        let mut remaining = input;

        loop {
            if self.in_block {
                // Look for either `</think>` or the textual `done thinking.` sentinel.
                let tag_end = remaining.find("</think>").map(|i| (i, "</think>".len()));
                let text_end = remaining
                    .find("done thinking.")
                    .map(|i| (i, "done thinking.".len()));
                let end = match (tag_end, text_end) {
                    (Some(a), Some(b)) => Some(if a.0 < b.0 { a } else { b }),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };
                match end {
                    Some((i, len)) => {
                        self.in_block = false;
                        remaining = &remaining[i + len..];
                        // Strip a leading newline that often follows the tag so
                        // the visible response doesn't start with a blank line.
                        if let Some(stripped) = remaining.strip_prefix('\n') {
                            remaining = stripped;
                        }
                    }
                    None => {
                        return if out.is_empty() { None } else { Some(out) };
                    }
                }
            }

            let tag_start = remaining.find("<think>").map(|i| (i, "<think>".len()));
            let text_start = remaining
                .find("Thinking Process:")
                .map(|i| (i, "Thinking Process:".len()))
                .or_else(|| {
                    remaining
                        .find("Thinking...")
                        .map(|i| (i, "Thinking...".len()))
                });
            let start = match (tag_start, text_start) {
                (Some(a), Some(b)) => Some(if a.0 < b.0 { a } else { b }),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };
            match start {
                Some((i, len)) => {
                    out.push_str(&remaining[..i]);
                    remaining = &remaining[i + len..];
                    self.in_block = true;
                }
                None => {
                    out.push_str(remaining);
                    break;
                }
            }
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ThinkFilter;

    #[test]
    fn strips_angle_bracket_think_block() {
        let mut f = ThinkFilter::new();
        let out = f.feed("hello <think>internal chain</think>world").unwrap();
        assert_eq!(out, "hello world");
    }

    #[test]
    fn strips_textual_gemma_block() {
        let mut f = ThinkFilter::new();
        let out = f
            .feed("prefix Thinking Process:step 1 step 2 done thinking.answer")
            .unwrap();
        assert_eq!(out, "prefix answer");
    }

    #[test]
    fn handles_block_split_across_feeds() {
        let mut f = ThinkFilter::new();
        assert_eq!(
            f.feed("leading <think>hidden"),
            Some("leading ".to_string())
        );
        assert_eq!(f.feed("more hidden"), None);
        assert_eq!(f.feed("</think>visible"), Some("visible".to_string()));
    }

    #[test]
    fn passes_through_clean_text() {
        let mut f = ThinkFilter::new();
        assert_eq!(f.feed("clean output\n"), Some("clean output\n".to_string()));
    }
}
