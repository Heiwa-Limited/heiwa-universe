//! Heiwa internal envelope types, shaped to be compatible with the public A2A
//! agent-to-agent protocol (task/message/artifact + agent identity).
//!
//! See `docs/superpowers/specs/2026-05-13-heiwa-life-plane-stdb-shell-design.md`
//! and the A2A spec at https://a2a-protocol.org/dev/specification/.
//!
//! This crate is intentionally I/O-free. It defines the wire shape only;
//! transport (localhost-WS, STDB reducers, file spool) is owned by the runtime.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identity of a worker (local or remote). Mirrors the A2A "agent card"
/// concept but reduced to what Heiwa needs internally.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentIdentity {
    pub agent_id: String,
    pub node: String,
    pub class: WorkerClass,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub locality: Locality,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Locality {
    Local,
    RemoteTrusted,
    RemoteUntrusted,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerClass {
    ShellMachine,
    Embedding,
    LocalLongrun,
    SummarySmall,
    PlannerFrontier,
    CodeExecutor,
    BrowserAgent,
    ApprovalBroker,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Submitted,
    Working,
    InputRequired,
    AuthRequired,
    Completed,
    Canceled,
    Failed,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub context_id: Option<String>,
    pub state: TaskState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub from: Option<AgentIdentity>,
    pub assignee: Option<AgentIdentity>,
    pub messages: Vec<Message>,
    pub artifacts: Vec<Artifact>,
    pub risk_tier: RiskTier,
    pub approval_required: bool,
}

impl Task {
    pub fn new(class: WorkerClass) -> Self {
        let now = Utc::now();
        Self {
            task_id: Uuid::new_v4().to_string(),
            context_id: None,
            state: TaskState::Submitted,
            created_at: now,
            updated_at: now,
            from: None,
            assignee: None,
            messages: Vec::new(),
            artifacts: Vec::new(),
            risk_tier: match class {
                WorkerClass::ShellMachine | WorkerClass::SummarySmall | WorkerClass::Embedding => {
                    RiskTier::T0
                }
                WorkerClass::LocalLongrun | WorkerClass::CodeExecutor => RiskTier::T1,
                WorkerClass::BrowserAgent | WorkerClass::PlannerFrontier => RiskTier::T2,
                WorkerClass::ApprovalBroker => RiskTier::T3,
            },
            approval_required: matches!(
                class,
                WorkerClass::BrowserAgent | WorkerClass::ApprovalBroker
            ),
        }
    }

    pub fn transition(&mut self, next: TaskState) {
        self.state = next;
        self.updated_at = Utc::now();
    }

    pub fn append_message(&mut self, message: Message) {
        self.updated_at = Utc::now();
        self.messages.push(message);
    }

    pub fn append_artifact(&mut self, artifact: Artifact) {
        self.updated_at = Utc::now();
        self.artifacts.push(artifact);
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    /// no side effects, local read-only
    T0,
    /// local side effects allowed
    T1,
    /// external side effects, approval required
    T2,
    /// destructive or financial side effects, explicit broker
    T3,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Agent,
    System,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub message_id: String,
    pub role: Role,
    pub parts: Vec<Part>,
    pub created_at: DateTime<Utc>,
}

impl Message {
    pub fn new(role: Role, parts: Vec<Part>) -> Self {
        Self {
            message_id: Uuid::new_v4().to_string(),
            role,
            parts,
            created_at: Utc::now(),
        }
    }

    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self::new(role, vec![Part::text(text)])
    }
}

/// Wire-compatible heterogeneous payload part. Uses internal tagging so a
/// runtime can match on `"kind"` without inspecting nested fields.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Part {
    Text {
        text: String,
    },
    Json {
        json: serde_json::Value,
    },
    FileRef {
        uri: String,
        media_type: Option<String>,
    },
    DataRef {
        source: String,
        key: String,
    },
}

impl Part {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn json(value: serde_json::Value) -> Self {
        Self::Json { json: value }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifact {
    pub artifact_id: String,
    pub name: String,
    pub parts: Vec<Part>,
    pub created_at: DateTime<Utc>,
}

impl Artifact {
    pub fn new(name: impl Into<String>, parts: Vec<Part>) -> Self {
        Self {
            artifact_id: Uuid::new_v4().to_string(),
            name: name.into(),
            parts,
            created_at: Utc::now(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatusEvent {
    pub task_id: String,
    pub state: TaskState,
    pub at: DateTime<Utc>,
    pub note: Option<String>,
}

impl StatusEvent {
    pub fn new(task: &Task, note: Option<String>) -> Self {
        Self {
            task_id: task.task_id.clone(),
            state: task.state,
            at: Utc::now(),
            note,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_task_starts_t0_no_approval() {
        let task = Task::new(WorkerClass::ShellMachine);
        assert_eq!(task.state, TaskState::Submitted);
        assert_eq!(task.risk_tier, RiskTier::T0);
        assert!(!task.approval_required);
    }

    #[test]
    fn browser_task_requires_approval_and_is_t2() {
        let task = Task::new(WorkerClass::BrowserAgent);
        assert_eq!(task.risk_tier, RiskTier::T2);
        assert!(task.approval_required);
    }

    #[test]
    fn approval_broker_is_t3() {
        let task = Task::new(WorkerClass::ApprovalBroker);
        assert_eq!(task.risk_tier, RiskTier::T3);
        assert!(task.approval_required);
    }

    #[test]
    fn task_round_trips_json() {
        let mut task = Task::new(WorkerClass::LocalLongrun);
        task.append_message(Message::text(Role::Agent, "started"));
        task.append_artifact(Artifact::new(
            "scan",
            vec![Part::json(serde_json::json!({"hits": 0}))],
        ));
        task.transition(TaskState::Working);
        let raw = serde_json::to_string(&task).expect("serialize");
        let decoded: Task = serde_json::from_str(&raw).expect("deserialize");
        assert_eq!(decoded.state, TaskState::Working);
        assert_eq!(decoded.messages.len(), 1);
        assert_eq!(decoded.artifacts.len(), 1);
    }

    #[test]
    fn part_text_serializes_with_kind_tag() {
        let part = Part::text("hello");
        let raw = serde_json::to_string(&part).expect("serialize part");
        assert!(raw.contains("\"kind\":\"text\""), "missing kind tag: {raw}");
        assert!(
            raw.contains("\"text\":\"hello\""),
            "missing text body: {raw}"
        );
        let decoded: Part = serde_json::from_str(&raw).expect("deserialize part");
        match decoded {
            Part::Text { text } => assert_eq!(text, "hello"),
            other => panic!("expected text part, got {other:?}"),
        }
    }
}
