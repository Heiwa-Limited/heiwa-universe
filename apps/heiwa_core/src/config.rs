use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub port: u16,
    pub state_backend: String,
    pub log_level: String,
    pub machine_auth_token: String,
    pub jwt_signing_secret: String,
    pub node_id: String,
    pub model_tiers_seed_path: String,
    pub ai_router_seed_path: String,
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
            machine_auth_token: env::var("HEIWA_MACHINE_AUTH_TOKEN")
                .or_else(|_| env::var("HEIWA_AUTH_TOKEN"))
                .unwrap_or_default(),
            jwt_signing_secret: env::var("HEIWA_JWT_SIGNING_SECRET")
                .or_else(|_| env::var("HEIWA_AUTH_SECRET"))
                .unwrap_or_default(),
            node_id: env::var("HEIWA_NODE_ID").unwrap_or_else(|_| "heiwa-core-0".to_string()),
            model_tiers_seed_path: env::var("MODEL_TIERS_SEED_PATH")
                .unwrap_or_else(|_| "config/seeds/model_tiers.json".to_string()),
            ai_router_seed_path: env::var("AI_ROUTER_SEED_PATH")
                .unwrap_or_else(|_| "config/swarm/ai_router.json".to_string()),
        }
    }
}
