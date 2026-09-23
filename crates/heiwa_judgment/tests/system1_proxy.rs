//! A backend classified local must be reached directly. A proxy taken from
//! the environment is another hop the locality decision never saw, so it
//! must not see the state either.
//!
//! This is its own test binary on purpose: it sets process-wide proxy
//! variables before any client exists, which would race with every other
//! test that builds a client in the same process.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use heiwa_judgment::backend::{Backend, System1Client};
use heiwa_judgment::question::{Question, QuestionSet};
use serde_json::json;

/// A loopback origin that counts requests and answers with a valid
/// structured-output completion, so a proxied call would *succeed* silently.
fn origin() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let payload = json!({
        "choices": [{
            "finish_reason": "stop",
            "message": { "role": "assistant", "content": json!({ "flag": { "noul": 0.9 } }).to_string() }
        }],
        "usage": { "prompt_tokens": 10, "completion_tokens": 2 }
    })
    .to_string();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            counted.fetch_add(1, Ordering::SeqCst);
            let payload = payload.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });
    (base_url, hits)
}

#[tokio::test]
async fn a_local_backend_is_never_reached_through_an_environment_proxy() {
    let (proxy_url, proxy_hits) = origin();
    let (local_url, local_hits) = origin();
    for variable in ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
        std::env::set_var(variable, &proxy_url);
    }
    for variable in ["NO_PROXY", "no_proxy"] {
        std::env::remove_var(variable);
    }

    let questions = QuestionSet::new(vec![(
        "flag".into(),
        Question::noul("Is it flagged?").unwrap(),
    )])
    .unwrap();
    let client =
        System1Client::new(Backend::structured_llm(&local_url, "gemma4:latest")).expect("client");
    let evaluation = client
        .evaluate(
            "SYNTHETIC_LOCAL_ONLY_SENTINEL",
            &questions,
            Duration::from_secs(5),
        )
        .await;

    assert_eq!(
        proxy_hits.load(Ordering::SeqCst),
        0,
        "an environment proxy received state meant for a loopback backend"
    );
    assert_eq!(
        local_hits.load(Ordering::SeqCst),
        1,
        "the local backend was asked directly"
    );
    assert!(evaluation.outcome.is_ok(), "{:?}", evaluation.outcome);
}
