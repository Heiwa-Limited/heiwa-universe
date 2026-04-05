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
