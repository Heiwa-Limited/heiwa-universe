//! Reading `heiwa.cli/v1` output in integration tests.
#![allow(dead_code)]

use serde_json::Value;

fn envelope(stdout: &[u8]) -> Value {
    let envelope: Value = serde_json::from_slice(stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not one JSON envelope: {error}: {}",
            String::from_utf8_lossy(stdout)
        )
    });
    assert_eq!(envelope["schema"], "heiwa.cli/v1", "{envelope}");
    envelope
}

/// The `data` of a successful envelope.
pub fn data(stdout: &[u8]) -> Value {
    let envelope = envelope(stdout);
    assert_eq!(envelope["ok"], true, "{envelope}");
    envelope["data"].clone()
}

/// The `error` of a failed envelope.
pub fn error(stdout: &[u8]) -> Value {
    let envelope = envelope(stdout);
    assert_eq!(envelope["ok"], false, "{envelope}");
    envelope["error"].clone()
}

/// The `next` hints of a successful envelope.
pub fn next(stdout: &[u8]) -> Vec<String> {
    let envelope = envelope(stdout);
    envelope["next"]
        .as_array()
        .expect("next is an array")
        .iter()
        .map(|hint| hint.as_str().expect("hint is a string").to_string())
        .collect()
}
