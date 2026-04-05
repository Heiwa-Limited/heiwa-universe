use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Intent {
    Chat,
    Code,
    Research,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRequest {
    pub raw_text: String,
    pub provider_pin: Option<String>,
    pub model_pin: Option<String>,
    pub intent: Intent,
}

pub fn parse_turn_intent(input: &str) -> TurnRequest {
    let lowercase = input.to_lowercase();
    let mut provider_pin = None;
    let mut model_pin = None;

    // Very simple extraction for "use [provider] [model]"
    if lowercase.contains("use ") {
        let parts: Vec<&str> = lowercase.split_whitespace().collect();
        if let Some(pos) = parts.iter().position(|&x| x == "use") {
            if let Some(provider) = parts.get(pos + 1) {
                provider_pin = Some((*provider).to_string());
                if let Some(model) = parts.get(pos + 2) {
                    model_pin = Some((*model).to_string());
                }
            }
        }
    }

    let intent = if lowercase.contains("code") || lowercase.contains("rust") || lowercase.contains("fix") {
        Intent::Code
    } else if lowercase.contains("research") || lowercase.contains("explore") {
        Intent::Research
    } else {
        Intent::Chat
    };

    TurnRequest {
        raw_text: input.to_string(),
        provider_pin,
        model_pin,
        intent,
    }
}
