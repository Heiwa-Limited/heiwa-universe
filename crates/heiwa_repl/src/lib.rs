use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReplCommand {
    Task(String),
    Shell(String),
    Slash(String, Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryState {
    pub provider: String,
    pub model: String,
    pub route: String,
    pub status: String,
    pub turn_count: u32,
    pub loop_info: Option<(u32, u32)>, // (current, max)
}

pub fn render_footer(state: &TelemetryState) -> String {
    let loop_str = if let Some((current, max)) = state.loop_info {
        format!(" | L: {}/{}", current, max)
    } else {
        "".to_string()
    };
    format!(
        "[{}] {} | {} | {}{} | turns: {}",
        state.status, state.provider, state.model, state.route, loop_str, state.turn_count
    )
}

pub fn parse_input(input: &str) -> ReplCommand {
    let input = input.trim();
    if input.is_empty() {
        return ReplCommand::Task("".to_string());
    }

    if input.starts_with('!') {
        return ReplCommand::Shell(input[1..].trim().to_string());
    }

    if input.starts_with('/') {
        let parts: Vec<String> = input[1..].split_whitespace().map(|s| s.to_string()).collect();
        if parts.is_empty() {
            return ReplCommand::Task(input.to_string());
        }
        let cmd = parts[0].clone();
        let args = parts[1..].to_vec();
        return ReplCommand::Slash(cmd, args);
    }

    ReplCommand::Task(input.to_string())
}
