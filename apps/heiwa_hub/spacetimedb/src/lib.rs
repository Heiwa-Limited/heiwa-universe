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
    pub user_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
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
    pub owner_id: Option<String>,
    #[default(None::<String>)]
    #[index(btree)]
    pub principal_id: Option<String>,
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
pub fn record_run(
    ctx: &ReducerContext,
    run_id: String,
    user_id: String,
    proposal_id: String,
    lease_id: String,
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
        owner_id: option_if_not_blank(owner_id),
        principal_id: option_if_not_blank(principal_id),
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
        cell_run_id,
        artifact_type,
        title,
        uri,
        path,
        content_json,
        created_at,
        lease_id: option_if_not_blank(lease_id),
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

#[table(accessor = rate_group_state, public)]
pub struct RateGroupState {
    #[primary_key]
    pub rate_group: String,
    pub turns_used: u32,
    pub turns_max: u32,
    pub window_start: String,       // ISO 8601
    pub window_seconds: u32,
    pub cooldown_until: String,     // ISO 8601, empty if not cooling down
    pub available: bool,
}

#[reducer]
pub fn upsert_rate_group_state(
    ctx: &ReducerContext,
    rate_group: String,
    turns_used: u32,
    turns_max: u32,
    window_seconds: u32,
    cooldown_until: String,
    available: bool,
) -> Result<(), String> {
    if let Some(mut existing) = ctx.db.rate_group_state().rate_group().find(&rate_group) {
        existing.turns_used = turns_used;
        existing.turns_max = turns_max;
        existing.window_start = ctx.timestamp.to_string();
        existing.window_seconds = window_seconds;
        existing.cooldown_until = cooldown_until;
        existing.available = available;
        ctx.db.rate_group_state().rate_group().update(existing);
    } else {
        ctx.db.rate_group_state().insert(RateGroupState {
            rate_group,
            turns_used,
            turns_max,
            window_start: ctx.timestamp.to_string(),
            window_seconds,
            cooldown_until,
            available,
        });
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
