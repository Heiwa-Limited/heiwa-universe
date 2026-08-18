//! Authorization request construction.

use crate::pkce::Pkce;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::Rng;
use url::Url;

use crate::OAuthError;

/// A provider's endpoints and this application's identity at that provider.
///
/// Endpoints are fields rather than constants so the whole flow can be pointed
/// at a mock authorization server in tests — the same reason the direct-API
/// provider adapters take `base_url` (AD-3).
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub auth_endpoint: String,
    pub token_endpoint: String,
    /// Public client identifier. This ships inside the distributed binary and
    /// is not a secret; PKCE is what makes the exchange safe (AD-17).
    pub client_id: String,
    pub scopes: Vec<String>,
}

impl ProviderConfig {
    /// Google's installed-application endpoints. The out-of-band and custom
    /// URI scheme flows are both retired, so loopback is the only option left
    /// on desktop.
    pub fn google(client_id: impl Into<String>, scopes: Vec<String>) -> Self {
        Self {
            auth_endpoint: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_endpoint: "https://oauth2.googleapis.com/token".into(),
            client_id: client_id.into(),
            scopes,
        }
    }
}

/// A pending authorization: the URL to open, plus the two secrets the callback
/// is checked against.
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    pub url: String,
    pub state: String,
    pub pkce: Pkce,
    pub redirect_uri: String,
}

/// Build the authorization URL for `redirect_uri`, which must be the loopback
/// address the listener actually bound.
pub fn build_authorization_request(
    config: &ProviderConfig,
    redirect_uri: &str,
) -> Result<AuthorizationRequest, OAuthError> {
    let pkce = Pkce::generate();
    let state = generate_state();

    let mut url = Url::parse(&config.auth_endpoint)
        .map_err(|source| OAuthError::InvalidEndpoint {
            endpoint: config.auth_endpoint.clone(),
            source,
        })?;

    url.query_pairs_mut()
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &config.scopes.join(" "))
        .append_pair("code_challenge", &pkce.challenge())
        .append_pair("code_challenge_method", pkce.method())
        .append_pair("state", &state)
        // Without this the provider returns no refresh token on repeat
        // authorizations, and the connector silently stops working when the
        // access token expires.
        .append_pair("access_type", "offline");

    Ok(AuthorizationRequest {
        url: url.to_string(),
        state,
        pkce,
        redirect_uri: redirect_uri.to_string(),
    })
}

fn generate_state() -> String {
    let bytes: [u8; 32] = rand::thread_rng().gen();
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config() -> ProviderConfig {
        ProviderConfig {
            auth_endpoint: "https://auth.example/authorize".into(),
            token_endpoint: "https://auth.example/token".into(),
            client_id: "client-123".into(),
            scopes: vec![
                "https://www.googleapis.com/auth/calendar.readonly".into(),
                "https://www.googleapis.com/auth/calendar.events".into(),
            ],
        }
    }

    fn params(url: &str) -> HashMap<String, String> {
        Url::parse(url)
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    #[test]
    fn carries_every_parameter_the_flow_requires() {
        let request = build_authorization_request(&config(), "http://127.0.0.1:7777").unwrap();
        let p = params(&request.url);

        assert_eq!(p["client_id"], "client-123");
        assert_eq!(p["redirect_uri"], "http://127.0.0.1:7777");
        assert_eq!(p["response_type"], "code");
        assert_eq!(p["code_challenge_method"], "S256");
        assert_eq!(p["state"], request.state);
        assert_eq!(p["code_challenge"], request.pkce.challenge());
        assert_eq!(p["access_type"], "offline");
    }

    #[test]
    fn scopes_are_space_delimited() {
        let request = build_authorization_request(&config(), "http://127.0.0.1:7777").unwrap();
        let p = params(&request.url);
        assert_eq!(
            p["scope"],
            "https://www.googleapis.com/auth/calendar.readonly \
             https://www.googleapis.com/auth/calendar.events"
        );
    }

    #[test]
    fn never_sends_the_verifier() {
        // Sending the verifier instead of the challenge would defeat PKCE
        // entirely while still appearing to work.
        let request = build_authorization_request(&config(), "http://127.0.0.1:7777").unwrap();
        assert!(!request.url.contains(request.pkce.verifier()));
    }

    #[test]
    fn each_request_gets_fresh_state_and_verifier() {
        let a = build_authorization_request(&config(), "http://127.0.0.1:7777").unwrap();
        let b = build_authorization_request(&config(), "http://127.0.0.1:7777").unwrap();
        assert_ne!(a.state, b.state);
        assert_ne!(a.pkce.verifier(), b.pkce.verifier());
    }

    #[test]
    fn a_malformed_endpoint_is_an_error_not_a_panic() {
        let mut cfg = config();
        cfg.auth_endpoint = "not a url".into();
        assert!(matches!(
            build_authorization_request(&cfg, "http://127.0.0.1:7777"),
            Err(OAuthError::InvalidEndpoint { .. })
        ));
    }
}
