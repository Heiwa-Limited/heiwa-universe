//! Model Context Protocol scaffold for Heiwa.
//!
//! This crate defines the *shape* of Heiwa's MCP surface — tools, registry,
//! and dispatch — decoupled from any specific transport. A follow-up PR binds
//! a stdio / SSE transport (likely `rmcp`) so L4's DREX router can speak MCP
//! to this server, and this server can speak MCP client-side to provider
//! MCP servers (Claude Code, Gemini CLI, Codex).
//!
//! The three tools that L4 needs are stubbed end-to-end:
//! - `list_providers` — enumerate configured routing targets
//! - `get_quota_status` — read current rate-limit window from the quota ledger
//! - `route_request` — pick a provider for an intent + forward the request
//!
//! For each, inputs are typed and validated via JSON Schema. Outputs are
//! plain JSON values for now; when we wire `rmcp`, these become `Content`
//! payloads.

pub mod local_tools;
pub mod tools;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use schemars::schema::RootSchema;
use serde_json::Value;
use thiserror::Error;

pub use local_tools::{local_repo_registry, FsList, FsRead, RepoGrep};
pub use tools::{GetQuotaStatus, ListProviders, ProviderSource, QuotaProvider, RouteRequest};

/// Typed reason for a policy denial. Driven by `ExecutionScope` checks in
/// `heiwa_protocol`. Each variant maps to a distinct operator-visible
/// failure mode so the audit trail can tell why a tool was blocked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDenial {
    /// No matching allowed `ToolLease` for this tool name.
    MissingLease { tool: String },
    /// Resolved path is outside the active `allowed_dirs`.
    OutsideExecutionScope { path: PathBuf },
}

impl std::fmt::Display for PolicyDenial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLease { tool } => write!(f, "tool lease missing for {tool}"),
            Self::OutsideExecutionScope { path } => {
                write!(f, "outside execution scope: {}", path.display())
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum McpError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid arguments for {tool}: {source}")]
    InvalidArguments {
        tool: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("policy denied: {0}")]
    PolicyDenied(PolicyDenial),
    #[error("tool call failed: {0}")]
    Tool(String),
}

impl McpError {
    /// True if this is a policy gate rejection (lease missing or scope
    /// violation), as opposed to a tool-level failure.
    pub fn is_policy_denial(&self) -> bool {
        matches!(self, Self::PolicyDenied(_))
    }
}

pub type Result<T> = std::result::Result<T, McpError>;

/// One tool exposed over MCP. Implementations own the input type and its
/// schema; the registry is type-erased over `Value` in and `Value` out.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> RootSchema;
    async fn call(&self, args: Value) -> Result<Value>;
}

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<&'static str, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) -> &mut Self {
        self.tools.insert(tool.name(), Arc::new(tool));
        self
    }

    pub fn names(&self) -> Vec<&'static str> {
        let mut names: Vec<_> = self.tools.keys().copied().collect();
        names.sort_unstable();
        names
    }

    pub fn schema(&self, name: &str) -> Option<RootSchema> {
        self.tools.get(name).map(|t| t.input_schema())
    }

    pub fn description(&self, name: &str) -> Option<&'static str> {
        self.tools.get(name).map(|t| t.description())
    }

    pub async fn call(&self, name: &str, args: Value) -> Result<Value> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| McpError::UnknownTool(name.to_string()))?;
        tool.call(args).await
    }
}

/// Build a registry preloaded with Heiwa's built-in tools. Providers supply
/// the dynamic data needed (routing targets, quota reads, request dispatch)
/// via the `QuotaProvider` + `ProviderSource` + `Router` traits.
pub fn default_registry(
    providers: Arc<dyn ProviderSource>,
    quota: Arc<dyn QuotaProvider>,
    router: Arc<dyn tools::Router>,
) -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    reg.register(ListProviders::new(providers));
    reg.register(GetQuotaStatus::new(quota));
    reg.register(RouteRequest::new(router));
    reg
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tools::{FakeProviders, FakeQuota, FakeRouter};

    #[tokio::test]
    async fn registry_lists_and_dispatches() {
        let reg = default_registry(
            Arc::new(FakeProviders::default()),
            Arc::new(FakeQuota),
            Arc::new(FakeRouter),
        );
        assert_eq!(
            reg.names(),
            vec!["get_quota_status", "list_providers", "route_request"]
        );
        assert!(reg.schema("list_providers").is_some());
        assert!(!reg.description("list_providers").unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_tool_errors() {
        let reg = ToolRegistry::new();
        let err = reg.call("nope", Value::Null).await.unwrap_err();
        assert!(matches!(err, McpError::UnknownTool(_)));
    }
}
