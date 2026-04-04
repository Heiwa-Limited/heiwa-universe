use anyhow::{anyhow, Result};
use heiwa_bindings::{
    complete_loop_session_reducer::complete_loop_session,
    record_loop_iteration_reducer::record_loop_iteration,
    start_loop_session_reducer::start_loop_session,
    record_run_reducer::record_run,
    DbConnection, ModelTier,
};
use heiwa_core::drex::{default_policy, plan_route, DrexIngress};
use heiwa_provider::adapter::ProviderAdapter;
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
}

pub struct LoopController {
    config: LoopConfig,
    loop_id: String,
    cancelled: Arc<AtomicBool>,
    stdb: Option<Arc<DbConnection>>,
    model_tiers: Vec<ModelTier>,
}

impl LoopController {
    pub fn new(config: LoopConfig, stdb: Option<Arc<DbConnection>>, model_tiers: Vec<ModelTier>) -> Self {
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
        if let Some(ref stdb) = self.stdb {
            stdb.reducers.start_loop_session(
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
                if let Some(ref stdb) = self.stdb {
                    stdb.reducers.complete_loop_session(
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
                available_vram_mb: 8192, // TODO: Pull from machine manifest
                required_context_tokens: 1024,
            };
            
            let policy = default_policy();
            let route_plan = plan_route(&ingress, &self.model_tiers, &policy)?;
            
            let selected_tier = route_plan.selected_model.ok_or_else(|| anyhow!("No model available for turn {}", current_turn))?;
            let adapter = adapters(&selected_tier.provider).ok_or_else(|| anyhow!("No adapter found for provider {}", selected_tier.provider))?;

            // 3. Execution
            let session_id = adapter.start_session().await?;
            adapter.send_input(&session_id, &ingress.raw_text).await?;
            let events = adapter.read_events(&session_id).await?;
            adapter.close(&session_id).await?;

            let output_summary = events.iter()
                .filter(|e| e.event_type == "text")
                .map(|e| e.payload.clone())
                .collect::<Vec<_>>()
                .join("\n");
            
            let turn_ended_at = Utc::now().to_rfc3339();
            last_summary = output_summary.clone();
            
            // 4. Record Evidence in STDB
            let run_id = format!("run-{}", Uuid::new_v4());
            let turn_cost = selected_tier.cost_per_turn;
            total_cost += turn_cost;

            if let Some(ref stdb) = self.stdb {
                // Real Run Receipt
                stdb.reducers.record_run(
                    run_id.clone(),
                    self.config.user_id.clone(),
                    format!("loop-{}", self.loop_id), // Link to loop via proposal_id proxy or similar
                    "loop-lease".to_string(),
                    Some(self.loop_id.clone()),
                    turn_started_at,
                    turn_ended_at,
                    "SUCCESS".to_string(),
                    "{}".to_string(), // chain_result
                    "{}".to_string(), // signals
                    "[]".to_string(), // artifacts
                    "local-node".to_string(),
                    "{}".to_string(), // replay
                    "loop".to_string(),
                    selected_tier.model_id.clone(),
                    0, 0, 0, // tokens
                    turn_cost,
                    None, None, // owner/principal
                    None, None, // failure
                ).map_err(|e| anyhow!(e.to_string()))?;

                // Record Iteration
                let iteration_id = Uuid::new_v4().to_string();
                stdb.reducers.record_loop_iteration(
                    iteration_id,
                    self.loop_id.clone(),
                    current_turn,
                    ingress.raw_text.clone(),
                    output_summary,
                    0.5, // TODO: Real scoring logic
                    Some(run_id),
                    turn_cost,
                ).map_err(|e| anyhow!(e.to_string()))?;
            }

            let _ = status_tx.send(LoopStatus {
                loop_id: self.loop_id.clone(),
                current_turn,
                total_cost_usd: total_cost,
                status: "RUNNING".to_string(),
            }).await;

            if total_cost >= self.config.max_cost_usd {
                println!("Loop {} exceeded cost budget.", self.loop_id);
                break;
            }
        }

        // 5. Finalize Session in STDB
        if let Some(ref stdb) = self.stdb {
            stdb.reducers.complete_loop_session(
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
        }).await;

        println!("Loop {} finished.", self.loop_id);
        Ok(())
    }
}
