use std::{collections::HashMap, sync::Arc};

use axum::extract::ws::Message;
use serde_json::Value;
use tokio::sync::{RwLock, mpsc};

use crate::config::RuntimeConfig;
use crate::stdb::{
    PersistedDispatchAck, PersistedWorkerLease, PersistedWorkerSession, ReducerTransport,
    StdbRuntime, StdbTransport,
};
use heiwa_bindings::ModelTier;

pub struct CoreState<T: StdbTransport = ReducerTransport> {
    pub config: RuntimeConfig,
    pub model_tiers: RwLock<Vec<ModelTier>>,
    pub status: RwLock<SystemStatus>,
    pub seeded: RwLock<bool>,
    pub stdb: StdbRuntime<T>,
    pub worker_registry: RwLock<WorkerRegistry>,
    pub worker_senders: RwLock<HashMap<String, mpsc::UnboundedSender<Message>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemStatus {
    Starting,
    Ready,
    Degraded,
}

impl SystemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SystemStatus::Starting => "starting",
            SystemStatus::Ready => "ok",
            SystemStatus::Degraded => "degraded",
        }
    }
}

impl<T: StdbTransport> CoreState<T> {
    pub fn new(config: RuntimeConfig, stdb: StdbRuntime<T>) -> Self {
        Self {
            config,
            model_tiers: RwLock::new(Vec::new()),
            status: RwLock::new(SystemStatus::Starting),
            seeded: RwLock::new(false),
            stdb,
            worker_registry: RwLock::new(WorkerRegistry::default()),
            worker_senders: RwLock::new(HashMap::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerProtocolFlavor {
    V1,
    Legacy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkerSessionRecord {
    pub session_id: String,
    pub node_id: String,
    pub instance_id: String,
    pub runtime: String,
    pub runtime_version: String,
    pub worker_version: String,
    pub protocol: WorkerProtocolFlavor,
    pub capabilities: Vec<String>,
    pub metadata: Value,
    pub max_concurrency: i64,
    pub active_tasks: u32,
    pub status: String,
    pub load: f64,
    pub created_at_ms: u64,
    pub session_expires_at_ms: u64,
    pub last_seen_at_ms: u64,
    pub current_task_id: Option<String>,
    pub lease_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkerSessionRegistration {
    pub session_id: String,
    pub node_id: String,
    pub instance_id: String,
    pub runtime: String,
    pub runtime_version: String,
    pub worker_version: String,
    pub protocol: WorkerProtocolFlavor,
    pub capabilities: Vec<String>,
    pub metadata: Value,
    pub max_concurrency: i64,
    pub session_expires_at_ms: u64,
    pub last_seen_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkerLeaseRecord {
    pub lease_id: String,
    pub task_id: String,
    pub session_id: String,
    pub node_id: String,
    pub capability: String,
    pub status: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub accepted: Option<bool>,
    pub acked_at_ms: Option<u64>,
    pub completed_at_ms: Option<u64>,
    pub failure_code: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryErrorCode {
    SessionExpired,
    LeaseExpired,
    LeaseNotFound,
    TaskNotFound,
    CapabilityMismatch,
    DispatchRejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryError {
    pub code: RegistryErrorCode,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct WorkerRegistry {
    sessions: HashMap<String, WorkerSessionRecord>,
    task_leases: HashMap<String, WorkerLeaseRecord>,
    task_index: HashMap<String, String>,
}

fn iso_from_ms(value: u64) -> String {
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};

    let offset = OffsetDateTime::from_unix_timestamp_nanos((value as i128) * 1_000_000)
        .expect("valid unix timestamp");
    offset
        .format(&Rfc3339)
        .expect("timestamp formatting should succeed")
}

impl WorkerSessionRecord {
    fn to_persisted(&self) -> PersistedWorkerSession {
        let persisted_status = if self.status == "closed" {
            "closed".to_string()
        } else if self.session_expires_at_ms <= self.last_seen_at_ms {
            "expired".to_string()
        } else {
            "active".to_string()
        };
        PersistedWorkerSession {
            session_id: self.session_id.clone(),
            node_id: self.node_id.clone(),
            instance_id: self.instance_id.clone(),
            runtime: self.runtime.clone(),
            runtime_version: self.runtime_version.clone(),
            worker_version: self.worker_version.clone(),
            protocol: match self.protocol {
                WorkerProtocolFlavor::V1 => "v1".to_string(),
                WorkerProtocolFlavor::Legacy => "legacy".to_string(),
            },
            capabilities_json: serde_json::to_string(&self.capabilities)
                .expect("worker capabilities serialize"),
            metadata_json: self.metadata.to_string(),
            max_concurrency: self.max_concurrency,
            active_tasks: self.active_tasks,
            status: persisted_status,
            load: self.load,
            created_at: iso_from_ms(self.created_at_ms),
            updated_at: iso_from_ms(self.last_seen_at_ms),
            expires_at: iso_from_ms(self.session_expires_at_ms),
            last_seen_at: iso_from_ms(self.last_seen_at_ms),
            closed_at: if self.status == "closed" {
                Some(iso_from_ms(self.last_seen_at_ms))
            } else {
                None
            },
            current_task_id: self.current_task_id.clone(),
            lease_id: self.lease_id.clone(),
        }
    }
}

impl WorkerLeaseRecord {
    fn to_persisted(&self) -> PersistedWorkerLease {
        PersistedWorkerLease {
            lease_id: self.lease_id.clone(),
            task_id: self.task_id.clone(),
            session_id: self.session_id.clone(),
            node_id: self.node_id.clone(),
            capability: self.capability.clone(),
            status: self.status.clone(),
            issued_at: iso_from_ms(self.issued_at_ms),
            updated_at: self
                .completed_at_ms
                .or(self.acked_at_ms)
                .map(iso_from_ms)
                .unwrap_or_else(|| iso_from_ms(self.issued_at_ms)),
            expires_at: iso_from_ms(self.expires_at_ms),
            acked_at: self.acked_at_ms.map(iso_from_ms),
            completed_at: self.completed_at_ms.map(iso_from_ms),
            failure_code: self.failure_code.clone(),
            reason: self.reason.clone(),
        }
    }
}

impl WorkerRegistry {
    pub fn register_session<T: StdbTransport>(
        &mut self,
        stdb: &StdbRuntime<T>,
        registration: WorkerSessionRegistration,
    ) -> WorkerSessionRecord {
        let session = WorkerSessionRecord {
            session_id: registration.session_id.clone(),
            node_id: registration.node_id.clone(),
            instance_id: registration.instance_id,
            runtime: registration.runtime,
            runtime_version: registration.runtime_version,
            worker_version: registration.worker_version,
            protocol: registration.protocol,
            capabilities: registration.capabilities,
            metadata: registration.metadata.clone(),
            max_concurrency: registration.max_concurrency.max(1),
            active_tasks: 0,
            status: "idle".to_string(),
            load: 0.0,
            created_at_ms: registration.last_seen_at_ms,
            session_expires_at_ms: registration.session_expires_at_ms,
            last_seen_at_ms: registration.last_seen_at_ms,
            current_task_id: None,
            lease_id: None,
        };
        self.sessions
            .insert(session.session_id.clone(), session.clone());

        let _ = stdb.transport.upsert_worker_session(session.to_persisted());

        session
    }

    pub fn update_heartbeat<T: StdbTransport>(
        &mut self,
        stdb: &StdbRuntime<T>,
        session_id: &str,
        now_ms: u64,
        status: String,
        active_tasks: u32,
        load: f64,
        capabilities: Option<Vec<String>>,
    ) -> Result<WorkerSessionRecord, RegistryError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| RegistryError {
                code: RegistryErrorCode::TaskNotFound,
                message: format!("worker session not found: {session_id}"),
            })?;
        if session.session_expires_at_ms <= now_ms {
            return Err(RegistryError {
                code: RegistryErrorCode::SessionExpired,
                message: format!("worker session expired: {session_id}"),
            });
        }
        session.last_seen_at_ms = now_ms;
        session.status = status;
        session.active_tasks = active_tasks;
        session.load = load;
        if let Some(capabilities) = capabilities {
            session.capabilities = capabilities;
        }
        let persisted = session.clone();
        let _ = stdb.transport.upsert_worker_session(persisted.to_persisted());
        Ok(persisted)
    }

    pub fn reserve_dispatch<T: StdbTransport>(
        &mut self,
        stdb: &StdbRuntime<T>,
        capability: &str,
        task_id: String,
        lease_id: String,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Option<(WorkerSessionRecord, WorkerLeaseRecord)> {
        let selected_session_id = self
            .sessions
            .iter()
            .find_map(|(session_id, session)| {
                let capacity_ok = session.active_tasks < session.max_concurrency as u32;
                let capability_ok = session.capabilities.iter().any(|item| item == capability);
                if capacity_ok && capability_ok && session.session_expires_at_ms > issued_at_ms {
                    Some(session_id.clone())
                } else {
                    None
                }
            })?;

        let session = self.sessions.get_mut(&selected_session_id)?;
        session.active_tasks += 1;
        session.status = "busy".to_string();
        session.last_seen_at_ms = issued_at_ms;
        session.current_task_id = Some(task_id.clone());
        session.lease_id = Some(lease_id.clone());

        let lease = WorkerLeaseRecord {
            lease_id: lease_id.clone(),
            task_id: task_id.clone(),
            session_id: session.session_id.clone(),
            node_id: session.node_id.clone(),
            capability: capability.to_string(),
            status: "issued".to_string(),
            issued_at_ms,
            expires_at_ms,
            accepted: None,
            acked_at_ms: None,
            completed_at_ms: None,
            failure_code: None,
            reason: None,
        };

        self.task_index.insert(task_id, lease_id.clone());
        self.task_leases.insert(lease_id, lease.clone());
        let _ = stdb.transport.upsert_worker_lease(lease.to_persisted());
        let _ = stdb.transport.upsert_worker_session(session.clone().to_persisted());
        Some((session.clone(), lease))
    }

    pub fn resolve_lease_for_task(&self, task_id: &str) -> Option<WorkerLeaseRecord> {
        let lease_id = self.task_index.get(task_id)?;
        self.task_leases.get(lease_id).cloned()
    }

    pub fn record_dispatch_ack<T: StdbTransport>(
        &mut self,
        stdb: &StdbRuntime<T>,
        session_id: &str,
        task_id: &str,
        lease_id: &str,
        accepted: bool,
        detail: Option<String>,
        now_ms: u64,
    ) -> Result<WorkerLeaseRecord, RegistryError> {
        let _lease = self.validate_lease(session_id, task_id, lease_id, now_ms)?;
        let stored = self
            .task_leases
            .get_mut(lease_id)
            .expect("validated lease should exist");
        stored.accepted = Some(accepted);
        stored.reason = detail.clone();
        let ack = PersistedDispatchAck {
            ack_id: format!("ack:{lease_id}:{now_ms}"),
            lease_id: stored.lease_id.clone(),
            session_id: stored.session_id.clone(),
            task_id: stored.task_id.clone(),
            node_id: stored.node_id.clone(),
            status: if accepted {
                "accepted".to_string()
            } else {
                "rejected".to_string()
            },
            decided_at: iso_from_ms(now_ms),
            detail: detail.clone(),
        };
        if !accepted {
            stored.status = "failed".to_string();
            stored.completed_at_ms = Some(now_ms);
            stored.failure_code = Some("DISPATCH_REJECTED".to_string());
            let rejected = stored.clone();
            let _ = stdb.transport.upsert_worker_lease(rejected.to_persisted());
            let _ = stdb.transport.record_dispatch_ack(ack);
            if let Some(session) = self.sessions.get_mut(session_id) {
                session.active_tasks = session.active_tasks.saturating_sub(1);
                session.status = "idle".to_string();
                session.load = 0.0;
                session.last_seen_at_ms = now_ms;
                session.current_task_id = None;
                session.lease_id = None;
                let _ = stdb.transport.upsert_worker_session(session.clone().to_persisted());
            }
            self.task_index.remove(task_id);
            self.task_leases.remove(lease_id);
            return Err(RegistryError {
                code: RegistryErrorCode::DispatchRejected,
                message: format!("dispatch rejected for task {task_id}"),
            });
        }
        stored.status = "acked".to_string();
        stored.acked_at_ms = Some(now_ms);
        let accepted_record = stored.clone();
        let _ = stdb.transport.upsert_worker_lease(accepted_record.to_persisted());
        let _ = stdb.transport.record_dispatch_ack(ack);
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.last_seen_at_ms = now_ms;
            let _ = stdb.transport.upsert_worker_session(session.clone().to_persisted());
        }
        Ok(accepted_record)
    }

    pub fn validate_lease(
        &self,
        session_id: &str,
        task_id: &str,
        lease_id: &str,
        now_ms: u64,
    ) -> Result<WorkerLeaseRecord, RegistryError> {
        let lease = self
            .task_leases
            .get(lease_id)
            .ok_or_else(|| RegistryError {
                code: RegistryErrorCode::LeaseNotFound,
                message: format!("lease not found: {lease_id}"),
            })?;
        if lease.session_id != session_id || lease.task_id != task_id {
            return Err(RegistryError {
                code: RegistryErrorCode::CapabilityMismatch,
                message: format!("lease does not match session/task for task {task_id}"),
            });
        }
        if lease.expires_at_ms <= now_ms {
            return Err(RegistryError {
                code: RegistryErrorCode::LeaseExpired,
                message: format!("lease expired for task {task_id}"),
            });
        }
        Ok(lease.clone())
    }

    pub fn complete_dispatch<T: StdbTransport>(
        &mut self,
        stdb: &StdbRuntime<T>,
        lease_id: &str,
        status: &str,
        failure_code: Option<String>,
        reason: Option<String>,
        now_ms: u64,
    ) -> Option<WorkerLeaseRecord> {
        let mut lease = self.task_leases.remove(lease_id)?;
        self.task_index.remove(&lease.task_id);
        lease.status = status.to_string();
        lease.completed_at_ms = Some(now_ms);
        lease.failure_code = failure_code;
        lease.reason = reason;
        let _ = stdb.transport.upsert_worker_lease(lease.clone().to_persisted());
        if let Some(session) = self.sessions.get_mut(&lease.session_id) {
            session.active_tasks = session.active_tasks.saturating_sub(1);
            session.last_seen_at_ms = now_ms;
            session.current_task_id = None;
            session.lease_id = None;
            if session.active_tasks == 0 {
                session.status = "idle".to_string();
                session.load = 0.0;
            }
            let _ = stdb.transport.upsert_worker_session(session.clone().to_persisted());
        }
        Some(lease)
    }

    pub fn remove_session<T: StdbTransport>(&mut self, stdb: &StdbRuntime<T>, session_id: &str) {
        let mut closed_at_ms = 0;
        if let Some(session) = self.sessions.remove(session_id) {
            let mut closed = session;
            closed_at_ms = closed.last_seen_at_ms;
            closed.status = "closed".to_string();
            closed.active_tasks = 0;
            closed.load = 0.0;
            let _ = stdb.transport.upsert_worker_session(closed.to_persisted());
            let _ = stdb.transport.close_session(session_id.to_string());
        }

        let orphaned: Vec<String> = self
            .task_leases
            .values()
            .filter(|lease| lease.session_id == session_id)
            .map(|lease| lease.lease_id.clone())
            .collect();
        for lease_id in orphaned {
            let _ = self.complete_dispatch(
                stdb,
                &lease_id,
                "revoked",
                Some("SESSION_CLOSED".to_string()),
                Some("worker session closed".to_string()),
                closed_at_ms,
            );
        }
    }

    pub fn session(&self, session_id: &str) -> Option<WorkerSessionRecord> {
        self.sessions.get(session_id).cloned()
    }
}

pub type SharedState = Arc<CoreState<ReducerTransport>>;
