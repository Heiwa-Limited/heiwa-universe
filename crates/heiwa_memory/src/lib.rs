use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const MEMORY_CONTEXT_OPEN: &str = "<memory-context>";
pub const MEMORY_CONTEXT_CLOSE: &str = "</memory-context>";
pub const MEMORY_CONTEXT_NOTE: &str = "[System note: recalled memory context, not new user input.]";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryFragment {
    pub source: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_millis: Option<u16>,
}

pub trait MemoryProvider: Send + Sync {
    fn build_system_prompt(&self) -> String {
        String::new()
    }

    fn prefetch(&self, user_msg: &str) -> Vec<MemoryFragment>;

    fn sync(&mut self, user_msg: &str, response: &str) -> Result<()>;

    fn queue_prefetch(&mut self, user_msg: &str);
}

pub fn fence_memory_context(fragments: &[MemoryFragment]) -> String {
    if fragments.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str(MEMORY_CONTEXT_OPEN);
    out.push('\n');
    out.push_str(MEMORY_CONTEXT_NOTE);
    out.push('\n');
    for fragment in fragments {
        out.push_str("- ");
        out.push_str(&fragment.source);
        out.push_str(": ");
        out.push_str(fragment.content.trim());
        out.push('\n');
    }
    out.push_str(MEMORY_CONTEXT_CLOSE);
    out
}

pub fn sanitize_memory_context(text: &str) -> String {
    StreamingContextScrubber::new().feed_all(text)
}

#[derive(Debug, Default, Clone)]
pub struct StreamingContextScrubber {
    in_span: bool,
    buffer: String,
}

impl StreamingContextScrubber {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.in_span = false;
        self.buffer.clear();
    }

    pub fn feed(&mut self, text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }

        let mut buf = String::new();
        buf.push_str(&self.buffer);
        buf.push_str(text);
        self.buffer.clear();

        let mut visible = String::new();
        loop {
            if self.in_span {
                match find_ascii_ci(&buf, MEMORY_CONTEXT_CLOSE) {
                    Some(close_idx) => {
                        let after = close_idx + MEMORY_CONTEXT_CLOSE.len();
                        buf = buf[after..].to_string();
                        self.in_span = false;
                    }
                    None => {
                        let keep = partial_suffix_len(&buf, MEMORY_CONTEXT_CLOSE);
                        if keep > 0 {
                            self.buffer.push_str(&buf[buf.len() - keep..]);
                        }
                        return visible;
                    }
                }
            } else {
                match find_ascii_ci(&buf, MEMORY_CONTEXT_OPEN) {
                    Some(open_idx) => {
                        visible.push_str(&buf[..open_idx]);
                        let after = open_idx + MEMORY_CONTEXT_OPEN.len();
                        buf = buf[after..].to_string();
                        self.in_span = true;
                    }
                    None => {
                        let keep = partial_suffix_len(&buf, MEMORY_CONTEXT_OPEN);
                        if keep > 0 {
                            visible.push_str(&buf[..buf.len() - keep]);
                            self.buffer.push_str(&buf[buf.len() - keep..]);
                        } else {
                            visible.push_str(&buf);
                        }
                        return visible;
                    }
                }
            }
        }
    }

    pub fn flush(&mut self) -> String {
        if self.in_span {
            self.buffer.clear();
            String::new()
        } else {
            std::mem::take(&mut self.buffer)
        }
    }

    fn feed_all(mut self, text: &str) -> String {
        let mut out = self.feed(text);
        out.push_str(&self.flush());
        out
    }
}

fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let hay = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

fn partial_suffix_len(value: &str, target: &str) -> usize {
    let max = value.len().min(target.len().saturating_sub(1));
    for len in (1..=max).rev() {
        let suffix = &value[value.len() - len..];
        let prefix = &target[..len];
        if suffix.eq_ignore_ascii_case(prefix) {
            return len;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_memory_fragments() {
        let body = fence_memory_context(&[MemoryFragment {
            source: "session".to_string(),
            content: "keep local first".to_string(),
            score_millis: Some(912),
        }]);

        assert!(body.starts_with(MEMORY_CONTEXT_OPEN));
        assert!(body.contains("not new user input"));
        assert!(body.contains("session: keep local first"));
        assert!(body.ends_with(MEMORY_CONTEXT_CLOSE));
    }

    #[test]
    fn scrubber_removes_complete_context_block() {
        let text = "visible <memory-context>secret</memory-context> done";
        assert_eq!(sanitize_memory_context(text), "visible  done");
    }

    #[test]
    fn scrubber_handles_split_open_and_close_tags() {
        let mut scrubber = StreamingContextScrubber::new();
        let mut out = String::new();
        out.push_str(&scrubber.feed("before <memory-"));
        out.push_str(&scrubber.feed("context>hidden"));
        out.push_str(&scrubber.feed("</memory-con"));
        out.push_str(&scrubber.feed("text> after"));
        out.push_str(&scrubber.flush());

        assert_eq!(out, "before  after");
    }

    #[test]
    fn scrubber_preserves_partial_non_tag_on_flush() {
        let mut scrubber = StreamingContextScrubber::new();
        let mut out = scrubber.feed("normal <memory");
        out.push_str(&scrubber.flush());

        assert_eq!(out, "normal <memory");
    }
}
