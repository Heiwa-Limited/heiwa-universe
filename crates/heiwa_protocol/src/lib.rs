use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Doctrine enums — canonical string representations for STDB fields
// ---------------------------------------------------------------------------

/// Generates a doctrine enum with Display, FromStr, Serialize, Deserialize.
/// STDB stores these as strings; Rust code uses typed enums.
macro_rules! doctrine_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $str:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        $vis enum $name {
            $( $(#[$vmeta])* $variant ),+
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let s = match self {
                    $( $name::$variant => $str ),+
                };
                f.write_str(s)
            }
        }

        impl FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $( $str => Ok($name::$variant), )+
                    _ => Err(format!("invalid {}: {}", stringify!($name), s)),
                }
            }
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$( $name::$variant ),+];

            pub fn as_str(&self) -> &'static str {
                match self {
                    $( $name::$variant => $str ),+
                }
            }
        }
    };
}

doctrine_enum! {
    pub enum BeliefStatus {
        Candidate => "candidate",
        Supported => "supported",
        Durable => "durable",
        Contested => "contested",
        Stale => "stale",
        Retired => "retired",
        False => "false",
    }
}

doctrine_enum! {
    pub enum PageStatus {
        Draft => "draft",
        Active => "active",
        Stale => "stale",
        Superseded => "superseded",
        Retired => "retired",
    }
}

doctrine_enum! {
    pub enum MissionTaskClass {
        Compile => "compile",
        Consolidate => "consolidate",
        Act => "act",
    }
}

doctrine_enum! {
    pub enum MissionBudgetClass {
        Cheap => "cheap",
        Standard => "standard",
        Premium => "premium",
    }
}

doctrine_enum! {
    pub enum ApprovalState {
        NoneNeeded => "none_needed",
        Pending => "pending",
        Granted => "granted",
        Denied => "denied",
    }
}

doctrine_enum! {
    pub enum TreasuryHealthState {
        Healthy => "healthy",
        Guarded => "guarded",
        Degraded => "degraded",
        Cooldown => "cooldown",
        Exhausted => "exhausted",
    }
}

doctrine_enum! {
    pub enum TreasuryScope {
        User => "user",
        Org => "org",
        Device => "device",
        ProviderAccount => "provider_account",
    }
}

doctrine_enum! {
    pub enum BudgetWindowKind {
        Hour => "hour",
        Day => "day",
        Month => "month",
        Rolling => "rolling",
    }
}

doctrine_enum! {
    pub enum ReservePolicy {
        Strict => "strict",
        Soft => "soft",
        None => "none",
    }
}

doctrine_enum! {
    pub enum ReservationStatus {
        Held => "held",
        Consumed => "consumed",
        Released => "released",
        Expired => "expired",
    }
}

doctrine_enum! {
    pub enum TreasuryDecision {
        Allow => "allow",
        AllowDowngraded => "allow_downgraded",
        Defer => "defer",
        Deny => "deny",
        RequireApproval => "require_approval",
    }
}

doctrine_enum! {
    pub enum ContradictionResolution {
        Open => "open",
        ResolvedPrimaryWins => "resolved_primary_wins",
        ResolvedChallengerWins => "resolved_challenger_wins",
        BothRetired => "both_retired",
        Merged => "merged",
    }
}

doctrine_enum! {
    pub enum EvidenceLinkType {
        Supports => "supports",
        Contradicts => "contradicts",
        DerivedFrom => "derived_from",
        VerifiedBy => "verified_by",
    }
}

doctrine_enum! {
    pub enum SourceKind {
        Web => "web",
        Pdf => "pdf",
        RepoFile => "repo_file",
        Conversation => "conversation",
        Api => "api",
        Note => "note",
        Dataset => "dataset",
    }
}

doctrine_enum! {
    pub enum ParseStatus {
        Pending => "pending",
        Parsed => "parsed",
        Failed => "failed",
    }
}

doctrine_enum! {
    /// Risk class for a `ToolLease`. Drives DREX policy routing:
    /// `HostSafeReadonly` runs on the host without sandboxing,
    /// `HostMutating` runs on the host but writes to scope-permitted paths,
    /// `SandboxRequired` must be dispatched to an E2B (or equivalent) sandbox.
    pub enum RiskClass {
        HostSafeReadonly => "host_safe_readonly",
        HostMutating => "host_mutating",
        SandboxRequired => "sandbox_required",
    }
}

// ---------------------------------------------------------------------------
// Cockpit event contract — controller <-> TUI communication
// ---------------------------------------------------------------------------

/// Events sent from the controller to the TUI for rendering.
#[derive(Debug, Clone)]
pub enum CockpitEvent {
    /// Append a completed block to the transcript.
    TranscriptAppend(TranscriptBlock),
    /// Update the routing display.
    RoutingUpdate(RoutingState),
    /// A streamed token fragment from the current assistant response.
    StreamToken(String),
    /// The current stream finished with usage stats.
    StreamDone {
        tokens_in: i64,
        tokens_out: i64,
        cost: f64,
    },
    /// The current stream hit an error.
    StreamError(String),
    /// Update the footer status text.
    StatusUpdate(String),
}

/// Commands sent from the TUI to the controller.
#[derive(Debug, Clone)]
pub enum CockpitCommand {
    /// User submitted input from the composer.
    SubmitInput(String),
    /// User requested exit.
    Quit,
}

// ---------------------------------------------------------------------------
// State structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub transcript: Vec<TranscriptBlock>,
    pub routing: RoutingState,
    pub devices: Vec<DeviceSummary>,
    pub receipts: Vec<RunReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TranscriptBlock {
    User(String),
    Assistant(String),
    Tool(String, String), // name, output
    Evidence(String),     // JSON or summary
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingState {
    pub current_provider: String,
    pub current_model: String,
    pub mode: String, // "Auto", "Manual", "Pinned"
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSummary {
    pub id: String,
    pub hostname: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReceipt {
    pub id: String,
    pub provider: String,
    pub cost: f64,
    pub tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Success,
    Failure,
    Denied,
}

impl ToolCallStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Denied => "denied",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallReceipt {
    pub id: String,
    pub call_id: String,
    pub provider: String,
    pub model_id: String,
    pub tool_name: String,
    pub status: ToolCallStatus,
    pub started_at: String,
    pub completed_at: String,
    pub arguments: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NetworkPolicy {
    Deny,
    LocalOnly,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SandboxMode {
    Host,
    Worktree,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolLease {
    pub name: String,
    pub risk_class: RiskClass,
    pub allowed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PrincipalKind {
    HumanUser,
    Agent,
    System,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionRole {
    Owner,
    Operator,
    Agent,
    Auditor,
    Viewer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Permission {
    ReadSessionContext,
    WriteTranscript,
    RouteModel,
    ExecuteModel,
    UseTool,
    RunShell,
    WriteFilesystem,
    NetworkAccess,
    ManageSession,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionPrincipal {
    pub id: String,
    pub kind: PrincipalKind,
    pub role: ExecutionRole,
}

impl SessionPrincipal {
    pub fn new(id: impl Into<String>, kind: PrincipalKind, role: ExecutionRole) -> Self {
        Self {
            id: id.into(),
            kind,
            role,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Deny { reason: String },
}

impl PermissionDecision {
    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny {
            reason: reason.into(),
        }
    }

    pub fn is_allowed(&self) -> bool {
        matches!(self, PermissionDecision::Allow)
    }

    pub fn reason(&self) -> &str {
        match self {
            PermissionDecision::Allow => "allow",
            PermissionDecision::Deny { reason } => reason,
        }
    }
}

impl ExecutionRole {
    pub fn allows(self, permission: Permission) -> bool {
        match self {
            ExecutionRole::Owner => true,
            ExecutionRole::Operator => !matches!(permission, Permission::ManageSession),
            ExecutionRole::Agent => matches!(
                permission,
                Permission::ReadSessionContext
                    | Permission::WriteTranscript
                    | Permission::RouteModel
                    | Permission::ExecuteModel
                    | Permission::UseTool
                    | Permission::RunShell
                    | Permission::WriteFilesystem
            ),
            ExecutionRole::Auditor => {
                matches!(
                    permission,
                    Permission::ReadSessionContext | Permission::RouteModel
                )
            }
            ExecutionRole::Viewer => matches!(permission, Permission::ReadSessionContext),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionScope {
    pub working_dir: PathBuf,
    pub allowed_dirs: Vec<PathBuf>,
    pub writable_dirs: Vec<PathBuf>,
    pub network_policy: NetworkPolicy,
    pub sandbox_mode: SandboxMode,
    pub tool_leases: Vec<ToolLease>,
}

impl ExecutionScope {
    pub fn local_default(working_dir: PathBuf) -> Self {
        let working_dir = canonicalize_existing_dir(&working_dir).unwrap_or(working_dir);
        Self {
            working_dir: working_dir.clone(),
            allowed_dirs: vec![working_dir.clone()],
            writable_dirs: vec![working_dir],
            network_policy: NetworkPolicy::Deny,
            sandbox_mode: SandboxMode::Host,
            tool_leases: Vec::new(),
        }
    }

    pub fn set_working_dir(&mut self, path: PathBuf) {
        let path = canonicalize_existing_dir(&path).unwrap_or(path);
        self.working_dir = path.clone();
        self.add_allowed_dir(path.clone());
        self.add_writable_dir(path);
    }

    pub fn add_allowed_dir(&mut self, path: PathBuf) -> bool {
        add_unique_dir(&mut self.allowed_dirs, path)
    }

    pub fn add_writable_dir(&mut self, path: PathBuf) -> bool {
        add_unique_dir(&mut self.writable_dirs, path)
    }

    pub fn allows_path(&self, path: &Path) -> bool {
        path_within_roots(path, &self.allowed_dirs)
    }

    pub fn allows_write_path(&self, path: &Path) -> bool {
        path_within_roots(path, &self.writable_dirs)
    }

    pub fn allows_tool(&self, name: &str) -> bool {
        self.tool_leases
            .iter()
            .any(|lease| lease.allowed && lease.name == name)
    }

    pub fn authorize(
        &self,
        principal: &SessionPrincipal,
        permission: Permission,
    ) -> PermissionDecision {
        if !principal.role.allows(permission) {
            return PermissionDecision::deny(format!(
                "role {:?} lacks permission {:?}",
                principal.role, permission
            ));
        }

        match permission {
            Permission::NetworkAccess if self.network_policy == NetworkPolicy::Deny => {
                PermissionDecision::deny("network policy denies access")
            }
            _ => PermissionDecision::Allow,
        }
    }

    pub fn authorize_tool(
        &self,
        principal: &SessionPrincipal,
        tool_name: &str,
        permission: Permission,
    ) -> PermissionDecision {
        let tool_gate = self.authorize(principal, Permission::UseTool);
        if !tool_gate.is_allowed() {
            return tool_gate;
        }

        let permission_gate = self.authorize(principal, permission);
        if !permission_gate.is_allowed() {
            return permission_gate;
        }

        if !self.allows_tool(tool_name) {
            return PermissionDecision::deny(format!("tool lease missing or denied: {tool_name}"));
        }

        PermissionDecision::Allow
    }
}

fn add_unique_dir(roots: &mut Vec<PathBuf>, path: PathBuf) -> bool {
    let path = canonicalize_existing_dir(&path).unwrap_or(path);
    if roots.iter().any(|root| root == &path) {
        return false;
    }
    roots.push(path);
    true
}

fn path_within_roots(path: &Path, roots: &[PathBuf]) -> bool {
    let Some(path) = canonicalize_for_scope_check(path) else {
        return false;
    };
    roots.iter().any(|root| path.starts_with(root))
}

fn canonicalize_existing_dir(path: &Path) -> Option<PathBuf> {
    path.canonicalize().ok().filter(|path| path.is_dir())
}

fn canonicalize_for_scope_check(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return path.canonicalize().ok();
    }

    let parent = path.parent()?;
    let file_name = path.file_name()?;
    parent
        .canonicalize()
        .ok()
        .map(|parent| parent.join(file_name))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Intent {
    Chat,
    Build,
    Deploy,
    Audit,
    Research,
    Strategy,
    StatusCheck,
}

impl Intent {
    /// Map to the key used by DREX's vector builder in `build_drex_vector()`.
    pub fn as_drex_key(&self) -> &'static str {
        match self {
            Intent::Chat => "chat",
            Intent::Build => "build",
            Intent::Deploy => "deploy",
            Intent::Audit => "audit",
            Intent::Research => "research",
            Intent::Strategy => "strategy",
            Intent::StatusCheck => "status_check",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRequest {
    pub raw_text: String,
    pub provider_pin: Option<String>,
    pub model_pin: Option<String>,
    pub intent: Intent,
}

/// Known provider names for pin extraction.
const KNOWN_PROVIDERS: &[&str] = &[
    "claude",
    "ollama",
    "anthropic",
    "openai",
    "google",
    "gemini",
    "codex",
    "antigravity",
];

pub fn parse_turn_intent(input: &str) -> TurnRequest {
    let lowercase = input.to_lowercase();
    let parts: Vec<&str> = lowercase.split_whitespace().collect();

    // --- Provider / model pin extraction ---
    // Patterns: "use <provider> [model] ...", "with <provider> [model] ...",
    //           "using <provider> [model] ..."
    let mut provider_pin = None;
    let mut model_pin = None;

    for keyword in &["use", "with", "using"] {
        if let Some(pos) = parts.iter().position(|&x| x == *keyword) {
            if let Some(candidate) = parts.get(pos + 1) {
                if KNOWN_PROVIDERS.contains(candidate) {
                    provider_pin = Some((*candidate).to_string());
                    // Next token after provider is a model pin only if it
                    // looks like a model identifier (contains digits, dashes,
                    // or dots — e.g. "opus-4.6", "sonnet-4", "qwen3:8b").
                    if let Some(maybe_model) = parts.get(pos + 2) {
                        let looks_like_model = maybe_model.chars().any(|c| c.is_ascii_digit())
                            || maybe_model.contains('-')
                            || maybe_model.contains('.')
                            || maybe_model.contains(':');
                        if looks_like_model {
                            model_pin = Some((*maybe_model).to_string());
                        }
                    }
                    break;
                }
            }
        }
    }

    // --- Intent classification ---
    let intent = classify_intent(&lowercase);

    TurnRequest {
        raw_text: input.to_string(),
        provider_pin,
        model_pin,
        intent,
    }
}

fn classify_intent(lowercase: &str) -> Intent {
    // Build / code — file-level, repo-level, or tool-level coding work.
    // Only include words strongly associated with programming tasks.
    let build_keywords = [
        "refactor",
        "function",
        "crate",
        "cargo",
        "rust",
        "python",
        "typescript",
        "javascript",
        "code",
        "bug",
        "test",
        "repo",
        "implement",
        "fix",
        "patch",
        "compile",
        "build",
        "cli",
        "adapter",
        "pytest",
        "bash",
        "npm",
        "struct",
        "enum",
        "trait",
        "dependency",
        "module",
    ];
    // Check for code-like sigils (but not bare `/` which is ambiguous)
    let has_code_sigil = lowercase.contains("::")
        || lowercase.contains('`')
        || lowercase.contains(".rs")
        || lowercase.contains(".py")
        || lowercase.contains(".ts")
        || lowercase.contains(".js");

    if has_code_sigil
        || build_keywords.iter().any(|kw| {
            // Match whole words to avoid substring collisions.
            lowercase
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word == *kw)
        })
    {
        return Intent::Build;
    }

    // Deploy — shipping, infrastructure, ops
    let deploy_keywords = [
        "deploy",
        "ship",
        "release",
        "publish",
        "cloudflare",
        "docker",
        "dockerfile",
        "ci",
        "cd",
        "pipeline",
        "prod",
        "staging",
    ];
    if deploy_keywords.iter().any(|kw| lowercase.contains(kw)) {
        return Intent::Deploy;
    }

    // StatusCheck — system health (before Audit so "check status" wins)
    let status_keywords = ["status", "health", "uptime", "heartbeat"];
    if status_keywords.iter().any(|kw| lowercase.contains(kw)) {
        return Intent::StatusCheck;
    }

    // Audit — review, inspection, checking
    let audit_keywords = ["audit", "lint", "review", "scan", "inspect"];
    if audit_keywords.iter().any(|kw| {
        lowercase
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == *kw)
    }) {
        return Intent::Audit;
    }

    // Strategy — high-level planning
    let strategy_keywords = [
        "strategy",
        "roadmap",
        "plan",
        "architecture",
        "design",
        "priority",
        "governance",
        "enterprise",
        "portfolio",
    ];
    if strategy_keywords.iter().any(|kw| lowercase.contains(kw)) {
        return Intent::Strategy;
    }

    // Research — exploration, understanding
    let research_keywords = [
        "research",
        "explore",
        "explain",
        "summarize",
        "analyze",
        "understand",
        "how does",
        "what is",
        "why does",
        "compare",
    ];
    if research_keywords.iter().any(|kw| lowercase.contains(kw)) {
        return Intent::Research;
    }

    Intent::Chat
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn execution_scope_allows_paths_inside_registered_roots() {
        let root = std::env::temp_dir().join(format!(
            "heiwa-scope-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();

        let scope = ExecutionScope::local_default(root.clone());

        assert!(scope.allows_path(&child));
        assert!(scope.allows_write_path(&root.join("new.txt")));
        assert!(!scope.allows_path(Path::new("/")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn execution_scope_tracks_tool_leases() {
        let root = std::env::current_dir().unwrap();
        let mut scope = ExecutionScope::local_default(root);
        scope.tool_leases.push(ToolLease {
            name: "shell".into(),
            risk_class: RiskClass::HostMutating,
            allowed: true,
        });

        assert!(scope.allows_tool("shell"));
        assert!(!scope.allows_tool("network"));
    }

    #[test]
    fn tool_call_status_has_stable_wire_values() {
        assert_eq!(ToolCallStatus::Success.as_str(), "success");
        assert_eq!(ToolCallStatus::Failure.as_str(), "failure");
        assert_eq!(ToolCallStatus::Denied.as_str(), "denied");
    }
}
