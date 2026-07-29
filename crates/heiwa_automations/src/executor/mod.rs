use crate::storage::AutomationStore;
use crate::types::{
    Automation, AutomationId, Execution, ExecutionId, ExecutionStatus, TriggerEventData,
};
use anyhow::{anyhow, Context, Result};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::future::Future;

/// Result of attempting to queue an execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueueResult {
    pub queued: bool,
    pub execution_id: Option<ExecutionId>,
    pub automation_id: AutomationId,
    pub reason: Option<String>,
}

/// Outcome produced by a runtime-specific automation runner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ExecutionOutcome {
    Completed {
        summary: String,
        output: Option<serde_json::Value>,
    },
    AwaitingConfirmation {
        request_id: String,
        summary: String,
    },
    Failed {
        message: String,
    },
}

/// Runtime hook for the actual work. The automation crate owns durable queueing
/// and evidence; the shell/runtime supplies a runner that routes through DREX.
pub trait ExecutionRunner: Send + Sync {
    fn run(
        &self,
        automation: &Automation,
        execution: &Execution,
    ) -> impl Future<Output = Result<ExecutionOutcome>> + Send;
}

/// Durable local executor: rate-limit checks, queue creation, status changes,
/// and receipts. It does not own model/provider internals.
#[derive(Clone)]
pub struct AutomationExecutor {
    store: AutomationStore,
}

impl AutomationExecutor {
    pub fn new(store: AutomationStore) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &AutomationStore {
        &self.store
    }

    pub fn queue_execution(
        &self,
        automation_id: AutomationId,
        trigger_data: TriggerEventData,
    ) -> Result<QueueResult> {
        let Some(automation) = self.store.get_automation(automation_id)? else {
            return Ok(QueueResult {
                queued: false,
                execution_id: None,
                automation_id,
                reason: Some("automation_not_found".into()),
            });
        };

        if automation.status != crate::types::AutomationStatus::Active {
            return Ok(QueueResult {
                queued: false,
                execution_id: None,
                automation_id,
                reason: Some("automation_not_active".into()),
            });
        }

        if !self.within_rate_limits(&automation)? {
            return Ok(QueueResult {
                queued: false,
                execution_id: None,
                automation_id,
                reason: Some("rate_limited".into()),
            });
        }

        let now = Utc::now();
        let execution = Execution {
            id: ExecutionId::new(),
            automation_id,
            status: ExecutionStatus::Pending,
            trigger_data: Some(trigger_data),
            started_at: None,
            completed_at: None,
            error_message: None,
            retry_count: 0,
            created_at: now,
        };
        self.store.insert_execution(&execution)?;
        self.write_receipt(&execution, "queued", json!({"reason": "triggered"}))?;
        Ok(QueueResult {
            queued: true,
            execution_id: Some(execution.id),
            automation_id,
            reason: None,
        })
    }

    pub fn start_execution(&self, execution_id: ExecutionId) -> Result<Execution> {
        self.claim_execution(execution_id)?
            .ok_or_else(|| anyhow!("execution {execution_id} is not pending"))
    }

    pub fn claim_execution(&self, execution_id: ExecutionId) -> Result<Option<Execution>> {
        let execution = self.store.claim_pending_execution(execution_id)?;
        if let Some(execution) = execution.as_ref() {
            self.write_receipt(execution, "started", json!({}))?;
        }
        Ok(execution)
    }

    pub fn complete_execution(
        &self,
        execution_id: ExecutionId,
        outcome: ExecutionOutcome,
    ) -> Result<Execution> {
        let mut execution = self
            .store
            .get_execution(execution_id)?
            .ok_or_else(|| anyhow!("execution {execution_id} not found"))?;
        execution.completed_at = Some(Utc::now());
        execution.status = match outcome {
            ExecutionOutcome::Completed { .. } => ExecutionStatus::Completed,
            ExecutionOutcome::AwaitingConfirmation { .. } => ExecutionStatus::AwaitingConfirmation,
            ExecutionOutcome::Failed { ref message } => {
                execution.error_message = Some(message.clone());
                ExecutionStatus::Failed
            }
        };
        self.store.update_execution(&execution)?;
        self.store
            .mark_last_executed(execution.automation_id, execution.completed_at.unwrap())?;
        self.write_receipt(&execution, "completed", serde_json::to_value(outcome)?)?;
        Ok(execution)
    }

    pub fn cancel_execution(&self, execution_id: ExecutionId, reason: &str) -> Result<Execution> {
        let mut execution = self
            .store
            .get_execution(execution_id)?
            .ok_or_else(|| anyhow!("execution {execution_id} not found"))?;
        execution.status = ExecutionStatus::Cancelled;
        execution.error_message = Some(reason.to_string());
        execution.completed_at = Some(Utc::now());
        self.store.update_execution(&execution)?;
        self.write_receipt(&execution, "cancelled", json!({"reason": reason}))?;
        Ok(execution)
    }

    /// Queue and run immediately with a runtime-provided runner. This is used by
    /// command-line/manual execution; daemon schedulers can queue first and let a
    /// worker process drain separately.
    pub async fn run_now<R: ExecutionRunner>(
        &self,
        automation_id: AutomationId,
        trigger_data: TriggerEventData,
        runner: &R,
    ) -> Result<Execution> {
        let queue = self.queue_execution(automation_id, trigger_data)?;
        let Some(execution_id) = queue.execution_id else {
            return Err(anyhow!(
                "automation {} was not queued: {}",
                queue.automation_id,
                queue.reason.unwrap_or_else(|| "unknown".into())
            ));
        };
        let execution = self
            .claim_execution(execution_id)?
            .ok_or_else(|| anyhow!("execution {execution_id} was claimed by another runner"))?;
        let automation = self
            .store
            .get_automation(execution.automation_id)?
            .ok_or_else(|| anyhow!("automation {} not found", execution.automation_id))?;
        let outcome = runner
            .run(&automation, &execution)
            .await
            .unwrap_or_else(|err| ExecutionOutcome::Failed {
                message: format!("{err:#}"),
            });
        self.complete_execution(execution_id, outcome)
    }

    /// Claim and run pending work in creation order.
    ///
    /// Claims are atomic in SQLite, so concurrent app/CLI pumps cannot execute
    /// the same row twice. Execution is sequential in this bounded slice;
    /// concurrency belongs in runtime policy, not storage.
    pub async fn run_pending<R: ExecutionRunner>(
        &self,
        runner: &R,
        limit: usize,
    ) -> Result<Vec<Execution>> {
        let pending = self.store.list_pending_executions(limit)?;
        let mut completed = Vec::new();
        for row in pending {
            let Some(execution) = self.claim_execution(row.id)? else {
                continue;
            };
            let automation = self
                .store
                .get_automation(execution.automation_id)?
                .ok_or_else(|| anyhow!("automation {} not found", execution.automation_id))?;
            let outcome = runner
                .run(&automation, &execution)
                .await
                .unwrap_or_else(|err| ExecutionOutcome::Failed {
                    message: format!("{err:#}"),
                });
            completed.push(self.complete_execution(execution.id, outcome)?);
        }
        Ok(completed)
    }

    fn within_rate_limits(&self, automation: &Automation) -> Result<bool> {
        if let Some(limit) = automation.max_executions_per_hour {
            let count = self
                .store
                .count_executions_since(automation.id, Utc::now() - Duration::hours(1))?;
            if count >= limit {
                return Ok(false);
            }
        }
        if let Some(limit) = automation.max_executions_per_day {
            let count = self
                .store
                .count_executions_since(automation.id, Utc::now() - Duration::days(1))?;
            if count >= limit {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn write_receipt(
        &self,
        execution: &Execution,
        event: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        let receipts_dir = self.store.state_dir().join("automations/receipts");
        fs::create_dir_all(&receipts_dir)
            .with_context(|| format!("create receipts dir {}", receipts_dir.display()))?;
        let path = receipts_dir.join(format!("rcpt-{}-{event}.json", execution.id));
        let receipt = json!({
            "kind": "automation_execution_event",
            "event": event,
            "execution_id": execution.id.to_string(),
            "automation_id": execution.automation_id.to_string(),
            "status": execution.status,
            "created_at": Utc::now().to_rfc3339(),
            "payload": payload,
        });
        fs::write(&path, serde_json::to_vec_pretty(&receipt)?)
            .with_context(|| format!("write receipt {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Automation, TriggerEventData};

    struct EchoRunner;
    impl ExecutionRunner for EchoRunner {
        async fn run(
            &self,
            automation: &Automation,
            _execution: &Execution,
        ) -> Result<ExecutionOutcome> {
            Ok(ExecutionOutcome::Completed {
                summary: format!("ran {}", automation.name),
                output: None,
            })
        }
    }

    #[tokio::test]
    async fn executor_queues_runs_and_writes_receipts() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open_state_dir(tmp.path()).unwrap();
        let automation = Automation::new("heartbeat".into(), "check status".into()).activate();
        store.upsert_automation(&automation).unwrap();
        let executor = AutomationExecutor::new(store.clone());

        let execution = executor
            .run_now(
                automation.id,
                TriggerEventData::External {
                    timestamp: Utc::now(),
                    source: crate::types::ExternalSource::Api,
                    metadata: None,
                },
                &EchoRunner,
            )
            .await
            .unwrap();

        assert_eq!(execution.status, ExecutionStatus::Completed);
        assert!(tmp
            .path()
            .join(format!(
                "automations/receipts/rcpt-{}-completed.json",
                execution.id
            ))
            .exists());
    }

    #[tokio::test]
    async fn pending_executions_are_drained_through_async_runner() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open_state_dir(tmp.path()).unwrap();
        let automation = Automation::new("brief".into(), "summarize".into()).activate();
        store.upsert_automation(&automation).unwrap();
        let executor = AutomationExecutor::new(store.clone());
        executor
            .queue_execution(
                automation.id,
                TriggerEventData::External {
                    timestamp: Utc::now(),
                    source: crate::types::ExternalSource::Api,
                    metadata: None,
                },
            )
            .unwrap();

        let completed = executor.run_pending(&EchoRunner, 10).await.unwrap();

        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].status, ExecutionStatus::Completed);
    }

    #[test]
    fn pending_execution_can_only_be_claimed_once() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open_state_dir(tmp.path()).unwrap();
        let automation = Automation::new("single".into(), "once".into()).activate();
        store.upsert_automation(&automation).unwrap();
        let executor = AutomationExecutor::new(store);
        let queued = executor
            .queue_execution(
                automation.id,
                TriggerEventData::External {
                    timestamp: Utc::now(),
                    source: crate::types::ExternalSource::Api,
                    metadata: None,
                },
            )
            .unwrap();
        let execution_id = queued.execution_id.unwrap();

        assert!(executor.claim_execution(execution_id).unwrap().is_some());
        assert!(executor.claim_execution(execution_id).unwrap().is_none());
    }

    #[test]
    fn executor_respects_hourly_rate_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open_state_dir(tmp.path()).unwrap();
        let mut automation = Automation::new("limited".into(), "once".into()).activate();
        automation.max_executions_per_hour = Some(1);
        store.upsert_automation(&automation).unwrap();
        let executor = AutomationExecutor::new(store);
        let trigger = TriggerEventData::External {
            timestamp: Utc::now(),
            source: crate::types::ExternalSource::Api,
            metadata: None,
        };

        assert!(
            executor
                .queue_execution(automation.id, trigger.clone())
                .unwrap()
                .queued
        );
        let denied = executor.queue_execution(automation.id, trigger).unwrap();
        assert!(!denied.queued);
        assert_eq!(denied.reason.as_deref(), Some("rate_limited"));
    }
}
