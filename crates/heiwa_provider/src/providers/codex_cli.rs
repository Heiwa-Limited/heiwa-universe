use crate::adapter::{Message, ProviderAdapter, ProviderTools, StreamEvent};
use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Supported Codex app-server transport using the user's provider-owned login.
pub struct CodexCliAdapter {
    binary: String,
    model: String,
}

impl CodexCliAdapter {
    pub(crate) fn from_admitted_lane(lane: &crate::admission::RegistryAdmittedLane) -> Self {
        let crate::Credential::OauthCli { binary } = &lane.account().credential else {
            unreachable!("admission validates CLI credential kind");
        };
        Self {
            binary: binary.clone(),
            model: lane.provider_model_id().to_string(),
        }
    }
}

#[async_trait]
impl ProviderAdapter for CodexCliAdapter {
    async fn send(
        &self,
        model: &str,
        messages: &[Message],
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        self.send_with_tools(model, messages, tx, Default::default())
            .await
    }
    async fn send_with_tools(
        &self,
        model: &str,
        messages: &[Message],
        tx: mpsc::Sender<StreamEvent>,
        tools: ProviderTools,
    ) -> Result<()> {
        super::codex_app_server::send(&self.binary, model, messages, tx, tools).await
    }
    async fn interrupt(&self) -> Result<()> {
        Ok(())
    }
    fn supports_native_tools(&self) -> bool {
        true
    }
    fn supported_models(&self) -> Vec<String> {
        vec![self.model.clone()]
    }
    fn execution_channel(&self) -> crate::adapter::ExecutionChannel {
        crate::adapter::ExecutionChannel::cli("codex")
    }
}
