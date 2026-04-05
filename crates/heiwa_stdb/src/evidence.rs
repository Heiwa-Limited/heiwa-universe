//! Evidence emission helpers — thin wrappers around SpacetimeDB reducers.
//!
//! Every method is a no-op when the client is offline (`connection()` returns
//! `None`), which lets callers fire-and-forget without branching.

use anyhow::{anyhow, Result};

use heiwa_bindings::register_device_reducer::register_device;
use heiwa_bindings::update_device_heartbeat_reducer::update_device_heartbeat;
use heiwa_bindings::upsert_provider_account_status_reducer::upsert_provider_account_status;
use heiwa_bindings::record_route_decision_reducer::record_route_decision;
use heiwa_bindings::record_run_reducer::record_run;

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
                "local".to_string(),              // rate_group
                available_models_json.to_string(),
                None,                             // last_validated_at
                None,                             // last_error
                now,                              // updated_at
                None,                             // user_id
                None,                             // owner_id
                None,                             // principal_id
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
                "v1".to_string(),                 // envelope_version
                raw_text.to_string(),
                "terminal".to_string(),           // source_surface
                intent_class.to_string(),
                risk_level.to_string(),
                privacy_level.to_string(),
                0,                                // compute_class
                assigned_worker.to_string(),
                target_tool.to_string(),
                target_model.to_string(),
                target_runtime.to_string(),
                "auto".to_string(),               // target_tier
                false,                            // requires_approval
                rationale.to_string(),
                confidence,
                "local".to_string(),              // gateway_transport
                now,                              // created_at
                None,                             // user_id
                None,                             // owner_id
                None,                             // principal_id
                None,                             // drex_decision_id
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
                "local-lease".to_string(),        // lease_id
                session_id,
                started_at.to_string(),
                ended_at.to_string(),
                status.to_string(),
                "{}".to_string(),                 // chain_result_json
                "{}".to_string(),                 // signals_json
                "[]".to_string(),                 // artifact_index_json
                "local".to_string(),              // node_id
                "{}".to_string(),                 // replay_receipt_json
                "terminal".to_string(),           // mode
                model_id.to_string(),
                tokens_input,
                tokens_output,
                tokens_input + tokens_output,     // tokens_total
                cost,
                None,                             // owner_id
                None,                             // principal_id
                failure_code,
                failure_message,
            )
            .map_err(|e| anyhow!(e.to_string()))
    }
}
