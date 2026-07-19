use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use heiwa_core::drex::{ModelCallStage, SafetyClass};
use heiwa_loop::{LoopCallRequest, LoopCallResult, LoopConfig, LoopController, LoopModelCaller};
use heiwa_provider::adapter::TokenUsage;
use tokio::sync::mpsc;

#[derive(Default)]
struct RecordingCaller {
    calls: Mutex<Vec<LoopCallRequest>>,
}

struct CancellationCaller {
    started: Arc<tokio::sync::Notify>,
}

#[async_trait]
impl LoopModelCaller for CancellationCaller {
    async fn call(&self, mut request: LoopCallRequest) -> Result<LoopCallResult> {
        self.started.notify_one();
        request.cancel.changed().await?;
        anyhow::bail!("caller should be dropped by loop cancellation")
    }
}

#[async_trait]
impl LoopModelCaller for RecordingCaller {
    async fn call(&self, request: LoopCallRequest) -> Result<LoopCallResult> {
        let turn = self.calls.lock().unwrap().len();
        self.calls.lock().unwrap().push(request);
        Ok(LoopCallResult {
            provider: "mock-provider".to_string(),
            model_id: "mock-model".to_string(),
            text: "turn complete".to_string(),
            usage: TokenUsage {
                cost_usd: 0.25,
                ..TokenUsage::default()
            },
            attempts: 1,
            failed_models: if turn == 0 {
                vec!["failed-primary".to_string()]
            } else {
                vec![]
            },
            cost_usd: 0.25,
            cost_truth: heiwa_core::drex::CostTruth::ExactProviderReport,
        })
    }
}

fn model_tier() -> heiwa_protocol::ModelTier {
    heiwa_protocol::ModelTier {
        id: 1,
        model_id: "mock-model".to_string(),
        provider_model_id: "mock".to_string(),
        provider: "mock-provider".to_string(),
        rate_group: "mock".to_string(),
        capability_class: 3,
        effort_knob: "default".to_string(),
        effort_level: 1,
        cost_per_turn: 0.25,
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
}

#[tokio::test]
async fn loop_iterations_request_fresh_routed_model_calls() {
    let evidence_dir = tempfile::tempdir().unwrap();
    std::env::set_var("HEIWA_EVIDENCE_DIR", evidence_dir.path());

    let config = LoopConfig {
        user_id: "test-user".to_string(),
        objective: "count to 2".to_string(),
        max_turns: 2,
        max_cost_usd: 1.0,
        intent: "code".to_string(),
        risk: "low".to_string(),
        privacy: "standard".to_string(),
        runtime: "any".to_string(),
        approved: true,
    };

    let controller = LoopController::new(config, vec![model_tier()]);
    let (tx, mut rx) = mpsc::channel(10);
    let caller = Arc::new(RecordingCaller::default());
    let run_caller = caller.clone();

    tokio::spawn(async move {
        controller.run(tx, run_caller).await.expect("loop failed");
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

    assert_eq!(turns, 2);
    let calls = caller.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_ne!(calls[0].call_id, calls[1].call_id);
    assert_eq!(calls[0].stage, ModelCallStage::LoopIteration);
    assert_eq!(calls[1].stage, ModelCallStage::LoopIteration);
    assert_eq!(calls[0].remaining_budget_usd, Some(1.0));
    assert_eq!(calls[1].remaining_budget_usd, Some(0.75));
    assert!(calls[0].prior_failed_models.is_empty());
    assert_eq!(calls[1].prior_failed_models, vec!["failed-primary"]);
    assert_eq!(calls[0].candidates.len(), 1);
    assert_eq!(calls[0].safety, SafetyClass::Approved);
}

#[test]
fn loop_config_defaults_to_unapproved_when_field_is_absent() {
    let config: LoopConfig = serde_json::from_value(serde_json::json!({
        "user_id": "test-user",
        "objective": "safe default",
        "max_turns": 1,
        "max_cost_usd": 1.0,
        "intent": "code",
        "risk": "high",
        "privacy": "standard",
        "runtime": "any"
    }))
    .unwrap();
    assert!(!config.approved);
}

#[tokio::test]
async fn cancel_interrupts_an_active_loop_model_call() {
    let evidence_dir = tempfile::tempdir().unwrap();
    std::env::set_var("HEIWA_EVIDENCE_DIR", evidence_dir.path());
    let controller = Arc::new(LoopController::new(
        LoopConfig {
            user_id: "test-user".to_string(),
            objective: "long call".to_string(),
            max_turns: 1,
            max_cost_usd: 1.0,
            intent: "code".to_string(),
            risk: "low".to_string(),
            privacy: "standard".to_string(),
            runtime: "any".to_string(),
            approved: false,
        },
        vec![model_tier()],
    ));
    let started = Arc::new(tokio::sync::Notify::new());
    let caller = Arc::new(CancellationCaller {
        started: started.clone(),
    });
    let (tx, mut rx) = mpsc::channel(10);
    let run_controller = controller.clone();
    let task = tokio::spawn(async move { run_controller.run(tx, caller).await });

    started.notified().await;
    controller.cancel();
    let status = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.status, "CANCELLED");
    task.await.unwrap().unwrap();
}
