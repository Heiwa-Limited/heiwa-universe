//! One replayable, read-only repository-analysis episode through the real
//! Work-scoped operator runner, bound to exact fixture, input, and output
//! digests, with a machine-checked label appended at the output digest.
//!
//! Harness smoke only. A scripted provider proves wiring and is recorded as
//! `synthetic`; a local-model episode is one real bounded workflow run.
//! Neither is evidence of model quality, routing advantage, desktop
//! certification, or Jev validation.
//!
//! The turn is built like a production Work model turn and goes through the
//! same runner boundary, which teaches the tool-call protocol. The binary's
//! working-context preamble is not sent; the episode record says so.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use heiwa_core::drex::{
    CallRisk, ModelCallCandidate, ModelCallRequest, ModelCallStage, PrivacyClass, SafetyClass,
};
use heiwa_evidence::{
    find_sensitive, now_iso, EvidenceTransport, JsonlTransport, OperatorEventType,
};
use heiwa_protocol::{ExecutionScope, RiskClass, ToolLease};
use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent};
use heiwa_session::operator::{OperatorSessionService, StartTurnRequest};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::operator::{
    OperatorModelTurn, OperatorTurnHandle, OperatorTurnRunner, OperatorTurnWork,
};
use crate::system1_shadow::{attempt_cost, AttemptCost, EpisodeCost};

pub const EVAL_STREAM: &str = "routing_eval";
pub const EVAL_SCHEMA: &str = "heiwa.routing_eval.v1";

/// The task, its fixture, and its rubric are versioned together: changing
/// any of them is a new version, never an edit.
pub const TASK_ID: &str = "repo-analysis/retry-functions";
pub const TASK_VERSION: u32 = 1;
/// Inspected while the harness was built, so never an untouched holdout.
pub const TASK_SPLIT: &str = "development";
pub const RUBRIC: &str = "exact-function-set@1";

pub const TASK_PROMPT: &str = "The repository to analyse is the current directory. List every Rust function defined in it whose name starts with `retry_`, with the file that defines it. Reply with JSON only, in exactly this shape: {\"functions\": [{\"name\": \"<function name>\", \"file\": \"<path relative to the repository root>\"}]}";

pub const FIXTURE: [(&str, &str); 5] = [
    (
        "README.md",
        "# ledger-lite\n\nA small bookkeeping library. Transfers are retried with backoff\n(see `src/retry.rs`). The retry_budget setting from older notes was removed.\n",
    ),
    ("src/lib.rs", "pub mod ledger;\npub mod retry;\n"),
    (
        "src/retry.rs",
        "/// Attempts before a transfer is abandoned.\npub const MAX_ATTEMPTS: u32 = 4;\n\n/// Backoff before `attempt` (0-based), doubling from 250 ms.\npub fn retry_delay_ms(attempt: u32) -> u64 {\n    250 * 2u64.pow(attempt)\n}\n\n/// Whether another attempt is allowed after `attempt` failures.\npub fn retry_allowed(attempt: u32) -> bool {\n    attempt < MAX_ATTEMPTS\n}\n",
    ),
    (
        "src/ledger.rs",
        "use crate::retry::{retry_allowed, retry_delay_ms};\n\npub struct Entry {\n    pub account: String,\n    pub cents: i64,\n}\n\n/// Post an entry, retrying transient failures.\npub fn post_entry(entry: &Entry) -> Result<(), String> {\n    retry_post(entry, 0)\n}\n\nfn retry_post(entry: &Entry, attempt: u32) -> Result<(), String> {\n    if entry.cents == 0 {\n        return Err(\"empty entry\".into());\n    }\n    if !retry_allowed(attempt) {\n        return Err(format!(\"gave up after {attempt} attempts\"));\n    }\n    let _wait = retry_delay_ms(attempt);\n    Ok(())\n}\n",
    ),
    (
        "tests/retry_test.rs",
        "use ledger_lite::retry::retry_delay_ms;\n\n#[test]\nfn retry_delay_doubles() {\n    assert_eq!(retry_delay_ms(1), 2 * retry_delay_ms(0));\n}\n",
    ),
];

/// The accepted answer: every `fn retry_*` definition and the file defining
/// it. Mentions, imports, and calls are not definitions.
pub const EXPECTED: [(&str, &str); 4] = [
    ("retry_allowed", "src/retry.rs"),
    ("retry_delay_doubles", "tests/retry_test.rs"),
    ("retry_delay_ms", "src/retry.rs"),
    ("retry_post", "src/ledger.rs"),
];

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// A file tree as sorted relative paths, each with its content digest.
fn tree_digest(files: &[(String, Vec<u8>)]) -> String {
    let mut entries: Vec<(&str, String)> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), sha256(bytes)))
        .collect();
    entries.sort();
    sha256(&serde_json::to_vec(&entries).unwrap_or_default())
}

pub fn fixture_digest() -> String {
    let files: Vec<(String, Vec<u8>)> = FIXTURE
        .iter()
        .map(|(path, content)| (path.to_string(), content.as_bytes().to_vec()))
        .collect();
    tree_digest(&files)
}

fn read_tree(root: &Path, dir: &Path, files: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            read_tree(root, &path, files)?;
        } else {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, std::fs::read(&path)?));
        }
    }
    Ok(())
}

/// Write the fixture into an empty directory, then verify what is on disk —
/// every file and nothing else — against the fixture digest.
pub fn materialize(dir: &Path) -> Result<String> {
    if std::fs::read_dir(dir)?.next().is_some() {
        bail!("the fixture directory is not empty");
    }
    for (path, content) in FIXTURE {
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, content)?;
    }
    let mut files = Vec::new();
    read_tree(dir, dir, &mut files)?;
    let on_disk = tree_digest(&files);
    if on_disk != fixture_digest() {
        bail!("the fixture on disk does not match its digest");
    }
    Ok(on_disk)
}

/// Nothing writable, network denied, and only the read tools the runner's
/// tool protocol names.
pub fn read_only_scope(dir: &Path) -> ExecutionScope {
    let mut scope = ExecutionScope::local_default(dir.to_path_buf());
    scope.writable_dirs.clear();
    for name in crate::agentic::TOOL_PROTOCOL_TOOLS {
        scope.tool_leases.push(ToolLease {
            name: name.to_string(),
            risk_class: RiskClass::HostSafeReadonly,
            allowed: true,
        });
    }
    scope
}

/// Digests of the exact message lists a provider received, one per stage.
#[derive(Clone, Default)]
pub struct Deliveries(Arc<Mutex<Vec<String>>>);

impl Deliveries {
    pub fn digests(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|digests| digests.clone())
            .unwrap_or_default()
    }
}

/// Wrap a provider so each message list it is sent is digested first. The
/// provider and what it receives are unchanged.
pub fn digesting(
    inner: Arc<dyn ProviderAdapter>,
    deliveries: Deliveries,
) -> Arc<dyn ProviderAdapter> {
    Arc::new(Digesting { inner, deliveries })
}

struct Digesting {
    inner: Arc<dyn ProviderAdapter>,
    deliveries: Deliveries,
}

#[async_trait]
impl ProviderAdapter for Digesting {
    async fn send(
        &self,
        model: &str,
        messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let digest = sha256(&serde_json::to_vec(messages).unwrap_or_default());
        if let Ok(mut digests) = self.deliveries.0.lock() {
            digests.push(digest);
        }
        self.inner.send(model, messages, stream_tx).await
    }

    async fn interrupt(&self) -> anyhow::Result<()> {
        self.inner.interrupt().await
    }

    fn supported_models(&self) -> Vec<String> {
        self.inner.supported_models()
    }
}

pub struct Setup {
    /// `synthetic` for a scripted provider, `live_local_smoke` for a model on
    /// this device.
    pub evidence_class: &'static str,
    pub work_id: String,
    pub thread_id: String,
    pub candidates: Vec<ModelCallCandidate>,
    pub timeout: Duration,
    pub model_identity: Value,
    pub harness_revision: Value,
}

/// An episode's record, not yet persisted, and the exact final answer the
/// operator journal holds for its turn.
pub struct Episode {
    pub record: Value,
    pub output: Option<String>,
}

/// Wait for the turn's terminal frame. A lagged receiver keeps waiting; the
/// journal, not the stream, is what the record is built from.
async fn wait_terminal(handle: &mut OperatorTurnHandle) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match handle.recv().await {
            Ok(frame) if frame.is_terminal() => return,
            Ok(_) | Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => return,
        }
    }
}

#[derive(Default)]
struct TurnFacts {
    terminal: Option<&'static str>,
    interrupt_reason: Option<String>,
    cancel_requested: bool,
    output: Option<String>,
    receipts: Vec<Value>,
    attempts: BTreeMap<(String, u64), Value>,
    costs: BTreeMap<(String, u64), Option<AttemptCost>>,
    tool_calls: Vec<Value>,
}

/// What the turn did, read back from the operator journal it wrote.
fn turn_facts(
    sessions: &OperatorSessionService,
    thread_id: &str,
    turn_id: &str,
) -> Result<TurnFacts> {
    let mut facts = TurnFacts::default();
    let mut cursor: Option<String> = None;
    loop {
        let page = sessions
            .events_after(thread_id, cursor.as_deref(), 256)
            .map_err(|error| anyhow!("{error}"))?;
        if page.events.is_empty() {
            break;
        }
        cursor = page.next_cursor;
        for row in page.events {
            let event = row.event;
            if event.turn_id.as_deref() != Some(turn_id) {
                continue;
            }
            let payload = &event.payload;
            let key = event.call_id.clone().zip(payload["attempt"].as_u64());
            match event.event_type {
                OperatorEventType::RouteAttempted => {
                    if let Some(key) = key {
                        facts.attempts.entry(key.clone()).or_insert_with(|| {
                            json!({
                                "call_id": key.0,
                                "attempt": key.1,
                                "provider": payload["provider"],
                                "model": payload["model"],
                                "outcome": "unended",
                            })
                        });
                        facts.costs.entry(key).or_insert(None);
                    }
                }
                OperatorEventType::RouteCompleted | OperatorEventType::RouteFailed => {
                    if let Some(key) = key {
                        let completed = event.event_type == OperatorEventType::RouteCompleted;
                        let cost = attempt_cost(payload);
                        let entry = facts
                            .attempts
                            .entry(key.clone())
                            .or_insert_with(|| json!({ "call_id": key.0, "attempt": key.1 }));
                        entry["outcome"] = json!(if completed { "completed" } else { "failed" });
                        entry["latency_ms"] = payload["latency_ms"].clone();
                        entry["failure_class"] = payload["failure_class"].clone();
                        entry["cost_truth"] = json!(truth_name(cost));
                        facts.costs.insert(key, Some(cost));
                    }
                }
                OperatorEventType::ToolCallCompleted => facts.tool_calls.push(json!({
                    "name": payload["name"],
                    "status": payload["status"],
                    "artifact": !payload["artifact_ref"].is_null(),
                })),
                OperatorEventType::AssistantCompleted => {
                    facts.output = payload["text"].as_str().map(str::to_string);
                }
                OperatorEventType::ReceiptLinked => facts.receipts.push(json!({
                    "receipt_ref": payload["receipt_ref"],
                    "provider": payload["provider"],
                    "model": payload["model"],
                })),
                OperatorEventType::TurnCompleted => facts.terminal = Some("completed"),
                OperatorEventType::TurnInterrupted => {
                    facts.terminal = Some("interrupted");
                    facts.interrupt_reason = payload["reason"].as_str().map(str::to_string);
                }
                OperatorEventType::Blocker => facts.terminal = Some("blocked"),
                OperatorEventType::TurnCancelRequested => facts.cancel_requested = true,
                _ => {}
            }
        }
    }
    Ok(facts)
}

fn truth_name(cost: AttemptCost) -> &'static str {
    match cost {
        AttemptCost::KnownZero => "known_zero",
        AttemptCost::Exact(_) => "exact",
        AttemptCost::Estimated(_) => "estimated",
        AttemptCost::Unknown => "unknown",
    }
}

/// Run the task once through `runner`, Work-scoped, in `workspace` (an empty
/// directory the fixture is written into). Nothing is persisted here; see
/// [`persist_episode`].
pub async fn run_episode(
    runner: &OperatorTurnRunner,
    sessions: &Arc<OperatorSessionService>,
    workspace: &Path,
    deliveries: &Deliveries,
    setup: Setup,
) -> Result<Episode> {
    let fixture = materialize(workspace)?;
    let work =
        heiwa_work::WorkId::parse(&setup.work_id).ok_or_else(|| anyhow!("invalid work id"))?;
    sessions.ensure_thread(&setup.thread_id)?;
    sessions.append_event(heiwa_work::work_created_event(
        &work,
        &setup.thread_id,
        TASK_ID,
        "routing-eval",
        &now_iso(),
        || format!("evt-{}", uuid::Uuid::new_v4()),
    ))?;
    let scope = read_only_scope(workspace);
    let turn = OperatorModelTurn {
        request: ModelCallRequest {
            thread_id: String::new(),
            turn_id: String::new(),
            work_id: Some(setup.work_id.clone()),
            call_id: format!("call-{}", uuid::Uuid::new_v4()),
            intent: "research".into(),
            stage: ModelCallStage::Execution,
            raw_text: TASK_PROMPT.into(),
            privacy: PrivacyClass::LocalOnly,
            risk: CallRisk::Low,
            safety: SafetyClass::low_risk_auto_approval(&CallRisk::Low),
            required_capabilities: vec![],
            required_context_tokens: 1,
            minimum_quality_class: 1,
            minimum_success_rate: 0.0,
            maximum_marginal_cost_usd: Some(0.0),
            preferred_provider: None,
            preferred_model: None,
            allowed_models: vec![],
            excluded_models: vec![],
        },
        candidates: setup.candidates.clone(),
        messages: vec![Message {
            role: Role::User,
            content: TASK_PROMPT.into(),
        }],
        remaining_budget_usd: Some(0.0),
        max_attempts: 1,
        tool_scope: Some(scope.clone()),
        done_payload: Arc::new(|_| json!({ "harness": "routing_eval" })),
    };
    let mut request = StartTurnRequest::auto(format!("eval-{}", uuid::Uuid::new_v4()), TASK_PROMPT);
    request.work_id = Some(setup.work_id.clone());

    let started = Instant::now();
    let mut handle = runner
        .submit(
            &setup.thread_id,
            request,
            OperatorTurnWork::Model(Box::new(turn)),
        )
        .map_err(|error| anyhow!("{error}"))?;
    let turn_id = handle.turn_id.clone();
    let timed_out = tokio::time::timeout(setup.timeout, wait_terminal(&mut handle))
        .await
        .is_err();
    if timed_out {
        runner.request_cancel(&turn_id)?;
        let _ = tokio::time::timeout(Duration::from_secs(15), wait_terminal(&mut handle)).await;
    }
    let latency_ms = started.elapsed().as_millis() as u64;

    let facts = turn_facts(sessions, &setup.thread_id, &turn_id)?;
    let mut cost = EpisodeCost::default();
    if facts.costs.is_empty() {
        // No provider was invoked: the executor announces every attempt first.
        cost.add(AttemptCost::KnownZero);
    }
    for outcome in facts.costs.values() {
        // An attempt with no recorded end may have been charged.
        cost.add(outcome.unwrap_or(AttemptCost::Unknown));
    }
    let calls: BTreeSet<&String> = facts.attempts.keys().map(|(call, _)| call).collect();
    let routes: BTreeSet<String> = facts
        .attempts
        .values()
        .map(|attempt| format!("{}/{}", attempt["provider"], attempt["model"]))
        .collect();
    let deliveries = deliveries.digests();
    let record = json!({
        "schema": EVAL_SCHEMA,
        "record": "episode",
        "episode_id": format!("episode-{}", uuid::Uuid::new_v4()),
        "recorded_at": now_iso(),
        "evidence_class": setup.evidence_class,
        "claims": {
            "establishes": "one bounded read-only Work-scoped runner episode with a machine-checked label",
            "not_evidence_of": ["model quality", "routing advantage", "desktop certification", "Jev validation"],
        },
        "task": {
            "id": TASK_ID,
            "version": TASK_VERSION,
            "split": TASK_SPLIT,
            "rubric": RUBRIC,
            "prompt_digest": sha256(TASK_PROMPT.as_bytes()),
        },
        "harness": {
            "revision": setup.harness_revision,
            "path": "OperatorTurnRunner::submit (Work-scoped model turn)",
            "tool_protocol": "taught by the runner (agentic::tool_instruction_prompt)",
            "working_context_prompt": "not sent: binary-only main.rs preamble",
            "max_attempts": 1,
            "tool_rounds": 1,
            "timeout_ms": setup.timeout.as_millis() as u64,
        },
        "digests": {
            "fixture": fixture,
            "input": deliveries.first(),
            "deliveries": deliveries,
            "output": facts.output.as_deref().map(|output| sha256(output.as_bytes())),
        },
        "work": { "work_id": setup.work_id, "thread_id": setup.thread_id, "turn_id": turn_id },
        "receipts": facts.receipts,
        "model": {
            "requested": setup.candidates.iter().map(|candidate| json!({
                "provider": candidate.tier.provider,
                "model": candidate.tier.provider_model_id,
            })).collect::<Vec<_>>(),
            "identity": setup.model_identity,
        },
        "candidates": setup.candidates.iter().map(|candidate| json!({
            "provider": candidate.tier.provider,
            "model": candidate.tier.model_id,
            "capability_class": candidate.tier.capability_class,
            "locality": format!("{:?}", candidate.locality),
            "cost_truth": format!("{:?}", candidate.cost_truth),
            "marginal_cost_usd": candidate.marginal_cost_usd,
        })).collect::<Vec<_>>(),
        "budget": { "turn_budget_usd": 0.0, "maximum_marginal_cost_usd": 0.0 },
        "constraints": {
            "privacy": "local_only",
            "network": format!("{:?}", scope.network_policy),
            "writable_dirs": scope.writable_dirs.len(),
            "tools": scope.tool_leases.iter().map(|lease| json!({
                "name": lease.name,
                "risk_class": lease.risk_class.as_str(),
            })).collect::<Vec<_>>(),
        },
        "episode": {
            "terminal": facts.terminal.unwrap_or(if timed_out { "timed_out" } else { "unknown" }),
            "timed_out": timed_out,
            "cancel_requested": facts.cancel_requested,
            "interrupt_reason": facts.interrupt_reason,
            "latency_ms": latency_ms,
            "model_calls": calls.len(),
            "attempts": facts.attempts.values().collect::<Vec<_>>(),
            "retries": facts.attempts.len().saturating_sub(calls.len()),
            "escalations": routes.len().saturating_sub(1),
            "repairs": 0,
            "tool_calls": facts.tool_calls,
        },
        "cost": {
            "execution": {
                "truth": truth_name(cost.truth()),
                "known_usd": cost.known_usd,
                "estimated_usd": cost.estimated_usd,
                "attempts": {
                    "known_zero": cost.known_zero,
                    "exact": cost.exact,
                    "estimated": cost.estimated,
                    "unknown": cost.unknown,
                },
            },
            "classifier": { "attached": false },
        },
    });
    Ok(Episode {
        record,
        output: facts.output,
    })
}

/// Append a record under `root` (the sessions' own evidence root). A record
/// that still matches the evidence plane's sensitive-pattern screen is
/// replaced by one that keeps only its identity; the screen is a defense,
/// not proof that nothing private remains.
fn append(root: &Path, record: Value) -> Result<Value> {
    let persisted = if find_sensitive(&record).is_some() {
        json!({
            "schema": EVAL_SCHEMA,
            "record": record["record"],
            "recorded_at": now_iso(),
            "episode_id": record["episode_id"],
            "status": "withheld",
            "withheld_reason": "sensitive_match",
        })
    } else {
        record
    };
    JsonlTransport::new(root.to_path_buf())?.journal(EVAL_STREAM, persisted.clone())?;
    Ok(persisted)
}

pub fn persist_episode(root: &Path, record: Value) -> Result<Value> {
    append(root, record)
}

pub fn machine_reviewer(harness_revision: &Value) -> Value {
    json!({ "kind": "machine_rubric", "id": RUBRIC, "harness_revision": harness_revision })
}

/// Label `output` with the rubric and append the label at its digest. The
/// output must be the episode's own: a label never moves to other text.
pub fn append_label(
    root: &Path,
    episode: &Value,
    output: Option<&str>,
    reviewer: Value,
) -> Result<Value> {
    let output_digest = output.map(|output| sha256(output.as_bytes()));
    if episode["digests"]["output"] != json!(output_digest) {
        bail!("the labelled output is not the episode's output");
    }
    let verdict = check(output);
    append(
        root,
        json!({
            "schema": EVAL_SCHEMA,
            "record": "label",
            "label_id": format!("label-{}", uuid::Uuid::new_v4()),
            "recorded_at": now_iso(),
            "episode_id": episode["episode_id"],
            "output_digest": output_digest,
            "label": verdict["label"],
            "reason": verdict["reason"],
            "rubric": RUBRIC,
            "checks": verdict["checks"],
            "reviewer": reviewer,
            "supersedes": Value::Null,
        }),
    )
}

/// A fenced block's body, when the whole answer is one fenced block.
fn strip_fence(text: &str) -> Option<&str> {
    let inner = text.strip_prefix("```")?.strip_suffix("```")?;
    let body = match inner.split_once('\n') {
        Some((info, rest)) if info.trim().chars().all(|c| c.is_ascii_alphanumeric()) => rest,
        _ => inner,
    };
    Some(body.trim())
}

/// A pair as text only when both parts look like identifiers or paths;
/// anything else is shown by digest, never echoed.
fn render((name, file): &(String, String)) -> String {
    let plain = |text: &str, extra: &str| {
        !text.is_empty()
            && text.len() <= 96
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
    };
    if plain(name, "_") && plain(file, "_./-") {
        format!("{name} @ {file}")
    } else {
        let digest = sha256(format!("{name}\u{0}{file}").as_bytes());
        format!("<unrenderable {}>", &digest[7..19])
    }
}

/// Rubric `exact-function-set@1`, fixed before any live run: the trimmed
/// answer, optionally one surrounding code fence, must parse as JSON of the
/// shape `{"functions": [{"name", "file"}]}` whose (name, file) pairs, with
/// a leading `./` dropped, equal the expected set. No output is unjudged;
/// anything else fails. The answer is never repaired.
pub fn check(output: Option<&str>) -> Value {
    let verdict = |label: &str, reason: &str, checks: Vec<Value>| json!({ "label": label, "reason": reason, "checks": checks });
    let Some(text) = output else {
        return verdict(
            "unjudged",
            "no_output",
            vec![json!({ "id": "output_present", "passed": false })],
        );
    };
    let mut checks = vec![json!({ "id": "output_present", "passed": true })];
    let trimmed = text.trim();
    let (body, form) = match strip_fence(trimmed) {
        Some(body) => (body, "fenced"),
        None => (trimmed, "bare"),
    };
    let parsed = serde_json::from_str::<Value>(body).ok();
    checks.push(json!({ "id": "json_only", "passed": parsed.is_some(), "form": form }));
    let Some(parsed) = parsed else {
        return verdict("fail", "not_json", checks);
    };
    let pairs: Option<Vec<(String, String)>> = parsed["functions"].as_array().and_then(|items| {
        items
            .iter()
            .map(|item| {
                Some((
                    item["name"].as_str()?.trim().to_string(),
                    item["file"]
                        .as_str()?
                        .trim()
                        .trim_start_matches("./")
                        .to_string(),
                ))
            })
            .collect()
    });
    checks.push(json!({ "id": "shape", "passed": pairs.is_some() }));
    let Some(pairs) = pairs else {
        return verdict("fail", "wrong_shape", checks);
    };
    let observed: BTreeSet<(String, String)> = pairs.into_iter().collect();
    let expected: BTreeSet<(String, String)> = EXPECTED
        .iter()
        .map(|(name, file)| (name.to_string(), file.to_string()))
        .collect();
    let missing: Vec<String> = expected.difference(&observed).map(render).collect();
    let unexpected: Vec<String> = observed.difference(&expected).map(render).collect();
    let exact = missing.is_empty() && unexpected.is_empty();
    checks.push(json!({
        "id": "exact_set",
        "passed": exact,
        "expected": expected.len(),
        "observed": observed.len(),
        "missing": missing,
        "unexpected": unexpected,
    }));
    if exact {
        verdict("pass", "exact_set", checks)
    } else {
        verdict("fail", "wrong_set", checks)
    }
}
