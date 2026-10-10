use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::authority::{self, DecisionKey, Integrity, Principal};
use crate::output::{self, CliError};

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("list") | Some("status") | None => list(args),
        Some("show") => show(&args[1..]),
        Some("decide") => decide(&args[1..]),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(
            CliError::usage(format!("unknown approvals command: {other}"))
                .with_hint("run `heiwa approvals --help`")
                .into(),
        ),
    }
}

fn list(args: &[String]) -> Result<()> {
    let key = DecisionKey::load_default().ok();
    let entries = pending_request_entries_in(&requests_dir(), &decisions_dir(), key.as_ref());
    let next = list_next(&entries);
    let pending: Vec<Value> = entries.into_iter().map(|(_, request)| request).collect();
    let decisions = scan_decisions_in(&decisions_dir(), key.as_ref());
    let pending_summary: Vec<Value> = pending.iter().map(approval_request_summary).collect();
    let data = json!({
        "requests_dir": requests_dir().display().to_string(),
        "decisions_dir": decisions_dir().display().to_string(),
        "pending": pending,
        "pending_summary": pending_summary,
        "decided": decisions,
        "decisions_verifiable": key.is_some(),
    });
    output::emit(has_flag(args, "--json"), data, &next, render_list)
}

/// Follow-up commands for pending requests. A request's *content* is
/// untrusted data (a contained worker may have staged it), so hints come only
/// from the file stem, the key that `show` and decisions resolve, and only
/// when that stem is a valid, flag-free request id.
fn list_next(entries: &[(String, Value)]) -> Vec<String> {
    entries
        .iter()
        .map(|(stem, _)| stem.as_str())
        .filter(|stem| is_hintable_request_id(stem))
        .take(10)
        .map(|stem| format!("heiwa approvals show {stem}"))
        .collect()
}

/// Whether `id` may appear in a suggested shell command: a valid request id
/// (only `[A-Za-z0-9._-]`) that cannot be mistaken for a flag.
pub(crate) fn is_hintable_request_id(id: &str) -> bool {
    validate_request_id(id).is_ok() && !id.starts_with('-')
}

fn render_list(data: &Value) {
    let pending = data["pending_summary"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    println!("approvals");
    println!("  requests: {} pending", pending.len());
    println!(
        "  decisions: {} on record",
        data["decided"].as_array().map_or(0, Vec::len)
    );
    let unverified = data["decided"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|decision| decision["record_integrity"] != "verified")
        .count();
    if unverified != 0 {
        println!("  unverified decisions: {unverified}; preserved for inspection; reconciliation required");
    }
    println!(
        "  requests dir: {}",
        data["requests_dir"].as_str().unwrap_or("?")
    );
    println!(
        "  decisions dir: {}",
        data["decisions_dir"].as_str().unwrap_or("?")
    );
    for summary in pending.iter().take(10) {
        let id = summary.get("id").and_then(Value::as_str).unwrap_or("?");
        let action = summary.get("action").and_then(Value::as_str).unwrap_or("?");
        let target = summary.get("target").and_then(Value::as_str).unwrap_or("?");
        let risk = summary.get("risk").and_then(Value::as_str).unwrap_or("?");
        println!("    {id}  {action} -> {target}  risk={risk}");
    }
    if pending.len() > 10 {
        println!("    ... {} more", pending.len() - 10);
    }
}

fn show(args: &[String]) -> Result<()> {
    let id = args
        .first()
        .filter(|arg| !arg.starts_with("--"))
        .ok_or_else(|| CliError::usage("usage: heiwa approvals show <id> [--json]"))?;
    // Validate before touching the filesystem: an id must never walk out of
    // the requests directory.
    validate_request_id(id).map_err(|error| CliError::usage(format!("{error}")))?;
    let path = requests_dir().join(format!("{id}.json"));
    if !path.exists() {
        return Err(CliError::not_found(format!("approval not found: {id}"))
            .with_hint("list pending approvals with `heiwa approvals list`")
            .into());
    }
    let raw = fs::read_to_string(&path)?;
    let value: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
    let next = vec![format!("heiwa approvals decide {id} --approve|--deny")];
    output::emit(has_flag(args, "--json"), value, &next, |value| {
        println!("approval {id}");
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
    })
}

fn decide(args: &[String]) -> Result<()> {
    let id = args
        .first()
        .filter(|arg| !arg.starts_with("--"))
        .ok_or_else(|| {
            CliError::usage("usage: heiwa approvals decide <id> --approve|--deny [--note ...]")
        })?;
    let approve = has_flag(args, "--approve");
    let deny = has_flag(args, "--deny");
    if approve == deny {
        return Err(CliError::usage("must pass exactly one of --approve or --deny").into());
    }
    let dry_run = has_flag(args, "--dry-run");

    validate_request_id(id)?;
    let plan = compute_effects(id, approve)?;
    let decision = json!({
        "id": id,
        "outcome": if approve { "approved" } else { "denied" },
        "decided_at_utc": Utc::now().to_rfc3339(),
        "operator": "local-cli",
        "note": flag_value(args, "--note"),
        "effects": plan,
    });
    let path = decisions_dir().join(format!("{id}.json"));
    if dry_run {
        let data = json!({
            "dry_run": true,
            "path": path.display().to_string(),
            "decision": decision,
        });
        return output::emit(has_flag(args, "--json"), data, &[], |_| {
            println!("approvals decide (dry-run)");
            println!("  id: {id}");
            println!("  outcome: {}", decision["outcome"]);
            println!("  effects: {}", summarize_effects(&plan));
            for effect in plan.as_array().into_iter().flatten() {
                println!(
                    "    - {}: {} -> {}",
                    effect.get("surface").and_then(Value::as_str).unwrap_or("?"),
                    effect.get("target").and_then(Value::as_str).unwrap_or("?"),
                    effect.get("change").and_then(Value::as_str).unwrap_or("?")
                );
            }
            println!("  would write: {}", path.display());
        });
    }

    let principal = Principal::cli()?;
    let result = decide_request(id, approve, flag_value(args, "--note"), &principal)?;
    let decision_out = result["decision"].clone();
    let applied = decision_out["applied_effects"].clone();
    let data = json!({
        "dry_run": false,
        "path": path.display().to_string(),
        "decision": decision_out,
    });
    output::emit(has_flag(args, "--json"), data, &[], |_| {
        println!("approvals decide");
        println!("  id: {id}");
        println!("  outcome: {}", decision_out["outcome"]);
        if let Some(applied_arr) = applied.as_array() {
            if !applied_arr.is_empty() {
                println!("  applied:");
                for effect in applied_arr {
                    println!(
                        "    - {}",
                        effect.get("summary").and_then(Value::as_str).unwrap_or("?")
                    );
                }
            }
        }
        println!("  wrote: {}", path.display());
    })
}

/// Apply one immutable local decision through the same service used by the
/// CLI and Heiwa.app. Replaying the same outcome is safe; changing a recorded
/// outcome is rejected before any effect runs.
pub(crate) fn decide_request(
    id: &str,
    approve: bool,
    note: Option<String>,
    principal: &Principal,
) -> Result<Value> {
    validate_request_id(id)?;
    let _lease = DecisionLease::acquire(id)?;
    let path = decisions_dir().join(format!("{id}.json"));
    let requested_outcome = if approve { "approved" } else { "denied" };
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(anyhow!("decision integrity verification failed for {id}: nonregular record; reconciliation needed"));
            }
            let existing = heiwa_core::integrity::read_decision_record(&path).map_err(|_| {
                anyhow!("decision integrity verification failed for {id}: unreadable or malformed record; reconciliation needed")
            })?;
            let integrity = authority::classify(Some(principal.key()), id, &existing);
            if integrity != Integrity::Verified {
                return Err(anyhow!("decision integrity verification failed for {id}: {}; reconciliation needed; existing record was preserved", integrity.as_str()));
            }
            let existing_outcome = existing
                .get("outcome")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("recorded decision {id} has no outcome"))?;
            if existing_outcome != requested_outcome {
                return Err(anyhow!(
                    "approval {id} is already {existing_outcome}; recorded decisions are immutable"
                ));
            }
            return Ok(json!({
                "decision": existing,
                "path": path.display().to_string(),
                "replayed": true,
            }));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let plan = compute_effects(id, approve)?;
    let applied = apply_effects(id, &plan, approve)?;
    let mut decision = json!({
        "id": id,
        "outcome": requested_outcome,
        "decided_at_utc": Utc::now().to_rfc3339(),
        "operator": principal.display_label(),
        "principal": principal.to_json(),
        "note": note,
        "effects": plan,
        "applied_effects": applied,
    });
    authority::seal(principal.key(), &mut decision)?;
    write_decision_atomic(&path, &decision)?;
    Ok(json!({
        "decision": decision,
        "path": path.display().to_string(),
        "replayed": false,
    }))
}

/// Cross-process exclusion for one approval request. The sidecar has no
/// identity or decision content; the operating-system lock is the authority.
#[derive(Debug)]
struct DecisionLease {
    file: fs::File,
}

impl Drop for DecisionLease {
    /// Release the lock explicitly. Closing this descriptor alone does not
    /// release an flock while another descriptor on the same open file
    /// description survives, such as one briefly inherited by a child process
    /// spawned while the lease was held.
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
impl DecisionLease {
    /// A second descriptor on the same open file description, as a briefly
    /// inherited child descriptor would be.
    fn duplicate_handle_for_test(&self) -> fs::File {
        self.file.try_clone().expect("duplicate lease descriptor")
    }
}

impl DecisionLease {
    fn acquire(id: &str) -> Result<Self> {
        let dir = decisions_dir();
        fs::create_dir_all(&dir)?;
        Self::acquire_at(&dir.join(format!(".{id}.lock")))
    }

    fn acquire_at(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.try_lock().map_err(|error| match error {
            fs::TryLockError::WouldBlock => anyhow!("approval decision is already in progress"),
            fs::TryLockError::Error(error) => anyhow!(error),
        })?;
        Ok(Self { file })
    }
}

fn write_decision_atomic(path: &Path, decision: &Value) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| anyhow!("approval decision path has no parent"))?;
    fs::create_dir_all(dir)?;
    let temporary = dir.join(format!(
        ".decision.{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    let result = (|| -> Result<()> {
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(decision)?)?;
        file.sync_all()?;
        drop(file);
        // Publish without replacing an existing decision, including a record
        // that appeared during effects. Such a collision needs reconciliation.
        fs::hard_link(&temporary, path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                anyhow!("decision record appeared while applying effects; reconciliation needed; existing record was preserved")
            } else { anyhow!(error) }
        })?;
        fs::remove_file(&temporary)?;
        let _ = fs::File::open(dir).and_then(|directory| directory.sync_all());
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn validate_request_id(id: &str) -> Result<()> {
    if !heiwa_core::integrity::valid_decision_id(id) {
        return Err(anyhow!("invalid approval request id"));
    }
    Ok(())
}

/// Inspect a pending request and return the effects an approve/deny decision
/// would have on the spine. Pure read; no writes.
fn compute_effects(id: &str, approve: bool) -> Result<Value> {
    let request_path = requests_dir().join(format!("{id}.json"));
    if !request_path.exists() {
        return Err(anyhow!(
            "request {id} not found in {}/",
            requests_dir().display()
        ));
    }
    let raw = fs::read_to_string(&request_path)?;
    let request: Value =
        serde_json::from_str(&raw).map_err(|e| anyhow!("request {id} is malformed: {e}"))?;
    let mut effects: Vec<Value> = Vec::new();
    let surface = request
        .get("target_surface")
        .and_then(Value::as_str)
        .unwrap_or("");
    let target = request
        .get("target_scope")
        .and_then(Value::as_str)
        .unwrap_or("");
    match (surface, target, approve) {
        ("calendar", hold_id, true) if !hold_id.is_empty() => {
            if let Some(promotion) = request
                .get("intent")
                .and_then(|intent| intent.get("promotion"))
                .filter(|value| !value.is_null())
            {
                let connector = promotion
                    .get("connector")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if connector != "apple_calendar" {
                    return Err(anyhow!(
                        "unsupported approved calendar connector: {connector}"
                    ));
                }
                let calendar = promotion
                    .get("calendar")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("approved Apple promotion has no calendar target"))?;
                let work_id = request
                    .get("work_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("approved Apple promotion has no work_id"))?;
                effects.push(json!({
                    "surface": "calendar",
                    "target": hold_id,
                    "calendar": calendar,
                    "work_id": work_id,
                    "change": format!("create event in Apple Calendar calendar {calendar:?}"),
                    "kind": "apple_calendar_create",
                }));
            }
            effects.push(json!({
                "surface": "calendar",
                "target": hold_id,
                "change": "hold.status: draft -> confirmed",
                "kind": "hold_confirm",
            }));
        }
        ("calendar", hold_id, false) if !hold_id.is_empty() => {
            effects.push(json!({
                "surface": "calendar",
                "target": hold_id,
                "change": "hold dropped (was draft)",
                "kind": "hold_drop",
            }));
        }
        ("calendar_plan", plan_id, true) if !plan_id.is_empty() => {
            let intent = request.get("intent").cloned().unwrap_or(Value::Null);
            let work_id = request
                .get("work_id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("approved calendar plan has no work_id"))?;
            effects.push(json!({
                "surface": "calendar_plan",
                "target": plan_id,
                "change": crate::cmd::calendar_plan::effect_summary(&intent),
                "kind": "apple_calendar_plan_apply",
                "work_id": work_id,
                "intent": intent,
            }));
        }
        ("calendar_plan", plan_id, false) if !plan_id.is_empty() => {
            effects.push(json!({
                "surface": "calendar_plan",
                "target": plan_id,
                "change": "staged plan changes discarded (nothing written)",
                "kind": "calendar_plan_discard",
            }));
        }
        ("mail", message_key, true) if !message_key.is_empty() => {
            effects.push(json!({
                "surface": "mail",
                "target": message_key,
                "change": "reply draft -> local outbox (manual send; nothing is sent)",
                "kind": "mail_outbox_stage",
                "intent": request.get("intent").cloned().unwrap_or(Value::Null),
            }));
        }
        ("mail", message_key, false) if !message_key.is_empty() => {
            effects.push(json!({
                "surface": "mail",
                "target": message_key,
                "change": "suggestion dismissed (dismissal receipt)",
                "kind": "mail_suggestion_dismiss",
            }));
        }
        _ => {}
    }
    Ok(json!(effects))
}

/// Apply the planned effects. Returns a per-effect summary of what
/// actually happened (used for the JSON and human output).
fn apply_effects(id: &str, plan: &Value, approve: bool) -> Result<Value> {
    let mut applied: Vec<Value> = Vec::new();
    for effect in plan.as_array().into_iter().flatten() {
        let kind = effect.get("kind").and_then(Value::as_str).unwrap_or("");
        let target = effect.get("target").and_then(Value::as_str).unwrap_or("");
        match (kind, approve) {
            ("apple_calendar_create", true) => {
                let calendar = effect
                    .get("calendar")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Apple Calendar effect has no calendar target"))?;
                let work_id = effect
                    .get("work_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Apple Calendar effect has no work_id"))?;
                let external_event =
                    crate::cmd::calendar::promote_hold_to_apple(target, calendar, id, work_id)?;
                applied.push(json!({
                    "kind": "apple_calendar_create",
                    "summary": format!(
                        "{} -> Apple Calendar {} ({})",
                        target,
                        calendar,
                        external_event.get("external_id").and_then(Value::as_str).unwrap_or("?")
                    ),
                    "external_event": external_event,
                }));
            }
            ("apple_calendar_plan_apply", true) => {
                let work_id = effect
                    .get("work_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("calendar plan effect has no work_id"))?;
                let intent = effect.get("intent").cloned().unwrap_or(Value::Null);
                let receipt = crate::cmd::calendar_plan::apply_approved(id, work_id, &intent)?;
                applied.push(json!({
                    "kind": "apple_calendar_plan_apply",
                    "summary": crate::cmd::calendar_plan::effect_summary(&intent),
                    "receipt_id": receipt.get("receipt_id").cloned().unwrap_or(Value::Null),
                }));
            }
            ("calendar_plan_discard", false) => {
                applied.push(json!({
                    "kind": "calendar_plan_discard",
                    "summary": format!("{target}: staged plan changes discarded"),
                }));
            }
            ("hold_confirm", true) => {
                let hold = crate::cmd::calendar::update_hold_status(target, "confirmed", id)?;
                applied.push(json!({
                    "kind": "hold_confirm",
                    "summary": format!("{} -> status=confirmed", target),
                    "hold": hold,
                }));
            }
            ("hold_drop", false) => {
                crate::cmd::calendar::drop_draft_hold(target, id)?;
                applied.push(json!({
                    "kind": "hold_drop",
                    "summary": format!("{} dropped (was draft)", target),
                }));
            }
            ("mail_outbox_stage", true) => {
                let intent = effect.get("intent").cloned().unwrap_or(Value::Null);
                let entry = crate::cmd::mail::stage_outbox_draft(id, &intent)?;
                applied.push(json!({
                    "kind": "mail_outbox_stage",
                    "summary": format!(
                        "reply draft staged to outbox for {} (manual send)",
                        entry.get("to").and_then(Value::as_str).unwrap_or("?")
                    ),
                    "outbox_entry": entry,
                }));
            }
            ("mail_suggestion_dismiss", false) => {
                crate::cmd::mail::dismiss_suggestion(id, target)?;
                applied.push(json!({
                    "kind": "mail_suggestion_dismiss",
                    "summary": format!("suggestion for {} dismissed", target),
                }));
            }
            _ => {}
        }
    }
    Ok(json!(applied))
}

fn summarize_effects(plan: &Value) -> String {
    let n = plan.as_array().map(|a| a.len()).unwrap_or(0);
    match n {
        0 => "none".to_string(),
        1 => "1 effect".to_string(),
        _ => format!("{n} effects"),
    }
}

/// Verified request IDs for runtime summaries and event consumers.
pub(crate) fn scan_verified_decision_ids_in(dir: &Path) -> std::collections::HashSet<String> {
    let key = DecisionKey::load_default().ok();
    scan_verified_decision_ids_with_key(dir, key.as_ref())
}

pub(crate) fn scan_verified_decision_ids_with_key(
    dir: &Path,
    key: Option<&DecisionKey>,
) -> std::collections::HashSet<String> {
    scan_decisions_in(dir, key)
        .into_iter()
        .filter(|decision| decision["record_integrity"] == "verified")
        .filter_map(|decision| decision["id"].as_str().map(str::to_string))
        .collect()
}

pub(crate) fn scan_pending_requests() -> Vec<Value> {
    scan_pending_requests_in(&requests_dir(), &decisions_dir())
}

pub(crate) fn scan_pending_requests_in(requests: &Path, decisions: &Path) -> Vec<Value> {
    let key = DecisionKey::load_default().ok();
    pending_request_entries_in(requests, decisions, key.as_ref())
        .into_iter()
        .map(|(_, request)| request)
        .collect()
}

/// Pending request files as `(file stem, parsed request)`. The stem is the
/// key `show` and decisions resolve; the parsed content is untrusted data.
fn pending_request_entries_in(
    requests: &Path,
    decisions: &Path,
    key: Option<&DecisionKey>,
) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(requests) else {
        return out;
    };
    let decided = scan_verified_decision_ids_with_key(decisions, key);
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if decided.contains(&stem) {
            continue;
        }
        let raw = fs::read_to_string(&path).unwrap_or_default();
        let mut value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({"raw": raw}));
        if value.get("id").is_none() {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("id".to_string(), Value::String(stem.clone()));
            }
        }
        out.push((stem, value));
    }
    out
}

pub(crate) fn approval_request_summary(req: &Value) -> Value {
    let id = string_field(req, &["id", "request_id"]).unwrap_or_else(|| "?".to_string());
    let action = string_field(req, &["action"]).unwrap_or_else(|| "?".to_string());
    let target = string_field(req, &["target"]).unwrap_or_else(|| {
        match (
            string_field(req, &["target_surface"]),
            string_field(req, &["target_scope"]),
        ) {
            (Some(surface), Some(scope)) => format!("{surface}:{scope}"),
            (Some(surface), None) => surface,
            (None, Some(scope)) => scope,
            (None, None) => "?".to_string(),
        }
    });
    let risk = string_field(req, &["risk_tier", "risk", "requested_mode"])
        .unwrap_or_else(|| "?".to_string());
    let requested_at = string_field(req, &["requested_at", "requested_at_utc", "created_at"]);

    json!({
        "id": id,
        "action": action,
        "target": target,
        "risk": risk,
        "requested_at": requested_at,
    })
}

pub(crate) fn pending_approvals_summary_payload() -> Value {
    let pending = scan_pending_requests();
    let pending_summary: Vec<Value> = pending.iter().map(approval_request_summary).collect();
    json!({
        "pending_count": pending_summary.len(),
        "pending": pending_summary,
        "requests_dir": requests_dir().display().to_string(),
        "decisions_dir": decisions_dir().display().to_string(),
    })
}

fn string_field(value: &Value, fields: &[&str]) -> Option<String> {
    fields.iter().find_map(|field| {
        value
            .get(*field)
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn scan_decisions_in(dir: &Path, key: Option<&DecisionKey>) -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        let (mut value, status) = match heiwa_core::integrity::read_decision_record(&path) {
            Ok(value) => {
                let status = authority::classify(key, stem, &value).as_str();
                (value, status)
            }
            Err(_) => (json!({"id": stem}), "unreadable_or_malformed"),
        };
        if !value.is_object() {
            value = json!({"id": stem, "record": value});
        }
        value["record_integrity"] = json!(status);
        value["record_path"] = json!(path.display().to_string());
        out.push(value);
    }
    out
}

fn dispatch_dir() -> PathBuf {
    crate::home::heiwa_state_dir().join("dispatch")
}

pub(crate) fn requests_dir() -> PathBuf {
    dispatch_dir().join("requests")
}

/// Used by staging surfaces: file existence alone is not a decided request.
pub(crate) fn decision_is_verified(id: &str) -> bool {
    if validate_request_id(id).is_err() {
        return false;
    }
    let key = DecisionKey::load_default().ok();
    let path = decisions_dir().join(format!("{id}.json"));
    heiwa_core::integrity::read_decision_record(&path)
        .is_ok_and(|record| authority::classify(key.as_ref(), id, &record) == Integrity::Verified)
}

pub(crate) fn decisions_dir() -> PathBuf {
    dispatch_dir().join("approvals").join("decisions")
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
    println!("heiwa approvals");
    println!();
    println!("Usage:");
    println!("  heiwa approvals list [--json]");
    println!("  heiwa approvals show <id> [--json]");
    println!("  heiwa approvals decide <id> --approve|--deny [--note ...] [--dry-run] [--json]");
    println!();
    println!("Reads from ~/.heiwa/state/dispatch/requests/ and writes decisions to");
    println!("~/.heiwa/state/dispatch/approvals/decisions/.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decision_lease_is_released_on_drop_even_if_a_duplicate_handle_survives() {
        // A child spawned while the lease is held can briefly share its open
        // file description (fork before exec). flock belongs to the
        // description, so closing only the owner's handle would leave the
        // lock held and make the next decision report "already in progress".
        let dir = tempfile::tempdir().expect("temp decision dir");
        let path = dir.path().join(".req_123.lock");
        let lease = DecisionLease::acquire_at(&path).expect("first lease");
        let inherited = lease.duplicate_handle_for_test();

        drop(lease);
        let next = DecisionLease::acquire_at(&path)
            .expect("the lease is released even while a duplicate handle survives");

        // Mutual exclusion still holds for the new owner.
        let blocked = DecisionLease::acquire_at(&path).expect_err("second lease must be blocked");
        assert!(blocked.to_string().contains("already in progress"));
        drop(next);
        drop(inherited);
    }

    #[test]
    fn pending_scans_require_integrity_and_the_same_file_stem() {
        let (root, key) = authority::tests::keyed_root();
        let requests = root.path().join("requests");
        let decisions = root.path().join("decisions");
        fs::create_dir(&requests).unwrap();
        fs::create_dir(&decisions).unwrap();
        for id in ["req_a", "req_b"] {
            fs::write(
                requests.join(format!("{id}.json")),
                json!({"id":id}).to_string(),
            )
            .unwrap();
        }
        let mut record = json!({"id":"req_a", "outcome":"approved"});
        authority::seal(&key, &mut record).unwrap();
        fs::write(decisions.join("req_b.json"), record.to_string()).unwrap();
        assert_eq!(
            pending_request_entries_in(&requests, &decisions, Some(&key)).len(),
            2,
            "renamed record cannot hide another request"
        );
        fs::write(decisions.join("req_a.json"), record.to_string()).unwrap();
        let pending = pending_request_entries_in(&requests, &decisions, Some(&key));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, "req_b");
        assert_eq!(
            pending_request_entries_in(&requests, &decisions, None).len(),
            2,
            "no key never closes pending requests"
        );
        record["outcome"] = json!("denied");
        fs::write(decisions.join("req_a.json"), record.to_string()).unwrap();
        assert_eq!(
            pending_request_entries_in(&requests, &decisions, Some(&key)).len(),
            2,
            "tampering never closes pending requests"
        );
    }

    #[test]
    fn decision_publication_never_overwrites_an_existing_record() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("req_a.json");
        fs::write(&path, "historical-unverified-record").unwrap();
        assert!(write_decision_atomic(&path, &json!({"id":"req_a"})).is_err());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "historical-unverified-record"
        );
        assert_eq!(
            fs::read_dir(directory.path()).unwrap().count(),
            1,
            "temporary file cleaned after collision"
        );
    }

    #[test]
    fn list_hints_use_validated_stems_not_request_content() {
        let entries = vec![
            ("req_evil".to_string(), json!({"id": "x; touch /tmp/pwned"})),
            ("bad id".to_string(), json!({})),
            ("--json".to_string(), json!({})),
            ("req_ok".to_string(), json!({"request_id": "req_ok"})),
        ];
        assert_eq!(
            list_next(&entries),
            vec![
                "heiwa approvals show req_evil".to_string(),
                "heiwa approvals show req_ok".to_string()
            ]
        );
    }

    #[test]
    fn pending_entries_keep_the_file_stem_and_the_scan_output_is_unchanged() {
        let root = tempfile::tempdir().expect("root");
        let requests = root.path().join("requests");
        let decisions = root.path().join("decisions");
        fs::create_dir_all(&requests).expect("requests");
        fs::create_dir_all(&decisions).expect("decisions");
        fs::write(requests.join("req_a.json"), r#"{"id": "content-id"}"#).expect("a");
        fs::write(requests.join("req_b.json"), r#"{"action": "x"}"#).expect("b");

        let mut entries = pending_request_entries_in(&requests, &decisions, None);
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        let stems: Vec<&str> = entries.iter().map(|(stem, _)| stem.as_str()).collect();
        assert_eq!(stems, vec!["req_a", "req_b"]);

        let mut scanned = scan_pending_requests_in(&requests, &decisions);
        scanned.sort_by_key(|value| value["id"].as_str().unwrap_or("").to_string());
        assert_eq!(scanned[0]["id"], "content-id", "content id kept as data");
        assert_eq!(
            scanned[1]["id"], "req_b",
            "stem fills a missing id, as before"
        );
    }

    #[test]
    fn hintable_ids_reject_shell_syntax_and_flag_lookalikes() {
        assert!(is_hintable_request_id("req_800eab706401"));
        assert!(!is_hintable_request_id("x; touch /tmp/pwned"));
        assert!(!is_hintable_request_id("--json"));
        assert!(!is_hintable_request_id(""));
    }

    #[test]
    fn approval_decision_lease_excludes_a_second_process_handle() {
        let dir = tempfile::tempdir().expect("temp decision dir");
        let path = dir.path().join(".req_123.lock");
        let first = DecisionLease::acquire_at(&path).expect("first lease");

        let second = DecisionLease::acquire_at(&path).expect_err("second lease must be blocked");

        assert!(second.to_string().contains("already in progress"));
        drop(first);
        DecisionLease::acquire_at(&path).expect("lease becomes available after drop");
    }

    #[test]
    fn approval_summary_maps_dispatch_v1_schema() {
        let request = json!({
            "schema_version": "operator_dispatch_request_v1",
            "request_id": "req_123",
            "created_at": "2026-03-30T18:20:22.112520Z",
            "action": "write-file",
            "target_surface": "filesystem",
            "target_scope": "/Users/example/.gemini/settings.json",
            "requested_mode": "write"
        });

        let summary = approval_request_summary(&request);

        assert_eq!(summary["id"], "req_123");
        assert_eq!(summary["action"], "write-file");
        assert_eq!(
            summary["target"],
            "filesystem:/Users/example/.gemini/settings.json"
        );
        assert_eq!(summary["risk"], "write");
        assert_eq!(summary["requested_at"], "2026-03-30T18:20:22.112520Z");
    }
}
