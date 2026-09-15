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
        HerdCommandSpec {
            id: "monitor.ops".to_string(),
            label: "Monitor ops".to_string(),
            command: "heiwa app api get /api/v1/monitor --json".to_string(),
            risk: "host_safe_readonly".to_string(),
            approval: "auto".to_string(),
            description:
                "Read combined user and machine ops state from the local Heiwa.app runtime."
                    .to_string(),
        },
        HerdCommandSpec {
            id: "monitor.machine".to_string(),
            label: "Monitor machine".to_string(),
            command: "heiwa app api get /api/v1/resource --json".to_string(),
            risk: "host_safe_readonly".to_string(),
            approval: "auto".to_string(),
            description: "Read CPU, memory, thermal, and admission state.".to_string(),
        },
        HerdCommandSpec {
            id: "monitor.inbox".to_string(),
            label: "Monitor inbox".to_string(),
            command: "heiwa app api get /api/v1/inbox --json".to_string(),
            risk: "host_safe_readonly".to_string(),
            approval: "auto".to_string(),
            description: "Read the local intake inbox for receipts and operator-facing items."
                .to_string(),
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

/// The live pane ids `herd_pane_send` checks a target against, or `None`
/// when neither listing source could be reached. `None` deliberately does
/// not block a send: with the bridge and the CLI both down, the
/// `run_herdr_text` call downstream fails on its own, so refusing here would
/// only add a second, redundant "herdr unavailable" error rather than a
/// second layer of safety.
async fn known_pane_ids() -> Option<Vec<String>> {
    if let Ok(panes) = panes_from_deno_bridge().await {
        return Some(panes.into_iter().map(|pane| pane.pane).collect());
    }
    panes_from_herdr_cli()
        .ok()
        .map(|panes| panes.into_iter().map(|pane| pane.pane).collect())
}

/// Longest text a single `herd_pane_send` call may type into a pane at once.
/// Generous for a shell command or a short prompt line; pasting a whole file
/// this way was never the intended use.
const MAX_PANE_TEXT_LEN: usize = 4096;

/// Longest a pane id may be. Real herdr ids (`workspace:pane`-shaped, see
/// this module's tests) are short; this only exists to cap the work the
/// charset check below does on hostile input.
const MAX_PANE_ID_LEN: usize = 256;

/// ASCII control bytes `herd_pane_send` rejects outright: everything in
/// 0x00-0x08, 0x0A-0x1F, and 0x7F. Tab (0x09) is the only control byte let
/// through. This range specifically includes newline (0x0A) and carriage
/// return (0x0D) — raw text with an embedded newline would let a caller
/// inject a second `send-keys ... enter`-worth of terminal input inside what
/// is supposed to be one line, and ESC (0x1B) starts ANSI/VT escape
/// sequences, which is exactly the class of "control the pane, not just
/// fill it with text" input this validation exists to stop.
fn contains_disallowed_control_byte(text: &str) -> bool {
    text.bytes()
        .any(|byte| matches!(byte, 0x00..=0x08 | 0x0A..=0x1F | 0x7F))
}

fn validate_pane_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("text is required".to_string());
    }
    if text.chars().count() > MAX_PANE_TEXT_LEN {
        return Err(format!(
            "text exceeds the {MAX_PANE_TEXT_LEN}-character limit for a single send"
        ));
    }
    if contains_disallowed_control_byte(text) {
        return Err(
            "text contains a control character (including newline, carriage return, or an \
             escape sequence) that herd_pane_send does not allow"
                .to_string(),
        );
    }
    Ok(())
}

/// The same charset real herdr pane/workspace ids use today (the
/// `w1:p1`-shaped ids in this module's tests): ASCII letters, digits, and
/// `:_-.`. Nothing else can be a real herdr pane id, so nothing else needs
/// to reach the `herdr` subprocess call below.
fn validate_pane_id_format(pane: &str) -> Result<(), String> {
    let trimmed = pane.trim();
    if trimmed.is_empty() {
        return Err("pane is required".to_string());
    }
    if trimmed.len() > MAX_PANE_ID_LEN {
        return Err("pane id is too long".to_string());
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.'))
    {
        return Err("pane id contains unexpected characters".to_string());
    }
    Ok(())
}

/// Pure validation core for `herd_pane_send`, factored out so unit tests can
/// cover every rejection and the happy path without shelling out to a real
/// `herdr`. See [`known_pane_ids`] for what `known_panes: None` means.
fn validate_pane_send(
    pane: &str,
    text: &str,
    known_panes: Option<&[String]>,
) -> Result<(), String> {
    validate_pane_id_format(pane)?;
    validate_pane_text(text)?;
    if let Some(known) = known_panes {
        if !known.iter().any(|id| id == pane.trim()) {
            return Err(format!("{} is not a live herdr pane", pane.trim()));
        }
    }
    Ok(())
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

/// Types `text` into a live terminal pane followed by Enter.
///
/// `text` is rejected outright if it contains a newline (see
/// [`contains_disallowed_control_byte`]): today's one caller,
/// `WindowsSurface`'s pane control in `src/surfaces/windows/index.tsx`, binds
/// this to a plain `<input type="text">` — HTML text inputs cannot contain a
/// newline character at all, so nothing in the current UI can legitimately
/// send multi-line text this way. (The desktop's other multi-line input, the
/// `Composer` textarea, submits through `app.operator.submit` to the
/// authenticated `/api/v1/operator/threads/{id}/turns` endpoint, an entirely
/// separate path that never touches `herd_pane_send`.) If a future UI needs
/// to paste multiple lines into a pane, that needs its own opt-in path
/// (e.g. an herdr paste mode with no `Enter` sent per line) rather than
/// relaxing this check.
#[tauri::command]
pub async fn herd_pane_send(pane: String, text: String) -> HerdActionResult {
    let known_panes = known_pane_ids().await;
    if let Err(error) = validate_pane_send(&pane, &text, known_panes.as_deref()) {
        return err(error);
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
        assert!(ids.contains(&"monitor.ops".to_string()));
        assert!(ids.contains(&"monitor.machine".to_string()));
        assert!(ids.contains(&"monitor.inbox".to_string()));
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

    #[test]
    fn validate_pane_send_happy_path_with_known_pane() {
        let known = vec!["w1:p1".to_string()];
        assert!(validate_pane_send("w1:p1", "git status", Some(&known)).is_ok());
    }

    #[test]
    fn validate_pane_send_happy_path_when_listing_is_unavailable() {
        // herdr and the bridge are both unreachable: format/content checks
        // still run, but existence cannot be enforced (see `known_pane_ids`).
        assert!(validate_pane_send("w1:p1", "git status", None).is_ok());
    }

    #[test]
    fn validate_pane_send_rejects_empty_pane_or_text() {
        assert!(validate_pane_send("", "text", None).is_err());
        assert!(validate_pane_send("   ", "text", None).is_err());
        assert!(validate_pane_send("w1:p1", "", None).is_err());
        assert!(validate_pane_send("w1:p1", "   ", None).is_err());
    }

    #[test]
    fn validate_pane_send_rejects_an_unknown_pane_when_the_listing_is_available() {
        let known = vec!["w1:p1".to_string()];
        let error = validate_pane_send("w9:p9", "git status", Some(&known)).unwrap_err();
        assert!(error.contains("not a live herdr pane"));
    }

    #[test]
    fn validate_pane_send_rejects_pane_ids_outside_the_herdr_charset() {
        assert!(validate_pane_send("w1;rm -rf /", "text", None).is_err());
        assert!(validate_pane_send("../etc/passwd", "text", None).is_err());
        assert!(validate_pane_send("w1 p1", "text", None).is_err());
    }

    #[test]
    fn validate_pane_send_rejects_an_oversized_pane_id() {
        let huge_pane = "a".repeat(MAX_PANE_ID_LEN + 1);
        assert!(validate_pane_send(&huge_pane, "text", None).is_err());
    }

    #[test]
    fn validate_pane_send_rejects_newline_and_carriage_return() {
        assert!(validate_pane_send("w1:p1", "line one\nline two", None).is_err());
        assert!(validate_pane_send("w1:p1", "line one\r\nline two", None).is_err());
    }

    #[test]
    fn validate_pane_send_rejects_escape_sequences_and_other_control_bytes() {
        assert!(validate_pane_send("w1:p1", "\u{1b}[31mred\u{1b}[0m", None).is_err());
        assert!(validate_pane_send("w1:p1", "bell\u{7}", None).is_err());
        assert!(validate_pane_send("w1:p1", "del\u{7f}", None).is_err());
    }

    #[test]
    fn validate_pane_send_allows_tab() {
        assert!(validate_pane_send("w1:p1", "a\tb", None).is_ok());
    }

    #[test]
    fn validate_pane_send_rejects_text_over_the_length_cap() {
        let huge_text = "a".repeat(MAX_PANE_TEXT_LEN + 1);
        assert!(validate_pane_send("w1:p1", &huge_text, None).is_err());
    }

    #[test]
    fn validate_pane_send_accepts_text_at_exactly_the_length_cap() {
        let text = "a".repeat(MAX_PANE_TEXT_LEN);
        assert!(validate_pane_send("w1:p1", &text, None).is_ok());
    }
}
