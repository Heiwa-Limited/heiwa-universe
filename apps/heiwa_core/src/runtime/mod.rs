use anyhow::{anyhow, Result};
use axum::{
    response::{IntoResponse, Redirect},
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
use crate::stdb::{ReducerTransport, StdbRuntime};
use heiwa_bindings::{
    upsert_model_tier_reducer::upsert_model_tier,
    upsert_node_heartbeat_reducer::upsert_node_heartbeat, DbConnection, ModelTier,
};

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

    // 1. Initialize STDB connection
    info!(
        "Connecting to SpacetimeDB at {}/{}",
        cfg.stdb_url, cfg.stdb_identity
    );
    let conn = DbConnection::builder()
        .with_uri(&cfg.stdb_url)
        .with_database_name(&cfg.stdb_identity)
        .with_token(if cfg.stdb_token.is_empty() {
            None
        } else {
            Some(&cfg.stdb_token)
        })
        .build()?;

    let conn_arc = Arc::new(conn);
    let conn_clone = conn_arc.clone();

    // 2. Start STDB background task
    tokio::spawn(async move {
        loop {
            if let Err(e) = conn_clone.advance_one_message_async().await {
                error!("STDB connection error: {:?}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        }
    });

    let transport = ReducerTransport::new(conn_arc.clone());
    let stdb_runtime = StdbRuntime::new(transport);
    let state = Arc::new(CoreState::new(cfg.clone(), stdb_runtime));

    // 3. Seed runtime catalogs and register node
    let state_clone = state.clone();
    let conn_heartbeat = conn_arc.clone();
    tokio::spawn(async move {
        // Wait for connection to be ready
        sleep(Duration::from_secs(2)).await;

        match seed_catalogs(&conn_heartbeat, state_clone.clone()).await {
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
            if let Err(e) = heartbeat(&conn_heartbeat, &state_clone.config).await {
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
        .route("/install", get(install_handler))
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

async fn seed_catalogs(conn: &DbConnection, state: SharedState) -> Result<()> {
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
        conn.reducers.upsert_model_tier(
            seed.model_id.clone(),
            seed.provider_model_id.clone(),
            seed.provider.clone(),
            seed.rate_group.clone(),
            seed.capability_class,
            seed.effort_knob.clone(),
            seed.effort_level,
            seed.cost_per_turn,
            seed.max_context_tokens,
            seed.vram_requirement_mb,
            seed.quantization_type.clone(),
            seed.kv_cache_strategy.clone(),
            strengths_json.clone(),
            seed.enabled,
        )?;

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

    {
        let mut state_tiers = state.model_tiers.write().await;
        *state_tiers = model_tiers;
    }

    Ok(())
}

async fn heartbeat(conn: &DbConnection, cfg: &RuntimeConfig) -> Result<()> {
    // TODO: Use sysinfo crate to gather real VRAM info
    let vram_mb = 0;
    let locality = "macbook".to_string();
    let trust_tier = 9; // Owner-local runtime trust.
    let provider_keys = "[]".to_string();
    let model_inventory = "[]".to_string();

    conn.reducers
        .upsert_node_heartbeat(
            cfg.node_id.clone(),
            "heiwa-core".to_string(),
            "ready".to_string(),
            "{}".to_string(),
            "{}".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "[]".to_string(),
            10,
            vram_mb,
            locality,
            trust_tier,
            provider_keys,
            model_inventory,
        )
        .map_err(|e| anyhow!(e.to_string()))
}

async fn install_handler() -> impl IntoResponse {
    Redirect::permanent("https://heiwa-clients.pages.dev/install.sh")
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

pub fn plan_ingress(
    ingress: &DrexIngress,
    model_tiers: &[heiwa_bindings::ModelTier],
) -> Result<RoutePlan> {
    plan_route(ingress, model_tiers, &default_policy())
}
