use crate::executor::{AutomationExecutor, QueueResult};
use crate::storage::AutomationStore;
use crate::types::{
    AutomationId, CronTriggerConfig, FileWatchEvent, FileWatchEventData, TriggerConfig,
    TriggerEventData,
};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use cron::Schedule;
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Event, EventKind};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Scheduler tick event. This is intentionally deterministic so launchd,
/// a long-lived daemon, or a UI-triggered tick can all use the same contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SchedulerEvent {
    Queued {
        automation_id: AutomationId,
        queue_result: QueueResult,
        scheduled_time: DateTime<Utc>,
        next_scheduled_at: Option<DateTime<Utc>>,
    },
    Rescheduled {
        automation_id: AutomationId,
        next_scheduled_at: DateTime<Utc>,
    },
    Skipped {
        automation_id: AutomationId,
        reason: String,
    },
}

/// Scheduler status summary for CLI/app read models.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchedulerStatus {
    pub active_cron: usize,
    pub active_file_watch: usize,
    pub next_scheduled_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct AutomationScheduler {
    store: AutomationStore,
    executor: AutomationExecutor,
}

impl AutomationScheduler {
    pub fn new(store: AutomationStore) -> Self {
        let executor = AutomationExecutor::new(store.clone());
        Self { store, executor }
    }

    pub fn store(&self) -> &AutomationStore {
        &self.store
    }

    pub fn executor(&self) -> &AutomationExecutor {
        &self.executor
    }

    /// Compute and persist missing next-run timestamps for active cron jobs.
    pub fn initialize_schedule_state(&self, now: DateTime<Utc>) -> Result<Vec<SchedulerEvent>> {
        let automations = self.store.list_active_with_triggers()?;
        let mut events = Vec::new();
        for automation in automations {
            let Some(TriggerConfig::Cron(config)) = automation.trigger_config.as_ref() else {
                continue;
            };
            if automation.next_scheduled_at.is_none() {
                match next_run_after(config, now) {
                    Ok(Some(next)) => {
                        self.store.mark_next_scheduled(automation.id, Some(next))?;
                        events.push(SchedulerEvent::Rescheduled {
                            automation_id: automation.id,
                            next_scheduled_at: next,
                        });
                    }
                    Ok(None) => events.push(SchedulerEvent::Skipped {
                        automation_id: automation.id,
                        reason: "no_future_run".into(),
                    }),
                    Err(err) => events.push(SchedulerEvent::Skipped {
                        automation_id: automation.id,
                        reason: format!("invalid_cron: {err:#}"),
                    }),
                }
            }
        }
        Ok(events)
    }

    /// One deterministic scheduler tick. Queues any due cron automations and
    /// advances their next-run timestamp. This is the MVP daemon primitive.
    pub fn tick(&self, now: DateTime<Utc>) -> Result<Vec<SchedulerEvent>> {
        let automations = self.store.list_active_with_triggers()?;
        let mut events = Vec::new();
        for automation in automations {
            match automation.trigger_config.as_ref() {
                Some(TriggerConfig::Cron(config)) => {
                    let due_at = automation
                        .next_scheduled_at
                        .or_else(|| next_run_after(config, now).ok().flatten());
                    let Some(due_at) = due_at else {
                        events.push(SchedulerEvent::Skipped {
                            automation_id: automation.id,
                            reason: "no_future_run".into(),
                        });
                        continue;
                    };

                    if due_at > now {
                        self.store
                            .mark_next_scheduled(automation.id, Some(due_at))?;
                        events.push(SchedulerEvent::Rescheduled {
                            automation_id: automation.id,
                            next_scheduled_at: due_at,
                        });
                        continue;
                    }

                    let trigger_data = TriggerEventData::Cron {
                        timestamp: now,
                        scheduled_time: due_at,
                    };
                    let queue_result =
                        self.executor.queue_execution(automation.id, trigger_data)?;
                    let next = next_run_after(config, now)?;
                    self.store.mark_next_scheduled(automation.id, next)?;
                    events.push(SchedulerEvent::Queued {
                        automation_id: automation.id,
                        queue_result,
                        scheduled_time: due_at,
                        next_scheduled_at: next,
                    });
                }
                Some(TriggerConfig::FileWatch(_)) => {
                    // File watching is event-driven; `queue_file_event` handles
                    // individual notify events. A tick only reports presence.
                }
                None => {}
            }
        }
        Ok(events)
    }

    /// Queue a normalized file watcher event. Long-lived watchers and app UI
    /// probes can both call this function.
    pub fn queue_file_event(
        &self,
        automation_id: AutomationId,
        file: FileWatchEventData,
        now: DateTime<Utc>,
    ) -> Result<QueueResult> {
        self.executor.queue_execution(
            automation_id,
            TriggerEventData::FileWatch {
                timestamp: now,
                file,
            },
        )
    }

    pub fn status(&self) -> Result<SchedulerStatus> {
        let automations = self.store.list_active_with_triggers()?;
        let mut active_cron = 0;
        let mut active_file_watch = 0;
        let mut next_scheduled_at = None;
        for automation in automations {
            match automation.trigger_config.as_ref() {
                Some(TriggerConfig::Cron(_)) => {
                    active_cron += 1;
                    if let Some(next) = automation.next_scheduled_at {
                        if next_scheduled_at
                            .map(|existing| next < existing)
                            .unwrap_or(true)
                        {
                            next_scheduled_at = Some(next);
                        }
                    }
                }
                Some(TriggerConfig::FileWatch(_)) => active_file_watch += 1,
                None => {}
            }
        }
        Ok(SchedulerStatus {
            active_cron,
            active_file_watch,
            next_scheduled_at,
        })
    }
}

/// Compute the next UTC run for a cron config after a point in time.
///
/// Five-field expressions are accepted (`m h dom mon dow`) and normalized to the
/// seconds-aware format expected by the Rust `cron` crate.
pub fn next_run_after(
    config: &CronTriggerConfig,
    after: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    let expr = normalize_cron_expression(&config.schedule)?;
    let schedule = Schedule::from_str(&expr)
        .with_context(|| format!("parse cron expression {}", config.schedule))?;
    let tz = config
        .timezone
        .as_deref()
        .unwrap_or("UTC")
        .parse::<chrono_tz::Tz>()
        .with_context(|| format!("parse timezone {:?}", config.timezone))?;
    let after_tz = after.with_timezone(&tz);
    Ok(schedule
        .after(&after_tz)
        .next()
        .map(|dt| dt.with_timezone(&Utc)))
}

pub fn normalize_cron_expression(expr: &str) -> Result<String> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    match parts.len() {
        // minute hour day-of-month month day-of-week
        5 => Ok(format!("0 {} *", parts.join(" "))),
        // second minute hour day-of-month month day-of-week
        6 => Ok(format!("{} *", parts.join(" "))),
        // second minute hour day-of-month month day-of-week year
        7 => Ok(parts.join(" ")),
        n => Err(anyhow!("expected 5, 6, or 7 cron fields, got {n}: {expr}")),
    }
}

/// Map a `notify` event to Heiwa's stable file-watch event vocabulary.
pub fn map_notify_event(event: &Event) -> Option<(FileWatchEvent, Vec<String>)> {
    let kind = match &event.kind {
        EventKind::Create(CreateKind::Any | CreateKind::File | CreateKind::Folder) => {
            FileWatchEvent::Create
        }
        EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_) | ModifyKind::Metadata(_)) => {
            FileWatchEvent::Modify
        }
        EventKind::Remove(RemoveKind::Any | RemoveKind::File | RemoveKind::Folder) => {
            FileWatchEvent::Delete
        }
        _ => return None,
    };
    Some((
        kind,
        event
            .paths
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Automation, FileWatchTriggerConfig};
    use chrono::{TimeZone, Utc};

    #[test]
    fn five_field_cron_normalizes_to_seconds_aware_expression() {
        assert_eq!(
            normalize_cron_expression("0 9 * * 1").unwrap(),
            "0 0 9 * * 1 *"
        );
    }

    #[test]
    fn next_run_after_handles_five_field_cron() {
        let config = CronTriggerConfig {
            schedule: "0 9 * * *".into(),
            timezone: Some("UTC".into()),
        };
        let after = Utc.with_ymd_and_hms(2026, 6, 12, 8, 0, 0).unwrap();
        let next = next_run_after(&config, after).unwrap().unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 6, 12, 9, 0, 0).unwrap());
    }

    #[test]
    fn tick_queues_due_cron_and_advances_next_run() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open_state_dir(tmp.path()).unwrap();
        let mut automation = Automation::new("brief".into(), "summarize".into())
            .with_cron_trigger("0 9 * * *".into(), Some("UTC".into()))
            .activate();
        automation.next_scheduled_at = Some(Utc.with_ymd_and_hms(2026, 6, 12, 9, 0, 0).unwrap());
        store.upsert_automation(&automation).unwrap();

        let scheduler = AutomationScheduler::new(store.clone());
        let events = scheduler
            .tick(Utc.with_ymd_and_hms(2026, 6, 12, 9, 1, 0).unwrap())
            .unwrap();
        assert!(matches!(events[0], SchedulerEvent::Queued { .. }));
        assert_eq!(store.list_executions(automation.id).unwrap().len(), 1);
        let updated = store.get_automation(automation.id).unwrap().unwrap();
        assert!(
            updated.next_scheduled_at.unwrap()
                > Utc.with_ymd_and_hms(2026, 6, 12, 9, 1, 0).unwrap()
        );
    }

    #[test]
    fn status_counts_cron_and_file_watch_triggers() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open_state_dir(tmp.path()).unwrap();
        let cron = Automation::new("cron".into(), "do cron".into())
            .with_cron_trigger("0 9 * * *".into(), None)
            .activate();
        let watch = Automation::new("watch".into(), "watch files".into())
            .with_file_watch_trigger(FileWatchTriggerConfig::new(
                vec!["~/Downloads".into()],
                vec![FileWatchEvent::Create],
            ))
            .activate();
        store.upsert_automation(&cron).unwrap();
        store.upsert_automation(&watch).unwrap();

        let status = AutomationScheduler::new(store).status().unwrap();
        assert_eq!(status.active_cron, 1);
        assert_eq!(status.active_file_watch, 1);
    }
}
