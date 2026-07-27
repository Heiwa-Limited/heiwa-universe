use crate::registry::{AccountStatus, DetectedModel, InventoryTruth, ProviderAccount};
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;

/// Default Ollama API endpoint.
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434";
pub const ENDPOINT_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
pub const ENDPOINT_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointOverride<'a> {
    Unset,
    Value(&'a str),
    NonUnicode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaEndpoint {
    base_url: String,
}

impl OllamaEndpoint {
    pub fn as_str(&self) -> &str {
        &self.base_url
    }

    pub fn api_url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EndpointError {
    #[error("HEIWA_OLLAMA_BASE is not valid Unicode")]
    NonUnicodeOverride,
    #[error("invalid {origin} endpoint: {reason}")]
    Invalid {
        origin: &'static str,
        reason: &'static str,
    },
}

/// Resolve a normalized Ollama base URL without touching process environment.
///
/// Precedence is explicit override, stored account endpoint, then default. A
/// configured but invalid override is an error, never a cue to contact the
/// default live daemon.
pub fn resolve_endpoint(
    override_value: EndpointOverride<'_>,
    stored_endpoint: Option<&str>,
) -> Result<OllamaEndpoint, EndpointError> {
    match override_value {
        EndpointOverride::Value(value) => parse_endpoint(value, "HEIWA_OLLAMA_BASE"),
        EndpointOverride::NonUnicode => Err(EndpointError::NonUnicodeOverride),
        EndpointOverride::Unset => match stored_endpoint {
            Some(value) => parse_endpoint(value, "stored Ollama"),
            None => parse_endpoint(DEFAULT_ENDPOINT, "default Ollama"),
        },
    }
}

/// Resolve from the process environment for production calls.
pub fn resolve_configured_endpoint(
    stored_endpoint: Option<&str>,
) -> Result<OllamaEndpoint, EndpointError> {
    match std::env::var("HEIWA_OLLAMA_BASE") {
        Ok(value) => resolve_endpoint(EndpointOverride::Value(&value), stored_endpoint),
        Err(std::env::VarError::NotPresent) => {
            resolve_endpoint(EndpointOverride::Unset, stored_endpoint)
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            resolve_endpoint(EndpointOverride::NonUnicode, stored_endpoint)
        }
    }
}

/// Return the configured endpoint from the first registered Ollama account.
pub fn registered_endpoint(accounts: &[ProviderAccount]) -> Option<&str> {
    accounts
        .iter()
        .find_map(|account| match &account.credential {
            crate::registry::Credential::LocalRuntime { endpoint }
                if account.provider == "ollama" =>
            {
                Some(endpoint.as_str())
            }
            _ => None,
        })
}

fn parse_endpoint(value: &str, source: &'static str) -> Result<OllamaEndpoint, EndpointError> {
    if value.trim().is_empty() {
        return Err(EndpointError::Invalid {
            origin: source,
            reason: "empty value",
        });
    }
    let parsed = reqwest::Url::parse(value).map_err(|_| EndpointError::Invalid {
        origin: source,
        reason: "invalid URL",
    })?;
    if parsed.scheme() != "http" {
        return Err(EndpointError::Invalid {
            origin: source,
            reason: "scheme must be http",
        });
    }
    if parsed.host_str().is_none() {
        return Err(EndpointError::Invalid {
            origin: source,
            reason: "missing authority",
        });
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(EndpointError::Invalid {
            origin: source,
            reason: "credentials are not allowed",
        });
    }
    if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(EndpointError::Invalid {
            origin: source,
            reason: "path, query, and fragment are not allowed",
        });
    }
    Ok(OllamaEndpoint {
        base_url: parsed.as_str().trim_end_matches('/').to_string(),
    })
}

// ---------------------------------------------------------------------------
// Ollama API response types (subset of /api/tags)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    size: u64,
    #[serde(default)]
    details: OllamaModelDetails,
}

#[derive(Debug, Default, Deserialize)]
struct OllamaModelDetails {
    #[serde(default)]
    parameter_size: String,
    #[serde(default)]
    #[allow(dead_code)]
    family: String,
    #[serde(default)]
    #[allow(dead_code)]
    quantization_level: String,
}

#[derive(Debug, Default, Deserialize)]
struct OllamaShowResponse {
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct CapabilityFlags {
    supports_streaming: bool,
    supports_tools: bool,
    supports_vision: bool,
    supports_audio: bool,
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Probe a running Ollama instance and return detected models.
///
/// Calls `GET {endpoint}/api/tags` and maps each model to a `DetectedModel`
/// with `inventory_truth: Verified`.
///
/// Updates the account's `status` and `models` fields in place.
pub async fn detect_models(account: &mut ProviderAccount) -> anyhow::Result<()> {
    let stored_endpoint = match &account.credential {
        crate::registry::Credential::LocalRuntime { endpoint } => Some(endpoint.as_str()),
        _ => None,
    };
    let endpoint = match resolve_configured_endpoint(stored_endpoint) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            account.status = AccountStatus::Error(error.to_string());
            account.models.clear();
            return Err(anyhow::anyhow!(error));
        }
    };
    detect_models_at_endpoint(account, endpoint).await
}

async fn detect_models_at_endpoint(
    account: &mut ProviderAccount,
    endpoint: OllamaEndpoint,
) -> anyhow::Result<()> {
    let url = endpoint.api_url("/api/tags");
    let client = reqwest::Client::builder()
        .connect_timeout(ENDPOINT_CONNECT_TIMEOUT)
        .timeout(ENDPOINT_REQUEST_TIMEOUT)
        .no_proxy()
        .build()?;

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            account.status = AccountStatus::Disconnected;
            account.models.clear();
            return Err(anyhow::anyhow!(
                "Ollama not reachable at {}: {}",
                endpoint.as_str(),
                e
            ));
        }
    };

    if !resp.status().is_success() {
        account.status = AccountStatus::Error(format!("HTTP {}", resp.status()));
        account.models.clear();
        return Err(anyhow::anyhow!("Ollama returned HTTP {}", resp.status()));
    }

    let tags: OllamaTagsResponse = resp.json().await?;

    let mut detected_models = Vec::with_capacity(tags.models.len());
    for m in tags.models {
        let capability_class = infer_capability_class(&m.name, &m.details);
        let context_window = infer_context_window(&m.name, &m.details);
        let capability_flags = match fetch_capability_flags(&client, &endpoint, &m.name).await {
            Ok(flags) => flags,
            Err(_) => infer_capability_flags_from_name(&m.name),
        };

        detected_models.push(DetectedModel {
            model_id: m.name.clone(),
            provider_model_id: m.name,
            provider: "ollama".to_string(),
            account_id: account.account_id.clone(),
            rate_group: "local".to_string(),
            capability_class,
            context_window,
            supports_streaming: capability_flags.supports_streaming,
            supports_tools: capability_flags.supports_tools,
            supports_vision: capability_flags.supports_vision,
            supports_audio: capability_flags.supports_audio,
            cost_per_1k_input: 0.0,
            cost_per_1k_output: 0.0,
            inventory_truth: InventoryTruth::Verified,
        });
    }
    account.models = detected_models;

    account.status = AccountStatus::Connected;
    Ok(())
}

/// Heuristic: infer capability class from model name and parameter size.
fn infer_capability_class(name: &str, details: &OllamaModelDetails) -> u8 {
    // Parse parameter count from details or name
    let param_billions = parse_param_billions(&details.parameter_size)
        .or_else(|| parse_param_billions_from_name(name))
        .unwrap_or(0.0);

    if param_billions >= 30.0 {
        4
    } else if param_billions >= 7.0 {
        3
    } else if param_billions >= 3.0 {
        2
    } else {
        1
    }
}

fn infer_context_window(name: &str, _details: &OllamaModelDetails) -> u32 {
    // Most modern Ollama models default to 128k or less depending on config.
    // Qwen3 models support up to 128k, Llama3 up to 128k.
    if name.contains("qwen3") || name.contains("llama3") {
        128_000
    } else if name.contains("qwen2.5") {
        32_768
    } else {
        8_192
    }
}

fn parse_param_billions(s: &str) -> Option<f64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }
    // Handle formats like "4B", "3.5B", "0.5B", "1.5b"
    let s = s.trim_end_matches('b');
    s.parse::<f64>().ok()
}

fn parse_param_billions_from_name(name: &str) -> Option<f64> {
    // Try to extract from patterns like "llama3.2:3b", "qwen2.5-coder:1.5b"
    let name = name.to_lowercase();
    // Look for a segment like ":3b" or ":1.5b" or ":0.5b"
    if let Some(pos) = name.rfind(':') {
        let suffix = &name[pos + 1..];
        let suffix = suffix.trim_end_matches('b');
        if let Ok(n) = suffix.parse::<f64>() {
            return Some(n);
        }
    }
    // Look for "-4b" or similar in the name
    for part in name.split(&['-', '_'][..]) {
        let part = part.trim_end_matches('b');
        if let Ok(n) = part.parse::<f64>() {
            if n > 0.0 && n < 1000.0 {
                return Some(n);
            }
        }
    }
    None
}

async fn fetch_capability_flags(
    client: &reqwest::Client,
    endpoint: &OllamaEndpoint,
    model_name: &str,
) -> anyhow::Result<CapabilityFlags> {
    let url = endpoint.api_url("/api/show");
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "model": model_name }))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Ollama show returned HTTP {}", resp.status());
    }

    let show: OllamaShowResponse = resp.json().await?;
    Ok(capability_flags_from_list(&show.capabilities))
}

fn capability_flags_from_list(capabilities: &[String]) -> CapabilityFlags {
    let has = |needle: &str| capabilities.iter().any(|item| item == needle);
    let supports_streaming =
        has("completion") || has("thinking") || has("vision") || has("audio") || has("tools");
    CapabilityFlags {
        supports_streaming,
        supports_tools: has("tools"),
        supports_vision: has("vision"),
        supports_audio: has("audio"),
    }
}

fn infer_capability_flags_from_name(name: &str) -> CapabilityFlags {
    let lowered = name.to_lowercase();
    CapabilityFlags {
        supports_streaming: !lowered.contains("embedding"),
        supports_tools: false,
        supports_vision: false,
        supports_audio: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_param_billions_from_detail() {
        assert_eq!(parse_param_billions("4B"), Some(4.0));
        assert_eq!(parse_param_billions("1.5b"), Some(1.5));
        assert_eq!(parse_param_billions("0.5B"), Some(0.5));
        assert_eq!(parse_param_billions(""), None);
    }

    #[test]
    fn parse_param_billions_from_model_name() {
        assert_eq!(parse_param_billions_from_name("llama3.2:3b"), Some(3.0));
        assert_eq!(
            parse_param_billions_from_name("qwen2.5-coder:1.5b"),
            Some(1.5)
        );
        assert_eq!(
            parse_param_billions_from_name("qwen2.5-coder:0.5b"),
            Some(0.5)
        );
        assert_eq!(parse_param_billions_from_name("qwen3.5:4b"), Some(4.0));
    }

    #[test]
    fn capability_class_heuristics() {
        let default_details = OllamaModelDetails::default();
        assert_eq!(infer_capability_class("qwen3.5:4b", &default_details), 2);
        assert_eq!(infer_capability_class("llama3.2:3b", &default_details), 2);
        assert_eq!(
            infer_capability_class("qwen2.5-coder:0.5b", &default_details),
            1
        );

        let big = OllamaModelDetails {
            parameter_size: "70B".to_string(),
            ..Default::default()
        };
        assert_eq!(infer_capability_class("llama3:70b", &big), 4);
    }

    #[test]
    fn capability_flags_detect_embedding_only_models() {
        let capabilities = vec!["embedding".to_string()];
        let flags = capability_flags_from_list(&capabilities);
        assert!(!flags.supports_streaming);
        assert!(!flags.supports_tools);
        assert!(!flags.supports_vision);
        assert!(!flags.supports_audio);
    }

    #[test]
    fn capability_flags_detect_multimodal_models() {
        let capabilities = vec![
            "completion".to_string(),
            "vision".to_string(),
            "audio".to_string(),
            "tools".to_string(),
            "thinking".to_string(),
        ];
        let flags = capability_flags_from_list(&capabilities);
        assert!(flags.supports_streaming);
        assert!(flags.supports_tools);
        assert!(flags.supports_vision);
        assert!(flags.supports_audio);
    }

    #[test]
    fn fallback_capability_flags_disable_streaming_for_embedding_models() {
        let flags = infer_capability_flags_from_name("qwen3-embedding:0.6b");
        assert!(!flags.supports_streaming);
        assert!(!flags.supports_tools);
        assert!(!flags.supports_vision);
        assert!(!flags.supports_audio);
    }

    #[test]
    fn resolver_prefers_override_normalizes_trailing_slash() {
        let endpoint = resolve_endpoint(
            EndpointOverride::Value("http://127.0.0.1:11435/"),
            Some("http://127.0.0.1:11434"),
        )
        .unwrap();
        assert_eq!(endpoint.as_str(), "http://127.0.0.1:11435");
        assert_eq!(
            endpoint.api_url("/api/tags"),
            "http://127.0.0.1:11435/api/tags"
        );
    }

    #[test]
    fn resolver_uses_stored_endpoint_before_default() {
        let endpoint =
            resolve_endpoint(EndpointOverride::Unset, Some("http://127.0.0.1:11436/")).unwrap();
        assert_eq!(endpoint.as_str(), "http://127.0.0.1:11436");
    }

    #[test]
    fn resolver_rejects_invalid_overrides_without_fallback() {
        for override_value in [
            EndpointOverride::Value(""),
            EndpointOverride::Value("not-a-url"),
            EndpointOverride::Value("https://127.0.0.1:11434"),
            EndpointOverride::NonUnicode,
        ] {
            assert!(resolve_endpoint(override_value, Some(DEFAULT_ENDPOINT)).is_err());
        }
    }

    #[test]
    fn invalid_override_does_not_probe_stored_or_live_endpoint() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let account = ProviderAccount {
            account_id: "invalid-override".to_string(),
            provider: "ollama".to_string(),
            credential: crate::registry::Credential::LocalRuntime { endpoint },
            rate_group: "local".to_string(),
            status: AccountStatus::Connected,
            models: vec![],
        };

        let result = resolve_endpoint(
            EndpointOverride::Value("malformed-endpoint"),
            match &account.credential {
                crate::registry::Credential::LocalRuntime { endpoint } => Some(endpoint.as_str()),
                _ => None,
            },
        );
        assert!(matches!(&result, Err(EndpointError::Invalid { .. })));
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
}
