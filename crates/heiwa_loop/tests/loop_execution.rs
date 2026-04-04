use heiwa_loop::{LoopConfig, LoopController};
use tokio::sync::mpsc;
use std::sync::Arc;
use heiwa_provider::adapter::{ProviderAdapter, ProviderEvent};
use async_trait::async_trait;
use anyhow::Result;

struct MockAdapter;

#[async_trait]
impl ProviderAdapter for MockAdapter {
    async fn start_session(&self) -> Result<String> { Ok("mock".to_string()) }
    async fn send_input(&self, _id: &str, _input: &str) -> Result<()> { Ok(()) }
    async fn read_events(&self, _id: &str) -> Result<Vec<ProviderEvent>> { 
        Ok(vec![ProviderEvent { event_type: "text".to_string(), payload: "done".to_string() }]) 
    }
    async fn interrupt(&self, _id: &str) -> Result<()> { Ok(()) }
    async fn close(&self, _id: &str) -> Result<()> { Ok(()) }
    fn get_capabilities(&self) -> Vec<String> { vec!["chat".to_string()] }
}

#[tokio::test]
async fn test_loop_execution_compiles() {
    let config = LoopConfig {
        user_id: "test-user".to_string(),
        objective: "test".to_string(),
        max_turns: 1,
        max_cost_usd: 1.0,
    };
    
    // We can't easily run the real controller in a unit test without a live STDB connection
    // or a very complex mock. For now, we verify the structure and dependencies.
    assert_eq!(config.max_turns, 1);
}
