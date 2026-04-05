use anyhow::{anyhow, Result};
use heiwa_protocol::TranscriptBlock;
use heiwa_bindings::{
    complete_loop_session_reducer::complete_loop_session,
    record_loop_iteration_reducer::record_loop_iteration,
    start_loop_session_reducer::start_loop_session,
    record_run_reducer::record_run,
    ModelTier,
};
use heiwa_stdb::StdbClient;
use heiwa_core::drex::{default_policy, plan_route, DrexIngress};
use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;
use chrono::Utc;

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

pub struct LoopController {
    config: LoopConfig,
    loop_id: String,
    cancelled: Arc<AtomicBool>,
    stdb: StdbClient,
    model_tiers: Vec<ModelTier>,
}

impl LoopController {
    pub fn new(config: LoopConfig, stdb: StdbClient, model_tiers: Vec<ModelTier>) -> Self {
        Self {
            config,
            loop_id: Uuid::new_v4().to_string(),
            cancelled: Arc::new(AtomicBool::new(false)),
            stdb,
            model_tiers,
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
        adapters: Arc<dyn Fn(&str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync>,
    ) -> Result<()> {
        println!(
            "Starting loop {} with objective: {}",
            self.loop_id, self.config.objective
        );

        // 1. Initialize Loop Session in STDB
        if let Some(conn) = self.stdb.connection() {
            conn.reducers.start_loop_session(
                self.loop_id.clone(),
                self.config.user_id.clone(),
                self.config.objective.clone(),
                self.config.max_turns,
                self.config.max_cost_usd,
            ).map_err(|e| anyhow!(e.to_string()))?;
        }

        let mut current_turn = 0;
        let mut total_cost = 0.0;
        let mut last_summary = String::new();

        while current_turn < self.config.max_turns {
            if self.cancelled.load(Ordering::SeqCst) {
                println!("Loop {} cancelled.", self.loop_id);
                if let Some(conn) = self.stdb.connection() {
                    conn.reducers.complete_loop_session(
                        self.loop_id.clone(),
                        "CANCELLED".to_string(),
                        "User requested cancellation".to_string(),
                    ).map_err(|e| anyhow!(e.to_string()))?;
                }

                let _ = status_tx.send(LoopStatus {
                    loop_id: self.loop_id.clone(),
                    current_turn,
                    total_cost_usd: total_cost,
                    status: "CANCELLED".to_string(),
                    latest_block: None,
                }).await;

                return Ok(());
            }

            current_turn += 1;
            let turn_started_at = Utc::now().to_rfc3339();
            println!("Turn {}/{}...", current_turn, self.config.max_turns);

            // 2. DREX Routing
            let ingress = DrexIngress {
                intent: self.config.intent.clone(),
                risk: self.config.risk.clone(),
                raw_text: format!("Objective: {}. Context: {}", self.config.objective, last_summary),
                privacy: self.config.privacy.clone(),
                runtime: self.config.runtime.clone(),
                available_vram_mb: 8192,
                required_context_tokens: 1024,
            };

            let policy = default_policy();
            let route_plan = plan_route(&ingress, &self.model_tiers, &policy)?;

            let selected_tier = route_plan.selected_model.ok_or_else(|| anyhow!("No model available for turn {}", current_turn))?;
            let adapter = adapters(&selected_tier.provider).ok_or_else(|| anyhow!("No adapter found for provider {}", selected_tier.provider))?;

            // 3. Execution via streaming adapter
            let messages = vec![Message {
                role: Role::User,
                content: ingress.raw_text.clone(),
            }];

            let (stream_tx, mut stream_rx) = mpsc::channel(32);
            let adapter_clone = adapter.clone();
            let model_id = selected_tier.model_id.clone();

            tokio::spawn(async move {
                if let Err(e) = adapter_clone.send(&model_id, &messages, stream_tx).await {
                    eprintln!("Adapter error: {}", e);
                }
            });

            // Collect streamed output
            let mut output_parts = Vec::new();
            let mut turn_usage = heiwa_provider::adapter::TokenUsage::default();

            while let Some(event) = stream_rx.recv().await {
                match event {
                    StreamEvent::Token(text) => output_parts.push(text),
                    StreamEvent::Done(usage) => {
                        turn_usage = usage;
                        break;
                    }
                    StreamEvent::Error(e) => {
                        eprintln!("Stream error: {}", e);
                        break;
                    }
                    StreamEvent::ToolUse { .. } => { /* future */ }
                }
            }

            let output_summary = output_parts.join("\n");
            let turn_ended_at = Utc::now().to_rfc3339();
            last_summary = output_summary.clone();

            // 4. Record Evidence in STDB
            let run_id = format!("run-{}", Uuid::new_v4());
            let turn_cost = if turn_usage.cost_usd > 0.0 {
                turn_usage.cost_usd
            } else {
                selected_tier.cost_per_turn
            };
            total_cost += turn_cost;

            if let Some(conn) = self.stdb.connection() {
                conn.reducers.record_run(
                    run_id.clone(),
                    self.config.user_id.clone(),
                    format!("loop-{}", self.loop_id),
                    "loop-lease".to_string(),
                    Some(self.loop_id.clone()),
                    turn_started_at,
                    turn_ended_at,
                    "SUCCESS".to_string(),
                    "{}".to_string(),
                    "{}".to_string(),
                    "[]".to_string(),
                    "local-node".to_string(),
                    "{}".to_string(),
                    "loop".to_string(),
                    selected_tier.model_id.clone(),
                    turn_usage.input_tokens as i64,
                    turn_usage.output_tokens as i64,
                    0,
                    turn_cost,
                    None, None,
                    None, None,
                ).map_err(|e| anyhow!(e.to_string()))?;

                let iteration_id = Uuid::new_v4().to_string();
                conn.reducers.record_loop_iteration(
                    iteration_id,
                    self.loop_id.clone(),
                    current_turn,
                    ingress.raw_text.clone(),
                    output_summary,
                    0.5,
                    Some(run_id),
                    turn_cost,
                ).map_err(|e| anyhow!(e.to_string()))?;
            }

            let _ = status_tx.send(LoopStatus {
                loop_id: self.loop_id.clone(),
                current_turn,
                total_cost_usd: total_cost,
                status: "RUNNING".to_string(),
                latest_block: Some(TranscriptBlock::Assistant(output_summary)),
            }).await;

            if total_cost >= self.config.max_cost_usd {
                println!("Loop {} exceeded cost budget.", self.loop_id);
                break;
            }
        }

        // 5. Finalize Session in STDB
        if let Some(conn) = self.stdb.connection() {
            conn.reducers.complete_loop_session(
                self.loop_id.clone(),
                "COMPLETED".to_string(),
                "Max turns reached or objective met".to_string(),
            ).map_err(|e| anyhow!(e.to_string()))?;
        }

        let _ = status_tx.send(LoopStatus {
            loop_id: self.loop_id.clone(),
            current_turn,
            total_cost_usd: total_cost,
            status: "COMPLETED".to_string(),
            latest_block: None,
        }).await;

        println!("Loop {} finished.", self.loop_id);
        Ok(())
    }
}
