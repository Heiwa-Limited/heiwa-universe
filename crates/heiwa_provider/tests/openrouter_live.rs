//! Live smoke test for the OpenRouter HTTP adapter.
//!
//! Ignored by default: requires a registered OpenRouter account
//! (`heiwa auth add-key openrouter <key>`), a real OS keychain session,
//! and network access. Run explicitly with:
//!
//! ```sh
//! cargo test -p heiwa-provider --test openrouter_live -- --ignored --nocapture
//! ```

use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent};
use heiwa_provider::providers::openrouter::OpenRouterAdapter;

#[tokio::test]
#[ignore = "requires keychain, registered OpenRouter account, and network"]
async fn streams_a_completion_from_free_tier() {
    let adapter = OpenRouterAdapter::from_registry()
        .expect("an OpenRouter account must be registered for this test");
    let models = adapter.supported_models();
    assert!(!models.is_empty(), "account has at least one model");

    // Free-tier models are individually flaky (upstream 429s); the test
    // passes if any registered model completes a streamed turn.
    let mut last_error = String::new();
    for model in &models {
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let messages = vec![Message {
            role: Role::User,
            content: "Reply with exactly: ok".to_string(),
        }];

        let a = OpenRouterAdapter::from_registry().unwrap();
        let m = model.clone();
        let send = tokio::spawn(async move { a.send(&m, &messages, tx).await });

        let mut text = String::new();
        let mut done = false;
        let mut errored = None;
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(t) => text.push_str(&t),
                StreamEvent::Done(usage) => {
                    println!(
                        "usage: in={} out={}",
                        usage.input_tokens, usage.output_tokens
                    );
                    done = true;
                }
                StreamEvent::Error(e) => errored = Some(e),
                StreamEvent::ToolUse { .. } => {}
            }
        }
        let _ = send.await.expect("task join");

        if let Some(e) = errored {
            println!("{model}: error, trying next — {e}");
            last_error = e;
            continue;
        }
        println!("{model}: response {text:?}");
        assert!(done, "stream must end with Done");
        assert!(!text.trim().is_empty(), "model returned no text");
        return;
    }
    panic!("all registered models failed; last error: {last_error}");
}
