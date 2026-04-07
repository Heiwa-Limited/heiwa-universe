use serde::{Deserialize, Serialize};
use std::fmt;
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
    "claude", "ollama", "anthropic", "openai", "google", "gemini",
    "codex", "antigravity",
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
        "refactor", "function", "crate", "cargo", "rust", "python", "typescript",
        "javascript", "code", "bug", "test", "repo", "implement", "fix",
        "patch", "compile", "build", "cli", "adapter", "pytest", "bash", "npm",
        "struct", "enum", "trait", "dependency", "module",
    ];
    // Check for code-like sigils (but not bare `/` which is ambiguous)
    let has_code_sigil = lowercase.contains("::")
        || lowercase.contains('`')
        || lowercase.contains(".rs")
        || lowercase.contains(".py")
        || lowercase.contains(".ts")
        || lowercase.contains(".js");

    if has_code_sigil || build_keywords.iter().any(|kw| {
        // Match whole words to avoid substring collisions.
        lowercase.split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == *kw)
    }) {
        return Intent::Build;
    }

    // Deploy — shipping, infrastructure, ops
    let deploy_keywords = [
        "deploy", "ship", "release", "publish", "railway", "docker",
        "dockerfile", "ci", "cd", "pipeline", "prod", "staging",
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
        lowercase.split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == *kw)
    }) {
        return Intent::Audit;
    }

    // Strategy — high-level planning
    let strategy_keywords = [
        "strategy", "roadmap", "plan", "architecture", "design",
        "priority", "governance", "enterprise", "portfolio",
    ];
    if strategy_keywords.iter().any(|kw| lowercase.contains(kw)) {
        return Intent::Strategy;
    }

    // Research — exploration, understanding
    let research_keywords = [
        "research", "explore", "explain", "summarize", "analyze",
        "understand", "how does", "what is", "why does", "compare",
    ];
    if research_keywords.iter().any(|kw| lowercase.contains(kw)) {
        return Intent::Research;
    }

    Intent::Chat
}

