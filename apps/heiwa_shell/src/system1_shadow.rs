//! Shadow System 1 judgment for Work-scoped operator turns.
//!
//! For each finished Work-scoped model turn, ask a System 1 backend two typed
//! questions about the user's prompt — which kind of work it is, and how much
//! capability it needs — then replay DREX's plan with the recommended floor.
//! The result is one `heiwa.system1_shadow.v1` record in the `system1_shadow`
//! evidence stream. Nothing here changes routing, execution, or a turn's
//! outcome: the judgment runs after the turn is terminal, in its own task.
//!
//! Rules the record makes auditable:
//!
//! - **Raise-only.** The counterfactual floor is `max(applied, recommended)`.
//!   An operator's floor is authority; a judgment may add cost but never
//!   remove a constraint. DREX's budget ceilings still apply to the replay.
//! - **Exact replay.** Baseline and counterfactual both run the executor's
//!   own `plan_model_call` over the same request, candidates, and remaining
//!   budget, so a difference is a difference in the floor and nothing else.
//! - **A remote backend gets only what policy allows.** It receives only
//!   `standard`-privacy prompts with no sensitive-pattern match (a pattern
//!   screen, not a detector of all private text). Requests never follow
//!   a redirect, and a local backend is never reached through a proxy, so the
//!   endpoint classified is the only one that can receive the prompt.
//! - **Nothing the provider says is persisted as text.** The record carries a
//!   digest and length of the prompt, errors as kind, status, and a message
//!   generated here, answers only as offered options and numbers, and the
//!   provider's model id only as provenance. That is what keeps provider and
//!   prompt text out; a final sensitive-pattern screen is a further defense
//!   that withholds any record still matching credential-shaped patterns.
//! - **A capped backend is labelled.** A structured-output fallback cannot
//!   reach the auto band, and the record says which backend answered.
//!
//! Design: `docs/superpowers/specs/2026-09-23-system1-shadow-judgment-design.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use heiwa_core::drex::{
    default_policy, plan_model_call, ModelCallCandidate, ModelCallRequest, PrivacyClass,
};
use heiwa_evidence::{find_sensitive, EvidenceTransport, JsonlTransport, OperatorEventType};
use heiwa_judgment::backend::{Backend, Evaluation, System1Client, TYPESAFE_DEFAULT_MODEL};
use heiwa_judgment::decode::model_provenance;
use heiwa_judgment::question::{Question, QuestionSet};
use heiwa_judgment::{gate_batch, Answer, Band, Policy, Thresholds};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::operator::{ShadowObserver, ShadowTurn};

pub const SHADOW_STREAM: &str = "system1_shadow";
pub const RECORD_SCHEMA: &str = "heiwa.system1_shadow.v1";
pub const REPORT_SCHEMA: &str = "heiwa.system1_shadow.report.v2";
pub const QUESTION_SET: &str = "turn-route-v1";
/// State sent to the backend: the prompt, bounded well inside TypeSafe's
/// 32k-token allowance for state plus the longest question.
pub const MAX_STATE_CHARS: usize = 8_000;
/// TypeSafe reports sub-second answers; three seconds is generous for a
/// shadow and still measures whether the fast path is fast.
pub const REMOTE_BUDGET_MS: u64 = 3_000;
/// A local structured-output model measured at 16-17s for six questions.
pub const LOCAL_BUDGET_MS: u64 = 60_000;
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";
/// The local model the TypeScript model doctor found suitable (no reasoning
/// before answering, honours `enum`).
pub const DEFAULT_LOCAL_MODEL: &str = "gemma4:latest";
const TYPESAFE_KEY_ENV: &str = "TYPESAFE_API_KEY";
const TYPESAFE_KEY_ACCOUNT: &str = "typesafe";

/// DREX intent keys, as `heiwa_protocol::Intent::as_drex_key` spells them.
const INTENTS: [(&str, &str); 7] = [
    (
        "chat",
        "A conversation, question, or explanation that changes no files or systems",
    ),
    (
        "build",
        "Writing, changing, fixing, or testing code, scripts, or configuration",
    ),
    (
        "deploy",
        "Shipping, releasing, publishing, or changing infrastructure, CI, or production",
    ),
    (
        "audit",
        "Reviewing, inspecting, linting, or scanning existing work for problems",
    ),
    (
        "research",
        "Finding, reading, comparing, or summarising information from sources",
    ),
    (
        "strategy",
        "Planning, prioritising, roadmaps, architecture, or design decisions",
    ),
    (
        "status_check",
        "Asking whether a system, service, or job is up, healthy, or finished",
    ),
];

/// Capability classes 1-5, low to high. Level `i` is class `i + 1`.
const CAPABILITY_LEVELS: [&str; 5] = [
    "Trivial: a greeting, a one-line fact, or a mechanical rewrite of given text",
    "Simple: a short answer or a small, fully specified change",
    "Moderate: several steps, an ordinary change in one place, or a careful explanation",
    "Hard: design trade-offs, a change across several files, or careful debugging",
    "Frontier: novel architecture, subtle correctness or security reasoning, or long-horizon planning",
];

/// Question set `turn-route-v1`: one judgment per question, stated literally
/// (TypeSafe's jev-1.13 limitations: literal reading, no hidden second
/// judgment, no context the question does not need).
pub fn turn_route_questions() -> QuestionSet {
    QuestionSet::new(vec![
        (
            "intent".to_string(),
            Question::choice("Which kind of work is this request asking for?", INTENTS)
                .expect("static intent question is valid"),
        ),
        (
            "capability".to_string(),
            Question::score(
                "How capable must a model be to complete this request well on the first attempt?",
                CAPABILITY_LEVELS,
            )
            .expect("static capability question is valid"),
        ),
    ])
    .expect("static question set is valid")
}

/// Bounds on what a judge may hold. Nothing past them waits: a refused or
/// expired turn is recorded without calling the backend, and a refusal that
/// cannot even be queued for recording is counted and reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Admission {
    /// Turns that may wait, each holding its prompt, while one is judged.
    /// At least 1; the judge holds at most `capacity + 1` prompts.
    pub capacity: usize,
    /// A turn that waited longer than this is recorded, not judged. Queue
    /// wait is budgeted here, apart from the backend's own `budget_ms`.
    pub max_wait: Duration,
    /// Records (metadata, never a prompt) that may wait to be written.
    /// At least 1. Past it, a refusal is counted rather than queued.
    pub refusal_capacity: usize,
}

impl Default for Admission {
    fn default() -> Self {
        Admission {
            capacity: 8,
            max_wait: Duration::from_secs(600),
            refusal_capacity: 64,
        }
    }
}

impl Admission {
    fn validate(&self) -> Result<()> {
        if self.capacity == 0 {
            return Err(anyhow!("admission capacity must be at least 1"));
        }
        if self.refusal_capacity == 0 {
            return Err(anyhow!("admission refusal_capacity must be at least 1"));
        }
        if self.max_wait.is_zero() {
            return Err(anyhow!("admission max_wait must be greater than zero"));
        }
        Ok(())
    }
}

/// Judges finished Work-scoped turns in the background and records them.
#[derive(Clone)]
pub struct ShadowJudge {
    inner: Arc<Inner>,
}

/// What the judge handles own. The admission sender lives here and nowhere
/// else, so dropping the last `ShadowJudge` closes admission: the judging
/// task drains it and exits, which closes the record queue, and the writer
/// drains that and exits. Neither task holds a `ShadowJudge`.
struct Inner {
    shared: Arc<Shared>,
    queues: std::sync::OnceLock<Queues>,
}

struct Queues {
    admitted: tokio::sync::mpsc::Sender<Admitted>,
    records: tokio::sync::mpsc::Sender<Value>,
    writer: tokio::task::AbortHandle,
}

/// Everything the worker needs, and nothing that keeps the queues open.
struct Shared {
    /// The backend with its transport policy (no redirects; no proxy for a
    /// local backend). Nothing else sends a judgment request.
    system1: System1Client,
    budget: Duration,
    questions: QuestionSet,
    journal: JsonlTransport,
    admission: Admission,
    /// Refusals that could not be queued for recording since the last
    /// record was written. The next record carries the count.
    unrecorded: std::sync::atomic::AtomicU64,
}

/// A finished turn waiting for its judgment.
struct Admitted {
    turn: ShadowTurn,
    admitted_at: std::time::Instant,
}

impl ShadowJudge {
    pub fn new(backend: Backend, budget: Duration, evidence_dir: PathBuf) -> Result<Self> {
        Ok(ShadowJudge {
            inner: Arc::new(Inner {
                shared: Arc::new(Shared {
                    system1: System1Client::new(backend).map_err(|error| anyhow!(error))?,
                    budget,
                    questions: turn_route_questions(),
                    journal: JsonlTransport::new(evidence_dir)?,
                    admission: Admission::default(),
                    unrecorded: std::sync::atomic::AtomicU64::new(0),
                }),
                queues: std::sync::OnceLock::new(),
            }),
        })
    }

    /// Replace the admission bounds. Only before the judge is shared or has
    /// observed a turn; invalid bounds are refused, never silently adjusted.
    pub fn with_admission(self, admission: Admission) -> Result<Self> {
        admission.validate()?;
        let inner = Arc::try_unwrap(self.inner)
            .map_err(|_| anyhow!("with_admission must precede sharing the judge"))?;
        if inner.queues.get().is_some() {
            return Err(anyhow!(
                "with_admission must precede the first observed turn"
            ));
        }
        let mut shared = Arc::try_unwrap(inner.shared)
            .map_err(|_| anyhow!("with_admission must precede sharing the judge"))?;
        shared.admission = admission;
        Ok(ShadowJudge {
            inner: Arc::new(Inner {
                shared: Arc::new(shared),
                queues: std::sync::OnceLock::new(),
            }),
        })
    }

    /// The configured judge, or `None` when shadow judgment is off.
    pub fn from_config(config: &heiwa_config::AppConfig) -> Result<Option<Self>> {
        let settings = &config.system1;
        if !settings.shadow {
            return Ok(None);
        }
        let backend = match settings.backend.as_str() {
            "typesafe" => {
                let model = settings
                    .model
                    .clone()
                    .unwrap_or_else(|| TYPESAFE_DEFAULT_MODEL.to_string());
                // A missing key is not fatal here: every judgment then records
                // `not_configured`, which the report shows, rather than the
                // experiment silently switching itself off.
                let key = typesafe_key();
                match &settings.base_url {
                    Some(url) => Backend::typesafe_at(url, key, model),
                    None => Backend::typesafe(key, model),
                }
            }
            "ollama" => Backend::structured_llm(
                settings
                    .base_url
                    .clone()
                    .or_else(|| config.embedding.ollama_url.clone())
                    .unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string()),
                settings
                    .model
                    .clone()
                    .unwrap_or_else(|| DEFAULT_LOCAL_MODEL.to_string()),
            ),
            other => {
                return Err(anyhow!(
                    "unknown [system1] backend {other:?}; expected \"typesafe\" or \"ollama\""
                ))
            }
        };
        let default_budget = if backend.is_remote() {
            REMOTE_BUDGET_MS
        } else {
            LOCAL_BUDGET_MS
        };
        let budget = Duration::from_millis(settings.budget_ms.unwrap_or(default_budget));
        Self::new(backend, budget, config.paths.evidence_dir.clone()).map(Some)
    }

    /// Judge one turn. Always returns a record; a skip or failure is data.
    pub async fn judge(&self, turn: &ShadowTurn) -> Value {
        self.inner.shared.judge(turn).await
    }

    /// Append one record to the `system1_shadow` stream. Synchronous: it
    /// takes the stream's append lock. The observer path never calls it on
    /// the turn's task; its worker writes on the blocking pool.
    pub fn record(&self, record: &Value) -> Result<()> {
        self.inner.shared.append(record.clone())
    }

    /// The writer task's handle, once a turn has been observed. It finishes
    /// last: after the last judge is dropped, admitted turns are judged or
    /// expired, and every queued record is written.
    pub fn worker(&self) -> Option<tokio::task::AbortHandle> {
        self.inner.queues.get().map(|queues| queues.writer.clone())
    }
}

impl Shared {
    async fn judge(&self, turn: &ShadowTurn) -> Value {
        let (mut record, planned) = self.base_record(turn);
        let applied_floor = planned.minimum_quality_class;
        let (state, _) = bounded(turn.request.raw_text.as_str(), MAX_STATE_CHARS);

        if let Some(reason) = self.skip_reason(&planned) {
            record["status"] = json!("skipped");
            record["skip_reason"] = json!(reason);
            return record;
        }

        let evaluation = self
            .system1
            .evaluate(&state, &self.questions, self.budget)
            .await;
        record["call"] = call_summary(&evaluation);
        let Ok(decoded) = evaluation.outcome else {
            record["status"] = json!("failed");
            return record;
        };
        record["status"] = json!("judged");

        // Intent informs; capability is the judgment with a routing
        // consequence, so it alone gates. An unanswered gating question is
        // quarantine, never permission.
        let informational = Policy {
            informational: true,
            ..Policy::default()
        };
        let gate = gate_batch(&decoded.answers, &[("intent".to_string(), informational)]);
        let capability_missing = decoded.answer("capability").is_none();
        record["gate"] = if capability_missing {
            json!({ "band": Band::Quarantine, "blockers": ["capability"], "reason": "incomplete" })
        } else {
            json!({ "band": gate.band, "blockers": gate.blockers })
        };
        record["judgments"] = Value::Array(
            gate.decisions
                .iter()
                .map(|(id, decision)| {
                    json!({
                        "id": id,
                        "answer": decoded.answer(id).map(answer_summary),
                        "confidence": decision.confidence,
                        "confidence_source": decision.confidence_source,
                        "margin": decision.margin,
                        "band": decision.band,
                        "reason": decision.reason,
                    })
                })
                .collect(),
        );

        let intent = match decoded.answer("intent") {
            Some(Answer::Choice { choice, .. }) => Some(choice.clone()),
            _ => None,
        };
        let floor = match decoded.answer("capability") {
            Some(Answer::Score { probabilities, .. }) => {
                Some(most_probable_level(probabilities) as u8 + 1)
            }
            _ => None,
        };
        record["recommendation"] = json!({ "intent": intent, "minimum_quality_class": floor });

        let mut same_model = Value::Null;
        if let Some(floor) = floor {
            let raised = floor.max(applied_floor);
            let mut counterfactual = planned.clone();
            counterfactual.minimum_quality_class = raised;
            let replay = plan_summary(&counterfactual, &turn.candidates);
            same_model =
                json!(record["baseline"]["plan"]["selected"]["id"] == replay["selected"]["id"]);
            record["counterfactual"] = json!({ "minimum_quality_class": raised, "plan": replay });
        }
        record["agreement"] = json!({
            "intent": intent.as_ref().map(|intent| *intent == planned.intent),
            "floor_delta": floor.map(|floor| i16::from(floor) - i16::from(applied_floor)),
            "same_model": same_model,
        });
        record
    }

    /// The part of every record that needs no judgment: identity, policy,
    /// the prompt's digest, and the deterministic baseline plan. CPU only.
    fn base_record(&self, turn: &ShadowTurn) -> (Value, ModelCallRequest) {
        let prompt = turn.request.raw_text.as_str();
        let (_, truncated) = bounded(prompt, MAX_STATE_CHARS);
        // The executor narrows the ceiling to the remaining budget before it
        // plans; the replay must start from the same request.
        let mut planned = turn.request.clone();
        crate::model_calls::apply_remaining_budget(&mut planned, turn.remaining_budget_usd);
        let record = json!({
            "schema": RECORD_SCHEMA,
            "recorded_at": heiwa_evidence::now_iso(),
            "thread_id": turn.thread_id,
            "turn_id": turn.turn_id,
            "work_id": turn.work_id,
            "status": Value::Null,
            "policy": self.policy(),
            "input": {
                "chars": prompt.chars().count(),
                "digest": format!("sha256:{:x}", Sha256::digest(prompt.as_bytes())),
                "truncated": truncated,
            },
            "baseline": {
                "intent": planned.intent,
                "minimum_quality_class": planned.minimum_quality_class,
                "plan": plan_summary(&planned, &turn.candidates),
            },
            "call": Value::Null,
            "judgments": [],
            "gate": Value::Null,
            "recommendation": Value::Null,
            "counterfactual": Value::Null,
            "agreement": Value::Null,
        });
        (record, planned)
    }

    /// A turn admission refused or gave up on: recorded, never judged.
    fn not_admitted(&self, turn: &ShadowTurn, reason: &str, waited: Option<Duration>) -> Value {
        let (mut record, _) = self.base_record(turn);
        record["status"] = json!("skipped");
        record["skip_reason"] = json!(reason);
        record["admission"] = self.admission_json(waited);
        record
    }

    fn admission_json(&self, waited: Option<Duration>) -> Value {
        let admission = self.admission;
        json!({
            "capacity": admission.capacity,
            "refusal_capacity": admission.refusal_capacity,
            "max_wait_ms": admission.max_wait.as_millis() as u64,
            "queue_wait_ms": waited.map(|waited| waited.as_millis() as u64),
        })
    }

    /// Append one record, carrying any refusals that could not be recorded
    /// since the last one.
    ///
    /// Records are built only from local values and validated answers; that
    /// construction is what keeps prompt and provider text out. The screen
    /// here is a further defense, not a detector of all private text: a
    /// record that still matches the evidence plane's sensitive-pattern rules
    /// (credential shapes, key names, secret paths) is replaced by one that
    /// keeps the turn accountable and drops everything else.
    fn append(&self, mut record: Value) -> Result<()> {
        use std::sync::atomic::Ordering;
        let unrecorded = self.unrecorded.swap(0, Ordering::AcqRel);
        if unrecorded > 0 {
            if !record["admission"].is_object() {
                record["admission"] = json!({});
            }
            record["admission"]["unrecorded_refusals_before"] = json!(unrecorded);
        }
        let persisted = if find_sensitive(&record).is_some() {
            json!({
                "schema": RECORD_SCHEMA,
                "recorded_at": heiwa_evidence::now_iso(),
                "thread_id": record["thread_id"],
                "turn_id": record["turn_id"],
                "work_id": record["work_id"],
                "status": "withheld",
                "withheld_reason": "sensitive_match",
                "admission": { "unrecorded_refusals_before": unrecorded },
            })
        } else {
            record
        };
        self.journal
            .journal(SHADOW_STREAM, persisted)
            .inspect_err(|_| {
                // Not written: the count must survive to the next record.
                self.unrecorded.fetch_add(unrecorded, Ordering::AcqRel);
            })
    }

    fn skip_reason(&self, request: &ModelCallRequest) -> Option<&'static str> {
        if !self.system1.backend().is_remote() {
            return None;
        }
        match request.privacy {
            PrivacyClass::LocalOnly => return Some("privacy_local_only"),
            PrivacyClass::Sovereign => return Some("privacy_sovereign"),
            PrivacyClass::Standard => {}
        }
        find_sensitive(&json!(request.raw_text)).map(|_| "sensitive_input")
    }

    /// Everything that must be pinned together for records to be comparable:
    /// question wording, backend and model, thresholds, and the floor rule.
    fn policy(&self) -> Value {
        let backend = self.system1.backend();
        let thresholds = Thresholds::default();
        json!({
            "question_set": QUESTION_SET,
            "question_digest": self.questions.digest(),
            "backend": backend.name(),
            "model": backend.model(),
            "remote": backend.is_remote(),
            "confidence_ceiling": backend.confidence_ceiling(),
            // A capped fallback answers with one selection recorded as a point
            // mass; only a System One model returns a real distribution.
            "answer_shape": if backend.confidence_ceiling().is_some() {
                "point_mass"
            } else {
                "distribution"
            },
            "thresholds": { "auto": thresholds.auto, "deliberate": thresholds.deliberate },
            "gating": ["capability"],
            "budget_ms": self.budget.as_millis() as u64,
            "floor_rule": "raise_only",
        })
    }
}

/// Write one record on the blocking pool, off the async workers: the append
/// takes a cross-process file lock and syncs to disk. One at a time.
async fn persist(shared: &Arc<Shared>, record: Value) {
    let turn = record["turn_id"].as_str().unwrap_or("?").to_string();
    let writer = shared.clone();
    match tokio::task::spawn_blocking(move || writer.append(record)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            eprintln!("heiwa: System 1 shadow record for turn {turn} was not written: {error}")
        }
        Err(error) => {
            eprintln!("heiwa: System 1 shadow writer for turn {turn} failed: {error}")
        }
    }
}

/// Judge admitted turns in order. A turn that waited past `max_wait` is
/// recorded without contacting the backend. Records go to the writer; this
/// task never touches the journal, so a held stream lock cannot stall it.
async fn judge_admitted(
    shared: Arc<Shared>,
    mut admitted: tokio::sync::mpsc::Receiver<Admitted>,
    records: tokio::sync::mpsc::Sender<Value>,
) {
    while let Some(turn) = admitted.recv().await {
        let waited = turn.admitted_at.elapsed();
        let record = if waited > shared.admission.max_wait {
            shared.not_admitted(&turn.turn, "queue_expired", Some(waited))
        } else {
            let mut record = shared.judge(&turn.turn).await;
            record["admission"] = shared.admission_json(Some(waited));
            record
        };
        // The prompt is released before the record waits to be written.
        drop(turn);
        if records.send(record).await.is_err() {
            shared
                .unrecorded
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        }
    }
}

/// Write records one at a time until every sender is gone, then leave a
/// coverage record if refusals were counted with nothing left to carry them.
async fn write_records(shared: Arc<Shared>, mut records: tokio::sync::mpsc::Receiver<Value>) {
    while let Some(record) = records.recv().await {
        persist(&shared, record).await;
    }
    if shared.unrecorded.load(std::sync::atomic::Ordering::Acquire) > 0 {
        persist(
            &shared,
            json!({ "schema": RECORD_SCHEMA, "recorded_at": heiwa_evidence::now_iso(), "status": "coverage" }),
        )
        .await;
    }
}

impl ShadowObserver for ShadowJudge {
    /// CPU only on the turn's task: no lock, no file, no await. A refused
    /// turn's prompt is dropped here; only its metadata is queued to be
    /// written, and if that queue is full too, the refusal is counted.
    fn observe(&self, turn: ShadowTurn) {
        use std::sync::atomic::Ordering;
        use tokio::sync::mpsc::error::TrySendError;
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let shared = &self.inner.shared;
        let queues = self.inner.queues.get_or_init(|| {
            let (admitted, admitted_rx) = tokio::sync::mpsc::channel(shared.admission.capacity);
            let (records, records_rx) =
                tokio::sync::mpsc::channel(shared.admission.refusal_capacity);
            runtime.spawn(judge_admitted(shared.clone(), admitted_rx, records.clone()));
            let writer = runtime
                .spawn(write_records(shared.clone(), records_rx))
                .abort_handle();
            Queues {
                admitted,
                records,
                writer,
            }
        });
        let admitted = Admitted {
            turn,
            admitted_at: std::time::Instant::now(),
        };
        let Err(TrySendError::Full(refused) | TrySendError::Closed(refused)) =
            queues.admitted.try_send(admitted)
        else {
            return;
        };
        let record = shared.not_admitted(&refused.turn, "backlog", None);
        drop(refused);
        if queues.records.try_send(record).is_err() {
            shared.unrecorded.fetch_add(1, Ordering::AcqRel);
        }
    }
}

fn typesafe_key() -> String {
    std::env::var(TYPESAFE_KEY_ENV)
        .ok()
        .filter(|key| !key.trim().is_empty())
        .or_else(|| {
            heiwa_provider::keychain::load_secret_optional(TYPESAFE_KEY_ACCOUNT)
                .ok()
                .flatten()
        })
        .unwrap_or_default()
}

/// The prompt, cut at a character boundary when it is longer than `limit`.
fn bounded(text: &str, limit: usize) -> (String, bool) {
    match text.char_indices().nth(limit) {
        Some((end, _)) => (text[..end].to_string(), true),
        None => (text.to_string(), false),
    }
}

/// The level holding the most probability. Ties go to the lower level:
/// between equally likely floors, the smallest sufficient one.
fn most_probable_level(probabilities: &[f64]) -> usize {
    let mut best = 0;
    for (level, probability) in probabilities.iter().enumerate() {
        if *probability > probabilities[best] {
            best = level;
        }
    }
    best
}

fn plan_summary(request: &ModelCallRequest, candidates: &[ModelCallCandidate]) -> Value {
    match plan_model_call(request, candidates, &default_policy()) {
        Ok(plan) => json!({
            "selected": plan.selected.as_ref().map(|candidate| json!({
                "id": candidate.tier.id,
                "provider": candidate.tier.provider,
                "model_id": candidate.tier.model_id,
                "capability_class": candidate.tier.capability_class,
                "marginal_cost_usd": candidate.marginal_cost_usd,
            })),
            "selection_reason": plan.selection_reason,
            "admitted": plan.admitted_ids.len(),
            "policy_version": plan.policy_version,
        }),
        Err(error) => json!({ "error": error.to_string() }),
    }
}

fn call_summary(evaluation: &Evaluation) -> Value {
    let mut call = json!({
        "latency_ms": evaluation.latency_ms,
        "budget_ms": evaluation.budget_ms,
        "over_budget": evaluation.over_budget(),
    });
    match &evaluation.outcome {
        Ok(decoded) => {
            // The provider's `model` string is its text: provenance only.
            call["model"] = json!(model_provenance(
                &evaluation.requested_model,
                &decoded.model
            ));
            call["input_tokens"] = json!(decoded.usage.input_tokens);
            call["output_tokens"] = json!(decoded.usage.output_tokens);
            call["missing"] = json!(decoded.missing);
        }
        Err(error) => {
            call["model"] = json!({ "requested": evaluation.requested_model });
            // Kind, status, and a message the judgment crate generated; never
            // a response body or an echoed value.
            call["error"] = json!({
                "kind": error.kind.as_str(),
                "message": error.message,
                "status": error.status,
            });
        }
    }
    call
}

fn answer_summary(answer: &Answer) -> Value {
    match answer {
        Answer::Choice { choice, .. } => json!({ "kind": "choice", "choice": choice }),
        Answer::Score {
            score,
            probabilities,
            ..
        } => json!({
            "kind": "score",
            "score": score,
            "level": most_probable_level(probabilities),
            "probabilities": probabilities,
        }),
        Answer::Noul { noul } => json!({ "kind": "noul", "noul": noul }),
    }
}

// ---- report -----------------------------------------------------------------

/// What is known about one provider attempt's cost. The executor writes
/// `cost_usd: 0.0` with `cost_truth: cannot_confirm` for an amount it cannot
/// state; that zero is not a cost, so it is `Unknown` here.
#[derive(Debug, Clone, Copy, PartialEq)]
enum AttemptCost {
    /// Nothing was charged: a model on this device.
    KnownZero,
    /// The provider reported the charge.
    Exact(f64),
    /// A price-list or proxy estimate, not a charge.
    Estimated(f64),
    /// No amount can be stated.
    Unknown,
}

/// Read an amount together with its cost truth, never the amount alone.
fn attempt_cost(payload: &Value) -> AttemptCost {
    let amount = payload["cost_usd"]
        .as_f64()
        .filter(|amount| amount.is_finite() && *amount >= 0.0);
    match (payload["cost_truth"].as_str(), amount) {
        (Some("local_zero_cost"), Some(0.0)) => AttemptCost::KnownZero,
        (Some("exact_provider_report"), Some(amount)) => AttemptCost::Exact(amount),
        (Some("proxy_estimate" | "target_only"), Some(amount)) => AttemptCost::Estimated(amount),
        _ => AttemptCost::Unknown,
    }
}

/// A turn's execution cost: every provider attempt of every model call in
/// the turn. A tool turn makes a follow-up call with its own call id, and its
/// receipt carries only that call's cost, so the receipt is not the episode.
#[derive(Debug, Default, Clone, PartialEq)]
struct EpisodeCost {
    known_usd: f64,
    estimated_usd: f64,
    exact: u64,
    known_zero: u64,
    estimated: u64,
    unknown: u64,
}

impl EpisodeCost {
    fn add(&mut self, cost: AttemptCost) {
        match cost {
            AttemptCost::KnownZero => self.known_zero += 1,
            AttemptCost::Exact(amount) => {
                self.exact += 1;
                self.known_usd += amount;
            }
            AttemptCost::Estimated(amount) => {
                self.estimated += 1;
                self.estimated_usd += amount;
            }
            AttemptCost::Unknown => self.unknown += 1,
        }
    }

    /// The turn's weakest cost truth.
    fn truth(&self) -> AttemptCost {
        if self.unknown > 0 {
            AttemptCost::Unknown
        } else if self.estimated > 0 {
            AttemptCost::Estimated(self.estimated_usd)
        } else if self.exact > 0 {
            AttemptCost::Exact(self.known_usd)
        } else {
            AttemptCost::KnownZero
        }
    }
}

/// What one Work-scoped turn actually did, read from the operator journal.
#[derive(Debug, Default)]
struct TurnFacts {
    model_turn: bool,
    terminal: Option<&'static str>,
    interrupt_reason: Option<String>,
    cancel_requested: bool,
    approvals_not_granted: u64,
    executed: Option<String>,
    /// Every provider attempt, keyed by `(call_id, attempt)`: `None` once
    /// `route_attempted` announces it, then its own cost from the
    /// `route_completed` or `route_failed` that ends it. The per-call
    /// cumulative fields are never read, so nothing is counted twice.
    attempts: BTreeMap<(String, u64), Option<AttemptCost>>,
    /// The receipt's cost, used only if a turn has no attempt events at all.
    receipt_cost: Option<AttemptCost>,
}

impl TurnFacts {
    fn cost(&self) -> EpisodeCost {
        let mut episode = EpisodeCost::default();
        if self.attempts.is_empty() {
            // No provider was invoked: the executor announces every attempt
            // before it calls a provider.
            episode.add(self.receipt_cost.unwrap_or(AttemptCost::KnownZero));
            return episode;
        }
        for outcome in self.attempts.values() {
            // An attempt with no recorded end (the process stopped mid-call)
            // may have been charged: unknown, never zero.
            episode.add(outcome.unwrap_or(AttemptCost::Unknown));
        }
        episode
    }
}

/// Execution cost over a set of turns. Turns are counted by their weakest
/// cost truth; amounts are summed per attempt, so a turn with one unknown
/// attempt still contributes its known charges to the subtotals.
#[derive(Debug, Default, Clone)]
struct CostTally {
    turns: u64,
    known_zero: u64,
    exact: u64,
    estimated: u64,
    unknown: u64,
    unknown_attempts: u64,
    known_usd: f64,
    estimated_usd: f64,
}

impl CostTally {
    fn add(&mut self, episode: &EpisodeCost) {
        self.turns += 1;
        match episode.truth() {
            AttemptCost::KnownZero => self.known_zero += 1,
            AttemptCost::Exact(_) => self.exact += 1,
            AttemptCost::Estimated(_) => self.estimated += 1,
            AttemptCost::Unknown => self.unknown += 1,
        }
        self.unknown_attempts += episode.unknown;
        self.known_usd += episode.known_usd;
        self.estimated_usd += episode.estimated_usd;
    }

    fn merge(&mut self, other: &CostTally) {
        self.turns += other.turns;
        self.known_zero += other.known_zero;
        self.exact += other.exact;
        self.estimated += other.estimated;
        self.unknown += other.unknown;
        self.unknown_attempts += other.unknown_attempts;
        self.known_usd += other.known_usd;
        self.estimated_usd += other.estimated_usd;
    }

    /// A total exists only when no contributing amount is unknown.
    fn total(&self) -> Option<f64> {
        (self.turns > 0 && self.unknown == 0).then_some(self.known_usd + self.estimated_usd)
    }

    fn basis(&self) -> &'static str {
        if self.turns == 0 {
            "no_turns"
        } else if self.unknown > 0 {
            "incomplete"
        } else if self.estimated > 0 {
            "includes_estimates"
        } else {
            "exact"
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "turns": self.turns,
            "known_zero": self.known_zero,
            "exact": self.exact,
            "estimated": self.estimated,
            "unknown": self.unknown,
            "unknown_attempts": self.unknown_attempts,
            "known_usd": self.known_usd,
            "estimated_usd": self.estimated_usd,
            "total_usd": self.total(),
            "basis": self.basis(),
        })
    }
}

#[derive(Debug, Default)]
struct Bucket {
    turns: u64,
    completed: u64,
    interrupted: u64,
    blocked: u64,
    open: u64,
    cancel_requested: u64,
    approvals_not_granted: u64,
    /// Completed with no cancel request. Not acceptance: no labels exist.
    completed_uncancelled: u64,
    cost: CostTally,
    executed_models: BTreeMap<String, u64>,
    interrupt_reasons: BTreeMap<String, u64>,
}

impl Bucket {
    fn add(&mut self, facts: Option<&TurnFacts>) {
        self.turns += 1;
        let Some(facts) = facts else {
            self.open += 1;
            let mut unknown = EpisodeCost::default();
            unknown.add(AttemptCost::Unknown);
            self.cost.add(&unknown);
            return;
        };
        match facts.terminal {
            Some("completed") => self.completed += 1,
            Some("interrupted") => self.interrupted += 1,
            Some("blocked") => self.blocked += 1,
            _ => self.open += 1,
        }
        if facts.cancel_requested {
            self.cancel_requested += 1;
        }
        self.approvals_not_granted += facts.approvals_not_granted;
        if facts.terminal == Some("completed") && !facts.cancel_requested {
            self.completed_uncancelled += 1;
        }
        self.cost.add(&facts.cost());
        if let Some(executed) = &facts.executed {
            *self.executed_models.entry(executed.clone()).or_default() += 1;
        }
        if let Some(reason) = &facts.interrupt_reason {
            *self.interrupt_reasons.entry(reason.clone()).or_default() += 1;
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "turns": self.turns,
            "completed": self.completed,
            "interrupted": self.interrupted,
            "blocked": self.blocked,
            "open": self.open,
            "cancel_requested": self.cancel_requested,
            "approvals_not_granted": self.approvals_not_granted,
            "completed_uncancelled": self.completed_uncancelled,
            "cost": self.cost.to_json(),
            "executed_models": self.executed_models,
            "interrupt_reasons": self.interrupt_reasons,
        })
    }
}

fn turn_facts(evidence_dir: &Path, work_id: Option<&str>) -> Result<BTreeMap<String, TurnFacts>> {
    const PAGE_SIZE: usize = 512;
    let mut facts: BTreeMap<String, TurnFacts> = BTreeMap::new();
    let journal = heiwa_evidence::OperatorJournal::new(evidence_dir.to_path_buf())
        .map_err(|error| anyhow!("{error}"))?;
    let mut cursor: Option<String> = None;
    loop {
        let page = journal
            .read_after(cursor.as_deref(), PAGE_SIZE)
            .map_err(|error| anyhow!("{error}"))?;
        if page.events.is_empty() {
            break;
        }
        cursor = page.next_cursor;
        for row in page.events {
            let event = row.event;
            let (Some(turn_id), Some(event_work)) = (event.turn_id, event.work_id) else {
                continue;
            };
            if work_id.is_some_and(|wanted| wanted != event_work) {
                continue;
            }
            let entry = facts.entry(turn_id).or_default();
            let payload = &event.payload;
            match event.event_type {
                // Deterministic turns carry `mode: deterministic`; model calls
                // are planned by the executor, which never sets it.
                OperatorEventType::RoutePlanned
                    if payload["mode"].as_str() != Some("deterministic") =>
                {
                    entry.model_turn = true;
                }
                OperatorEventType::RouteAttempted => {
                    if let (Some(call_id), Some(attempt)) =
                        (event.call_id.as_ref(), payload["attempt"].as_u64())
                    {
                        entry
                            .attempts
                            .entry((call_id.clone(), attempt))
                            .or_insert(None);
                    }
                }
                // Deterministic routes carry no attempt; only executor
                // attempts end with their own cost.
                OperatorEventType::RouteCompleted | OperatorEventType::RouteFailed => {
                    if let (Some(call_id), Some(attempt)) =
                        (event.call_id.as_ref(), payload["attempt"].as_u64())
                    {
                        entry
                            .attempts
                            .insert((call_id.clone(), attempt), Some(attempt_cost(payload)));
                    }
                }
                OperatorEventType::TurnCompleted => entry.terminal = Some("completed"),
                OperatorEventType::TurnInterrupted => {
                    entry.terminal = Some("interrupted");
                    entry.interrupt_reason = payload["reason"].as_str().map(str::to_string);
                }
                OperatorEventType::Blocker => entry.terminal = Some("blocked"),
                OperatorEventType::TurnCancelRequested => entry.cancel_requested = true,
                OperatorEventType::ApprovalDecided
                    if !matches!(
                        payload["outcome"].as_str(),
                        Some("approved" | "auto_approved")
                    ) =>
                {
                    entry.approvals_not_granted += 1;
                }
                OperatorEventType::ReceiptLinked => {
                    if let (Some(provider), Some(model)) =
                        (payload["provider"].as_str(), payload["model"].as_str())
                    {
                        entry.executed = Some(format!("{provider}/{model}"));
                        entry.receipt_cost = Some(attempt_cost(payload));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(facts)
}

fn percentile(sorted: &[u64], fraction: f64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    Some(sorted[index])
}

/// Rebuild a JSON value with every object's keys sorted, so equal content
/// always serializes to equal bytes whatever the map implementation keeps.
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|key| (key.clone(), canonical(&map[key])))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

fn digest_of(value: &Value) -> String {
    let bytes = serde_json::to_vec(&canonical(value)).unwrap_or_default();
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// `family-1.2.3`: a version id that cannot move, unlike an alias.
fn is_pinned_version(model: &str) -> bool {
    let Some((family, version)) = model.split_once('-') else {
        return false;
    };
    !family.is_empty()
        && family.chars().all(|c| c.is_ascii_alphanumeric())
        && version.contains('.')
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

/// What can safely be said about which model answered. A mutable tag or
/// alias is never presented as an immutable identity.
fn model_identity(record: &Value) -> Value {
    let policy = &record["policy"];
    let requested = policy["model"].as_str().unwrap_or("?");
    let backend = policy["backend"].as_str().unwrap_or("?");
    let provenance = &record["call"]["model"];
    let legacy = policy["question_digest"].is_null()
        || policy["answer_shape"].is_null()
        || (record["status"] == "judged" && !provenance.is_object());
    if legacy {
        return json!({ "status": "legacy_unrecorded", "requested": requested });
    }
    match provenance["returned"].as_str() {
        None => json!({ "status": "no_model_response", "requested": requested }),
        Some("version") => json!({
            "status": "resolved_by_provider",
            "requested": requested,
            "version": provenance["returned_version"],
        }),
        Some("unrecognised") => json!({
            "status": "unrecognised",
            "requested": requested,
            "returned_digest": provenance["returned_digest"],
        }),
        Some(_) if backend == "typesafe" && is_pinned_version(requested) => json!({
            "status": "pinned",
            "requested": requested,
            "version": requested,
        }),
        Some(_) => json!({
            "status": "version_unresolved",
            "requested": requested,
            "why": if backend == "structured_llm" {
                "a local model tag can move and no content digest was recorded"
            } else {
                "the requested id is an alias the provider did not resolve to a version"
            },
        }),
    }
}

/// Metrics for one cohort. Nothing here is ever combined across cohorts.
fn cohort_summary(
    identity: Value,
    policy: &Value,
    records: &[(&String, &Value)],
    facts: &BTreeMap<String, TurnFacts>,
) -> Value {
    let mut status: BTreeMap<String, u64> = BTreeMap::new();
    let mut skip_reasons: BTreeMap<String, u64> = BTreeMap::new();
    let mut error_kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut bands: BTreeMap<String, u64> = BTreeMap::new();
    let mut intent_pairs: BTreeMap<String, u64> = BTreeMap::new();
    let (mut intent_agree, mut intent_disagree) = (0u64, 0u64);
    let (mut raise_floor, mut change_model) = (0u64, 0u64);
    let mut latencies = Vec::new();
    let mut waits = Vec::new();
    let (mut over_budget, mut input_tokens, mut output_tokens) = (0u64, 0u64, 0u64);
    let mut changed = Bucket::default();
    let mut same = Bucket::default();

    for (turn_id, record) in records {
        let state = record["status"].as_str().unwrap_or("unknown").to_string();
        *status.entry(state.clone()).or_default() += 1;
        if let Some(reason) = record["skip_reason"].as_str() {
            *skip_reasons.entry(reason.to_string()).or_default() += 1;
        }
        if let Some(kind) = record["call"]["error"]["kind"].as_str() {
            *error_kinds.entry(kind.to_string()).or_default() += 1;
        }
        if let Some(latency) = record["call"]["latency_ms"].as_u64() {
            latencies.push(latency);
        }
        if let Some(wait) = record["admission"]["queue_wait_ms"].as_u64() {
            waits.push(wait);
        }
        if record["call"]["over_budget"] == true {
            over_budget += 1;
        }
        input_tokens += record["call"]["input_tokens"].as_u64().unwrap_or(0);
        output_tokens += record["call"]["output_tokens"].as_u64().unwrap_or(0);
        if state != "judged" {
            continue;
        }
        if let Some(band) = record["gate"]["band"].as_str() {
            *bands.entry(band.to_string()).or_default() += 1;
        }
        match record["agreement"]["intent"].as_bool() {
            Some(true) => intent_agree += 1,
            Some(false) => {
                intent_disagree += 1;
                let pair = format!(
                    "{}->{}",
                    record["baseline"]["intent"].as_str().unwrap_or("?"),
                    record["recommendation"]["intent"].as_str().unwrap_or("?"),
                );
                *intent_pairs.entry(pair).or_default() += 1;
            }
            None => {}
        }
        if record["agreement"]["floor_delta"].as_i64().unwrap_or(0) > 0 {
            raise_floor += 1;
        }
        match record["agreement"]["same_model"].as_bool() {
            Some(false) => {
                change_model += 1;
                changed.add(facts.get(*turn_id));
            }
            Some(true) => same.add(facts.get(*turn_id)),
            None => {}
        }
    }

    latencies.sort_unstable();
    waits.sort_unstable();
    let completed = changed.completed_uncancelled + same.completed_uncancelled;
    let mut execution_cost = changed.cost.clone();
    execution_cost.merge(&same.cost);
    let cost_per_completed_turn = match (completed, execution_cost.total()) {
        (0, _) => json!({
            "usd": Value::Null,
            "basis": "no_completed_turns",
            "completed_uncancelled": 0,
            "turns_with_unknown_cost": execution_cost.unknown,
        }),
        (_, None) => json!({
            "usd": Value::Null,
            "basis": "incomplete",
            "completed_uncancelled": completed,
            "turns_with_unknown_cost": execution_cost.unknown,
        }),
        (_, Some(total)) => json!({
            "usd": total / completed as f64,
            "basis": execution_cost.basis(),
            "completed_uncancelled": completed,
            "turns_with_unknown_cost": 0,
        }),
    };

    let mut caveats = Vec::new();
    if let Some(ceiling) = policy["confidence_ceiling"].as_f64() {
        caveats.push(format!(
            "{} is capped at confidence {ceiling}, so it can never reach the auto band; its counterfactuals show what would change if it were trusted.",
            policy["model"].as_str().unwrap_or("?")
        ));
    }
    if identity["status"] == "version_unresolved" {
        caveats.push(format!(
            "Model identity is unresolved: {}.",
            identity["why"].as_str().unwrap_or("no version")
        ));
    }
    if execution_cost.unknown > 0 {
        caveats.push(format!(
            "{} turn(s) have no known execution cost, so there is no total and no cost per completed turn; known and estimated subtotals are shown separately.",
            execution_cost.unknown
        ));
    }
    if execution_cost.estimated > 0 {
        caveats
            .push("Estimated amounts are price-list or proxy estimates, not charges.".to_string());
    }

    json!({
        "cohort": {
            "id": digest_of(&json!([digest_of(policy), identity]))[7..19].to_string(),
            "policy_digest": digest_of(policy),
            "question_set": policy["question_set"],
            "question_digest": policy["question_digest"],
            "backend": policy["backend"],
            "model": policy["model"],
            "answer_shape": policy["answer_shape"],
            "model_identity": identity,
            "policy": policy,
        },
        "records": records.len(),
        "status": status,
        "skip_reasons": skip_reasons,
        "error_kinds": error_kinds,
        "admission": {
            "queue_wait_ms": {
                "p50": percentile(&waits, 0.5),
                "p95": percentile(&waits, 0.95),
                "max": waits.last(),
            },
        },
        "classifier": {
            "latency_ms": {
                "p50": percentile(&latencies, 0.5),
                "p95": percentile(&latencies, 0.95),
                "max": latencies.last(),
            },
            "over_budget": over_budget,
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
        },
        "bands": {
            "auto": bands.get("auto").copied().unwrap_or(0),
            "deliberate": bands.get("deliberate").copied().unwrap_or(0),
            "quarantine": bands.get("quarantine").copied().unwrap_or(0),
        },
        "intent": { "agree": intent_agree, "disagree": intent_disagree, "pairs": intent_pairs },
        "route": {
            "would_raise_floor": raise_floor,
            "would_change_model": change_model,
            "same_route": same.turns,
        },
        "outcomes": { "would_change_model": changed.to_json(), "same_route": same.to_json() },
        "execution_cost": execution_cost.to_json(),
        "cost_per_completed_turn": cost_per_completed_turn,
        "classifier_input_tokens_per_completed_turn": if completed > 0 { json!(input_tokens as f64 / completed as f64) } else { Value::Null },
        "caveats": caveats,
    })
}

/// Join shadow records with what their turns actually did, one cohort per
/// exact policy and model identity. Cohorts are never pooled: latency, gate
/// bands, and agreement from different policies or models are not
/// comparable.
pub fn report(evidence_dir: &Path, work_id: Option<&str>) -> Result<Value> {
    let stream = heiwa_evidence::read_stream(evidence_dir, SHADOW_STREAM)?;
    // One record per turn; a later record for the same turn replaces an
    // earlier one. Refusals nobody could record are counted from every
    // record that carries them, including coverage-only records.
    let mut records: BTreeMap<String, Value> = BTreeMap::new();
    let mut unrecorded_refusals = 0u64;
    for event in stream.events {
        let record = event.record;
        if record["schema"] != RECORD_SCHEMA {
            continue;
        }
        unrecorded_refusals += record["admission"]["unrecorded_refusals_before"]
            .as_u64()
            .unwrap_or(0);
        if work_id.is_some_and(|wanted| record["work_id"] != wanted) {
            continue;
        }
        if let Some(turn_id) = record["turn_id"].as_str() {
            records.insert(turn_id.to_string(), record);
        }
    }
    let facts = turn_facts(evidence_dir, work_id)?;

    let mut cohorts: BTreeMap<String, (Value, Value, Vec<(&String, &Value)>)> = BTreeMap::new();
    for (turn_id, record) in &records {
        let identity = model_identity(record);
        let key = digest_of(&json!([digest_of(&record["policy"]), identity]));
        cohorts
            .entry(key)
            .or_insert_with(|| (identity, record["policy"].clone(), Vec::new()))
            .2
            .push((turn_id, record));
    }
    let cohorts: Vec<Value> = cohorts
        .into_values()
        .map(|(identity, policy, members)| cohort_summary(identity, &policy, &members, &facts))
        .collect();

    let work_model_turns = facts.values().filter(|fact| fact.model_turn).count();
    let without_record = facts
        .iter()
        .filter(|(turn_id, fact)| fact.model_turn && !records.contains_key(*turn_id))
        .count();

    let mut caveats = vec![
        "No quality labels are collected yet: completed means the turn finished with no cancel request, not that its result was accepted or correct."
            .to_string(),
        "Outcomes are the deterministic route's; a counterfactual was never executed.".to_string(),
    ];
    if cohorts.len() > 1 {
        caveats.push(
            "Cohorts are reported separately: latency, gate bands, and agreement are not comparable across them."
                .to_string(),
        );
    }
    if unrecorded_refusals > 0 {
        caveats.push(format!(
            "{unrecorded_refusals} refused turn(s) have no record because the refusal queue was full; they are counted, not attributed to a Work."
        ));
    }
    if records.is_empty() {
        caveats.push(
            "No shadow records yet. Enable `[system1] shadow = true` in config.toml and run Work-scoped turns."
                .to_string(),
        );
    }

    Ok(json!({
        "schema": REPORT_SCHEMA,
        "work_id": work_id,
        "records": records.len(),
        "coverage": {
            "work_model_turns": work_model_turns,
            "without_record": without_record,
            "unrecorded_refusals": unrecorded_refusals,
            "skipped_stream_lines": stream.skipped_lines,
        },
        "cohorts": cohorts,
        "caveats": caveats,
    }))
}

/// A short human rendering of [`report`]: one block per cohort, and nothing
/// that pools calibration, latency, or cost across cohorts.
pub fn render_report(report: &Value) -> String {
    let count = |value: &Value| value.as_u64().unwrap_or(0);
    let tally = |value: &Value| -> String {
        value
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(key, n)| format!("{key} {}", n.as_u64().unwrap_or(0)))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    let non_empty = |value: &Value| value.as_object().is_some_and(|map| !map.is_empty());
    let coverage = &report["coverage"];
    let cohorts = report["cohorts"].as_array().cloned().unwrap_or_default();
    let mut lines = vec![
        format!(
            "System 1 shadow — {}",
            report["work_id"].as_str().unwrap_or("all Work")
        ),
        format!(
            "coverage: {} Work model turn(s), {} without a record, {} refused turn(s) unrecorded",
            count(&coverage["work_model_turns"]),
            count(&coverage["without_record"]),
            count(&coverage["unrecorded_refusals"])
        ),
        format!(
            "{} record(s) in {} cohort(s); cohorts are not comparable to one another",
            count(&report["records"]),
            cohorts.len()
        ),
    ];
    for cohort in &cohorts {
        let id = &cohort["cohort"];
        let identity = &id["model_identity"];
        lines.push(String::new());
        lines.push(format!(
            "cohort {} — {} {} [{}{}] · {} q:{} · {}: {} record(s)",
            id["id"].as_str().unwrap_or("?"),
            id["backend"].as_str().unwrap_or("?"),
            id["model"].as_str().unwrap_or("?"),
            identity["status"].as_str().unwrap_or("?"),
            identity["version"]
                .as_str()
                .map(|version| format!(" {version}"))
                .unwrap_or_default(),
            id["question_set"].as_str().unwrap_or("?"),
            id["question_digest"]
                .as_str()
                .and_then(|digest| digest.strip_prefix("sha256:"))
                .map(|digest| &digest[..8.min(digest.len())])
                .unwrap_or("?"),
            id["answer_shape"].as_str().unwrap_or("legacy"),
            count(&cohort["records"])
        ));
        lines.push(format!("  status: {}", tally(&cohort["status"])));
        if non_empty(&cohort["skip_reasons"]) {
            lines.push(format!("  skipped: {}", tally(&cohort["skip_reasons"])));
        }
        if non_empty(&cohort["error_kinds"]) {
            lines.push(format!("  failed: {}", tally(&cohort["error_kinds"])));
        }
        let wait = &cohort["admission"]["queue_wait_ms"];
        if wait["p50"].is_u64() {
            lines.push(format!(
                "  queue wait: p50 {}ms, p95 {}ms, max {}ms",
                count(&wait["p50"]),
                count(&wait["p95"]),
                count(&wait["max"])
            ));
        }
        let classifier = &cohort["classifier"];
        let latency = &classifier["latency_ms"];
        if latency["p50"].is_u64() {
            lines.push(format!(
                "  classifier: p50 {}ms, p95 {}ms, max {}ms; over budget {}; tokens {} in / {} out",
                count(&latency["p50"]),
                count(&latency["p95"]),
                count(&latency["max"]),
                count(&classifier["over_budget"]),
                count(&classifier["input_tokens"]),
                count(&classifier["output_tokens"])
            ));
        }
        lines.push(format!("  gate: {}", tally(&cohort["bands"])));
        let intent = &cohort["intent"];
        lines.push(format!(
            "  intent: agrees with keyword rules on {} of {}{}",
            count(&intent["agree"]),
            count(&intent["agree"]) + count(&intent["disagree"]),
            if non_empty(&intent["pairs"]) {
                format!(" (keyword->judged: {})", tally(&intent["pairs"]))
            } else {
                String::new()
            }
        ));
        let route = &cohort["route"];
        lines.push(format!(
            "  route: would raise the floor on {}; would pick a different model on {}",
            count(&route["would_raise_floor"]),
            count(&route["would_change_model"])
        ));
        for (label, key) in [
            ("would change model", "would_change_model"),
            ("same route", "same_route"),
        ] {
            let bucket = &cohort["outcomes"][key];
            lines.push(format!(
                "  outcomes ({label}): {} turn(s), {} completed, {} interrupted, {} cancel requested, {} completed without a cancel; cost {}",
                count(&bucket["turns"]),
                count(&bucket["completed"]),
                count(&bucket["interrupted"]),
                count(&bucket["cancel_requested"]),
                count(&bucket["completed_uncancelled"]),
                cost_phrase(&bucket["cost"])
            ));
        }
        lines.push(format!(
            "  execution cost: {}",
            cost_phrase(&cohort["execution_cost"])
        ));
        let per = &cohort["cost_per_completed_turn"];
        lines.push(match (per["usd"].as_f64(), per["basis"].as_str()) {
            (Some(usd), Some(basis)) => format!("  cost per completed turn: ${usd:.4} ({basis})"),
            (None, Some("incomplete")) => format!(
                "  cost per completed turn: unavailable ({} turn(s) with unknown cost)",
                count(&per["turns_with_unknown_cost"])
            ),
            _ => "  cost per completed turn: no completed turns".to_string(),
        });
        for caveat in cohort["caveats"].as_array().into_iter().flatten() {
            lines.push(format!("  ! {}", caveat.as_str().unwrap_or("")));
        }
    }
    lines.push(String::new());
    lines.push("caveats:".to_string());
    for caveat in report["caveats"].as_array().into_iter().flatten() {
        lines.push(format!("  - {}", caveat.as_str().unwrap_or("")));
    }
    lines.join("\n") + "\n"
}

/// One cost tally, stated with its truth: known and estimated amounts apart,
/// unknown turns counted, and a total only when nothing is unknown.
fn cost_phrase(cost: &Value) -> String {
    let count = |value: &Value| value.as_u64().unwrap_or(0);
    if count(&cost["turns"]) == 0 {
        return "no turns".to_string();
    }
    let mut parts = vec![format!(
        "${:.4} known ({} exact, {} known zero)",
        cost["known_usd"].as_f64().unwrap_or(0.0),
        count(&cost["exact"]),
        count(&cost["known_zero"])
    )];
    if count(&cost["estimated"]) > 0 {
        parts.push(format!(
            "${:.4} estimated ({})",
            cost["estimated_usd"].as_f64().unwrap_or(0.0),
            count(&cost["estimated"])
        ));
    }
    if count(&cost["unknown"]) > 0 {
        parts.push(format!("{} unknown", count(&cost["unknown"])));
    }
    let total = match cost["total_usd"].as_f64() {
        Some(total) => format!(
            "total ${total:.4} ({})",
            cost["basis"].as_str().unwrap_or("?")
        ),
        None => "total unavailable".to_string(),
    };
    format!("{}; {total}", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use heiwa_protocol::Intent;

    /// Fails to compile when an intent is added, and fails at runtime when the
    /// question set stops offering one DREX would route on.
    #[test]
    fn the_intent_question_offers_exactly_the_drex_intent_keys() {
        fn every(intent: Intent) -> Intent {
            match intent {
                Intent::Chat
                | Intent::Build
                | Intent::Deploy
                | Intent::Audit
                | Intent::Research
                | Intent::Strategy
                | Intent::StatusCheck => intent,
            }
        }
        let keys: Vec<&str> = [
            Intent::Chat,
            Intent::Build,
            Intent::Deploy,
            Intent::Audit,
            Intent::Research,
            Intent::Strategy,
            Intent::StatusCheck,
        ]
        .map(every)
        .iter()
        .map(Intent::as_drex_key)
        .collect();
        let offered: Vec<&str> = INTENTS.iter().map(|(key, _)| *key).collect();
        assert_eq!(offered, keys);
    }

    #[test]
    fn ties_between_levels_resolve_to_the_smallest_sufficient_floor() {
        assert_eq!(most_probable_level(&[0.1, 0.45, 0.45]), 1);
        assert_eq!(most_probable_level(&[0.2, 0.2, 0.6]), 2);
        assert_eq!(most_probable_level(&[0.5, 0.5]), 0);
    }

    #[test]
    fn a_long_prompt_is_cut_on_a_character_boundary() {
        let (cut, truncated) = bounded("héllo wörld", 4);
        assert_eq!(cut, "héll");
        assert!(truncated);
        let (whole, truncated) = bounded("short", 10);
        assert_eq!(whole, "short");
        assert!(!truncated);
    }
}
