use spacetimedb::{reducer, table, view, Identity, ReducerContext, Table, ViewContext};

#[table(accessor = organization_tasks, private)]
pub struct OrganizationTask {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub tenant_id: Identity,
    pub payload: String,
    #[index(btree)]
    pub status: String,
    pub assigned_worker: Option<String>,
}

#[table(accessor = proposals, public)]
pub struct Proposal {
    #[primary_key]
    pub proposal_id: String,
    pub created_at: String,
    #[index(btree)]
    pub status: String,
    pub fingerprint: Option<String>,
    pub payload: String,
    pub payload_raw: Option<String>,
    pub node_id: Option<String>,
    pub claimed_at: Option<String>,
    pub lease_expires_at: Option<String>,
    pub last_heartbeat_at: Option<String>,
    pub last_heartbeat_node_id: Option<String>,
    pub last_heartbeat_node_instance_id: Option<String>,
    pub last_heartbeat_detail: Option<String>,
    pub mode: String,
    pub execution_targeting: Option<String>,
    pub assigned_node_id: Option<String>,
    pub assignment_expires_at: Option<String>,
    pub attempt_count: i64,
    pub proposal_hash: Option<String>,
    pub hub_signature: Option<String>,
    pub approved_at: Option<String>,
    pub expires_at: Option<String>,
    pub eligibility_snapshot: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub user_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
}

#[table(accessor = proposal_consents, public)]
pub struct ProposalConsent {
    #[primary_key]
    pub consent_id: String,
    #[index(btree)]
    pub proposal_id: String,
    pub proposal_hash: String,
    pub actor_type: String,
    pub actor_id: String,
    pub decision: String,
    pub comment: Option<String>,
    pub created_at: String,
    pub metadata: Option<String>,
}

#[table(accessor = events, public)]
pub struct EventRecord {
    #[primary_key]
    pub event_id: String,
    #[index(btree)]
    pub owner_id: Option<String>,
    #[index(btree)]
    pub principal_id: Option<String>,
    pub session_id: Option<String>,
    pub mission_id: Option<String>,
    pub battlefield_id: Option<String>,
    #[index(btree)]
    pub event_type: String,
    pub payload_json: String,
    #[index(btree)]
    pub created_at: String,
}

#[table(accessor = battlefields, public)]
pub struct BattlefieldRecord {
    #[primary_key]
    pub battlefield_id: String,
    #[index(btree)]
    pub owner_id: Option<String>,
    #[index(btree)]
    pub principal_id: Option<String>,
    pub name: String,
    pub repo_url: Option<String>,
    pub root_path: Option<String>,
    #[index(btree)]
    pub node_id: Option<String>,
    #[index(btree)]
    pub status: String,
    #[index(btree)]
    pub created_at: String,
    #[index(btree)]
    pub last_active_at: String,
}

#[table(accessor = capability_leases, public)]
pub struct CapabilityLease {
    #[primary_key]
    pub lease_id: String,
    #[index(btree)]
    pub proposal_id: String,
    pub run_id: Option<String>,
    pub holder_kind: String,
    #[index(btree)]
    pub holder_id: String,
    pub tool_scope_json: String,
    pub network_scope_json: String,
    pub filesystem_scope_json: String,
    pub secret_scope_json: String,
    pub privilege_tier: String,
    #[index(btree)]
    pub status: String,
    pub issued_at: String,
    pub renewed_at: Option<String>,
    #[index(btree)]
    pub expires_at: String,
    pub revoked_at: Option<String>,
    pub revocation_reason: Option<String>,
    pub hub_signature: String,
    #[default(None::<String>)]
    #[index(btree)]
    pub parent_lease_id: Option<String>,
    #[default(None::<String>)]
    pub failure_policy: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub chain_state: Option<String>,
    #[default(None::<String>)]
    pub routing_lock_json: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
}

#[table(accessor = approval_requests, public)]
pub struct ApprovalRequest {
    #[primary_key]
    pub request_id: String,
    #[index(btree)]
    pub proposal_id: String,
    #[index(btree)]
    pub status: String,
    pub requested_at: String,
    pub expires_at: Option<String>,
    pub requested_by: String,
    pub reason: Option<String>,
    pub payload_json: String,
}

#[table(accessor = approval_decisions, public)]
pub struct ApprovalDecision {
    #[primary_key]
    pub decision_id: String,
    #[index(btree)]
    pub request_id: String,
    #[index(btree)]
    pub proposal_id: String,
    pub actor_type: String,
    pub actor_id: String,
    pub decision: String,
    pub reason: Option<String>,
    pub created_at: String,
    pub metadata_json: String,
}

#[table(accessor = route_decisions, public)]
pub struct RouteDecision {
    #[primary_key]
    pub request_id: String,
    #[index(btree)]
    pub task_id: String,
    pub envelope_version: String,
    pub raw_text: String,
    pub source_surface: String,
    pub intent_class: String,
    pub risk_level: String,
    pub privacy_level: String,
    pub compute_class: i64,
    pub assigned_worker: String,
    pub target_tool: String,
    pub target_model: String,
    pub target_runtime: String,
    pub target_tier: String,
    pub requires_approval: bool,
    pub rationale: String,
    pub confidence: f64,
    pub gateway_transport: String,
    #[index(btree)]
    pub created_at: String,
    #[default(None::<String>)]
    #[index(btree)]
    pub drex_decision_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub user_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
}

#[table(accessor = drex_decisions, public)]
pub struct DrexDecisionRow {
    #[primary_key]
    pub decision_id: String,
    #[index(btree)]
    pub request_id: String,
    #[index(btree)]
    pub task_id: String,
    #[index(btree)]
    pub active_tier: String,
    pub route_runtime: String,
    pub route_model: String,
    pub scope: f64,
    pub abstraction: f64,
    pub context_span: f64,
    pub execution_proximity: f64,
    pub blast_radius: f64,
    pub coordination_load: f64,
    pub latency_pressure: f64,
    pub macro_score: f64,
    pub meso_score: f64,
    pub micro_score: f64,
    pub score_confidence: f64,
    pub authority_required: String,
    pub requires_approval: bool,
    pub reasons_json: String,
    pub vector_json: String,
    pub scorecard_json: String,
    pub gate_json: String,
    pub policy_version: String,
    #[index(btree)]
    pub created_at_ms: u64,
}

#[table(accessor = drex_failures, public)]
pub struct DrexFailureRow {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub decision_id: String,
    #[index(btree)]
    pub request_id: String,
    pub failure_mode: String,
    pub stage: String,
    pub details_json: String,
    pub recovered: bool,
    #[index(btree)]
    pub created_at_ms: u64,
}

#[table(accessor = runs, public)]
pub struct RunRecord {
    #[primary_key]
    pub run_id: String,
    #[index(btree)]
    pub proposal_id: String,
    pub started_at: String,
    #[index(btree)]
    pub ended_at: String,
    #[index(btree)]
    pub status: String,
    pub chain_result_json: String,
    pub signals_json: String,
    pub artifact_index_json: String,
    pub node_id: String,
    pub replay_receipt_json: String,
    pub mode: String,
    #[index(btree)]
    pub model_id: String,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub tokens_total: i64,
    pub cost: f64,
    #[default(None::<String>)]
    #[index(btree)]
    pub user_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub lease_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub session_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
    #[default(None::<String>)]
    pub failure_code: Option<String>,
    #[default(None::<String>)]
    pub failure_message: Option<String>,
}

#[table(accessor = run_failures, public)]
pub struct RunFailure {
    #[primary_key]
    pub failure_id: String,
    #[index(btree)]
    pub run_id: String,
    #[index(btree)]
    pub lease_id: String,
    #[index(btree)]
    pub session_id: String,
    #[index(btree)]
    pub failure_code: String,
    pub failure_message: String,
    pub failure_type: String, // "transient" | "terminal" | "system"
    pub retryable: bool,
    pub details_json: String,
    pub created_at: String,
}

#[table(accessor = provider_accounts, public)]
pub struct ProviderAccount {
    #[primary_key]
    pub account_id: String,
    #[index(btree)]
    pub provider_id: String,
    #[index(btree)]
    pub node_id: String,
    pub auth_kind: String,
    pub local_handle_ref: String,
    #[index(btree)]
    pub status: String,
    pub display_name: Option<String>,
    pub default_model: Option<String>,
    pub rate_group: String,
    pub available_models_json: String,
    pub last_validated_at: Option<String>,
    pub last_error: Option<String>,
    #[index(btree)]
    pub updated_at: String,
    #[default(None::<String>)]
    #[index(btree)]
    pub user_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
}

#[table(accessor = missions, public)]
pub struct MissionRecord {
    #[primary_key]
    pub mission_id: String,
    #[index(btree)]
    pub created_at: String,
    #[index(btree)]
    pub updated_at: String,
    #[index(btree)]
    pub status: String,
    pub source_surface: String,
    pub node_id: String,
    pub prompt: String,
    pub intent_class: String,
    pub risk_level: String,
    pub active_step_id: Option<String>,
    pub active_cell_id: Option<String>,
    pub target_tool: Option<String>,
    pub target_model: Option<String>,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub metadata_json: String,
    #[default(None::<String>)]
    #[index(btree)]
    pub user_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
    // -- Doctrine Phase 1 expansion --
    /// "compile", "consolidate", "act"
    #[default(None::<String>)]
    #[index(btree)]
    pub task_class: Option<String>,
    /// "cheap", "standard", "premium"
    #[default(None::<String>)]
    pub budget_class: Option<String>,
    /// FK to Treasury
    #[default(None::<String>)]
    #[index(btree)]
    pub treasury_id: Option<String>,
    #[default(None::<String>)]
    pub novelty_policy_json: Option<String>,
    #[default(None::<String>)]
    pub termination_policy_json: Option<String>,
    #[default(None::<String>)]
    pub required_belief_query: Option<String>,
    #[default(None::<String>)]
    pub allowed_tools_json: Option<String>,
    #[default(None::<String>)]
    pub allowed_side_effects_json: Option<String>,
    #[default(None::<String>)]
    pub target_outputs_json: Option<String>,
    #[default(None::<i64>)]
    pub cost_so_far_millicents: Option<i64>,
    #[default(None::<f64>)]
    pub novelty_so_far: Option<f64>,
    #[default(None::<u32>)]
    pub turn_count: Option<u32>,
    /// "none_needed", "pending", "granted", "denied"
    #[default(None::<String>)]
    #[index(btree)]
    pub approval_state: Option<String>,
}

#[table(accessor = mission_steps, public)]
pub struct MissionStepRecord {
    #[primary_key]
    pub step_id: String,
    #[index(btree)]
    pub mission_id: String,
    pub position: i64,
    #[index(btree)]
    pub status: String,
    pub step_kind: String,
    pub cell_role: String,
    pub title: String,
    pub detail: Option<String>,
    pub input_json: String,
    pub output_json: String,
    pub created_at: String,
    pub updated_at: String,
    #[default(None::<String>)]
    #[index(btree)]
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
}

#[table(accessor = cell_runs, public)]
pub struct CellRunRecord {
    #[primary_key]
    pub cell_run_id: String,
    #[index(btree)]
    pub mission_id: String,
    pub step_id: Option<String>,
    #[index(btree)]
    pub status: String,
    pub cell_id: String,
    pub cell_role: String,
    pub provider: String,
    pub model: String,
    pub rate_group: String,
    pub tool: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub tokens_total: i64,
    pub output_summary: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub user_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
}

#[table(accessor = session_summaries, public)]
pub struct SessionSummaryRecord {
    #[primary_key]
    pub summary_id: String,
    #[index(btree)]
    pub session_id: String,
    #[index(btree)]
    pub node_id: String,
    #[index(btree)]
    pub created_at: String,
    pub summary_text: String,
    pub metadata_json: String,
    #[default(None::<String>)]
    #[index(btree)]
    pub user_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
}

#[table(accessor = artifacts, public)]
pub struct ArtifactRecord {
    #[primary_key]
    pub artifact_id: String,
    #[index(btree)]
    pub mission_id: String,
    #[default(None::<String>)]
    #[index(btree)]
    pub run_id: Option<String>,
    pub cell_run_id: Option<String>,
    #[index(btree)]
    pub artifact_type: String,
    pub title: String,
    pub uri: Option<String>,
    pub path: Option<String>,
    pub content_json: String,
    #[index(btree)]
    pub created_at: String,
    #[default(None::<String>)]
    #[index(btree)]
    pub lease_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub session_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub user_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
}

#[table(accessor = pods, public)]
pub struct Pod {
    #[primary_key]
    pub pod_id: String,
    pub host_identity: String,
    pub provider: String,
    pub runtime_capabilities: Vec<String>,
    pub trust_tier: String,
    pub privacy_floor: String,
    #[index(btree)]
    pub liveness: String,
    pub leaseable: bool,
    pub registered_at: String,
    pub last_heartbeat: String,
}

#[table(accessor = gpu_slots, public)]
pub struct GpuSlot {
    #[primary_key]
    pub slot_id: String,
    #[index(btree)]
    pub pod_id: String,
    pub gpu_type: String,
    pub vram_gb: i64,
    pub loaded_models: Vec<String>,
    pub available_slots: i64,
}

#[table(accessor = nodes, public)]
pub struct NodeStatus {
    #[primary_key]
    pub node_id: String,
    pub last_heartbeat_at: String,
    pub first_seen_at: String,
    #[index(btree)]
    pub last_seen_at: String,
    #[index(btree)]
    pub status: String,
    pub meta_json: String,
    pub capabilities_json: String,
    pub agent_version: String,
    pub tags_json: String,
    pub max_concurrency: i64,
    pub vram_mb: i64,
    pub locality: String,
    pub trust_tier: i32,
    pub provider_keys_json: String,
    pub model_inventory_json: String,
}

#[table(accessor = liveness_state, public)]
pub struct LivenessState {
    #[primary_key]
    pub key: String,
    pub last_state: String,
    #[index(btree)]
    pub last_changed_at: String,
}

#[table(accessor = discord_users, public)]
pub struct DiscordUser {
    #[primary_key]
    pub user_id: u64,
    pub username: String,
    pub trust_score: f32,
    pub last_seen_at: u64,
}

#[table(accessor = discord_channels, public)]
pub struct DiscordChannel {
    #[primary_key]
    pub channel_id: u64,
    pub name: String,
    #[index(btree)]
    pub purpose: String,
    pub metadata_json: String,
}

#[table(accessor = discord_interactions, public)]
pub struct DiscordInteraction {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub user_id: u64,
    pub channel_id: u64,
    pub intent_class: String,
    pub timestamp: u64,
}

#[table(accessor = users, public)]
pub struct User {
    #[primary_key]
    pub user_id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    #[index(btree)]
    pub status: String,
    pub tier: String,
    pub created_at: String,
    #[index(btree)]
    pub last_seen_at: String,
    pub metadata_json: String,
}

#[table(accessor = oauth_identities, public)]
pub struct OAuthIdentity {
    #[primary_key]
    pub identity_id: String,
    #[index(btree)]
    pub user_id: String,
    pub provider: String,
    pub provider_user_id: String,
    pub username: String,
    pub access_token_enc: String,
    pub refresh_token_enc: Option<String>,
    pub token_expires_at: Option<String>,
    pub scopes_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[table(accessor = provider_credentials, public)]
pub struct ProviderCredential {
    #[primary_key]
    pub credential_id: String,
    #[index(btree)]
    pub user_id: String,
    #[index(btree)]
    pub provider_id: String,
    pub credential_kind: String,
    pub credential_enc: String,
    #[index(btree)]
    pub status: String,
    pub rate_group: String,
    pub display_label: Option<String>,
    pub last_validated_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[table(accessor = billing_events, public)]
pub struct BillingEvent {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub user_id: String,
    #[index(btree)]
    pub event_type: String,
    pub provider_id: String,
    pub model_id: String,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub estimated_cost_usd: f64,
    pub credential_id: Option<String>,
    pub mission_id: Option<String>,
    pub run_id: Option<String>,
    #[index(btree)]
    pub created_at: String,
}

#[view(accessor = tenant_task_view, public)]
pub fn tenant_tasks(ctx: &ViewContext) -> Vec<OrganizationTask> {
    ctx.db
        .organization_tasks()
        .tenant_id()
        .filter(ctx.sender())
        .collect()
}

fn now_string(ctx: &ReducerContext) -> String {
    (ctx.timestamp.to_micros_since_unix_epoch() / 1_000_000).to_string()
}

fn normalize_decision(decision: &str) -> String {
    decision.trim().to_uppercase()
}

fn proposal_terminal(status: &str) -> bool {
    matches!(status, "COMPLETED" | "FAILED" | "REJECTED" | "EXPIRED")
}

fn upsert_mission_status(
    ctx: &ReducerContext,
    mission_id: String,
    status: String,
    updated_at: String,
    summary: Option<String>,
    error: Option<String>,
) -> Result<(), String> {
    let mut mission = ctx
        .db
        .missions()
        .mission_id()
        .find(mission_id)
        .ok_or("Mission not found")?;

    mission.status = status;
    mission.updated_at = updated_at;
    if summary.is_some() {
        mission.summary = summary;
    }
    if error.is_some() {
        mission.error = error;
    }
    ctx.db.missions().mission_id().update(mission);
    Ok(())
}

#[reducer]
pub fn claim_task(ctx: &ReducerContext, task_id: u64, worker_id: String) -> Result<(), String> {
    let mut task = ctx
        .db
        .organization_tasks()
        .id()
        .find(task_id)
        .ok_or("Task not found")?;

    if task.status != "pending" || task.assigned_worker.is_some() {
        return Err("Task already claimed by another worker.".into());
    }

    task.status = "claimed".to_string();
    task.assigned_worker = Some(worker_id);
    ctx.db.organization_tasks().id().update(task);
    Ok(())
}

#[reducer]
pub fn add_proposal(
    ctx: &ReducerContext,
    proposal_id: String,
    created_at: String,
    status: String,
    fingerprint: Option<String>,
    payload: String,
    payload_raw: Option<String>,
    mode: String,
    execution_targeting: Option<String>,
    assigned_node_id: Option<String>,
    hub_signature: Option<String>,
    assignment_expires_at: Option<String>,
    attempt_count: i64,
    proposal_hash: Option<String>,
    approved_at: Option<String>,
    expires_at: Option<String>,
    eligibility_snapshot: Option<String>,
    user_id: Option<String>,
    owner_id: Option<String>,
    principal_id: Option<String>,
) -> Result<(), String> {
    if ctx
        .db
        .proposals()
        .proposal_id()
        .find(proposal_id.clone())
        .is_some()
    {
        return Err("Proposal already exists".into());
    }

    ctx.db.proposals().insert(Proposal {
        proposal_id,
        created_at: if created_at.is_empty() {
            now_string(ctx)
        } else {
            created_at
        },
        status,
        fingerprint,
        payload,
        payload_raw,
        node_id: None,
        claimed_at: None,
        lease_expires_at: None,
        last_heartbeat_at: None,
        last_heartbeat_node_id: None,
        last_heartbeat_node_instance_id: None,
        last_heartbeat_detail: None,
        mode,
        execution_targeting,
        assigned_node_id,
        assignment_expires_at,
        attempt_count,
        proposal_hash,
        hub_signature,
        approved_at,
        expires_at,
        eligibility_snapshot,
        user_id: option_if_not_blank(user_id),
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
    });
    Ok(())
}

#[reducer]
pub fn assign_proposal(
    ctx: &ReducerContext,
    proposal_id: String,
    assigned_node_id: String,
    assignment_expires_at: String,
    hub_signature: String,
    proposal_hash: String,
    attempt_count: i64,
    eligibility_snapshot: Option<String>,
) -> Result<(), String> {
    let mut proposal = ctx
        .db
        .proposals()
        .proposal_id()
        .find(proposal_id)
        .ok_or("Proposal not found")?;

    if proposal_terminal(&proposal.status) {
        return Err("Proposal is terminal".into());
    }

    proposal.status = "ASSIGNED".to_string();
    proposal.assigned_node_id = Some(assigned_node_id);
    proposal.assignment_expires_at = Some(assignment_expires_at);
    proposal.hub_signature = Some(hub_signature);
    proposal.proposal_hash = Some(proposal_hash);
    proposal.attempt_count = attempt_count;
    proposal.eligibility_snapshot = eligibility_snapshot;
    ctx.db.proposals().proposal_id().update(proposal);
    Ok(())
}

#[reducer]
pub fn approve_proposal(
    ctx: &ReducerContext,
    proposal_id: String,
    approved_at: String,
    expires_at: Option<String>,
    proposal_hash: String,
) -> Result<(), String> {
    let mut proposal = ctx
        .db
        .proposals()
        .proposal_id()
        .find(proposal_id)
        .ok_or("Proposal not found")?;

    proposal.status = "APPROVED".to_string();
    proposal.approved_at = Some(approved_at);
    proposal.expires_at = expires_at;
    proposal.proposal_hash = Some(proposal_hash);
    ctx.db.proposals().proposal_id().update(proposal);
    Ok(())
}

#[reducer]
pub fn reject_proposal(ctx: &ReducerContext, proposal_id: String) -> Result<(), String> {
    let mut proposal = ctx
        .db
        .proposals()
        .proposal_id()
        .find(proposal_id)
        .ok_or("Proposal not found")?;

    proposal.status = "REJECTED".to_string();
    ctx.db.proposals().proposal_id().update(proposal);
    Ok(())
}

#[reducer]
pub fn queue_proposal(
    ctx: &ReducerContext,
    proposal_id: String,
    eligibility_snapshot: Option<String>,
) -> Result<(), String> {
    let mut proposal = ctx
        .db
        .proposals()
        .proposal_id()
        .find(proposal_id)
        .ok_or("Proposal not found")?;

    proposal.status = "QUEUED".to_string();
    proposal.assigned_node_id = None;
    proposal.assignment_expires_at = None;
    proposal.hub_signature = None;
    proposal.eligibility_snapshot = eligibility_snapshot;
    ctx.db.proposals().proposal_id().update(proposal);
    Ok(())
}

#[reducer]
pub fn expire_proposal(
    ctx: &ReducerContext,
    proposal_id: String,
    eligibility_snapshot: Option<String>,
) -> Result<(), String> {
    let mut proposal = ctx
        .db
        .proposals()
        .proposal_id()
        .find(proposal_id)
        .ok_or("Proposal not found")?;

    proposal.status = "EXPIRED".to_string();
    proposal.eligibility_snapshot = eligibility_snapshot;
    ctx.db.proposals().proposal_id().update(proposal);
    Ok(())
}

#[reducer]
pub fn claim_proposal(
    ctx: &ReducerContext,
    proposal_id: String,
    node_id: String,
    claimed_at: String,
    lease_expires_at: String,
) -> Result<(), String> {
    let mut proposal = ctx
        .db
        .proposals()
        .proposal_id()
        .find(proposal_id)
        .ok_or("Proposal not found")?;

    if proposal.status != "APPROVED" && proposal.status != "ASSIGNED" {
        return Err("Proposal is not claimable".into());
    }
    if let Some(assigned_node_id) = proposal.assigned_node_id.clone() {
        if assigned_node_id != node_id {
            return Err("Proposal assigned to a different node".into());
        }
    }

    proposal.status = "CLAIMED".to_string();
    proposal.node_id = Some(node_id);
    proposal.claimed_at = Some(claimed_at);
    proposal.lease_expires_at = Some(lease_expires_at);
    ctx.db.proposals().proposal_id().update(proposal);
    Ok(())
}

#[reducer]
pub fn requeue_proposal(ctx: &ReducerContext, proposal_id: String) -> Result<(), String> {
    let mut proposal = ctx
        .db
        .proposals()
        .proposal_id()
        .find(proposal_id)
        .ok_or("Proposal not found")?;

    proposal.status = "QUEUED".to_string();
    proposal.node_id = None;
    proposal.claimed_at = None;
    proposal.lease_expires_at = None;
    proposal.last_heartbeat_at = None;
    proposal.last_heartbeat_node_id = None;
    proposal.last_heartbeat_node_instance_id = None;
    proposal.last_heartbeat_detail = None;
    proposal.assigned_node_id = None;
    proposal.assignment_expires_at = None;
    proposal.hub_signature = None;
    ctx.db.proposals().proposal_id().update(proposal);
    Ok(())
}

#[reducer]
pub fn record_proposal_heartbeat(
    ctx: &ReducerContext,
    proposal_id: String,
    node_id: String,
    node_instance_id: String,
    heartbeat_at: String,
    detail: Option<String>,
) -> Result<(), String> {
    let mut proposal = ctx
        .db
        .proposals()
        .proposal_id()
        .find(proposal_id)
        .ok_or("Proposal not found")?;

    if proposal.status != "CLAIMED" {
        return Err("Proposal is not claimed".into());
    }
    if proposal.node_id.clone().unwrap_or_default() != node_id {
        return Err("Proposal claimed by a different node".into());
    }

    proposal.last_heartbeat_at = Some(heartbeat_at);
    proposal.last_heartbeat_node_id = Some(node_id);
    proposal.last_heartbeat_node_instance_id = Some(node_instance_id);
    proposal.last_heartbeat_detail = detail;
    ctx.db.proposals().proposal_id().update(proposal);
    Ok(())
}

#[reducer]
pub fn add_approval_request(
    ctx: &ReducerContext,
    request_id: String,
    proposal_id: String,
    status: String,
    requested_at: String,
    expires_at: Option<String>,
    requested_by: String,
    reason: Option<String>,
    payload_json: String,
) -> Result<(), String> {
    if let Some(mut request) = ctx
        .db
        .approval_requests()
        .request_id()
        .find(request_id.clone())
    {
        request.status = status;
        request.requested_at = requested_at;
        request.expires_at = expires_at;
        request.requested_by = requested_by;
        request.reason = reason;
        request.payload_json = payload_json;
        ctx.db.approval_requests().request_id().update(request);
    } else {
        ctx.db.approval_requests().insert(ApprovalRequest {
            request_id,
            proposal_id,
            status,
            requested_at,
            expires_at,
            requested_by,
            reason,
            payload_json,
        });
    }
    Ok(())
}

#[reducer]
pub fn record_approval_decision(
    ctx: &ReducerContext,
    decision_id: String,
    request_id: String,
    proposal_id: String,
    actor_type: String,
    actor_id: String,
    decision: String,
    reason: Option<String>,
    created_at: String,
    metadata_json: String,
) -> Result<(), String> {
    let normalized_decision = normalize_decision(&decision);
    if let Some(mut request) = ctx
        .db
        .approval_requests()
        .request_id()
        .find(request_id.clone())
    {
        request.status = if normalized_decision == "APPROVE" || normalized_decision == "APPROVED" {
            "APPROVED".to_string()
        } else {
            "REJECTED".to_string()
        };
        ctx.db.approval_requests().request_id().update(request);
    }

    let row = ApprovalDecision {
        decision_id: decision_id.clone(),
        request_id,
        proposal_id,
        actor_type,
        actor_id,
        decision: normalized_decision,
        reason,
        created_at,
        metadata_json,
    };

    if ctx
        .db
        .approval_decisions()
        .decision_id()
        .find(decision_id)
        .is_some()
    {
        ctx.db.approval_decisions().decision_id().update(row);
    } else {
        ctx.db.approval_decisions().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn upsert_pod(
    ctx: &ReducerContext,
    pod_id: String,
    host_identity: String,
    provider: String,
    runtime_capabilities: Vec<String>,
    trust_tier: String,
    privacy_floor: String,
    liveness: String,
    leaseable: bool,
    registered_at: String,
    last_heartbeat: String,
) -> Result<(), String> {
    let row = Pod {
        pod_id: pod_id.clone(),
        host_identity,
        provider,
        runtime_capabilities,
        trust_tier,
        privacy_floor,
        liveness,
        leaseable,
        registered_at,
        last_heartbeat,
    };
    if ctx.db.pods().pod_id().find(pod_id).is_some() {
        ctx.db.pods().pod_id().update(row);
    } else {
        ctx.db.pods().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn upsert_gpu_slot(
    ctx: &ReducerContext,
    slot_id: String,
    pod_id: String,
    gpu_type: String,
    vram_gb: i64,
    loaded_models: Vec<String>,
    available_slots: i64,
) -> Result<(), String> {
    let row = GpuSlot {
        slot_id: slot_id.clone(),
        pod_id,
        gpu_type,
        vram_gb,
        loaded_models,
        available_slots,
    };
    if ctx.db.gpu_slots().slot_id().find(slot_id).is_some() {
        ctx.db.gpu_slots().slot_id().update(row);
    } else {
        ctx.db.gpu_slots().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn update_pod_heartbeat(
    ctx: &ReducerContext,
    pod_id: String,
    last_heartbeat: String,
    liveness: String,
) -> Result<(), String> {
    if let Some(mut pod) = ctx.db.pods().pod_id().find(pod_id) {
        pod.last_heartbeat = last_heartbeat;
        pod.liveness = liveness;
        ctx.db.pods().pod_id().update(pod);
    }
    Ok(())
}

#[reducer]
pub fn record_consent(
    ctx: &ReducerContext,
    consent_id: String,
    proposal_id: String,
    proposal_hash: String,
    actor_type: String,
    actor_id: String,
    decision: String,
    comment: Option<String>,
    metadata: Option<String>,
    approval_request_id: Option<String>,
    requested_by: Option<String>,
    requested_at: Option<String>,
    request_expires_at: Option<String>,
    request_reason: Option<String>,
    request_payload_json: Option<String>,
    approved_at: Option<String>,
    expires_at: Option<String>,
) -> Result<(), String> {
    let mut proposal = ctx
        .db
        .proposals()
        .proposal_id()
        .find(proposal_id.clone())
        .ok_or("Proposal not found")?;

    let decision_normalized = normalize_decision(&decision);
    let now = now_string(ctx);
    let request_id = approval_request_id.unwrap_or_else(|| format!("APR-{}", proposal_id));
    let metadata_json = metadata.unwrap_or_else(|| "{}".to_string());

    if let Some(mut request) = ctx
        .db
        .approval_requests()
        .request_id()
        .find(request_id.clone())
    {
        request.status = if decision_normalized == "APPROVE" || decision_normalized == "APPROVED" {
            "APPROVED".to_string()
        } else {
            "REJECTED".to_string()
        };
        request.requested_at = requested_at.clone().unwrap_or(request.requested_at);
        request.expires_at = request_expires_at.clone().or(request.expires_at);
        request.requested_by = requested_by.clone().unwrap_or(request.requested_by);
        request.reason = request_reason.clone().or(request.reason);
        request.payload_json = request_payload_json.clone().unwrap_or(request.payload_json);
        ctx.db.approval_requests().request_id().update(request);
    } else {
        ctx.db.approval_requests().insert(ApprovalRequest {
            request_id: request_id.clone(),
            proposal_id: proposal_id.clone(),
            status: if decision_normalized == "APPROVE" || decision_normalized == "APPROVED" {
                "APPROVED".to_string()
            } else {
                "REJECTED".to_string()
            },
            requested_at: requested_at.unwrap_or_else(|| now.clone()),
            expires_at: request_expires_at,
            requested_by: requested_by.unwrap_or_else(|| "heiwa-hub".to_string()),
            reason: request_reason,
            payload_json: request_payload_json.unwrap_or_else(|| "{}".to_string()),
        });
    }

    ctx.db.proposal_consents().insert(ProposalConsent {
        consent_id: consent_id.clone(),
        proposal_id: proposal_id.clone(),
        proposal_hash: proposal_hash.clone(),
        actor_type: actor_type.clone(),
        actor_id: actor_id.clone(),
        decision: decision_normalized.clone(),
        comment: comment.clone(),
        created_at: now.clone(),
        metadata: Some(metadata_json.clone()),
    });

    let approval_decision = ApprovalDecision {
        decision_id: format!("DEC-{}", consent_id),
        request_id,
        proposal_id: proposal_id.clone(),
        actor_type,
        actor_id,
        decision: decision_normalized.clone(),
        reason: comment,
        created_at: now.clone(),
        metadata_json,
    };
    ctx.db.approval_decisions().insert(approval_decision);

    proposal.status = if decision_normalized == "APPROVE" || decision_normalized == "APPROVED" {
        "APPROVED".to_string()
    } else {
        "REJECTED".to_string()
    };
    proposal.proposal_hash = Some(proposal_hash);
    if proposal.status == "APPROVED" {
        proposal.approved_at = approved_at.or(Some(now));
        proposal.expires_at = expires_at;
    }
    ctx.db.proposals().proposal_id().update(proposal);
    Ok(())
}

#[reducer]
pub fn issue_capability_lease(
    ctx: &ReducerContext,
    lease_id: String,
    proposal_id: String,
    run_id: Option<String>,
    parent_lease_id: Option<String>,
    holder_kind: String,
    holder_id: String,
    tool_scope_json: String,
    network_scope_json: String,
    filesystem_scope_json: String,
    secret_scope_json: String,
    privilege_tier: String,
    failure_policy: String,
    chain_state: String,
    routing_lock_json: Option<String>,
    status: String,
    issued_at: String,
    expires_at: String,
    hub_signature: String,
    owner_id: Option<String>,
    principal_id: Option<String>,
) -> Result<(), String> {
    let row = CapabilityLease {
        lease_id: lease_id.clone(),
        proposal_id,
        run_id,
        holder_kind,
        holder_id,
        tool_scope_json,
        network_scope_json,
        filesystem_scope_json,
        secret_scope_json,
        privilege_tier,
        status,
        issued_at,
        renewed_at: None,
        expires_at,
        revoked_at: None,
        revocation_reason: None,
        hub_signature,
        parent_lease_id: option_if_not_blank(parent_lease_id),
        failure_policy: some_if_not_blank(failure_policy),
        chain_state: some_if_not_blank(chain_state),
        routing_lock_json: option_if_not_blank(routing_lock_json),
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
    };

    if ctx
        .db
        .capability_leases()
        .lease_id()
        .find(lease_id)
        .is_some()
    {
        ctx.db.capability_leases().lease_id().update(row);
    } else {
        ctx.db.capability_leases().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn renew_capability_lease(
    ctx: &ReducerContext,
    lease_id: String,
    renewed_at: String,
    expires_at: String,
) -> Result<(), String> {
    let mut lease = ctx
        .db
        .capability_leases()
        .lease_id()
        .find(lease_id)
        .ok_or("Capability lease not found")?;

    lease.status = "ACTIVE".to_string();
    lease.renewed_at = Some(renewed_at);
    lease.expires_at = expires_at;
    ctx.db.capability_leases().lease_id().update(lease);
    Ok(())
}

#[reducer]
pub fn revoke_capability_lease(
    ctx: &ReducerContext,
    lease_id: String,
    revoked_at: String,
    revocation_reason: Option<String>,
) -> Result<(), String> {
    let mut lease = ctx
        .db
        .capability_leases()
        .lease_id()
        .find(lease_id)
        .ok_or("Capability lease not found")?;

    lease.status = "REVOKED".to_string();
    lease.revoked_at = Some(revoked_at);
    lease.revocation_reason = revocation_reason;
    ctx.db.capability_leases().lease_id().update(lease);
    Ok(())
}

#[reducer]
pub fn revoke_capability_chain(
    ctx: &ReducerContext,
    lease_id: String,
    revoked_at: String,
    revocation_reason: Option<String>,
) -> Result<(), String> {
    let mut stack = vec![lease_id];
    let mut seen: Vec<String> = Vec::new();

    while let Some(current_id) = stack.pop() {
        if seen.iter().any(|seen_id| seen_id == &current_id) {
            continue;
        }
        seen.push(current_id.clone());

        let Some(mut lease) = ctx.db.capability_leases().lease_id().find(current_id.clone()) else {
            continue;
        };

        let failure_policy = lease
            .failure_policy
            .clone()
            .unwrap_or_else(|| "ABORT".to_string())
            .trim()
            .to_uppercase();
        let cascade_children = failure_policy != "CONTINUE";
        lease.status = "REVOKED".to_string();
        lease.revoked_at = Some(revoked_at.clone());
        lease.revocation_reason = revocation_reason.clone();
        lease.chain_state = Some(if failure_policy == "COMPENSATE" {
            "ROLLED_BACK".to_string()
        } else {
            "FAILED".to_string()
        });
        ctx.db.capability_leases().lease_id().update(lease);

        if !cascade_children {
            continue;
        }

        let child_ids: Vec<String> = ctx
            .db
            .capability_leases()
            .iter()
            .filter(|row| row.parent_lease_id.as_deref() == Some(current_id.as_str()))
            .map(|row| row.lease_id.clone())
            .collect();
        stack.extend(child_ids);
    }

    Ok(())
}

#[reducer]
pub fn record_route_decision(
    ctx: &ReducerContext,
    request_id: String,
    task_id: String,
    envelope_version: String,
    raw_text: String,
    source_surface: String,
    intent_class: String,
    risk_level: String,
    privacy_level: String,
    compute_class: i64,
    assigned_worker: String,
    target_tool: String,
    target_model: String,
    target_runtime: String,
    target_tier: String,
    requires_approval: bool,
    rationale: String,
    confidence: f64,
    gateway_transport: String,
    created_at: String,
    user_id: Option<String>,
    owner_id: Option<String>,
    principal_id: Option<String>,
    drex_decision_id: Option<String>,
) -> Result<(), String> {
    let row = RouteDecision {
        request_id: request_id.clone(),
        task_id,
        envelope_version,
        raw_text,
        source_surface,
        intent_class,
        risk_level,
        privacy_level,
        compute_class,
        assigned_worker,
        target_tool,
        target_model,
        target_runtime,
        target_tier,
        requires_approval,
        rationale,
        confidence,
        gateway_transport,
        created_at,
        drex_decision_id: option_if_not_blank(drex_decision_id),
        user_id: option_if_not_blank(user_id),
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
    };

    if ctx
        .db
        .route_decisions()
        .request_id()
        .find(request_id)
        .is_some()
    {
        ctx.db.route_decisions().request_id().update(row);
    } else {
        ctx.db.route_decisions().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn attach_drex_decision_to_route(
    ctx: &ReducerContext,
    request_id: String,
    drex_decision_id: String,
) -> Result<(), String> {
    let mut row = ctx
        .db
        .route_decisions()
        .request_id()
        .find(request_id.clone())
        .ok_or_else(|| format!("route decision not found for request_id={request_id}"))?;
    row.drex_decision_id = some_if_not_blank(drex_decision_id);
    ctx.db.route_decisions().request_id().update(row);
    Ok(())
}

#[reducer]
pub fn record_drex_decision(
    ctx: &ReducerContext,
    decision_id: String,
    request_id: String,
    task_id: String,
    active_tier: String,
    route_runtime: String,
    route_model: String,
    scope: f64,
    abstraction: f64,
    context_span: f64,
    execution_proximity: f64,
    blast_radius: f64,
    coordination_load: f64,
    latency_pressure: f64,
    macro_score: f64,
    meso_score: f64,
    micro_score: f64,
    score_confidence: f64,
    authority_required: String,
    requires_approval: bool,
    reasons_json: String,
    vector_json: String,
    scorecard_json: String,
    gate_json: String,
    policy_version: String,
    created_at_ms: u64,
) -> Result<(), String> {
    let row = DrexDecisionRow {
        decision_id: decision_id.clone(),
        request_id,
        task_id,
        active_tier,
        route_runtime,
        route_model,
        scope,
        abstraction,
        context_span,
        execution_proximity,
        blast_radius,
        coordination_load,
        latency_pressure,
        macro_score,
        meso_score,
        micro_score,
        score_confidence,
        authority_required,
        requires_approval,
        reasons_json,
        vector_json,
        scorecard_json,
        gate_json,
        policy_version,
        created_at_ms,
    };

    if ctx
        .db
        .drex_decisions()
        .decision_id()
        .find(decision_id)
        .is_some()
    {
        ctx.db.drex_decisions().decision_id().update(row);
    } else {
        ctx.db.drex_decisions().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn record_drex_failure(
    ctx: &ReducerContext,
    decision_id: String,
    request_id: String,
    failure_mode: String,
    stage: String,
    details_json: String,
    recovered: bool,
    created_at_ms: u64,
) -> Result<(), String> {
    ctx.db.drex_failures().insert(DrexFailureRow {
        id: 0,
        decision_id,
        request_id,
        failure_mode,
        stage,
        details_json,
        recovered,
        created_at_ms,
    });
    Ok(())
}

#[reducer]
pub fn record_run(
    ctx: &ReducerContext,
    run_id: String,
    user_id: String,
    proposal_id: String,
    lease_id: String,
    session_id: Option<String>,
    started_at: String,
    ended_at: String,
    status: String,
    chain_result_json: String,
    signals_json: String,
    artifact_index_json: String,
    node_id: String,
    replay_receipt_json: String,
    mode: String,
    model_id: String,
    tokens_input: i64,
    tokens_output: i64,
    tokens_total: i64,
    cost: f64,
    owner_id: Option<String>,
    principal_id: Option<String>,
    failure_code: Option<String>,
    failure_message: Option<String>,
) -> Result<(), String> {
    let row = RunRecord {
        run_id: run_id.clone(),
        proposal_id: proposal_id.clone(),
        started_at,
        ended_at,
        status: status.clone(),
        chain_result_json,
        signals_json,
        artifact_index_json,
        node_id,
        replay_receipt_json,
        mode,
        model_id,
        tokens_input,
        tokens_output,
        tokens_total,
        cost,
        user_id: some_if_not_blank(user_id),
        lease_id: some_if_not_blank(lease_id),
        session_id,
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
        failure_code,
        failure_message,
    };

    if ctx.db.runs().run_id().find(run_id).is_some() {
        ctx.db.runs().run_id().update(row);
    } else {
        ctx.db.runs().insert(row);
    }

    if let Some(mut proposal) = ctx.db.proposals().proposal_id().find(proposal_id) {
        let normalized_status = status.trim().to_uppercase();
        proposal.status = if matches!(normalized_status.as_str(), "PASS" | "SUCCESS" | "COMPLETED")
        {
            "COMPLETED".to_string()
        } else {
            "FAILED".to_string()
        };
        ctx.db.proposals().proposal_id().update(proposal);
    }

    Ok(())
}

#[reducer]
pub fn record_run_failure(
    ctx: &ReducerContext,
    failure_id: String,
    run_id: String,
    lease_id: String,
    session_id: String,
    failure_code: String,
    failure_message: String,
    failure_type: String,
    retryable: bool,
    details_json: String,
) -> Result<(), String> {
    ctx.db.run_failures().insert(RunFailure {
        failure_id,
        run_id,
        lease_id,
        session_id,
        failure_code,
        failure_message,
        failure_type,
        retryable,
        details_json,
        created_at: ctx.timestamp.to_string(),
    });
    Ok(())
}

#[reducer]
pub fn upsert_provider_account_status(
    ctx: &ReducerContext,
    account_id: String,
    provider_id: String,
    node_id: String,
    auth_kind: String,
    local_handle_ref: String,
    status: String,
    display_name: Option<String>,
    default_model: Option<String>,
    rate_group: String,
    available_models_json: String,
    last_validated_at: Option<String>,
    last_error: Option<String>,
    updated_at: String,
    user_id: Option<String>,
    owner_id: Option<String>,
    principal_id: Option<String>,
) -> Result<(), String> {
    let row = ProviderAccount {
        account_id: account_id.clone(),
        provider_id,
        node_id,
        auth_kind,
        local_handle_ref,
        status,
        display_name,
        default_model,
        rate_group,
        available_models_json,
        last_validated_at,
        last_error,
        updated_at,
        user_id: option_if_not_blank(user_id),
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
    };

    if ctx
        .db
        .provider_accounts()
        .account_id()
        .find(account_id)
        .is_some()
    {
        ctx.db.provider_accounts().account_id().update(row);
    } else {
        ctx.db.provider_accounts().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn create_mission(
    ctx: &ReducerContext,
    mission_id: String,
    created_at: String,
    updated_at: String,
    status: String,
    source_surface: String,
    node_id: String,
    prompt: String,
    intent_class: String,
    risk_level: String,
    active_step_id: Option<String>,
    active_cell_id: Option<String>,
    target_tool: Option<String>,
    target_model: Option<String>,
    summary: Option<String>,
    error: Option<String>,
    metadata_json: String,
    user_id: Option<String>,
    owner_id: Option<String>,
    principal_id: Option<String>,
) -> Result<(), String> {
    let row = MissionRecord {
        mission_id: mission_id.clone(),
        created_at,
        updated_at,
        status,
        source_surface,
        node_id,
        prompt,
        intent_class,
        risk_level,
        active_step_id,
        active_cell_id,
        target_tool,
        target_model,
        summary,
        error,
        metadata_json,
        user_id: option_if_not_blank(user_id),
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
        // Doctrine Phase 1 fields — defaulted to None for backward compat
        task_class: None,
        budget_class: None,
        treasury_id: None,
        novelty_policy_json: None,
        termination_policy_json: None,
        required_belief_query: None,
        allowed_tools_json: None,
        allowed_side_effects_json: None,
        target_outputs_json: None,
        cost_so_far_millicents: None,
        novelty_so_far: None,
        turn_count: None,
        approval_state: None,
    };

    if ctx.db.missions().mission_id().find(mission_id).is_some() {
        ctx.db.missions().mission_id().update(row);
    } else {
        ctx.db.missions().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn append_mission_step(
    ctx: &ReducerContext,
    step_id: String,
    mission_id: String,
    position: i64,
    status: String,
    step_kind: String,
    cell_role: String,
    title: String,
    detail: Option<String>,
    input_json: String,
    output_json: String,
    created_at: String,
    updated_at: String,
    owner_id: Option<String>,
    principal_id: Option<String>,
) -> Result<(), String> {
    let step_id_for_mission = step_id.clone();
    let row = MissionStepRecord {
        step_id: step_id.clone(),
        mission_id: mission_id.clone(),
        position,
        status,
        step_kind,
        cell_role,
        title,
        detail,
        input_json,
        output_json,
        created_at,
        updated_at: updated_at.clone(),
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
    };

    if ctx.db.mission_steps().step_id().find(step_id).is_some() {
        ctx.db.mission_steps().step_id().update(row);
    } else {
        ctx.db.mission_steps().insert(row);
    }

    if let Some(mut mission) = ctx.db.missions().mission_id().find(mission_id) {
        mission.active_step_id = Some(step_id_for_mission);
        mission.updated_at = updated_at;
        ctx.db.missions().mission_id().update(mission);
    }
    Ok(())
}

#[reducer]
pub fn append_event(
    ctx: &ReducerContext,
    event_id: String,
    owner_id: Option<String>,
    principal_id: Option<String>,
    session_id: Option<String>,
    mission_id: Option<String>,
    battlefield_id: Option<String>,
    event_type: String,
    payload_json: String,
    created_at: String,
) -> Result<(), String> {
    if ctx.db.events().event_id().find(event_id.clone()).is_some() {
        return Err("Event already exists".into());
    }
    ctx.db.events().insert(EventRecord {
        event_id,
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
        session_id: option_if_not_blank(session_id),
        mission_id: option_if_not_blank(mission_id),
        battlefield_id: option_if_not_blank(battlefield_id),
        event_type,
        payload_json,
        created_at: if created_at.is_empty() {
            now_string(ctx)
        } else {
            created_at
        },
    });
    Ok(())
}

#[reducer]
pub fn upsert_battlefield(
    ctx: &ReducerContext,
    battlefield_id: String,
    owner_id: Option<String>,
    principal_id: Option<String>,
    name: String,
    repo_url: Option<String>,
    root_path: Option<String>,
    node_id: Option<String>,
    status: String,
    created_at: Option<String>,
    last_active_at: Option<String>,
) -> Result<(), String> {
    let now = now_string(ctx);
    let row = BattlefieldRecord {
        battlefield_id: battlefield_id.clone(),
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
        name,
        repo_url: option_if_not_blank(repo_url),
        root_path: option_if_not_blank(root_path),
        node_id: option_if_not_blank(node_id),
        status,
        created_at: created_at
            .and_then(some_if_not_blank)
            .unwrap_or_else(|| now.clone()),
        last_active_at: last_active_at
            .and_then(some_if_not_blank)
            .unwrap_or_else(|| now.clone()),
    };

    if ctx
        .db
        .battlefields()
        .battlefield_id()
        .find(battlefield_id)
        .is_some()
    {
        ctx.db.battlefields().battlefield_id().update(row);
    } else {
        ctx.db.battlefields().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn archive_battlefield(ctx: &ReducerContext, battlefield_id: String) -> Result<(), String> {
    let mut row = ctx
        .db
        .battlefields()
        .battlefield_id()
        .find(battlefield_id)
        .ok_or_else(|| "Battlefield not found".to_string())?;
    row.status = "archived".into();
    row.last_active_at = now_string(ctx);
    ctx.db.battlefields().battlefield_id().update(row);
    Ok(())
}

#[reducer]
pub fn start_cell_run(
    ctx: &ReducerContext,
    cell_run_id: String,
    mission_id: String,
    step_id: Option<String>,
    status: String,
    cell_id: String,
    cell_role: String,
    provider: String,
    model: String,
    rate_group: String,
    tool: String,
    started_at: String,
    tokens_input: i64,
    tokens_output: i64,
    tokens_total: i64,
    output_summary: Option<String>,
    user_id: Option<String>,
    owner_id: Option<String>,
    principal_id: Option<String>,
) -> Result<(), String> {
    let row = CellRunRecord {
        cell_run_id: cell_run_id.clone(),
        mission_id: mission_id.clone(),
        step_id: step_id.clone(),
        status,
        cell_id: cell_id.clone(),
        cell_role,
        provider,
        model,
        rate_group,
        tool,
        started_at: started_at.clone(),
        ended_at: None,
        tokens_input,
        tokens_output,
        tokens_total,
        output_summary,
        user_id: option_if_not_blank(user_id),
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
    };

    if ctx.db.cell_runs().cell_run_id().find(cell_run_id).is_some() {
        ctx.db.cell_runs().cell_run_id().update(row);
    } else {
        ctx.db.cell_runs().insert(row);
    }

    if let Some(mut mission) = ctx.db.missions().mission_id().find(mission_id) {
        mission.status = "running".to_string();
        mission.active_step_id = step_id;
        mission.active_cell_id = Some(cell_id);
        mission.updated_at = started_at;
        ctx.db.missions().mission_id().update(mission);
    }
    Ok(())
}

#[reducer]
pub fn finish_cell_run(
    ctx: &ReducerContext,
    cell_run_id: String,
    status: String,
    ended_at: String,
    tokens_input: i64,
    tokens_output: i64,
    tokens_total: i64,
    output_summary: Option<String>,
) -> Result<(), String> {
    let mut row = ctx
        .db
        .cell_runs()
        .cell_run_id()
        .find(cell_run_id)
        .ok_or("Cell run not found")?;

    row.status = status;
    row.ended_at = Some(ended_at);
    row.tokens_input = tokens_input;
    row.tokens_output = tokens_output;
    row.tokens_total = tokens_total;
    if output_summary.is_some() {
        row.output_summary = output_summary;
    }
    ctx.db.cell_runs().cell_run_id().update(row);
    Ok(())
}

#[reducer]
pub fn pause_mission(
    ctx: &ReducerContext,
    mission_id: String,
    updated_at: String,
    summary: Option<String>,
) -> Result<(), String> {
    upsert_mission_status(ctx, mission_id, "paused".to_string(), updated_at, summary, None)
}

#[reducer]
pub fn resume_mission(
    ctx: &ReducerContext,
    mission_id: String,
    updated_at: String,
    summary: Option<String>,
) -> Result<(), String> {
    upsert_mission_status(ctx, mission_id, "running".to_string(), updated_at, summary, None)
}

#[reducer]
pub fn complete_mission(
    ctx: &ReducerContext,
    mission_id: String,
    updated_at: String,
    summary: Option<String>,
) -> Result<(), String> {
    upsert_mission_status(ctx, mission_id, "completed".to_string(), updated_at, summary, None)
}

#[reducer]
pub fn fail_mission(
    ctx: &ReducerContext,
    mission_id: String,
    updated_at: String,
    error: Option<String>,
) -> Result<(), String> {
    upsert_mission_status(ctx, mission_id, "failed".to_string(), updated_at, None, error)
}

#[reducer]
pub fn write_session_summary(
    ctx: &ReducerContext,
    summary_id: String,
    session_id: String,
    node_id: String,
    created_at: String,
    summary_text: String,
    metadata_json: String,
    user_id: Option<String>,
    owner_id: Option<String>,
    principal_id: Option<String>,
) -> Result<(), String> {
    let row = SessionSummaryRecord {
        summary_id: summary_id.clone(),
        session_id,
        node_id,
        created_at,
        summary_text,
        metadata_json,
        user_id: option_if_not_blank(user_id),
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
    };

    if ctx
        .db
        .session_summaries()
        .summary_id()
        .find(summary_id)
        .is_some()
    {
        ctx.db.session_summaries().summary_id().update(row);
    } else {
        ctx.db.session_summaries().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn register_artifact(
    ctx: &ReducerContext,
    artifact_id: String,
    lease_id: Option<String>,
    run_id: Option<String>,
    session_id: Option<String>,
    user_id: String,
    mission_id: String,
    cell_run_id: Option<String>,
    artifact_type: String,
    title: String,
    uri: Option<String>,
    path: Option<String>,
    content_json: String,
    created_at: String,
    owner_id: Option<String>,
    principal_id: Option<String>,
) -> Result<(), String> {
    let row = ArtifactRecord {
        artifact_id: artifact_id.clone(),
        mission_id,
        run_id: option_if_not_blank(run_id),
        cell_run_id,
        artifact_type,
        title,
        uri,
        path,
        content_json,
        created_at,
        lease_id: option_if_not_blank(lease_id),
        session_id,
        user_id: some_if_not_blank(user_id),
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
    };

    if ctx.db.artifacts().artifact_id().find(artifact_id).is_some() {
        ctx.db.artifacts().artifact_id().update(row);
    } else {
        ctx.db.artifacts().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn upsert_node_heartbeat(
    ctx: &ReducerContext,
    node_id: String,
    last_heartbeat_at: String,
    last_seen_at: String,
    meta_json: String,
    capabilities_json: String,
    agent_version: String,
    tags_json: String,
    max_concurrency: i64,
    vram_mb: i64,
    locality: String,
    trust_tier: i32,
    provider_keys_json: String,
    model_inventory_json: String,
) -> Result<(), String> {
    if let Some(mut node) = ctx.db.nodes().node_id().find(node_id.clone()) {
        node.last_heartbeat_at = last_heartbeat_at;
        node.last_seen_at = last_seen_at;
        node.status = "ONLINE".to_string();
        node.meta_json = meta_json;
        node.capabilities_json = capabilities_json;
        node.agent_version = agent_version;
        node.tags_json = tags_json;
        node.max_concurrency = max_concurrency;
        node.vram_mb = vram_mb;
        node.locality = locality;
        node.trust_tier = trust_tier;
        node.provider_keys_json = provider_keys_json;
        node.model_inventory_json = model_inventory_json;
        ctx.db.nodes().node_id().update(node);
    } else {
        ctx.db.nodes().insert(NodeStatus {
            node_id,
            last_heartbeat_at: last_heartbeat_at.clone(),
            first_seen_at: last_seen_at.clone(),
            last_seen_at,
            status: "ONLINE".to_string(),
            meta_json,
            capabilities_json,
            agent_version,
            tags_json,
            max_concurrency,
            vram_mb,
            locality,
            trust_tier,
            provider_keys_json,
            model_inventory_json,
        });
    }
    Ok(())
}

#[reducer]
pub fn set_node_status(
    ctx: &ReducerContext,
    node_id: String,
    status: String,
) -> Result<(), String> {
    let mut node = ctx
        .db
        .nodes()
        .node_id()
        .find(node_id)
        .ok_or("Node not found")?;
    node.status = status;
    ctx.db.nodes().node_id().update(node);
    Ok(())
}

#[reducer]
pub fn upsert_liveness_state(
    ctx: &ReducerContext,
    key: String,
    last_state: String,
    last_changed_at: String,
) -> Result<(), String> {
    if let Some(mut row) = ctx.db.liveness_state().key().find(key.clone()) {
        row.last_state = last_state;
        row.last_changed_at = last_changed_at;
        ctx.db.liveness_state().key().update(row);
    } else {
        ctx.db.liveness_state().insert(LivenessState {
            key,
            last_state,
            last_changed_at,
        });
    }
    Ok(())
}

#[reducer]
pub fn upsert_discord_user(
    ctx: &ReducerContext,
    user_id: u64,
    username: String,
    trust_score: f32,
) -> Result<(), String> {
    let now = (ctx.timestamp.to_micros_since_unix_epoch() / 1_000_000) as u64;
    if let Some(mut user) = ctx.db.discord_users().user_id().find(user_id) {
        user.username = username;
        user.trust_score = trust_score;
        user.last_seen_at = now;
        ctx.db.discord_users().user_id().update(user);
    } else {
        ctx.db.discord_users().insert(DiscordUser {
            user_id,
            username,
            trust_score,
            last_seen_at: now,
        });
    }
    Ok(())
}

#[reducer]
pub fn register_discord_channel(
    ctx: &ReducerContext,
    channel_id: u64,
    name: String,
    purpose: String,
    metadata: String,
) -> Result<(), String> {
    if let Some(mut channel) = ctx.db.discord_channels().channel_id().find(channel_id) {
        channel.name = name;
        channel.purpose = purpose;
        channel.metadata_json = metadata;
        ctx.db.discord_channels().channel_id().update(channel);
    } else {
        ctx.db.discord_channels().insert(DiscordChannel {
            channel_id,
            name,
            purpose,
            metadata_json: metadata,
        });
    }
    Ok(())
}

#[reducer]
pub fn record_interaction(
    ctx: &ReducerContext,
    user_id: u64,
    channel_id: u64,
    intent: String,
) -> Result<(), String> {
    ctx.db.discord_interactions().insert(DiscordInteraction {
        id: 0,
        user_id,
        channel_id,
        intent_class: intent,
        timestamp: (ctx.timestamp.to_micros_since_unix_epoch() / 1_000_000) as u64,
    });
    Ok(())
}

#[table(accessor = model_tiers, public)]
pub struct ModelTier {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    #[unique]
    pub model_id: String,           // "ollama/qwen3.5:4b"
    pub provider_model_id: String,  // "qwen3.5:4b" (actual API string)
    pub provider: String,           // "ollama"
    pub rate_group: String,         // "local_ollama"
    pub capability_class: u8,       // 1=light, 2=medium, 3=heavy
    pub effort_knob: String,        // "thinking:off", "effort:medium", "reasoning:xhigh"
    pub effort_level: u8,           // normalized 1-5
    pub cost_per_turn: f64,         // 0.0 for local/free
    pub max_context_tokens: u32,
    pub vram_requirement_mb: u32,
    pub quantization_type: String,
    pub kv_cache_strategy: String,
    pub strengths_json: String,     // JSON array: ["code_generation", "research"]
    pub enabled: bool,
    pub last_success_rate: f64,     // rolling 20-execution window
    pub avg_latency_ms: u64,
    pub latency_p95_ms: u64,
    pub updated_at: String,         // ISO 8601
}

#[table(accessor = devices, public)]
pub struct Device {
    #[primary_key]
    pub device_id: String,
    #[index(btree)]
    pub user_id: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub last_seen_at: String,
    pub status: String, // "ONLINE", "OFFLINE"
}

#[table(accessor = platform_entitlements, public)]
pub struct PlatformEntitlement {
    #[primary_key]
    pub user_id: String,
    pub tier: String, // "FREE", "PRO", "ENTERPRISE"
    pub concurrency_limit: i64,
    pub monthly_token_budget: i64,
    pub features_json: String, // ["advanced_routing", "priority_support"]
}

#[table(accessor = loop_sessions, public)]
pub struct LoopSession {
    #[primary_key]
    pub loop_id: String,
    #[index(btree)]
    pub user_id: String,
    pub objective: String,
    pub status: String, // "RUNNING", "COMPLETED", "FAILED", "CANCELLED"
    pub max_turns: u32,
    pub max_cost_usd: f64,
    pub current_turn: u32,
    pub total_cost_usd: f64,
    pub termination_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[table(accessor = loop_iterations, public)]
pub struct LoopIteration {
    #[primary_key]
    pub iteration_id: String,
    #[index(btree)]
    pub loop_id: String,
    pub turn_number: u32,
    pub input_summary: String,
    pub output_summary: String,
    pub score: f64, // 0.0 to 1.0 (success/completion probability)
    pub run_id: Option<String>, // Link to RunRecord
    pub created_at: String,
}

#[reducer]
pub fn upsert_model_tier(
    ctx: &ReducerContext,
    model_id: String,
    provider_model_id: String,
    provider: String,
    rate_group: String,
    capability_class: u8,
    effort_knob: String,
    effort_level: u8,
    cost_per_turn: f64,
    max_context_tokens: u32,
    vram_requirement_mb: u32,
    quantization_type: String,
    kv_cache_strategy: String,
    strengths_json: String,
    enabled: bool,
) -> Result<(), String> {
    let now = ctx.timestamp.to_string();
    if let Some(mut existing) = ctx.db.model_tiers().model_id().find(&model_id) {
        existing.provider_model_id = provider_model_id;
        existing.provider = provider;
        existing.rate_group = rate_group;
        existing.capability_class = capability_class;
        existing.effort_knob = effort_knob;
        existing.effort_level = effort_level;
        existing.cost_per_turn = cost_per_turn;
        existing.max_context_tokens = max_context_tokens;
        existing.vram_requirement_mb = vram_requirement_mb;
        existing.quantization_type = quantization_type;
        existing.kv_cache_strategy = kv_cache_strategy;
        existing.strengths_json = strengths_json;
        existing.enabled = enabled;
        existing.updated_at = now;
        ctx.db.model_tiers().id().update(existing);
    } else {
        ctx.db.model_tiers().insert(ModelTier {
            id: 0, // auto_inc
            model_id,
            provider_model_id,
            provider,
            rate_group,
            capability_class,
            effort_knob,
            effort_level,
            cost_per_turn,
            max_context_tokens,
            vram_requirement_mb,
            quantization_type,
            kv_cache_strategy,
            strengths_json,
            enabled,
            last_success_rate: 1.0,
            avg_latency_ms: 0,
            latency_p95_ms: 0,
            updated_at: now,
        });
    }
    Ok(())
}

#[reducer]
pub fn register_device(
    ctx: &ReducerContext,
    device_id: String,
    user_id: String,
    hostname: String,
    os: String,
    arch: String,
) -> Result<(), String> {
    let now = ctx.timestamp.to_string();
    let row = Device {
        device_id: device_id.clone(),
        user_id,
        hostname,
        os,
        arch,
        last_seen_at: now,
        status: "ONLINE".to_string(),
    };
    if ctx.db.devices().device_id().find(&device_id).is_some() {
        ctx.db.devices().device_id().update(row);
    } else {
        ctx.db.devices().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn update_device_heartbeat(ctx: &ReducerContext, device_id: String) -> Result<(), String> {
    if let Some(mut device) = ctx.db.devices().device_id().find(&device_id) {
        device.last_seen_at = ctx.timestamp.to_string();
        device.status = "ONLINE".to_string();
        ctx.db.devices().device_id().update(device);
    }
    Ok(())
}

#[reducer]
pub fn upsert_platform_entitlement(
    ctx: &ReducerContext,
    user_id: String,
    tier: String,
    concurrency_limit: i64,
    monthly_token_budget: i64,
    features_json: String,
) -> Result<(), String> {
    let row = PlatformEntitlement {
        user_id: user_id.clone(),
        tier,
        concurrency_limit,
        monthly_token_budget,
        features_json,
    };
    if ctx
        .db
        .platform_entitlements()
        .user_id()
        .find(&user_id)
        .is_some()
    {
        ctx.db.platform_entitlements().user_id().update(row);
    } else {
        ctx.db.platform_entitlements().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn start_loop_session(
    ctx: &ReducerContext,
    loop_id: String,
    user_id: String,
    objective: String,
    max_turns: u32,
    max_cost_usd: f64,
) -> Result<(), String> {
    let now = ctx.timestamp.to_string();
    ctx.db.loop_sessions().insert(LoopSession {
        loop_id,
        user_id,
        objective,
        status: "RUNNING".to_string(),
        max_turns,
        max_cost_usd,
        current_turn: 0,
        total_cost_usd: 0.0,
        termination_reason: None,
        created_at: now.clone(),
        updated_at: now,
    });
    Ok(())
}

#[reducer]
pub fn record_loop_iteration(
    ctx: &ReducerContext,
    iteration_id: String,
    loop_id: String,
    turn_number: u32,
    input_summary: String,
    output_summary: String,
    score: f64,
    run_id: Option<String>,
    cost_increment: f64,
) -> Result<(), String> {
    let now = ctx.timestamp.to_string();
    ctx.db.loop_iterations().insert(LoopIteration {
        iteration_id,
        loop_id: loop_id.clone(),
        turn_number,
        input_summary,
        output_summary,
        score,
        run_id,
        created_at: now.clone(),
    });

    if let Some(mut session) = ctx.db.loop_sessions().loop_id().find(&loop_id) {
        session.current_turn = turn_number;
        session.total_cost_usd += cost_increment;
        session.updated_at = now;
        ctx.db.loop_sessions().loop_id().update(session);
    }
    Ok(())
}

#[reducer]
pub fn complete_loop_session(
    ctx: &ReducerContext,
    loop_id: String,
    status: String, // "COMPLETED", "FAILED", "CANCELLED"
    termination_reason: String,
) -> Result<(), String> {
    if let Some(mut session) = ctx.db.loop_sessions().loop_id().find(&loop_id) {
        session.status = status;
        session.termination_reason = Some(termination_reason);
        session.updated_at = ctx.timestamp.to_string();
        ctx.db.loop_sessions().loop_id().update(session);
    }
    Ok(())
}

#[reducer]
pub fn update_model_tier_stats(
    ctx: &ReducerContext,
    model_id: String,
    success_rate: f64,
    avg_latency_ms: u64,
    latency_p95_ms: u64,
) -> Result<(), String> {
    if let Some(mut tier) = ctx.db.model_tiers().model_id().find(&model_id) {
        tier.last_success_rate = success_rate;
        tier.avg_latency_ms = avg_latency_ms;
        tier.latency_p95_ms = latency_p95_ms;
        tier.updated_at = ctx.timestamp.to_string();
        ctx.db.model_tiers().id().update(tier);
    }
    Ok(())
}

#[table(accessor = task_dispatches, public)]
pub struct TaskDispatch {
    #[primary_key]
    pub task_id: String,
    pub parent_task_id: Option<String>, // None for top-level
    pub intent_class: String,
    pub risk_level: String,
    pub assigned_model: String,     // FK to model_tiers.model_id
    pub effort_knob: String,
    pub assigned_cell: String,
    pub budget_max_turns: u8,
    pub budget_max_seconds: u32,
    pub fallback_model: String,
    pub sandbox_mode: String,       // "trusted" | "e2b"
    pub tools_allowed_json: String, // JSON array
    pub context_files_json: String, // JSON array
    #[index(btree)]
    pub status: String,             // "queued"|"running"|"complete"|"failed"|"budget_exceeded"
    pub result_summary: String,
    pub tokens_used: u32,
    pub latency_ms: u64,
    pub created_at: String,
    pub completed_at: String,
}

#[reducer]
pub fn create_task_dispatch(
    ctx: &ReducerContext,
    task_id: String,
    parent_task_id: Option<String>,
    intent_class: String,
    risk_level: String,
    assigned_model: String,
    effort_knob: String,
    assigned_cell: String,
    budget_max_turns: u8,
    budget_max_seconds: u32,
    fallback_model: String,
    sandbox_mode: String,
    tools_allowed_json: String,
    context_files_json: String,
) -> Result<(), String> {
    ctx.db.task_dispatches().insert(TaskDispatch {
        task_id,
        parent_task_id,
        intent_class,
        risk_level,
        assigned_model,
        effort_knob,
        assigned_cell,
        budget_max_turns,
        budget_max_seconds,
        fallback_model,
        sandbox_mode,
        tools_allowed_json,
        context_files_json,
        status: "queued".to_string(),
        result_summary: String::new(),
        tokens_used: 0,
        latency_ms: 0,
        created_at: ctx.timestamp.to_string(),
        completed_at: String::new(),
    });
    Ok(())
}

#[reducer]
pub fn update_task_dispatch_status(
    ctx: &ReducerContext,
    task_id: String,
    status: String,
    result_summary: String,
    tokens_used: u32,
    latency_ms: u64,
) -> Result<(), String> {
    if let Some(mut dispatch) = ctx.db.task_dispatches().task_id().find(&task_id) {
        dispatch.status = status;
        dispatch.result_summary = result_summary;
        dispatch.tokens_used = tokens_used;
        dispatch.latency_ms = latency_ms;
        dispatch.completed_at = ctx.timestamp.to_string();
        ctx.db.task_dispatches().task_id().update(dispatch);
    }
    Ok(())
}

// worker_sessions: active | expired | closed
#[table(accessor = worker_sessions, public)]
#[derive(Clone)]
pub struct WorkerSession {
    #[primary_key]
    pub session_id: String,
    #[index(btree)]
    pub node_id: String,
    #[index(btree)]
    pub instance_id: String,
    pub runtime: String,
    pub runtime_version: String,
    pub worker_version: String,
    pub protocol: String,
    pub capabilities_json: String,
    pub metadata_json: String,
    pub max_concurrency: i64,
    pub active_tasks: u32,
    #[index(btree)]
    pub status: String,
    pub load: f64,
    #[index(btree)]
    pub created_at: String,
    #[index(btree)]
    pub updated_at: String,
    #[index(btree)]
    pub expires_at: String,
    #[index(btree)]
    pub last_seen_at: String,
    #[default(None::<String>)]
    #[index(btree)]
    pub closed_at: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub current_task_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub lease_id: Option<String>,
}

// leases: issued | acked | running | completed | failed | expired | revoked
#[table(accessor = leases, public)]
#[derive(Clone)]
pub struct Lease {
    #[primary_key]
    pub lease_id: String,
    #[index(btree)]
    pub task_id: String,
    #[index(btree)]
    pub session_id: String,
    #[index(btree)]
    pub node_id: String,
    #[index(btree)]
    pub capability: String,
    #[index(btree)]
    pub status: String,
    pub issued_at: String,
    pub updated_at: String,
    #[index(btree)]
    pub expires_at: String,
    #[default(None::<String>)]
    #[index(btree)]
    pub acked_at: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub completed_at: Option<String>,
    #[default(None::<String>)]
    pub failure_code: Option<String>,
    #[default(None::<String>)]
    pub reason: Option<String>,
}

// dispatch_acks: accepted | rejected
#[table(accessor = dispatch_acks, public)]
#[derive(Clone)]
pub struct DispatchAck {
    #[primary_key]
    pub ack_id: String,
    #[index(btree)]
    pub lease_id: String,
    #[index(btree)]
    pub session_id: String,
    #[index(btree)]
    pub task_id: String,
    #[index(btree)]
    pub node_id: String,
    #[index(btree)]
    pub status: String,
    #[index(btree)]
    pub decided_at: String,
    #[default(None::<String>)]
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerSessionStatus {
    Active,
    Expired,
    Closed,
}

impl WorkerSessionStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Closed => "closed",
        }
    }

    fn parse(status: &str) -> Result<Self, String> {
        match status.trim().to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "expired" => Ok(Self::Expired),
            "closed" => Ok(Self::Closed),
            other => Err(format!("invalid worker session status: {other}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeaseStatus {
    Issued,
    Acked,
    Running,
    Completed,
    Failed,
    Expired,
    Revoked,
}

impl LeaseStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Issued => "issued",
            Self::Acked => "acked",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
        }
    }

    fn parse(status: &str) -> Result<Self, String> {
        match status.trim().to_lowercase().as_str() {
            "issued" => Ok(Self::Issued),
            "acked" => Ok(Self::Acked),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "expired" => Ok(Self::Expired),
            "revoked" => Ok(Self::Revoked),
            other => Err(format!("invalid lease status: {other}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DispatchAckStatus {
    Accepted,
    Rejected,
}

impl DispatchAckStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }

    fn parse(status: &str) -> Result<Self, String> {
        match status.trim().to_lowercase().as_str() {
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            other => Err(format!("invalid dispatch ack status: {other}")),
        }
    }
}

fn upsert_worker_session_row(ctx: &ReducerContext, row: WorkerSession) {
    if ctx
        .db
        .worker_sessions()
        .session_id()
        .find(row.session_id.clone())
        .is_some()
    {
        ctx.db.worker_sessions().session_id().update(row);
    } else {
        ctx.db.worker_sessions().insert(row);
    }
}

fn load_worker_session(ctx: &ReducerContext, session_id: &str) -> Result<WorkerSession, String> {
    ctx.db
        .worker_sessions()
        .session_id()
        .find(session_id.to_string())
        .ok_or_else(|| format!("worker session not found: {session_id}"))
}

fn upsert_lease_row(ctx: &ReducerContext, row: Lease) {
    if ctx.db.leases().lease_id().find(row.lease_id.clone()).is_some() {
        ctx.db.leases().lease_id().update(row);
    } else {
        ctx.db.leases().insert(row);
    }
}

fn upsert_dispatch_ack_row(ctx: &ReducerContext, row: DispatchAck) {
    if ctx
        .db
        .dispatch_acks()
        .ack_id()
        .find(row.ack_id.clone())
        .is_some()
    {
        ctx.db.dispatch_acks().ack_id().update(row);
    } else {
        ctx.db.dispatch_acks().insert(row);
    }
}

#[reducer]
pub fn upsert_session(
    ctx: &ReducerContext,
    session_id: String,
    node_id: String,
    instance_id: String,
    runtime: String,
    runtime_version: String,
    worker_version: String,
    protocol: String,
    capabilities_json: String,
    metadata_json: String,
    max_concurrency: i64,
    active_tasks: u32,
    status: String,
    load: f64,
    created_at: String,
    updated_at: String,
    expires_at: String,
    last_seen_at: String,
    closed_at: Option<String>,
    current_task_id: Option<String>,
    lease_id: Option<String>,
) -> Result<(), String> {
    let _ = WorkerSessionStatus::parse(&status)?;
    if session_id.trim().is_empty() {
        return Err("session_id is required".to_string());
    }
    if node_id.trim().is_empty() {
        return Err("node_id is required".to_string());
    }
    if expires_at.trim().is_empty() {
        return Err("expires_at is required".to_string());
    }

    upsert_worker_session_row(
        ctx,
        WorkerSession {
            session_id,
            node_id,
            instance_id,
            runtime,
            runtime_version,
            worker_version,
            protocol,
            capabilities_json,
            metadata_json,
            max_concurrency: max_concurrency.max(1),
            active_tasks,
            status,
            load,
            created_at,
            updated_at,
            expires_at,
            last_seen_at,
            closed_at,
            current_task_id,
            lease_id,
        },
    );
    Ok(())
}

#[reducer]
pub fn close_session(ctx: &ReducerContext, session_id: String) -> Result<(), String> {
    let mut session = load_worker_session(ctx, &session_id)?;
    let now = now_string(ctx);
    session.status = WorkerSessionStatus::Closed.as_str().to_string();
    session.active_tasks = 0;
    session.load = 0.0;
    session.updated_at = now.clone();
    session.last_seen_at = now.clone();
    session.closed_at = Some(now);
    session.current_task_id = None;
    session.lease_id = None;
    upsert_worker_session_row(ctx, session);
    Ok(())
}

#[reducer]
pub fn upsert_lease(
    ctx: &ReducerContext,
    lease_id: String,
    task_id: String,
    session_id: String,
    node_id: String,
    capability: String,
    status: String,
    issued_at: String,
    updated_at: String,
    expires_at: String,
    acked_at: Option<String>,
    completed_at: Option<String>,
    failure_code: Option<String>,
    reason: Option<String>,
) -> Result<(), String> {
    let _ = LeaseStatus::parse(&status)?;
    if lease_id.trim().is_empty() {
        return Err("lease_id is required".to_string());
    }
    if task_id.trim().is_empty() {
        return Err("task_id is required".to_string());
    }
    if session_id.trim().is_empty() {
        return Err("session_id is required".to_string());
    }
    if node_id.trim().is_empty() {
        return Err("node_id is required".to_string());
    }
    if capability.trim().is_empty() {
        return Err("capability is required".to_string());
    }

    upsert_lease_row(
        ctx,
        Lease {
            lease_id,
            task_id,
            session_id,
            node_id,
            capability,
            status,
            issued_at,
            updated_at,
            expires_at,
            acked_at,
            completed_at,
            failure_code,
            reason,
        },
    );
    Ok(())
}

#[reducer]
pub fn record_dispatch_ack(
    ctx: &ReducerContext,
    ack_id: String,
    lease_id: String,
    session_id: String,
    task_id: String,
    node_id: String,
    status: String,
    decided_at: String,
    detail: Option<String>,
) -> Result<(), String> {
    let ack_status = DispatchAckStatus::parse(&status)?;
    let mut lease = ctx
        .db
        .leases()
        .lease_id()
        .find(lease_id.clone())
        .ok_or_else(|| format!("lease not found: {lease_id}"))?;
    if lease.session_id != session_id || lease.task_id != task_id {
        return Err(format!(
            "dispatch ack/session mismatch for lease {}",
            lease.lease_id
        ));
    }

    lease.status = match ack_status {
        DispatchAckStatus::Accepted => LeaseStatus::Acked.as_str().to_string(),
        DispatchAckStatus::Rejected => LeaseStatus::Failed.as_str().to_string(),
    };
    lease.updated_at = decided_at.clone();
    lease.acked_at = if ack_status == DispatchAckStatus::Accepted {
        Some(decided_at.clone())
    } else {
        None
    };
    if ack_status == DispatchAckStatus::Rejected {
        lease.completed_at = Some(decided_at.clone());
        lease.failure_code = Some("DISPATCH_REJECTED".to_string());
    }
    lease.reason = detail.clone();
    upsert_lease_row(ctx, lease);

    upsert_dispatch_ack_row(
        ctx,
        DispatchAck {
            ack_id,
            lease_id,
            session_id,
            task_id,
            node_id,
            status,
            decided_at,
            detail,
        },
    );
    Ok(())
}

#[table(accessor = rate_group_state, public)]
pub struct RateGroupState {
    #[primary_key]
    pub group_id: String,
    pub tier: String,
    pub limit_rpm: i64,
    pub limit_tpm: i64,
    pub current_rpm: i64,
    pub current_tpm: i64,
    pub reset_at: String,
}

#[reducer]
pub fn upsert_rate_group_state(
    ctx: &ReducerContext,
    group_id: String,
    tier: String,
    limit_rpm: i64,
    limit_tpm: i64,
    current_rpm: i64,
    current_tpm: i64,
    reset_at: String,
) -> Result<(), String> {
    let row = RateGroupState {
        group_id: group_id.clone(),
        tier,
        limit_rpm,
        limit_tpm,
        current_rpm,
        current_tpm,
        reset_at,
    };
    if ctx.db.rate_group_state().group_id().find(group_id).is_some() {
        ctx.db.rate_group_state().group_id().update(row);
    } else {
        ctx.db.rate_group_state().insert(row);
    }
    Ok(())
}

#[table(accessor = execution_memory, public)]
pub struct ExecutionMemory {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub task_dispatch_id: String,
    pub model_used: String,
    pub outcome: String,             // "success" | "fail" | "timeout"
    pub duration_ms: u64,
    pub error_summary: Option<String>,
    pub feedback_score: Option<f64>,
    pub created_at: String,
    #[default(None::<String>)]
    #[index(btree)]
    pub lease_id: Option<String>,
}

fn some_if_not_blank(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn option_if_not_blank(value: Option<String>) -> Option<String> {
    value.and_then(some_if_not_blank)
}

#[reducer]
pub fn insert_execution_memory(
    ctx: &ReducerContext,
    task_dispatch_id: String,
    lease_id: String,
    model_used: String,
    outcome: String,
    duration_ms: u64,
    error_summary: Option<String>,
    feedback_score: Option<f64>,
) -> Result<(), String> {
    ctx.db.execution_memory().insert(ExecutionMemory {
        id: 0,
        task_dispatch_id,
        model_used,
        outcome,
        duration_ms,
        error_summary,
        feedback_score,
        created_at: ctx.timestamp.to_string(),
        lease_id: some_if_not_blank(lease_id),
    });
    Ok(())
}

#[table(accessor = knowledge_embeddings, public)]
pub struct KnowledgeEmbedding {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    pub source_type: String,
    pub source_id: String,
    pub content_hash: String,
    pub embedding_json: String,
    pub ttl_hours: u32,
    pub created_at: String,
    pub last_accessed_at: Option<String>,
}

#[reducer]
pub fn insert_knowledge_embedding(
    ctx: &ReducerContext,
    source_type: String,
    source_id: String,
    content_hash: String,
    embedding_json: String,
    ttl_hours: u32,
) -> Result<(), String> {
    ctx.db.knowledge_embeddings().insert(KnowledgeEmbedding {
        id: 0,
        source_type,
        source_id,
        content_hash,
        embedding_json,
        ttl_hours,
        created_at: ctx.timestamp.to_string(),
        last_accessed_at: None,
    });
    Ok(())
}

#[table(accessor = agent_registry, public)]
pub struct AgentRegistryEntry {
    #[primary_key]
    pub cell_id: String,
    pub display_name: String,
    pub model_preference: Option<String>,
    pub tools_allowed_json: String,
    pub sandbox_mode: String,
    pub active: bool,
    pub created_at: String,
}

#[reducer]
pub fn upsert_agent_registry(
    ctx: &ReducerContext,
    cell_id: String,
    display_name: String,
    model_preference: Option<String>,
    tools_allowed_json: String,
    sandbox_mode: String,
    active: bool,
) -> Result<(), String> {
    if let Some(mut existing) = ctx.db.agent_registry().cell_id().find(&cell_id) {
        existing.display_name = display_name;
        existing.model_preference = model_preference;
        existing.tools_allowed_json = tools_allowed_json;
        existing.sandbox_mode = sandbox_mode;
        existing.active = active;
        ctx.db.agent_registry().cell_id().update(existing);
    } else {
        ctx.db.agent_registry().insert(AgentRegistryEntry {
            cell_id,
            display_name,
            model_preference,
            tools_allowed_json,
            sandbox_mode,
            active,
            created_at: ctx.timestamp.to_string(),
        });
    }
    Ok(())
}

#[table(accessor = node_registry, public)]
pub struct NodeRegistryEntry {
    #[primary_key]
    pub node_id: String,
    pub hostname: String,
    pub platform: String,
    pub capabilities_json: String,
    pub last_heartbeat: Option<String>,
    pub status: String,
}

#[reducer]
pub fn upsert_node_registry(
    ctx: &ReducerContext,
    node_id: String,
    hostname: String,
    platform: String,
    capabilities_json: String,
    status: String,
) -> Result<(), String> {
    if let Some(mut existing) = ctx.db.node_registry().node_id().find(&node_id) {
        existing.hostname = hostname;
        existing.platform = platform;
        existing.capabilities_json = capabilities_json;
        existing.last_heartbeat = Some(ctx.timestamp.to_string());
        existing.status = status;
        ctx.db.node_registry().node_id().update(existing);
    } else {
        ctx.db.node_registry().insert(NodeRegistryEntry {
            node_id,
            hostname,
            platform,
            capabilities_json,
            last_heartbeat: Some(ctx.timestamp.to_string()),
            status,
        });
    }
    Ok(())
}

#[table(accessor = captain_directives, public)]
pub struct CaptainDirective {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    pub directive_type: String,
    pub payload_json: String,
    pub priority: u8,
    #[index(btree)]
    pub status: String,
    pub created_at: String,
    pub executed_at: Option<String>,
}

#[reducer]
pub fn insert_captain_directive(
    ctx: &ReducerContext,
    directive_type: String,
    payload_json: String,
    priority: u8,
) -> Result<(), String> {
    ctx.db.captain_directives().insert(CaptainDirective {
        id: 0,
        directive_type,
        payload_json,
        priority,
        status: "pending".to_string(),
        created_at: ctx.timestamp.to_string(),
        executed_at: None,
    });
    Ok(())
}

#[reducer]
pub fn prune_knowledge_embeddings(_ctx: &ReducerContext, _ttl_hours: u32) -> Result<(), String> {
    // In a real STDB app, we would use ctx.timestamp to find old rows.
    Ok(())
}

#[reducer]
pub fn prune_execution_memory(ctx: &ReducerContext, keep_last: u32) -> Result<(), String> {
    let mut all_ids: Vec<u64> = ctx.db.execution_memory().iter().map(|r| r.id).collect();
    if all_ids.len() > keep_last as usize {
        all_ids.sort();
        let to_delete = all_ids.len() - keep_last as usize;
        for i in 0..to_delete {
            ctx.db.execution_memory().id().delete(all_ids[i]);
        }
    }
    Ok(())
}

// ── Captain Memory (Heiwa Agent persistent conversation) ───────

#[table(accessor = captain_messages, public)]
pub struct CaptainMessage {
    #[primary_key]
    pub message_id: String,
    #[index(btree)]
    pub session_id: String,
    pub role: String,
    pub content: String,
    #[index(btree)]
    pub timestamp: u64,
    pub source: String,
    pub compressed: bool,
}

#[table(accessor = captain_summaries, public)]
pub struct CaptainSummary {
    #[primary_key]
    pub summary_id: String,
    #[index(btree)]
    pub summary_type: String,
    pub content: String,
    pub message_range_start: u64,
    pub message_range_end: u64,
    pub messages_compressed: u32,
    pub created_at: u64,
}

#[table(accessor = captain_focus, public)]
pub struct CaptainFocus {
    #[primary_key]
    pub focus_id: String,
    pub topic: String,
    pub context_json: String,
    #[index(btree)]
    pub priority: u8,
    pub created_at: u64,
    pub resolved_at: u64,
}

#[reducer]
pub fn insert_captain_message(
    ctx: &ReducerContext,
    message_id: String,
    session_id: String,
    role: String,
    content: String,
    timestamp: u64,
    source: String,
) -> Result<(), String> {
    ctx.db.captain_messages().insert(CaptainMessage {
        message_id,
        session_id,
        role,
        content,
        timestamp,
        source,
        compressed: false,
    });
    Ok(())
}

#[reducer]
pub fn mark_messages_compressed(
    ctx: &ReducerContext,
    session_id: String,
    before_timestamp: u64,
) -> Result<(), String> {
    let msgs: Vec<CaptainMessage> = ctx
        .db
        .captain_messages()
        .iter()
        .filter(|m| m.session_id == session_id && m.timestamp < before_timestamp && !m.compressed)
        .collect();
    for mut msg in msgs {
        msg.compressed = true;
        ctx.db.captain_messages().message_id().update(msg);
    }
    Ok(())
}

#[reducer]
pub fn insert_captain_summary(
    ctx: &ReducerContext,
    summary_id: String,
    summary_type: String,
    content: String,
    range_start: u64,
    range_end: u64,
    messages_compressed: u32,
) -> Result<(), String> {
    ctx.db.captain_summaries().insert(CaptainSummary {
        summary_id,
        summary_type,
        content,
        message_range_start: range_start,
        message_range_end: range_end,
        messages_compressed,
        created_at: (ctx.timestamp.to_micros_since_unix_epoch() / 1000) as u64,
    });
    Ok(())
}

#[reducer]
pub fn upsert_captain_focus(
    ctx: &ReducerContext,
    focus_id: String,
    topic: String,
    context_json: String,
    priority: u8,
) -> Result<(), String> {
    if let Some(mut existing) = ctx.db.captain_focus().focus_id().find(&focus_id) {
        existing.topic = topic;
        existing.context_json = context_json;
        existing.priority = priority;
        ctx.db.captain_focus().focus_id().update(existing);
    } else {
        ctx.db.captain_focus().insert(CaptainFocus {
            focus_id,
            topic,
            context_json,
            priority,
            created_at: (ctx.timestamp.to_micros_since_unix_epoch() / 1000) as u64,
            resolved_at: 0,
        });
    }
    Ok(())
}

#[reducer]
pub fn resolve_captain_focus(
    ctx: &ReducerContext,
    focus_id: String,
    resolved_at: u64,
) -> Result<(), String> {
    let mut focus = ctx
        .db
        .captain_focus()
        .focus_id()
        .find(&focus_id)
        .ok_or("Focus not found")?;
    focus.resolved_at = resolved_at;
    ctx.db.captain_focus().focus_id().update(focus);
    Ok(())
}

#[reducer]
pub fn prune_captain_messages(
    ctx: &ReducerContext,
    before_timestamp: u64,
) -> Result<(), String> {
    let old: Vec<String> = ctx
        .db
        .captain_messages()
        .iter()
        .filter(|m| m.compressed && m.timestamp < before_timestamp)
        .map(|m| m.message_id.clone())
        .collect();
    for id in old {
        ctx.db.captain_messages().message_id().delete(&id);
    }
    Ok(())
}

#[reducer]
pub fn create_user(
    ctx: &ReducerContext,
    user_id: String,
    display_name: String,
    email: Option<String>,
    avatar_url: Option<String>,
    tier: String,
) -> Result<(), String> {
    if ctx.db.users().user_id().find(user_id.clone()).is_some() {
        return Err("User already exists".into());
    }
    let now = now_string(ctx);
    ctx.db.users().insert(User {
        user_id,
        display_name,
        email,
        avatar_url,
        status: "active".to_string(),
        tier,
        created_at: now.clone(),
        last_seen_at: now,
        metadata_json: "{}".to_string(),
    });
    Ok(())
}

#[reducer]
pub fn update_user_seen(
    ctx: &ReducerContext,
    user_id: String,
) -> Result<(), String> {
    let mut user = ctx.db.users().user_id().find(user_id).ok_or("User not found")?;
    user.last_seen_at = now_string(ctx);
    ctx.db.users().user_id().update(user);
    Ok(())
}

#[reducer]
pub fn link_oauth_identity(
    ctx: &ReducerContext,
    identity_id: String,
    user_id: String,
    provider: String,
    provider_user_id: String,
    username: String,
    access_token_enc: String,
    refresh_token_enc: Option<String>,
    token_expires_at: Option<String>,
    scopes_json: String,
) -> Result<(), String> {
    let now = now_string(ctx);
    let row = OAuthIdentity {
        identity_id: identity_id.clone(),
        user_id,
        provider,
        provider_user_id,
        username,
        access_token_enc,
        refresh_token_enc,
        token_expires_at,
        scopes_json,
        created_at: now.clone(),
        updated_at: now,
    };
    if ctx.db.oauth_identities().identity_id().find(identity_id).is_some() {
        ctx.db.oauth_identities().identity_id().update(row);
    } else {
        ctx.db.oauth_identities().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn store_provider_credential(
    ctx: &ReducerContext,
    credential_id: String,
    user_id: String,
    provider_id: String,
    credential_kind: String,
    credential_enc: String,
    rate_group: String,
    display_label: Option<String>,
) -> Result<(), String> {
    let now = now_string(ctx);
    let row = ProviderCredential {
        credential_id: credential_id.clone(),
        user_id,
        provider_id,
        credential_kind,
        credential_enc,
        status: "active".to_string(),
        rate_group,
        display_label,
        last_validated_at: None,
        last_error: None,
        created_at: now.clone(),
        updated_at: now,
    };
    if ctx.db.provider_credentials().credential_id().find(credential_id).is_some() {
        ctx.db.provider_credentials().credential_id().update(row);
    } else {
        ctx.db.provider_credentials().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn revoke_provider_credential(
    ctx: &ReducerContext,
    credential_id: String,
) -> Result<(), String> {
    let mut cred = ctx.db.provider_credentials().credential_id().find(credential_id).ok_or("Credential not found")?;
    cred.status = "revoked".to_string();
    cred.updated_at = now_string(ctx);
    ctx.db.provider_credentials().credential_id().update(cred);
    Ok(())
}

#[reducer]
pub fn record_billing_event(
    ctx: &ReducerContext,
    user_id: String,
    event_type: String,
    provider_id: String,
    model_id: String,
    tokens_input: i64,
    tokens_output: i64,
    estimated_cost_usd: f64,
    credential_id: Option<String>,
    mission_id: Option<String>,
    run_id: Option<String>,
) -> Result<(), String> {
    ctx.db.billing_events().insert(BillingEvent {
        id: 0,
        user_id,
        event_type,
        provider_id,
        model_id,
        tokens_input,
        tokens_output,
        estimated_cost_usd,
        credential_id,
        mission_id,
        run_id,
        created_at: now_string(ctx),
    });
    Ok(())
}

// ===========================================================================
// DOCTRINE PHASE 1 — Knowledge, Belief, Treasury, Novelty, Schedule
// ===========================================================================

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn validate_enum(value: &str, allowed: &[&str], enum_name: &str) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!("invalid {}: '{}' (allowed: {:?})", enum_name, value, allowed))
    }
}

const SOURCE_KINDS: &[&str] = &["web", "pdf", "repo_file", "conversation", "api", "note", "dataset"];
const PARSE_STATUSES: &[&str] = &["pending", "parsed", "failed"];
const PAGE_STATUSES: &[&str] = &["draft", "active", "stale", "superseded", "retired"];
const BELIEF_STATUSES: &[&str] = &["candidate", "supported", "durable", "contested", "stale", "retired", "false"];
const EVIDENCE_LINK_TYPES: &[&str] = &["supports", "contradicts", "derived_from", "verified_by"];
const CONTRADICTION_RESOLUTIONS: &[&str] = &["open", "resolved_primary_wins", "resolved_challenger_wins", "both_retired", "merged"];
const TREASURY_SCOPES: &[&str] = &["user", "org", "device", "provider_account"];
const BUDGET_WINDOW_KINDS: &[&str] = &["hour", "day", "month", "rolling"];
const RESERVE_POLICIES: &[&str] = &["strict", "soft", "none"];
const TREASURY_HEALTH_STATES: &[&str] = &["healthy", "guarded", "degraded", "cooldown", "exhausted"];
const RESERVATION_STATUSES: &[&str] = &["held", "consumed", "released", "expired"];
const TREASURY_DECISIONS: &[&str] = &["allow", "allow_downgraded", "defer", "deny", "require_approval"];
const TASK_CLASSES: &[&str] = &["compile", "consolidate", "act"];
const BUDGET_CLASSES: &[&str] = &["cheap", "standard", "premium"];
const APPROVAL_STATES: &[&str] = &["none_needed", "pending", "granted", "denied"];

// ---------------------------------------------------------------------------
// Step 1: Knowledge Plane tables
// ---------------------------------------------------------------------------

/// Immutable raw input — the atomic evidence unit.
#[table(accessor = sources, private)]
pub struct Source {
    #[primary_key]
    pub source_id: String,
    #[index(btree)]
    pub source_kind: String,
    #[index(btree)]
    pub uri: String,
    #[index(btree)]
    pub content_hash: String,
    pub title: String,
    pub author: String,
    #[index(btree)]
    pub captured_at: String,
    pub published_at: String,
    pub retrieved_by: String,
    #[index(btree)]
    pub owner_scope: String,
    pub trust_class: String,
    pub raw_storage_ref: String,
    #[index(btree)]
    pub parse_status: String,
    pub supersedes_source_id: Option<String>,
    pub created_at: String,
}

/// Human-readable synthesized knowledge node — compiled from sources.
#[table(accessor = pages, public)]
pub struct Page {
    #[primary_key]
    pub page_id: String,
    #[index(btree)]
    pub namespace: String,
    pub title: String,
    #[index(btree)]
    pub slug: String,
    pub body_markdown: String,
    pub summary: String,
    pub topic_keys_json: String,
    #[index(btree)]
    pub status: String,
    pub source_set_hash: String,
    #[index(btree)]
    pub last_compiled_at: String,
    pub last_verified_at: String,
    #[index(btree)]
    pub freshness_score: f64,
    pub novelty_score_last_compile: f64,
    #[index(btree)]
    pub owner_scope: String,
    pub generated_by_run_id: Option<String>,
    #[index(btree)]
    pub created_at: String,
    #[index(btree)]
    pub updated_at: String,
}

/// Links between pages, and from pages to sources.
#[table(accessor = page_links, public)]
pub struct PageLink {
    #[primary_key]
    pub link_id: String,
    #[index(btree)]
    pub from_page_id: String,
    #[index(btree)]
    pub to_page_id: Option<String>,
    #[index(btree)]
    pub to_source_id: Option<String>,
    #[index(btree)]
    pub link_type: String,
    pub created_at: String,
}

/// Domain-scoped freshness decay model for beliefs.
#[table(accessor = decay_profiles, public)]
pub struct DecayProfile {
    #[primary_key]
    pub decay_profile_id: String,
    #[index(btree)]
    pub domain: String,
    pub half_life_seconds: u64,
    pub floor_freshness: f64,
    pub verification_reset_mode: String,
    pub staleness_trigger_policy: String,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Step 2: Belief tables
// ---------------------------------------------------------------------------

/// Structured durable claim with lifecycle — the key doctrine abstraction.
#[table(accessor = beliefs, public)]
pub struct Belief {
    #[primary_key]
    pub belief_id: String,
    pub claim_text: String,
    #[index(btree)]
    pub domain: String,
    pub topic_keys_json: String,
    #[index(btree)]
    pub confidence: f64,
    #[index(btree)]
    pub status: String,
    #[index(btree)]
    pub freshness_score: f64,
    #[index(btree)]
    pub decay_profile_id: String,
    pub corroboration_count: u32,
    pub contradiction_count: u32,
    pub source_count: u32,
    pub supporting_evidence_weight: f64,
    pub contradicting_evidence_weight: f64,
    pub last_verified_at: Option<String>,
    pub expires_at: Option<String>,
    pub promoted_at: Option<String>,
    pub retired_at: Option<String>,
    pub derived_from_page_ids_json: String,
    pub derived_from_source_ids_json: String,
    #[index(btree)]
    pub owner_scope: String,
    pub generated_by_run_id: Option<String>,
    #[index(btree)]
    pub created_at: String,
    #[index(btree)]
    pub updated_at: String,
}

/// Joins beliefs to their supporting/contradicting evidence.
#[table(accessor = belief_evidence_links, private)]
pub struct BeliefEvidenceLink {
    #[primary_key]
    pub link_id: String,
    #[index(btree)]
    pub belief_id: String,
    #[index(btree)]
    pub source_id: Option<String>,
    #[index(btree)]
    pub page_id: Option<String>,
    pub run_id: Option<String>,
    #[index(btree)]
    pub link_type: String,
    pub weight: f64,
    pub created_at: String,
}

/// Explicit record that a belief is challenged by another belief or source.
#[table(accessor = belief_contradictions, public)]
pub struct BeliefContradiction {
    #[primary_key]
    pub contradiction_id: String,
    #[index(btree)]
    pub primary_belief_id: String,
    #[index(btree)]
    pub challenger_belief_id: Option<String>,
    #[index(btree)]
    pub challenger_source_id: Option<String>,
    pub description: String,
    pub severity: f64,
    #[index(btree)]
    pub resolution_status: String,
    pub resolved_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Step 3: Treasury tables
// ---------------------------------------------------------------------------

/// Provider/account budget governor — manages scarcity over time.
#[table(accessor = treasuries, public)]
pub struct Treasury {
    #[primary_key]
    pub treasury_id: String,
    #[index(btree)]
    pub scope: String,
    #[index(btree)]
    pub provider: String,
    pub account_ref: String,
    pub budget_window_kind: String,
    pub max_requests: i64,
    pub max_tokens_in: i64,
    pub max_tokens_out: i64,
    pub max_cost_millicents: i64,
    pub reserve_fraction: f64,
    pub reserve_policy: String,
    pub degraded_mode_threshold: f64,
    #[index(btree)]
    pub health_state: String,
    #[index(btree)]
    pub health_score: f64,
    pub cooldown_until: Option<String>,
    pub failure_streak: u32,
    pub last_429_at: Option<String>,
    pub last_auth_failure_at: Option<String>,
    pub spend_so_far_requests: i64,
    pub spend_so_far_tokens_in: i64,
    pub spend_so_far_tokens_out: i64,
    pub spend_so_far_cost_millicents: i64,
    pub last_rebalanced_at: String,
    pub created_at: String,
    #[index(btree)]
    pub updated_at: String,
}

/// Pre-committed budget reservation for a mission.
#[table(accessor = treasury_reservations, private)]
pub struct TreasuryReservation {
    #[primary_key]
    pub reservation_id: String,
    #[index(btree)]
    pub treasury_id: String,
    #[index(btree)]
    pub mission_id: String,
    pub reserved_requests: i64,
    pub reserved_tokens: i64,
    pub reserved_cost_millicents: i64,
    #[index(btree)]
    pub status: String,
    pub decision: String,
    pub created_at: String,
    #[index(btree)]
    pub expires_at: String,
    pub consumed_at: Option<String>,
    pub released_at: Option<String>,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Step 4: NoveltyVector
// ---------------------------------------------------------------------------

/// Per-run novelty scoring — measures whether work produced new understanding.
#[table(accessor = novelty_vectors, public)]
pub struct NoveltyVector {
    #[primary_key]
    pub vector_id: String,
    #[index(btree)]
    pub run_id: String,
    #[index(btree)]
    pub mission_id: String,
    #[index(btree)]
    pub turn_index: u32,
    pub new_entity: f64,
    pub new_contradiction: f64,
    pub stronger_citation: f64,
    pub new_causal_connection: f64,
    pub changed_recommendation: f64,
    #[index(btree)]
    pub composite_score: f64,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Step 5: Schedule tables (schema hooks for Phase 2 workers)
// ---------------------------------------------------------------------------

/// Queued belief staleness verification — populated by decay, consumed by consolidation workers.
#[table(accessor = belief_verification_schedule, private)]
pub struct BeliefVerificationSchedule {
    #[primary_key]
    pub schedule_id: String,
    #[index(btree)]
    pub belief_id: String,
    pub reason: String,
    #[index(btree)]
    pub priority: i64,
    #[index(btree)]
    pub scheduled_at: String,
    pub claimed_by_run_id: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}

/// Queued treasury rebalance — populated by spend/failure, consumed by maintenance workers.
#[table(accessor = treasury_rebalance_schedule, private)]
pub struct TreasuryRebalanceSchedule {
    #[primary_key]
    pub schedule_id: String,
    #[index(btree)]
    pub treasury_id: String,
    pub reason: String,
    #[index(btree)]
    pub priority: i64,
    #[index(btree)]
    pub scheduled_at: String,
    pub claimed_by_run_id: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}

// ===========================================================================
// DOCTRINE REDUCERS
// ===========================================================================

// ---------------------------------------------------------------------------
// Step 7: Knowledge Plane reducers
// ---------------------------------------------------------------------------

#[reducer]
pub fn ingest_source(
    ctx: &ReducerContext,
    source_id: String,
    source_kind: String,
    uri: String,
    content_hash: String,
    title: String,
    author: String,
    captured_at: String,
    published_at: String,
    retrieved_by: String,
    owner_scope: String,
    trust_class: String,
    raw_storage_ref: String,
    supersedes_source_id: Option<String>,
) -> Result<(), String> {
    validate_enum(&source_kind, SOURCE_KINDS, "source_kind")?;
    let now = now_string(ctx);
    ctx.db.sources().insert(Source {
        source_id,
        source_kind,
        uri,
        content_hash,
        title,
        author,
        captured_at,
        published_at,
        retrieved_by,
        owner_scope,
        trust_class,
        raw_storage_ref,
        parse_status: "pending".to_string(),
        supersedes_source_id,
        created_at: now,
    });
    Ok(())
}

#[reducer]
pub fn mark_source_parsed(
    ctx: &ReducerContext,
    source_id: String,
    parse_status: String,
) -> Result<(), String> {
    validate_enum(&parse_status, PARSE_STATUSES, "parse_status")?;
    let mut row = ctx.db.sources().source_id().find(&source_id)
        .ok_or_else(|| format!("source not found: {}", source_id))?;
    row.parse_status = parse_status;
    ctx.db.sources().source_id().update(row);
    Ok(())
}

#[reducer]
pub fn create_source_version(
    ctx: &ReducerContext,
    new_source_id: String,
    supersedes_source_id: String,
    source_kind: String,
    uri: String,
    content_hash: String,
    title: String,
    author: String,
    captured_at: String,
    published_at: String,
    retrieved_by: String,
    owner_scope: String,
    trust_class: String,
    raw_storage_ref: String,
) -> Result<(), String> {
    validate_enum(&source_kind, SOURCE_KINDS, "source_kind")?;
    // Verify the superseded source exists
    ctx.db.sources().source_id().find(&supersedes_source_id)
        .ok_or_else(|| format!("superseded source not found: {}", supersedes_source_id))?;
    let now = now_string(ctx);
    ctx.db.sources().insert(Source {
        source_id: new_source_id,
        source_kind,
        uri,
        content_hash,
        title,
        author,
        captured_at,
        published_at,
        retrieved_by,
        owner_scope,
        trust_class,
        raw_storage_ref,
        parse_status: "pending".to_string(),
        supersedes_source_id: Some(supersedes_source_id),
        created_at: now,
    });
    Ok(())
}

#[reducer]
pub fn create_page(
    ctx: &ReducerContext,
    page_id: String,
    namespace: String,
    title: String,
    slug: String,
    body_markdown: String,
    summary: String,
    topic_keys_json: String,
    source_set_hash: String,
    owner_scope: String,
    generated_by_run_id: Option<String>,
) -> Result<(), String> {
    // Enforce (namespace, slug) uniqueness
    for existing in ctx.db.pages().namespace().filter(&namespace) {
        if existing.slug == slug {
            return Err(format!(
                "page with namespace='{}' slug='{}' already exists (page_id={})",
                namespace, slug, existing.page_id
            ));
        }
    }
    let now = now_string(ctx);
    ctx.db.pages().insert(Page {
        page_id,
        namespace,
        title,
        slug,
        body_markdown,
        summary,
        topic_keys_json,
        status: "draft".to_string(),
        source_set_hash,
        last_compiled_at: now.clone(),
        last_verified_at: now.clone(),
        freshness_score: 1.0,
        novelty_score_last_compile: 0.0,
        owner_scope,
        generated_by_run_id,
        created_at: now.clone(),
        updated_at: now,
    });
    Ok(())
}

#[reducer]
pub fn recompile_page(
    ctx: &ReducerContext,
    page_id: String,
    body_markdown: String,
    summary: String,
    topic_keys_json: String,
    source_set_hash: String,
    novelty_score: f64,
) -> Result<(), String> {
    let mut row = ctx.db.pages().page_id().find(&page_id)
        .ok_or_else(|| format!("page not found: {}", page_id))?;
    let now = now_string(ctx);
    row.body_markdown = body_markdown;
    row.summary = summary;
    row.topic_keys_json = topic_keys_json;
    row.source_set_hash = source_set_hash;
    row.novelty_score_last_compile = novelty_score;
    row.freshness_score = 1.0; // recompilation resets freshness
    row.last_compiled_at = now.clone();
    row.status = "active".to_string();
    row.updated_at = now;
    ctx.db.pages().page_id().update(row);
    Ok(())
}

#[reducer]
pub fn mark_page_stale(
    ctx: &ReducerContext,
    page_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.pages().page_id().find(&page_id)
        .ok_or_else(|| format!("page not found: {}", page_id))?;
    row.status = "stale".to_string();
    row.updated_at = now_string(ctx);
    ctx.db.pages().page_id().update(row);
    Ok(())
}

#[reducer]
pub fn retire_page(
    ctx: &ReducerContext,
    page_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.pages().page_id().find(&page_id)
        .ok_or_else(|| format!("page not found: {}", page_id))?;
    row.status = "retired".to_string();
    row.updated_at = now_string(ctx);
    ctx.db.pages().page_id().update(row);
    Ok(())
}

#[reducer]
pub fn create_page_link(
    ctx: &ReducerContext,
    link_id: String,
    from_page_id: String,
    to_page_id: Option<String>,
    to_source_id: Option<String>,
    link_type: String,
) -> Result<(), String> {
    let now = now_string(ctx);
    ctx.db.page_links().insert(PageLink {
        link_id,
        from_page_id,
        to_page_id,
        to_source_id,
        link_type,
        created_at: now,
    });
    Ok(())
}

#[reducer]
pub fn upsert_decay_profile(
    ctx: &ReducerContext,
    decay_profile_id: String,
    domain: String,
    half_life_seconds: u64,
    floor_freshness: f64,
    verification_reset_mode: String,
    staleness_trigger_policy: String,
) -> Result<(), String> {
    let now = now_string(ctx);
    // Enforce domain uniqueness: no other profile may own this domain
    for existing in ctx.db.decay_profiles().domain().filter(&domain) {
        if existing.decay_profile_id != decay_profile_id {
            return Err(format!(
                "domain '{}' already owned by decay_profile_id={}",
                domain, existing.decay_profile_id
            ));
        }
    }
    let row = DecayProfile {
        decay_profile_id: decay_profile_id.clone(),
        domain,
        half_life_seconds,
        floor_freshness,
        verification_reset_mode,
        staleness_trigger_policy,
        created_at: now.clone(),
        updated_at: now,
    };
    if ctx.db.decay_profiles().decay_profile_id().find(&decay_profile_id).is_some() {
        ctx.db.decay_profiles().decay_profile_id().update(row);
    } else {
        ctx.db.decay_profiles().insert(row);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Step 8: Belief reducers — reducers own authoritative computation
// ---------------------------------------------------------------------------

#[reducer]
pub fn extract_belief_candidate(
    ctx: &ReducerContext,
    belief_id: String,
    claim_text: String,
    domain: String,
    topic_keys_json: String,
    initial_confidence: f64,
    decay_profile_id: String,
    derived_from_page_ids_json: String,
    derived_from_source_ids_json: String,
    owner_scope: String,
    generated_by_run_id: Option<String>,
) -> Result<(), String> {
    // Verify decay profile exists
    ctx.db.decay_profiles().decay_profile_id().find(&decay_profile_id)
        .ok_or_else(|| format!("decay profile not found: {}", decay_profile_id))?;
    // Clamp initial confidence to [0, 1]
    let confidence = initial_confidence.clamp(0.0, 1.0);
    let now = now_string(ctx);
    ctx.db.beliefs().insert(Belief {
        belief_id,
        claim_text,
        domain,
        topic_keys_json,
        confidence,
        status: "candidate".to_string(),
        freshness_score: 1.0,
        decay_profile_id,
        corroboration_count: 0,
        contradiction_count: 0,
        source_count: 0,
        supporting_evidence_weight: 0.0,
        contradicting_evidence_weight: 0.0,
        last_verified_at: None,
        expires_at: None,
        promoted_at: None,
        retired_at: None,
        derived_from_page_ids_json,
        derived_from_source_ids_json,
        owner_scope,
        generated_by_run_id,
        created_at: now.clone(),
        updated_at: now,
    });
    Ok(())
}

#[reducer]
pub fn link_belief_evidence(
    ctx: &ReducerContext,
    link_id: String,
    belief_id: String,
    source_id: Option<String>,
    page_id: Option<String>,
    run_id: Option<String>,
    link_type: String,
    weight: f64,
) -> Result<(), String> {
    validate_enum(&link_type, EVIDENCE_LINK_TYPES, "link_type")?;
    // Verify belief exists
    ctx.db.beliefs().belief_id().find(&belief_id)
        .ok_or_else(|| format!("belief not found: {}", belief_id))?;
    let now = now_string(ctx);
    ctx.db.belief_evidence_links().insert(BeliefEvidenceLink {
        link_id,
        belief_id,
        source_id,
        page_id,
        run_id,
        link_type,
        weight,
        created_at: now,
    });
    Ok(())
}

#[reducer]
pub fn corroborate_belief(
    ctx: &ReducerContext,
    belief_id: String,
    evidence_source_id: String,
    evidence_weight: f64,
) -> Result<(), String> {
    let mut row = ctx.db.beliefs().belief_id().find(&belief_id)
        .ok_or_else(|| format!("belief not found: {}", belief_id))?;
    // Must be in an active status to corroborate
    if row.status == "retired" || row.status == "false" {
        return Err(format!("cannot corroborate belief in status '{}'", row.status));
    }
    row.corroboration_count += 1;
    row.source_count += 1;
    row.supporting_evidence_weight += evidence_weight;
    // Recompute confidence from evidence weights
    let total_weight = row.supporting_evidence_weight + row.contradicting_evidence_weight;
    if total_weight > 0.0 {
        row.confidence = (row.supporting_evidence_weight / total_weight).clamp(0.0, 1.0);
    }
    // Auto-transition: candidate -> supported if corroboration >= 2
    if row.status == "candidate" && row.corroboration_count >= 2 {
        row.status = "supported".to_string();
    }
    let now = now_string(ctx);
    row.last_verified_at = Some(now.clone());
    row.updated_at = now;
    ctx.db.beliefs().belief_id().update(row);
    let _ = evidence_source_id; // used for audit trail via link_belief_evidence
    Ok(())
}

#[reducer]
pub fn contradict_belief(
    ctx: &ReducerContext,
    belief_id: String,
    challenger_belief_id: Option<String>,
    challenger_source_id: Option<String>,
    contradiction_id: String,
    description: String,
    severity: f64,
) -> Result<(), String> {
    // Invariant: exactly one challenger
    match (&challenger_belief_id, &challenger_source_id) {
        (Some(_), None) | (None, Some(_)) => {}
        _ => return Err("exactly one of challenger_belief_id or challenger_source_id must be set".to_string()),
    }
    let mut row = ctx.db.beliefs().belief_id().find(&belief_id)
        .ok_or_else(|| format!("belief not found: {}", belief_id))?;
    if row.status == "retired" || row.status == "false" {
        return Err(format!("cannot contradict belief in status '{}'", row.status));
    }
    row.contradiction_count += 1;
    row.contradicting_evidence_weight += severity;
    // Recompute confidence
    let total_weight = row.supporting_evidence_weight + row.contradicting_evidence_weight;
    if total_weight > 0.0 {
        row.confidence = (row.supporting_evidence_weight / total_weight).clamp(0.0, 1.0);
    }
    // Transition to contested if contradiction weight is significant
    if row.contradicting_evidence_weight > 0.2 * total_weight {
        row.status = "contested".to_string();
    }
    let now = now_string(ctx);
    row.updated_at = now.clone();
    ctx.db.beliefs().belief_id().update(row);
    // Create contradiction record
    ctx.db.belief_contradictions().insert(BeliefContradiction {
        contradiction_id,
        primary_belief_id: belief_id,
        challenger_belief_id,
        challenger_source_id,
        description,
        severity,
        resolution_status: "open".to_string(),
        resolved_at: None,
        created_at: now.clone(),
        updated_at: now,
    });
    Ok(())
}

/// Promote a belief from supported -> durable. Reducer owns threshold computation.
#[reducer]
pub fn promote_belief(
    ctx: &ReducerContext,
    belief_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.beliefs().belief_id().find(&belief_id)
        .ok_or_else(|| format!("belief not found: {}", belief_id))?;
    if row.status != "supported" {
        return Err(format!("can only promote 'supported' beliefs, got '{}'", row.status));
    }
    // Load decay profile for domain-specific floor
    let profile = ctx.db.decay_profiles().decay_profile_id().find(&row.decay_profile_id)
        .ok_or_else(|| format!("decay profile not found: {}", row.decay_profile_id))?;
    // Doctrine durability thresholds (fail closed)
    if row.confidence < 0.80 {
        return Err(format!("confidence {:.2} below threshold 0.80", row.confidence));
    }
    if row.corroboration_count < 2 {
        return Err(format!("corroboration_count {} below threshold 2", row.corroboration_count));
    }
    if row.freshness_score < profile.floor_freshness {
        return Err(format!("freshness {:.2} below domain floor {:.2}", row.freshness_score, profile.floor_freshness));
    }
    let total_weight = row.supporting_evidence_weight + row.contradicting_evidence_weight;
    if total_weight > 0.0 && (row.contradicting_evidence_weight / total_weight) > 0.20 {
        return Err(format!(
            "contradiction weight ratio {:.2} exceeds threshold 0.20",
            row.contradicting_evidence_weight / total_weight
        ));
    }
    let now = now_string(ctx);
    row.status = "durable".to_string();
    row.promoted_at = Some(now.clone());
    row.updated_at = now;
    ctx.db.beliefs().belief_id().update(row);
    Ok(())
}

/// Reducer-owned freshness decay. Computes new freshness from decay profile.
#[reducer]
pub fn decay_belief_tick(
    ctx: &ReducerContext,
    belief_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.beliefs().belief_id().find(&belief_id)
        .ok_or_else(|| format!("belief not found: {}", belief_id))?;
    // Only decay active beliefs
    if row.status == "retired" || row.status == "false" {
        return Ok(());
    }
    let profile = ctx.db.decay_profiles().decay_profile_id().find(&row.decay_profile_id)
        .ok_or_else(|| format!("decay profile not found: {}", row.decay_profile_id))?;
    // Compute elapsed seconds since last verification (or creation)
    let reference_time = row.last_verified_at.as_deref()
        .unwrap_or(&row.created_at);
    let ref_secs: u64 = reference_time.parse().unwrap_or(0);
    let now_secs = (ctx.timestamp.to_micros_since_unix_epoch() / 1_000_000) as u64;
    let elapsed = now_secs.saturating_sub(ref_secs);
    // Exponential decay: freshness = e^(-ln(2) * elapsed / half_life)
    if profile.half_life_seconds > 0 {
        let decay_factor = (-(0.693147 * elapsed as f64) / profile.half_life_seconds as f64).exp();
        row.freshness_score = decay_factor.clamp(0.0, 1.0);
    }
    let now = now_string(ctx);
    // Transition to stale if below floor
    if row.freshness_score < profile.floor_freshness {
        if row.status != "stale" {
            row.status = "stale".to_string();
            // Enqueue verification
            ctx.db.belief_verification_schedule().insert(BeliefVerificationSchedule {
                schedule_id: format!("bvs-{}-{}", belief_id, now),
                belief_id: belief_id.clone(),
                reason: "freshness_decay".to_string(),
                priority: 0,
                scheduled_at: now.clone(),
                claimed_by_run_id: None,
                completed_at: None,
                created_at: now.clone(),
            });
        }
    }
    row.updated_at = now;
    ctx.db.beliefs().belief_id().update(row);
    Ok(())
}

/// Reverify a belief — caller provides evidence refs, reducer computes scores.
#[reducer]
pub fn reverify_belief(
    ctx: &ReducerContext,
    belief_id: String,
    verification_evidence_refs_json: String,
) -> Result<(), String> {
    let mut row = ctx.db.beliefs().belief_id().find(&belief_id)
        .ok_or_else(|| format!("belief not found: {}", belief_id))?;
    if row.status == "retired" || row.status == "false" {
        return Err(format!("cannot reverify belief in status '{}'", row.status));
    }
    let now = now_string(ctx);
    // Reset freshness toward 1.0 (verification confirms currency)
    row.freshness_score = 1.0;
    row.last_verified_at = Some(now.clone());
    // Restore from stale/contested to supported if evidence warrants
    if row.status == "stale" || row.status == "contested" {
        let total_weight = row.supporting_evidence_weight + row.contradicting_evidence_weight;
        let contradiction_ratio = if total_weight > 0.0 {
            row.contradicting_evidence_weight / total_weight
        } else {
            0.0
        };
        if contradiction_ratio <= 0.20 && row.confidence >= 0.50 {
            row.status = "supported".to_string();
        }
    }
    row.updated_at = now;
    let _ = verification_evidence_refs_json; // stored via link_belief_evidence
    ctx.db.beliefs().belief_id().update(row);
    Ok(())
}

#[reducer]
pub fn retire_belief(
    ctx: &ReducerContext,
    belief_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.beliefs().belief_id().find(&belief_id)
        .ok_or_else(|| format!("belief not found: {}", belief_id))?;
    let now = now_string(ctx);
    row.status = "retired".to_string();
    row.retired_at = Some(now.clone());
    row.updated_at = now;
    ctx.db.beliefs().belief_id().update(row);
    Ok(())
}

#[reducer]
pub fn disprove_belief(
    ctx: &ReducerContext,
    belief_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.beliefs().belief_id().find(&belief_id)
        .ok_or_else(|| format!("belief not found: {}", belief_id))?;
    let now = now_string(ctx);
    row.status = "false".to_string();
    row.confidence = 0.0;
    row.updated_at = now;
    ctx.db.beliefs().belief_id().update(row);
    Ok(())
}

#[reducer]
pub fn resolve_contradiction(
    ctx: &ReducerContext,
    contradiction_id: String,
    resolution_status: String,
) -> Result<(), String> {
    validate_enum(&resolution_status, CONTRADICTION_RESOLUTIONS, "resolution_status")?;
    if resolution_status == "open" {
        return Err("cannot resolve contradiction to 'open'".to_string());
    }
    let mut row = ctx.db.belief_contradictions().contradiction_id().find(&contradiction_id)
        .ok_or_else(|| format!("contradiction not found: {}", contradiction_id))?;
    if row.resolution_status != "open" {
        return Err(format!("contradiction already resolved: {}", row.resolution_status));
    }
    let now = now_string(ctx);
    row.resolution_status = resolution_status;
    row.resolved_at = Some(now.clone());
    row.updated_at = now;
    ctx.db.belief_contradictions().contradiction_id().update(row);
    Ok(())
}

// ---------------------------------------------------------------------------
// Step 9: Treasury reducers
// ---------------------------------------------------------------------------

fn compute_treasury_health(treasury: &Treasury) -> (String, f64) {
    let request_ratio = if treasury.max_requests > 0 {
        treasury.spend_so_far_requests as f64 / treasury.max_requests as f64
    } else {
        0.0
    };
    let cost_ratio = if treasury.max_cost_millicents > 0 {
        treasury.spend_so_far_cost_millicents as f64 / treasury.max_cost_millicents as f64
    } else {
        0.0
    };
    let spend_ratio = request_ratio.max(cost_ratio);
    let failure_penalty = (treasury.failure_streak as f64 * 0.1).min(0.5);
    let health_score = (1.0 - spend_ratio - failure_penalty).clamp(0.0, 1.0);

    let health_state = if treasury.cooldown_until.is_some() {
        "cooldown"
    } else if spend_ratio >= 1.0 {
        "exhausted"
    } else if health_score < treasury.degraded_mode_threshold {
        "degraded"
    } else if spend_ratio > (1.0 - treasury.reserve_fraction) {
        "guarded"
    } else {
        "healthy"
    };
    (health_state.to_string(), health_score)
}

#[reducer]
pub fn create_treasury(
    ctx: &ReducerContext,
    treasury_id: String,
    scope: String,
    provider: String,
    account_ref: String,
    budget_window_kind: String,
    max_requests: i64,
    max_tokens_in: i64,
    max_tokens_out: i64,
    max_cost_millicents: i64,
    reserve_fraction: f64,
    reserve_policy: String,
    degraded_mode_threshold: f64,
) -> Result<(), String> {
    validate_enum(&scope, TREASURY_SCOPES, "scope")?;
    validate_enum(&budget_window_kind, BUDGET_WINDOW_KINDS, "budget_window_kind")?;
    validate_enum(&reserve_policy, RESERVE_POLICIES, "reserve_policy")?;
    // Enforce uniqueness: (scope, provider, account_ref, budget_window_kind)
    for existing in ctx.db.treasuries().scope().filter(&scope) {
        if existing.provider == provider
            && existing.account_ref == account_ref
            && existing.budget_window_kind == budget_window_kind
        {
            return Err(format!(
                "treasury already exists for ({}, {}, {}, {}) -> treasury_id={}",
                scope, provider, account_ref, budget_window_kind, existing.treasury_id
            ));
        }
    }
    let now = now_string(ctx);
    ctx.db.treasuries().insert(Treasury {
        treasury_id,
        scope,
        provider,
        account_ref,
        budget_window_kind,
        max_requests,
        max_tokens_in,
        max_tokens_out,
        max_cost_millicents,
        reserve_fraction,
        reserve_policy,
        degraded_mode_threshold,
        health_state: "healthy".to_string(),
        health_score: 1.0,
        cooldown_until: None,
        failure_streak: 0,
        last_429_at: None,
        last_auth_failure_at: None,
        spend_so_far_requests: 0,
        spend_so_far_tokens_in: 0,
        spend_so_far_tokens_out: 0,
        spend_so_far_cost_millicents: 0,
        last_rebalanced_at: now.clone(),
        created_at: now.clone(),
        updated_at: now,
    });
    Ok(())
}

#[reducer]
pub fn reserve_budget(
    ctx: &ReducerContext,
    reservation_id: String,
    treasury_id: String,
    mission_id: String,
    reserved_requests: i64,
    reserved_tokens: i64,
    reserved_cost_millicents: i64,
    expires_at: String,
) -> Result<(), String> {
    let treasury = ctx.db.treasuries().treasury_id().find(&treasury_id)
        .ok_or_else(|| format!("treasury not found: {}", treasury_id))?;
    // Compute decision based on treasury state
    let available_requests = treasury.max_requests - treasury.spend_so_far_requests;
    let available_cost = treasury.max_cost_millicents - treasury.spend_so_far_cost_millicents;
    let reserve_requests = (treasury.max_requests as f64 * treasury.reserve_fraction) as i64;
    let reserve_cost = (treasury.max_cost_millicents as f64 * treasury.reserve_fraction) as i64;

    let decision = match treasury.health_state.as_str() {
        "exhausted" => "deny",
        "cooldown" => "defer",
        _ if reserved_requests > available_requests || reserved_cost_millicents > available_cost => "deny",
        _ if reserved_requests > (available_requests - reserve_requests)
            || reserved_cost_millicents > (available_cost - reserve_cost) => {
            match treasury.reserve_policy.as_str() {
                "strict" => "require_approval",
                "soft" => "allow_downgraded",
                _ => "allow",
            }
        }
        _ => "allow",
    };
    let status = if decision == "deny" || decision == "defer" {
        "expired" // immediately non-actionable
    } else {
        "held"
    };
    let now = now_string(ctx);
    ctx.db.treasury_reservations().insert(TreasuryReservation {
        reservation_id,
        treasury_id,
        mission_id,
        reserved_requests,
        reserved_tokens,
        reserved_cost_millicents,
        status: status.to_string(),
        decision: decision.to_string(),
        created_at: now.clone(),
        expires_at,
        consumed_at: None,
        released_at: None,
        updated_at: now,
    });
    Ok(())
}

#[reducer]
pub fn release_budget(
    ctx: &ReducerContext,
    reservation_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.treasury_reservations().reservation_id().find(&reservation_id)
        .ok_or_else(|| format!("reservation not found: {}", reservation_id))?;
    if row.status != "held" {
        return Err(format!("can only release 'held' reservations, got '{}'", row.status));
    }
    let now = now_string(ctx);
    row.status = "released".to_string();
    row.released_at = Some(now.clone());
    row.updated_at = now;
    ctx.db.treasury_reservations().reservation_id().update(row);
    Ok(())
}

#[reducer]
pub fn consume_reservation(
    ctx: &ReducerContext,
    reservation_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.treasury_reservations().reservation_id().find(&reservation_id)
        .ok_or_else(|| format!("reservation not found: {}", reservation_id))?;
    if row.status != "held" {
        return Err(format!("can only consume 'held' reservations, got '{}'", row.status));
    }
    let now = now_string(ctx);
    row.status = "consumed".to_string();
    row.consumed_at = Some(now.clone());
    row.updated_at = now;
    ctx.db.treasury_reservations().reservation_id().update(row);
    Ok(())
}

#[reducer]
pub fn expire_reservation(
    ctx: &ReducerContext,
    reservation_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.treasury_reservations().reservation_id().find(&reservation_id)
        .ok_or_else(|| format!("reservation not found: {}", reservation_id))?;
    if row.status == "consumed" || row.status == "released" || row.status == "expired" {
        return Ok(()); // already terminal
    }
    let now = now_string(ctx);
    row.status = "expired".to_string();
    row.updated_at = now;
    ctx.db.treasury_reservations().reservation_id().update(row);
    Ok(())
}

#[reducer]
pub fn record_spend(
    ctx: &ReducerContext,
    treasury_id: String,
    requests: i64,
    tokens_in: i64,
    tokens_out: i64,
    cost_millicents: i64,
) -> Result<(), String> {
    let mut row = ctx.db.treasuries().treasury_id().find(&treasury_id)
        .ok_or_else(|| format!("treasury not found: {}", treasury_id))?;
    row.spend_so_far_requests += requests;
    row.spend_so_far_tokens_in += tokens_in;
    row.spend_so_far_tokens_out += tokens_out;
    row.spend_so_far_cost_millicents += cost_millicents;
    let (health_state, health_score) = compute_treasury_health(&row);
    row.health_state = health_state;
    row.health_score = health_score;
    row.updated_at = now_string(ctx);
    ctx.db.treasuries().treasury_id().update(row);
    Ok(())
}

#[reducer]
pub fn record_provider_failure(
    ctx: &ReducerContext,
    treasury_id: String,
    failure_type: String,
) -> Result<(), String> {
    let mut row = ctx.db.treasuries().treasury_id().find(&treasury_id)
        .ok_or_else(|| format!("treasury not found: {}", treasury_id))?;
    let now = now_string(ctx);
    row.failure_streak += 1;
    match failure_type.as_str() {
        "429" => row.last_429_at = Some(now.clone()),
        "auth" => row.last_auth_failure_at = Some(now.clone()),
        _ => {} // other failure types just increment streak
    }
    // Enter cooldown after 3 consecutive failures
    if row.failure_streak >= 3 {
        // Default 5-minute cooldown
        let cooldown_secs = (ctx.timestamp.to_micros_since_unix_epoch() / 1_000_000) as u64 + 300;
        row.cooldown_until = Some(cooldown_secs.to_string());
    }
    let (health_state, health_score) = compute_treasury_health(&row);
    row.health_state = health_state;
    row.health_score = health_score;
    row.updated_at = now;
    ctx.db.treasuries().treasury_id().update(row);
    Ok(())
}

#[reducer]
pub fn rebalance_treasury(
    ctx: &ReducerContext,
    treasury_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.treasuries().treasury_id().find(&treasury_id)
        .ok_or_else(|| format!("treasury not found: {}", treasury_id))?;
    let now = now_string(ctx);
    // Clear expired cooldown
    if let Some(ref until) = row.cooldown_until {
        let until_secs: u64 = until.parse().unwrap_or(0);
        let now_secs = (ctx.timestamp.to_micros_since_unix_epoch() / 1_000_000) as u64;
        if now_secs >= until_secs {
            row.cooldown_until = None;
            row.failure_streak = 0;
        }
    }
    let (health_state, health_score) = compute_treasury_health(&row);
    row.health_state = health_state;
    row.health_score = health_score;
    row.last_rebalanced_at = now.clone();
    row.updated_at = now;
    ctx.db.treasuries().treasury_id().update(row);
    Ok(())
}

#[reducer]
pub fn reset_treasury_window(
    ctx: &ReducerContext,
    treasury_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.treasuries().treasury_id().find(&treasury_id)
        .ok_or_else(|| format!("treasury not found: {}", treasury_id))?;
    let now = now_string(ctx);
    row.spend_so_far_requests = 0;
    row.spend_so_far_tokens_in = 0;
    row.spend_so_far_tokens_out = 0;
    row.spend_so_far_cost_millicents = 0;
    // Reset health if no active failures
    if row.cooldown_until.is_none() && row.failure_streak == 0 {
        row.health_state = "healthy".to_string();
        row.health_score = 1.0;
    } else {
        let (health_state, health_score) = compute_treasury_health(&row);
        row.health_state = health_state;
        row.health_score = health_score;
    }
    row.last_rebalanced_at = now.clone();
    row.updated_at = now;
    ctx.db.treasuries().treasury_id().update(row);
    Ok(())
}

// ---------------------------------------------------------------------------
// Step 10: Mission enrichment reducers
// ---------------------------------------------------------------------------

#[reducer]
pub fn plan_mission_doctrine(
    ctx: &ReducerContext,
    mission_id: String,
    task_class: String,
    budget_class: String,
    treasury_id: Option<String>,
    novelty_policy_json: Option<String>,
    termination_policy_json: Option<String>,
    required_belief_query: Option<String>,
    allowed_tools_json: Option<String>,
    allowed_side_effects_json: Option<String>,
    target_outputs_json: Option<String>,
) -> Result<(), String> {
    validate_enum(&task_class, TASK_CLASSES, "task_class")?;
    validate_enum(&budget_class, BUDGET_CLASSES, "budget_class")?;
    let mut row = ctx.db.missions().mission_id().find(&mission_id)
        .ok_or_else(|| format!("mission not found: {}", mission_id))?;
    row.task_class = Some(task_class);
    row.budget_class = Some(budget_class);
    row.treasury_id = treasury_id;
    row.novelty_policy_json = novelty_policy_json;
    row.termination_policy_json = termination_policy_json;
    row.required_belief_query = required_belief_query;
    row.allowed_tools_json = allowed_tools_json;
    row.allowed_side_effects_json = allowed_side_effects_json;
    row.target_outputs_json = target_outputs_json;
    row.approval_state = Some("none_needed".to_string());
    row.turn_count = Some(0);
    row.cost_so_far_millicents = Some(0);
    row.novelty_so_far = Some(0.0);
    row.updated_at = now_string(ctx);
    ctx.db.missions().mission_id().update(row);
    Ok(())
}

#[reducer]
pub fn gate_mission_treasury(
    ctx: &ReducerContext,
    mission_id: String,
) -> Result<(), String> {
    let mut mission = ctx.db.missions().mission_id().find(&mission_id)
        .ok_or_else(|| format!("mission not found: {}", mission_id))?;
    let treasury_id = mission.treasury_id.as_ref()
        .ok_or_else(|| "mission has no treasury_id set".to_string())?;
    let treasury = ctx.db.treasuries().treasury_id().find(treasury_id)
        .ok_or_else(|| format!("treasury not found: {}", treasury_id))?;
    let new_state = match treasury.health_state.as_str() {
        "healthy" | "guarded" => "granted",
        "degraded" => {
            match mission.budget_class.as_deref() {
                Some("cheap") => "granted",
                _ => "pending",
            }
        }
        "cooldown" | "exhausted" => "denied",
        _ => "pending",
    };
    mission.approval_state = Some(new_state.to_string());
    mission.updated_at = now_string(ctx);
    ctx.db.missions().mission_id().update(mission);
    Ok(())
}

// ---------------------------------------------------------------------------
// Step 11: Novelty reducers
// ---------------------------------------------------------------------------

#[reducer]
pub fn record_novelty_vector(
    ctx: &ReducerContext,
    vector_id: String,
    run_id: String,
    mission_id: String,
    turn_index: u32,
    new_entity: f64,
    new_contradiction: f64,
    stronger_citation: f64,
    new_causal_connection: f64,
    changed_recommendation: f64,
    composite_score: f64,
) -> Result<(), String> {
    let now = now_string(ctx);
    ctx.db.novelty_vectors().insert(NoveltyVector {
        vector_id,
        run_id,
        mission_id,
        turn_index,
        new_entity,
        new_contradiction,
        stronger_citation,
        new_causal_connection,
        changed_recommendation,
        composite_score,
        created_at: now,
    });
    Ok(())
}

#[reducer]
pub fn mark_run_non_novel(
    ctx: &ReducerContext,
    run_id: String,
    mission_id: String,
    turn_index: u32,
) -> Result<(), String> {
    let now = now_string(ctx);
    // Record a zero-novelty vector
    ctx.db.novelty_vectors().insert(NoveltyVector {
        vector_id: format!("nv-zero-{}-{}", run_id, turn_index),
        run_id,
        mission_id: mission_id.clone(),
        turn_index,
        new_entity: 0.0,
        new_contradiction: 0.0,
        stronger_citation: 0.0,
        new_causal_connection: 0.0,
        changed_recommendation: 0.0,
        composite_score: 0.0,
        created_at: now.clone(),
    });
    // Update mission novelty_so_far if doctrine fields are set
    if let Some(mut mission) = ctx.db.missions().mission_id().find(&mission_id) {
        if mission.task_class.is_some() {
            mission.novelty_so_far = Some(0.0);
            mission.updated_at = now;
            ctx.db.missions().mission_id().update(mission);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Step 12: Schedule reducers
// ---------------------------------------------------------------------------

#[reducer]
pub fn schedule_belief_verification(
    ctx: &ReducerContext,
    schedule_id: String,
    belief_id: String,
    reason: String,
    priority: i64,
    scheduled_at: String,
) -> Result<(), String> {
    let now = now_string(ctx);
    ctx.db.belief_verification_schedule().insert(BeliefVerificationSchedule {
        schedule_id,
        belief_id,
        reason,
        priority,
        scheduled_at,
        claimed_by_run_id: None,
        completed_at: None,
        created_at: now,
    });
    Ok(())
}

#[reducer]
pub fn claim_belief_verification(
    ctx: &ReducerContext,
    schedule_id: String,
    run_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.belief_verification_schedule().schedule_id().find(&schedule_id)
        .ok_or_else(|| format!("schedule not found: {}", schedule_id))?;
    if row.claimed_by_run_id.is_some() {
        return Err(format!("schedule {} already claimed", schedule_id));
    }
    if row.completed_at.is_some() {
        return Err(format!("schedule {} already completed", schedule_id));
    }
    row.claimed_by_run_id = Some(run_id);
    ctx.db.belief_verification_schedule().schedule_id().update(row);
    Ok(())
}

#[reducer]
pub fn complete_belief_verification(
    ctx: &ReducerContext,
    schedule_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.belief_verification_schedule().schedule_id().find(&schedule_id)
        .ok_or_else(|| format!("schedule not found: {}", schedule_id))?;
    row.completed_at = Some(now_string(ctx));
    ctx.db.belief_verification_schedule().schedule_id().update(row);
    Ok(())
}

#[reducer]
pub fn schedule_treasury_rebalance(
    ctx: &ReducerContext,
    schedule_id: String,
    treasury_id: String,
    reason: String,
    priority: i64,
    scheduled_at: String,
) -> Result<(), String> {
    let now = now_string(ctx);
    ctx.db.treasury_rebalance_schedule().insert(TreasuryRebalanceSchedule {
        schedule_id,
        treasury_id,
        reason,
        priority,
        scheduled_at,
        claimed_by_run_id: None,
        completed_at: None,
        created_at: now,
    });
    Ok(())
}

#[reducer]
pub fn claim_treasury_rebalance(
    ctx: &ReducerContext,
    schedule_id: String,
    run_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.treasury_rebalance_schedule().schedule_id().find(&schedule_id)
        .ok_or_else(|| format!("schedule not found: {}", schedule_id))?;
    if row.claimed_by_run_id.is_some() {
        return Err(format!("schedule {} already claimed", schedule_id));
    }
    if row.completed_at.is_some() {
        return Err(format!("schedule {} already completed", schedule_id));
    }
    row.claimed_by_run_id = Some(run_id);
    ctx.db.treasury_rebalance_schedule().schedule_id().update(row);
    Ok(())
}

#[reducer]
pub fn complete_treasury_rebalance(
    ctx: &ReducerContext,
    schedule_id: String,
) -> Result<(), String> {
    let mut row = ctx.db.treasury_rebalance_schedule().schedule_id().find(&schedule_id)
        .ok_or_else(|| format!("schedule not found: {}", schedule_id))?;
    row.completed_at = Some(now_string(ctx));
    ctx.db.treasury_rebalance_schedule().schedule_id().update(row);
    Ok(())
}
