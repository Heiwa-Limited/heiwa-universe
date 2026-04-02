use axum::{
    extract::{State, WebSocketUpgrade, ws::{WebSocket, Message}},
    response::IntoResponse,
    Json,
};
use futures_util::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};
use crate::runtime::state::SharedState;
use crate::drex::{DrexIngress, plan_route, default_policy};

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
    State(_state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_worker_socket(socket, _state))
}

pub async fn battlefield_handler(
    State(_state): State<SharedState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    info!("Legacy battlefield request: {:?}", payload);
    // Compatibility adapter for WireClient
    Json(json!({ "status": "ok", "battlefield_id": format!("bf-{}", uuid::Uuid::new_v4()) }))
}

pub async fn task_handler(
    State(_state): State<SharedState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    info!("Legacy task request: {:?}", payload);
    // Compatibility adapter for WireClient
    // If it looks like a route.preview, we could actually run it.
    // For Phase 1, we just return a mock task_id.
    Json(json!({ "status": "ok", "task_id": format!("task-{}", uuid::Uuid::new_v4()) }))
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

async fn handle_worker_socket(_socket: WebSocket, _state: SharedState) {
    info!("New worker WS connection (minimal registration support)");
    // Phase 1 only needs to support authenticated worker registration, 
    // capability advertisement, keepalive, and STDB node updates.
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
