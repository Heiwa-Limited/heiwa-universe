use std::sync::Arc;
use tokio::sync::RwLock;
use crate::config::RuntimeConfig;
use heiwa_bindings::ModelTier;

pub struct CoreState {
    pub config: RuntimeConfig,
    pub model_tiers: RwLock<Vec<ModelTier>>,
    pub status: RwLock<SystemStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemStatus {
    Starting,
    Ready,
    Degraded,
}

impl CoreState {
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            config,
            model_tiers: RwLock::new(Vec::new()),
            status: RwLock::new(SystemStatus::Starting),
        }
    }
}

pub type SharedState = Arc<CoreState>;
