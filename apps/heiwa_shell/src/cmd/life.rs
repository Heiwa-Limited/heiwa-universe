use anyhow::{anyhow, Result};
use serde_json::json;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
struct SourceProbe {
    group: &'static str,
    label: String,
    path: PathBuf,
    present: bool,
    modified_unix: Option<u64>,
}

#[derive(Clone, Debug)]
struct SourceGroupSummary {
    group: &'static str,
    configured: usize,
    present: usize,
    missing: usize,
    latest_modified_unix: Option<u64>,
}

#[derive(Clone, Debug)]
struct ImportPlan {
    target: String,
    dry_run: bool,
    stdb_mode: String,
    row_counts: Vec<RowCount>,
}

#[derive(Clone, Debug)]
struct RowCount {
    table: &'static str,
    rows: usize,
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("status") => status(args),
        Some("today") => today(args),
        Some("freshness") => freshness(args),
        Some("approvals") => approvals(args),
        Some("import") => import(args),
        Some("--help") | Some("-h") | None => {
            print_help();
            Ok(())
        }
        Some(other) => Err(anyhow!("unknown life command: {other}")),
    }
}

fn status(args: &[String]) -> Result<()> {
    let json_output = has_flag(args, "--json");
    let groups = source_group_summaries(&all_source_probes());
    let stdb_mode = stdb_mode();

    if json_output {
        let groups_json = groups
            .iter()
            .map(|group| {
                json!({
                    "group": group.group,
                    "configured": group.configured,
                    "present": group.present,
                    "missing": group.missing,
                    "latest_modified_unix": group.latest_modified_unix,
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            json!({
                "command": "life status",
                "stdb_mode": stdb_mode,
                "sources": groups_json,
                "next": [
                    "heiwa life import home --dry-run",
                    "heiwa life import claude --dry-run",
                    "heiwa life import codex --dry-run"
                ]
            })
        );
        return Ok(());
    }

    println!("life status");
    println!("  stdb: {stdb_mode}");
    println!("  sources:");
    for group in groups {
        println!(
            "    {}: {}/{} present, {} missing",
            group.group, group.present, group.configured, group.missing
        );
    }
    println!("  next: heiwa life import home|claude|codex --dry-run");
    Ok(())
}

fn today(args: &[String]) -> Result<()> {
    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "life today",
                "status": "not_materialized",
                "next": "heiwa life import home --dry-run"
            })
        );
        return Ok(());
    }
    println!("life today");
    println!("  status: not materialized yet");
    println!("  next: heiwa life import home --dry-run");
    Ok(())
}

fn freshness(args: &[String]) -> Result<()> {
    let probes = all_source_probes();
    if has_flag(args, "--json") {
        let sources = probes
            .iter()
            .map(|probe| {
                json!({
                    "group": probe.group,
                    "label": probe.label,
                    "path": probe.path.display().to_string(),
                    "present": probe.present,
                    "modified_unix": probe.modified_unix,
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            json!({
                "command": "life freshness",
                "sources": sources
            })
        );
        return Ok(());
    }

    println!("life freshness");
    for probe in probes {
        let status = if probe.present { "present" } else { "missing" };
        println!("  {}:{} {}", probe.group, probe.label, status);
    }
    Ok(())
}

fn approvals(args: &[String]) -> Result<()> {
    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "life approvals",
                "status": "not_synced",
                "source": "STDB approval_requests",
                "pending": []
            })
        );
        return Ok(());
    }
    println!("life approvals");
    println!("  status: not synced");
    println!("  source: STDB approval_requests");
    Ok(())
}

fn import(args: &[String]) -> Result<()> {
    let target = args
        .get(1)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("usage: heiwa life import home|claude|codex|calendar --dry-run"))?;
    let dry_run = has_flag(args, "--dry-run");
    if !dry_run {
        return Err(anyhow!(
            "life import requires --dry-run until STDB sync is explicitly wired"
        ));
    }

    let plan = import_plan(target)?;
    if has_flag(args, "--json") {
        let rows = plan
            .row_counts
            .iter()
            .map(|row| json!({"table": row.table, "rows": row.rows}))
            .collect::<Vec<_>>();
        println!(
            "{}",
            json!({
                "command": "life import",
                "target": plan.target,
                "dry_run": plan.dry_run,
                "stdb_mode": plan.stdb_mode,
                "row_counts": rows
            })
        );
        return Ok(());
    }

    if has_flag(args, "--jsonl") {
        for row in &plan.row_counts {
            println!(
                "{}",
                json!({
                    "command": "life import",
                    "target": plan.target,
                    "dry_run": plan.dry_run,
                    "stdb_mode": plan.stdb_mode,
                    "table": row.table,
                    "rows": row.rows
                })
            );
        }
        return Ok(());
    }

    println!("life import {}", plan.target);
    println!("  dry_run: {}", plan.dry_run);
    println!("  stdb: {}", plan.stdb_mode);
    for row in plan.row_counts {
        println!("  {}: {} rows", row.table, row.rows);
    }
    Ok(())
}

fn import_plan(target: &str) -> Result<ImportPlan> {
    let probes = all_source_probes();
    let row_counts = match target {
        "home" => {
            let present = probes
                .iter()
                .filter(|probe| probe.group == "home" && probe.present)
                .count();
            vec![
                RowCount {
                    table: "life_sources",
                    rows: present,
                },
                RowCount {
                    table: "life_memory_events",
                    rows: present,
                },
                RowCount {
                    table: "life_readmodel_snapshots",
                    rows: usize::from(present > 0),
                },
            ]
        }
        "claude" => {
            let present = probes
                .iter()
                .filter(|probe| probe.group == "claude" && probe.present)
                .count();
            let task_count = claude_scheduled_task_count().unwrap_or(present);
            vec![
                RowCount {
                    table: "life_sources",
                    rows: present,
                },
                RowCount {
                    table: "life_automation_sources",
                    rows: task_count,
                },
                RowCount {
                    table: "life_briefs",
                    rows: task_count.min(2),
                },
            ]
        }
        "codex" => {
            let present = probes
                .iter()
                .filter(|probe| probe.group == "codex" && probe.present)
                .count();
            vec![
                RowCount {
                    table: "life_sources",
                    rows: present,
                },
                RowCount {
                    table: "life_automation_sources",
                    rows: present,
                },
            ]
        }
        "calendar" => vec![
            RowCount {
                table: "life_sources",
                rows: 0,
            },
            RowCount {
                table: "life_schedule_windows",
                rows: 0,
            },
        ],
        other => return Err(anyhow!("unknown life import target: {other}")),
    };

    Ok(ImportPlan {
        target: target.to_string(),
        dry_run: true,
        stdb_mode: stdb_mode(),
        row_counts,
    })
}

fn source_group_summaries(probes: &[SourceProbe]) -> Vec<SourceGroupSummary> {
    let mut groups = Vec::new();
    for group in ["home", "claude", "codex"] {
        let group_probes = probes
            .iter()
            .filter(|probe| probe.group == group)
            .collect::<Vec<_>>();
        let present = group_probes.iter().filter(|probe| probe.present).count();
        let latest_modified_unix = group_probes
            .iter()
            .filter_map(|probe| probe.modified_unix)
            .max();
        groups.push(SourceGroupSummary {
            group,
            configured: group_probes.len(),
            present,
            missing: group_probes.len().saturating_sub(present),
            latest_modified_unix,
        });
    }
    groups
}

fn all_source_probes() -> Vec<SourceProbe> {
    let mut probes = home_source_probes();
    probes.extend(claude_source_probes());
    probes.extend(codex_source_probes());
    probes
}

fn home_source_probes() -> Vec<SourceProbe> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    [
        "current_state_register.md",
        "heiwa_life_project.md",
        "heiwa_background_machine_plane_2026-05-13.md",
        "schedule_and_automation_map.md",
        "life_os_roi_cadence_2026-05-12.md",
        "work_schedule_may_2026.md",
        "daily_scorecard.md",
    ]
    .iter()
    .map(|name| {
        probe_path(
            "home",
            (*name).to_string(),
            home.join("plans").join("ultimate_devon").join(name),
        )
    })
    .collect()
}

fn claude_source_probes() -> Vec<SourceProbe> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let root = home
        .join("Library")
        .join("Application Support")
        .join("Claude")
        .join("local-agent-mode-sessions");
    find_named_files(&root, OsStr::new("scheduled-tasks.json"), 5)
        .into_iter()
        .map(|path| probe_path("claude", "scheduled-tasks.json".to_string(), path))
        .collect()
}

fn codex_source_probes() -> Vec<SourceProbe> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let root = home.join(".codex").join("automations");
    let mut probes = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path().join("automation.toml");
            if path.exists() {
                let label = entry.file_name().to_string_lossy().to_string();
                probes.push(probe_path("codex", label, path));
            }
        }
    }
    probes
}

fn probe_path(group: &'static str, label: String, path: PathBuf) -> SourceProbe {
    let metadata = fs::metadata(&path).ok();
    let modified_unix = metadata
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    SourceProbe {
        group,
        label,
        present: path.exists(),
        path,
        modified_unix,
    }
}

fn find_named_files(root: &Path, name: &OsStr, max_depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    find_named_files_inner(root, name, max_depth, &mut found);
    found
}

fn find_named_files_inner(root: &Path, name: &OsStr, depth: usize, found: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name() == Some(name) {
            found.push(path);
        } else if path.is_dir() {
            find_named_files_inner(&path, name, depth - 1, found);
        }
    }
}

fn claude_scheduled_task_count() -> Option<usize> {
    let probes = claude_source_probes();
    let first = probes.iter().find(|probe| probe.present)?;
    let raw = fs::read_to_string(&first.path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    json.get("scheduledTasks")?.as_array().map(Vec::len)
}

fn stdb_mode() -> String {
    if heiwa_stdb::StdbConfig::resolve().is_some() {
        "configured-offline-spool".to_string()
    } else {
        "offline-spool".to_string()
    }
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn print_help() {
    println!("heiwa life");
    println!();
    println!("Usage:");
    println!("  heiwa life status [--json]");
    println!("  heiwa life today [--json]");
    println!("  heiwa life freshness [--json]");
    println!("  heiwa life approvals [--json]");
    println!("  heiwa life import home|claude|codex|calendar --dry-run [--json|--jsonl]");
    println!();
    println!("Writes nothing unless explicit STDB sync is added later.");
}
