use std::{collections::HashMap, sync::Arc};

use axum::extract::ws::Message;
use serde_json::Value;
use tokio::sync::{RwLock, mpsc};

use crate::config::RuntimeConfig;
use crate::stdb::{StdbRuntime, ReducerTransport};
use heiwa_bindings::ModelTier;

pub struct CoreState {
    pub config: RuntimeConfig,
    pub model_tiers: RwLock<Vec<ModelTier>>,
    pub status: RwLock<SystemStatus>,
    pub seeded: RwLock<bool>,
    pub stdb: StdbRuntime<ReducerTransport>,
    pub worker_registry: RwLock<WorkerRegistry>,
    pub worker_senders: RwLock<HashMap<String, mpsc::UnboundedSender<Message>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemStatus {
    Starting,
    Ready,
    Degraded,
}

impl CoreState {
    pub fn new(config: RuntimeConfig, stdb: StdbRuntime<ReducerTransport>) -> Self {
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
    pub session_expires_at_ms: u64,
    pub last_seen_at_ms: u64,
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
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub accepted: Option<bool>,
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

impl WorkerRegistry {
    pub fn register_session(
        &mut self,
        registration: WorkerSessionRegistration,
    ) -> WorkerSessionRecord {
        let session = WorkerSessionRecord {
            session_id: registration.session_id.clone(),
            node_id: registration.node_id,
            instance_id: registration.instance_id,
            runtime: registration.runtime,
            runtime_version: registration.runtime_version,
            worker_version: registration.worker_version,
            protocol: registration.protocol,
            capabilities: registration.capabilities,
            metadata: registration.metadata,
            max_concurrency: registration.max_concurrency.max(1),
            active_tasks: 0,
            status: "idle".to_string(),
            load: 0.0,
            session_expires_at_ms: registration.session_expires_at_ms,
            last_seen_at_ms: registration.last_seen_at_ms,
        };
        self.sessions
            .insert(session.session_id.clone(), session.clone());
        session
    }

    pub fn update_heartbeat(
        &mut self,
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
        Ok(session.clone())
    }

    pub fn reserve_dispatch(
        &mut self,
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

        let lease = WorkerLeaseRecord {
            lease_id: lease_id.clone(),
            task_id: task_id.clone(),
            session_id: session.session_id.clone(),
            node_id: session.node_id.clone(),
            capability: capability.to_string(),
            issued_at_ms,
            expires_at_ms,
            accepted: None,
        };

        self.task_index.insert(task_id, lease_id.clone());
        self.task_leases.insert(lease_id, lease.clone());
        Some((session.clone(), lease))
    }

    pub fn resolve_lease_for_task(&self, task_id: &str) -> Option<WorkerLeaseRecord> {
        let lease_id = self.task_index.get(task_id)?;
        self.task_leases.get(lease_id).cloned()
    }

    pub fn record_dispatch_ack(
        &mut self,
        session_id: &str,
        task_id: &str,
        lease_id: &str,
        accepted: bool,
        now_ms: u64,
    ) -> Result<WorkerLeaseRecord, RegistryError> {
        let _lease = self.validate_lease(session_id, task_id, lease_id, now_ms)?;
        let stored = self
            .task_leases
            .get_mut(lease_id)
            .expect("validated lease should exist");
        stored.accepted = Some(accepted);
        if !accepted {
            self.complete_dispatch(lease_id);
            return Err(RegistryError {
                code: RegistryErrorCode::DispatchRejected,
                message: format!("dispatch rejected for task {task_id}"),
            });
        }
        Ok(stored.clone())
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

    pub fn complete_dispatch(&mut self, lease_id: &str) -> Option<WorkerLeaseRecord> {
        let lease = self.task_leases.remove(lease_id)?;
        self.task_index.remove(&lease.task_id);
        if let Some(session) = self.sessions.get_mut(&lease.session_id) {
            session.active_tasks = session.active_tasks.saturating_sub(1);
            if session.active_tasks == 0 {
                session.status = "idle".to_string();
                session.load = 0.0;
            }
        }
        Some(lease)
    }

    pub fn remove_session(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
        let orphaned: Vec<String> = self
            .task_leases
            .values()
            .filter(|lease| lease.session_id == session_id)
            .map(|lease| lease.lease_id.clone())
            .collect();
        for lease_id in orphaned {
            self.complete_dispatch(&lease_id);
        }
    }

    pub fn session(&self, session_id: &str) -> Option<WorkerSessionRecord> {
        self.sessions.get(session_id).cloned()
    }
}

pub type SharedState = Arc<CoreState>;
