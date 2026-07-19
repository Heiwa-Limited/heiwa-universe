//! Shared sensitive-material gate.
//!
//! Moved verbatim (basename/prefix policy byte-for-byte) from
//! `apps/heiwa_shell/src/cmd/capabilities.rs` so every writer into the
//! evidence plane — capability refresh, the operator journal, and future
//! callers — shares one definition of "looks like a credential or live
//! token" rather than each maintaining its own drifting copy.
//!
//! Object keys are intentionally not inspected, only values: policy
//! vocabulary such as `oauth_tokens` or `credential_files` must not be a
//! false positive.

use serde_json::Value;

/// Filesystem basenames that must never appear in surfaced metadata.
const SENSITIVE_BASENAMES: &[&str] = &[
    "auth.json",
    "accounts.json",
    "credentials",
    "credential.json",
    "id_rsa",
    "id_ed25519",
    "secrets.json",
    "token.json",
    ".pem",
    ".env",
];

/// Value prefixes that look like live secrets / bearer tokens.
const SENSITIVE_VALUE_PREFIXES: &[&str] = &[
    "sk-",
    "ghp_",
    "gho_",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "Bearer ",
    "AKIA",
    "AIza",
    "ya29.",
];

/// Which policy list matched. Callers that only need a boolean gate can
/// ignore this; callers building diagnostics can branch on it without
/// re-deriving the match themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensitiveMatch {
    pub category: &'static str,
}

fn sensitive_basename(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    SENSITIVE_BASENAMES.iter().any(|name| lower.contains(name))
}

fn sensitive_prefix(text: &str) -> bool {
    text.lines().any(|line| {
        SENSITIVE_VALUE_PREFIXES
            .iter()
            .any(|prefix| line.starts_with(prefix))
    })
}

/// Walk a JSON value and return the first match against the credential-path
/// or token-prefix policy. Arrays and object *values* are recursed; object
/// keys are not inspected.
pub fn find_sensitive(value: &Value) -> Option<SensitiveMatch> {
    match value {
        Value::String(text) if sensitive_basename(text) => Some(SensitiveMatch {
            category: "credential_path",
        }),
        Value::String(text) if sensitive_prefix(text) => Some(SensitiveMatch {
            category: "token_prefix",
        }),
        Value::Array(values) => values.iter().find_map(find_sensitive),
        Value::Object(values) => values.values().find_map(find_sensitive),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flags_auth_path_as_credential_path() {
        let v = json!({ "observed": "/Users/x/.codex/auth.json" });
        assert_eq!(
            find_sensitive(&v),
            Some(SensitiveMatch {
                category: "credential_path"
            })
        );
    }

    #[test]
    fn flags_token_prefix() {
        let v = json!({ "leak": "ghp_ABCDEF0123456789abcdef" });
        assert_eq!(
            find_sensitive(&v),
            Some(SensitiveMatch {
                category: "token_prefix"
            })
        );
    }

    #[test]
    fn flags_token_prefix_after_a_newline_in_one_raw_value() {
        let v = json!({ "leak": "safe transcript\nghp_ABCDEF0123456789abcdef" });
        assert_eq!(
            find_sensitive(&v),
            Some(SensitiveMatch {
                category: "token_prefix"
            })
        );
    }

    #[test]
    fn ignores_policy_vocabulary() {
        let v = json!({
            "source_policy": {
                "excluded": ["oauth_tokens", "api_keys", "token_bearing_mcp_headers", "credential_files"]
            },
            "reference_sources": ["official.openai.agents-sdk", "official.ollama.api"],
            "secret_policy": "redacted_config_metadata_only"
        });
        assert_eq!(find_sensitive(&v), None);
    }

    #[test]
    fn does_not_inspect_object_keys() {
        let v = json!({ "ghp_looks_like_a_key_not_a_value": "safe" });
        assert_eq!(find_sensitive(&v), None);
    }

    #[test]
    fn recurses_into_arrays_and_nested_objects() {
        let v = json!({ "nested": { "list": [ "safe", "Bearer live-token" ] } });
        assert_eq!(
            find_sensitive(&v),
            Some(SensitiveMatch {
                category: "token_prefix"
            })
        );
    }
}
