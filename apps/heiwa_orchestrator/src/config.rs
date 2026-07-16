use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub port: u16,
    pub state_backend: String,
    pub log_level: String,
}

impl RuntimeConfig {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8080),
            state_backend: env::var("HEIWA_STATE_BACKEND")
                .unwrap_or_else(|_| "local-jsonl".to_string()),
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "INFO".to_string()),
        }
    }
}
