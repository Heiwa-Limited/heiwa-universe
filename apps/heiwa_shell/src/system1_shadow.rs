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
//! - **Nothing sensitive leaves the machine.** A remote backend receives only
//!   `standard`-privacy prompts with no sensitive match; the record carries a
//!   digest and length of the prompt, never its text.
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
use heiwa_judgment::backend::{Backend, Evaluation, TYPESAFE_DEFAULT_MODEL};
use heiwa_judgment::question::{Question, QuestionSet};
use heiwa_judgment::{gate_batch, Answer, Band, Policy, Thresholds};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::operator::{ShadowObserver, ShadowTurn};

pub const SHADOW_STREAM: &str = "system1_shadow";
pub const RECORD_SCHEMA: &str = "heiwa.system1_shadow.v1";
pub const REPORT_SCHEMA: &str = "heiwa.system1_shadow.report.v1";
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

/// Judges finished Work-scoped turns in the background and records them.
#[derive(Clone)]
pub struct ShadowJudge {
    inner: Arc<Inner>,
}

struct Inner {
    backend: Backend,
    budget: Duration,
    questions: QuestionSet,
    client: reqwest::Client,
    journal: JsonlTransport,
    /// One judgment at a time: a local backend shares the machine with the
    /// turns it shadows, and the turn stream is human-paced.
    in_flight: tokio::sync::Semaphore,
}

impl ShadowJudge {
    pub fn new(backend: Backend, budget: Duration, evidence_dir: PathBuf) -> Result<Self> {
        Ok(ShadowJudge {
            inner: Arc::new(Inner {
                backend,
                budget,
                questions: turn_route_questions(),
                client: reqwest::Client::builder().build()?,
                journal: JsonlTransport::new(evidence_dir)?,
                in_flight: tokio::sync::Semaphore::new(1),
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
        let inner = &self.inner;
        let prompt = turn.request.raw_text.as_str();
        let (state, truncated) = bounded(prompt, MAX_STATE_CHARS);

        // The executor narrows the ceiling to the remaining budget before it
        // plans; the replay must start from the same request.
        let mut planned = turn.request.clone();
        crate::model_calls::apply_remaining_budget(&mut planned, turn.remaining_budget_usd);
        let applied_floor = planned.minimum_quality_class;

        let mut record = json!({
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
                "minimum_quality_class": applied_floor,
                "plan": plan_summary(&planned, &turn.candidates),
            },
            "call": Value::Null,
            "judgments": [],
            "gate": Value::Null,
            "recommendation": Value::Null,
            "counterfactual": Value::Null,
            "agreement": Value::Null,
        });

        if let Some(reason) = self.skip_reason(&planned) {
            record["status"] = json!("skipped");
            record["skip_reason"] = json!(reason);
            return record;
        }

        let evaluation = inner
            .backend
            .evaluate(&inner.client, &state, &inner.questions, inner.budget)
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

    /// Append one record to the `system1_shadow` stream.
    pub fn record(&self, record: &Value) -> Result<()> {
        self.inner.journal.journal(SHADOW_STREAM, record.clone())
    }

    fn skip_reason(&self, request: &ModelCallRequest) -> Option<&'static str> {
        if !self.inner.backend.is_remote() {
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
        let inner = &self.inner;
        let thresholds = Thresholds::default();
        json!({
            "question_set": QUESTION_SET,
            "question_digest": inner.questions.digest(),
            "backend": inner.backend.name(),
            "model": inner.backend.model(),
            "remote": inner.backend.is_remote(),
            "confidence_ceiling": inner.backend.confidence_ceiling(),
            // A capped fallback answers with one selection recorded as a point
            // mass; only a System One model returns a real distribution.
            "answer_shape": if inner.backend.confidence_ceiling().is_some() {
                "point_mass"
            } else {
                "distribution"
            },
            "thresholds": { "auto": thresholds.auto, "deliberate": thresholds.deliberate },
            "gating": ["capability"],
            "budget_ms": inner.budget.as_millis() as u64,
            "floor_rule": "raise_only",
        })
    }
}

impl ShadowObserver for ShadowJudge {
    fn observe(&self, turn: ShadowTurn) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let judge = self.clone();
        runtime.spawn(async move {
            let Ok(_permit) = judge.inner.in_flight.acquire().await else {
                return;
            };
            let record = judge.judge(&turn).await;
            if let Err(error) = judge.record(&record) {
                eprintln!(
                    "heiwa: System 1 shadow record for turn {} was not written: {error}",
                    turn.turn_id
                );
            }
        });
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
            call["model_answered"] = json!(decoded.model);
            call["input_tokens"] = json!(decoded.usage.input_tokens);
            call["output_tokens"] = json!(decoded.usage.output_tokens);
            call["missing"] = json!(decoded.missing);
        }
        Err(error) => {
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

/// What one Work-scoped turn actually did, read from the operator journal.
#[derive(Debug, Default)]
struct TurnFacts {
    model_turn: bool,
    terminal: Option<&'static str>,
    interrupt_reason: Option<String>,
    cancel_requested: bool,
    approvals_not_granted: u64,
    executed: Option<String>,
    cost_usd: f64,
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
    accepted: u64,
    executed_cost_usd: f64,
    executed_models: BTreeMap<String, u64>,
    interrupt_reasons: BTreeMap<String, u64>,
}

impl Bucket {
    fn add(&mut self, facts: Option<&TurnFacts>) {
        self.turns += 1;
        let Some(facts) = facts else {
            self.open += 1;
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
            self.accepted += 1;
        }
        self.executed_cost_usd += facts.cost_usd;
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
            "accepted": self.accepted,
            "executed_cost_usd": self.executed_cost_usd,
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
                        entry.cost_usd += payload["cost_usd"].as_f64().unwrap_or(0.0);
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

/// Join shadow records with what their turns actually did.
pub fn report(evidence_dir: &Path, work_id: Option<&str>) -> Result<Value> {
    let stream = heiwa_evidence::read_stream(evidence_dir, SHADOW_STREAM)?;
    // One record per turn; a later record for the same turn replaces an
    // earlier one.
    let mut records: BTreeMap<String, Value> = BTreeMap::new();
    for event in stream.events {
        let record = event.record;
        if record["schema"] != RECORD_SCHEMA {
            continue;
        }
        if work_id.is_some_and(|wanted| record["work_id"] != wanted) {
            continue;
        }
        if let Some(turn_id) = record["turn_id"].as_str() {
            records.insert(turn_id.to_string(), record);
        }
    }
    let facts = turn_facts(evidence_dir, work_id)?;

    let mut status: BTreeMap<String, u64> = BTreeMap::new();
    let mut skip_reasons: BTreeMap<String, u64> = BTreeMap::new();
    let mut error_kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut bands: BTreeMap<String, u64> = BTreeMap::new();
    let mut intent_pairs: BTreeMap<String, u64> = BTreeMap::new();
    let (mut intent_agree, mut intent_disagree) = (0u64, 0u64);
    let (mut raise_floor, mut change_model) = (0u64, 0u64);
    let mut latencies = Vec::new();
    let (mut over_budget, mut input_tokens, mut output_tokens) = (0u64, 0u64, 0u64);
    let mut changed = Bucket::default();
    let mut same = Bucket::default();
    let mut capped: BTreeMap<String, f64> = BTreeMap::new();

    for (turn_id, record) in &records {
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
        if record["call"]["over_budget"] == true {
            over_budget += 1;
        }
        input_tokens += record["call"]["input_tokens"].as_u64().unwrap_or(0);
        output_tokens += record["call"]["output_tokens"].as_u64().unwrap_or(0);
        if let Some(ceiling) = record["policy"]["confidence_ceiling"].as_f64() {
            let model = record["policy"]["model"]
                .as_str()
                .unwrap_or("?")
                .to_string();
            capped.insert(model, ceiling);
        }
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
                changed.add(facts.get(turn_id));
            }
            Some(true) => same.add(facts.get(turn_id)),
            None => {}
        }
    }

    latencies.sort_unstable();
    let work_model_turns = facts.values().filter(|fact| fact.model_turn).count();
    let without_record = facts
        .iter()
        .filter(|(turn_id, fact)| fact.model_turn && !records.contains_key(*turn_id))
        .count();
    let accepted = changed.accepted + same.accepted;
    let executed_cost = changed.executed_cost_usd + same.executed_cost_usd;

    let mut caveats = vec![
        "No quality labels are collected yet: accepted means completed with no cancel request, not judged correct."
            .to_string(),
        "Outcomes are the deterministic route's; a counterfactual was never executed.".to_string(),
    ];
    for (model, ceiling) in &capped {
        caveats.push(format!(
            "{model} is capped at confidence {ceiling}, so it can never reach the auto band; its counterfactuals show what would change if it were trusted."
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
        "question_set": QUESTION_SET,
        "records": records.len(),
        "status": status,
        "skip_reasons": skip_reasons,
        "error_kinds": error_kinds,
        "coverage": {
            "work_model_turns": work_model_turns,
            "without_record": without_record,
            "skipped_stream_lines": stream.skipped_lines,
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
        "cost_per_accepted_usd": if accepted > 0 { json!(executed_cost / accepted as f64) } else { Value::Null },
        "classifier_input_tokens_per_accepted": if accepted > 0 { json!(input_tokens as f64 / accepted as f64) } else { Value::Null },
        "caveats": caveats,
    }))
}

/// A short human rendering of [`report`].
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
    let mut lines = vec![format!(
        "System 1 shadow — {} ({})",
        report["work_id"].as_str().unwrap_or("all Work"),
        report["question_set"].as_str().unwrap_or(QUESTION_SET)
    )];
    lines.push(format!(
        "records {}: {}",
        count(&report["records"]),
        tally(&report["status"])
    ));
    if report["skip_reasons"]
        .as_object()
        .is_some_and(|map| !map.is_empty())
    {
        lines.push(format!("  skipped: {}", tally(&report["skip_reasons"])));
    }
    if report["error_kinds"]
        .as_object()
        .is_some_and(|map| !map.is_empty())
    {
        lines.push(format!("  failed: {}", tally(&report["error_kinds"])));
    }
    let coverage = &report["coverage"];
    lines.push(format!(
        "coverage: {} Work model turn(s), {} without a record",
        count(&coverage["work_model_turns"]),
        count(&coverage["without_record"])
    ));
    let classifier = &report["classifier"];
    let latency = &classifier["latency_ms"];
    if latency["p50"].is_u64() {
        lines.push(format!(
            "classifier: p50 {}ms, p95 {}ms, max {}ms; over budget {}; tokens {} in / {} out",
            count(&latency["p50"]),
            count(&latency["p95"]),
            count(&latency["max"]),
            count(&classifier["over_budget"]),
            count(&classifier["input_tokens"]),
            count(&classifier["output_tokens"])
        ));
    }
    lines.push(format!("gate: {}", tally(&report["bands"])));
    let intent = &report["intent"];
    lines.push(format!(
        "intent: agrees with keyword rules on {} of {}{}",
        count(&intent["agree"]),
        count(&intent["agree"]) + count(&intent["disagree"]),
        if intent["pairs"]
            .as_object()
            .is_some_and(|map| !map.is_empty())
        {
            format!(" (keyword->judged: {})", tally(&intent["pairs"]))
        } else {
            String::new()
        }
    ));
    let route = &report["route"];
    lines.push(format!(
        "route: would raise the floor on {}; would pick a different model on {}",
        count(&route["would_raise_floor"]),
        count(&route["would_change_model"])
    ));
    for (label, key) in [
        ("would change model", "would_change_model"),
        ("same route", "same_route"),
    ] {
        let bucket = &report["outcomes"][key];
        lines.push(format!(
            "outcomes ({label}): {} turn(s), {} completed, {} interrupted, {} cancel requested, {} accepted, ${:.4}",
            count(&bucket["turns"]),
            count(&bucket["completed"]),
            count(&bucket["interrupted"]),
            count(&bucket["cancel_requested"]),
            count(&bucket["accepted"]),
            bucket["executed_cost_usd"].as_f64().unwrap_or(0.0)
        ));
    }
    if let Some(cost) = report["cost_per_accepted_usd"].as_f64() {
        lines.push(format!("cost per accepted result: ${cost:.4} executed"));
    }
    lines.push("caveats:".to_string());
    for caveat in report["caveats"].as_array().into_iter().flatten() {
        lines.push(format!("  - {}", caveat.as_str().unwrap_or("")));
    }
    lines.join("\n") + "\n"
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
