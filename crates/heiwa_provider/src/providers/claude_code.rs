use anyhow::Result;
use async_trait::async_trait;
use crate::adapter::{ProviderAdapter, ProviderEvent};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::sync::Arc;

pub struct ClaudeCodeAdapter {
    sessions: Arc<Mutex<HashMap<String, tokio::process::Child>>>,
}

impl ClaudeCodeAdapter {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl ProviderAdapter for ClaudeCodeAdapter {
    async fn start_session(&self) -> Result<String> {
        let session_id = uuid::Uuid::new_v4().to_string();
        
        // Probe if claude is authenticated
        let output = Command::new("claude").arg("--version").output().await?;
        if !output.status.success() {
            return Err(anyhow::anyhow!("Claude Code CLI is not available or failed check"));
        }

        Ok(session_id)
    }

    async fn send_input(&self, session_id: &str, input: &str) -> Result<()> {
        let child = Command::new("claude")
            .arg(input)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut sessions = self.sessions.lock().await;
        sessions.insert(session_id.to_string(), child);
        
        Ok(())
    }

    async fn read_events(&self, session_id: &str) -> Result<Vec<ProviderEvent>> {
        let mut sessions = self.sessions.lock().await;
        if let Some(mut child) = sessions.remove(session_id) {
            let stdout = child.stdout.take().unwrap();
            let mut reader = BufReader::new(stdout).lines();
            let mut events = Vec::new();

            while let Some(line) = reader.next_line().await? {
                events.push(ProviderEvent {
                    event_type: "text".to_string(),
                    payload: line,
                });
            }

            Ok(events)
        } else {
            Ok(vec![])
        }
    }

    async fn interrupt(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        if let Some(mut child) = sessions.remove(session_id) {
            child.kill().await?;
        }
        Ok(())
    }

    async fn close(&self, session_id: &str) -> Result<()> {
        self.interrupt(session_id).await
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec!["cloud_llm".to_string(), "advanced_coding".to_string(), "chat".to_string()]
    }
}
