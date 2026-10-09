//! Provider execution observations folded from operator route events.
//!
//! An observation is a dated fact that one provider attempt succeeded or
//! failed through one channel. It is not readiness: a success last week does
//! not prove the provider works now, so views carry the observation time and
//! leave age and wording to the presentation layer.
//!
//! The fold reads an allowlist only: event type, `occurred_at`, `call_id`,
//! and the payload's `attempt`, `provider`, `model`, `channel`,
//! `failure_class`, `failure_origin` and `provider_invoked`. It never reads
//! the provider `message`, and its output types have no field that could
//! carry one. Failures that never reached a provider (resolver misses) or
//! that followed a provider response (accounting) are not provider failures;
//! cancellation says nothing about provider health. Rows written before
//! channel and origin provenance existed keep an `unknown` channel and an
//! unverified failure origin.
//!
//! This is a presentation read model over executor route events. It is not
//! the Provider Truth Contract's `provider_observations` record kind
//! (docs/superpowers/specs/2026-08-31-provider-truth-contract-design.md) and
//! it neither admits nor ranks routes.

use std::collections::{BTreeMap, HashSet, VecDeque};

use heiwa_evidence::{
    CursorError, OperatorEvent, OperatorEventType, OperatorJournal, OperatorTail,
    OPERATOR_EVENT_SCHEMA_VERSION,
};
use serde::Serialize;
use serde_json::Value;

/// Credential kinds a channel may name (`Credential::kind_label`).
const KNOWN_CHANNEL_KINDS: [&str; 4] = ["api_key", "oauth", "oauth_cli", "local_runtime"];
/// `ProviderFailureClass::as_str` values recorded by the model-call executor.
const KNOWN_FAILURE_CLASSES: [&str; 8] = [
    "cancelled",
    "rate_limited",
    "authentication",
    "quota_exhausted",
    "timeout",
    "availability",
    "invalid_usage",
    "provider",
];
const MAX_IDENTIFIER_LEN: usize = 128;
/// Attempts awaiting an outcome that the fold remembers at once.
const MAX_PENDING_ATTEMPTS: usize = 64;

/// Default bounds for a fresh-process read (`heiwa doctor`).
pub const DEFAULT_TAIL_BYTES: u64 = 4 * 1024 * 1024;
pub const DEFAULT_TAIL_EVENTS: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ObservedChannel {
    /// A credential kind, or `unknown` when the row did not record one.
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary: Option<String>,
}

impl ObservedChannel {
    pub fn unknown() -> Self {
        Self {
            kind: "unknown".to_string(),
            account_id: None,
            binary: None,
        }
    }

    fn from_payload(value: &Value) -> Self {
        let Some(kind) = value["kind"]
            .as_str()
            .filter(|kind| KNOWN_CHANNEL_KINDS.contains(kind))
        else {
            return Self::unknown();
        };
        Self {
            kind: kind.to_string(),
            account_id: value["account_id"].as_str().and_then(identifier),
            binary: value["binary"].as_str().and_then(identifier),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuccessObservation {
    pub at: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FailureObservation {
    pub at: String,
    pub model: String,
    /// A known failure class, or `unclassified`.
    pub class: String,
    /// `provider` when the executor attributed the failure to the provider;
    /// `unverified` for rows that predate origin provenance.
    pub origin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LatestOutcome {
    Success,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderExecution {
    pub provider: String,
    pub channel: ObservedChannel,
    pub last_success: Option<SuccessObservation>,
    pub last_failure: Option<FailureObservation>,
    pub latest: LatestOutcome,
    pub successes: u64,
    pub failures: u64,
}

/// An attempt with no recorded outcome: in flight, or lost to a crash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenAttempt {
    pub provider: String,
    pub model: String,
    pub at: String,
}

/// Route outcomes that are deliberately not provider observations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ExcludedOutcomes {
    /// No adapter was resolved; the provider was never called.
    pub not_invoked: u64,
    /// The provider answered but Heiwa could not account for the response.
    /// Each is also counted as a provider success.
    pub accounting: u64,
    pub cancelled: u64,
    /// Rows whose provider, time or shape did not validate.
    pub malformed: u64,
    /// Open attempts forgotten because more than the pending cap were open.
    pub pending_overflow: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceState {
    /// Every operator event was inspected and none was damaged.
    Complete,
    /// Some events were not inspected or were damaged; absence of an
    /// observation is not absence of an execution.
    Partial,
    /// No operator stream exists for this profile.
    Empty,
    /// The stream could not be read.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecutionEvidence {
    pub state: EvidenceState,
    /// `runtime_projection` or `journal_tail`.
    pub source: &'static str,
    /// `occurred_at` of the oldest inspected event, when any was inspected.
    pub window_from: Option<String>,
    pub events_inspected: usize,
    pub skipped_lines: usize,
    pub starts_mid_stream: bool,
    pub dropped_events: usize,
    /// Error category when `state` is `unavailable`.
    pub error: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderExecutionView {
    pub evidence: ExecutionEvidence,
    pub observations: Vec<ProviderExecution>,
    pub open_attempts: Vec<OpenAttempt>,
    pub excluded: ExcludedOutcomes,
}

#[derive(Debug, Default)]
struct Row {
    last_success: Option<(u64, SuccessObservation)>,
    last_failure: Option<(u64, FailureObservation)>,
    successes: u64,
    failures: u64,
}

/// Incremental fold over operator events in append order.
#[derive(Debug, Default)]
pub struct ProviderExecutionFold {
    rows: BTreeMap<(String, ObservedChannel), Row>,
    pending: VecDeque<(String, OpenAttempt)>,
    excluded: ExcludedOutcomes,
    order: u64,
    first_seen_at: Option<String>,
    events_inspected: usize,
}

impl ProviderExecutionFold {
    /// Fold one event that already passed event-id dedup and schema checks.
    /// Thread and turn state are deliberately not consulted: a provider that
    /// answered after a cancel request still answered.
    pub fn observe(&mut self, event: &OperatorEvent) {
        self.events_inspected = self.events_inspected.saturating_add(1);
        if self.first_seen_at.is_none() && timestamp(&event.occurred_at).is_some() {
            self.first_seen_at = Some(event.occurred_at.clone());
        }
        let route = matches!(
            event.event_type,
            OperatorEventType::RouteAttempted
                | OperatorEventType::RouteCompleted
                | OperatorEventType::RouteFailed
        );
        if !route {
            return;
        }
        let payload = &event.payload;
        let (Some(provider), Some(at)) = (
            payload["provider"].as_str().and_then(provider_name),
            timestamp(&event.occurred_at),
        ) else {
            self.excluded.malformed += 1;
            return;
        };
        let model = payload["model"]
            .as_str()
            .and_then(identifier)
            .unwrap_or_else(|| "unknown".to_string());
        let attempt_key = format!(
            "{}#{}",
            event.call_id.as_deref().unwrap_or(""),
            payload["attempt"].as_u64().unwrap_or(0)
        );
        self.order += 1;
        let order = self.order;

        match event.event_type {
            OperatorEventType::RouteAttempted => {
                if self.pending.len() == MAX_PENDING_ATTEMPTS {
                    self.pending.pop_front();
                    self.excluded.pending_overflow += 1;
                }
                self.pending.push_back((
                    attempt_key,
                    OpenAttempt {
                        provider,
                        model,
                        at: at.to_string(),
                    },
                ));
            }
            OperatorEventType::RouteCompleted => {
                self.close(&attempt_key);
                let channel = ObservedChannel::from_payload(&payload["channel"]);
                self.success(provider, channel, model, at, order);
            }
            _ => {
                self.close(&attempt_key);
                let channel = ObservedChannel::from_payload(&payload["channel"]);
                let class = payload["failure_class"]
                    .as_str()
                    .filter(|class| KNOWN_FAILURE_CLASSES.contains(class))
                    .unwrap_or("unclassified");
                if class == "cancelled" {
                    self.excluded.cancelled += 1;
                    return;
                }
                if payload["provider_invoked"] == Value::Bool(false) {
                    self.excluded.not_invoked += 1;
                    return;
                }
                let origin = match payload["failure_origin"].as_str() {
                    Some("provider") => "provider",
                    Some("accounting") => {
                        self.excluded.accounting += 1;
                        self.success(provider, channel, model, at, order);
                        return;
                    }
                    Some("resolver") => {
                        self.excluded.not_invoked += 1;
                        return;
                    }
                    _ => "unverified",
                };
                let row = self.rows.entry((provider, channel)).or_default();
                row.failures += 1;
                row.last_failure = Some((
                    order,
                    FailureObservation {
                        at: at.to_string(),
                        model,
                        class: class.to_string(),
                        origin: origin.to_string(),
                    },
                ));
            }
        }
    }

    fn success(
        &mut self,
        provider: String,
        channel: ObservedChannel,
        model: String,
        at: &str,
        order: u64,
    ) {
        let row = self.rows.entry((provider, channel)).or_default();
        row.successes += 1;
        row.last_success = Some((
            order,
            SuccessObservation {
                at: at.to_string(),
                model,
            },
        ));
    }

    fn close(&mut self, attempt_key: &str) {
        if let Some(index) = self.pending.iter().position(|(key, _)| key == attempt_key) {
            self.pending.remove(index);
        }
    }

    pub fn events_inspected(&self) -> usize {
        self.events_inspected
    }

    pub fn view(&self, evidence: ExecutionEvidence) -> ProviderExecutionView {
        let observations = self
            .rows
            .iter()
            .map(|((provider, channel), row)| {
                let success_order = row.last_success.as_ref().map(|(order, _)| *order);
                let failure_order = row.last_failure.as_ref().map(|(order, _)| *order);
                ProviderExecution {
                    provider: provider.clone(),
                    channel: channel.clone(),
                    last_success: row.last_success.as_ref().map(|(_, seen)| seen.clone()),
                    last_failure: row.last_failure.as_ref().map(|(_, seen)| seen.clone()),
                    latest: if failure_order > success_order {
                        LatestOutcome::Failure
                    } else {
                        LatestOutcome::Success
                    },
                    successes: row.successes,
                    failures: row.failures,
                }
            })
            .collect();
        ProviderExecutionView {
            evidence: ExecutionEvidence {
                window_from: self.first_seen_at.clone(),
                events_inspected: self.events_inspected,
                ..evidence
            },
            observations,
            open_attempts: self.pending.iter().map(|(_, open)| open.clone()).collect(),
            excluded: self.excluded.clone(),
        }
    }
}

/// [`observe_journal_tail`] for an evidence root that may not exist yet.
/// Creates nothing: a missing root or stream is `empty`.
pub fn observe_root_tail(
    root: &std::path::Path,
    max_bytes: u64,
    max_events: usize,
) -> ProviderExecutionView {
    let stream = root.join(format!("{}.jsonl", heiwa_evidence::OPERATOR_STREAM_KIND));
    if !root.is_dir() || !stream.exists() {
        return observe_tail(&OperatorTail {
            events: Vec::new(),
            stream_present: false,
            starts_mid_stream: false,
            dropped_events: 0,
            skipped_lines: 0,
            window_bytes: 0,
        });
    }
    match OperatorJournal::new(root.to_path_buf()) {
        Ok(journal) => observe_journal_tail(&journal, max_bytes, max_events),
        Err(_) => unavailable("storage"),
    }
}

/// A view for evidence that could not be read at all.
pub fn unavailable(error: &'static str) -> ProviderExecutionView {
    ProviderExecutionFold::default().view(ExecutionEvidence {
        state: EvidenceState::Unavailable,
        source: "journal_tail",
        window_from: None,
        events_inspected: 0,
        skipped_lines: 0,
        starts_mid_stream: false,
        dropped_events: 0,
        error: Some(error),
    })
}

/// Observations from the newest events in a bounded tail of the journal.
///
/// Event-id dedup and schema checks apply within the window; a duplicate
/// whose first copy precedes the window re-observes the same fact. Reads at
/// most `max_bytes` of events plus the journal's bounded lineage work (see
/// [`OperatorJournal::read_tail`]).
pub fn observe_journal_tail(
    journal: &OperatorJournal,
    max_bytes: u64,
    max_events: usize,
) -> ProviderExecutionView {
    match journal.read_tail(max_bytes, max_events) {
        Ok(tail) => observe_tail(&tail),
        Err(error) => unavailable(match error {
            CursorError::UnstableLineage { .. } => "unstable_lineage",
            CursorError::InvalidCursor { .. } => "invalid_cursor",
            CursorError::Storage(_) => "storage",
        }),
    }
}

pub fn observe_tail(tail: &OperatorTail) -> ProviderExecutionView {
    let mut fold = ProviderExecutionFold::default();
    let mut seen = HashSet::new();
    for row in &tail.events {
        if !seen.insert(row.event.event_id.as_str())
            || row.event.schema_version != OPERATOR_EVENT_SCHEMA_VERSION
        {
            continue;
        }
        fold.observe(&row.event);
    }
    let state = if !tail.stream_present {
        EvidenceState::Empty
    } else if tail.is_complete() {
        EvidenceState::Complete
    } else {
        EvidenceState::Partial
    };
    fold.view(ExecutionEvidence {
        state,
        source: "journal_tail",
        window_from: None,
        events_inspected: 0,
        skipped_lines: tail.skipped_lines,
        starts_mid_stream: tail.starts_mid_stream,
        dropped_events: tail.dropped_events,
        error: None,
    })
}

fn provider_name(raw: &str) -> Option<String> {
    (!raw.is_empty()
        && raw.len() <= 64
        && raw.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        }))
    .then(|| raw.to_string())
}

/// Non-secret identifiers (account ids, binaries, model ids): bounded and
/// free of whitespace or control characters.
fn identifier(raw: &str) -> Option<String> {
    (!raw.is_empty()
        && raw.len() <= MAX_IDENTIFIER_LEN
        && raw
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-:/@+".contains(&byte)))
    .then(|| raw.to_string())
}

/// An RFC 3339-shaped timestamp; parsing is left to presentation.
fn timestamp(raw: &str) -> Option<&str> {
    (raw.len() >= 20
        && raw.len() <= 40
        && raw.as_bytes()[4] == b'-'
        && raw
            .bytes()
            .all(|byte| byte.is_ascii_digit() || b"-:.TZ+".contains(&byte)))
    .then_some(raw)
}
