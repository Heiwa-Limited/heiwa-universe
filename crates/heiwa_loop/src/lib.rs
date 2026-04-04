use anyhow::Result;
use serde::{Deserialize, Serialize};
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
}

impl LoopController {
    pub fn new(config: LoopConfig) -> Self {
        Self {
            config,
            loop_id: Uuid::new_v4().to_string(),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn get_id(&self) -> String {
        self.loop_id.clone()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub async fn run(&self, status_tx: mpsc::Sender<LoopStatus>) -> Result<()> {
        println!("Starting loop {} with objective: {}", self.loop_id, self.config.objective);
        
        let mut current_turn = 0;
        let mut total_cost = 0.0;

        while current_turn < self.config.max_turns {
            if self.cancelled.load(Ordering::SeqCst) {
                println!("Loop {} cancelled.", self.loop_id);
                let _ = status_tx.send(LoopStatus {
                    loop_id: self.loop_id.clone(),
                    current_turn,
                    total_cost_usd: total_cost,
                    status: "CANCELLED".to_string(),
                }).await;
                return Ok(());
            }

            current_turn += 1;
            println!("Turn {}/{}...", current_turn, self.config.max_turns);

            // In a real implementation, this would:
            // 1. Call DREX to route the iteration
            // 2. Call the selected provider adapter
            // 3. Record the iteration in STDB
            // 4. Update the state for the next turn
            
            // Mocking execution for now
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            total_cost += 0.001;

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
            
            // Mock stop condition (e.g. LLM says it's done)
            if current_turn == self.config.max_turns {
                 println!("Loop {} reached max turns.", self.loop_id);
            }
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
