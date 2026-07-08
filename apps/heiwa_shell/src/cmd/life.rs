use anyhow::{anyhow, Result};
use chrono::Local;
use serde_json::{json, Value};
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
struct FreshnessProbe {
    group: &'static str,
    label: String,
    path: PathBuf,
    present: bool,
    modified_unix: Option<u64>,
    age_days: Option<i64>,
    sla_days: i64,
    stale: bool,
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
    let snapshot = build_today_snapshot();

    if has_flag(args, "--json") {
        println!("{}", snapshot.to_value());
        return Ok(());
    }

    println!("life today  {}  ({})", snapshot.date, snapshot.timezone);
    println!("  day_type: {}", snapshot.day_type);
    if snapshot.work_shifts.is_empty() {
        println!("  work_shifts: none");
    } else {
        println!("  work_shifts: {}", snapshot.work_shifts.join("; "));
    }
    if !snapshot.appointments.is_empty() {
        println!("  appointments:");
        for appt in &snapshot.appointments {
            println!("    - {} {}: {}", appt.date, appt.kind, appt.note);
        }
    }
    println!(
        "  proof_target: {}",
        snapshot
            .proof_target
            .as_deref()
            .unwrap_or("(no scorecard row for today)")
    );
    if snapshot.stale_facts.is_empty() {
        println!("  stale_facts: none");
    } else {
        println!("  stale_facts:");
        for fact in &snapshot.stale_facts {
            println!(
                "    - {} (age {}d, sla {}d, source {})",
                fact.label, fact.age_days, fact.sla_days, fact.source
            );
        }
    }
    if snapshot.pending_approvals.is_empty() {
        println!("  pending_approvals: 0");
    } else {
        println!("  pending_approvals: {}", snapshot.pending_approvals.len());
        for req in snapshot.pending_approvals.iter().take(5) {
            let id = req.get("id").and_then(Value::as_str).unwrap_or("?");
            let action = req.get("action").and_then(Value::as_str).unwrap_or("?");
            let target = req.get("target").and_then(Value::as_str).unwrap_or("?");
            let risk = req.get("risk").and_then(Value::as_str).unwrap_or("?");
            println!("    - {id}  {action} -> {target}  risk={risk}");
        }
        if snapshot.pending_approvals.len() > 5 {
            println!("    ... {} more", snapshot.pending_approvals.len() - 5);
        }
    }
    println!("  provider_runtime: stdb={}", snapshot.runtime.stdb_mode);
    println!("  next: heiwa life freshness --json | heiwa approvals list");
    Ok(())
}

fn freshness(args: &[String]) -> Result<()> {
    if has_flag(args, "--json") {
        println!("{}", freshness_payload());
        return Ok(());
    }

    let date = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let probes = freshness_probes(&date);
    println!("life freshness");
    for probe in probes {
        let status = if probe.present { "present" } else { "missing" };
        let age = probe
            .age_days
            .map(|days| format!("{days}d"))
            .unwrap_or_else(|| "unknown".to_string());
        println!(
            "  {}:{} {} age={} sla={}d stale={}",
            probe.group, probe.label, status, age, probe.sla_days, probe.stale
        );
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
    let Some(home) = crate::home::heiwa_home() else {
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
    let Some(home) = crate::home::heiwa_home() else {
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
    let Some(home) = crate::home::heiwa_home() else {
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

#[derive(Clone, Debug)]
struct Appointment {
    kind: String,
    date: String,
    note: String,
}

#[derive(Clone, Debug)]
struct StaleFact {
    label: String,
    source: String,
    age_days: i64,
    sla_days: i64,
}

#[derive(Clone, Debug)]
struct RuntimeStatus {
    stdb_mode: String,
}

#[derive(Clone, Debug)]
struct TodaySnapshot {
    date: String,
    timezone: String,
    day_type: String,
    work_shifts: Vec<String>,
    appointments: Vec<Appointment>,
    proof_target: Option<String>,
    scorecard_notes: Option<String>,
    stale_facts: Vec<StaleFact>,
    pending_approvals: Vec<Value>,
    runtime: RuntimeStatus,
}

impl TodaySnapshot {
    fn to_value(&self) -> Value {
        let appointments: Vec<Value> = self
            .appointments
            .iter()
            .map(|a| json!({ "kind": a.kind, "date": a.date, "note": a.note }))
            .collect();
        let stale: Vec<Value> = self
            .stale_facts
            .iter()
            .map(|s| {
                json!({
                    "label": s.label,
                    "source": s.source,
                    "age_days": s.age_days,
                    "sla_days": s.sla_days
                })
            })
            .collect();
        json!({
            "command": "life today",
            "date": self.date,
            "timezone": self.timezone,
            "day_type": self.day_type,
            "work_shifts": self.work_shifts,
            "appointments": appointments,
            "proof_target": self.proof_target,
            "scorecard_notes": self.scorecard_notes,
            "stale_facts": stale,
            "pending_approvals": self.pending_approvals,
            "runtime": {
                "stdb_mode": self.runtime.stdb_mode
            },
            "next": [
                "heiwa life freshness --json",
                "heiwa approvals list"
            ]
        })
    }
}

pub(crate) fn today_payload() -> Value {
    let mut payload = build_today_snapshot().to_value();

    // Merge the calendar and mail read models so /api/v1/life/today is the
    // one-call morning brief: schedule pressure + priority mail + approvals.
    if let Value::Object(ref mut map) = payload {
        let date = Local::now().date_naive().format("%Y-%m-%d").to_string();
        let holds = crate::cmd::calendar::holds_for_date(&date);
        map.insert(
            "calendar".to_string(),
            json!({
                "holds_today": holds.len(),
                "holds": holds,
            }),
        );

        let priority = crate::cmd::mail::priority_rows();
        let draft_tier = priority
            .iter()
            .filter(|row| row.get("action").and_then(Value::as_str) == Some("draft"))
            .count();
        map.insert(
            "mail".to_string(),
            json!({
                "priority_count": priority.len(),
                "draft_tier": draft_tier,
                "top": priority.into_iter().take(3).collect::<Vec<_>>(),
            }),
        );
    }

    payload
}

/// Today's dated appointments from the life register, as JSON rows.
/// Shared with the calendar read model.
pub(crate) fn appointments_for_today() -> Vec<Value> {
    let date = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let register_path = plan_path("current_state_register.md");
    parse_appointments(&register_path, &date)
        .into_iter()
        .filter(|appointment| appointment.date == date)
        .map(|appointment| {
            json!({
                "kind": appointment.kind,
                "date": appointment.date,
                "note": appointment.note,
            })
        })
        .collect()
}

pub(crate) fn freshness_payload() -> Value {
    let date = Local::now().date_naive().format("%Y-%m-%d").to_string();
    freshness_payload_for_date(&date)
}

fn freshness_payload_for_date(date: &str) -> Value {
    let probes = freshness_probes(date);
    let stale_sources = probes.iter().filter(|probe| probe.stale).count();
    let sources = probes
        .iter()
        .map(|probe| {
            json!({
                "group": probe.group,
                "label": probe.label,
                "path": probe.path.display().to_string(),
                "present": probe.present,
                "modified_unix": probe.modified_unix,
                "age_days": probe.age_days,
                "sla_days": probe.sla_days,
                "stale": probe.stale,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "command": "life freshness",
        "date": date,
        "stale_sources": stale_sources,
        "sources": sources
    })
}

fn build_today_snapshot() -> TodaySnapshot {
    let date = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let timezone = "America/Vancouver".to_string();

    let schedule_path = plan_path("work_schedule_may_2026.md");
    let scorecard_path = plan_path("daily_scorecard.md");
    let register_path = plan_path("current_state_register.md");

    let (day_type, work_shifts) = parse_work_schedule_row(&schedule_path, &date);
    let (proof_target, scorecard_notes) = parse_scorecard_row(&scorecard_path, &date);
    let appointments = parse_appointments(&register_path, &date);
    let stale_facts = compute_stale_facts(&date);
    let pending_approvals = pending_approvals_summary();

    TodaySnapshot {
        date,
        timezone,
        day_type,
        work_shifts,
        appointments,
        proof_target,
        scorecard_notes,
        stale_facts,
        pending_approvals,
        runtime: RuntimeStatus {
            stdb_mode: stdb_mode(),
        },
    }
}

fn pending_approvals_summary() -> Vec<Value> {
    crate::cmd::approvals::scan_pending_requests()
        .into_iter()
        .map(|req| crate::cmd::approvals::approval_request_summary(&req))
        .collect()
}

fn plan_path(name: &str) -> Option<PathBuf> {
    crate::home::heiwa_home().map(|home| home.join("plans").join("ultimate_devon").join(name))
}

fn parse_work_schedule_row(path: &Option<PathBuf>, date: &str) -> (String, Vec<String>) {
    let Some(path) = path else {
        return ("unknown".to_string(), Vec::new());
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return ("unknown".to_string(), Vec::new());
    };
    for line in raw.lines() {
        let line = line.trim();
        if !line.starts_with('|') || !line.contains(date) {
            continue;
        }
        let cells: Vec<String> = line
            .split('|')
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        if cells.len() < 3 {
            continue;
        }
        let shift_cell = &cells[2];
        if shift_cell.eq_ignore_ascii_case("off") {
            return ("off".to_string(), Vec::new());
        }
        let shifts: Vec<String> = shift_cell
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let day_type = if shifts.is_empty() {
            "unknown".to_string()
        } else {
            "work".to_string()
        };
        return (day_type, shifts);
    }
    ("unknown".to_string(), Vec::new())
}

fn parse_scorecard_row(path: &Option<PathBuf>, date: &str) -> (Option<String>, Option<String>) {
    let Some(path) = path else {
        return (None, None);
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return (None, None);
    };
    for line in raw.lines() {
        let line = line.trim();
        if !line.starts_with('|') || !line.contains(date) {
            continue;
        }
        let cells: Vec<String> = line.split('|').map(|c| c.trim().to_string()).collect();
        // Header columns (from file): Date | Wake | Sleep Cutoff | Brush AM | Brush PM | Floss
        // | No Grazing | Protein Meals | Walk/Train | Work Artifact | Admin/Money | Social/Service
        // | Spirit | Gaming Contained | Mood 1-5 | Notes
        // After split with leading/trailing empties, indices shift by 1.
        let work_artifact = cells.get(10).cloned().filter(|s| !s.is_empty());
        let notes = cells.get(16).cloned().filter(|s| !s.is_empty());
        return (work_artifact, notes);
    }
    (None, None)
}

fn parse_appointments(path: &Option<PathBuf>, today: &str) -> Vec<Appointment> {
    let Some(path) = path else {
        return Vec::new();
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        // Match lines like "- Next dental: 2026-05-26, one filling plus dentist consultation."
        // or         "- Dermatologist consultation: 2026-06-03 at 2:00 PM."
        if let Some(rest) = trimmed.strip_prefix("- Next dental:") {
            if let Some(appt) = extract_dated_note("dental", rest, today) {
                out.push(appt);
            }
        } else if let Some(rest) = trimmed.strip_prefix("- Dermatologist consultation:") {
            if let Some(appt) = extract_dated_note("dermatology", rest, today) {
                out.push(appt);
            }
        }
    }
    out
}

fn extract_dated_note(kind: &str, rest: &str, today: &str) -> Option<Appointment> {
    let rest = rest.trim().trim_end_matches('.').trim();
    let date = rest.split(|c: char| c == ',' || c.is_whitespace()).next()?;
    if !date.starts_with("20") || date.len() < 10 {
        return None;
    }
    let date = date[..10].to_string();
    if date.as_str() < today {
        return None;
    }
    let note = rest[date.len()..]
        .trim_start_matches(|c: char| c == ',' || c.is_whitespace())
        .to_string();
    Some(Appointment {
        kind: kind.to_string(),
        date,
        note,
    })
}

fn compute_stale_facts(today: &str) -> Vec<StaleFact> {
    freshness_probes(today)
        .into_iter()
        .filter(|probe| probe.group == "home" && probe.present && probe.stale)
        .filter_map(|probe| {
            Some(StaleFact {
                label: probe.label,
                source: probe.path.display().to_string(),
                age_days: probe.age_days?,
                sla_days: probe.sla_days,
            })
        })
        .collect()
}

fn freshness_probes(today: &str) -> Vec<FreshnessProbe> {
    let today_date = match chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    all_source_probes()
        .into_iter()
        .map(|probe| {
            let age_days = probe.modified_unix.map(|unix| age_days(today_date, unix));
            let sla_days = freshness_sla_days(probe.group, &probe.label);
            let stale = !probe.present || age_days.is_some_and(|age| age > sla_days);
            FreshnessProbe {
                group: probe.group,
                label: probe.label,
                path: probe.path,
                present: probe.present,
                modified_unix: probe.modified_unix,
                age_days,
                sla_days,
                stale,
            }
        })
        .collect()
}

fn age_days(today: chrono::NaiveDate, modified_unix: u64) -> i64 {
    let system_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(modified_unix);
    let modified: chrono::DateTime<Local> = system_time.into();
    (today - modified.date_naive()).num_days().max(0)
}

fn freshness_sla_days(group: &str, label: &str) -> i64 {
    match (group, label) {
        ("home", "daily_scorecard.md") => 1,
        ("home", "current_state_register.md") => 7,
        ("home", "work_schedule_may_2026.md") => 7,
        ("home", "schedule_and_automation_map.md") => 14,
        ("home", "heiwa_life_project.md") => 30,
        ("home", "heiwa_background_machine_plane_2026-05-13.md") => 30,
        ("home", "life_os_roi_cadence_2026-05-12.md") => 30,
        ("codex", _) => 7,
        ("claude", _) => 7,
        _ => 14,
    }
}
