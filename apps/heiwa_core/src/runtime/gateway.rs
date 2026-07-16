use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{
    auth::verify_jwt,
    drex::{default_policy, plan_route, DrexIngress},
    runtime::state::{
        RegistryErrorCode, SharedState, WorkerProtocolFlavor, WorkerSessionRegistration,
    },
    evidence::{EvidenceTransport, PersistedArtifact, PersistedRunReceipt},
};
const WORKER_PROTOCOL_VERSION: &str = "v1";
const HEARTBEAT_INTERVAL_MS: u64 = 30_000;
const WORKER_SESSION_TTL_MS: u64 = 6 * 60 * 60 * 1000;
const DISPATCH_LEASE_TTL_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "auth")]
    Auth { token: String },
    #[serde(rename = "action")]
    Action {
        action: String,
        request_id: String,
        payload: Value,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "auth_ok")]
    AuthOk,
    #[serde(rename = "result")]
    Result {
        request_id: String,
        status: String,
        payload: Value,
    },
    #[serde(rename = "error")]
    Error {
        request_id: Option<String>,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkerEnvelopeType {
    Register,
    AuthOk,
    Heartbeat,
    Dispatch,
    DispatchAck,
    Result,
    Error,
    TaskCancel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerEnvelope {
    pub version: String,
    #[serde(rename = "type")]
    pub kind: WorkerEnvelopeType,
    pub timestamp: String,
    pub node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegisterPayload {
    pub instance_id: String,
    pub runtime: String,
    pub runtime_version: String,
    pub worker_version: String,
    pub capabilities: Vec<String>,
    pub max_concurrency: i64,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub host_role: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub quantization: Value,
    #[serde(default)]
    pub vram_mb: Option<u64>,
    #[serde(default)]
    pub embedding_capable: Option<bool>,
    #[serde(default)]
    pub media_capable: Option<bool>,
    #[serde(default)]
    pub filesystem_capable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthOkPayload {
    pub heartbeat_interval_ms: u64,
    pub session_expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeartbeatPayload {
    pub status: String,
    pub active_tasks: u32,
    pub load: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DispatchPolicy {
    pub side_effects: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DispatchPayload {
    pub task_id: String,
    pub lease_id: String,
    pub capability: String,
    pub expires_at: String,
    pub input: Value,
    pub policy: DispatchPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DispatchAckPayload {
    pub task_id: String,
    pub lease_id: String,
    pub accepted: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactDescriptor {
    pub artifact_id: String,
    #[serde(rename = "type")]
    pub artifact_type: String,
    pub hash: String,
    pub size_bytes: u64,
    pub location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResultMetrics {
    pub duration_ms: u64,
    #[serde(default)]
    pub tokens_in: u32,
    #[serde(default)]
    pub tokens_out: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResultPayload {
    pub task_id: String,
    pub lease_id: String,
    pub status: String,
    pub artifacts: Vec<ArtifactDescriptor>,
    pub metrics: ResultMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorPayload {
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub lease_id: Option<String>,
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum LegacyWorkerMessage {
    #[serde(rename = "register")]
    Register {
        worker_id: Option<String>,
        node_id: Option<String>,
        auth_token: String,
        capabilities: Value,
    },
    #[serde(rename = "heartbeat")]
    Heartbeat {
        capabilities: Option<Value>,
        status: Option<String>,
    },
    #[serde(rename = "result")]
    Result { data: LegacyResultPayload },
    #[serde(rename = "llm_response")]
    LlmResponse { request_id: String, text: String },
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyResultPayload {
    task_id: String,
    status: String,
    summary: String,
    #[serde(default)]
    duration_ms: u64,
    #[serde(default)]
    _target_tool: Option<String>,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

pub async fn ws_client_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_client_socket(socket, state))
}

pub async fn ws_worker_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if !worker_token_matches(extract_worker_token(&headers, &params), &state) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "version": WORKER_PROTOCOL_VERSION,
                "type": "ERROR",
                "payload": {
                    "code": "AUTH_FAILED",
                    "message": "missing or invalid worker token",
                    "retryable": false
                }
            })),
        )
            .into_response();
    }
    ws.on_upgrade(move |socket| handle_worker_socket(socket, state))
        .into_response()
}

pub async fn ws_worker_legacy_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_legacy_worker_socket(socket, state))
}

pub async fn battlefield_handler(
    State(state): State<SharedState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    info!("Battlefield request: {:?}", payload);
    let battlefield_id = format!("bf-{}", uuid::Uuid::new_v4());
    let name = payload["name"].as_str().unwrap_or("unnamed").to_string();

    let _ = state.evidence.transport.journal(
        "battlefields",
        json!({
            "battlefield_id": battlefield_id,
            "name": name,
            "status": "active",
            "created_at": now_iso(),
            "updated_at": now_iso(),
        }),
    );

    Json(json!({ "status": "ok", "battlefield_id": battlefield_id }))
}

pub async fn task_handler(
    State(state): State<SharedState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    info!("Task dispatch request: {:?}", payload);
    let capability = payload
        .get("capability")
        .and_then(Value::as_str)
        .unwrap_or("llm")
        .to_string();
    let task_id = format!("task-{}", uuid::Uuid::new_v4());
    let lease_id = format!("lease-{}", uuid::Uuid::new_v4());
    let now_ms = now_ms();
    let issued_at = now_iso();
    let expires_at_ms = now_ms + DISPATCH_LEASE_TTL_MS;
    let expires_at = iso_from_ms(expires_at_ms);
    let policy = DispatchPolicy {
        side_effects: payload
            .get("policy")
            .and_then(|policy| policy.get("side_effects"))
            .and_then(Value::as_str)
            .unwrap_or("deny")
            .to_string(),
        timeout_ms: payload
            .get("policy")
            .and_then(|policy| policy.get("timeout_ms"))
            .and_then(Value::as_u64)
            .unwrap_or(300_000),
    };
    let selected = {
        let mut registry = state.worker_registry.write().await;
        registry.reserve_dispatch(
            &state.evidence,
            &capability,
            task_id.clone(),
            lease_id.clone(),
            now_ms,
            expires_at_ms,
        )
    };

    let Some((session, lease)) = selected else {
        let _ = state.evidence.transport.journal(
            "task_dispatches",
            json!({
                "task_id": task_id,
                "capability": capability,
                "priority": "medium",
                "queue": "normal",
                "assigned_node": "unassigned",
                "sandbox_mode": "trusted",
            }),
        );
        return Json(json!({
            "status": "queued",
            "task_id": task_id,
            "reason": "no compatible worker session available"
        }));
    };

    let _ = state.evidence.transport.journal(
        "task_dispatches",
        json!({
            "task_id": task_id,
            "capability": capability,
            "priority": "medium",
            "queue": "normal",
            "assigned_node": session.node_id,
            "sandbox_mode": if policy.side_effects == "allow" {
                "trusted"
            } else {
                "sandbox"
            },
        }),
    );
    let _ = state.evidence.transport.journal(
        "capability_leases",
        json!({
            "lease_id": lease_id,
            "task_id": task_id,
            "holder_type": "worker_session",
            "holder_id": session.session_id,
            "capabilities": [capability],
            "policy": "fail_closed",
            "scope": "mesh",
            "status": "ACTIVE",
            "issued_at": issued_at,
            "expires_at": expires_at,
            "issuer": "heiwa-core",
            "node_id": session.node_id,
        }),
    );

    let outbound = if session.protocol == WorkerProtocolFlavor::Legacy {
        legacy_task_assignment_message(&lease, payload.clone(), &policy)
    } else {
        worker_message(
            &session.node_id,
            Some(session.session_id.as_str()),
            WorkerEnvelopeType::Dispatch,
            &DispatchPayload {
                task_id: lease.task_id.clone(),
                lease_id: lease.lease_id.clone(),
                capability: lease.capability.clone(),
                expires_at,
                input: payload.clone(),
                policy,
            },
        )
    };

    if let Some(sender) = state
        .worker_senders
        .read()
        .await
        .get(&session.session_id)
        .cloned()
    {
        if sender.send(outbound).is_err() {
            let mut registry = state.worker_registry.write().await;
            registry.complete_dispatch(
                &state.evidence,
                &lease.lease_id,
                "failed",
                Some("DISPATCH_UNAVAILABLE".to_string()),
                Some("worker sender unavailable".to_string()),
                now_ms,
            );
            return Json(json!({
                "status": "queued",
                "task_id": task_id,
                "reason": "worker sender unavailable"
            }));
        }
    }

    Json(json!({
        "status": "dispatched",
        "task_id": task_id,
        "lease_id": lease_id,
        "node_id": session.node_id,
        "session_id": session.session_id,
        "capability": capability,
    }))
}

async fn handle_socket(socket: WebSocket, state: SharedState) {
    let (mut sender, mut receiver) = socket.split();
    info!("New unified WS connection");

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Auth { token }) => {
                    let authenticated = if token == state.config.machine_auth_token {
                        true
                    } else if !state.config.jwt_signing_secret.is_empty() {
                        verify_jwt(&token, &state.config.jwt_signing_secret).is_ok()
                    } else {
                        false
                    };

                    if authenticated {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::to_string(&ServerMessage::AuthOk).unwrap(),
                            ))
                            .await;
                    } else {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::to_string(&ServerMessage::Error {
                                    request_id: None,
                                    message: "Invalid token".to_string(),
                                })
                                .unwrap(),
                            ))
                            .await;
                    }
                }
                Ok(ClientMessage::Action {
                    action,
                    request_id,
                    payload,
                }) => {
                    if action == "route.preview" {
                        match handle_route_preview(&state, request_id.clone(), payload).await {
                            Ok(result) => {
                                let _ = sender
                                    .send(Message::Text(
                                        serde_json::to_string(&ServerMessage::Result {
                                            request_id,
                                            status: "success".to_string(),
                                            payload: result,
                                        })
                                        .unwrap(),
                                    ))
                                    .await;
                            }
                            Err(error) => {
                                let _ = sender
                                    .send(Message::Text(
                                        serde_json::to_string(&ServerMessage::Error {
                                            request_id: Some(request_id),
                                            message: error.to_string(),
                                        })
                                        .unwrap(),
                                    ))
                                    .await;
                            }
                        }
                    } else {
                        let _ = sender
                            .send(Message::Text(
                                serde_json::to_string(&ServerMessage::Error {
                                    request_id: Some(request_id),
                                    message: format!("Unknown action: {action}"),
                                })
                                .unwrap(),
                            ))
                            .await;
                    }
                }
                Err(error) => {
                    warn!("Failed to parse client message: {:?}", error);
                }
            }
        }
    }
}

async fn handle_client_socket(socket: WebSocket, state: SharedState) {
    info!("New legacy client WS connection");
    handle_socket(socket, state).await;
}

async fn handle_worker_socket(socket: WebSocket, state: SharedState) {
    let (mut ws_sender, mut receiver) = socket.split();
    let (sender, mut outbound_rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(message) = outbound_rx.recv().await {
            if ws_sender.send(message).await.is_err() {
                break;
            }
        }
    });

    info!("New canonical worker WS connection");
    let mut session_id: Option<String> = None;
    let mut node_id: Option<String> = None;

    while let Some(Ok(message)) = receiver.next().await {
        let Message::Text(text) = message else {
            continue;
        };

        let envelope = match parse_worker_envelope(&text) {
            Ok(envelope) => envelope,
            Err(error) => {
                let node = node_id.as_deref().unwrap_or("unknown");
                let _ = sender.send(worker_error(
                    node,
                    session_id.as_deref(),
                    error.code,
                    &error.message,
                    None,
                    None,
                ));
                continue;
            }
        };

        if session_id.is_none() {
            if envelope.kind != WorkerEnvelopeType::Register {
                let _ = sender.send(worker_error(
                    &envelope.node_id,
                    None,
                    "INVALID_SCHEMA",
                    "first canonical worker message must be REGISTER",
                    None,
                    None,
                ));
                continue;
            }
            if envelope.session_id.is_some() {
                let _ = sender.send(worker_error(
                    &envelope.node_id,
                    None,
                    "INVALID_SCHEMA",
                    "REGISTER must not include session_id",
                    None,
                    None,
                ));
                continue;
            }
            match serde_json::from_value::<RegisterPayload>(envelope.payload.clone()) {
                Ok(register) => {
                    let now_ms = now_ms();
                    let created_session_id = uuid::Uuid::new_v4().to_string();
                    let expires_at_ms = now_ms + WORKER_SESSION_TTL_MS;
                    let registration = WorkerSessionRegistration {
                        session_id: created_session_id.clone(),
                        node_id: envelope.node_id.clone(),
                        instance_id: register.instance_id,
                        runtime: register.runtime,
                        runtime_version: register.runtime_version,
                        worker_version: register.worker_version,
                        protocol: WorkerProtocolFlavor::V1,
                        capabilities: register.capabilities,
                        metadata: json!({
                            "platform": register.platform,
                            "host_role": register.host_role,
                            "models": register.models,
                            "quantization": register.quantization,
                            "vram_mb": register.vram_mb,
                            "embedding_capable": register.embedding_capable,
                            "media_capable": register.media_capable,
                            "filesystem_capable": register.filesystem_capable,
                        }),
                        max_concurrency: register.max_concurrency,
                        session_expires_at_ms: expires_at_ms,
                        last_seen_at_ms: now_ms,
                    };
                    let session = {
                        let mut registry = state.worker_registry.write().await;
                        registry.register_session(&state.evidence, registration)
                    };
                    state
                        .worker_senders
                        .write()
                        .await
                        .insert(session.session_id.clone(), sender.clone());
                    persist_worker_presence(&state, &session).await;
                    session_id = Some(session.session_id.clone());
                    node_id = Some(session.node_id.clone());
                    let _ = sender.send(worker_message(
                        &session.node_id,
                        Some(session.session_id.as_str()),
                        WorkerEnvelopeType::AuthOk,
                        &AuthOkPayload {
                            heartbeat_interval_ms: HEARTBEAT_INTERVAL_MS,
                            session_expires_at: iso_from_ms(expires_at_ms),
                        },
                    ));
                }
                Err(error) => {
                    let _ = sender.send(worker_error(
                        &envelope.node_id,
                        None,
                        "INVALID_SCHEMA",
                        &format!("invalid register payload: {error}"),
                        None,
                        None,
                    ));
                }
            }
            continue;
        }

        let Some(current_session_id) = session_id.clone() else {
            continue;
        };
        if envelope.session_id.as_deref() != Some(current_session_id.as_str()) {
            let _ = sender.send(worker_error(
                &envelope.node_id,
                Some(current_session_id.as_str()),
                "SESSION_EXPIRED",
                "session_id mismatch",
                None,
                None,
            ));
            continue;
        }

        match envelope.kind {
            WorkerEnvelopeType::Heartbeat => {
                match serde_json::from_value::<HeartbeatPayload>(envelope.payload.clone()) {
                    Ok(heartbeat) => {
                        let session = {
                            let mut registry = state.worker_registry.write().await;
                            registry.update_heartbeat(
                                &state.evidence,
                                &current_session_id,
                                now_ms(),
                                heartbeat.status,
                                heartbeat.active_tasks,
                                heartbeat.load,
                                None,
                            )
                        };
                        match session {
                            Ok(session) => {
                                persist_worker_presence(&state, &session).await;
                            }
                            Err(error) => {
                                let _ = sender.send(worker_error(
                                    &envelope.node_id,
                                    Some(current_session_id.as_str()),
                                    error_code(&error.code),
                                    &error.message,
                                    None,
                                    None,
                                ));
                            }
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(worker_error(
                            &envelope.node_id,
                            Some(current_session_id.as_str()),
                            "INVALID_SCHEMA",
                            &format!("invalid heartbeat payload: {error}"),
                            None,
                            None,
                        ));
                    }
                }
            }
            WorkerEnvelopeType::DispatchAck => {
                match serde_json::from_value::<DispatchAckPayload>(envelope.payload.clone()) {
                    Ok(ack) => {
                        let result = {
                            let mut registry = state.worker_registry.write().await;
                            registry.record_dispatch_ack(
                                &state.evidence,
                                &current_session_id,
                                &ack.task_id,
                                &ack.lease_id,
                                ack.accepted,
                                ack.reason.clone(),
                                now_ms(),
                            )
                        };
                        match result {
                            Ok(_) => {
                                let _ = state.evidence.transport.journal(
                                    "task_dispatch_status",
                                    json!({
                                        "task_id": ack.task_id,
                                        "status": "running",
                                        "detail": "worker accepted dispatch",
                                    }),
                                );
                            }
                            Err(error) => {
                                let _ = state.evidence.transport.journal(
                                    "task_dispatch_status",
                                    json!({
                                        "task_id": ack.task_id.clone(),
                                        "status": "failed",
                                        "detail": error.message.clone(),
                                    }),
                                );
                                let _ = state.evidence.transport.journal(
                                    "capability_lease_revocations",
                                    json!({
                                        "lease_id": ack.lease_id.clone(),
                                        "revoked_at": now_iso(),
                                        "reason": "dispatch_rejected",
                                    }),
                                );
                                let _ = sender.send(worker_error(
                                    &envelope.node_id,
                                    Some(current_session_id.as_str()),
                                    error_code(&error.code),
                                    &error.message,
                                    Some(ack.task_id.as_str()),
                                    Some(ack.lease_id.as_str()),
                                ));
                            }
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(worker_error(
                            &envelope.node_id,
                            Some(current_session_id.as_str()),
                            "INVALID_SCHEMA",
                            &format!("invalid dispatch ack payload: {error}"),
                            None,
                            None,
                        ));
                    }
                }
            }
            WorkerEnvelopeType::Result => {
                match serde_json::from_value::<ResultPayload>(envelope.payload.clone()) {
                    Ok(payload) => {
                        if let Err(error) =
                            finalize_result(&state, &current_session_id, &envelope.node_id, payload)
                                .await
                        {
                            let _ = sender.send(worker_error(
                                &envelope.node_id,
                                Some(current_session_id.as_str()),
                                error.code,
                                &error.message,
                                None,
                                None,
                            ));
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(worker_error(
                            &envelope.node_id,
                            Some(current_session_id.as_str()),
                            "INVALID_SCHEMA",
                            &format!("invalid result payload: {error}"),
                            None,
                            None,
                        ));
                    }
                }
            }
            WorkerEnvelopeType::Error => {
                match serde_json::from_value::<ErrorPayload>(envelope.payload.clone()) {
                    Ok(payload) => {
                        if let Err(error) =
                            finalize_error(&state, &current_session_id, &envelope.node_id, payload)
                                .await
                        {
                            let _ = sender.send(worker_error(
                                &envelope.node_id,
                                Some(current_session_id.as_str()),
                                error.code,
                                &error.message,
                                None,
                                None,
                            ));
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(worker_error(
                            &envelope.node_id,
                            Some(current_session_id.as_str()),
                            "INVALID_SCHEMA",
                            &format!("invalid error payload: {error}"),
                            None,
                            None,
                        ));
                    }
                }
            }
            WorkerEnvelopeType::Register
            | WorkerEnvelopeType::AuthOk
            | WorkerEnvelopeType::Dispatch
            | WorkerEnvelopeType::TaskCancel => {
                let _ = sender.send(worker_error(
                    &envelope.node_id,
                    Some(current_session_id.as_str()),
                    "INVALID_SCHEMA",
                    "message type is not valid for worker ingress",
                    None,
                    None,
                ));
            }
        }
    }

    if let Some(session_id) = session_id {
        state.worker_senders.write().await.remove(&session_id);
        state
            .worker_registry
            .write()
            .await
            .remove_session(&state.evidence, &session_id);
    }
    writer.abort();
}

async fn handle_legacy_worker_socket(socket: WebSocket, state: SharedState) {
    let (mut ws_sender, mut receiver) = socket.split();
    let (sender, mut outbound_rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(message) = outbound_rx.recv().await {
            if ws_sender.send(message).await.is_err() {
                break;
            }
        }
    });

    info!("New legacy worker WS connection");
    let mut session_id: Option<String> = None;
    let mut node_id: Option<String> = None;

    while let Some(Ok(message)) = receiver.next().await {
        let Message::Text(text) = message else {
            continue;
        };

        match serde_json::from_str::<LegacyWorkerMessage>(&text) {
            Ok(LegacyWorkerMessage::Register {
                worker_id,
                node_id: legacy_node_id,
                auth_token,
                capabilities,
            }) => {
                if auth_token != state.config.machine_auth_token {
                    let _ = sender.send(Message::Text(
                        json!({
                            "type": "error",
                            "detail": "Invalid legacy auth token"
                        })
                        .to_string(),
                    ));
                    continue;
                }
                let node = legacy_node_id
                    .or(worker_id)
                    .unwrap_or_else(|| format!("legacy-{}", uuid::Uuid::new_v4()));
                let (capabilities_vec, meta) = normalize_legacy_capabilities(&capabilities);
                let max_concurrency = meta
                    .get("max_concurrency")
                    .and_then(Value::as_i64)
                    .unwrap_or(1);
                let now_ms = now_ms();
                let created_session_id = uuid::Uuid::new_v4().to_string();
                let expires_at_ms = now_ms + WORKER_SESSION_TTL_MS;
                let session = {
                    let mut registry = state.worker_registry.write().await;
                    registry.register_session(
                        &state.evidence,
                        WorkerSessionRegistration {
                            session_id: created_session_id.clone(),
                            node_id: node.clone(),
                            instance_id: format!("legacy-{created_session_id}"),
                            runtime: "python".to_string(),
                            runtime_version: "legacy".to_string(),
                            worker_version: "legacy".to_string(),
                            protocol: WorkerProtocolFlavor::Legacy,
                            capabilities: capabilities_vec,
                            metadata: meta,
                            max_concurrency,
                            session_expires_at_ms: expires_at_ms,
                            last_seen_at_ms: now_ms,
                        },
                    )
                };
                state
                    .worker_senders
                    .write()
                    .await
                    .insert(session.session_id.clone(), sender.clone());
                persist_worker_presence(&state, &session).await;
                session_id = Some(session.session_id.clone());
                node_id = Some(session.node_id.clone());
                let _ = sender.send(Message::Text(
                    json!({
                        "type": "auth_ok",
                        "worker_id": session.node_id,
                        "session_id": session.session_id,
                    })
                    .to_string(),
                ));
            }
            Ok(LegacyWorkerMessage::Heartbeat {
                capabilities,
                status,
            }) => {
                let Some(current_session_id) = session_id.clone() else {
                    continue;
                };
                let (capabilities_vec, _) = normalize_legacy_capabilities(
                    &capabilities.unwrap_or_else(|| json!({ "capabilities": [] })),
                );
                let updated = {
                    let mut registry = state.worker_registry.write().await;
                    registry.update_heartbeat(
                        &state.evidence,
                        &current_session_id,
                        now_ms(),
                        status.unwrap_or_else(|| "idle".to_string()),
                        0,
                        0.0,
                        Some(capabilities_vec),
                    )
                };
                if let Ok(session) = updated {
                    persist_worker_presence(&state, &session).await;
                }
            }
            Ok(LegacyWorkerMessage::Result { data }) => {
                let Some(current_session_id) = session_id.clone() else {
                    continue;
                };
                let lease = {
                    let registry = state.worker_registry.read().await;
                    registry.resolve_lease_for_task(&data.task_id)
                };
                if let Some(lease) = lease {
                    let payload = ResultPayload {
                        task_id: data.task_id,
                        lease_id: lease.lease_id,
                        status: data.status,
                        artifacts: vec![ArtifactDescriptor {
                            artifact_id: format!("artifact-{}", uuid::Uuid::new_v4()),
                            artifact_type: "log".to_string(),
                            hash: format!("sha256:{}", fake_digest(&data.summary)),
                            size_bytes: data.summary.len() as u64,
                            location: format!("artifact://legacy/{}/summary", lease.task_id),
                        }],
                        metrics: ResultMetrics {
                            duration_ms: data.duration_ms,
                            tokens_in: 0,
                            tokens_out: 0,
                        },
                    };
                    let _ = finalize_result(
                        &state,
                        &current_session_id,
                        node_id.as_deref().unwrap_or("legacy"),
                        payload,
                    )
                    .await;
                }
            }
            Ok(LegacyWorkerMessage::LlmResponse { request_id, text }) => {
                warn!("Ignoring legacy llm_response {request_id}: {}", text.len());
            }
            Err(error) => {
                warn!("Failed to parse legacy worker message: {:?}", error);
                let _ = sender.send(Message::Text(
                    json!({
                        "type": "error",
                        "detail": format!("Invalid legacy worker message: {error}")
                    })
                    .to_string(),
                ));
            }
        }
    }

    if let Some(session_id) = session_id {
        state.worker_senders.write().await.remove(&session_id);
        state
            .worker_registry
            .write()
            .await
            .remove_session(&state.evidence, &session_id);
    }
    writer.abort();
}

async fn handle_route_preview(
    state: &SharedState,
    request_id: String,
    payload: Value,
) -> anyhow::Result<Value> {
    let ingress: DrexIngress = serde_json::from_value(payload)?;
    let model_tiers = state.model_tiers.read().await;
    let plan = plan_route(&ingress, &model_tiers, &default_policy())?;

    let task_id = format!("task-preview-{}", uuid::Uuid::new_v4());
    let _ = state
        .evidence
        .record_drex_decision(&request_id, &task_id, &plan)
        .await?;

    Ok(json!({
        "target_tier": format!("{:?}", plan.decision.active_tier),
        "runtime_hint": plan.runtime_hint,
        "selected_model": plan.selected_model.map(|model| model.model_id),
        "requires_approval": plan.decision.gate.requires_approval,
        "authority_required": plan.decision.gate.authority_required,
        "task_id": task_id,
    }))
}

pub fn parse_worker_envelope(text: &str) -> Result<WorkerEnvelope, ProtocolError> {
    let envelope: WorkerEnvelope = serde_json::from_str(text).map_err(|error| ProtocolError {
        code: "INVALID_SCHEMA",
        message: format!("invalid worker envelope: {error}"),
    })?;
    if envelope.version != WORKER_PROTOCOL_VERSION {
        return Err(ProtocolError {
            code: "VERSION_MISMATCH",
            message: format!("unsupported worker protocol version: {}", envelope.version),
        });
    }
    if envelope.node_id.trim().is_empty() {
        return Err(ProtocolError {
            code: "INVALID_SCHEMA",
            message: "node_id is required".to_string(),
        });
    }
    Ok(envelope)
}

fn worker_message<T: Serialize>(
    node_id: &str,
    session_id: Option<&str>,
    kind: WorkerEnvelopeType,
    payload: &T,
) -> Message {
    Message::Text(
        serde_json::to_string(&WorkerEnvelope {
            version: WORKER_PROTOCOL_VERSION.to_string(),
            kind,
            timestamp: now_iso(),
            node_id: node_id.to_string(),
            session_id: session_id.map(ToString::to_string),
            payload: serde_json::to_value(payload).unwrap_or_else(|_| json!({})),
        })
        .expect("worker envelope should serialize"),
    )
}

fn worker_error(
    node_id: &str,
    session_id: Option<&str>,
    code: &str,
    message: &str,
    task_id: Option<&str>,
    lease_id: Option<&str>,
) -> Message {
    worker_message(
        node_id,
        session_id,
        WorkerEnvelopeType::Error,
        &ErrorPayload {
            task_id: task_id.map(ToString::to_string),
            lease_id: lease_id.map(ToString::to_string),
            code: code.to_string(),
            message: message.to_string(),
            retryable: matches!(
                code,
                "TIMEOUT" | "SESSION_EXPIRED" | "LEASE_EXPIRED" | "TASK_NOT_FOUND"
            ),
        },
    )
}

async fn persist_worker_presence(
    state: &SharedState,
    session: &crate::runtime::state::WorkerSessionRecord,
) {
    let meta = json!({
        "session_id": session.session_id,
        "instance_id": session.instance_id,
        "runtime": session.runtime,
        "runtime_version": session.runtime_version,
        "protocol": match session.protocol {
            WorkerProtocolFlavor::V1 => "v1",
            WorkerProtocolFlavor::Legacy => "legacy",
        },
        "status": session.status,
        "load": session.load,
        "session_expires_at": iso_from_ms(session.session_expires_at_ms),
        "worker_metadata": session.metadata,
    });
    let tags = json!([
        format!(
            "protocol:{}",
            match session.protocol {
                WorkerProtocolFlavor::V1 => "v1",
                WorkerProtocolFlavor::Legacy => "legacy",
            }
        ),
        format!("runtime:{}", session.runtime),
    ]);
    let _ = state.evidence.transport.journal(
        "node_heartbeats",
        json!({
            "node_id": session.node_id.clone(),
            "seen_at": now_iso(),
            "meta": meta,
            "capabilities": session.capabilities.clone(),
            "worker_version": session.worker_version.clone(),
            "tags": tags,
            "max_concurrency": session.max_concurrency,
            "vram_mb": 0,
            "locality": "local",
            "trust_tier": 10,
        }),
    );
}

async fn finalize_result(
    state: &SharedState,
    session_id: &str,
    node_id: &str,
    payload: ResultPayload,
) -> Result<(), ProtocolError> {
    let lease = {
        let registry = state.worker_registry.read().await;
        registry
            .validate_lease(session_id, &payload.task_id, &payload.lease_id, now_ms())
            .map_err(|error| ProtocolError {
                code: error_code(&error.code),
                message: error.message,
            })?
    };
    let run_id = format!("run-{}", payload.task_id);
    let completed_at = now_iso();
    let artifact_ids: Vec<String> = payload
        .artifacts
        .iter()
        .map(|artifact| artifact.artifact_id.clone())
        .collect();
    let artifacts: Vec<PersistedArtifact> = payload
        .artifacts
        .iter()
        .map(|artifact| PersistedArtifact {
            artifact_id: artifact.artifact_id.clone(),
            run_id: Some(run_id.clone()),
            lease_id: Some(payload.lease_id.clone()),
            session_id: Some(session_id.to_string()),
            user_id: "mesh-worker".to_string(),
            mission_id: payload.task_id.clone(),
            cell_run_id: None,
            artifact_type: artifact.artifact_type.clone(),
            title: format!("{} artifact", artifact.artifact_type),
            uri: Some(artifact.location.clone()),
            path: None,
            content_json: json!({
                "hash": artifact.hash,
                "size_bytes": artifact.size_bytes,
                "location": artifact.location,
            })
            .to_string(),
            created_at: completed_at.clone(),
            owner_id: None,
            principal_id: Some(session_id.to_string()),
        })
        .collect();

    let summary = format!(
        "worker result {} for task {}",
        payload.status, payload.task_id
    );
    let _ = state.evidence.transport.journal(
        "task_dispatch_status",
        json!({
            "task_id": payload.task_id.clone(),
            "status": if payload.status.eq_ignore_ascii_case("success") {
                "complete"
            } else {
                "failed"
            },
            "detail": summary.clone(),
            "tokens_total": payload
                .metrics
                .tokens_in
                .saturating_add(payload.metrics.tokens_out),
            "duration_ms": payload.metrics.duration_ms,
        }),
    );
    let _ = state
        .evidence
        .record_receipt_bundle(
            PersistedRunReceipt {
                run_id,
                user_id: "mesh-worker".to_string(),
                proposal_id: payload.task_id.clone(),
                lease_id: payload.lease_id.clone(),
                session_id: Some(session_id.to_string()),
                started_at: completed_at.clone(),
                ended_at: completed_at,
                status: payload.status.to_uppercase(),
                chain_result_json: json!(payload).to_string(),
                signals_json: json!({
                    "session_id": session_id,
                    "node_id": node_id,
                })
                .to_string(),
                artifact_index_json: json!(artifact_ids).to_string(),
                node_id: node_id.to_string(),
                replay_receipt_json: json!({
                    "task_id": payload.task_id,
                    "lease_id": payload.lease_id,
                    "session_id": session_id,
                })
                .to_string(),
                mode: "worker_mesh".to_string(),
                model_id: lease.capability.clone(),
                tokens_input: payload.metrics.tokens_in as i64,
                tokens_output: payload.metrics.tokens_out as i64,
                tokens_total: (payload.metrics.tokens_in + payload.metrics.tokens_out) as i64,
                cost: 0.0,
                owner_id: None,
                principal_id: Some(session_id.to_string()),
                failure_code: None,
                failure_message: None,
            },
            artifacts,
        )
        .await;
    let _ = state.evidence.transport.journal(
        "capability_lease_revocations",
        json!({
            "lease_id": payload.lease_id.clone(),
            "revoked_at": now_iso(),
            "reason": "completed",
        }),
    );
    state.worker_registry.write().await.complete_dispatch(
        &state.evidence,
        &payload.lease_id,
        "completed",
        None,
        Some(payload.status.clone()),
        now_ms(),
    );
    Ok(())
}

async fn finalize_error(
    state: &SharedState,
    session_id: &str,
    node_id: &str,
    payload: ErrorPayload,
) -> Result<(), ProtocolError> {
    let task_id = payload.task_id.clone().ok_or_else(|| ProtocolError {
        code: "TASK_NOT_FOUND",
        message: "error payload missing task_id".to_string(),
    })?;
    let lease_id = payload.lease_id.clone().ok_or_else(|| ProtocolError {
        code: "LEASE_NOT_FOUND",
        message: "error payload missing lease_id".to_string(),
    })?;

    {
        let registry = state.worker_registry.read().await;
        registry
            .validate_lease(session_id, &task_id, &lease_id, now_ms())
            .map_err(|error| ProtocolError {
                code: error_code(&error.code),
                message: error.message,
            })?;
    }

    let run_id = format!("run-{task_id}");
    let artifact_id = format!("artifact-{}", uuid::Uuid::new_v4());
    let completed_at = now_iso();
    let _ = state.evidence.transport.journal(
        "task_dispatch_status",
        json!({
            "task_id": task_id.clone(),
            "status": "failed",
            "detail": payload.message.clone(),
        }),
    );
    let _ = state
        .evidence
        .record_receipt_bundle(
            PersistedRunReceipt {
                run_id: run_id.clone(),
                user_id: "mesh-worker".to_string(),
                proposal_id: task_id.clone(),
                lease_id: lease_id.clone(),
                session_id: Some(session_id.to_string()),
                started_at: completed_at.clone(),
                ended_at: completed_at.clone(),
                status: "FAILED".to_string(),
                chain_result_json: json!(payload).to_string(),
                signals_json: json!({
                    "session_id": session_id,
                    "node_id": node_id,
                })
                .to_string(),
                artifact_index_json: json!([artifact_id.clone()]).to_string(),
                node_id: node_id.to_string(),
                replay_receipt_json: json!({
                    "task_id": task_id,
                    "lease_id": lease_id,
                    "session_id": session_id,
                    "retryable": payload.retryable,
                })
                .to_string(),
                mode: "worker_mesh".to_string(),
                model_id: "error".to_string(),
                tokens_input: 0,
                tokens_output: 0,
                tokens_total: 0,
                cost: 0.0,
                owner_id: None,
                principal_id: Some(session_id.to_string()),
                failure_code: Some(payload.code.clone()),
                failure_message: Some(payload.message.clone()),
            },
            vec![PersistedArtifact {
                artifact_id,
                run_id: Some(run_id.clone()),
                lease_id: Some(lease_id.clone()),
                session_id: Some(session_id.to_string()),
                user_id: "mesh-worker".to_string(),
                mission_id: task_id.clone(),
                cell_run_id: None,
                artifact_type: "log".to_string(),
                title: "worker error log".to_string(),
                uri: Some(format!("artifact://runs/{task_id}/error")),
                path: None,
                content_json: json!({
                    "code": payload.code,
                    "message": payload.message,
                    "retryable": payload.retryable,
                })
                .to_string(),
                created_at: completed_at,
                owner_id: None,
                principal_id: Some(session_id.to_string()),
            }],
        )
        .await;

    let failure_type = match payload.code.as_str() {
        "TIMEOUT" | "CONNECTION_LOST" | "SYSTEM_ERROR" => "system",
        "EXEC_ERROR" | "TOOL_NOT_FOUND" | "PERMISSION_DENIED" => "tool",
        "MODEL_ERROR" | "INFERENCE_FAILED" | "TOKEN_LIMIT" => "model",
        "INVALID_INPUT" | "USER_ABORT" => "terminal",
        _ => "worker",
    };

    let _ = state
        .evidence
        .record_run_failure(
            &run_id,
            &lease_id,
            session_id,
            &payload.code,
            &payload.message,
            failure_type,
            payload.retryable,
            &json!(payload).to_string(),
        )
        .await;
    let _ = state.evidence.transport.journal(
        "capability_lease_revocations",
        json!({
            "lease_id": lease_id.clone(),
            "revoked_at": now_iso(),
            "reason": payload.code.clone(),
        }),
    );
    state.worker_registry.write().await.complete_dispatch(
        &state.evidence,
        &lease_id,
        "failed",
        Some(payload.code.clone()),
        Some(payload.message.clone()),
        now_ms(),
    );
    Ok(())
}

fn legacy_task_assignment_message(
    lease: &crate::runtime::state::WorkerLeaseRecord,
    input: Value,
    policy: &DispatchPolicy,
) -> Message {
    Message::Text(
        json!({
            "type": "task_assignment",
            "data": {
                "task_id": lease.task_id,
                "lease_id": lease.lease_id,
                "capability": lease.capability,
                "instruction": input.get("instruction").cloned().unwrap_or_else(|| input.clone()),
                "raw_text": input.get("raw_text").cloned().unwrap_or_else(|| input.clone()),
                "target_tool": input.get("target_tool").and_then(Value::as_str).unwrap_or("openclaw"),
                "policy": {
                    "side_effects": policy.side_effects,
                    "timeout_ms": policy.timeout_ms,
                }
            }
        })
        .to_string(),
    )
}

fn normalize_legacy_capabilities(raw: &Value) -> (Vec<String>, Value) {
    let mut capabilities = Vec::new();
    if let Some(items) = raw.as_array() {
        capabilities = items
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect();
    } else if let Some(items) = raw.get("capabilities").and_then(Value::as_array) {
        capabilities = items
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect();
    }
    if capabilities.is_empty() {
        capabilities.push("llm".to_string());
    }
    (capabilities, raw.clone())
}

fn extract_worker_token(headers: &HeaderMap, params: &HashMap<String, String>) -> Option<String> {
    if let Some(header) = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
    {
        if let Some(token) = header.strip_prefix("Bearer ") {
            return Some(token.trim().to_string());
        }
        if !header.is_empty() {
            return Some(header.to_string());
        }
    }
    params.get("token").cloned()
}

fn worker_token_matches(token: Option<String>, state: &SharedState) -> bool {
    token
        .map(|token| token == state.config.machine_auth_token)
        .unwrap_or(false)
}

fn error_code(code: &RegistryErrorCode) -> &'static str {
    match code {
        RegistryErrorCode::SessionExpired => "SESSION_EXPIRED",
        RegistryErrorCode::LeaseExpired => "LEASE_EXPIRED",
        RegistryErrorCode::LeaseNotFound => "LEASE_NOT_FOUND",
        RegistryErrorCode::TaskNotFound => "TASK_NOT_FOUND",
        RegistryErrorCode::CapabilityMismatch => "CAPABILITY_MISMATCH",
        RegistryErrorCode::DispatchRejected => "DISPATCH_REJECTED",
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis() as u64
}

fn now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn iso_from_ms(ms: u64) -> String {
    OffsetDateTime::from_unix_timestamp((ms / 1000) as i64)
        .ok()
        .and_then(|timestamp| timestamp.format(&Rfc3339).ok())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

fn fake_digest(input: &str) -> String {
    let mut acc: u64 = 0;
    for byte in input.bytes() {
        acc = acc.wrapping_mul(16777619).wrapping_add(byte as u64);
    }
    format!("{acc:016x}")
}
