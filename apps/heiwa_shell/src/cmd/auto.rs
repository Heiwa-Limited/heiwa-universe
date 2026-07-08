use anyhow::{anyhow, Result};
use chrono::Utc;
use heiwa_automations::{
    Automation, AutomationId, AutomationScheduler, AutomationStatus, AutomationStore,
    ExternalSource, FileWatchEvent, FileWatchTriggerConfig, TriggerEventData,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("status") | None => status(args),
        Some("list") => list(args),
        Some("create") | Some("add") => create(&args[1..]),
        Some("trigger") | Some("queue") => trigger(&args[1..]),
        Some("tick") => tick(&args[1..]),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(anyhow!("unknown auto command: {other}")),
    }
}

fn status(args: &[String]) -> Result<()> {
    let payload = automations_payload();

    if has_flag(args, "--json") {
        println!("{}", payload);
        return Ok(());
    }

    let data = payload.get("data").unwrap_or(&payload);
    println!("auto status");
    println!(
        "  db: {}",
        data.get("db_path")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
    );
    println!(
        "  automations: {} (active: {})",
        data.get("automation_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        data.get("active_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    );
    if let Some(scheduler) = data.get("scheduler") {
        println!(
            "  cron jobs: {}",
            scheduler
                .get("active_cron")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        );
        println!(
            "  file watchers: {}",
            scheduler
                .get("active_file_watch")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        );
        if let Some(next) = scheduler.get("next_scheduled_at").and_then(|v| v.as_str()) {
            println!("  next: {next}");
        }
    }
    Ok(())
}

pub(crate) fn automations_payload() -> serde_json::Value {
    match automations_payload_result() {
        Ok(payload) => payload,
        Err(err) => json!({
            "command": "auto status",
            "error": format!("{err:#}"),
            "state_dir": state_dir().display().to_string(),
            "automation_count": 0,
            "active_count": 0,
            "scheduler": {
                "active_cron": 0,
                "active_file_watch": 0,
                "next_scheduled_at": null,
            },
            "automations": [],
            "recent_executions": [],
        }),
    }
}

fn automations_payload_result() -> Result<serde_json::Value> {
    let store = open_store()?;
    let scheduler = AutomationScheduler::new(store.clone());
    let scheduler_status = scheduler.status()?;
    let automations = store.list_automations()?;
    let active = automations
        .iter()
        .filter(|a| a.status == AutomationStatus::Active)
        .count();
    let mut recent_executions = Vec::new();
    for automation in &automations {
        let mut rows = store.list_executions(automation.id)?;
        rows.sort_by_key(|row| std::cmp::Reverse(row.created_at));
        recent_executions.extend(rows.into_iter().take(3).map(|execution| {
            json!({
                "id": execution.id,
                "automation_id": execution.automation_id,
                "status": execution.status,
                "created_at": execution.created_at,
                "started_at": execution.started_at,
                "completed_at": execution.completed_at,
                "error_message": execution.error_message,
            })
        }));
    }
    recent_executions.sort_by(|a, b| {
        b.get("created_at")
            .and_then(|v| v.as_str())
            .cmp(&a.get("created_at").and_then(|v| v.as_str()))
    });
    recent_executions.truncate(20);

    Ok(json!({
        "command": "auto status",
        "state_dir": state_dir().display().to_string(),
        "db_path": store.db_path().display().to_string(),
        "automation_count": automations.len(),
        "active_count": active,
        "scheduler": scheduler_status,
        "automations": automations,
        "recent_executions": recent_executions,
        "next": [
            "heiwa auto create --name 'Daily brief' --prompt 'Summarize today' --cron '0 9 * * *' --active",
            "heiwa auto tick --json"
        ],
    }))
}

fn list(args: &[String]) -> Result<()> {
    let store = open_store()?;
    let automations = store.list_automations()?;
    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "auto list",
                "automations": automations,
            })
        );
        return Ok(());
    }

    println!("automations");
    if automations.is_empty() {
        println!("  (none)");
        println!(
            "  create: heiwa auto create --name NAME --prompt PROMPT --cron '0 9 * * *' --active"
        );
        return Ok(());
    }
    for automation in automations {
        println!(
            "  - {}  {}  {:?}",
            automation.id, automation.name, automation.status
        );
        if let Some(trigger) = automation.trigger_config {
            println!("      trigger: {}", trigger_summary(&trigger));
        }
        if let Some(next) = automation.next_scheduled_at {
            println!("      next: {next}");
        }
    }
    Ok(())
}

fn create(args: &[String]) -> Result<()> {
    let name = flag_value(args, "--name").ok_or_else(|| anyhow!("missing --name"))?;
    let prompt = flag_value(args, "--prompt").ok_or_else(|| anyhow!("missing --prompt"))?;
    let mut automation = Automation::new(name, prompt);
    automation.max_executions_per_hour = flag_value(args, "--max-hour")
        .or_else(|| flag_value(args, "--max-executions-per-hour"))
        .map(|v| v.parse())
        .transpose()?;
    automation.max_executions_per_day = flag_value(args, "--max-day")
        .or_else(|| flag_value(args, "--max-executions-per-day"))
        .map(|v| v.parse())
        .transpose()?;

    if let Some(cron) = flag_value(args, "--cron") {
        let timezone = flag_value(args, "--timezone");
        automation = automation.with_cron_trigger(cron, timezone);
    } else if let Some(path) = flag_value(args, "--watch") {
        let events = flag_value(args, "--events")
            .unwrap_or_else(|| "create,modify".into())
            .split(',')
            .map(parse_file_event)
            .collect::<Result<Vec<_>>>()?;
        let mut config = FileWatchTriggerConfig::new(vec![path], events);
        config.pattern = flag_value(args, "--pattern");
        config.debounce_ms = flag_value(args, "--debounce-ms")
            .map(|v| v.parse())
            .transpose()?;
        automation = automation.with_file_watch_trigger(config);
    }

    if has_flag(args, "--active") {
        automation = automation.activate();
    }

    let store = open_store()?;
    store.upsert_automation(&automation)?;
    let scheduler = AutomationScheduler::new(store.clone());
    let init_events = scheduler.initialize_schedule_state(Utc::now())?;

    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "auto create",
                "automation": automation,
                "init_events": init_events,
                "db_path": store.db_path().display().to_string(),
            })
        );
    } else {
        println!("auto create");
        println!("  id: {}", automation.id);
        println!("  name: {}", automation.name);
        println!("  status: {:?}", automation.status);
        if let Some(trigger) = automation.trigger_config.as_ref() {
            println!("  trigger: {}", trigger_summary(trigger));
        }
        println!("  db: {}", store.db_path().display());
    }
    Ok(())
}

fn trigger(args: &[String]) -> Result<()> {
    let id_arg = args
        .first()
        .ok_or_else(|| anyhow!("usage: heiwa auto trigger <automation-id> [--json]"))?;
    let id = AutomationId::from_string(id_arg)?;
    let store = open_store()?;
    let scheduler = AutomationScheduler::new(store);
    let result = scheduler.executor().queue_execution(
        id,
        TriggerEventData::External {
            timestamp: Utc::now(),
            source: ExternalSource::Api,
            metadata: Some(json!({"command": "heiwa auto trigger"})),
        },
    )?;

    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "auto trigger",
                "result": result,
            })
        );
    } else if result.queued {
        println!("auto trigger");
        println!("  automation: {}", result.automation_id);
        if let Some(execution_id) = result.execution_id {
            println!("  execution: {execution_id}");
        }
    } else {
        println!("auto trigger skipped");
        println!("  automation: {}", result.automation_id);
        println!(
            "  reason: {}",
            result.reason.unwrap_or_else(|| "unknown".into())
        );
    }
    Ok(())
}

fn tick(args: &[String]) -> Result<()> {
    let store = open_store()?;
    let scheduler = AutomationScheduler::new(store.clone());
    let now = Utc::now();
    let mut events = scheduler.initialize_schedule_state(now)?;
    events.extend(scheduler.tick(now)?);

    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "auto tick",
                "at": now,
                "events": events,
                "db_path": store.db_path().display().to_string(),
            })
        );
        return Ok(());
    }

    println!("auto tick");
    println!("  at: {now}");
    if events.is_empty() {
        println!("  events: none");
    } else {
        for event in events {
            println!("  - {}", event_summary(&event));
        }
    }
    Ok(())
}

fn open_store() -> Result<AutomationStore> {
    ensure_state_dir()?;
    AutomationStore::open_state_dir(state_dir())
}

fn state_dir() -> PathBuf {
    let home = crate::home::heiwa_home().unwrap_or_else(|| PathBuf::from("."));
    home.join(".heiwa").join("state")
}

fn ensure_state_dir() -> Result<()> {
    fs::create_dir_all(state_dir())?;
    Ok(())
}

fn trigger_summary(trigger: &heiwa_automations::TriggerConfig) -> String {
    match trigger {
        heiwa_automations::TriggerConfig::Cron(config) => format!(
            "cron {}{}",
            config.schedule,
            config
                .timezone
                .as_ref()
                .map(|tz| format!(" ({tz})"))
                .unwrap_or_default()
        ),
        heiwa_automations::TriggerConfig::FileWatch(config) => format!(
            "file_watch paths={} events={:?} pattern={}",
            config.paths.join(","),
            config.events,
            config.pattern.as_deref().unwrap_or("*")
        ),
    }
}

fn event_summary(event: &heiwa_automations::SchedulerEvent) -> String {
    match event {
        heiwa_automations::SchedulerEvent::Queued {
            automation_id,
            queue_result,
            scheduled_time,
            next_scheduled_at,
        } => format!(
            "queued automation={} execution={:?} scheduled={} next={:?} reason={:?}",
            automation_id,
            queue_result.execution_id,
            scheduled_time,
            next_scheduled_at,
            queue_result.reason
        ),
        heiwa_automations::SchedulerEvent::Rescheduled {
            automation_id,
            next_scheduled_at,
        } => format!("rescheduled automation={automation_id} next={next_scheduled_at}"),
        heiwa_automations::SchedulerEvent::Skipped {
            automation_id,
            reason,
        } => format!("skipped automation={automation_id} reason={reason}"),
    }
}

fn parse_file_event(value: &str) -> Result<FileWatchEvent> {
    match value.trim() {
        "create" | "created" => Ok(FileWatchEvent::Create),
        "modify" | "modified" | "change" | "changed" => Ok(FileWatchEvent::Modify),
        "delete" | "deleted" | "remove" | "removed" => Ok(FileWatchEvent::Delete),
        other => Err(anyhow!("unknown file event: {other}")),
    }
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().cloned();
        }
        if let Some(rest) = arg.strip_prefix(&format!("{flag}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

fn print_help() {
    println!("heiwa auto");
    println!();
    println!("Usage:");
    println!("  heiwa auto status [--json]");
    println!("  heiwa auto list [--json]");
    println!("  heiwa auto create --name NAME --prompt PROMPT [--cron EXPR] [--timezone TZ] [--active] [--json]");
    println!("  heiwa auto create --name NAME --prompt PROMPT --watch PATH [--events create,modify] [--pattern GLOB] [--active]");
    println!("  heiwa auto trigger <automation-id> [--json]");
    println!("  heiwa auto tick [--json]");
    println!();
    println!("Stores local automations under ~/.heiwa/state/automations/.");
    println!("Cron uses five-field expressions by default, e.g. '0 9 * * *'.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_file_event_accepts_common_names() {
        assert_eq!(parse_file_event("create").unwrap(), FileWatchEvent::Create);
        assert_eq!(parse_file_event("changed").unwrap(), FileWatchEvent::Modify);
        assert_eq!(parse_file_event("removed").unwrap(), FileWatchEvent::Delete);
    }
}
