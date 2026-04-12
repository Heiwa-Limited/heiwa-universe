use anyhow::Result;
use async_trait::async_trait;
use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent};
use tokio::sync::mpsc;

struct MockAdapter;

#[async_trait]
impl ProviderAdapter for MockAdapter {
    async fn send(
        &self,
        _model: &str,
        _messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        stream_tx
            .send(StreamEvent::Token("hello from mock".to_string()))
            .await
            .ok();
        stream_tx
            .send(StreamEvent::Done(Default::default()))
            .await
            .ok();
        Ok(())
    }

    async fn interrupt(&self) -> Result<()> {
        Ok(())
    }

    fn supported_models(&self) -> Vec<String> {
        vec!["mock-model".to_string()]
    }
}

#[tokio::test]
async fn test_mock_adapter_streaming() {
    let adapter = MockAdapter;
    let (tx, mut rx) = mpsc::channel(10);

    let messages = vec![Message {
        role: Role::User,
        content: "hello".to_string(),
    }];

    adapter
        .send("mock-model", &messages, tx)
        .await
        .expect("send failed");

    let mut tokens = Vec::new();
    let mut got_done = false;
    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Token(t) => tokens.push(t),
            StreamEvent::Done(_) => {
                got_done = true;
                break;
            }
            StreamEvent::Error(e) => panic!("unexpected error: {}", e),
            _ => {}
        }
    }

    assert_eq!(tokens, vec!["hello from mock"]);
    assert!(got_done, "should receive Done event");
}
