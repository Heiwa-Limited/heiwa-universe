use heiwa_provider::adapter::{ProviderAdapter, ProviderEvent};
use async_trait::async_trait;
use anyhow::Result;

struct MockAdapter;

#[async_trait]
impl ProviderAdapter for MockAdapter {
    async fn start_session(&self) -> Result<String> {
        Ok("mock-session".to_string())
    }
    async fn send_input(&self, _session_id: &str, _input: &str) -> Result<()> {
        Ok(())
    }
    async fn read_events(&self, _session_id: &str) -> Result<Vec<ProviderEvent>> {
        Ok(vec![ProviderEvent {
            event_type: "text".to_string(),
            payload: "hello from mock".to_string(),
        }])
    }
    async fn interrupt(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }
    async fn close(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }
    fn get_capabilities(&self) -> Vec<String> {
        vec!["chat".to_string()]
    }
}

#[tokio::test]
async fn test_mock_adapter_lifecycle() {
    let adapter = MockAdapter;
    let session_id = adapter.start_session().await.expect("failed to start");
    assert_eq!(session_id, "mock-session");
    
    let events = adapter.read_events(&session_id).await.expect("failed to read");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload, "hello from mock");
}
