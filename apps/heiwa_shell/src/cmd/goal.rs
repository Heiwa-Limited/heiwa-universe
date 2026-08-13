use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: &str = "heiwa_goal_v1";
const DEFAULT_BUDGET_TURNS: u32 = 20;
const DEFAULT_JUDGE_MODEL: &str = "ollama/qwen3.5:9b";

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("start") => start(&args[1..]),
        Some("list") => list(&args[1..]),
        Some("show") => show(&args[1..]),
        Some("step") => step(&args[1..]),
        Some("complete") => decide(&args[1..], "complete"),
        Some("abandon") => decide(&args[1..], "abandoned"),
        Some("--help") | Some("-h") | None => {
            print_help();
            Ok(())
        }
        Some(other) => Err(anyhow!("unknown goal command: {other}")),
    }
}

fn start(args: &[String]) -> Result<()> {
    let id = args
        .first()
        .filter(|arg| !arg.starts_with("--"))
        .cloned()
        .ok_or_else(|| anyhow!("usage: heiwa goal start <id> --title \"...\" --description \"...\" [--budget N] [--judge model] [--executor model]"))?;
    let title =
        flag_value(args, "--title").ok_or_else(|| anyhow!("--title \"...\" is required"))?;
    let description = flag_value(args, "--description").unwrap_or_default();
    let budget = flag_value(args, "--budget")
        .as_deref()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(DEFAULT_BUDGET_TURNS);
    let judge_model =
        flag_value(args, "--judge").unwrap_or_else(|| DEFAULT_JUDGE_MODEL.to_string());
    let executor_model = flag_value(args, "--executor");

    let path = goals_dir().join(format!("{id}.json"));
    if path.exists() {
        return Err(anyhow!("goal already exists: {}", path.display()));
    }
    let now = Utc::now().to_rfc3339();
    let record = json!({
        "schema_version": SCHEMA_VERSION,
        "id": id,
        "title": title,
        "description": description,
        "status": "open",
        "budget_turns": budget,
        "executor_model": executor_model,
        "judge_model": judge_model,
        "created_at_utc": now,
        "updated_at_utc": now,
        "steps": [],
        "outcome": null,
    });
    fs::create_dir_all(goals_dir())?;
    fs::write(&path, serde_json::to_string_pretty(&record)?)?;

    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "goal start",
                "id": id,
                "path": path.display().to_string(),
                "goal": record,
            })
        );
    } else {
        println!("goal {id} started");
        println!("  title: {title}");
        println!("  budget: {budget} turns");
        println!("  judge: {judge_model}");
        println!("  wrote: {}", path.display());
    }
    Ok(())
}

fn list(args: &[String]) -> Result<()> {
    let goals = scan_goals(&goals_dir());
    let counts = count_by_status(&goals);
    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "goal list",
                "goals_dir": goals_dir().display().to_string(),
                "counts": {
                    "open": counts.open,
                    "complete": counts.complete,
                    "abandoned": counts.abandoned,
                },
                "goals": goals,
            })
        );
        return Ok(());
    }
    println!(
        "goals  open={}  complete={}  abandoned={}",
        counts.open, counts.complete, counts.abandoned
    );
    println!("  dir: {}", goals_dir().display());
    for goal in &goals {
        let id = goal.get("id").and_then(Value::as_str).unwrap_or("?");
        let title = goal.get("title").and_then(Value::as_str).unwrap_or("");
        let status = goal.get("status").and_then(Value::as_str).unwrap_or("?");
        let steps = goal
            .get("steps")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        println!("  {id:24}  [{status:9}]  steps={steps}  {title}");
    }
    Ok(())
}

fn show(args: &[String]) -> Result<()> {
    let id = args
        .first()
        .filter(|arg| !arg.starts_with("--"))
        .ok_or_else(|| anyhow!("usage: heiwa goal show <id> [--json]"))?;
    let path = goals_dir().join(format!("{id}.json"));
    if !path.exists() {
        return Err(anyhow!("goal not found: {}", path.display()));
    }
    let raw = fs::read_to_string(&path)?;
    let value: Value = serde_json::from_str(&raw)?;
    if has_flag(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("goal {id}");
        println!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}

fn step(args: &[String]) -> Result<()> {
    let id = args
        .first()
        .filter(|arg| !arg.starts_with("--"))
        .ok_or_else(|| {
            anyhow!("usage: heiwa goal step <id> --note \"...\" [--kind note|checkpoint|blocker]")
        })?;
    let note = flag_value(args, "--note").ok_or_else(|| anyhow!("--note \"...\" is required"))?;
    let kind = flag_value(args, "--kind").unwrap_or_else(|| "note".to_string());

    let path = goals_dir().join(format!("{id}.json"));
    if !path.exists() {
        return Err(anyhow!("goal not found: {}", path.display()));
    }
    let raw = fs::read_to_string(&path)?;
    let mut value: Value = serde_json::from_str(&raw)?;
    let now = Utc::now().to_rfc3339();
    let entry = json!({ "ts_utc": now, "kind": kind, "note": note });
    if let Some(steps) = value.get_mut("steps").and_then(Value::as_array_mut) {
        steps.push(entry.clone());
    } else {
        return Err(anyhow!("goal record missing steps array"));
    }
    if let Some(obj) = value.as_object_mut() {
        obj.insert("updated_at_utc".to_string(), Value::String(now));
    }
    fs::write(&path, serde_json::to_string_pretty(&value)?)?;

    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({ "command": "goal step", "id": id, "step": entry })
        );
    } else {
        println!("goal {id} step appended  kind={kind}");
    }
    Ok(())
}

fn decide(args: &[String], outcome: &str) -> Result<()> {
    let id = args
        .first()
        .filter(|arg| !arg.starts_with("--"))
        .ok_or_else(|| anyhow!("usage: heiwa goal {outcome} <id> [--reason \"...\"]"))?;
    let reason = flag_value(args, "--reason").unwrap_or_default();
    let path = goals_dir().join(format!("{id}.json"));
    if !path.exists() {
        return Err(anyhow!("goal not found: {}", path.display()));
    }
    let raw = fs::read_to_string(&path)?;
    let mut value: Value = serde_json::from_str(&raw)?;
    let now = Utc::now().to_rfc3339();
    let status = if outcome == "complete" {
        "complete"
    } else {
        "abandoned"
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert("status".to_string(), Value::String(status.to_string()));
        obj.insert("updated_at_utc".to_string(), Value::String(now.clone()));
        obj.insert(
            "outcome".to_string(),
            json!({ "outcome": status, "reason": reason, "decided_at_utc": now }),
        );
    }
    fs::write(&path, serde_json::to_string_pretty(&value)?)?;

    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({ "command": format!("goal {outcome}"), "id": id, "status": status })
        );
    } else {
        println!("goal {id} -> {status}");
    }
    Ok(())
}

pub(crate) fn goals_payload() -> Value {
    let dir = goals_dir();
    let goals = scan_goals(&dir);
    let counts = count_by_status(&goals);
    json!({
        "goals_dir": dir.display().to_string(),
        "counts": {
            "open": counts.open,
            "complete": counts.complete,
            "abandoned": counts.abandoned,
        },
        "goals": goals,
    })
}

#[derive(Default, Debug, PartialEq)]
struct StatusCounts {
    open: usize,
    complete: usize,
    abandoned: usize,
}

fn count_by_status(goals: &[Value]) -> StatusCounts {
    let mut c = StatusCounts::default();
    for goal in goals {
        match goal.get("status").and_then(Value::as_str) {
            Some("open") => c.open += 1,
            Some("complete") => c.complete += 1,
            Some("abandoned") => c.abandoned += 1,
            _ => {}
        }
    }
    c
}

pub(crate) fn scan_goals(dir: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path).unwrap_or_default();
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            out.push(value);
        }
    }
    out.sort_by(|a, b| {
        let ta = a
            .get("created_at_utc")
            .and_then(Value::as_str)
            .unwrap_or("");
        let tb = b
            .get("created_at_utc")
            .and_then(Value::as_str)
            .unwrap_or("");
        ta.cmp(tb)
    });
    out
}

pub(crate) fn goals_dir() -> PathBuf {
    crate::home::heiwa_state_dir().join("goals")
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
    }
    None
}

fn print_help() {
    println!("heiwa goal");
    println!();
    println!("Usage:");
    println!("  heiwa goal start <id> --title \"...\" [--description \"...\"] [--budget N] [--judge model] [--executor model] [--json]");
    println!("  heiwa goal list [--json]");
    println!("  heiwa goal show <id> [--json]");
    println!("  heiwa goal step <id> --note \"...\" [--kind note|checkpoint|blocker] [--json]");
    println!("  heiwa goal complete <id> [--reason \"...\"] [--json]");
    println!("  heiwa goal abandon <id> [--reason \"...\"] [--json]");
    println!();
    println!("Goals persist in ~/.heiwa/state/goals/<id>.json (heiwa_goal_v1).");
    println!("Judge model defaults to local Ollama; executor model is unset until route policy wires it.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_goals_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = env::temp_dir().join(format!("heiwa-goal-test-{}-{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp goals dir");
        dir
    }

    #[test]
    fn scan_goals_returns_records_sorted_by_created_at() {
        let dir = temp_goals_dir();
        fs::write(
            dir.join("g_b.json"),
            json!({
                "schema_version": SCHEMA_VERSION,
                "id": "g_b",
                "title": "B",
                "status": "open",
                "created_at_utc": "2026-05-26T11:00:00Z",
                "steps": [],
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            dir.join("g_a.json"),
            json!({
                "schema_version": SCHEMA_VERSION,
                "id": "g_a",
                "title": "A",
                "status": "open",
                "created_at_utc": "2026-05-26T10:00:00Z",
                "steps": [],
            })
            .to_string(),
        )
        .unwrap();

        let goals = scan_goals(&dir);
        assert_eq!(goals.len(), 2);
        assert_eq!(goals[0].get("id").and_then(Value::as_str), Some("g_a"));
        assert_eq!(goals[1].get("id").and_then(Value::as_str), Some("g_b"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn count_by_status_partitions_correctly() {
        let goals = vec![
            json!({"status": "open"}),
            json!({"status": "open"}),
            json!({"status": "complete"}),
            json!({"status": "abandoned"}),
            json!({"status": "weird"}),
        ];
        let counts = count_by_status(&goals);
        assert_eq!(
            counts,
            StatusCounts {
                open: 2,
                complete: 1,
                abandoned: 1
            }
        );
    }
}
