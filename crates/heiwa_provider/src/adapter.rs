use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEvent {
    pub event_type: String,
    pub payload: String,
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    async fn start_session(&self) -> Result<String>;
    async fn send_input(&self, session_id: &str, input: &str) -> Result<()>;
    async fn read_events(&self, session_id: &str) -> Result<Vec<ProviderEvent>>;
    async fn interrupt(&self, session_id: &str) -> Result<()>;
    async fn close(&self, session_id: &str) -> Result<()>;
    fn get_capabilities(&self) -> Vec<String>;
}
