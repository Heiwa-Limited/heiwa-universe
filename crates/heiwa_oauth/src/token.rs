//! Authorization-code exchange and refresh.

use serde::Deserialize;

use crate::authorize::ProviderConfig;
use crate::OAuthError;

/// A token set as the provider returned it.
///
/// Deliberately not `heiwa_vault::OAuthSecret`: this crate speaks the protocol
/// and should not decide where the caller stores what it gets back.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    /// Absent on refresh — the provider only issues one on first authorization,
    /// so a caller must keep the original rather than overwrite it with `None`.
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// Exchange an authorization code for tokens.
pub async fn exchange_code(
    client: &reqwest::Client,
    config: &ProviderConfig,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, OAuthError> {
    let form = [
        ("client_id", config.client_id.as_str()),
        ("code", code),
        ("code_verifier", code_verifier),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri),
    ];
    post_token_request(client, &config.token_endpoint, &form).await
}

/// Trade a refresh token for a fresh access token.
pub async fn refresh(
    client: &reqwest::Client,
    config: &ProviderConfig,
    refresh_token: &str,
) -> Result<TokenResponse, OAuthError> {
    let form = [
        ("client_id", config.client_id.as_str()),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ];
    post_token_request(client, &config.token_endpoint, &form).await
}

async fn post_token_request(
    client: &reqwest::Client,
    endpoint: &str,
    form: &[(&str, &str)],
) -> Result<TokenResponse, OAuthError> {
    let response = client
        .post(endpoint)
        .form(form)
        .send()
        .await
        .map_err(|source| OAuthError::Transport {
            source: Box::new(source),
        })?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|source| OAuthError::Transport {
            source: Box::new(source),
        })?;

    if !status.is_success() {
        // Classify from the status, never by matching text in the body — the
        // same rule the provider adapters follow for credential rejection.
        return Err(OAuthError::TokenEndpoint {
            status: status.as_u16(),
            // OAuth error bodies carry `error` and `error_description`, not
            // credentials, so this is safe to surface to the user.
            body: truncate(&body, 500),
        });
    }

    serde_json::from_str(&body).map_err(|source| OAuthError::MalformedTokenResponse { source })
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut end = max;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_authorization_response() {
        let parsed: TokenResponse = serde_json::from_str(
            r#"{"access_token":"at","refresh_token":"rt","expires_in":3599,
                "token_type":"Bearer","scope":"a b"}"#,
        )
        .unwrap();
        assert_eq!(parsed.access_token, "at");
        assert_eq!(parsed.refresh_token.as_deref(), Some("rt"));
        assert_eq!(parsed.expires_in, Some(3599));
    }

    #[test]
    fn parses_a_refresh_response_that_omits_the_refresh_token() {
        // Providers return no refresh_token when refreshing. Treating that as
        // malformed would break every renewal after the first.
        let parsed: TokenResponse =
            serde_json::from_str(r#"{"access_token":"at2","expires_in":3599}"#).unwrap();
        assert_eq!(parsed.access_token, "at2");
        assert!(parsed.refresh_token.is_none());
    }

    #[test]
    fn truncate_does_not_split_a_multibyte_character() {
        let value = "é".repeat(400);
        let cut = truncate(&value, 501);
        assert!(cut.len() <= 504);
        assert!(std::str::from_utf8(cut.as_bytes()).is_ok());
    }
}
