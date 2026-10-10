//! Tamper-evident local decision records.
//!
//! A decision is tagged with an HMAC keyed by this installation's runtime
//! credential. The tag proves only that the writer could read that
//! owner-private credential; it is local evidence, not a signature a third
//! party can check.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::Value;

use crate::auth::{constant_time_eq, hmac_sha256};

pub const DECISION_INTEGRITY_VERSION: &str = "1";
const DECISION_DOMAIN: &[u8] = b"heiwa-decision-v1\n";

/// Compact JSON with object keys sorted at every depth, so a tag never
/// depends on map ordering, serde features, or whitespace.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                write_canonical(&map[key.as_str()], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

/// The tag over `record`'s canonical form, or `None` for an empty secret.
pub fn decision_tag(secret: &str, record: &Value) -> Option<String> {
    if secret.is_empty() {
        return None;
    }
    let mut message = DECISION_DOMAIN.to_vec();
    message.extend_from_slice(canonical_json(record).as_bytes());
    Some(URL_SAFE_NO_PAD.encode(hmac_sha256(secret.as_bytes(), &message)))
}

/// Constant-time comparison of `tag` with the expected tag.
pub fn verify_decision_tag(secret: &str, record: &Value, tag: &str) -> bool {
    decision_tag(secret, record)
        .is_some_and(|expected| constant_time_eq(expected.as_bytes(), tag.as_bytes()))
}

/// Conservative classification of a stored decision. Historical untagged
/// records are evidence to inspect, never proof of approval authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionIntegrity {
    Verified,
    Invalid(&'static str),
    Unverifiable,
}

impl DecisionIntegrity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Invalid(reason) => reason,
            Self::Unverifiable => "unverifiable",
        }
    }
}

/// File stems and content IDs share one constrained request key.
pub fn valid_decision_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && !matches!(id, "." | "..")
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// Read a regular decision file through one bounded, no-follow path. The
/// reader never repairs or rewrites an unverified record.
pub fn read_decision_record(path: &std::path::Path) -> anyhow::Result<Value> {
    use std::io::Read;
    const LIMIT: u64 = 16 * 1024 * 1024;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(not(unix))]
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(anyhow::anyhow!("nonregular decision record"));
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > LIMIT {
        return Err(anyhow::anyhow!("invalid decision record file"));
    }
    let mut raw = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut raw)?;
    if raw.len() as u64 > LIMIT {
        return Err(anyhow::anyhow!("decision record exceeds limit"));
    }
    Ok(serde_json::from_slice(&raw)?)
}

/// Verify both the immutable content and the file name resolving it.
pub fn classify_decision(secret: Option<&str>, stem: &str, record: &Value) -> DecisionIntegrity {
    if !valid_decision_id(stem) || record.get("id").and_then(Value::as_str) != Some(stem) {
        return DecisionIntegrity::Invalid("id_mismatch");
    }
    if !matches!(
        record.get("outcome").and_then(Value::as_str),
        Some("approved" | "denied")
    ) {
        return DecisionIntegrity::Invalid("invalid_outcome");
    }
    let Some(stamp) = record.get("integrity") else {
        return DecisionIntegrity::Invalid("untagged");
    };
    let Some(secret) = secret.filter(|secret| !secret.is_empty()) else {
        return DecisionIntegrity::Unverifiable;
    };
    if stamp.get("version").and_then(Value::as_str) != Some(DECISION_INTEGRITY_VERSION)
        || stamp.get("alg").and_then(Value::as_str) != Some("hmac-sha256")
    {
        return DecisionIntegrity::Invalid("invalid_stamp");
    }
    let mut body = record.clone();
    body.as_object_mut()
        .expect("an id-bearing record is an object")
        .remove("integrity");
    if verify_decision_tag(
        secret,
        &body,
        stamp.get("tag").and_then(Value::as_str).unwrap_or(""),
    ) {
        DecisionIntegrity::Verified
    } else {
        DecisionIntegrity::Invalid("tag_mismatch")
    }
}

/// Seal a new record. Persistence and principal authorization belong to the
/// decision service; this shared primitive keeps readers on one contract.
pub fn seal_decision(secret: &str, record: &mut Value) -> anyhow::Result<()> {
    let object = record
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("decision must be an object"))?;
    let id = object.get("id").and_then(Value::as_str).unwrap_or("");
    if !valid_decision_id(id)
        || !matches!(
            object.get("outcome").and_then(Value::as_str),
            Some("approved" | "denied")
        )
    {
        return Err(anyhow::anyhow!("invalid decision id or outcome"));
    }
    object.remove("integrity");
    let tag = decision_tag(secret, &Value::Object(object.clone()))
        .ok_or_else(|| anyhow::anyhow!("runtime credential is empty"))?;
    object.insert(
        "integrity".to_string(),
        serde_json::json!({
            "version": DECISION_INTEGRITY_VERSION, "alg": "hmac-sha256", "tag": tag,
        }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(unix)]
    #[test]
    fn decision_reader_rejects_linked_nonregular_and_oversized_files() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.json");
        std::fs::write(&source, r#"{"id":"req_a","outcome":"approved"}"#).unwrap();
        assert!(read_decision_record(&source).is_ok());
        let link = directory.path().join("linked.json");
        std::os::unix::fs::symlink(&source, &link).unwrap();
        assert!(read_decision_record(&link).is_err());
        assert!(read_decision_record(directory.path()).is_err());
        let fifo = directory.path().join("fifo");
        let fifo_name = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
        assert!(read_decision_record(&fifo).is_err());
        let file = std::fs::File::create(&source).unwrap();
        file.set_len(16 * 1024 * 1024 + 1).unwrap();
        assert!(read_decision_record(&source).is_err());
    }

    #[test]
    fn recorded_decisions_bind_name_outcome_key_and_all_nested_content() {
        let mut record = json!({"id":"req_a", "outcome":"approved", "principal":{"client_label":"Heiwa.app"}, "effects":[{"kind":"hold_confirm","target":"hold-a"}]});
        seal_decision("key-a", &mut record).unwrap();
        assert_eq!(
            classify_decision(Some("key-a"), "req_a", &record),
            DecisionIntegrity::Verified
        );
        assert_eq!(
            classify_decision(Some("key-a"), "req_b", &record),
            DecisionIntegrity::Invalid("id_mismatch")
        );
        assert_eq!(
            classify_decision(Some("key-b"), "req_a", &record),
            DecisionIntegrity::Invalid("tag_mismatch")
        );
        assert_eq!(
            classify_decision(None, "req_a", &record),
            DecisionIntegrity::Unverifiable
        );
        let mut changed = record.clone();
        changed["effects"][0]["target"] = json!("hold-b");
        assert_eq!(
            classify_decision(Some("key-a"), "req_a", &changed),
            DecisionIntegrity::Invalid("tag_mismatch")
        );
        changed = record.clone();
        changed["outcome"] = json!("other");
        assert_eq!(
            classify_decision(Some("key-a"), "req_a", &changed),
            DecisionIntegrity::Invalid("invalid_outcome")
        );
        changed = record.clone();
        changed["integrity"]["alg"] = json!("sha256");
        assert_eq!(
            classify_decision(Some("key-a"), "req_a", &changed),
            DecisionIntegrity::Invalid("invalid_stamp")
        );
        changed = record;
        changed.as_object_mut().unwrap().remove("integrity");
        assert_eq!(
            classify_decision(Some("key-a"), "req_a", &changed),
            DecisionIntegrity::Invalid("untagged")
        );
        assert!(!valid_decision_id("../req_a"));
    }

    #[test]
    fn canonical_form_sorts_keys_at_every_depth() {
        let value = json!({"b": 1, "a": {"d": [{"z": 1, "y": 2}], "c": null}});
        assert_eq!(
            canonical_json(&value),
            r#"{"a":{"c":null,"d":[{"y":2,"z":1}]},"b":1}"#
        );
    }

    #[test]
    fn a_tag_verifies_only_for_the_same_record_and_secret() {
        let record = json!({"id": "req_1", "outcome": "approved"});
        let tag = decision_tag("secret-a", &record).expect("tag");
        assert!(verify_decision_tag("secret-a", &record, &tag));
        assert!(!verify_decision_tag("secret-b", &record, &tag));
        let changed = json!({"id": "req_1", "outcome": "denied"});
        assert!(!verify_decision_tag("secret-a", &changed, &tag));
        assert!(!verify_decision_tag("secret-a", &record, "not-a-tag"));
        assert!(decision_tag("", &record).is_none());
        assert!(!verify_decision_tag("", &record, &tag));
    }
}
