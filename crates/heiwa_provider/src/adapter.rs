use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Message types (provider-normalized)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

// ---------------------------------------------------------------------------
// Stream events — the normalized contract between adapters and consumers
// ---------------------------------------------------------------------------

/// Events emitted by an adapter during model inference.
///
/// Consumers receive these through an `mpsc::Receiver<StreamEvent>`.
/// The stream always ends with either `Done` or `Error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    /// A chunk of text output from the model.
    Token(String),

    /// The model is invoking a tool / function call.
    ToolUse { name: String, input: String },

    /// The response is complete.  Carries usage metadata.
    Done(TokenUsage),

    /// An error occurred during generation.
    Error(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
    pub cost_usd: f64,
}

// ---------------------------------------------------------------------------
// Adapter trait
// ---------------------------------------------------------------------------

/// A provider adapter that can execute model inference.
///
/// Adapters are constructed per-account and know which credential / endpoint
/// to use.  Two implementation families exist:
///
/// - **HTTP API adapters** — call provider APIs directly using API keys or
///   OAuth tokens.  This is the primary BYOK path.
/// - **CLI process adapters** — wrap an installed provider CLI subprocess.
///   This is the optional bonus path for subscription users.
///
/// Uses `async_trait` for object safety — DREX routing needs `dyn` dispatch
/// since the concrete adapter type is determined at runtime.
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Send a conversation to the model and stream events back.
    ///
    /// `model` is the provider_model_id (e.g. "claude-sonnet-4-20250514").
    /// `messages` is the conversation history.
    /// `stream_tx` receives normalized `StreamEvent`s as they arrive.
    ///
    /// The method returns when the response is complete or an error occurs.
    /// It must send exactly one `Done` or `Error` event before returning.
    async fn send(
        &self,
        model: &str,
        messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()>;

    /// Interrupt an in-progress generation.  Best-effort — the adapter
    /// should attempt to cancel but may not guarantee immediate stop.
    async fn interrupt(&self) -> anyhow::Result<()>;

    /// Provider model IDs this adapter can serve.
    fn supported_models(&self) -> Vec<String>;
}
