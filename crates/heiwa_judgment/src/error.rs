//! Typed System 1 failures.
//!
//! Every runtime condition — a slow provider, a bad key, a body that does not
//! match the questions asked — becomes one of these values. None of them is a
//! panic, and none of them is ever treated as permission: a caller that cannot
//! read a judgment has no judgment.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The backend has no credential or endpoint to call.
    NotConfigured,
    /// The wall-clock budget expired before an answer arrived.
    Timeout,
    /// The request never completed: connection refused, reset, DNS.
    Transport,
    RateLimited,
    ProviderUnavailable,
    Unauthorized,
    InvalidRequest,
    /// The body arrived but does not answer the questions that were asked.
    SchemaViolation,
    /// The endpoint answered with a redirect. Judgment requests never follow
    /// one: the destination was classified before the state was sent.
    Redirected,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::NotConfigured => "not_configured",
            ErrorKind::Timeout => "timeout",
            ErrorKind::Transport => "transport",
            ErrorKind::RateLimited => "rate_limited",
            ErrorKind::ProviderUnavailable => "provider_unavailable",
            ErrorKind::Unauthorized => "unauthorized",
            ErrorKind::InvalidRequest => "invalid_request",
            ErrorKind::SchemaViolation => "schema_violation",
            ErrorKind::Redirected => "redirected",
        }
    }

    /// The same classification the TypeScript client uses, so both report a
    /// provider's status identically. TypeSafe documents 401, 422, 429, 529.
    pub fn for_status(status: u16) -> Self {
        match status {
            401 | 403 => ErrorKind::Unauthorized,
            422 => ErrorKind::InvalidRequest,
            429 => ErrorKind::RateLimited,
            // Includes TypeSafe's non-standard 529 Overloaded.
            500..=599 => ErrorKind::ProviderUnavailable,
            _ => ErrorKind::InvalidRequest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JudgmentError {
    pub kind: ErrorKind,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
}

impl JudgmentError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        JudgmentError {
            kind,
            message: message.into(),
            status: None,
        }
    }

    pub fn schema(message: impl Into<String>) -> Self {
        JudgmentError::new(ErrorKind::SchemaViolation, message)
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }
}

impl std::fmt::Display for JudgmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind.as_str(), self.message)
    }
}

impl std::error::Error for JudgmentError {}
