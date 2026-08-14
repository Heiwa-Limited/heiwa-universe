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
    evidence_mode: String,
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
    let evidence_mode = evidence_mode();

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
                "evidence_mode": evidence_mode,
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
    println!("  evidence: {evidence_mode}");
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
    println!(
        "  provider_runtime: evidence={}",
        snapshot.runtime.evidence_mode
    );
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
    let pending = pending_approvals_summary();
    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "life approvals",
                "status": "local",
                "source": "local approval request journal",
                "pending": pending
            })
        );
        return Ok(());
    }
    println!("life approvals");
    println!("  status: local");
    println!("  source: local approval request journal");
    println!("  pending: {}", pending.len());
    for request in pending {
        println!("    {}", request);
    }
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
            "life import is preview-only; no canonical local journal importer is available"
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
                "evidence_mode": plan.evidence_mode,
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
                    "evidence_mode": plan.evidence_mode,
                    "table": row.table,
                    "rows": row.rows
                })
            );
        }
        return Ok(());
    }

    println!("life import {}", plan.target);
    println!("  dry_run: {}", plan.dry_run);
    println!("  evidence: {}", plan.evidence_mode);
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
        evidence_mode: evidence_mode(),
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
            crate::home::heiwa_state_dir()
                .join("life")
                .join("plans")
                .join(name),
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

fn evidence_mode() -> String {
    "local-jsonl".to_string()
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
    println!("Import is preview-only; it does not write local operator or evidence journals.");
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
    evidence_mode: String,
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
                "evidence_mode": self.runtime.evidence_mode
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

/// Schema this runtime accepts. A projection carrying any other version is
/// rejected rather than best-effort parsed: the producer lives outside this
/// repo, so version skew is a normal condition and must fail loudly.
const SOCIAL_SCHEMA_VERSION: &str = "life_social_v1";

/// The producer's declared handling policy. Asserted, not trusted - a
/// projection that does not declare exactly this is refused.
const SOCIAL_POLICY: &str = "metadata-only-no-message-text";

/// Longest a display name or relation label may be.
///
/// A closed schema proves only these FIELDS exist. It cannot prove the string
/// in `name` is a name - message text can arrive through any allowed string.
/// Length bounds shrink that channel; they do not close it. See the trust
/// boundary note on `SocialProjection`.
const MAX_LABEL_LEN: usize = 120;

/// Which relationship bucket a contact or aggregate count falls in. A closed
/// set, so it is an enum rather than a `String`: an unrecognised map key or
/// contact value is rejected instead of being forwarded verbatim.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum SocialBucket {
    Friend,
    Family,
    Work,
    Group,
    Ended,
    Service,
    Acquaintance,
    Unidentified,
    Other,
}

/// Reciprocity classification. Also a closed set.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum SocialClass {
    Reciprocal,
    OneSidedOut,
    OneSidedIn,
    Broadcast,
    Incidental,
    Group,
    Ended,
    Service,
}

/// One contact. `deny_unknown_fields` is the structural control here.
///
/// The first version of this route deserialised into `serde_json::Value` and
/// re-emitted it, checking four forbidden key names on the way past. That is a
/// denylist, and a denylist cannot enforce metadata-only: `snippet`, `raw`,
/// `preview`, a nested object, or anything else unanticipated passes straight
/// through. Only a closed schema can state what is allowed.
///
/// `bucket` and `class` are enums because their value sets are closed.
/// `name` and `relation` cannot be - they carry operator-defined labels - so
/// they are length-bounded and otherwise TRUSTED. That is the residual
/// semantic boundary, stated rather than papered over.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct SocialContact {
    name: String,
    relation: String,
    bucket: SocialBucket,
    class: SocialClass,
    sent: u64,
    recv: u64,
    total: u64,
    active_days: u64,
    last: chrono::NaiveDateTime,
    stale_days: i64,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct ReconnectDue {
    name: String,
    relation: String,
    stale_days: i64,
    score: f64,
}

#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum ReconnectStatus {
    Ok,
    Error,
}

/// Failure as a CODE, never as a message.
///
/// The producer previously published `f"{type(e).__name__}: {e}"`, so an
/// exception string reached this boundary unfiltered - and an exception string
/// can contain a query, a row, or a fragment of the very message text the
/// whole projection exists to keep out. A closed set of codes says what went
/// wrong without carrying an arbitrary payload to say it.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum ReconnectError {
    ProducerUnavailable,
    RankingFailed,
    DataUnavailable,
}

/// Reconnect ranking carries an explicit status. The producer previously
/// swallowed a failed calculation into an empty list, which made "the
/// calculation broke" indistinguishable from "nobody is due" - the two states
/// call for opposite responses.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct ReconnectState {
    status: ReconnectStatus,
    #[serde(default)]
    error: Option<ReconnectError>,
    due: Vec<ReconnectDue>,
}

/// # Trust boundary
///
/// This schema proves **which fields exist and, where the value set is closed,
/// which values they hold**. It cannot prove that the string in `name` is a
/// name. `name` and `relation` carry operator-defined labels, so a producer
/// that put message text there would satisfy every check in this file.
///
/// What is enforced: no unknown fields; count keys, `bucket`, `class`,
/// `status` and `error` restricted to closed sets; timestamps parsed and
/// canonically re-serialised; reconnect cross-field invariants; labels
/// length-bounded; the declared `policy` and `schema_version` matched exactly.
///
/// What is trusted: that `name` and `relation` contain labels. That is a
/// producer contract, documented in `docs/standards/life-social-projection.md`
/// and not verifiable here.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct SocialProjection {
    schema_version: String,
    generated_at: chrono::NaiveDateTime,
    window_days: u32,
    counts: std::collections::BTreeMap<SocialBucket, u64>,
    messages_total: u64,
    identified_pct: f64,
    live_relationships: u64,
    reconnect: ReconnectState,
    contacts: Vec<SocialContact>,
    policy: String,
}

/// Canonical location: `$HEIWA_STATE_DIR/life/social.json`, falling back to
/// `~/.heiwa/state/life/social.json`.
///
/// The first version read a personal plans directory hardcoded into a
/// product route. That cannot survive per-machine initialisation and made
/// the route unusable for anyone but its author.
fn social_projection_path() -> PathBuf {
    // `HEIWA_STATE_DIR` is honored by ConfigRoot; reading it here as well
    // was the second resolver this function used to carry.
    crate::home::heiwa_state_dir()
        .join("life")
        .join("social.json")
}

/// Social read model: relationship cadence, reciprocity, and who is due a
/// reconnect.
///
/// Sourced from a published projection rather than from `life.db` directly.
/// `heiwa_shell` has no SQLite dependency, and taking one so a read model can
/// be served would put schema knowledge in two places and tie the runtime's
/// release cycle to an external producer's schema.
///
/// Everything served here is re-serialised from the validated struct above -
/// nothing is passed through from the file. A field the schema does not name
/// cannot reach a consumer.
pub(crate) fn social_payload() -> Value {
    social_payload_at(&social_projection_path())
}

pub(crate) fn social_payload_at(path: &Path) -> Value {
    let display = path.display().to_string();
    let Ok(raw) = fs::read_to_string(path) else {
        return social_unavailable("no published social projection", Some(display));
    };
    let parsed: SocialProjection = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(err) => {
            // Only the LOCATION, never serde's message. serde embeds the
            // offending value in errors like `unknown variant \`…\``, so
            // echoing it back made the rejection reason itself a leak channel -
            // a projection carrying message text in a bad field would have had
            // that text reflected into the response that rejected it. Caught by
            // `free_text_error_message_is_rejected`.
            return social_unavailable(
                &format!(
                    "projection rejected by schema at line {}, column {}",
                    err.line(),
                    err.column()
                ),
                Some(display),
            );
        }
    };
    // Neither branch echoes the producer's value back - `schema_version` and
    // `policy` are producer-controlled strings like any other.
    if parsed.schema_version != SOCIAL_SCHEMA_VERSION {
        return social_unavailable(
            &format!("unsupported schema_version; this runtime accepts {SOCIAL_SCHEMA_VERSION:?}"),
            Some(display),
        );
    }
    if parsed.policy != SOCIAL_POLICY {
        return social_unavailable(
            &format!("projection policy mismatch; refusing anything but {SOCIAL_POLICY:?}"),
            Some(display),
        );
    }
    if let Err(reason) = validate_projection(&parsed) {
        return social_unavailable(&reason, Some(display));
    }

    json!({
        "command": "life social",
        "available": true,
        "source": display,
        "age_days": source_age_days(path),
        "schema_version": parsed.schema_version,
        "policy": parsed.policy,
        "generated_at": parsed.generated_at,
        "window_days": parsed.window_days,
        "counts": parsed.counts,
        "messages_total": parsed.messages_total,
        "identified_pct": parsed.identified_pct,
        "live_relationships": parsed.live_relationships,
        "reconnect": parsed.reconnect,
        "contacts": parsed.contacts,
    })
}

/// Validate semantic invariants not expressible through serde's field types.
fn validate_projection(projection: &SocialProjection) -> Result<(), String> {
    for contact in &projection.contacts {
        for (field, value) in [("name", &contact.name), ("relation", &contact.relation)] {
            if value.chars().count() > MAX_LABEL_LEN {
                return Err(format!(
                    "contact {field} exceeds {MAX_LABEL_LEN} chars; labels only"
                ));
            }
        }
    }
    for due in &projection.reconnect.due {
        for (field, value) in [("name", &due.name), ("relation", &due.relation)] {
            if value.chars().count() > MAX_LABEL_LEN {
                return Err(format!(
                    "reconnect {field} exceeds {MAX_LABEL_LEN} chars; labels only"
                ));
            }
        }
    }

    match (
        &projection.reconnect.status,
        projection.reconnect.error.as_ref(),
        projection.reconnect.due.is_empty(),
    ) {
        (ReconnectStatus::Ok, None, _) | (ReconnectStatus::Error, Some(_), true) => {}
        _ => return Err("reconnect state is inconsistent".to_string()),
    }
    Ok(())
}

fn social_unavailable(reason: &str, source: Option<String>) -> Value {
    json!({
        "command": "life social",
        "available": false,
        "reason": reason,
        "source": source,
        "counts": {},
        "contacts": [],
        "reconnect": { "status": "unavailable", "due": [] },
    })
}

fn source_age_days(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let age = std::time::SystemTime::now().duration_since(modified).ok()?;
    Some((age.as_secs() / 86_400) as i64)
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
            evidence_mode: evidence_mode(),
        },
    }
}

fn pending_approvals_summary() -> Vec<Value> {
    crate::cmd::approvals::scan_pending_requests()
        .into_iter()
        .map(|req| crate::cmd::approvals::approval_request_summary(&req))
        .collect()
}

/// Per-user plans live under the state dir (`state/life/plans/`), matching
/// [`social_projection_path`]. The first version read one operator's personal
/// personal plans directory, which cannot survive per-machine
/// initialisation.
fn plan_path(name: &str) -> Option<PathBuf> {
    Some(
        crate::home::heiwa_state_dir()
            .join("life")
            .join("plans")
            .join(name),
    )
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

#[cfg(test)]
pub(crate) mod social_projection_tests {
    use super::*;
    use serde_json::Value;
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};

    /// Serialises every test that touches process environment.
    ///
    /// `std::env::set_var` mutates state shared by the whole test binary, and
    /// cargo runs tests in parallel threads. Without this a concurrent test
    /// could observe another's HEIWA_STATE_DIR - or, worse, observe the real
    /// one after a test removed the override and read live operator state.
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// Sets HEIWA_STATE_DIR for a scope and restores the PRIOR value, which
    /// may itself have been set. Removing unconditionally would leak a
    /// different bug: the next reader falls back to the real state directory.
    pub(crate) struct StateDirGuard {
        prior: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl StateDirGuard {
        pub(crate) fn set(path: &Path) -> Self {
            let lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
            let prior = std::env::var_os("HEIWA_STATE_DIR");
            std::env::set_var("HEIWA_STATE_DIR", path);
            Self { prior, _lock: lock }
        }
    }

    impl Drop for StateDirGuard {
        fn drop(&mut self) {
            match self.prior.take() {
                Some(value) => std::env::set_var("HEIWA_STATE_DIR", value),
                None => std::env::remove_var("HEIWA_STATE_DIR"),
            }
        }
    }

    /// Every case writes its own file and passes the path in. The earlier test
    /// called the real `social_payload()`, so under an empty HOME it took the
    /// `available:false` branch and asserted nothing about validation - it
    /// passed without ever exercising the code it claimed to cover.
    fn projection(body: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("social.json");
        let mut file = fs::File::create(&path).expect("create");
        file.write_all(body.as_bytes()).expect("write");
        (dir, path)
    }

    fn valid_body() -> String {
        r#"{
          "schema_version": "life_social_v1",
          "generated_at": "2026-08-11T09:00:00",
          "window_days": 90,
          "counts": {"friend": 12},
          "messages_total": 5189,
          "identified_pct": 99.4,
          "live_relationships": 20,
          "reconnect": {"status": "ok", "error": null, "due": [
            {"name":"Clayton Ring","relation":"friend","stale_days":62,"score":74.4}
          ]},
          "contacts": [
            {"name":"Justin","relation":"friend","bucket":"friend","class":"reciprocal",
             "sent":456,"recv":916,"total":1372,"active_days":78,
             "last":"2026-08-10T20:00:00","stale_days":1}
          ],
          "policy": "metadata-only-no-message-text"
        }"#
        .to_string()
    }

    fn reason(payload: &Value) -> String {
        payload
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn valid_projection_is_served() {
        let (_dir, path) = projection(&valid_body());
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload.get("policy").and_then(Value::as_str),
            Some(SOCIAL_POLICY)
        );
        assert_eq!(
            payload.pointer("/contacts/0/name").and_then(Value::as_str),
            Some("Justin")
        );
        assert_eq!(
            payload.pointer("/reconnect/status").and_then(Value::as_str),
            Some("ok")
        );
    }

    #[test]
    fn missing_file_is_reported_not_fatal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let payload = social_payload_at(&dir.path().join("absent.json"));
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(false)
        );
        assert!(reason(&payload).contains("no published"));
        assert!(payload.get("contacts").is_some_and(Value::is_array));
    }

    #[test]
    fn malformed_json_is_rejected() {
        let (_dir, path) = projection("{ this is not json ");
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(false)
        );
        assert!(reason(&payload).contains("rejected by schema"));
    }

    #[test]
    fn bare_scalar_json_is_rejected() {
        // Valid JSON, wrong shape. A Value-based reader would accept this.
        for body in ["42", "\"hello\"", "null", "[]", "true"] {
            let (_dir, path) = projection(body);
            let payload = social_payload_at(&path);
            assert_eq!(
                payload.get("available").and_then(Value::as_bool),
                Some(false),
                "scalar {body} was accepted"
            );
        }
    }

    #[test]
    fn unknown_contact_field_is_rejected() {
        // The whole point of the closed schema. `snippet` is not on the
        // forbidden-key list the first implementation checked, and would have
        // sailed through to the consumer.
        let body = valid_body().replace(
            r#""stale_days":1}"#,
            r#""stale_days":1,"snippet":"hey are you free saturday"}"#,
        );
        let (_dir, path) = projection(&body);
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(false)
        );
        assert!(reason(&payload).contains("rejected by schema"));
        // The reason must not reflect producer-controlled content back.
        assert!(!reason(&payload).contains("snippet"));
        assert!(!serde_json::to_string(&payload)
            .unwrap()
            .contains("saturday"));
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let body = valid_body().replace(
            r#""policy": "metadata-only-no-message-text""#,
            r#""policy": "metadata-only-no-message-text", "raw_messages": ["leak"]"#,
        );
        let (_dir, path) = projection(&body);
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(false)
        );
        assert!(!serde_json::to_string(&payload).unwrap().contains("leak"));
    }

    #[test]
    fn wrong_schema_version_is_rejected() {
        let body = valid_body().replace("life_social_v1", "life_social_v2");
        let (_dir, path) = projection(&body);
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(false)
        );
        assert!(reason(&payload).contains("unsupported schema_version"));
    }

    #[test]
    fn wrong_policy_declaration_is_rejected() {
        let body = valid_body().replace(SOCIAL_POLICY, "includes-message-text");
        let (_dir, path) = projection(&body);
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(false)
        );
        assert!(reason(&payload).contains("refusing"));
    }

    #[test]
    fn stale_but_valid_projection_is_still_served_with_age() {
        let (_dir, path) = projection(&valid_body());
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(true)
        );
        assert!(payload.get("age_days").is_some_and(Value::is_number));
    }

    #[test]
    fn reconnect_error_state_survives_to_the_consumer() {
        // "the calculation broke" must not look like "nobody is due".
        let body = valid_body()
            .replace(
                r#""status": "ok", "error": null"#,
                r#""status": "error", "error": "ranking_failed""#,
            )
            .replace(
                r#""due": [
            {"name":"Clayton Ring","relation":"friend","stale_days":62,"score":74.4}
          ]"#,
                r#""due": []"#,
            );
        let (_dir, path) = projection(&body);
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload.pointer("/reconnect/status").and_then(Value::as_str),
            Some("error")
        );
        assert_eq!(
            payload.pointer("/reconnect/error").and_then(Value::as_str),
            Some("ranking_failed")
        );
    }

    #[test]
    fn free_text_error_message_is_rejected() {
        // The producer used to publish `f"{type(e).__name__}: {e}"`. An
        // exception string can carry a query, a row, or message text.
        let body = valid_body().replace(
            r#""error": null"#,
            r#""error": "OperationalError: no such column: hey are you free saturday""#,
        );
        let (_dir, path) = projection(&body);
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(false)
        );
        assert!(!serde_json::to_string(&payload)
            .unwrap()
            .contains("saturday"));
    }

    #[test]
    fn unknown_bucket_or_class_is_rejected() {
        for (from, to) in [
            (r#""bucket":"friend""#, r#""bucket":"secret_lover""#),
            (r#""class":"reciprocal""#, r#""class":"leaked""#),
        ] {
            let (_dir, path) = projection(&valid_body().replace(from, to));
            let payload = social_payload_at(&path);
            assert_eq!(
                payload.get("available").and_then(Value::as_bool),
                Some(false),
                "{to} was accepted"
            );
        }
    }

    #[test]
    fn unknown_counts_key_is_rejected_without_reflection() {
        let body = valid_body().replace(
            r#""counts": {"friend": 12}"#,
            r#""counts": {"hey are you free saturday": 12}"#,
        );
        let (_dir, path) = projection(&body);
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(false)
        );
        assert!(!serde_json::to_string(&payload)
            .unwrap()
            .contains("saturday"));
    }

    #[test]
    fn timestamps_are_parsed_and_canonically_reserialized() {
        let (_dir, path) = projection(&valid_body());
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("generated_at").and_then(Value::as_str),
            Some("2026-08-11T09:00:00")
        );
        assert_eq!(
            payload
                .get("contacts")
                .and_then(Value::as_array)
                .and_then(|contacts| contacts.first())
                .and_then(|contact| contact.get("last"))
                .and_then(Value::as_str),
            Some("2026-08-10T20:00:00")
        );

        let body = valid_body()
            .replace("2026-08-11T09:00:00", "message text hidden in generated_at")
            .replace("2026-08-10T20:00:00", "message text hidden in last");
        let (_dir, path) = projection(&body);
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(false)
        );
        let encoded = serde_json::to_string(&payload).unwrap();
        assert!(!encoded.contains("message text"));
    }

    #[test]
    fn reconnect_state_rejects_contradictory_combinations() {
        let cases = [
            (
                r#""status": "ok", "error": null"#,
                r#""status": "ok", "error": "ranking_failed""#,
            ),
            (
                r#""status": "ok", "error": null"#,
                r#""status": "error", "error": null"#,
            ),
            (
                r#""status": "ok", "error": null"#,
                r#""status": "error", "error": "ranking_failed""#,
            ),
        ];

        for (index, (from, to)) in cases.into_iter().enumerate() {
            let mut body = valid_body().replace(from, to);
            if index == 2 {
                // valid_body carries one due row; error states must not.
                assert!(body.contains(r#""due": ["#));
            } else if index == 1 {
                body = body.replace(
                    r#""due": [
            {"name":"Clayton Ring","relation":"friend","stale_days":62,"score":74.4}
          ]"#,
                    r#""due": []"#,
                );
            }
            let (_dir, path) = projection(&body);
            let payload = social_payload_at(&path);
            assert_eq!(
                payload.get("available").and_then(Value::as_bool),
                Some(false),
                "contradictory reconnect case {index} was accepted"
            );
        }
    }

    #[test]
    fn overlong_label_is_rejected() {
        let long = "x".repeat(MAX_LABEL_LEN + 1);
        let body = valid_body().replace(r#""name":"Justin""#, &format!(r#""name":"{long}""#));
        let (_dir, path) = projection(&body);
        let payload = social_payload_at(&path);
        assert_eq!(
            payload.get("available").and_then(Value::as_bool),
            Some(false)
        );
        assert!(reason(&payload).contains("labels only"));
    }

    #[test]
    fn state_dir_env_overrides_the_projection_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let prior = std::env::var_os("HEIWA_STATE_DIR");
        let resolved = {
            let _guard = StateDirGuard::set(dir.path());
            social_projection_path()
        };
        assert_eq!(resolved, dir.path().join("life").join("social.json"));
        assert!(!resolved.to_string_lossy().contains("ultimate_devon"));
        // The guard restored whatever was there before, set or unset.
        assert_eq!(std::env::var_os("HEIWA_STATE_DIR"), prior);
    }

    #[test]
    fn default_path_is_under_dot_heiwa_state() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let prior = std::env::var_os("HEIWA_STATE_DIR");
        std::env::remove_var("HEIWA_STATE_DIR");
        let resolved = social_projection_path();
        if let Some(value) = prior {
            std::env::set_var("HEIWA_STATE_DIR", value);
        }
        assert!(resolved.ends_with("life/social.json"), "{resolved:?}");
        assert!(
            resolved
                .ancestors()
                .any(|path| path.ends_with(Path::new(".heiwa").join("state"))),
            "{resolved:?}"
        );
    }
}
