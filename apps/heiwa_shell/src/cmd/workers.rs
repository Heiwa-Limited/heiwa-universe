use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::PathBuf;

const WORKERS_FILE: &str = "workers.json";
const HEARTBEAT_TTL_SECS: i64 = 120;

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("heartbeat") => heartbeat(&args[1..]),
        Some("status") | None => status(args),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(anyhow!("unknown workers command: {other}")),
    }
}

fn heartbeat(args: &[String]) -> Result<()> {
    let dry_run = has_flag(args, "--dry-run");
    let class = flag_value(args, "--class").unwrap_or_else(|| "shell_machine".to_string());
    let worker_id =
        flag_value(args, "--id").unwrap_or_else(|| format!("local-{}", hostname_string()));
    let mut workers = load_or_empty();
    let now = Utc::now().to_rfc3339();
    let entry = json!({
        "worker_id": worker_id,
        "class": class,
        "node": hostname_string(),
        "last_heartbeat_utc": now,
        "ttl_seconds": HEARTBEAT_TTL_SECS,
        "transport": "local-file",
    });
    upsert_worker(&mut workers, &worker_id, entry.clone());

    if dry_run {
        if has_flag(args, "--json") {
            println!(
                "{}",
                json!({
                    "command": "workers heartbeat",
                    "dry_run": true,
                    "worker": entry,
                    "path": workers_path().display().to_string(),
                })
            );
        } else {
            println!("workers heartbeat (dry-run)");
            println!("  worker: {worker_id}");
            println!("  class: {class}");
            println!("  path: {}", workers_path().display());
        }
        return Ok(());
    }

    ensure_state_dir()?;
    fs::write(workers_path(), serde_json::to_string_pretty(&workers)?)?;
    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "workers heartbeat",
                "dry_run": false,
                "worker": entry,
                "path": workers_path().display().to_string(),
            })
        );
    } else {
        println!("workers heartbeat");
        println!("  worker: {worker_id} (class: {class})");
        println!("  at: {now}");
        println!("  path: {}", workers_path().display());
    }
    Ok(())
}

fn status(args: &[String]) -> Result<()> {
    let workers = load_or_empty();
    let entries = workers
        .get("workers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let now = Utc::now().timestamp();
    let mut live = 0usize;
    let mut stale = 0usize;
    let mut rows = Vec::new();
    for entry in &entries {
        let last = entry
            .get("last_heartbeat_utc")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp())
            .unwrap_or(0);
        let age = now - last;
        let ttl = entry
            .get("ttl_seconds")
            .and_then(Value::as_i64)
            .unwrap_or(HEARTBEAT_TTL_SECS);
        let is_live = age <= ttl;
        if is_live {
            live += 1;
        } else {
            stale += 1;
        }
        rows.push(json!({
            "worker_id": entry.get("worker_id"),
            "class": entry.get("class"),
            "node": entry.get("node"),
            "age_seconds": age,
            "live": is_live,
        }));
    }

    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "workers status",
                "path": workers_path().display().to_string(),
                "live": live,
                "stale": stale,
                "workers": rows,
            })
        );
        return Ok(());
    }
    println!("workers status");
    println!("  path: {}", workers_path().display());
    println!("  live: {live}  stale: {stale}");
    for row in rows {
        println!(
            "    {} [{}] age={}s live={}",
            row.get("worker_id").and_then(Value::as_str).unwrap_or("?"),
            row.get("class").and_then(Value::as_str).unwrap_or("?"),
            row.get("age_seconds").and_then(Value::as_i64).unwrap_or(0),
            row.get("live").and_then(Value::as_bool).unwrap_or(false)
        );
    }
    Ok(())
}

fn upsert_worker(workers: &mut Value, worker_id: &str, entry: Value) {
    let arr = workers.as_object_mut().and_then(|obj| {
        obj.entry("workers")
            .or_insert(Value::Array(Vec::new()))
            .as_array_mut()
    });
    if let Some(arr) = arr {
        if let Some(idx) = arr
            .iter()
            .position(|w| w.get("worker_id").and_then(Value::as_str) == Some(worker_id))
        {
            arr[idx] = entry;
        } else {
            arr.push(entry);
        }
    }
}

fn workers_path() -> PathBuf {
    state_dir().join(WORKERS_FILE)
}

fn state_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".heiwa").join("state")
}

fn ensure_state_dir() -> Result<()> {
    fs::create_dir_all(state_dir())?;
    Ok(())
}

fn load_or_empty() -> Value {
    fs::read_to_string(workers_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({"workers": []}))
}

fn hostname_string() -> String {
    hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .unwrap_or_else(|| "unknown-host".to_string())
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
    let _ = env::var("HEIWA_WORKER_ID").ok();
    None
}

fn print_help() {
    println!("heiwa workers");
    println!();
    println!("Usage:");
    println!("  heiwa workers heartbeat [--class CLASS] [--id ID] [--dry-run] [--json]");
    println!("  heiwa workers status [--json]");
    println!();
    println!("Records local worker liveness under ~/.heiwa/state/workers.json.");
    println!("Heartbeat TTL is {HEARTBEAT_TTL_SECS} seconds.");
}
