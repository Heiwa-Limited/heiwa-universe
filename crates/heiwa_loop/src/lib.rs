pub mod fanout;

pub use fanout::{
    aggregate_child_harness_receipts, plan_recursive_harness, AggregatedHarnessReceipt,
    ChildHarnessReceipt, ChildHarnessStatus, ChildHarnessTask, RecursiveHarnessConstraints,
    RecursiveHarnessEntry, RecursiveHarnessPlan, RecursiveHarnessStrategy,
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Utc;
use heiwa_core::drex::{
    CallRisk, CostTruth, ExecutionLocality, ModelCallCandidate, ModelCallStage, PrivacyClass,
};
use heiwa_core::evidence::{EvidenceTransport, JsonlTransport};
use heiwa_protocol::{ModelTier, TranscriptBlock};
use heiwa_provider::adapter::{Message, Role, TokenUsage};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopConfig {
    pub user_id: String,
    pub objective: String,
    pub max_turns: u32,
    pub max_cost_usd: f64,
    pub intent: String,
    pub risk: String,
    pub privacy: String,
    pub runtime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStatus {
    pub loop_id: String,
    pub current_turn: u32,
    pub total_cost_usd: f64,
    pub status: String,
    pub latest_block: Option<TranscriptBlock>,
}

#[derive(Debug, Clone)]
pub struct LoopCallRequest {
    pub thread_id: String,
    pub turn_id: String,
    pub call_id: String,
    pub stage: ModelCallStage,
    pub intent: String,
    pub raw_text: String,
    pub privacy: PrivacyClass,
    pub risk: CallRisk,
    pub messages: Vec<Message>,
    pub candidates: Vec<ModelCallCandidate>,
    pub remaining_budget_usd: Option<f64>,
    pub prior_failed_models: Vec<String>,
    pub max_attempts: usize,
}

#[derive(Debug, Clone)]
pub struct LoopCallResult {
    pub provider: String,
    pub model_id: String,
    pub text: String,
    pub usage: TokenUsage,
    pub attempts: usize,
    pub failed_models: Vec<String>,
}

#[async_trait]
pub trait LoopModelCaller: Send + Sync {
    async fn call(&self, request: LoopCallRequest) -> Result<LoopCallResult>;
}

pub struct LoopController {
    config: LoopConfig,
    loop_id: String,
    cancelled: Arc<AtomicBool>,
    evidence: Option<JsonlTransport>,
    model_tiers: Vec<ModelTier>,
}

impl LoopController {
    pub fn new(config: LoopConfig, model_tiers: Vec<ModelTier>) -> Self {
        Self {
            config,
            loop_id: Uuid::new_v4().to_string(),
            cancelled: Arc::new(AtomicBool::new(false)),
            evidence: JsonlTransport::default_local().ok(),
            model_tiers,
        }
    }

    fn journal(&self, kind: &str, payload: serde_json::Value) {
        if let Some(evidence) = &self.evidence {
            let _ = evidence.journal(kind, payload);
        }
    }

    pub fn get_id(&self) -> String {
        self.loop_id.clone()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub async fn run(
        &self,
        status_tx: mpsc::Sender<LoopStatus>,
        caller: Arc<dyn LoopModelCaller>,
    ) -> Result<()> {
        println!(
            "Starting loop {} with objective: {}",
            self.loop_id, self.config.objective
        );

        // 1. Journal loop session start
        self.journal(
            "loop_sessions",
            json!({
                "loop_id": self.loop_id,
                "user_id": self.config.user_id,
                "objective": self.config.objective,
                "max_turns": self.config.max_turns,
                "max_cost_usd": self.config.max_cost_usd,
                "status": "STARTED",
            }),
        );

        let mut current_turn = 0;
        let mut total_cost = 0.0;
        let mut last_summary = String::new();
        let mut prior_failed_models = Vec::new();

        while current_turn < self.config.max_turns {
            if self.cancelled.load(Ordering::SeqCst) {
                println!("Loop {} cancelled.", self.loop_id);
                self.journal(
                    "loop_sessions",
                    json!({
                        "loop_id": self.loop_id,
                        "status": "CANCELLED",
                        "reason": "User requested cancellation",
                    }),
                );

                let _ = status_tx
                    .send(LoopStatus {
                        loop_id: self.loop_id.clone(),
                        current_turn,
                        total_cost_usd: total_cost,
                        status: "CANCELLED".to_string(),
                        latest_block: None,
                    })
                    .await;

                return Ok(());
            }

            current_turn += 1;
            let turn_started_at = Utc::now().to_rfc3339();
            println!("Turn {}/{}...", current_turn, self.config.max_turns);

            // 2. One routed execution request. The caller owns the sole
            // provider-send boundary; the loop only supplies per-iteration
            // identity, policy inputs, candidates, and remaining budget.
            let raw_text = format!(
                "Objective: {}. Context: {}",
                self.config.objective, last_summary
            );
            let messages = vec![Message {
                role: Role::User,
                content: raw_text.clone(),
            }];
            let privacy = PrivacyClass::parse(&self.config.privacy)
                .map_err(|error| anyhow!("invalid loop privacy: {error}"))?;
            let risk = CallRisk::parse(&self.config.risk)
                .map_err(|error| anyhow!("invalid loop risk: {error}"))?;
            let candidates = loop_candidates(&self.model_tiers);
            let call_id = format!("call-{}", Uuid::new_v4());
            let result = caller
                .call(LoopCallRequest {
                    thread_id: format!("loop-{}", self.loop_id),
                    turn_id: format!("loop-{}-iteration-{current_turn}", self.loop_id),
                    call_id,
                    stage: ModelCallStage::LoopIteration,
                    intent: self.config.intent.clone(),
                    raw_text: raw_text.clone(),
                    privacy,
                    risk,
                    messages,
                    candidates,
                    remaining_budget_usd: Some((self.config.max_cost_usd - total_cost).max(0.0)),
                    prior_failed_models: prior_failed_models.clone(),
                    max_attempts: 3,
                })
                .await?;
            for failed in &result.failed_models {
                if !prior_failed_models.contains(failed) {
                    prior_failed_models.push(failed.clone());
                }
            }

            let output_summary = result.text;
            let turn_usage = result.usage;
            let turn_ended_at = Utc::now().to_rfc3339();
            last_summary = output_summary.clone();

            // 4. Record evidence in the local journal.
            let run_id = format!("run-{}", Uuid::new_v4());
            let turn_cost = if turn_usage.cost_usd > 0.0 {
                turn_usage.cost_usd
            } else {
                self.model_tiers
                    .iter()
                    .find(|tier| tier.model_id == result.model_id)
                    .map(|tier| tier.cost_per_turn)
                    .unwrap_or(0.0)
            };
            if !turn_cost.is_finite() || turn_cost < 0.0 {
                return Err(anyhow!("model caller returned invalid cost_usd"));
            }
            total_cost += turn_cost;

            self.journal(
                "runs",
                json!({
                    "run_id": run_id,
                    "user_id": self.config.user_id,
                    "proposal_id": format!("loop-{}", self.loop_id),
                    "loop_id": self.loop_id,
                    "started_at": turn_started_at,
                    "ended_at": turn_ended_at,
                    "status": "SUCCESS",
                    "mode": "loop",
                    "provider": result.provider,
                    "model_id": result.model_id,
                    "attempts": result.attempts,
                    "tokens_input": turn_usage.input_tokens,
                    "tokens_output": turn_usage.output_tokens,
                    "cost": turn_cost,
                }),
            );
            self.journal(
                "loop_iterations",
                json!({
                    "iteration_id": Uuid::new_v4().to_string(),
                    "loop_id": self.loop_id,
                    "turn": current_turn,
                    "input": raw_text,
                    "output_summary": output_summary,
                    "run_id": run_id,
                    "cost": turn_cost,
                }),
            );

            let _ = status_tx
                .send(LoopStatus {
                    loop_id: self.loop_id.clone(),
                    current_turn,
                    total_cost_usd: total_cost,
                    status: "RUNNING".to_string(),
                    latest_block: Some(TranscriptBlock::Assistant(output_summary)),
                })
                .await;

            if total_cost >= self.config.max_cost_usd {
                println!("Loop {} exceeded cost budget.", self.loop_id);
                break;
            }
        }

        // 5. Journal session completion
        self.journal(
            "loop_sessions",
            json!({
                "loop_id": self.loop_id,
                "status": "COMPLETED",
                "reason": "Max turns reached or objective met",
            }),
        );

        let _ = status_tx
            .send(LoopStatus {
                loop_id: self.loop_id.clone(),
                current_turn,
                total_cost_usd: total_cost,
                status: "COMPLETED".to_string(),
                latest_block: None,
            })
            .await;

        println!("Loop {} finished.", self.loop_id);
        Ok(())
    }
}

fn loop_candidates(model_tiers: &[ModelTier]) -> Vec<ModelCallCandidate> {
    model_tiers
        .iter()
        .cloned()
        .map(|tier| {
            let on_device = tier.rate_group == "local_ollama" && tier.vram_requirement_mb > 0;
            let marginal_cost_usd = if tier.cost_per_turn == 0.0 && !on_device {
                None
            } else {
                Some(tier.cost_per_turn)
            };
            let cost_truth = if tier.cost_per_turn == 0.0 {
                if on_device {
                    CostTruth::LocalZeroCost
                } else {
                    CostTruth::CannotConfirm
                }
            } else {
                CostTruth::ProxyEstimate
            };
            ModelCallCandidate {
                tier,
                locality: if on_device {
                    ExecutionLocality::OnDevice
                } else {
                    ExecutionLocality::Unverified
                },
                connected: true,
                adapter_capable: true,
                quota_available: true,
                marginal_cost_usd,
                cost_truth,
            }
        })
        .collect()
}
