use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HerdPane {
    pub workspace: String,
    pub pane: String,
    pub agent: String,
    pub state: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HerdSnapshot {
    pub status: String,
    pub source: String,
    pub panes: Vec<HerdPane>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HerdPaneRead {
    pub ok: bool,
    pub pane: String,
    pub text: String,
    pub source: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HerdActionResult {
    pub ok: bool,
    pub message: String,
    pub source: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HerdCommandSpec {
    pub id: String,
    pub label: String,
    pub command: String,
    pub risk: String,
    pub approval: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
struct HerdrWorkspace {
    workspace_id: String,
    label: String,
}

#[derive(Debug, Deserialize)]
struct HerdrPaneRow {
    pane_id: String,
    workspace_id: String,
    agent_status: Option<String>,
    cwd: Option<String>,
    foreground_cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HerdrAgent {
    pane_id: Option<String>,
    agent: Option<String>,
    name: Option<String>,
    state: Option<String>,
    agent_status: Option<String>,
    message: Option<String>,
}

fn rows<T: for<'de> Deserialize<'de>>(value: &Value, key: &str) -> Vec<T> {
    value
        .get("result")
        .and_then(|result| result.get(key))
        .cloned()
        .and_then(|rows| serde_json::from_value(rows).ok())
        .unwrap_or_default()
}

fn run_herdr(args: &[&str]) -> Result<Value, String> {
    let output = Command::new("herdr")
        .args(args)
        .output()
        .map_err(|error| format!("herdr unavailable: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|error| format!("herdr json decode: {error}"))
}

fn run_herdr_text(args: &[String]) -> Result<String, String> {
    let output = Command::new("herdr")
        .args(args)
        .output()
        .map_err(|error| format!("herdr unavailable: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn ok(message: impl Into<String>) -> HerdActionResult {
    HerdActionResult {
        ok: true,
        message: message.into(),
        source: "herdr-cli".to_string(),
        error: None,
    }
}

fn err(error: impl Into<String>) -> HerdActionResult {
    let error = error.into();
    HerdActionResult {
        ok: false,
        message: error.clone(),
        source: "herdr-cli".to_string(),
        error: Some(error),
    }
}

fn command_catalog() -> Vec<HerdCommandSpec> {
    vec![
        HerdCommandSpec {
            id: "git.status".to_string(),
            label: "Git status".to_string(),
            command: "git status --short --branch".to_string(),
            risk: "host_safe_readonly".to_string(),
            approval: "auto".to_string(),
            description: "Show the current branch and dirty worktree without mutating files."
                .to_string(),
        },
        HerdCommandSpec {
            id: "git.diff.stat".to_string(),
            label: "Git diff stat".to_string(),
            command: "git diff --stat".to_string(),
            risk: "host_safe_readonly".to_string(),
            approval: "auto".to_string(),
            description: "Summarize unstaged file changes without printing full diff contents."
                .to_string(),
        },
        HerdCommandSpec {
            id: "npm.typecheck".to_string(),
            label: "TypeScript typecheck".to_string(),
            command: "npm run typecheck".to_string(),
            risk: "host_safe_readonly".to_string(),
            approval: "auto".to_string(),
            description: "Run the repository TypeScript contract checks.".to_string(),
        },
        HerdCommandSpec {
            id: "deno.herd.check".to_string(),
            label: "Deno herd check".to_string(),
            command: "deno check scripts/herd.ts prototypes/heiwa-desk/main.ts prototypes/heiwa-desk/herdr.ts"
                .to_string(),
            risk: "host_safe_readonly".to_string(),
            approval: "auto".to_string(),
            description: "Typecheck least-privilege herdr/Deno glue.".to_string(),
        },
    ]
}

fn command_ids() -> String {
    command_catalog()
        .into_iter()
        .map(|spec| spec.id)
        .collect::<Vec<_>>()
        .join(", ")
}

fn raw_herd_run_allowed() -> bool {
    matches!(
        std::env::var("HEIWA_HERD_ALLOW_RAW_RUN").as_deref(),
        Ok("1" | "true" | "yes")
    )
}

fn resolve_herd_run_command_with_raw_policy(
    command: &str,
    raw_allowed: bool,
) -> Result<HerdCommandSpec, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("command is required".to_string());
    }

    if let Some(spec) = command_catalog()
        .into_iter()
        .find(|spec| spec.id == command)
    {
        return Ok(spec);
    }

    if raw_allowed {
        return Ok(HerdCommandSpec {
            id: "raw.operator".to_string(),
            label: "Raw operator command".to_string(),
            command: command.to_string(),
            risk: "host_mutating".to_string(),
            approval: "operator_env_opt_in".to_string(),
            description:
                "Operator explicitly enabled raw herdr pane commands with HEIWA_HERD_ALLOW_RAW_RUN."
                    .to_string(),
        });
    }

    Err(format!(
        "raw herd pane run is disabled; choose a command id ({}) or set HEIWA_HERD_ALLOW_RAW_RUN=1 for an operator-only escape hatch",
        command_ids()
    ))
}

fn panes_from_herdr_values(workspaces: Value, panes: Value, agents: Value) -> Vec<HerdPane> {
    let workspace_labels: HashMap<String, String> =
        rows::<HerdrWorkspace>(&workspaces, "workspaces")
            .into_iter()
            .map(|workspace| (workspace.workspace_id, workspace.label))
            .collect();
    let agents_by_pane: HashMap<String, HerdrAgent> = rows::<HerdrAgent>(&agents, "agents")
        .into_iter()
        .filter_map(|agent| agent.pane_id.clone().map(|pane_id| (pane_id, agent)))
        .collect();

    rows::<HerdrPaneRow>(&panes, "panes")
        .into_iter()
        .map(|pane| {
            let agent = agents_by_pane.get(&pane.pane_id);
            HerdPane {
                workspace: workspace_labels
                    .get(&pane.workspace_id)
                    .cloned()
                    .unwrap_or(pane.workspace_id),
                pane: pane.pane_id,
                agent: agent
                    .and_then(|agent| agent.agent.clone().or_else(|| agent.name.clone()))
                    .unwrap_or_else(|| "-".to_string()),
                state: agent
                    .and_then(|agent| agent.state.clone().or_else(|| agent.agent_status.clone()))
                    .or(pane.agent_status)
                    .unwrap_or_else(|| "unknown".to_string()),
                cwd: pane
                    .foreground_cwd
                    .or(pane.cwd)
                    .unwrap_or_else(|| "-".to_string()),
                message: agent
                    .and_then(|agent| agent.message.clone())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

async fn panes_from_deno_bridge() -> Result<Vec<HerdPane>, String> {
    let url = std::env::var("HEIWA_HERD_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:7480/api/herd".to_string());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(900))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("herd bridge returned {}", response.status()));
    }
    response
        .json::<Vec<HerdPane>>()
        .await
        .map_err(|error| error.to_string())
}

fn panes_from_herdr_cli() -> Result<Vec<HerdPane>, String> {
    let workspaces = run_herdr(&["workspace", "list"])?;
    let panes = run_herdr(&["pane", "list"])?;
    let agents = run_herdr(&["agent", "list"])?;
    Ok(panes_from_herdr_values(workspaces, panes, agents))
}

#[tauri::command]
pub fn herd_command_catalog() -> Vec<HerdCommandSpec> {
    command_catalog()
}

#[tauri::command]
pub async fn herd_panes() -> HerdSnapshot {
    match panes_from_deno_bridge().await {
        Ok(panes) => HerdSnapshot {
            status: "online".to_string(),
            source: "deno-bridge".to_string(),
            panes,
            error: None,
        },
        Err(bridge_error) => match panes_from_herdr_cli() {
            Ok(panes) => HerdSnapshot {
                status: "online".to_string(),
                source: "herdr-cli".to_string(),
                panes,
                error: None,
            },
            Err(cli_error) => HerdSnapshot {
                status: "offline".to_string(),
                source: "none".to_string(),
                panes: Vec::new(),
                error: Some(format!("bridge: {bridge_error}; cli: {cli_error}")),
            },
        },
    }
}

#[tauri::command]
pub async fn herd_pane_read(pane: String) -> HerdPaneRead {
    if pane.trim().is_empty() {
        return HerdPaneRead {
            ok: false,
            pane,
            text: String::new(),
            source: "herdr-cli".to_string(),
            error: Some("pane is required".to_string()),
        };
    }

    let args = vec![
        "pane".to_string(),
        "read".to_string(),
        pane.clone(),
        "--source".to_string(),
        "visible".to_string(),
        "--lines".to_string(),
        "160".to_string(),
        "--format".to_string(),
        "text".to_string(),
    ];

    match run_herdr_text(&args) {
        Ok(text) => HerdPaneRead {
            ok: true,
            pane,
            text,
            source: "herdr-cli".to_string(),
            error: None,
        },
        Err(error) => HerdPaneRead {
            ok: false,
            pane,
            text: String::new(),
            source: "herdr-cli".to_string(),
            error: Some(error),
        },
    }
}

#[tauri::command]
pub async fn herd_pane_send(pane: String, text: String) -> HerdActionResult {
    if pane.trim().is_empty() || text.trim().is_empty() {
        return err("pane and text are required");
    }
    let send_text = vec![
        "pane".to_string(),
        "send-text".to_string(),
        pane.clone(),
        text,
    ];
    if let Err(error) = run_herdr_text(&send_text) {
        return err(error);
    }
    let enter = vec![
        "pane".to_string(),
        "send-keys".to_string(),
        pane.clone(),
        "enter".to_string(),
    ];
    match run_herdr_text(&enter) {
        Ok(_) => ok(format!("sent to {pane}")),
        Err(error) => err(error),
    }
}

#[tauri::command]
pub async fn herd_pane_run(pane: String, command: String) -> HerdActionResult {
    if pane.trim().is_empty() || command.trim().is_empty() {
        return err("pane and command are required");
    }
    let spec = match resolve_herd_run_command_with_raw_policy(&command, raw_herd_run_allowed()) {
        Ok(spec) => spec,
        Err(error) => return err(error),
    };
    let args = vec![
        "pane".to_string(),
        "run".to_string(),
        pane.clone(),
        spec.command,
    ];
    match run_herdr_text(&args) {
        Ok(_) => ok(format!("ran {} in {pane}", spec.id)),
        Err(error) => err(error),
    }
}

#[tauri::command]
pub async fn herd_pane_focus(pane: String) -> HerdActionResult {
    if pane.trim().is_empty() {
        return err("pane is required");
    }
    let args = vec!["agent".to_string(), "focus".to_string(), pane.clone()];
    match run_herdr_text(&args) {
        Ok(_) => ok(format!("focused {pane}")),
        Err(error) => err(error),
    }
}

#[tauri::command]
pub async fn herd_pane_split(
    pane: String,
    direction: String,
    cwd: Option<String>,
) -> HerdActionResult {
    if pane.trim().is_empty() {
        return err("pane is required");
    }
    if direction != "right" && direction != "down" {
        return err("direction must be right or down");
    }

    let mut args = vec![
        "pane".to_string(),
        "split".to_string(),
        pane.clone(),
        "--direction".to_string(),
        direction,
        "--focus".to_string(),
    ];
    if let Some(cwd) = cwd.filter(|cwd| !cwd.trim().is_empty() && cwd != "-") {
        args.push("--cwd".to_string());
        args.push(cwd);
    }

    match run_herdr_text(&args) {
        Ok(_) => ok(format!("split {pane}")),
        Err(error) => err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn panes_from_herdr_values_joins_workspace_and_agent_rows() {
        let workspaces = json!({"result":{"workspaces":[{"workspace_id":"w1","label":"heiwa"}]}});
        let panes = json!({"result":{"panes":[{"pane_id":"w1:p1","workspace_id":"w1","agent_status":"unknown","cwd":"/tmp"}]}});
        let agents = json!({"result":{"agents":[{"pane_id":"w1:p1","agent":"qwen-local","state":"idle","message":"ready"}]}});

        let rows = panes_from_herdr_values(workspaces, panes, agents);

        assert_eq!(
            rows,
            vec![HerdPane {
                workspace: "heiwa".to_string(),
                pane: "w1:p1".to_string(),
                agent: "qwen-local".to_string(),
                state: "idle".to_string(),
                cwd: "/tmp".to_string(),
                message: "ready".to_string(),
            }]
        );
    }

    #[test]
    fn command_catalog_exposes_readonly_checks() {
        let ids: Vec<_> = command_catalog().into_iter().map(|spec| spec.id).collect();

        assert!(ids.contains(&"git.status".to_string()));
        assert!(ids.contains(&"git.diff.stat".to_string()));
        assert!(ids.contains(&"npm.typecheck".to_string()));
        assert!(ids.contains(&"deno.herd.check".to_string()));
    }

    #[test]
    fn known_herd_run_command_resolves_to_static_command() {
        let spec = resolve_herd_run_command_with_raw_policy("git.status", false).unwrap();

        assert_eq!(spec.command, "git status --short --branch");
        assert_eq!(spec.risk, "host_safe_readonly");
    }

    #[test]
    fn raw_herd_run_command_is_denied_without_operator_escape_hatch() {
        let err = resolve_herd_run_command_with_raw_policy("rm -rf /tmp/heiwa", false).unwrap_err();

        assert!(err.contains("raw herd pane run is disabled"));
        assert!(err.contains("git.status"));
    }

    #[test]
    fn raw_herd_run_command_requires_explicit_operator_policy() {
        let spec = resolve_herd_run_command_with_raw_policy("echo ok", true).unwrap();

        assert_eq!(spec.id, "raw.operator");
        assert_eq!(spec.command, "echo ok");
        assert_eq!(spec.approval, "operator_env_opt_in");
    }
}
