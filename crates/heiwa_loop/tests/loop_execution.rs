use anyhow::Result;
use async_trait::async_trait;
use heiwa_loop::{LoopConfig, LoopController};
use heiwa_provider::adapter::{Message, ProviderAdapter, StreamEvent, TokenUsage};
use std::sync::Arc;
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
            .send(StreamEvent::Token("turn complete".to_string()))
            .await
            .ok();
        stream_tx
            .send(StreamEvent::Done(TokenUsage::default()))
            .await
            .ok();
        Ok(())
    }
    async fn interrupt(&self) -> Result<()> {
        Ok(())
    }
    fn supported_models(&self) -> Vec<String> {
        vec!["mock".to_string()]
    }
}

#[tokio::test]
async fn test_loop_turn_budget() {
    let config = LoopConfig {
        user_id: "test-user".to_string(),
        objective: "count to 2".to_string(),
        max_turns: 2,
        max_cost_usd: 1.0,
        intent: "code".to_string(),
        risk: "low".to_string(),
        privacy: "standard".to_string(),
        runtime: "any".to_string(),
    };

    // We pass None for stdb to use offline mode
    let model_tiers = vec![heiwa_bindings::ModelTier {
        id: 1,
        model_id: "mock-model".to_string(),
        provider_model_id: "mock".to_string(),
        provider: "mock-provider".to_string(),
        rate_group: "mock".to_string(),
        capability_class: 3,
        effort_knob: "default".to_string(),
        effort_level: 1,
        cost_per_turn: 0.0,
        max_context_tokens: 8192,
        strengths_json: "[\"advanced_coding\"]".to_string(),
        vram_requirement_mb: 0,
        quantization_type: "none".to_string(),
        kv_cache_strategy: "none".to_string(),
        enabled: true,
        last_success_rate: 1.0,
        avg_latency_ms: 0,
        latency_p_95_ms: 0,
        updated_at: "".to_string(),
    }];

    let controller = LoopController::new(config, heiwa_stdb::StdbClient::offline(), model_tiers);
    let (tx, mut rx) = mpsc::channel(10);

    let adapters: Arc<dyn Fn(&str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync> =
        Arc::new(|_| Some(Arc::new(MockAdapter) as Arc<dyn ProviderAdapter>));

    tokio::spawn(async move {
        controller.run(tx, adapters).await.expect("loop failed");
    });

    let mut turns = 0;
    while let Some(status) = rx.recv().await {
        if status.status == "RUNNING" {
            turns = status.current_turn;
        }
        if status.status == "COMPLETED" {
            break;
        }
    }

    assert_eq!(turns, 2, "Loop should have executed exactly 2 turns");
}
