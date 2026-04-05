use crate::registry::{AccountStatus, DetectedModel, InventoryTruth, ProviderAccount};
use serde::Deserialize;

/// Default Ollama API endpoint.
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434";

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
    let endpoint = match &account.credential {
        crate::registry::Credential::LocalRuntime { endpoint } => endpoint.clone(),
        _ => DEFAULT_ENDPOINT.to_string(),
    };

    let url = format!("{}/api/tags", endpoint);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            account.status = AccountStatus::Disconnected;
            account.models.clear();
            return Err(anyhow::anyhow!("Ollama not reachable at {}: {}", endpoint, e));
        }
    };

    if !resp.status().is_success() {
        account.status = AccountStatus::Error(format!("HTTP {}", resp.status()));
        account.models.clear();
        return Err(anyhow::anyhow!("Ollama returned HTTP {}", resp.status()));
    }

    let tags: OllamaTagsResponse = resp.json().await?;

    account.models = tags
        .models
        .into_iter()
        .map(|m| {
            let capability_class = infer_capability_class(&m.name, &m.details);
            let context_window = infer_context_window(&m.name, &m.details);

            DetectedModel {
                model_id: m.name.clone(),
                provider_model_id: m.name,
                provider: "ollama".to_string(),
                account_id: account.account_id.clone(),
                rate_group: "local".to_string(),
                capability_class,
                context_window,
                supports_streaming: true,
                supports_tools: false, // conservative — depends on model
                supports_vision: false,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
                inventory_truth: InventoryTruth::Verified,
            }
        })
        .collect();

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
        assert_eq!(parse_param_billions_from_name("qwen2.5-coder:1.5b"), Some(1.5));
        assert_eq!(parse_param_billions_from_name("qwen2.5-coder:0.5b"), Some(0.5));
        assert_eq!(parse_param_billions_from_name("qwen3.5:4b"), Some(4.0));
    }

    #[test]
    fn capability_class_heuristics() {
        let default_details = OllamaModelDetails::default();
        assert_eq!(infer_capability_class("qwen3.5:4b", &default_details), 2);
        assert_eq!(infer_capability_class("llama3.2:3b", &default_details), 2);
        assert_eq!(infer_capability_class("qwen2.5-coder:0.5b", &default_details), 1);

        let big = OllamaModelDetails { parameter_size: "70B".to_string(), ..Default::default() };
        assert_eq!(infer_capability_class("llama3:70b", &big), 4);
    }
}
