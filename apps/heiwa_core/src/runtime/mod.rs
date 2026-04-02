use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use anyhow::{anyhow, Result};
use axum::{
    routing::{get, post},
    Router,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::trace::TraceLayer;
use tracing::{info, error, warn};
use tokio::time::sleep;

pub mod state;
pub mod gateway;

use crate::config::RuntimeConfig;
use crate::drex::{default_policy, plan_route, DrexIngress, RoutePlan};
use self::state::{CoreState, SharedState, SystemStatus};
use heiwa_bindings::{
    DbConnection,
    upsert_model_tier_reducer::upsert_model_tier,
    upsert_node_heartbeat_reducer::upsert_node_heartbeat,
    ModelTier,
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
    
    let state = Arc::new(CoreState::new(cfg.clone()));
    
    // 1. Initialize STDB connection
    info!("Connecting to SpacetimeDB at {}/{}", cfg.stdb_url, cfg.stdb_identity);
    let conn = DbConnection::builder()
        .with_uri(&cfg.stdb_url)
        .with_database_name(&cfg.stdb_identity)
        .with_token(Some(&cfg.auth_token))
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

    // 3. Seed runtime catalogs and register node
    let state_clone = state.clone();
    let conn_heartbeat = conn_arc.clone();
    tokio::spawn(async move {
        // Wait for connection to be ready
        sleep(Duration::from_secs(2)).await;
        
        match seed_catalogs(&conn_heartbeat, state_clone.clone()).await {
            Ok(_) => info!("Runtime catalogs seeded successfully"),
            Err(e) => error!("Failed to seed runtime catalogs: {:?}", e),
        }

        loop {
            if let Err(e) = heartbeat(&conn_heartbeat, &state_clone.config).await {
                error!("Node heartbeat failed: {:?}", e);
                let mut status = state_clone.status.write().await;
                *status = SystemStatus::Degraded;
            } else {
                let mut status = state_clone.status.write().await;
                if *status == SystemStatus::Starting || *status == SystemStatus::Degraded {
                    *status = SystemStatus::Ready;
                }
            }
            sleep(Duration::from_secs(30)).await;
        }
    });

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route("/status", get(status_handler))
        .route("/ws", get(gateway::ws_handler))
        .route("/ws/client", get(gateway::ws_client_handler))
        .route("/battlefields", post(gateway::battlefield_handler))
        .route("/tasks", post(gateway::task_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    info!("Heiwa Core listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn seed_catalogs(conn: &DbConnection, state: SharedState) -> Result<()> {
    let path = std::path::Path::new(&state.config.model_tiers_seed_path);
    if !path.exists() {
        warn!("Model tiers seed file not found: {}", state.config.model_tiers_seed_path);
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
            strengths_json.clone(),
            seed.vram_requirement_mb,
            seed.quantization_type.clone(),
            seed.kv_cache_strategy.clone(),
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
            strengths_json,
            vram_requirement_mb: seed.vram_requirement_mb,
            quantization_type: seed.quantization_type,
            kv_cache_strategy: seed.kv_cache_strategy,
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
    conn.reducers.upsert_node_heartbeat(
        cfg.node_id.clone(),
        "cloud-hq".to_string(),
        "ready".to_string(),
        "{}".to_string(),
        "{}".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
        "[]".to_string(),
        10,
    ).map_err(|e| anyhow!(e.to_string()))
}

async fn health_handler() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn ready_handler(axum::extract::State(state): axum::extract::State<SharedState>) -> impl IntoResponse {
    let status = state.status.read().await;
    if *status == SystemStatus::Ready {
        (axum::http::StatusCode::OK, Json(json!({ "ready": true })))
    } else {
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "ready": false })))
    }
}

async fn status_handler(axum::extract::State(state): axum::extract::State<SharedState>) -> impl IntoResponse {
    let status = state.status.read().await;
    Json(json!({
        "node_id": state.config.node_id,
        "status": format!("{:?}", *status),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

pub fn plan_ingress(ingress: &DrexIngress, model_tiers: &[heiwa_bindings::ModelTier]) -> Result<RoutePlan> {
    plan_route(ingress, model_tiers, &default_policy())
}
