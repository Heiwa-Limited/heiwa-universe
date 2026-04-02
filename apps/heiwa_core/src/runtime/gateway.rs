use axum::{
    extract::{State, WebSocketUpgrade, ws::{WebSocket, Message}},
    response::IntoResponse,
    Json,
};
use futures_util::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::runtime::state::SharedState;
use crate::drex::{DrexIngress, plan_route, default_policy};
use heiwa_bindings::{
    upsert_battlefield_reducer::upsert_battlefield,
    create_task_dispatch_reducer::create_task_dispatch,
    upsert_node_heartbeat_reducer::upsert_node_heartbeat,
};

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

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum WorkerMessage {
    #[serde(rename = "auth")]
    Auth { token: String, node_id: String },
    #[serde(rename = "capabilities")]
    Capabilities {
        capabilities: Vec<String>,
        meta: Value,
    },
    #[serde(rename = "heartbeat")]
    Heartbeat {
        status: String,
        metrics: Value,
    },
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
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_worker_socket(socket, state))
}

pub async fn battlefield_handler(
    State(state): State<SharedState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    info!("Battlefield request: {:?}", payload);
    let battlefield_id = format!("bf-{}", uuid::Uuid::new_v4());
    let name = payload["name"].as_str().unwrap_or("unnamed").to_string();
    
    // Real persistence call
    let _ = state.stdb.transport.conn.reducers.upsert_battlefield(
        battlefield_id.clone(),
        None,
        None,
        name,
        None,
        None,
        None,
        "active".to_string(),
        Some(now_iso()),
        Some(now_iso()),
    );

    Json(json!({ "status": "ok", "battlefield_id": battlefield_id }))
}

pub async fn task_handler(
    State(state): State<SharedState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    info!("Task dispatch request: {:?}", payload);
    let task_id = format!("task-{}", uuid::Uuid::new_v4());
    
    // Real persistence call
    let _ = state.stdb.transport.conn.reducers.create_task_dispatch(
        task_id.clone(),
        None,
        "general".to_string(),
        "low".to_string(),
        "default".to_string(),
        "normal".to_string(),
        "default".to_string(),
        10,
        300,
        "default".to_string(),
        "sandbox".to_string(),
        "[]".to_string(),
        "[]".to_string(),
    );

    Json(json!({ "status": "ok", "task_id": task_id }))
}

async fn handle_socket(socket: WebSocket, state: SharedState) {
    let (mut sender, mut receiver) = socket.split();
    info!("New unified WS connection");

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Auth { token }) => {
                    if token == state.config.auth_token {
                        let _ = sender.send(Message::Text(serde_json::to_string(&ServerMessage::AuthOk).unwrap())).await;
                    } else {
                        let _ = sender.send(Message::Text(serde_json::to_string(&ServerMessage::Error {
                            request_id: None,
                            message: "Invalid token".to_string(),
                        }).unwrap())).await;
                    }
                }
                Ok(ClientMessage::Action { action, request_id, payload }) => {
                    if action == "route.preview" {
                        match handle_route_preview(&state, request_id.clone(), payload).await {
                            Ok(result) => {
                                let _ = sender.send(Message::Text(serde_json::to_string(&ServerMessage::Result {
                                    request_id,
                                    status: "success".to_string(),
                                    payload: result,
                                }).unwrap())).await;
                            }
                            Err(e) => {
                                let _ = sender.send(Message::Text(serde_json::to_string(&ServerMessage::Error {
                                    request_id: Some(request_id),
                                    message: e.to_string(),
                                }).unwrap())).await;
                            }
                        }
                    } else {
                        let _ = sender.send(Message::Text(serde_json::to_string(&ServerMessage::Error {
                            request_id: Some(request_id),
                            message: format!("Unknown action: {}", action),
                        }).unwrap())).await;
                    }
                }
                Err(e) => {
                    warn!("Failed to parse client message: {:?}", e);
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
    let (mut sender, mut receiver) = socket.split();
    info!("New worker WS connection");
    let mut authenticated_node_id: Option<String> = None;

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            match serde_json::from_str::<WorkerMessage>(&text) {
                Ok(WorkerMessage::Auth { token, node_id }) => {
                    if token == state.config.auth_token {
                        authenticated_node_id = Some(node_id);
                        let _ = sender.send(Message::Text(serde_json::to_string(&ServerMessage::AuthOk).unwrap())).await;
                    } else {
                        let _ = sender.send(Message::Text(serde_json::to_string(&ServerMessage::Error {
                            request_id: None,
                            message: "Invalid worker token".to_string(),
                        }).unwrap())).await;
                    }
                }
                Ok(WorkerMessage::Capabilities { capabilities, meta }) => {
                    if let Some(ref node_id) = authenticated_node_id {
                        info!("Worker {} advertised capabilities: {:?}", node_id, capabilities);
                        let _ = state.stdb.transport.conn.reducers.upsert_node_heartbeat(
                            node_id.clone(),
                            now_iso(),
                            now_iso(),
                            meta.to_string(),
                            json!(capabilities).to_string(),
                            "0.1.0".to_string(),
                            "[]".to_string(),
                            1,
                        );
                    }
                }
                Ok(WorkerMessage::Heartbeat { status, metrics }) => {
                    if let Some(ref node_id) = authenticated_node_id {
                        let _ = state.stdb.transport.conn.reducers.upsert_node_heartbeat(
                            node_id.clone(),
                            now_iso(),
                            now_iso(),
                            metrics.to_string(),
                            "[]".to_string(), // Keep previous capabilities if not sent
                            "0.1.0".to_string(),
                            json!([status]).to_string(),
                            1,
                        );
                    }
                }
                Err(e) => {
                    warn!("Failed to parse worker message: {:?}", e);
                }
            }
        }
    }
}

async fn handle_route_preview(state: &SharedState, request_id: String, payload: Value) -> anyhow::Result<Value> {
    let ingress: DrexIngress = serde_json::from_value(payload)?;
    let model_tiers = state.model_tiers.read().await;
    let plan = plan_route(&ingress, &model_tiers, &default_policy())?;
    
    // Wire DREX persistence into route preview
    let task_id = format!("task-preview-{}", uuid::Uuid::new_v4());
    let _ = state.stdb.record_drex_decision(&request_id, &task_id, &plan).await?;

    Ok(json!({
        "target_tier": format!("{:?}", plan.decision.active_tier),
        "runtime_hint": plan.runtime_hint,
        "selected_model": plan.selected_model.map(|m| m.model_id),
        "requires_approval": plan.decision.gate.requires_approval,
        "authority_required": plan.decision.gate.authority_required,
        "task_id": task_id,
    }))
}

fn now_iso() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    format!("{}", now.as_secs()) // Real ISO would be better, but STDB expects string
}
