//! Shared streaming loop for direct-API adapters.
//!
//! Every adapter had the same three obligations and got two of them wrong:
//!
//! 1. `ProviderAdapter::send` must emit exactly one `Done` or `Error` before
//!    returning. The `?` on a mid-stream network error skipped that, so a
//!    connection reset ended the turn with no terminal event and nothing
//!    surfaced to the user.
//! 2. A stalled connection — a half-open socket after a NAT timeout or a
//!    sleeping laptop — must not hang the turn forever. `connect_timeout`
//!    does not bound the gap between chunks.
//! 3. A consumer that went away must stop the stream.
//!
//! Doing this once means an adapter only owns its wire vocabulary.

use crate::adapter::StreamEvent;
use crate::providers::sse::{SseDecoder, SseEvent};
use anyhow::Result;
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

/// Maximum quiet time between chunks before the stream is abandoned.
///
/// Generous: a model may think for a long stretch before its first token, and
/// providers do not all send keep-alives. This is a liveness backstop, not a
/// latency budget.
pub(crate) const IDLE_TIMEOUT: Duration = Duration::from_secs(180);

/// A provider's wire vocabulary: SSE events in, `StreamEvent`s out.
pub(crate) trait SseConsumer {
    fn consume(&mut self, event: &SseEvent) -> Vec<StreamEvent>;
    /// Terminal event when the stream ends without the provider sending one.
    /// Must be idempotent — the pump may or may not reach it.
    fn finish(&mut self) -> Vec<StreamEvent>;
}

/// Drive an SSE response to exactly one terminal event.
pub(crate) async fn pump<C: SseConsumer>(
    response: reqwest::Response,
    mut consumer: C,
    stream_tx: mpsc::Sender<StreamEvent>,
    provider: &str,
) -> Result<()> {
    let mut decoder = SseDecoder::new();
    let mut body = response.bytes_stream();

    loop {
        let next = tokio::time::timeout(IDLE_TIMEOUT, body.next()).await;

        let chunk = match next {
            Err(_) => {
                return fail(
                    &stream_tx,
                    format!(
                        "{provider} stream stalled: no data for {}s",
                        IDLE_TIMEOUT.as_secs()
                    ),
                )
                .await
            }
            Ok(None) => break, // body ended
            Ok(Some(Err(error))) => {
                return fail(&stream_tx, format!("{provider} stream failed: {error}")).await
            }
            Ok(Some(Ok(chunk))) => chunk,
        };

        for event in decoder.push(&String::from_utf8_lossy(&chunk)) {
            for stream_event in consumer.consume(&event) {
                let terminal = matches!(stream_event, StreamEvent::Done(_) | StreamEvent::Error(_));
                if stream_tx.send(stream_event).await.is_err() {
                    return Ok(()); // consumer went away
                }
                if terminal {
                    return Ok(());
                }
            }
        }
    }

    // Ended without a provider-sent terminal event: report what was counted
    // rather than leaving the consumer waiting.
    for stream_event in consumer.finish() {
        if stream_tx.send(stream_event).await.is_err() {
            return Ok(());
        }
    }
    Ok(())
}

/// Emit the error to the consumer *and* return it, so a caller that logs the
/// `Result` and a consumer reading the stream both learn what happened.
async fn fail(stream_tx: &mpsc::Sender<StreamEvent>, message: String) -> Result<()> {
    let _ = stream_tx.send(StreamEvent::Error(message.clone())).await;
    Err(anyhow::anyhow!(message))
}
