use anyhow::Result;
use async_trait::async_trait;
use crate::adapter::{ProviderAdapter, ProviderEvent};

pub struct OllamaAdapter;

#[async_trait]
impl ProviderAdapter for OllamaAdapter {
    async fn start_session(&self) -> Result<String> {
        // In a real implementation, this would verify ollama is running or start it
        Ok("ollama-session".to_string())
    }
    async fn send_input(&self, _session_id: &str, input: &str) -> Result<()> {
        println!("Ollama adapter received: {}", input);
        Ok(())
    }
    async fn read_events(&self, _session_id: &str) -> Result<Vec<ProviderEvent>> {
        Ok(vec![ProviderEvent {
            event_type: "text".to_string(),
            payload: "Ollama response stub".to_string(),
        }])
    }
    async fn interrupt(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }
    async fn close(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }
    fn get_capabilities(&self) -> Vec<String> {
        vec!["local_llm".to_string(), "chat".to_string()]
    }
}
