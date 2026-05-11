//! Evidence emission helpers — thin wrappers around SpacetimeDB reducers.
//!
//! Every method is a no-op when the client is offline (`connection()` returns
//! `None`), which lets callers fire-and-forget without branching.

use anyhow::{anyhow, Result};

use heiwa_bindings::record_route_decision_reducer::record_route_decision;
use heiwa_bindings::record_run_reducer::record_run;
use heiwa_bindings::register_device_reducer::register_device;
use heiwa_bindings::update_device_heartbeat_reducer::update_device_heartbeat;
use heiwa_bindings::upsert_provider_account_status_reducer::upsert_provider_account_status;
// Doctrine Phase 1 reducer imports
use heiwa_bindings::create_page_reducer::create_page;
use heiwa_bindings::extract_belief_candidate_reducer::extract_belief_candidate;
use heiwa_bindings::ingest_source_reducer::ingest_source;
use heiwa_bindings::plan_mission_doctrine_reducer::plan_mission_doctrine;
use heiwa_bindings::promote_belief_reducer::promote_belief;
use heiwa_bindings::recompile_page_reducer::recompile_page;
use heiwa_bindings::record_novelty_vector_reducer::record_novelty_vector;
use heiwa_bindings::record_spend_reducer::record_spend;

use crate::StdbClient;

impl StdbClient {
    // ── Device ───────────────────────────────────────────────────────────

    pub fn register_device(
        &self,
        device_id: &str,
        user_id: &str,
        hostname: &str,
        os: &str,
        arch: &str,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        conn.reducers
            .register_device(
                device_id.to_string(),
                user_id.to_string(),
                hostname.to_string(),
                os.to_string(),
                arch.to_string(),
            )
            .map_err(|e| anyhow!(e.to_string()))
    }

    pub fn heartbeat_device(&self, device_id: &str) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        conn.reducers
            .update_device_heartbeat(device_id.to_string())
            .map_err(|e| anyhow!(e.to_string()))
    }

    // ── Provider status ──────────────────────────────────────────────────

    pub fn sync_provider_status(
        &self,
        account_id: &str,
        provider_id: &str,
        node_id: &str,
        auth_kind: &str,
        local_handle_ref: &str,
        status: &str,
        display_name: Option<String>,
        default_model: Option<String>,
        available_models_json: &str,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        let now = chrono::Utc::now().to_rfc3339();
        conn.reducers
            .upsert_provider_account_status(
                account_id.to_string(),
                provider_id.to_string(),
                node_id.to_string(),
                auth_kind.to_string(),
                local_handle_ref.to_string(),
                status.to_string(),
                display_name,
                default_model,
                "local".to_string(), // rate_group
                available_models_json.to_string(),
                None, // last_validated_at
                None, // last_error
                now,  // updated_at
                None, // user_id
                None, // owner_id
                None, // principal_id
            )
            .map_err(|e| anyhow!(e.to_string()))
    }

    // ── Route decision ───────────────────────────────────────────────────

    pub fn record_route_decision(
        &self,
        request_id: &str,
        task_id: &str,
        raw_text: &str,
        intent_class: &str,
        risk_level: &str,
        privacy_level: &str,
        assigned_worker: &str,
        target_tool: &str,
        target_model: &str,
        target_runtime: &str,
        rationale: &str,
        confidence: f64,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        let now = chrono::Utc::now().to_rfc3339();
        conn.reducers
            .record_route_decision(
                request_id.to_string(),
                task_id.to_string(),
                "v1".to_string(), // envelope_version
                raw_text.to_string(),
                "terminal".to_string(), // source_surface
                intent_class.to_string(),
                risk_level.to_string(),
                privacy_level.to_string(),
                0, // compute_class
                assigned_worker.to_string(),
                target_tool.to_string(),
                target_model.to_string(),
                target_runtime.to_string(),
                "auto".to_string(), // target_tier
                false,              // requires_approval
                rationale.to_string(),
                confidence,
                "local".to_string(), // gateway_transport
                now,                 // created_at
                None,                // user_id
                None,                // owner_id
                None,                // principal_id
                None,                // drex_decision_id
            )
            .map_err(|e| anyhow!(e.to_string()))
    }

    // ── Run record ───────────────────────────────────────────────────────

    pub fn record_run(
        &self,
        run_id: &str,
        user_id: &str,
        proposal_id: &str,
        started_at: &str,
        ended_at: &str,
        status: &str,
        model_id: &str,
        tokens_input: i64,
        tokens_output: i64,
        cost: f64,
        session_id: Option<String>,
        failure_code: Option<String>,
        failure_message: Option<String>,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        conn.reducers
            .record_run(
                run_id.to_string(),
                user_id.to_string(),
                proposal_id.to_string(),
                "local-lease".to_string(), // lease_id
                session_id,
                started_at.to_string(),
                ended_at.to_string(),
                status.to_string(),
                "{}".to_string(),       // chain_result_json
                "{}".to_string(),       // signals_json
                "[]".to_string(),       // artifact_index_json
                "local".to_string(),    // node_id
                "{}".to_string(),       // replay_receipt_json
                "terminal".to_string(), // mode
                model_id.to_string(),
                tokens_input,
                tokens_output,
                tokens_input + tokens_output, // tokens_total
                cost,
                None, // owner_id
                None, // principal_id
                failure_code,
                failure_message,
            )
            .map_err(|e| anyhow!(e.to_string()))
    }

    pub fn record_tool_call_receipt(
        &self,
        receipt_id: &str,
        user_id: &str,
        call_id: &str,
        session_id: Option<String>,
        tool_name: &str,
        status: &str,
        started_at: &str,
        completed_at: &str,
        receipt_json: &str,
        failure_message: Option<String>,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        let status = match status {
            "success" => "TOOL_SUCCESS",
            "denied" => "TOOL_DENIED",
            _ => "TOOL_FAILURE",
        };
        conn.reducers
            .record_run(
                receipt_id.to_string(),
                user_id.to_string(),
                call_id.to_string(),
                "local-tool-lease".to_string(),
                session_id,
                started_at.to_string(),
                completed_at.to_string(),
                status.to_string(),
                serde_json::json!({ "tool": tool_name, "call_id": call_id }).to_string(),
                "{}".to_string(),
                "[]".to_string(),
                "local".to_string(),
                receipt_json.to_string(),
                "tool".to_string(),
                tool_name.to_string(),
                0,
                0,
                0,
                0.0,
                None,
                None,
                failure_message
                    .as_ref()
                    .map(|_| "TOOL_CALL_FAILED".to_string()),
                failure_message,
            )
            .map_err(|e| anyhow!(e.to_string()))
    }

    // ── Doctrine Phase 1: Knowledge Plane ────────────────────────────────

    pub fn emit_source_ingested(
        &self,
        source_id: &str,
        source_kind: &str,
        uri: &str,
        content_hash: &str,
        title: &str,
        author: &str,
        owner_scope: &str,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        let now = chrono::Utc::now().to_rfc3339();
        conn.reducers
            .ingest_source(
                source_id.to_string(),
                source_kind.to_string(),
                uri.to_string(),
                content_hash.to_string(),
                title.to_string(),
                author.to_string(),
                now.clone(),         // captured_at
                String::new(),       // published_at
                "heiwa".to_string(), // retrieved_by
                owner_scope.to_string(),
                "default".to_string(), // trust_class
                String::new(),         // raw_storage_ref
                None,                  // supersedes_source_id
            )
            .map_err(|e| anyhow!(e.to_string()))
    }

    pub fn emit_page_compiled(
        &self,
        page_id: &str,
        namespace: &str,
        title: &str,
        slug: &str,
        body_markdown: &str,
        summary: &str,
        topic_keys_json: &str,
        source_set_hash: &str,
        owner_scope: &str,
        generated_by_run_id: Option<String>,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        conn.reducers
            .create_page(
                page_id.to_string(),
                namespace.to_string(),
                title.to_string(),
                slug.to_string(),
                body_markdown.to_string(),
                summary.to_string(),
                topic_keys_json.to_string(),
                source_set_hash.to_string(),
                owner_scope.to_string(),
                generated_by_run_id,
            )
            .map_err(|e| anyhow!(e.to_string()))
    }

    pub fn emit_page_recompiled(
        &self,
        page_id: &str,
        body_markdown: &str,
        summary: &str,
        topic_keys_json: &str,
        source_set_hash: &str,
        novelty_score: f64,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        conn.reducers
            .recompile_page(
                page_id.to_string(),
                body_markdown.to_string(),
                summary.to_string(),
                topic_keys_json.to_string(),
                source_set_hash.to_string(),
                novelty_score,
            )
            .map_err(|e| anyhow!(e.to_string()))
    }

    // ── Doctrine Phase 1: Belief ─────────────────────────────────────────

    pub fn emit_belief_extracted(
        &self,
        belief_id: &str,
        claim_text: &str,
        domain: &str,
        topic_keys_json: &str,
        initial_confidence: f64,
        decay_profile_id: &str,
        derived_from_page_ids_json: &str,
        derived_from_source_ids_json: &str,
        owner_scope: &str,
        generated_by_run_id: Option<String>,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        conn.reducers
            .extract_belief_candidate(
                belief_id.to_string(),
                claim_text.to_string(),
                domain.to_string(),
                topic_keys_json.to_string(),
                initial_confidence,
                decay_profile_id.to_string(),
                derived_from_page_ids_json.to_string(),
                derived_from_source_ids_json.to_string(),
                owner_scope.to_string(),
                generated_by_run_id,
            )
            .map_err(|e| anyhow!(e.to_string()))
    }

    pub fn emit_belief_promoted(&self, belief_id: &str) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        conn.reducers
            .promote_belief(belief_id.to_string())
            .map_err(|e| anyhow!(e.to_string()))
    }

    // ── Doctrine Phase 1: Treasury ───────────────────────────────────────

    pub fn emit_treasury_spend(
        &self,
        treasury_id: &str,
        requests: i64,
        tokens_in: i64,
        tokens_out: i64,
        cost_millicents: i64,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        conn.reducers
            .record_spend(
                treasury_id.to_string(),
                requests,
                tokens_in,
                tokens_out,
                cost_millicents,
            )
            .map_err(|e| anyhow!(e.to_string()))
    }

    // ── Doctrine Phase 1: Novelty ────────────────────────────────────────

    pub fn emit_novelty_vector(
        &self,
        vector_id: &str,
        run_id: &str,
        mission_id: &str,
        turn_index: u32,
        new_entity: f64,
        new_contradiction: f64,
        stronger_citation: f64,
        new_causal_connection: f64,
        changed_recommendation: f64,
        composite_score: f64,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        conn.reducers
            .record_novelty_vector(
                vector_id.to_string(),
                run_id.to_string(),
                mission_id.to_string(),
                turn_index,
                new_entity,
                new_contradiction,
                stronger_citation,
                new_causal_connection,
                changed_recommendation,
                composite_score,
            )
            .map_err(|e| anyhow!(e.to_string()))
    }

    // ── Doctrine Phase 1: Mission ────────────────────────────────────────

    pub fn emit_mission_planned(
        &self,
        mission_id: &str,
        task_class: &str,
        budget_class: &str,
        treasury_id: Option<String>,
    ) -> Result<()> {
        let Some(conn) = self.connection() else {
            return Ok(());
        };
        conn.reducers
            .plan_mission_doctrine(
                mission_id.to_string(),
                task_class.to_string(),
                budget_class.to_string(),
                treasury_id,
                None, // novelty_policy_json
                None, // termination_policy_json
                None, // required_belief_query
                None, // allowed_tools_json
                None, // allowed_side_effects_json
                None, // target_outputs_json
            )
            .map_err(|e| anyhow!(e.to_string()))
    }
}
