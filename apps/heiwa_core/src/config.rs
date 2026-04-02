use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub port: u16,
    pub state_backend: String,
    pub stdb_server: String,
    pub stdb_identity: String,
    pub stdb_url: String,
    pub log_level: String,
    pub auth_token: String,
    pub auth_secret: String,
    pub node_id: String,
    pub model_tiers_seed_path: String,
    pub ai_router_seed_path: String,
}

impl RuntimeConfig {
    #[must_use]
    pub fn from_env() -> Self {
        let stdb_server = env::var("STDB_SERVER").unwrap_or_else(|_| "local".to_string());
        let stdb_url = if stdb_server == "local" {
            "http://localhost:3000".to_string()
        } else {
            "https://maincloud.spacetimedb.com".to_string()
        };

        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8080),
            state_backend: env::var("HEIWA_STATE_BACKEND").unwrap_or_else(|_| "spacetimedb".to_string()),
            stdb_server,
            stdb_identity: env::var("STDB_IDENTITY").unwrap_or_else(|_| "heiwaproductiondb".to_string()),
            stdb_url,
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "INFO".to_string()),
            auth_token: env::var("HEIWA_AUTH_TOKEN").unwrap_or_default(),
            auth_secret: env::var("HEIWA_AUTH_SECRET").unwrap_or_default(),
            node_id: env::var("HEIWA_NODE_ID").unwrap_or_else(|_| "cloud-hq-0".to_string()),
            model_tiers_seed_path: env::var("MODEL_TIERS_SEED_PATH")
                .unwrap_or_else(|_| "config/seeds/model_tiers.json".to_string()),
            ai_router_seed_path: env::var("AI_ROUTER_SEED_PATH")
                .unwrap_or_else(|_| "config/swarm/ai_router.json".to_string()),
        }
    }
}
