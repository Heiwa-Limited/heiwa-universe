//! Shared Server-Sent Events decoding for direct-API adapters.
//!
//! Every provider streams over SSE, and every one of them splits payloads
//! across chunk boundaries. Decoding is factored out here so each adapter
//! only owns its wire vocabulary, not the framing.

/// One dispatched SSE event: the optional `event:` name plus the joined
/// `data:` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SseEvent {
    pub name: Option<String>,
    pub data: String,
}

impl SseEvent {
    /// Test constructor: production events come out of the decoder.
    #[cfg(test)]
    pub fn new(name: Option<&str>, data: &str) -> Self {
        Self {
            name: name.map(str::to_string),
            data: data.to_string(),
        }
    }

    /// The OpenAI-family terminator. Anthropic and Gemini never send it.
    pub fn is_done(&self) -> bool {
        self.data.trim() == "[DONE]"
    }
}

/// Incremental SSE framer. Feed it byte chunks as they arrive; it returns
/// the events those chunks completed and retains any partial remainder.
#[derive(Default)]
pub(crate) struct SseDecoder {
    carry: String,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.carry.push_str(chunk);
        let mut events = Vec::new();

        // Events are separated by a blank line. Scan the buffer once and
        // remove the consumed prefix once: a fast provider can deliver
        // thousands of events in a single read, and rescanning the whole
        // buffer per event turns one read into quadratic work.
        let mut consumed = 0;
        while let Some((event_end, next_start)) = find_separator(&self.carry[consumed..]) {
            if let Some(event) = parse_event(&self.carry[consumed..consumed + event_end]) {
                events.push(event);
            }
            consumed += next_start;
        }
        self.carry.drain(..consumed);
        events
    }
}

/// Byte length of the line terminator at `index`, if one starts there.
///
/// A lone `\r` at the very end of the buffer is ambiguous — the `\n` that
/// would make it CRLF may be in the next chunk — but that resolves itself:
/// a separator needs a *second* terminator after it, which cannot be read
/// past the end, so the scan waits for more data instead of mis-framing.
fn terminator_len(bytes: &[u8], index: usize) -> Option<usize> {
    match bytes.get(index)? {
        b'\r' if bytes.get(index + 1) == Some(&b'\n') => Some(2),
        b'\r' | b'\n' => Some(1),
        _ => None,
    }
}

/// Locate the first blank line: `(end of the event, start of the next one)`.
///
/// Byte indexing is safe because UTF-8 never encodes a multi-byte character
/// with an ASCII byte, so a `\r` or `\n` match is always a char boundary.
fn find_separator(buffer: &str) -> Option<(usize, usize)> {
    let bytes = buffer.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let Some(first) = terminator_len(bytes, index) else {
            index += 1;
            continue;
        };
        if let Some(second) = terminator_len(bytes, index + first) {
            return Some((index, index + first + second));
        }
        index += first;
    }
    None
}

fn parse_event(raw: &str) -> Option<SseEvent> {
    let mut name = None;
    let mut data_lines: Vec<&str> = Vec::new();

    for line in raw.lines() {
        if line.is_empty() || line.starts_with(':') {
            continue; // blank separator or comment/keep-alive
        }
        if let Some(value) = line.strip_prefix("event:") {
            name = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }

    // An event with no data line carries nothing a consumer can act on.
    if data_lines.is_empty() {
        return None;
    }
    Some(SseEvent {
        name,
        data: data_lines.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yields_complete_payloads_from_one_chunk() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push("data: {\"a\":1}\n\ndata: {\"b\":2}\n\n");
        assert_eq!(
            events,
            vec![SseEvent::new(None, "{\"a\":1}")]
                .into_iter()
                .chain(std::iter::once(SseEvent::new(None, "{\"b\":2}")))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn reassembles_a_payload_split_across_chunks() {
        let mut decoder = SseDecoder::new();
        assert!(decoder.push("data: {\"hel").is_empty());
        assert!(decoder.push("lo\":\"wor").is_empty());
        let events = decoder.push("ld\"}\n\n");
        assert_eq!(events, vec![SseEvent::new(None, "{\"hello\":\"world\"}")]);
    }

    #[test]
    fn captures_the_event_name_when_present() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push("event: content_block_delta\ndata: {\"x\":1}\n\n");
        assert_eq!(
            events,
            vec![SseEvent::new(Some("content_block_delta"), "{\"x\":1}")]
        );
    }

    #[test]
    fn ignores_comments_and_blank_padding() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push(": keep-alive\n\ndata: {\"ok\":true}\n\n");
        assert_eq!(events, vec![SseEvent::new(None, "{\"ok\":true}")]);
    }

    #[test]
    fn decodes_a_large_burst_in_linear_time() {
        // A fast provider can land thousands of events in one read. Rescanning
        // and reallocating the whole buffer per event made this quadratic:
        // 16k events took 6.5s, which stalls the turn it is supposed to
        // stream. The bound is deliberately loose — the point is the shape of
        // the curve, not a latency budget.
        const EVENTS: usize = 20_000;
        let mut chunk = String::new();
        for n in 0..EVENTS {
            chunk.push_str(&format!("data: {{\"n\":{n}}}\n\n"));
        }

        let started = std::time::Instant::now();
        let events = SseDecoder::new().push(&chunk);
        let elapsed = started.elapsed();

        assert_eq!(events.len(), EVENTS);
        assert_eq!(events[EVENTS - 1].data, format!("{{\"n\":{}}}", EVENTS - 1));
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "decoding {EVENTS} events took {elapsed:?}; that is quadratic behavior"
        );
    }

    #[test]
    fn holds_a_trailing_cr_until_the_next_chunk_decides_it() {
        // A CRLF pair split across chunks must not frame an event early.
        let mut decoder = SseDecoder::new();
        assert!(decoder.push("data: {\"n\":1}\r").is_empty());
        let events = decoder.push("\n\r\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "{\"n\":1}");
    }

    #[test]
    fn tolerates_crlf_line_endings() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push("event: ping\r\ndata: {\"n\":1}\r\n\r\n");
        assert_eq!(events, vec![SseEvent::new(Some("ping"), "{\"n\":1}")]);
    }

    #[test]
    fn joins_multiple_data_lines_in_one_event() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push("data: line one\ndata: line two\n\n");
        assert_eq!(events, vec![SseEvent::new(None, "line one\nline two")]);
    }

    #[test]
    fn treats_done_sentinel_as_an_ordinary_payload() {
        // Adapters decide what [DONE] means; the decoder stays dumb.
        let mut decoder = SseDecoder::new();
        let events = decoder.push("data: [DONE]\n\n");
        assert_eq!(events, vec![SseEvent::new(None, "[DONE]")]);
        assert!(events[0].is_done());
    }

    #[test]
    fn drops_an_event_carrying_no_data_line() {
        let mut decoder = SseDecoder::new();
        assert!(decoder.push("event: heartbeat\n\n").is_empty());
    }
}
