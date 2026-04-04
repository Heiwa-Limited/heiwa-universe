use anyhow::Result;
use async_trait::async_trait;
use crate::adapter::{ProviderAdapter, ProviderEvent};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::sync::Arc;

pub struct OllamaAdapter {
    sessions: Arc<Mutex<HashMap<String, tokio::process::Child>>>,
}

impl OllamaAdapter {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl ProviderAdapter for OllamaAdapter {
    async fn start_session(&self) -> Result<String> {
        let session_id = uuid::Uuid::new_v4().to_string();
        
        // Check if ollama is running
        if !std::net::TcpStream::connect("127.0.0.1:11434").is_ok() {
            return Err(anyhow::anyhow!("Ollama is not running on 127.0.0.1:11434"));
        }

        Ok(session_id)
    }

    async fn send_input(&self, _session_id: &str, input: &str) -> Result<()> {
        // In a real Ollama adapter, we'd use the HTTP API (localhost:11434/api/generate)
        // But the plan calls for a subprocess adapter for CLI providers.
        // For Ollama, we'll implement a 'subprocess-like' call to `ollama run` for simplicity in this task.
        
        let child = Command::new("ollama")
            .arg("run")
            .arg("llama3")
            .arg(input)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut sessions = self.sessions.lock().await;
        sessions.insert(_session_id.to_string(), child);
        
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
        vec!["local_llm".to_string(), "chat".to_string()]
    }
}
