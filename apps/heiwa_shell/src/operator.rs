use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use heiwa_core::drex::{ExecutionLocality, ModelCallCandidate, ModelCallRequest, PrivacyClass};
use heiwa_evidence::{
    find_sensitive, journal_root, now_iso, CursorEvent, OperatorActor, OperatorEvent,
    OperatorEventType, OperatorRisk, OperatorSensitivity, PersistedArtifact,
    OPERATOR_EVENT_SCHEMA_VERSION,
};
use heiwa_protocol::ExecutionScope;
use heiwa_provider::adapter::{Message, StreamEvent};
use heiwa_session::operator::{
    OperatorSessionService, RouteMode, StartTurnRequest, TurnRoutePolicy,
};
use serde_json::json;
use tokio::sync::{broadcast, mpsc, watch};
use uuid::Uuid;

use crate::model_calls::{ModelCallError, ModelCallExecution, ModelCallExecutor, ModelCallResult};

const OPERATOR_STREAM_CAPACITY: usize = 128;

#[derive(Default, Clone)]
pub struct ActiveTurnRegistry {
    turns: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
}

impl ActiveTurnRegistry {
    pub fn register(&self, turn_id: String) -> watch::Receiver<bool> {
        let (sender, receiver) = watch::channel(false);
        if let Ok(mut turns) = self.turns.lock() {
            turns.insert(turn_id, sender);
        }
        receiver
    }

    pub fn signal_cancel(&self, turn_id: &str) -> bool {
        self.turns
            .lock()
            .ok()
            .and_then(|turns| turns.get(turn_id).cloned())
            .is_some_and(|sender| sender.send(true).is_ok())
    }

    pub fn remove(&self, turn_id: &str) {
        if let Ok(mut turns) = self.turns.lock() {
            turns.remove(turn_id);
        }
    }

    pub fn contains(&self, turn_id: &str) -> bool {
        self.turns
            .lock()
            .map(|turns| turns.contains_key(turn_id))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone)]
pub enum OperatorStreamFrame {
    Durable(Box<CursorEvent>),
    AssistantDelta {
        thread_id: String,
        turn_id: String,
        text: String,
    },
    Error {
        thread_id: String,
        turn_id: String,
        message: String,
    },
}

impl OperatorStreamFrame {
    pub fn is_terminal(&self) -> bool {
        match self {
            Self::Error { .. } => true,
            Self::Durable(row) => matches!(
                row.event.event_type,
                OperatorEventType::TurnCompleted
                    | OperatorEventType::TurnInterrupted
                    | OperatorEventType::Blocker
            ),
            Self::AssistantDelta { .. } => false,
        }
    }
}

#[async_trait]
pub trait OperatorModelExecutor: Send + Sync {
    async fn execute(
        &self,
        execution: ModelCallExecution,
    ) -> std::result::Result<ModelCallResult, ModelCallError>;
}

#[async_trait]
impl OperatorModelExecutor for ModelCallExecutor {
    async fn execute(
        &self,
        execution: ModelCallExecution,
    ) -> std::result::Result<ModelCallResult, ModelCallError> {
        ModelCallExecutor::execute(self, execution).await
    }
}

pub type DonePayload = Arc<dyn Fn(&ModelCallResult) -> serde_json::Value + Send + Sync>;

#[derive(Debug, Clone)]
pub struct CommittedOperatorArtifact {
    pub artifact_id: String,
    pub artifact_ref: String,
    path: PathBuf,
}

pub trait OperatorArtifactStore: Send + Sync {
    /// Commit raw data reversibly, then return the exact durable reference.
    /// The runner rolls this back if its matching `artifact_created` journal
    /// append fails, keeping both planes symmetric.
    fn commit(&self, artifact: PersistedArtifact) -> Result<CommittedOperatorArtifact>;
    fn rollback(&self, artifact: &CommittedOperatorArtifact) -> Result<()>;
}

/// Injectable boundary around the existing DREX approval policy, staging, and
/// blocking decision watcher. The runner can race only the watcher against a
/// cancellation signal; it never moves a blocking filesystem poll onto Tokio.
pub trait OperatorApprovalService: Send + Sync {
    fn plan(&self, call: &heiwa_protocol::ToolCall) -> crate::agentic::ToolApproval;
    fn stage(
        &self,
        call: &heiwa_protocol::ToolCall,
        approval: &crate::agentic::ToolApproval,
    ) -> Result<()>;
    fn wait(
        &self,
        approval: &crate::agentic::ToolApproval,
        cancelled: &AtomicBool,
    ) -> Result<String>;
}

#[derive(Default)]
struct DrexApprovalService;

impl OperatorApprovalService for DrexApprovalService {
    fn plan(&self, call: &heiwa_protocol::ToolCall) -> crate::agentic::ToolApproval {
        crate::agentic::plan_tool_approval(call)
    }

    fn stage(
        &self,
        call: &heiwa_protocol::ToolCall,
        approval: &crate::agentic::ToolApproval,
    ) -> Result<()> {
        crate::agentic::stage_tool_approval(call, approval)
    }

    fn wait(
        &self,
        approval: &crate::agentic::ToolApproval,
        cancelled: &AtomicBool,
    ) -> Result<String> {
        crate::agentic::wait_for_tool_approval_cancellable(approval, cancelled)
    }
}

#[derive(Default)]
struct LocalArtifactStore;

impl OperatorArtifactStore for LocalArtifactStore {
    fn commit(&self, artifact: PersistedArtifact) -> Result<CommittedOperatorArtifact> {
        let dir = journal_root()?.join("operator_artifacts");
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", artifact.artifact_id));
        let temp = dir.join(format!(".{}.{}.tmp", artifact.artifact_id, Uuid::new_v4()));
        let write_result = (|| -> Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(&serde_json::to_vec(&artifact)?)?;
            file.sync_all()?;
            fs::rename(&temp, &path)?;
            // Best-effort directory durability: filesystems that permit a
            // directory handle get the rename metadata flushed too.
            if let Ok(directory) = OpenOptions::new().read(true).open(&dir) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        write_result?;
        Ok(CommittedOperatorArtifact {
            artifact_id: artifact.artifact_id,
            artifact_ref: path.to_string_lossy().to_string(),
            path,
        })
    }

    fn rollback(&self, artifact: &CommittedOperatorArtifact) -> Result<()> {
        if artifact.path.exists() {
            fs::remove_file(&artifact.path)?;
        }
        Ok(())
    }
}

pub struct OperatorModelTurn {
    pub request: ModelCallRequest,
    pub candidates: Vec<ModelCallCandidate>,
    pub messages: Vec<Message>,
    pub remaining_budget_usd: Option<f64>,
    pub max_attempts: usize,
    pub tool_scope: Option<ExecutionScope>,
    pub done_payload: DonePayload,
}

pub enum OperatorTurnWork {
    Deterministic {
        response: String,
        route: serde_json::Value,
        done: serde_json::Value,
    },
    Model(Box<OperatorModelTurn>),
}

pub struct OperatorTurnHandle {
    pub thread_id: String,
    pub turn_id: String,
    pub cursor: String,
    pub duplicate: bool,
    sessions: Arc<OperatorSessionService>,
    replay: VecDeque<OperatorStreamFrame>,
    replay_complete: bool,
    seen_event_ids: HashSet<String>,
    frames: broadcast::Receiver<OperatorStreamFrame>,
    /// New submissions get a bounded, backpressured private stream. The
    /// global broadcast is intentionally lossy for observers and duplicates,
    /// but never carries the original caller's only copy of answer tokens.
    direct_frames: Option<mpsc::Receiver<OperatorStreamFrame>>,
}

impl OperatorTurnHandle {
    pub async fn recv(
        &mut self,
    ) -> std::result::Result<OperatorStreamFrame, broadcast::error::RecvError> {
        loop {
            if let Some(frame) = self.replay.pop_front() {
                if self.accept_frame(&frame) {
                    return Ok(frame);
                }
                continue;
            }
            if !self.replay_complete {
                self.refill_replay()
                    .map_err(|_| broadcast::error::RecvError::Closed)?;
                continue;
            }

            if self.direct_frames.is_some() {
                // Prefer every durable global row before taking the next
                // direct frame. `request_cancel` must stay synchronous, so
                // it only broadcasts its durable intent; this priority keeps
                // that intent ordered ahead of a direct terminal frame.
                match self.frames.try_recv() {
                    Ok(frame) if self.accept_global_frame(&frame) => return Ok(frame),
                    Ok(_) => continue,
                    Err(broadcast::error::TryRecvError::Lagged(_)) => {
                        self.refill_replay()
                            .map_err(|_| broadcast::error::RecvError::Closed)?;
                        continue;
                    }
                    Err(broadcast::error::TryRecvError::Closed) => {
                        // The original handle remains lossless while its
                        // private sender is alive, even if observers close.
                    }
                    Err(broadcast::error::TryRecvError::Empty) => {}
                }

                enum Incoming {
                    Direct(Option<OperatorStreamFrame>),
                    Global(std::result::Result<OperatorStreamFrame, broadcast::error::RecvError>),
                }
                let incoming = {
                    let direct_frames = self
                        .direct_frames
                        .as_mut()
                        .expect("direct frame receiver checked above");
                    tokio::select! {
                        biased;
                        global = self.frames.recv() => Incoming::Global(global),
                        direct = direct_frames.recv() => Incoming::Direct(direct),
                    }
                };
                match incoming {
                    Incoming::Direct(Some(frame)) if self.accept_frame(&frame) => return Ok(frame),
                    Incoming::Direct(Some(_)) => continue,
                    Incoming::Direct(None) => {
                        self.direct_frames = None;
                        continue;
                    }
                    Incoming::Global(Ok(frame)) if self.accept_global_frame(&frame) => {
                        return Ok(frame);
                    }
                    Incoming::Global(Ok(_)) => continue,
                    Incoming::Global(Err(broadcast::error::RecvError::Lagged(_))) => {
                        self.refill_replay()
                            .map_err(|_| broadcast::error::RecvError::Closed)?;
                        continue;
                    }
                    Incoming::Global(Err(broadcast::error::RecvError::Closed)) => continue,
                }
            }

            match self.frames.recv().await {
                Ok(frame) if self.accept_frame(&frame) => return Ok(frame),
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    self.refill_replay()
                        .map_err(|_| broadcast::error::RecvError::Closed)?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn accept_global_frame(&mut self, frame: &OperatorStreamFrame) -> bool {
        // The private channel owns original-turn terminal ordering. A global
        // terminal can race ahead of buffered direct token deltas, whereas a
        // global cancellation intent has no private counterpart and must be
        // surfaced before that direct terminal.
        matches!(frame, OperatorStreamFrame::Durable(_))
            && !frame.is_terminal()
            && self.accept_frame(frame)
    }

    fn accept_frame(&mut self, frame: &OperatorStreamFrame) -> bool {
        match frame {
            OperatorStreamFrame::Durable(row) => {
                if row.event.turn_id.as_deref() != Some(self.turn_id.as_str()) {
                    return false;
                }
                self.cursor = row.cursor.clone();
                self.seen_event_ids.insert(row.event.event_id.clone())
            }
            OperatorStreamFrame::AssistantDelta { turn_id, .. }
            | OperatorStreamFrame::Error { turn_id, .. } => turn_id == &self.turn_id,
        }
    }

    fn refill_replay(&mut self) -> Result<()> {
        let page = self.sessions.events_after(
            &self.thread_id,
            (!self.cursor.is_empty()).then_some(self.cursor.as_str()),
            256,
        )?;
        let caught_up = page.events.len() < 256;
        let reached_terminal = page.events.iter().any(|row| {
            row.event.turn_id.as_deref() == Some(self.turn_id.as_str())
                && matches!(
                    row.event.event_type,
                    OperatorEventType::TurnCompleted
                        | OperatorEventType::TurnInterrupted
                        | OperatorEventType::Blocker
                )
        });
        if let Some(cursor) = page.next_cursor {
            self.cursor = cursor;
        }
        self.replay.extend(
            page.events
                .into_iter()
                .filter(|row| row.event.turn_id.as_deref() == Some(self.turn_id.as_str()))
                .map(|row| OperatorStreamFrame::Durable(Box::new(row))),
        );
        self.replay_complete = caught_up || reached_terminal;
        Ok(())
    }
}

#[derive(Clone)]
pub struct OperatorTurnRunner {
    sessions: Arc<OperatorSessionService>,
    executor: Arc<dyn OperatorModelExecutor>,
    active: ActiveTurnRegistry,
    submissions: Arc<Mutex<()>>,
    recoverable_orphans: Arc<Mutex<HashSet<String>>>,
    active_threads: Arc<Mutex<HashMap<String, String>>>,
    frames: broadcast::Sender<OperatorStreamFrame>,
    artifacts: Arc<dyn OperatorArtifactStore>,
    approvals: Arc<dyn OperatorApprovalService>,
}

impl OperatorTurnRunner {
    pub fn new(
        sessions: Arc<OperatorSessionService>,
        executor: Arc<dyn OperatorModelExecutor>,
    ) -> Self {
        let (frames, _) = broadcast::channel(OPERATOR_STREAM_CAPACITY);
        Self {
            sessions,
            executor,
            active: ActiveTurnRegistry::default(),
            submissions: Arc::new(Mutex::new(())),
            recoverable_orphans: Arc::new(Mutex::new(HashSet::new())),
            active_threads: Arc::new(Mutex::new(HashMap::new())),
            frames,
            artifacts: Arc::new(LocalArtifactStore),
            approvals: Arc::new(DrexApprovalService),
        }
    }

    pub fn with_artifact_store(mut self, artifacts: Arc<dyn OperatorArtifactStore>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_approval_service(mut self, approvals: Arc<dyn OperatorApprovalService>) -> Self {
        self.approvals = approvals;
        self
    }

    pub fn active_turns(&self) -> &ActiveTurnRegistry {
        &self.active
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OperatorStreamFrame> {
        self.frames.subscribe()
    }

    pub fn submit(
        &self,
        thread_id: &str,
        request: StartTurnRequest,
        work: OperatorTurnWork,
    ) -> Result<OperatorTurnHandle> {
        let _submission = self
            .submissions
            .lock()
            .map_err(|_| anyhow!("operator submission mutex poisoned"))?;
        let route_policy = request.route_policy.clone();
        let frames = self.subscribe();
        let submission = self.sessions.start_turn(thread_id, request)?;
        let mut direct_frames = None;
        if !submission.duplicate {
            let cancel = self.active.register(submission.turn_id.clone());
            self.active_threads
                .lock()
                .map_err(|_| anyhow!("operator active turn mutex poisoned"))?
                .insert(submission.turn_id.clone(), thread_id.to_string());
            let (direct_tx, direct_rx) = mpsc::channel(OPERATOR_STREAM_CAPACITY);
            direct_frames = Some(direct_rx);
            let runner = self.clone();
            let thread_id = thread_id.to_string();
            let turn_id = submission.turn_id.clone();
            tokio::spawn(async move {
                runner
                    .run_and_close(thread_id, turn_id, route_policy, work, cancel, direct_tx)
                    .await;
            });
        } else if !self.active.contains(&submission.turn_id) {
            let owns_orphan = self
                .recoverable_orphans
                .lock()
                .map_err(|_| anyhow!("operator orphan mutex poisoned"))?
                .contains(&submission.turn_id);
            if owns_orphan {
                if let Some(row) = self
                    .sessions
                    .recover_proven_orphan(&submission.thread_id, &submission.turn_id)?
                {
                    let _ = self
                        .frames
                        .send(OperatorStreamFrame::Durable(Box::new(row)));
                }
                self.recoverable_orphans
                    .lock()
                    .map_err(|_| anyhow!("operator orphan mutex poisoned"))?
                    .remove(&submission.turn_id);
            } else if self
                .sessions
                .thread(&submission.thread_id)?
                .turns
                .iter()
                .any(|turn| turn.turn_id == submission.turn_id && turn.status == "open")
            {
                return Err(anyhow!(
                    "duplicate turn {} is open and owned by another runtime",
                    submission.turn_id
                ));
            }
        }
        let (cursor, replay_complete) = if submission.duplicate {
            (String::new(), false)
        } else {
            (submission.cursor.clone(), true)
        };
        Ok(OperatorTurnHandle {
            thread_id: submission.thread_id,
            turn_id: submission.turn_id,
            cursor,
            duplicate: submission.duplicate,
            sessions: self.sessions.clone(),
            replay: VecDeque::new(),
            replay_complete,
            seen_event_ids: HashSet::new(),
            frames,
            direct_frames,
        })
    }

    pub fn request_cancel(&self, turn_id: &str) -> Result<bool> {
        let Some(thread_id) = self
            .active_threads
            .lock()
            .map_err(|_| anyhow!("operator active turn mutex poisoned"))?
            .get(turn_id)
            .cloned()
        else {
            return Ok(false);
        };
        let row = self.sessions.append_event(runtime_event(
            &thread_id,
            turn_id,
            None,
            OperatorEventType::TurnCancelRequested,
            json!({"reason": "OPERATOR_REQUEST"}),
        ))?;
        let _ = self
            .frames
            .send(OperatorStreamFrame::Durable(Box::new(row)));
        Ok(self.active.signal_cancel(turn_id))
    }

    async fn run_and_close(
        &self,
        thread_id: String,
        turn_id: String,
        route_policy: TurnRoutePolicy,
        work: OperatorTurnWork,
        cancel: watch::Receiver<bool>,
        direct_frames: mpsc::Sender<OperatorStreamFrame>,
    ) {
        let result = self
            .run_turn(
                &thread_id,
                &turn_id,
                &route_policy,
                work,
                cancel.clone(),
                &direct_frames,
            )
            .await;
        if let Err(error) = result {
            let cancelled = *cancel.borrow();
            let payload = if cancelled {
                json!({"reason": "OPERATOR_CANCELLED"})
            } else {
                json!({"reason": "EXECUTION_FAILED", "message": bounded_text(&error.to_string(), 1024)})
            };
            if let Ok(row) = self.sessions.append_event(runtime_event(
                &thread_id,
                &turn_id,
                None,
                OperatorEventType::TurnInterrupted,
                payload,
            )) {
                self.publish_frame(
                    Some(&direct_frames),
                    OperatorStreamFrame::Durable(Box::new(row)),
                )
                .await;
            } else {
                if let Ok(mut orphans) = self.recoverable_orphans.lock() {
                    orphans.insert(turn_id.clone());
                }
                self.publish_frame(
                    Some(&direct_frames),
                    OperatorStreamFrame::Error {
                        thread_id: thread_id.clone(),
                        turn_id: turn_id.clone(),
                        message: error.to_string(),
                    },
                )
                .await;
            }
        }
        self.active.remove(&turn_id);
        if let Ok(mut active_threads) = self.active_threads.lock() {
            active_threads.remove(&turn_id);
        }
    }

    async fn run_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
        route_policy: &TurnRoutePolicy,
        work: OperatorTurnWork,
        cancel: watch::Receiver<bool>,
        direct_frames: &mpsc::Sender<OperatorStreamFrame>,
    ) -> Result<()> {
        let mut cursor = self
            .append_and_publish(
                runtime_event(
                    thread_id,
                    turn_id,
                    None,
                    OperatorEventType::AssistantStarted,
                    json!({}),
                ),
                Some(direct_frames),
            )
            .await?
            .cursor;

        match work {
            OperatorTurnWork::Deterministic {
                response,
                route,
                done,
            } => {
                let call_id = format!("call-{}", Uuid::new_v4());
                self.append_and_publish_at(
                    &mut cursor,
                    runtime_event(
                        thread_id,
                        turn_id,
                        Some(&call_id),
                        OperatorEventType::RoutePlanned,
                        route.clone(),
                    ),
                    Some(direct_frames),
                )
                .await?;
                let route_completed = self
                    .append_and_publish_at(
                        &mut cursor,
                        runtime_event(
                            thread_id,
                            turn_id,
                            Some(&call_id),
                            OperatorEventType::RouteCompleted,
                            route,
                        ),
                        Some(direct_frames),
                    )
                    .await?;
                self.publish_frame(
                    Some(direct_frames),
                    OperatorStreamFrame::AssistantDelta {
                        thread_id: thread_id.to_string(),
                        turn_id: turn_id.to_string(),
                        text: response.clone(),
                    },
                )
                .await;
                self.finish_turn(
                    &mut cursor,
                    thread_id,
                    turn_id,
                    response,
                    done,
                    None,
                    Some(route_completed.event.event_id),
                    direct_frames,
                )
                .await?;
            }
            OperatorTurnWork::Model(mut model) => {
                apply_route_policy(&mut model.request, route_policy)?;
                apply_candidate_policy(&mut model.candidates, route_policy);
                model.request.thread_id = thread_id.to_string();
                model.request.turn_id = turn_id.to_string();
                model.remaining_budget_usd =
                    stricter_budget(route_policy.turn_budget_usd, model.remaining_budget_usd);
                let result = self
                    .execute_model(
                        thread_id,
                        turn_id,
                        &mut cursor,
                        *model,
                        cancel,
                        direct_frames,
                    )
                    .await?;
                let done = (result.done_payload)(&result.result);
                let response = result.result.text.clone();
                self.finish_turn(
                    &mut cursor,
                    thread_id,
                    turn_id,
                    response,
                    done,
                    Some(result.result),
                    Some(result.receipt_ref),
                    direct_frames,
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn execute_model(
        &self,
        thread_id: &str,
        turn_id: &str,
        cursor: &mut String,
        model: OperatorModelTurn,
        cancel: watch::Receiver<bool>,
        direct_frames: &mpsc::Sender<OperatorStreamFrame>,
    ) -> Result<CompletedModelTurn> {
        let request_template = model.request.clone();
        let candidates = model.candidates.clone();
        let mut messages = model.messages.clone();
        let remaining_budget_usd = model.remaining_budget_usd;
        let max_attempts = model.max_attempts;
        let done_payload = model.done_payload;
        let first = self
            .execute_model_stage(
                thread_id,
                turn_id,
                cursor,
                model.request,
                model.candidates,
                model.messages,
                remaining_budget_usd,
                max_attempts,
                cancel.clone(),
                direct_frames,
            )
            .await?;
        let remaining_after_first = subtract_turn_budget(remaining_budget_usd, first.cost_usd)?;

        let tool_calls = crate::agentic::parse_tool_calls(&first.text);
        let Some(scope) = model.tool_scope else {
            let receipt_ref = first.route_receipt_ref.clone();
            return Ok(CompletedModelTurn {
                result: first,
                done_payload,
                receipt_ref,
            });
        };
        if tool_calls.is_empty() {
            let receipt_ref = first.route_receipt_ref.clone();
            return Ok(CompletedModelTurn {
                result: first,
                done_payload,
                receipt_ref,
            });
        }

        let mut tool_entries = Vec::new();
        for call in tool_calls {
            if *cancel.borrow() {
                return Err(anyhow!("operator turn cancelled"));
            }
            *cursor = self
                .append_and_publish(
                    runtime_event(
                        thread_id,
                        turn_id,
                        Some(&call.id),
                        OperatorEventType::ToolCallStarted,
                        json!({"name": call.name, "arguments": call.arguments}),
                    ),
                    Some(direct_frames),
                )
                .await?
                .cursor;
            // DREX owns the approval policy and packet format. We own the
            // ordered durable audit trail around it, so an append failure
            // prevents staging, waiting, and the side effect.
            let approval = self.approvals.plan(&call);
            let request_id = approval.request_id.clone();
            *cursor = self.append_and_publish(runtime_event(
                thread_id,
                turn_id,
                Some(&call.id),
                OperatorEventType::ApprovalRequested,
                json!({
                    "request_id": request_id,
                    "tool": call.name,
                    "call_id": call.id,
                    "risk": approval.risk.as_str(),
                    "surface": approval.surface,
                    "outcome": if approval.request_id.is_some() { "pending" } else { "auto_approved" },
                }),
            ), Some(direct_frames)).await?.cursor;
            let outcome = if approval.request_id.is_some() {
                self.approvals.stage(&call, &approval)?;
                let wait_approval = approval.clone();
                let approvals = self.approvals.clone();
                let approval_wait_cancelled = Arc::new(AtomicBool::new(false));
                let waiter_cancelled = approval_wait_cancelled.clone();
                let mut wait = tokio::task::spawn_blocking(move || {
                    approvals.wait(&wait_approval, &waiter_cancelled)
                });
                let mut approval_cancel = cancel.clone();
                tokio::select! {
                    waited = &mut wait => match waited {
                        Ok(Ok(outcome)) => outcome,
                        Ok(Err(error)) => format!("timeout: {error}"),
                        Err(error) => return Err(anyhow!("approval waiter failed: {error}")),
                    },
                    changed = approval_cancel.changed() => {
                        match changed {
                            Ok(()) if *approval_cancel.borrow() => {
                                approval_wait_cancelled.store(true, Ordering::Release);
                                match wait.await {
                                    Ok(Ok(_)) | Ok(Err(_)) => {}
                                    Err(error) => return Err(anyhow!("approval waiter failed: {error}")),
                                }
                                *cursor = self.append_and_publish(runtime_event(
                                    thread_id,
                                    turn_id,
                                    Some(&call.id),
                                    OperatorEventType::ApprovalDecided,
                                    json!({
                                        "request_id": approval.request_id,
                                        "tool": call.name,
                                        "call_id": call.id,
                                        "risk": approval.risk.as_str(),
                                        "surface": approval.surface,
                                        "outcome": "cancelled",
                                        "reason": "OPERATOR_CANCELLED",
                                    }),
                                ), Some(direct_frames)).await?.cursor;
                                return Err(anyhow!("operator turn cancelled"));
                            }
                            _ => return Err(anyhow!("operator cancellation channel closed")),
                        }
                    }
                }
            } else {
                "auto_approved".to_string()
            };
            *cursor = self.append_and_publish(runtime_event(
                thread_id,
                turn_id,
                Some(&call.id),
                OperatorEventType::ApprovalDecided,
                json!({
                    "request_id": approval.request_id,
                    "tool": call.name,
                    "call_id": call.id,
                    "risk": approval.risk.as_str(),
                    "surface": approval.surface,
                    "outcome": outcome,
                    "reason": if outcome == "approved" || outcome == "auto_approved" { serde_json::Value::Null } else { json!("policy_denied_or_timeout") },
                }),
            ), Some(direct_frames)).await?.cursor;
            if outcome != "approved" && outcome != "auto_approved" {
                return Err(anyhow!("tool approval did not permit execution: {outcome}"));
            }
            if *cancel.borrow() {
                return Err(anyhow!("operator turn cancelled"));
            }
            let mut tool_cancel = cancel.clone();
            let tool_execution = crate::agentic::execute_approved_tool_call(
                scope.clone(),
                call.clone(),
                &first.provider,
                &first.model_id,
            );
            tokio::pin!(tool_execution);
            let (receipt, entry) = tokio::select! {
                result = &mut tool_execution => result?,
                changed = tool_cancel.changed() => {
                    match changed {
                        Ok(()) if *tool_cancel.borrow() => {
                            *cursor = self.append_and_publish(runtime_event(
                                thread_id,
                                turn_id,
                                Some(&call.id),
                                OperatorEventType::ToolCallCompleted,
                                json!({
                                    "name": call.name,
                                    "status": "uncertain",
                                    "outcome": "uncertain",
                                    "reason": "OPERATOR_CANCELLED",
                                    "output": "",
                                    "output_preview": "",
                                    "artifact_ref": serde_json::Value::Null,
                                    "receipt_id": serde_json::Value::Null,
                                    "error": "tool outcome uncertain after cancellation",
                                }),
                            ), Some(direct_frames)).await?.cursor;
                            return Err(anyhow!("operator turn cancelled"));
                        }
                        _ => return Err(anyhow!("operator cancellation channel closed")),
                    }
                }
            };
            if *cancel.borrow() {
                return Err(anyhow!("operator turn cancelled"));
            }
            let (preview, artifact_ref) = self
                .persist_large_tool_output(
                    cursor,
                    thread_id,
                    turn_id,
                    &call.id,
                    &call.name,
                    &entry.output,
                    direct_frames,
                )
                .await?;
            *cursor = self
                .append_and_publish(
                    runtime_event(
                        thread_id,
                        turn_id,
                        Some(&call.id),
                        OperatorEventType::ToolCallCompleted,
                        json!({
                            "name": call.name,
                            "status": receipt.status.as_str(),
                            "output": preview.clone(),
                            "output_preview": preview,
                            "artifact_ref": artifact_ref,
                            "receipt_id": receipt.id,
                            "error": receipt.error,
                        }),
                    ),
                    Some(direct_frames),
                )
                .await?
                .cursor;
            tool_entries.push(entry);
        }

        if *cancel.borrow() {
            return Err(anyhow!("operator turn cancelled"));
        }
        messages.push(Message {
            role: heiwa_provider::adapter::Role::Assistant,
            content: first.text,
        });
        messages.push(Message {
            role: heiwa_provider::adapter::Role::User,
            content: crate::agentic::tool_result_prompt(&tool_entries),
        });
        let mut follow_up = request_template;
        follow_up.call_id = format!("call-{}", Uuid::new_v4());
        follow_up.raw_text = crate::agentic::tool_result_prompt(&tool_entries);
        let result = self
            .execute_model_stage(
                thread_id,
                turn_id,
                cursor,
                follow_up,
                candidates,
                messages,
                remaining_after_first,
                max_attempts,
                cancel,
                direct_frames,
            )
            .await?;
        let receipt_ref = result.route_receipt_ref.clone();
        Ok(CompletedModelTurn {
            result,
            done_payload,
            receipt_ref,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_model_stage(
        &self,
        thread_id: &str,
        turn_id: &str,
        cursor: &mut String,
        request: ModelCallRequest,
        candidates: Vec<ModelCallCandidate>,
        messages: Vec<Message>,
        remaining_budget_usd: Option<f64>,
        max_attempts: usize,
        cancel: watch::Receiver<bool>,
        direct_frames: &mpsc::Sender<OperatorStreamFrame>,
    ) -> Result<ModelCallResult> {
        let (delta_tx, mut delta_rx) = mpsc::channel(32);
        let execution = ModelCallExecution {
            request,
            candidates,
            messages,
            remaining_budget_usd,
            max_attempts,
            cancel,
            delta_tx: Some(delta_tx),
        };
        let future = self.executor.execute(execution);
        tokio::pin!(future);
        let result = loop {
            tokio::select! {
                result = &mut future => break result.map_err(anyhow::Error::from)?,
                delta = delta_rx.recv() => {
                    let Some(delta) = delta else { continue; };
                    self.publish_durable_after(thread_id, cursor, Some(direct_frames)).await?;
                    if let StreamEvent::Token(text) = delta {
                        self.publish_frame(Some(direct_frames), OperatorStreamFrame::AssistantDelta {
                            thread_id: thread_id.to_string(), turn_id: turn_id.to_string(), text,
                        }).await;
                    }
                }
            }
        };
        self.publish_durable_after(thread_id, cursor, Some(direct_frames))
            .await?;
        while let Ok(delta) = delta_rx.try_recv() {
            if let StreamEvent::Token(text) = delta {
                self.publish_frame(
                    Some(direct_frames),
                    OperatorStreamFrame::AssistantDelta {
                        thread_id: thread_id.to_string(),
                        turn_id: turn_id.to_string(),
                        text,
                    },
                )
                .await;
            }
        }
        Ok(result)
    }

    async fn persist_large_tool_output(
        &self,
        cursor: &mut String,
        thread_id: &str,
        turn_id: &str,
        call_id: &str,
        tool_name: &str,
        output: &str,
        direct_frames: &mpsc::Sender<OperatorStreamFrame>,
    ) -> Result<(String, Option<String>)> {
        const MAX_OPERATOR_TOOL_OUTPUT_BYTES: usize = 16 * 1024;
        if tool_output_is_sensitive(output) {
            return Err(anyhow!("sensitive tool output cannot be persisted"));
        }
        if output.len() <= MAX_OPERATOR_TOOL_OUTPUT_BYTES {
            return Ok((output.to_string(), None));
        }
        let artifact_id = format!("artifact-{}", Uuid::new_v4());
        let committed = self.artifacts.commit(PersistedArtifact {
            artifact_id: artifact_id.clone(),
            run_id: None,
            lease_id: None,
            session_id: Some(thread_id.to_string()),
            user_id: "local-operator".to_string(),
            mission_id: turn_id.to_string(),
            cell_run_id: None,
            artifact_type: "tool_output".to_string(),
            title: format!("{tool_name} output"),
            uri: None,
            path: None,
            content_json: serde_json::to_string(output)?,
            created_at: now_iso(),
            owner_id: Some("local-operator".to_string()),
            principal_id: Some("operator-turn-runner".to_string()),
        })?;
        let row = self
            .append_and_publish(
                runtime_event(
                    thread_id,
                    turn_id,
                    Some(call_id),
                    OperatorEventType::ArtifactCreated,
                    json!({
                        "artifact_id": committed.artifact_id,
                        "artifact_ref": committed.artifact_ref,
                        "kind": "tool_output",
                        "tool_name": tool_name,
                        "byte_len": output.len(),
                    }),
                ),
                Some(direct_frames),
            )
            .await;
        let row = match row {
            Ok(row) => row,
            Err(error) => {
                if let Err(rollback_error) = self.artifacts.rollback(&committed) {
                    return Err(anyhow!(
                        "artifact link append failed: {error}; rollback failed: {rollback_error}"
                    ));
                }
                return Err(error);
            }
        };
        *cursor = row.cursor;
        Ok((
            bounded_text(output, MAX_OPERATOR_TOOL_OUTPUT_BYTES),
            Some(artifact_id),
        ))
    }

    async fn finish_turn(
        &self,
        cursor: &mut String,
        thread_id: &str,
        turn_id: &str,
        response: String,
        done: serde_json::Value,
        model: Option<ModelCallResult>,
        receipt_ref: Option<String>,
        direct_frames: &mpsc::Sender<OperatorStreamFrame>,
    ) -> Result<()> {
        *cursor = self
            .append_and_publish(
                runtime_event(
                    thread_id,
                    turn_id,
                    None,
                    OperatorEventType::AssistantCompleted,
                    json!({"text": response}),
                ),
                Some(direct_frames),
            )
            .await?
            .cursor;
        *cursor = self.append_and_publish(runtime_event(
            thread_id,
            turn_id,
            None,
            OperatorEventType::ReceiptLinked,
            json!({
                "kind": "operator_turn",
                "receipt_ref": receipt_ref.clone(),
                "text": receipt_ref.as_ref().map(|reference| format!("operator turn receipt {reference}")),
                "provider": model.as_ref().map(|result| result.provider.as_str()),
                "model": model.as_ref().map(|result| result.model_id.as_str()),
                "cost_usd": model.as_ref().map(|result| result.cost_usd),
                "cost_truth": model.as_ref().map(|result| &result.cost_truth),
            }),
        ), Some(direct_frames)).await?.cursor;
        *cursor = self
            .append_and_publish(
                runtime_event(
                    thread_id,
                    turn_id,
                    None,
                    OperatorEventType::TurnCompleted,
                    json!({"trace": done}),
                ),
                Some(direct_frames),
            )
            .await?
            .cursor;
        Ok(())
    }

    async fn append_and_publish(
        &self,
        event: OperatorEvent,
        direct_frames: Option<&mpsc::Sender<OperatorStreamFrame>>,
    ) -> Result<CursorEvent> {
        let row = self.sessions.append_event(event)?;
        self.publish_frame(
            direct_frames,
            OperatorStreamFrame::Durable(Box::new(row.clone())),
        )
        .await;
        Ok(row)
    }

    async fn append_and_publish_at(
        &self,
        cursor: &mut String,
        event: OperatorEvent,
        direct_frames: Option<&mpsc::Sender<OperatorStreamFrame>>,
    ) -> Result<CursorEvent> {
        let row = self.append_and_publish(event, direct_frames).await?;
        *cursor = row.cursor.clone();
        Ok(row)
    }

    async fn publish_durable_after(
        &self,
        thread_id: &str,
        cursor: &mut String,
        direct_frames: Option<&mpsc::Sender<OperatorStreamFrame>>,
    ) -> Result<()> {
        let page = self.sessions.events_after(thread_id, Some(cursor), 256)?;
        for row in page.events {
            *cursor = row.cursor.clone();
            self.publish_frame(direct_frames, OperatorStreamFrame::Durable(Box::new(row)))
                .await;
        }
        if let Some(next) = page.next_cursor {
            *cursor = next;
        }
        Ok(())
    }

    async fn publish_frame(
        &self,
        direct_frames: Option<&mpsc::Sender<OperatorStreamFrame>>,
        frame: OperatorStreamFrame,
    ) {
        if let Some(direct_frames) = direct_frames {
            let _ = direct_frames.send(frame.clone()).await;
        }
        let _ = self.frames.send(frame);
    }
}

struct CompletedModelTurn {
    result: ModelCallResult,
    done_payload: DonePayload,
    receipt_ref: String,
}

fn apply_route_policy(request: &mut ModelCallRequest, policy: &TurnRoutePolicy) -> Result<()> {
    request.preferred_provider = policy.preferred_provider.clone();
    request.preferred_model = policy.preferred_model.clone();
    request.allowed_models = policy.allowed_models.clone();
    request.excluded_models = policy.excluded_models.clone();
    request.minimum_quality_class = policy.minimum_quality_class;
    request.maximum_marginal_cost_usd = policy.maximum_marginal_cost_usd;
    request.privacy = PrivacyClass::parse(&policy.privacy).map_err(|error| anyhow!(error))?;
    match policy.mode {
        RouteMode::Auto => {}
        RouteMode::LocalOnly => request.privacy = PrivacyClass::LocalOnly,
        RouteMode::RemoteOnly => {}
        RouteMode::Explicit => {
            if request.preferred_provider.is_none() && request.preferred_model.is_none() {
                return Err(anyhow!(
                    "explicit route policy requires preferred_provider or preferred_model"
                ));
            }
        }
    }
    Ok(())
}

fn apply_candidate_policy(candidates: &mut Vec<ModelCallCandidate>, policy: &TurnRoutePolicy) {
    if policy.mode == RouteMode::RemoteOnly {
        candidates.retain(|candidate| candidate.locality != ExecutionLocality::OnDevice);
    }
}

fn stricter_budget(durable: Option<f64>, caller: Option<f64>) -> Option<f64> {
    match (durable, caller) {
        (Some(durable), Some(caller)) => Some(durable.min(caller)),
        (Some(durable), None) => Some(durable),
        (None, Some(caller)) => Some(caller),
        (None, None) => None,
    }
}

fn subtract_turn_budget(remaining: Option<f64>, spent: f64) -> Result<Option<f64>> {
    let Some(remaining) = remaining else {
        return Ok(None);
    };
    if !spent.is_finite() || spent < 0.0 {
        return Err(anyhow!("model result returned an invalid cumulative cost"));
    }
    Ok(Some((remaining - spent).max(0.0)))
}

fn runtime_event(
    thread_id: &str,
    turn_id: &str,
    call_id: Option<&str>,
    event_type: OperatorEventType,
    payload: serde_json::Value,
) -> OperatorEvent {
    OperatorEvent {
        schema_version: OPERATOR_EVENT_SCHEMA_VERSION,
        event_id: format!("evt-{}", Uuid::new_v4()),
        thread_id: thread_id.to_string(),
        turn_id: Some(turn_id.to_string()),
        run_id: None,
        call_id: call_id.map(str::to_string),
        event_type,
        occurred_at: now_iso(),
        actor: OperatorActor {
            kind: "runtime".to_string(),
            id: "operator-turn-runner".to_string(),
        },
        risk_class: OperatorRisk::Low,
        sensitivity: OperatorSensitivity::LocalPrivate,
        parent_event_id: None,
        correlation_id: call_id.map(str::to_string),
        source_refs: vec![],
        evidence_refs: vec![],
        payload,
    }
}

fn bounded_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn tool_output_is_sensitive(output: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(output) {
        Ok(value) => find_sensitive(&value).is_some(),
        Err(_) => find_sensitive(&json!(output)).is_some(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use heiwa_core::drex::{
        CallRisk, CostTruth, ExecutionLocality, ModelCallCandidate, ModelCallRequest,
        ModelCallStage, PrivacyClass, SafetyClass,
    };
    use heiwa_evidence::{OperatorEventType, OperatorJournal, PersistedArtifact};
    use heiwa_protocol::{ExecutionScope, ModelTier, RiskClass, ToolLease};
    use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent, TokenUsage};
    use heiwa_session::operator::{OperatorSessionService, StartTurnRequest};
    use serde_json::json;
    use tokio::sync::{mpsc, Notify};

    use super::{
        ActiveTurnRegistry, CommittedOperatorArtifact, OperatorApprovalService,
        OperatorArtifactStore, OperatorModelExecutor, OperatorModelTurn, OperatorStreamFrame,
        OperatorTurnRunner, OperatorTurnWork,
    };
    use crate::model_calls::{
        ModelCallError, ModelCallExecution, ModelCallExecutor, ModelCallResult,
    };

    #[derive(Default)]
    struct RecordingExecutor {
        calls: AtomicUsize,
        entered_after: Mutex<Vec<OperatorEventType>>,
        sessions: Mutex<Option<Arc<OperatorSessionService>>>,
        started: Notify,
        block: AtomicBool,
    }

    struct SequencedExecutor {
        calls: AtomicUsize,
        responses: Vec<String>,
    }

    struct BurstingExecutor {
        answer: String,
    }

    struct BudgetExecutor {
        calls: AtomicUsize,
        remaining_budgets: Mutex<Vec<Option<f64>>>,
    }

    struct DelayedApproval;

    struct CancellableApproval {
        active_waiters: AtomicUsize,
        entered: Notify,
    }

    impl OperatorApprovalService for DelayedApproval {
        fn plan(&self, _call: &heiwa_protocol::ToolCall) -> crate::agentic::ToolApproval {
            crate::agentic::ToolApproval {
                request_id: Some("approval-test".to_string()),
                request_path: Some(std::path::PathBuf::from("/tmp/approval-test")),
                risk: heiwa_drex::drex_gate::RiskLevel::Critical,
                surface: "test".to_string(),
            }
        }

        fn stage(
            &self,
            _call: &heiwa_protocol::ToolCall,
            _approval: &crate::agentic::ToolApproval,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn wait(
            &self,
            _approval: &crate::agentic::ToolApproval,
            _cancelled: &AtomicBool,
        ) -> anyhow::Result<String> {
            std::thread::sleep(std::time::Duration::from_millis(200));
            Ok("approved".to_string())
        }
    }

    impl OperatorApprovalService for CancellableApproval {
        fn plan(&self, _call: &heiwa_protocol::ToolCall) -> crate::agentic::ToolApproval {
            crate::agentic::ToolApproval {
                request_id: Some("approval-cancellable".to_string()),
                request_path: Some(std::path::PathBuf::from("/tmp/approval-cancellable")),
                risk: heiwa_drex::drex_gate::RiskLevel::Critical,
                surface: "test".to_string(),
            }
        }

        fn stage(
            &self,
            _call: &heiwa_protocol::ToolCall,
            _approval: &crate::agentic::ToolApproval,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn wait(
            &self,
            _approval: &crate::agentic::ToolApproval,
            cancelled: &AtomicBool,
        ) -> anyhow::Result<String> {
            self.active_waiters.fetch_add(1, Ordering::SeqCst);
            self.entered.notify_waiters();
            while !cancelled.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            self.active_waiters.fetch_sub(1, Ordering::SeqCst);
            Err(anyhow::anyhow!("approval wait cancelled"))
        }
    }

    struct CorruptingApproval {
        stream: std::path::PathBuf,
        backup: std::path::PathBuf,
        stage_calls: AtomicUsize,
        wait_calls: AtomicUsize,
    }

    impl OperatorApprovalService for CorruptingApproval {
        fn plan(&self, _call: &heiwa_protocol::ToolCall) -> crate::agentic::ToolApproval {
            std::fs::rename(&self.stream, &self.backup).unwrap();
            std::fs::create_dir(&self.stream).unwrap();
            crate::agentic::ToolApproval {
                request_id: Some("approval-corrupt".to_string()),
                request_path: Some(std::path::PathBuf::from("/tmp/approval-corrupt")),
                risk: heiwa_drex::drex_gate::RiskLevel::Critical,
                surface: "test".to_string(),
            }
        }

        fn stage(
            &self,
            _call: &heiwa_protocol::ToolCall,
            _approval: &crate::agentic::ToolApproval,
        ) -> anyhow::Result<()> {
            self.stage_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn wait(
            &self,
            _approval: &crate::agentic::ToolApproval,
            _cancelled: &AtomicBool,
        ) -> anyhow::Result<String> {
            self.wait_calls.fetch_add(1, Ordering::SeqCst);
            Ok("approved".to_string())
        }
    }

    struct CorruptingDecisionApproval {
        stream: std::path::PathBuf,
        backup: std::path::PathBuf,
        stage_calls: AtomicUsize,
        wait_calls: AtomicUsize,
    }

    impl OperatorApprovalService for CorruptingDecisionApproval {
        fn plan(&self, _call: &heiwa_protocol::ToolCall) -> crate::agentic::ToolApproval {
            crate::agentic::ToolApproval {
                request_id: Some("approval-decision-corrupt".to_string()),
                request_path: Some(std::path::PathBuf::from("/tmp/approval-decision-corrupt")),
                risk: heiwa_drex::drex_gate::RiskLevel::Critical,
                surface: "test".to_string(),
            }
        }

        fn stage(
            &self,
            _call: &heiwa_protocol::ToolCall,
            _approval: &crate::agentic::ToolApproval,
        ) -> anyhow::Result<()> {
            self.stage_calls.fetch_add(1, Ordering::SeqCst);
            std::fs::rename(&self.stream, &self.backup).unwrap();
            std::fs::create_dir(&self.stream).unwrap();
            Ok(())
        }

        fn wait(
            &self,
            _approval: &crate::agentic::ToolApproval,
            _cancelled: &AtomicBool,
        ) -> anyhow::Result<String> {
            self.wait_calls.fetch_add(1, Ordering::SeqCst);
            Ok("approved".to_string())
        }
    }

    struct CompletingAdapter;

    #[async_trait]
    impl ProviderAdapter for CompletingAdapter {
        async fn send(
            &self,
            _model: &str,
            _messages: &[Message],
            stream_tx: tokio::sync::mpsc::Sender<StreamEvent>,
        ) -> anyhow::Result<()> {
            stream_tx
                .send(StreamEvent::Token("model answer".into()))
                .await?;
            stream_tx
                .send(StreamEvent::Done(TokenUsage::default()))
                .await?;
            Ok(())
        }

        async fn interrupt(&self) -> anyhow::Result<()> {
            Ok(())
        }

        fn supported_models(&self) -> Vec<String> {
            vec!["fake-model".into()]
        }
    }

    #[async_trait]
    impl OperatorModelExecutor for SequencedExecutor {
        async fn execute(
            &self,
            _execution: ModelCallExecution,
        ) -> Result<ModelCallResult, ModelCallError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ModelCallResult {
                route_receipt_ref: "mock-route-receipt".to_string(),
                provider: "fake".into(),
                model_id: "fake-model".into(),
                provider_model_id: "fake-model".into(),
                rate_group: "test".into(),
                text: self.responses[index].clone(),
                usage: TokenUsage::default(),
                attempts: 1,
                failed_models: vec![],
                cost_usd: 0.0,
                cost_truth: CostTruth::LocalZeroCost,
                attempt_records: vec![],
            })
        }
    }

    #[async_trait]
    impl OperatorModelExecutor for BurstingExecutor {
        async fn execute(
            &self,
            execution: ModelCallExecution,
        ) -> Result<ModelCallResult, ModelCallError> {
            let deltas = execution
                .delta_tx
                .as_ref()
                .expect("runner supplies a delta channel");
            for byte in self.answer.bytes() {
                deltas
                    .send(StreamEvent::Token((byte as char).to_string()))
                    .await
                    .expect("runner remains subscribed");
            }
            Ok(ModelCallResult {
                route_receipt_ref: execution.request.call_id.clone(),
                provider: "fake".into(),
                model_id: "fake-model".into(),
                provider_model_id: "fake-model".into(),
                rate_group: "test".into(),
                text: self.answer.clone(),
                usage: TokenUsage::default(),
                attempts: 1,
                failed_models: vec![],
                cost_usd: 0.0,
                cost_truth: CostTruth::LocalZeroCost,
                attempt_records: vec![],
            })
        }
    }

    #[async_trait]
    impl OperatorModelExecutor for BudgetExecutor {
        async fn execute(
            &self,
            execution: ModelCallExecution,
        ) -> Result<ModelCallResult, ModelCallError> {
            self.remaining_budgets
                .lock()
                .unwrap()
                .push(execution.remaining_budget_usd);
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ModelCallResult {
                route_receipt_ref: execution.request.call_id.clone(),
                provider: "fake".into(),
                model_id: "fake-model".into(),
                provider_model_id: "fake-model".into(),
                rate_group: "test".into(),
                text: if index == 0 {
                    r#"{"tool_calls":[{"id":"tool-1","name":"fs.list","arguments":{"path":"."}}]}"#
                        .to_string()
                } else {
                    "final answer".to_string()
                },
                usage: TokenUsage::default(),
                attempts: 1,
                failed_models: vec![],
                cost_usd: 0.6,
                cost_truth: CostTruth::ProxyEstimate,
                attempt_records: vec![],
            })
        }
    }

    #[derive(Default)]
    struct RecordingArtifactStore {
        artifacts: Mutex<Vec<PersistedArtifact>>,
    }

    #[derive(Default)]
    struct FailingArtifactStore;

    impl OperatorArtifactStore for RecordingArtifactStore {
        fn commit(&self, artifact: PersistedArtifact) -> anyhow::Result<CommittedOperatorArtifact> {
            let artifact_id = artifact.artifact_id.clone();
            self.artifacts.lock().unwrap().push(artifact);
            Ok(CommittedOperatorArtifact {
                artifact_ref: format!("memory://{artifact_id}"),
                path: std::path::PathBuf::from(format!("memory-{artifact_id}")),
                artifact_id,
            })
        }

        fn rollback(&self, artifact: &CommittedOperatorArtifact) -> anyhow::Result<()> {
            self.artifacts
                .lock()
                .unwrap()
                .retain(|stored| stored.artifact_id != artifact.artifact_id);
            Ok(())
        }
    }

    impl OperatorArtifactStore for FailingArtifactStore {
        fn commit(
            &self,
            _artifact: PersistedArtifact,
        ) -> anyhow::Result<CommittedOperatorArtifact> {
            Err(anyhow::anyhow!("artifact commit failed"))
        }

        fn rollback(&self, _artifact: &CommittedOperatorArtifact) -> anyhow::Result<()> {
            Ok(())
        }
    }

    impl RecordingExecutor {
        fn with_sessions(sessions: Arc<OperatorSessionService>) -> Self {
            Self {
                sessions: Mutex::new(Some(sessions)),
                ..Self::default()
            }
        }
    }

    #[async_trait]
    impl OperatorModelExecutor for RecordingExecutor {
        async fn execute(
            &self,
            mut execution: ModelCallExecution,
        ) -> Result<ModelCallResult, ModelCallError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(sessions) = self.sessions.lock().unwrap().clone() {
                let events = sessions
                    .events_after(&execution.request.thread_id, None, 64)
                    .unwrap();
                *self.entered_after.lock().unwrap() = events
                    .events
                    .iter()
                    .map(|row| row.event.event_type.clone())
                    .collect();
            }
            self.started.notify_waiters();
            while self.block.load(Ordering::SeqCst) {
                if *execution.cancel.borrow() {
                    return Err(ModelCallError::Cancelled);
                }
                if execution.cancel.changed().await.is_err() {
                    return Err(ModelCallError::Cancelled);
                }
            }
            Ok(ModelCallResult {
                route_receipt_ref: execution.request.call_id.clone(),
                provider: "fake".into(),
                model_id: "fake-model".into(),
                provider_model_id: "fake-model".into(),
                rate_group: "test".into(),
                text: "model answer".into(),
                usage: TokenUsage {
                    input_tokens: 2,
                    output_tokens: 2,
                    cost_usd: 0.0,
                    ..TokenUsage::default()
                },
                attempts: 1,
                failed_models: vec![],
                cost_usd: 0.0,
                cost_truth: CostTruth::LocalZeroCost,
                attempt_records: vec![],
            })
        }
    }

    fn service(path: &std::path::Path) -> Arc<OperatorSessionService> {
        Arc::new(OperatorSessionService::new(
            OperatorJournal::new(path.to_path_buf()).unwrap(),
        ))
    }

    fn model_turn() -> OperatorModelTurn {
        OperatorModelTurn {
            request: ModelCallRequest {
                thread_id: String::new(),
                turn_id: String::new(),
                call_id: "call-1".into(),
                intent: "chat".into(),
                stage: ModelCallStage::Execution,
                raw_text: "hello".into(),
                privacy: PrivacyClass::Standard,
                risk: CallRisk::Low,
                safety: SafetyClass::low_risk_auto_approval(&CallRisk::Low),
                required_capabilities: vec![],
                required_context_tokens: 1,
                minimum_quality_class: 1,
                minimum_success_rate: 0.0,
                maximum_marginal_cost_usd: None,
                preferred_provider: None,
                preferred_model: None,
                allowed_models: vec![],
                excluded_models: vec![],
            },
            candidates: vec![],
            messages: vec![Message {
                role: Role::User,
                content: "hello".into(),
            }],
            remaining_budget_usd: None,
            max_attempts: 1,
            tool_scope: None,
            done_payload: Arc::new(|result| json!({"text": result.text})),
        }
    }

    async fn wait_for_terminal(handle: &mut super::OperatorTurnHandle) -> Vec<OperatorStreamFrame> {
        let mut frames = Vec::new();
        loop {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(2), handle.recv())
                .await
                .expect("runner timed out")
                .expect("runner stream closed");
            if frame.is_terminal() {
                frames.push(frame);
                return frames;
            }
            frames.push(frame);
        }
    }

    #[test]
    fn operator_active_turn_registry_registers_signals_and_removes() {
        let registry = ActiveTurnRegistry::default();
        let receiver = registry.register("turn-1".into());
        assert!(!*receiver.borrow());
        assert!(registry.signal_cancel("turn-1"));
        assert!(*receiver.borrow());
        registry.remove("turn-1");
        assert!(!registry.signal_cancel("turn-1"));
    }

    #[tokio::test]
    async fn operator_deterministic_turn_is_durable_and_streams_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        let runner = OperatorTurnRunner::new(sessions.clone(), executor);

        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Deterministic {
                    response: "deterministic answer".into(),
                    route: json!({"mode": "deterministic"}),
                    done: json!({"mode": "deterministic"}),
                },
            )
            .unwrap();
        wait_for_terminal(&mut handle).await;

        let events = sessions.events_after("default", None, 64).unwrap();
        let types = events
            .events
            .iter()
            .map(|row| row.event.event_type.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            types,
            vec![
                OperatorEventType::ThreadCreated,
                OperatorEventType::TurnStarted,
                OperatorEventType::UserMessage,
                OperatorEventType::AssistantStarted,
                OperatorEventType::RoutePlanned,
                OperatorEventType::RouteCompleted,
                OperatorEventType::AssistantCompleted,
                OperatorEventType::ReceiptLinked,
                OperatorEventType::TurnCompleted,
            ]
        );
    }

    #[tokio::test]
    async fn operator_model_executor_enters_only_after_durable_intent_and_assistant_start() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        let runner = OperatorTurnRunner::new(sessions, executor.clone());

        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        wait_for_terminal(&mut handle).await;

        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            *executor.entered_after.lock().unwrap(),
            vec![
                OperatorEventType::ThreadCreated,
                OperatorEventType::TurnStarted,
                OperatorEventType::UserMessage,
                OperatorEventType::AssistantStarted,
            ]
        );
    }

    #[tokio::test]
    async fn operator_original_handle_is_lossless_after_broadcast_lag() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let answer = "x".repeat(256);
        let runner = OperatorTurnRunner::new(
            sessions,
            Arc::new(BurstingExecutor {
                answer: answer.clone(),
            }),
        );
        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("lossless-original", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let frames = wait_for_terminal(&mut handle).await;
        let observed = frames
            .iter()
            .filter_map(|frame| match frame {
                OperatorStreamFrame::AssistantDelta { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(observed, answer);
    }

    #[tokio::test]
    async fn operator_durable_budget_caps_direct_runner_and_follow_up() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(BudgetExecutor {
            calls: AtomicUsize::new(0),
            remaining_budgets: Mutex::new(Vec::new()),
        });
        let runner = OperatorTurnRunner::new(sessions, executor.clone());
        let mut request = StartTurnRequest::auto("durable-budget", "hello");
        request.route_policy.turn_budget_usd = Some(0.5);
        let mut turn = model_turn();
        turn.remaining_budget_usd = Some(2.0);
        let mut scope = ExecutionScope::local_default(dir.path().to_path_buf());
        scope.tool_leases.push(ToolLease {
            name: "fs.list".into(),
            risk_class: RiskClass::HostSafeReadonly,
            allowed: true,
        });
        turn.tool_scope = Some(scope);

        let mut handle = runner
            .submit("default", request, OperatorTurnWork::Model(Box::new(turn)))
            .unwrap();
        wait_for_terminal(&mut handle).await;
        assert_eq!(
            *executor.remaining_budgets.lock().unwrap(),
            vec![Some(0.5), Some(0.0)]
        );
    }

    #[tokio::test]
    async fn operator_model_backed_turn_persists_route_receipt_and_terminal_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let adapter: Arc<dyn ProviderAdapter> = Arc::new(CompletingAdapter);
        let resolver = Arc::new(move |_provider: &str, _model: &str| Some(adapter.clone()));
        let executor = Arc::new(ModelCallExecutor::new(resolver, sessions.clone()));
        let runner = OperatorTurnRunner::new(sessions.clone(), executor);
        let mut turn = model_turn();
        turn.candidates = vec![ModelCallCandidate {
            tier: ModelTier {
                id: 1,
                model_id: "fake-model".into(),
                provider_model_id: "fake-model".into(),
                provider: "fake".into(),
                rate_group: "test".into(),
                capability_class: 3,
                effort_knob: "default".into(),
                effort_level: 1,
                cost_per_turn: 0.0,
                max_context_tokens: 8192,
                strengths_json: "[]".into(),
                vram_requirement_mb: 0,
                quantization_type: "none".into(),
                kv_cache_strategy: "none".into(),
                enabled: true,
                last_success_rate: 1.0,
                avg_latency_ms: 1,
                latency_p_95_ms: 1,
                updated_at: String::new(),
            },
            locality: ExecutionLocality::OnDevice,
            connected: true,
            adapter_capable: true,
            quota_available: true,
            marginal_cost_usd: Some(0.0),
            cost_truth: CostTruth::LocalZeroCost,
        }];
        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Model(Box::new(turn)),
            )
            .unwrap();
        wait_for_terminal(&mut handle).await;

        let rows = sessions.events_after("default", None, 64).unwrap().events;
        let types = rows
            .iter()
            .map(|row| row.event.event_type.clone())
            .collect::<Vec<_>>();
        assert!(types.windows(3).any(|events| {
            events
                == [
                    OperatorEventType::RoutePlanned,
                    OperatorEventType::RouteAttempted,
                    OperatorEventType::RouteCompleted,
                ]
        }));
        assert_eq!(
            types[types.len() - 3],
            OperatorEventType::AssistantCompleted
        );
        assert_eq!(types[types.len() - 2], OperatorEventType::ReceiptLinked);
        assert_eq!(types[types.len() - 1], OperatorEventType::TurnCompleted);
        let receipt_ref = rows[rows.len() - 2].event.payload["receipt_ref"]
            .as_str()
            .unwrap();
        assert!(rows.iter().any(|row| {
            row.event.event_id == receipt_ref
                && row.event.event_type == OperatorEventType::RouteCompleted
        }));
    }

    #[tokio::test]
    async fn operator_journal_append_failure_never_enters_executor() {
        let dir = tempfile::tempdir().unwrap();
        let evidence_path = dir.path().join("evidence");
        let sessions = service(&evidence_path);
        std::fs::remove_dir(&evidence_path).unwrap();
        std::fs::write(&evidence_path, "not a directory").unwrap();
        let executor = Arc::new(RecordingExecutor::default());
        let runner = OperatorTurnRunner::new(sessions, executor.clone());

        assert!(runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .is_err());
        assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn operator_duplicate_submission_does_not_spawn_twice() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        executor.block.store(true, Ordering::SeqCst);
        let runner = OperatorTurnRunner::new(sessions, executor.clone());

        let first = runner
            .submit(
                "default",
                StartTurnRequest::auto("same-request", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        let second = runner
            .submit(
                "default",
                StartTurnRequest::auto("same-request", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        assert_eq!(first.turn_id, second.turn_id);
        assert!(!first.duplicate);
        assert!(second.duplicate);
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            executor.started.notified(),
        )
        .await
        .unwrap();
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        runner.request_cancel(&first.turn_id).unwrap();
    }

    #[tokio::test]
    async fn operator_concurrent_duplicate_does_not_recover_registering_turn() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        executor.block.store(true, Ordering::SeqCst);
        let runner = OperatorTurnRunner::new(sessions.clone(), executor.clone());
        let request = StartTurnRequest::auto("serialized-duplicate", "hello");
        let duplicate_request = request.clone();
        let (first, second) = tokio::join!(
            async {
                runner.submit(
                    "default",
                    request,
                    OperatorTurnWork::Model(Box::new(model_turn())),
                )
            },
            async {
                runner.submit(
                    "default",
                    duplicate_request,
                    OperatorTurnWork::Model(Box::new(model_turn())),
                )
            }
        );
        let first = first.unwrap();
        let second = second.unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            executor.started.notified(),
        )
        .await
        .unwrap();
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        assert!(first.duplicate ^ second.duplicate);
        let rows = sessions.events_after("default", None, 64).unwrap().events;
        assert!(!rows.iter().any(|row| {
            row.event.event_type == OperatorEventType::TurnInterrupted
                && row.event.payload["reason"] == "RUNTIME_RESTART"
        }));
        runner.request_cancel(&first.turn_id).unwrap();
    }

    #[tokio::test]
    async fn operator_second_runner_cannot_recover_a_live_foreign_turn() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        executor.block.store(true, Ordering::SeqCst);
        let first_runner = OperatorTurnRunner::new(sessions.clone(), executor.clone());
        let second_runner = OperatorTurnRunner::new(sessions.clone(), executor.clone());
        let request = StartTurnRequest::auto("foreign-owner", "hello");
        let first = first_runner
            .submit(
                "default",
                request.clone(),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            executor.started.notified(),
        )
        .await
        .unwrap();

        let error = second_runner
            .submit(
                "default",
                request,
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .err()
            .expect("foreign live turn must not receive a handle");
        assert!(error.to_string().contains("owned by another runtime"));
        assert!(!sessions
            .events_after("default", None, 64)
            .unwrap()
            .events
            .iter()
            .any(|row| row.event.event_type == OperatorEventType::TurnInterrupted));
        first_runner.request_cancel(&first.turn_id).unwrap();
    }

    #[tokio::test]
    async fn operator_cancel_intent_is_durable_before_signal_and_runner_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        executor.block.store(true, Ordering::SeqCst);
        let runner = OperatorTurnRunner::new(sessions.clone(), executor.clone());

        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            executor.started.notified(),
        )
        .await
        .unwrap();
        assert!(runner.request_cancel(&handle.turn_id).unwrap());
        // Let the runner observe its cancellation and queue the direct
        // terminal before the handle starts draining. The global durable
        // cancel intent must still win.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let frames = wait_for_terminal(&mut handle).await;

        let event_ids = frames
            .iter()
            .filter_map(|frame| match frame {
                OperatorStreamFrame::Durable(row) => Some(row.event.event_id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let unique = event_ids.iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(
            event_ids.len(),
            unique.len(),
            "durable frames must be unique"
        );

        let events = sessions.events_after("default", None, 64).unwrap();
        let tail = &events.events[events.events.len() - 2..];
        assert_eq!(
            tail[0].event.event_type,
            OperatorEventType::TurnCancelRequested
        );
        assert_eq!(tail[1].event.event_type, OperatorEventType::TurnInterrupted);
        assert_eq!(tail[1].event.payload["reason"], "OPERATOR_CANCELLED");
        assert!(!runner.active_turns().signal_cancel(&handle.turn_id));
    }

    #[tokio::test]
    async fn operator_original_handle_observes_cancel_intent_before_interruption() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions));
        executor.block.store(true, Ordering::SeqCst);
        let runner = OperatorTurnRunner::new(service(dir.path()), executor.clone());
        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("original-cancel-sequence", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            executor.started.notified(),
        )
        .await
        .unwrap();
        assert!(runner.request_cancel(&handle.turn_id).unwrap());
        let frames = wait_for_terminal(&mut handle).await;
        let event_types = frames
            .iter()
            .filter_map(|frame| match frame {
                OperatorStreamFrame::Durable(row) => Some(row.event.event_type.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let cancel = event_types
            .iter()
            .position(|event| *event == OperatorEventType::TurnCancelRequested)
            .expect("original handle must receive durable cancel intent");
        let interrupted = event_types
            .iter()
            .position(|event| *event == OperatorEventType::TurnInterrupted)
            .expect("original handle must receive durable interruption");
        assert!(cancel < interrupted);
    }

    #[tokio::test]
    async fn operator_cancel_append_failure_leaves_execution_running() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        executor.block.store(true, Ordering::SeqCst);
        let runner = OperatorTurnRunner::new(sessions, executor.clone());
        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "hello"),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            executor.started.notified(),
        )
        .await
        .unwrap();

        let stream = dir.path().join("operator_events.jsonl");
        let backup = dir.path().join("operator_events.backup");
        std::fs::rename(&stream, &backup).unwrap();
        std::fs::create_dir(&stream).unwrap();
        assert!(runner.request_cancel(&handle.turn_id).is_err());
        assert!(runner.active_turns().signal_cancel("missing") == false);
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

        std::fs::remove_dir(&stream).unwrap();
        std::fs::rename(&backup, &stream).unwrap();
        assert!(runner.request_cancel(&handle.turn_id).unwrap());
        wait_for_terminal(&mut handle).await;
    }

    #[tokio::test]
    async fn operator_duplicate_repairs_orphan_after_terminal_append_failure() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        executor.block.store(true, Ordering::SeqCst);
        let runner = OperatorTurnRunner::new(sessions.clone(), executor.clone());
        let request = StartTurnRequest::auto("orphan-repair", "hello");
        let mut original = runner
            .submit(
                "default",
                request.clone(),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            executor.started.notified(),
        )
        .await
        .unwrap();
        let stream = dir.path().join("operator_events.jsonl");
        let backup = dir.path().join("operator_events.backup");
        std::fs::rename(&stream, &backup).unwrap();
        std::fs::create_dir(&stream).unwrap();
        assert!(runner.active_turns().signal_cancel(&original.turn_id));
        let frames = wait_for_terminal(&mut original).await;
        assert!(frames
            .iter()
            .any(|frame| matches!(frame, OperatorStreamFrame::Error { .. })));
        std::fs::remove_dir(&stream).unwrap();
        std::fs::rename(&backup, &stream).unwrap();

        let mut duplicate = runner
            .submit(
                "default",
                request,
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        assert!(duplicate.duplicate);
        let frames = wait_for_terminal(&mut duplicate).await;
        assert!(frames.iter().any(|frame| matches!(
            frame,
            OperatorStreamFrame::Durable(row)
                if row.event.event_type == OperatorEventType::TurnInterrupted
                    && row.event.payload["reason"] == "RUNTIME_RESTART"
        )));
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn operator_tools_are_intent_first_and_large_output_is_artifact_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let data = "x".repeat(20 * 1024);
        std::fs::write(dir.path().join("large.txt"), data).unwrap();
        let sessions = service(&dir.path().join("evidence"));
        let executor = Arc::new(SequencedExecutor {
            calls: AtomicUsize::new(0),
            responses: vec![
                json!({"tool_calls": [{"id": "tool-1", "name": "fs.read", "arguments": {"path": "large.txt", "max_bytes": 32768}}]}).to_string(),
                "final answer".into(),
            ],
        });
        let artifacts = Arc::new(RecordingArtifactStore::default());
        let runner = OperatorTurnRunner::new(sessions.clone(), executor.clone())
            .with_artifact_store(artifacts.clone());
        let mut turn = model_turn();
        let mut scope = ExecutionScope::local_default(dir.path().to_path_buf());
        scope.tool_leases.push(ToolLease {
            name: "fs.read".into(),
            risk_class: RiskClass::HostSafeReadonly,
            allowed: true,
        });
        turn.tool_scope = Some(scope);

        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("request-1", "read it"),
                OperatorTurnWork::Model(Box::new(turn)),
            )
            .unwrap();
        let frames = wait_for_terminal(&mut handle).await;

        assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
        let events = sessions.events_after("default", None, 64).unwrap();
        let started = events
            .events
            .iter()
            .position(|row| row.event.event_type == OperatorEventType::ToolCallStarted)
            .unwrap();
        let completed = events
            .events
            .iter()
            .position(|row| row.event.event_type == OperatorEventType::ToolCallCompleted)
            .unwrap();
        let approval_requested = events
            .events
            .iter()
            .position(|row| row.event.event_type == OperatorEventType::ApprovalRequested)
            .unwrap();
        let approval_decided = events
            .events
            .iter()
            .position(|row| row.event.event_type == OperatorEventType::ApprovalDecided)
            .unwrap();
        assert!(started < completed);
        assert!(started < approval_requested);
        assert!(approval_requested < approval_decided && approval_decided < completed);
        assert_eq!(
            events.events[approval_requested].event.payload["outcome"],
            "auto_approved"
        );
        assert_eq!(
            events.events[approval_decided].event.payload["outcome"],
            "auto_approved"
        );
        let completed_payload = &events.events[completed].event.payload;
        assert!(completed_payload["output_preview"].as_str().unwrap().len() <= 16 * 1024);
        assert!(completed_payload["artifact_ref"].as_str().is_some());
        let stored = artifacts.artifacts.lock().unwrap();
        assert_eq!(stored.len(), 1);
        assert!(stored[0].content_json.len() > 16 * 1024);

        let streamed_ids = frames
            .iter()
            .filter_map(|frame| match frame {
                OperatorStreamFrame::Durable(row) => Some(row.event.event_id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for row in events.events.iter().filter(|row| {
            row.event.turn_id.as_deref() == Some(handle.turn_id.as_str())
                && !matches!(
                    row.event.event_type,
                    OperatorEventType::TurnStarted | OperatorEventType::UserMessage
                )
        }) {
            assert_eq!(
                streamed_ids
                    .iter()
                    .filter(|id| *id == &row.event.event_id)
                    .count(),
                1,
                "durable event {} must broadcast exactly once",
                row.event.event_id
            );
        }
    }

    #[tokio::test]
    async fn operator_rejects_sensitive_raw_tool_output_before_artifact_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let submission = sessions
            .start_turn(
                "default",
                StartTurnRequest::auto("sensitive-artifact", "hello"),
            )
            .unwrap();
        let artifacts = Arc::new(RecordingArtifactStore::default());
        let runner = OperatorTurnRunner::new(sessions, Arc::new(RecordingExecutor::default()))
            .with_artifact_store(artifacts.clone());
        let (direct, _receiver) = mpsc::channel(1);
        let mut cursor = submission.cursor;
        let output = format!("{}\nghp_secret-beyond-preview", "x".repeat(20 * 1024));

        assert!(runner
            .persist_large_tool_output(
                &mut cursor,
                "default",
                &submission.turn_id,
                "call-1",
                "fs.read",
                &output,
                &direct,
            )
            .await
            .is_err());
        assert!(artifacts.artifacts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn operator_rejects_sensitive_value_nested_in_json_tool_output() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let submission = sessions
            .start_turn(
                "default",
                StartTurnRequest::auto("sensitive-json-artifact", "hello"),
            )
            .unwrap();
        let artifacts = Arc::new(RecordingArtifactStore::default());
        let runner = OperatorTurnRunner::new(sessions, Arc::new(RecordingExecutor::default()))
            .with_artifact_store(artifacts.clone());
        let (direct, _receiver) = mpsc::channel(1);
        let mut cursor = submission.cursor;
        let output = json!({
            "padding": "x".repeat(20 * 1024),
            "nested": {"token": "ghp_secret-beyond-preview"},
        })
        .to_string();

        assert!(runner
            .persist_large_tool_output(
                &mut cursor,
                "default",
                &submission.turn_id,
                "call-1",
                "fs.read",
                &output,
                &direct,
            )
            .await
            .is_err());
        assert!(artifacts.artifacts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn operator_artifact_append_failure_leaves_no_committed_raw_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let submission = sessions
            .start_turn(
                "default",
                StartTurnRequest::auto("artifact-rollback", "hello"),
            )
            .unwrap();
        let artifacts = Arc::new(RecordingArtifactStore::default());
        let runner = OperatorTurnRunner::new(sessions, Arc::new(RecordingExecutor::default()))
            .with_artifact_store(artifacts.clone());
        let stream = dir.path().join("operator_events.jsonl");
        let backup = dir.path().join("operator_events.backup");
        std::fs::rename(&stream, &backup).unwrap();
        std::fs::create_dir(&stream).unwrap();
        let (direct, _receiver) = mpsc::channel(1);
        let mut cursor = submission.cursor;

        assert!(runner
            .persist_large_tool_output(
                &mut cursor,
                "default",
                &submission.turn_id,
                "call-1",
                "fs.read",
                &"x".repeat(20 * 1024),
                &direct,
            )
            .await
            .is_err());
        assert!(artifacts.artifacts.lock().unwrap().is_empty());
        std::fs::remove_dir(&stream).unwrap();
        std::fs::rename(&backup, &stream).unwrap();
    }

    #[tokio::test]
    async fn operator_artifact_commit_failure_never_appends_artifact_created() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let submission = sessions
            .start_turn(
                "default",
                StartTurnRequest::auto("artifact-commit-failure", "hello"),
            )
            .unwrap();
        let runner =
            OperatorTurnRunner::new(sessions.clone(), Arc::new(RecordingExecutor::default()))
                .with_artifact_store(Arc::new(FailingArtifactStore));
        let (direct, _receiver) = mpsc::channel(1);
        let mut cursor = submission.cursor;

        assert!(runner
            .persist_large_tool_output(
                &mut cursor,
                "default",
                &submission.turn_id,
                "call-1",
                "fs.read",
                &"x".repeat(20 * 1024),
                &direct,
            )
            .await
            .is_err());
        assert!(!sessions
            .events_after("default", None, 64)
            .unwrap()
            .events
            .iter()
            .any(|row| row.event.event_type == OperatorEventType::ArtifactCreated));
    }

    #[tokio::test]
    async fn operator_completed_duplicate_replays_terminal_without_hanging() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        let runner = OperatorTurnRunner::new(sessions, executor);
        let request = StartTurnRequest::auto("same-request", "hello");
        let mut first = runner
            .submit(
                "default",
                request.clone(),
                OperatorTurnWork::Deterministic {
                    response: "done".into(),
                    route: json!({"mode": "deterministic"}),
                    done: json!({"mode": "deterministic"}),
                },
            )
            .unwrap();
        wait_for_terminal(&mut first).await;

        let mut duplicate = runner
            .submit(
                "default",
                request,
                OperatorTurnWork::Deterministic {
                    response: "must not run".into(),
                    route: json!({}),
                    done: json!({}),
                },
            )
            .unwrap();
        assert!(duplicate.duplicate);
        let frames = wait_for_terminal(&mut duplicate).await;
        assert!(frames.iter().any(OperatorStreamFrame::is_terminal));
    }

    #[tokio::test]
    async fn operator_completed_duplicate_pages_bounded_replay_to_old_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        let runner = OperatorTurnRunner::new(sessions, executor);

        for index in 0..30 {
            let mut handle = runner
                .submit(
                    "default",
                    StartTurnRequest::auto(format!("old-{index}"), "hello"),
                    OperatorTurnWork::Deterministic {
                        response: format!("done-{index}"),
                        route: json!({"mode": "deterministic"}),
                        done: json!({"mode": "deterministic"}),
                    },
                )
                .unwrap();
            wait_for_terminal(&mut handle).await;
        }

        let request = StartTurnRequest::auto("paged-target", "hello");
        let mut first = runner
            .submit(
                "default",
                request.clone(),
                OperatorTurnWork::Deterministic {
                    response: "target done".into(),
                    route: json!({"mode": "deterministic"}),
                    done: json!({"mode": "deterministic"}),
                },
            )
            .unwrap();
        wait_for_terminal(&mut first).await;

        let mut duplicate = runner
            .submit(
                "default",
                request,
                OperatorTurnWork::Deterministic {
                    response: "must not run".into(),
                    route: json!({}),
                    done: json!({}),
                },
            )
            .unwrap();
        let frames = wait_for_terminal(&mut duplicate).await;
        assert!(duplicate.duplicate);
        assert!(frames.iter().any(OperatorStreamFrame::is_terminal));
    }

    #[tokio::test]
    async fn operator_active_duplicate_follows_live_terminal_without_hanging() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(RecordingExecutor::with_sessions(sessions.clone()));
        executor.block.store(true, Ordering::SeqCst);
        let runner = OperatorTurnRunner::new(sessions, executor.clone());
        let request = StartTurnRequest::auto("same-request", "hello");
        let first = runner
            .submit(
                "default",
                request.clone(),
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            executor.started.notified(),
        )
        .await
        .unwrap();
        let mut duplicate = runner
            .submit(
                "default",
                request,
                OperatorTurnWork::Model(Box::new(model_turn())),
            )
            .unwrap();
        assert!(duplicate.duplicate);
        runner.request_cancel(&first.turn_id).unwrap();
        let frames = wait_for_terminal(&mut duplicate).await;
        assert!(frames.iter().any(|frame| matches!(
            frame,
            OperatorStreamFrame::Durable(row)
                if row.event.event_type == OperatorEventType::TurnInterrupted
        )));
    }

    #[tokio::test]
    async fn operator_cancelled_gated_approval_returns_promptly_without_tool_completion() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let executor = Arc::new(SequencedExecutor {
            calls: AtomicUsize::new(0),
            responses: vec![
                r#"{"tool_calls":[{"id":"deploy-1","name":"app.deploy","arguments":{}}]}"#
                    .to_string(),
            ],
        });
        let runner = OperatorTurnRunner::new(sessions.clone(), executor)
            .with_approval_service(Arc::new(DelayedApproval));
        let mut turn = model_turn();
        turn.tool_scope = Some(ExecutionScope::local_default(dir.path().to_path_buf()));
        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("gated-cancel", "deploy"),
                OperatorTurnWork::Model(Box::new(turn)),
            )
            .unwrap();

        loop {
            let frame = tokio::time::timeout(std::time::Duration::from_millis(300), handle.recv())
                .await
                .expect("approval request was not observable")
                .expect("runner stream closed");
            if matches!(frame, OperatorStreamFrame::Durable(row) if row.event.event_type == OperatorEventType::ApprovalRequested)
            {
                break;
            }
        }
        assert!(runner.request_cancel(&handle.turn_id).unwrap());
        let frames = wait_for_terminal(&mut handle).await;
        assert!(frames.iter().any(|frame| matches!(
            frame,
            OperatorStreamFrame::Durable(row)
                if row.event.event_type == OperatorEventType::ApprovalDecided
                    && row.event.payload["outcome"] == "cancelled"
        )));
        let rows = sessions.events_after("default", None, 64).unwrap().events;
        assert!(!rows
            .iter()
            .any(|row| row.event.event_type == OperatorEventType::ToolCallCompleted));
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let rows = sessions.events_after("default", None, 64).unwrap().events;
        assert!(!rows
            .iter()
            .any(|row| row.event.event_type == OperatorEventType::ToolCallCompleted));
    }

    #[tokio::test]
    async fn operator_cancelled_approval_waiter_terminates_before_turn_closes() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let approval = Arc::new(CancellableApproval {
            active_waiters: AtomicUsize::new(0),
            entered: Notify::new(),
        });
        let executor = Arc::new(SequencedExecutor {
            calls: AtomicUsize::new(0),
            responses: vec![
                r#"{"tool_calls":[{"id":"deploy-1","name":"app.deploy","arguments":{}}]}"#
                    .to_string(),
            ],
        });
        let runner =
            OperatorTurnRunner::new(sessions, executor).with_approval_service(approval.clone());
        let mut turn = model_turn();
        turn.tool_scope = Some(ExecutionScope::local_default(dir.path().to_path_buf()));
        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("cancellable-approval", "deploy"),
                OperatorTurnWork::Model(Box::new(turn)),
            )
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            approval.entered.notified(),
        )
        .await
        .expect("approval waiter did not start");
        assert_eq!(approval.active_waiters.load(Ordering::SeqCst), 1);
        assert!(runner.request_cancel(&handle.turn_id).unwrap());
        wait_for_terminal(&mut handle).await;
        assert_eq!(approval.active_waiters.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn operator_approval_requested_append_failure_prevents_stage_wait_and_tool() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let stream = dir.path().join("operator_events.jsonl");
        let backup = dir.path().join("operator_events.backup");
        let probe = Arc::new(CorruptingApproval {
            stream: stream.clone(),
            backup: backup.clone(),
            stage_calls: AtomicUsize::new(0),
            wait_calls: AtomicUsize::new(0),
        });
        let executor = Arc::new(SequencedExecutor {
            calls: AtomicUsize::new(0),
            responses: vec![
                r#"{"tool_calls":[{"id":"deploy-1","name":"app.deploy","arguments":{}}]}"#
                    .to_string(),
            ],
        });
        let runner = OperatorTurnRunner::new(sessions, executor.clone())
            .with_approval_service(probe.clone());
        let mut turn = model_turn();
        turn.tool_scope = Some(ExecutionScope::local_default(dir.path().to_path_buf()));
        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("approval-request-append-failure", "deploy"),
                OperatorTurnWork::Model(Box::new(turn)),
            )
            .unwrap();
        wait_for_terminal(&mut handle).await;
        assert_eq!(probe.stage_calls.load(Ordering::SeqCst), 0);
        assert_eq!(probe.wait_calls.load(Ordering::SeqCst), 0);
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        std::fs::remove_dir(&stream).unwrap();
        std::fs::rename(&backup, &stream).unwrap();
    }

    #[tokio::test]
    async fn operator_approval_decided_append_failure_prevents_tool() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = service(dir.path());
        let stream = dir.path().join("operator_events.jsonl");
        let backup = dir.path().join("operator_events.backup");
        let probe = Arc::new(CorruptingDecisionApproval {
            stream: stream.clone(),
            backup: backup.clone(),
            stage_calls: AtomicUsize::new(0),
            wait_calls: AtomicUsize::new(0),
        });
        let executor = Arc::new(SequencedExecutor {
            calls: AtomicUsize::new(0),
            responses: vec![
                r#"{"tool_calls":[{"id":"deploy-1","name":"app.deploy","arguments":{}}]}"#
                    .to_string(),
            ],
        });
        let runner = OperatorTurnRunner::new(sessions, executor.clone())
            .with_approval_service(probe.clone());
        let mut turn = model_turn();
        turn.tool_scope = Some(ExecutionScope::local_default(dir.path().to_path_buf()));
        let mut handle = runner
            .submit(
                "default",
                StartTurnRequest::auto("approval-decision-append-failure", "deploy"),
                OperatorTurnWork::Model(Box::new(turn)),
            )
            .unwrap();
        wait_for_terminal(&mut handle).await;
        assert_eq!(probe.stage_calls.load(Ordering::SeqCst), 1);
        assert_eq!(probe.wait_calls.load(Ordering::SeqCst), 1);
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        std::fs::remove_dir(&stream).unwrap();
        std::fs::rename(&backup, &stream).unwrap();
    }
}
