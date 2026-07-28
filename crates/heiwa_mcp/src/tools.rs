//! Built-in Heiwa MCP tools: `list_providers`, `get_quota_status`,
//! `route_request`.
//!
//! Each tool is a thin adapter over a trait the hosting runtime implements.
//! Fake implementations are provided in this module behind `cfg(test)` plus
//! plain `pub` so integration crates can exercise the wiring without needing
//! the real provider/quota/router stack.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::{schema::RootSchema, schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{McpError, Result, Tool};

// ─── traits hosting runtime implements ────────────────────────────────────

pub trait ProviderSource: Send + Sync {
    fn list(&self) -> Vec<ProviderInfo>;
}

pub trait QuotaProvider: Send + Sync {
    fn status(&self, provider: &str, rate_group: &str) -> Option<QuotaSnapshot>;
}

#[async_trait]
pub trait Router: Send + Sync {
    async fn route(&self, req: RouteInput) -> std::result::Result<RouteDecision, String>;
}

// ─── shared payload types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProviderInfo {
    pub id: String,
    pub kind: String,
    pub rate_group: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct QuotaSnapshot {
    pub provider: String,
    pub rate_group: String,
    pub window_start_unix: i64,
    pub window_seconds: i64,
    pub tokens_used: i64,
    pub requests: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct RouteDecision {
    pub provider: String,
    pub model_id: String,
    pub rationale: String,
}

// ─── list_providers ───────────────────────────────────────────────────────

pub struct ListProviders {
    source: Arc<dyn ProviderSource>,
}

impl ListProviders {
    pub fn new(source: Arc<dyn ProviderSource>) -> Self {
        Self { source }
    }
}

#[derive(Deserialize, JsonSchema, Default)]
#[serde(default)]
pub struct ListProvidersInput {
    pub include_disabled: bool,
}

#[async_trait]
impl Tool for ListProviders {
    fn name(&self) -> &'static str {
        "list_providers"
    }
    fn description(&self) -> &'static str {
        "Enumerate routing targets the Heiwa runtime knows about, with rate-group and enabled state."
    }
    fn input_schema(&self) -> RootSchema {
        schema_for!(ListProvidersInput)
    }
    async fn call(&self, args: Value) -> Result<Value> {
        let input: ListProvidersInput = if args.is_null() {
            ListProvidersInput::default()
        } else {
            serde_json::from_value(args).map_err(|source| McpError::InvalidArguments {
                tool: self.name().to_string(),
                source,
            })?
        };
        let providers: Vec<_> = self
            .source
            .list()
            .into_iter()
            .filter(|p| input.include_disabled || p.enabled)
            .collect();
        Ok(json!({ "providers": providers }))
    }
}

// ─── get_quota_status ─────────────────────────────────────────────────────

pub struct GetQuotaStatus {
    quota: Arc<dyn QuotaProvider>,
}

impl GetQuotaStatus {
    pub fn new(quota: Arc<dyn QuotaProvider>) -> Self {
        Self { quota }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct GetQuotaStatusInput {
    pub provider: String,
    pub rate_group: String,
}

#[async_trait]
impl Tool for GetQuotaStatus {
    fn name(&self) -> &'static str {
        "get_quota_status"
    }
    fn description(&self) -> &'static str {
        "Read the current quota window for a provider + rate group from the local SQLite ledger."
    }
    fn input_schema(&self) -> RootSchema {
        schema_for!(GetQuotaStatusInput)
    }
    async fn call(&self, args: Value) -> Result<Value> {
        let input: GetQuotaStatusInput =
            serde_json::from_value(args).map_err(|source| McpError::InvalidArguments {
                tool: self.name().to_string(),
                source,
            })?;
        match self.quota.status(&input.provider, &input.rate_group) {
            Some(s) => Ok(serde_json::to_value(s).expect("serialize quota snapshot")),
            None => Ok(
                json!({ "provider": input.provider, "rate_group": input.rate_group, "state": null }),
            ),
        }
    }
}

// ─── route_request ────────────────────────────────────────────────────────

pub struct RouteRequest {
    router: Arc<dyn Router>,
}

impl RouteRequest {
    pub fn new(router: Arc<dyn Router>) -> Self {
        Self { router }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RouteInput {
    pub intent: String,
    pub prompt: String,
    #[serde(default)]
    pub hints: serde_json::Value,

    /// Minimum capability class the task actually needs (1=small/fast .. 5=frontier).
    ///
    /// `intent` is a *category*; this is a *magnitude*. "Summarise this line" and
    /// "design this subsystem" are both `chat`, and no amount of downstream
    /// scoring can recover the difference if the input never carried it.
    ///
    /// When absent the router infers a floor from `intent`. Callers that know
    /// better should say so.
    #[serde(default)]
    pub min_capability: Option<u8>,

    /// Hard ceiling on estimated USD for this single turn. Candidates whose
    /// estimate exceeds it are rejected rather than silently chosen.
    #[serde(default)]
    pub max_cost_usd: Option<f64>,

    /// Expected output size, for cost estimation. Defaults to
    /// [`DEFAULT_OUTPUT_TOKENS`] when the caller has no better guess.
    #[serde(default)]
    pub est_output_tokens: Option<u32>,
}

/// Assumed completion length when a caller gives no estimate.
pub const DEFAULT_OUTPUT_TOKENS: u32 = 800;

impl RouteInput {
    /// Rough token count for a prompt. Deliberately crude — this feeds a
    /// *comparison* between candidates, not a billing figure, and every
    /// candidate is measured the same way so the bias cancels out.
    pub fn est_input_tokens(&self) -> u32 {
        (self.prompt.len() / 4).max(1) as u32
    }

    pub fn output_tokens(&self) -> u32 {
        self.est_output_tokens.unwrap_or(DEFAULT_OUTPUT_TOKENS)
    }

    /// Capability floor for this turn: explicit if given, else inferred from intent.
    ///
    /// The inferred defaults are deliberately conservative — routing a hard task
    /// to a small model wastes a whole turn discovering it could not do the job,
    /// which costs more than the model tier it was trying to save.
    pub fn required_capability(&self) -> u8 {
        if let Some(c) = self.min_capability {
            return c.clamp(1, 5);
        }
        match self.intent.to_ascii_lowercase().as_str() {
            "trivial" | "classify" | "extract" | "format" => 1,
            "chat" | "summarize" | "summarise" => 2,
            "code" | "review" | "research" | "general" => 3,
            "architect" | "design" | "strategy" | "audit" => 4,
            _ => 2,
        }
    }
}

#[async_trait]
impl Tool for RouteRequest {
    fn name(&self) -> &'static str {
        "route_request"
    }
    fn description(&self) -> &'static str {
        "Pick a provider and model for a prompt given an intent classification. Returns the decision only — does not execute."
    }
    fn input_schema(&self) -> RootSchema {
        schema_for!(RouteInput)
    }
    async fn call(&self, args: Value) -> Result<Value> {
        let input: RouteInput =
            serde_json::from_value(args).map_err(|source| McpError::InvalidArguments {
                tool: self.name().to_string(),
                source,
            })?;
        self.router
            .route(input)
            .await
            .map(|d| serde_json::to_value(d).expect("serialize decision"))
            .map_err(McpError::Tool)
    }
}

// ─── test fakes — exposed for integration tests ───────────────────────────

#[derive(Default)]
pub struct FakeProviders {
    pub items: Vec<ProviderInfo>,
}

impl ProviderSource for FakeProviders {
    fn list(&self) -> Vec<ProviderInfo> {
        if self.items.is_empty() {
            vec![
                ProviderInfo {
                    id: "claude-code".into(),
                    kind: "oauth-cli".into(),
                    rate_group: "anthropic".into(),
                    enabled: true,
                },
                ProviderInfo {
                    id: "ollama".into(),
                    kind: "local".into(),
                    rate_group: "local".into(),
                    enabled: true,
                },
                ProviderInfo {
                    id: "codex".into(),
                    kind: "oauth-cli".into(),
                    rate_group: "openai".into(),
                    enabled: false,
                },
            ]
        } else {
            self.items.clone()
        }
    }
}

#[derive(Default)]
pub struct FakeQuota;

impl QuotaProvider for FakeQuota {
    fn status(&self, provider: &str, rate_group: &str) -> Option<QuotaSnapshot> {
        (provider == "claude-code" && rate_group == "anthropic").then(|| QuotaSnapshot {
            provider: provider.into(),
            rate_group: rate_group.into(),
            window_start_unix: 1_700_000_000,
            window_seconds: 3_600,
            tokens_used: 12_345,
            requests: 27,
        })
    }
}

#[derive(Default)]
pub struct FakeRouter;

#[async_trait]
impl Router for FakeRouter {
    async fn route(&self, req: RouteInput) -> std::result::Result<RouteDecision, String> {
        if req.intent.is_empty() {
            return Err("intent is required".into());
        }
        let provider = match req.intent.as_str() {
            "local" | "fast" => "ollama",
            _ => "claude-code",
        };
        Ok(RouteDecision {
            provider: provider.into(),
            model_id: "claude-opus-4-7".into(),
            rationale: format!("intent={}", req.intent),
        })
    }
}

#[cfg(test)]
mod tool_tests {
    use super::*;

    #[tokio::test]
    async fn list_providers_filters_disabled_by_default() {
        let tool = ListProviders::new(Arc::new(FakeProviders::default()));
        let out = tool.call(Value::Null).await.unwrap();
        let providers = out["providers"].as_array().unwrap();
        assert_eq!(providers.len(), 2);
        assert!(providers.iter().all(|p| p["enabled"] == true));
    }

    #[tokio::test]
    async fn list_providers_includes_disabled_when_requested() {
        let tool = ListProviders::new(Arc::new(FakeProviders::default()));
        let out = tool
            .call(json!({ "include_disabled": true }))
            .await
            .unwrap();
        assert_eq!(out["providers"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn get_quota_status_hit_and_miss() {
        let tool = GetQuotaStatus::new(Arc::new(FakeQuota));
        let hit = tool
            .call(json!({ "provider": "claude-code", "rate_group": "anthropic" }))
            .await
            .unwrap();
        assert_eq!(hit["tokens_used"], 12_345);

        let miss = tool
            .call(json!({ "provider": "unknown", "rate_group": "none" }))
            .await
            .unwrap();
        assert!(miss["state"].is_null());
    }

    #[tokio::test]
    async fn route_request_dispatches_by_intent() {
        let tool = RouteRequest::new(Arc::new(FakeRouter));
        let local = tool
            .call(json!({ "intent": "local", "prompt": "hi" }))
            .await
            .unwrap();
        assert_eq!(local["provider"], "ollama");

        let pair = tool
            .call(json!({ "intent": "pair", "prompt": "review this" }))
            .await
            .unwrap();
        assert_eq!(pair["provider"], "claude-code");
    }

    #[tokio::test]
    async fn route_request_surfaces_router_error() {
        let tool = RouteRequest::new(Arc::new(FakeRouter));
        let err = tool
            .call(json!({ "intent": "", "prompt": "x" }))
            .await
            .unwrap_err();
        assert!(matches!(err, McpError::Tool(_)));
    }

    #[tokio::test]
    async fn invalid_arguments_produce_typed_error() {
        let tool = GetQuotaStatus::new(Arc::new(FakeQuota));
        let err = tool.call(json!({ "provider": 123 })).await.unwrap_err();
        assert!(matches!(err, McpError::InvalidArguments { .. }));
    }
}
