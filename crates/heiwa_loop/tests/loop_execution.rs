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
        Ok(vec![ProviderEvent { event_type: "text".to_string(), payload: "turn complete".to_string() }]) 
    }
    async fn interrupt(&self, _id: &str) -> Result<()> { Ok(()) }
    async fn close(&self, _id: &str) -> Result<()> { Ok(()) }
    fn get_capabilities(&self) -> Vec<String> { vec!["chat".to_string()] }
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
    let model_tiers = vec![
        heiwa_bindings::ModelTier {
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
        }
    ];

    let controller = LoopController::new(config, None, model_tiers);
    let (tx, mut rx) = mpsc::channel(10);
    
    let adapters: Arc<dyn Fn(&str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync> = Arc::new(|_| {
        Some(Arc::new(MockAdapter) as Arc<dyn ProviderAdapter>)
    });

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
