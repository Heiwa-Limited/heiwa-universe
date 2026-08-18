//! OAuth 2.0 authorization-code flow with PKCE, for installed applications.
//!
//! Scope: this crate speaks the protocol. It does not store tokens (that is
//! `heiwa_vault`), decide when to refresh them (`heiwa_provider::oauth` already
//! has `needs_refresh`), or know which provider a connector wants. Endpoints,
//! client id, and scopes are all parameters, so the whole flow runs against a
//! mock authorization server with no account and no network.

pub mod authorize;
pub mod loopback;
pub mod pkce;
pub mod token;

pub use authorize::{build_authorization_request, AuthorizationRequest, ProviderConfig};
pub use loopback::LoopbackListener;
pub use pkce::Pkce;
pub use token::{exchange_code, refresh, TokenResponse};

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("authorization endpoint is not a valid URL: {endpoint}")]
    InvalidEndpoint {
        endpoint: String,
        source: url::ParseError,
    },

    #[error("could not bind or read the loopback redirect listener")]
    Listener(#[source] std::io::Error),

    #[error("the redirect callback did not arrive before the timeout")]
    CallbackTimeout,

    #[error("the redirect callback was not a readable authorization response")]
    MalformedCallback,

    #[error("the redirect callback carried a state value this request did not issue")]
    StateMismatch,

    #[error("authorization was refused: {reason}")]
    AuthorizationDenied { reason: String },

    #[error("could not reach the token endpoint")]
    Transport {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("token endpoint returned HTTP {status}: {body}")]
    TokenEndpoint { status: u16, body: String },

    #[error("token endpoint returned a body that is not a token response")]
    MalformedTokenResponse {
        #[source]
        source: serde_json::Error,
    },
}
