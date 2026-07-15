use anyhow::Result;
use axum::{
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

pub mod gateway;
pub mod state;

use self::state::{CoreState, SharedState, SystemStatus};
use crate::auth;
use crate::config::RuntimeConfig;
use crate::drex::{default_policy, plan_route, DrexIngress, RoutePlan};
use crate::evidence::{EvidenceRuntime, EvidenceTransport, JsonlTransport};
use heiwa_protocol::ModelTier;

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ModelTierSeed {
    model_id: String,
    provider_model_id: String,
    provider: String,
    rate_group: String,
    capability_class: u8,
    effort_knob: String,
    effort_level: u8,
    cost_per_turn: f64,
    max_context_tokens: u32,
    strengths: Vec<String>,
    vram_requirement_mb: u32,
    quantization_type: String,
    kv_cache_strategy: String,
    enabled: bool,
}

pub async fn run(cfg: RuntimeConfig) -> Result<()> {
    info!("Initializing Heiwa Core Runtime...");

    // 1. Local evidence plane (JSONL under ~/.heiwa/evidence/)
    let transport = JsonlTransport::default_local()?;
    let evidence_runtime = EvidenceRuntime::new(transport);
    let state = Arc::new(CoreState::new(cfg.clone(), evidence_runtime));

    // 2. Seed runtime catalogs and start heartbeat journal
    let state_clone = state.clone();
    tokio::spawn(async move {
        match seed_catalogs(state_clone.clone()).await {
            Ok(_) => {
                info!("Runtime catalogs seeded successfully");
                let mut seeded = state_clone.seeded.write().await;
                *seeded = true;
            }
            Err(e) => {
                error!("Failed to seed runtime catalogs: {:?}", e);
            }
        }

        loop {
            if let Err(e) = heartbeat(&state_clone).await {
                error!("Node heartbeat failed: {:?}", e);
                let mut status = state_clone.status.write().await;
                *status = SystemStatus::Degraded;
            } else {
                let mut status = state_clone.status.write().await;
                let seeded = state_clone.seeded.read().await;
                if *seeded {
                    if *status == SystemStatus::Starting || *status == SystemStatus::Degraded {
                        *status = SystemStatus::Ready;
                    }
                } else {
                    // Stay starting/red if not seeded
                    *status = SystemStatus::Starting;
                }
            }
            sleep(Duration::from_secs(30)).await;
        }
    });

    let app = build_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    info!("Heiwa Core listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/status", get(status_handler))
        .route("/auth/me", get(auth::auth_me_handler))
        .route("/ws", get(gateway::ws_handler))
        .route("/ws/client", get(gateway::ws_client_handler))
        .route("/ws/worker", get(gateway::ws_worker_handler))
        .route("/ws/worker/legacy", get(gateway::ws_worker_legacy_handler))
        .route("/battlefields", post(gateway::battlefield_handler))
        .route("/tasks", post(gateway::task_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn seed_catalogs(state: SharedState) -> Result<()> {
    let path = std::path::Path::new(&state.config.model_tiers_seed_path);
    if !path.exists() {
        warn!(
            "Model tiers seed file not found: {}",
            state.config.model_tiers_seed_path
        );
        return Ok(());
    }

    let data = std::fs::read_to_string(path)?;
    let seeds: Vec<ModelTierSeed> = serde_json::from_str(&data)?;

    info!("Seeding {} model tiers...", seeds.len());
    let mut model_tiers = Vec::new();
    for seed in seeds {
        let strengths_json = serde_json::to_string(&seed.strengths)?;
        model_tiers.push(ModelTier {
            id: 0,
            model_id: seed.model_id,
            provider_model_id: seed.provider_model_id,
            provider: seed.provider,
            rate_group: seed.rate_group,
            capability_class: seed.capability_class,
            effort_knob: seed.effort_knob,
            effort_level: seed.effort_level,
            cost_per_turn: seed.cost_per_turn,
            max_context_tokens: seed.max_context_tokens,
            vram_requirement_mb: seed.vram_requirement_mb,
            quantization_type: seed.quantization_type,
            kv_cache_strategy: seed.kv_cache_strategy,
            strengths_json,
            enabled: seed.enabled,
            last_success_rate: 1.0,
            avg_latency_ms: 0,
            latency_p_95_ms: 0,
            updated_at: "".to_string(),
        });
    }

    let _ = state.evidence.transport.journal(
        "model_tier_seeds",
        json!({ "count": model_tiers.len(), "tiers": &model_tiers }),
    );

    {
        let mut state_tiers = state.model_tiers.write().await;
        *state_tiers = model_tiers;
    }

    Ok(())
}

async fn heartbeat(state: &SharedState) -> Result<()> {
    // TODO: Use sysinfo crate to gather real VRAM info
    let vram_mb = 0;
    let locality = "macbook".to_string();
    let trust_tier = 9; // Owner-local runtime trust.

    state.evidence.transport.journal(
        "node_heartbeats",
        json!({
            "node_id": state.config.node_id,
            "service": "heiwa-core",
            "status": "ready",
            "version": env!("CARGO_PKG_VERSION"),
            "vram_mb": vram_mb,
            "locality": locality,
            "trust_tier": trust_tier,
        }),
    )
}

async fn health_handler(
    axum::extract::State(state): axum::extract::State<SharedState>,
) -> impl IntoResponse {
    let status = state.status.read().await;
    let is_ready = *status == SystemStatus::Ready;

    let body = Json(json!({
        "status": status.as_str(),
        "service": "heiwa-core",
        "ready": is_ready,
        "timestamp": time::OffsetDateTime::now_utc().unix_timestamp(),
    }));

    if is_ready {
        (axum::http::StatusCode::OK, body)
    } else {
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, body)
    }
}

async fn status_handler(
    axum::extract::State(state): axum::extract::State<SharedState>,
) -> impl IntoResponse {
    let status = state.status.read().await;
    Json(json!({
        "node_id": state.config.node_id,
        "status": format!("{:?}", *status),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

pub fn plan_ingress(ingress: &DrexIngress, model_tiers: &[ModelTier]) -> Result<RoutePlan> {
    plan_route(ingress, model_tiers, &default_policy())
}
