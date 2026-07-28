//! Shared sensitive-material gate.
//!
//! Every writer into the evidence plane shares this definition of "looks like
//! credential material". Matching is boundary-aware and recursive: raw values,
//! embedded authorization fields, and sensitive object-key/value pairs are
//! rejected, while explicit redaction markers remain persistable.

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

const SENSITIVE_NORMALIZED_KEYS: &[&str] = &[
    "authorization",
    "proxyauthorization",
    "apikey",
    "openaiapikey",
    "anthropicapikey",
    "googleapikey",
    "accesskey",
    "secretkey",
    "accesstoken",
    "refreshtoken",
    "idtoken",
    "oauthtoken",
    "oauthaccesstoken",
    "oauthrefreshtoken",
    "clientsecret",
    "privatekey",
    "password",
    "passwd",
    "token",
];

const REDACTION_MARKERS: &[&str] = &["[redacted]", "<redacted>", "***redacted***"];

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
        let line = line.trim_start();
        SENSITIVE_VALUE_PREFIXES
            .iter()
            .any(|prefix| starts_with_ascii_case(line, prefix))
    }) || SENSITIVE_VALUE_PREFIXES
        .iter()
        .any(|prefix| contains_boundary_prefix(text, prefix))
}

fn starts_with_ascii_case(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

fn contains_boundary_prefix(text: &str, prefix: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let prefix = prefix.to_ascii_lowercase();
    lower.match_indices(&prefix).any(|(index, _)| {
        index == 0
            || lower[..index]
                .chars()
                .next_back()
                .is_none_or(|before| !before.is_ascii_alphanumeric())
    })
}

fn normalized_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn sensitive_key(key: &str) -> bool {
    let normalized = normalized_key(key);
    SENSITIVE_NORMALIZED_KEYS.contains(&normalized.as_str())
}

fn is_redaction_marker(text: &str) -> bool {
    REDACTION_MARKERS.contains(&text.trim().to_ascii_lowercase().as_str())
}

fn safely_absent_or_redacted(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty() || is_redaction_marker(text),
        _ => false,
    }
}

fn contains_private_key(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    [
        concat!("-----BEGIN ", "PRIVATE KEY-----"),
        concat!("-----BEGIN RSA ", "PRIVATE KEY-----"),
        concat!("-----BEGIN EC ", "PRIVATE KEY-----"),
        concat!("-----BEGIN DSA ", "PRIVATE KEY-----"),
        concat!("-----BEGIN OPENSSH ", "PRIVATE KEY-----"),
    ]
    .iter()
    .any(|marker| upper.contains(marker))
}

fn contains_jwt(text: &str) -> bool {
    text.split(|character: char| {
        !(character.is_ascii_alphanumeric()
            || matches!(character, '-' | '_' | '.' | '=' | '+' | '/'))
    })
    .any(|candidate| {
        let segments = candidate.split('.').collect::<Vec<_>>();
        segments.len() == 3
            && segments[0].starts_with("eyJ")
            && segments[0].len() >= 8
            && segments[1].len() >= 8
            && segments[2].len() >= 4
            && segments.iter().all(|segment| {
                segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            })
    })
}

fn contains_labeled_secret(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "authorization",
        "proxy-authorization",
        "api_key",
        "api-key",
        "api key",
        "access_token",
        "access-token",
        "refresh_token",
        "refresh-token",
        "oauth_token",
        "oauth-token",
        "client_secret",
        "client-secret",
        "private_key",
        "private-key",
    ]
    .iter()
    .any(|label| {
        lower.match_indices(label).any(|(index, _)| {
            let before_ok = index == 0
                || lower[..index]
                    .chars()
                    .next_back()
                    .is_none_or(|character| !character.is_ascii_alphanumeric());
            if !before_ok {
                return false;
            }
            let tail = lower[index + label.len()..].trim_start();
            let Some(value) = tail
                .strip_prefix(':')
                .or_else(|| tail.strip_prefix('='))
                .map(str::trim_start)
            else {
                return false;
            };
            let value = value
                .split(['\n', '\r'])
                .next()
                .unwrap_or("")
                .trim();
            !value.is_empty() && !is_redaction_marker(value)
        })
    })
}

fn contains_bearer_value(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.match_indices("bearer").any(|(index, _)| {
        let before_ok = index == 0
            || lower[..index]
                .chars()
                .next_back()
                .is_none_or(|character| !character.is_ascii_alphanumeric());
        if !before_ok {
            return false;
        }
        let tail = text[index + "bearer".len()..].trim_start();
        let candidate = tail
            .split(|character: char| {
                character.is_ascii_whitespace() || matches!(character, ',' | ';' | '"' | '\'')
            })
            .next()
            .unwrap_or("")
            .trim();
        if candidate.is_empty() || is_redaction_marker(candidate) {
            return false;
        }
        !matches!(
            candidate.to_ascii_lowercase().as_str(),
            "token"
                | "tokens"
                | "authentication"
                | "auth"
                | "header"
                | "headers"
                | "disabled"
                | "omitted"
                | "compatibility"
        )
    })
}

/// Walk a JSON value and return the first match against the credential-path
/// or token policy. Sensitive object keys are accepted only when their values
/// are empty, null, or an explicit redaction marker.
pub fn find_sensitive(value: &Value) -> Option<SensitiveMatch> {
    match value {
        Value::String(text) if is_redaction_marker(text) => None,
        Value::String(text) if sensitive_basename(text) => Some(SensitiveMatch {
            category: "credential_path",
        }),
        Value::String(text) if contains_private_key(text) => Some(SensitiveMatch {
            category: "private_key",
        }),
        Value::String(text) if contains_jwt(text) => Some(SensitiveMatch { category: "jwt" }),
        Value::String(text) if contains_labeled_secret(text) => Some(SensitiveMatch {
            category: "sensitive_field",
        }),
        Value::String(text) if sensitive_prefix(text) => Some(SensitiveMatch {
            category: "token_prefix",
        }),
        Value::String(text) if contains_bearer_value(text) => Some(SensitiveMatch {
            category: "authorization",
        }),
        Value::Array(values) => values.iter().find_map(find_sensitive),
        Value::Object(values) => values.iter().find_map(|(key, value)| {
            if sensitive_key(key) && !safely_absent_or_redacted(value) {
                Some(SensitiveMatch {
                    category: "sensitive_key",
                })
            } else {
                find_sensitive(value)
            }
        }),
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
    fn ignores_token_shapes_in_non_sensitive_object_keys() {
        let v = json!({ "ghp_looks_like_a_key_not_a_value": "safe" });
        assert_eq!(find_sensitive(&v), None);
    }

    #[test]
    fn rejects_embedded_and_indented_authorization_and_api_keys() {
        for value in [
            "request headers: Authorization: Bearer live-token",
            "  bearer live-token",
            "OPENAI_API_KEY = sk-live-token",
            "metadata api-key: live-token",
        ] {
            assert!(
                find_sensitive(&json!({"output": value})).is_some(),
                "embedded credential must be rejected: {value:?}"
            );
        }
    }

    #[test]
    fn rejects_sensitive_object_values_unless_explicitly_redacted() {
        for value in [
            json!({"refresh_token": "opaque-oauth-value"}),
            json!({"clientSecret": "opaque-client-secret"}),
            json!({"authorization": "Basic dXNlcjpwYXNz"}),
            json!({"api-key": "opaque-provider-key"}),
        ] {
            assert!(
                find_sensitive(&value).is_some(),
                "sensitive keyed value must be rejected: {value}"
            );
        }

        for marker in ["[REDACTED]", "<redacted>", "***REDACTED***"] {
            assert_eq!(
                find_sensitive(&json!({"refresh_token": marker})),
                None,
                "explicit redaction marker must remain persistable"
            );
        }
    }

    #[test]
    fn rejects_private_keys_jwts_and_oauth_tokens() {
        for value in [
            concat!(
                "-----BEGIN ",
                "PRIVATE KEY-----\nopaque\n-----END PRIVATE KEY-----"
            ),
            concat!("-----BEGIN OPENSSH ", "PRIVATE KEY-----\nopaque"),
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJvcGVyYXRvciJ9.c2lnbmF0dXJl",
            "oauth_token=opaque-live-value",
            "refresh-token: opaque-live-value",
        ] {
            assert!(
                find_sensitive(&json!({"output": value})).is_some(),
                "credential material must be rejected: {value:?}"
            );
        }
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
