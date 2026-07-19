use anyhow::Result;
use chrono::Utc;
use heiwa_protocol::{ExecutionScope, ToolCall, ToolCallReceipt, ToolCallStatus};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AgenticTurnInput {
    pub prompt: String,
    pub scope: ExecutionScope,
    pub model_responses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolTranscriptEntry {
    pub name: String,
    pub output: String,
}

/// Policy/staging adapter over DREX's existing approval mechanics. The turn
/// runner owns durable provenance around this adapter; it never reimplements
/// the policy matrix or approval-packet format.
#[derive(Debug, Clone)]
pub struct ToolApproval {
    pub request_id: Option<String>,
    pub request_path: Option<PathBuf>,
    pub risk: heiwa_drex::drex_gate::RiskLevel,
    pub surface: String,
}

pub fn plan_tool_approval(call: &ToolCall) -> ToolApproval {
    let risk = tool_risk(call);
    let surface = std::env::var("HEIWA_SURFACE").unwrap_or_else(|_| "cli".to_string());
    match heiwa_drex::drex_gate::evaluate_approval_policy(
        &call.name,
        &serde_json::to_string(&call.arguments).unwrap_or_default(),
        risk,
        &surface,
    ) {
        heiwa_drex::drex_gate::ApprovalVerdict::AutoApproved => ToolApproval {
            request_id: None,
            request_path: None,
            risk,
            surface,
        },
        heiwa_drex::drex_gate::ApprovalVerdict::AwaitingApproval {
            request_id,
            request_path,
        } => ToolApproval {
            request_id: Some(request_id),
            request_path: Some(request_path),
            risk,
            surface,
        },
    }
}

pub fn stage_tool_approval(call: &ToolCall, approval: &ToolApproval) -> Result<()> {
    let (Some(request_id), Some(request_path)) = (&approval.request_id, &approval.request_path)
    else {
        return Ok(());
    };
    heiwa_drex::drex_gate::stage_approval_request(
        request_id,
        request_path,
        &call.name,
        &serde_json::to_string(&call.arguments).unwrap_or_default(),
        approval.risk,
        &approval.surface,
        &call.arguments,
    )
}

pub fn wait_for_tool_approval(approval: &ToolApproval) -> Result<String> {
    let request_id = approval
        .request_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("auto-approved action has no approval wait"))?;
    heiwa_drex::drex_gate::wait_for_decision(request_id, Duration::from_secs(300))
}

fn tool_risk(call: &ToolCall) -> heiwa_drex::drex_gate::RiskLevel {
    match call.name.as_str() {
        "deploy" | "app.deploy" => heiwa_drex::drex_gate::RiskLevel::Critical,
        name if name.contains("write") || name.contains("delete") => {
            heiwa_drex::drex_gate::RiskLevel::High
        }
        name if name.contains("net") || name.contains("http") => {
            heiwa_drex::drex_gate::RiskLevel::Medium
        }
        _ => heiwa_drex::drex_gate::RiskLevel::Low,
    }
}

#[derive(Debug, Clone)]
pub struct AgenticTurnOutput {
    pub final_answer: String,
    pub tool_receipts: Vec<ToolCallReceipt>,
    pub tool_transcript: Vec<ToolTranscriptEntry>,
}

pub async fn run_agentic_turn_with_responses(input: AgenticTurnInput) -> Result<AgenticTurnOutput> {
    run_agentic_turn_with_context(
        input.prompt,
        input.scope,
        input.model_responses,
        "test",
        "test",
    )
    .await
}

pub async fn run_agentic_turn_with_context(
    prompt: String,
    scope: ExecutionScope,
    model_responses: Vec<String>,
    provider: &str,
    model_id: &str,
) -> Result<AgenticTurnOutput> {
    let mut responses = model_responses.into_iter();
    let first_response = responses.next().unwrap_or_default();
    let tool_calls = parse_tool_calls(&first_response);

    if tool_calls.is_empty() {
        return Ok(AgenticTurnOutput {
            final_answer: first_response,
            tool_receipts: Vec::new(),
            tool_transcript: Vec::new(),
        });
    }

    let (tool_receipts, tool_transcript) =
        execute_tool_calls(scope, tool_calls, provider, model_id).await?;

    let final_answer = responses
        .next()
        .unwrap_or_else(|| summarize_tool_outputs(&prompt, &tool_transcript));

    Ok(AgenticTurnOutput {
        final_answer,
        tool_receipts,
        tool_transcript,
    })
}

pub async fn execute_tool_calls(
    scope: ExecutionScope,
    tool_calls: Vec<ToolCall>,
    provider: &str,
    model_id: &str,
) -> Result<(Vec<ToolCallReceipt>, Vec<ToolTranscriptEntry>)> {
    let registry = heiwa_mcp::local_repo_registry(scope);
    let mut tool_receipts = Vec::new();
    let mut tool_transcript = Vec::new();

    for call in tool_calls {
        let started_at = Utc::now().to_rfc3339();

        let risk = match call.name.as_str() {
            "deploy" | "app.deploy" => heiwa_drex::drex_gate::RiskLevel::Critical,
            name if name.contains("write") || name.contains("delete") => {
                heiwa_drex::drex_gate::RiskLevel::High
            }
            name if name.contains("net") || name.contains("http") => {
                heiwa_drex::drex_gate::RiskLevel::Medium
            }
            _ => heiwa_drex::drex_gate::RiskLevel::Low,
        };

        let surface = std::env::var("HEIWA_SURFACE").unwrap_or_else(|_| "cli".to_string());
        let verdict = heiwa_drex::drex_gate::evaluate_approval_policy(
            &call.name,
            &serde_json::to_string(&call.arguments).unwrap_or_default(),
            risk,
            &surface,
        );

        let mut approved = true;
        let mut denied_reason = None;

        if let heiwa_drex::drex_gate::ApprovalVerdict::AwaitingApproval {
            request_id,
            request_path,
        } = verdict
        {
            if let Err(err) = heiwa_drex::drex_gate::stage_approval_request(
                &request_id,
                &request_path,
                &call.name,
                &serde_json::to_string(&call.arguments).unwrap_or_default(),
                risk,
                &surface,
                &call.arguments,
            ) {
                eprintln!("Failed to stage approval request: {}", err);
            }

            println!(
                "\n[HOLD] Action requires operator approval: {} | ID: {}",
                call.name, request_id
            );
            println!("Run: heiwa approvals decide {} --approve", request_id);

            match heiwa_drex::drex_gate::wait_for_decision(
                &request_id,
                std::time::Duration::from_secs(300),
            ) {
                Ok(outcome) if outcome == "approved" => {
                    println!(
                        "\n[APPROVED] Resuming execution for {} | ID: {}",
                        call.name, request_id
                    );
                }
                Ok(outcome) => {
                    println!(
                        "\n[DENIED] Halting execution for {} | ID: {} | Outcome: {}",
                        call.name, request_id, outcome
                    );
                    approved = false;
                    denied_reason = Some(format!("denied: {}", outcome));
                }
                Err(err) => {
                    println!(
                        "\n[TIMEOUT] Halting execution for {} | ID: {} | Reason: {}",
                        call.name, request_id, err
                    );
                    approved = false;
                    denied_reason = Some(format!("timeout: {}", err));
                }
            }
        }

        let result = if approved {
            registry.call(&call.name, call.arguments.clone()).await
        } else {
            Err(heiwa_mcp::McpError::PolicyDenied(
                heiwa_mcp::PolicyDenial::MissingLease {
                    tool: format!(
                        "{} (gated, approval: {})",
                        call.name,
                        denied_reason.unwrap_or_default()
                    ),
                },
            ))
        };

        let completed_at = Utc::now().to_rfc3339();

        match result {
            Ok(value) => {
                let output = serde_json::to_string_pretty(&value)?;
                tool_transcript.push(ToolTranscriptEntry {
                    name: call.name.clone(),
                    output,
                });
                tool_receipts.push(ToolCallReceipt {
                    id: format!("tool-receipt-{}", uuid::Uuid::new_v4()),
                    call_id: call.id,
                    provider: provider.to_string(),
                    model_id: model_id.to_string(),
                    tool_name: call.name,
                    status: ToolCallStatus::Success,
                    started_at,
                    completed_at,
                    arguments: call.arguments,
                    result: Some(value),
                    error: None,
                });
            }
            Err(error) => {
                let status = if error.is_policy_denial() {
                    ToolCallStatus::Denied
                } else {
                    ToolCallStatus::Failure
                };
                tool_transcript.push(ToolTranscriptEntry {
                    name: call.name.clone(),
                    output: format!("error: {error}"),
                });
                tool_receipts.push(ToolCallReceipt {
                    id: format!("tool-receipt-{}", uuid::Uuid::new_v4()),
                    call_id: call.id,
                    provider: provider.to_string(),
                    model_id: model_id.to_string(),
                    tool_name: call.name,
                    status,
                    started_at,
                    completed_at,
                    arguments: call.arguments,
                    result: None,
                    error: Some(error.to_string()),
                });
            }
        }
    }

    Ok((tool_receipts, tool_transcript))
}

/// Execute exactly one tool through the existing approval/MCP/evidence path.
/// Turn runners use this narrow adapter so they can persist `tool_call_started`
/// before the side effect and `tool_call_completed` immediately after it.
pub async fn execute_tool_call(
    scope: ExecutionScope,
    call: ToolCall,
    provider: &str,
    model_id: &str,
) -> Result<(ToolCallReceipt, ToolTranscriptEntry)> {
    let (mut receipts, mut transcript) =
        execute_tool_calls(scope, vec![call], provider, model_id).await?;
    let receipt = receipts
        .pop()
        .ok_or_else(|| anyhow::anyhow!("tool execution returned no receipt"))?;
    let entry = transcript
        .pop()
        .ok_or_else(|| anyhow::anyhow!("tool execution returned no transcript entry"))?;
    Ok((receipt, entry))
}

/// Execute a tool after the caller has completed DREX approval policy and
/// durable approval provenance. Keeping this separate prevents a cancelled
/// detached approval waiter from ever reaching the MCP side effect.
pub async fn execute_approved_tool_call(
    scope: ExecutionScope,
    call: ToolCall,
    provider: &str,
    model_id: &str,
) -> Result<(ToolCallReceipt, ToolTranscriptEntry)> {
    let registry = heiwa_mcp::local_repo_registry(scope);
    let started_at = Utc::now().to_rfc3339();
    let completed_at = Utc::now().to_rfc3339();
    match registry.call(&call.name, call.arguments.clone()).await {
        Ok(value) => {
            let output = serde_json::to_string_pretty(&value)?;
            Ok((
                ToolCallReceipt {
                    id: format!("tool-receipt-{}", uuid::Uuid::new_v4()),
                    call_id: call.id,
                    provider: provider.to_string(),
                    model_id: model_id.to_string(),
                    tool_name: call.name.clone(),
                    status: ToolCallStatus::Success,
                    started_at,
                    completed_at,
                    arguments: call.arguments,
                    result: Some(value),
                    error: None,
                },
                ToolTranscriptEntry {
                    name: call.name,
                    output,
                },
            ))
        }
        Err(error) => {
            let status = if error.is_policy_denial() {
                ToolCallStatus::Denied
            } else {
                ToolCallStatus::Failure
            };
            Ok((
                ToolCallReceipt {
                    id: format!("tool-receipt-{}", uuid::Uuid::new_v4()),
                    call_id: call.id,
                    provider: provider.to_string(),
                    model_id: model_id.to_string(),
                    tool_name: call.name.clone(),
                    status,
                    started_at,
                    completed_at,
                    arguments: call.arguments,
                    result: None,
                    error: Some(error.to_string()),
                },
                ToolTranscriptEntry {
                    name: call.name,
                    output: format!("error: {error}"),
                },
            ))
        }
    }
}

pub fn tool_instruction_prompt() -> String {
    "Agentic mode is active. If repo/file context is needed, respond only with JSON: {\"tool_calls\":[{\"name\":\"fs.list|fs.read|repo.grep\",\"arguments\":{...}}]}. Available tools: fs.list {path}, fs.read {path,max_bytes}, repo.grep {pattern,path,max_matches}. Use paths relative to current directory. If no tool is needed, answer normally.".to_string()
}

pub fn tool_result_prompt(entries: &[ToolTranscriptEntry]) -> String {
    let body = entries
        .iter()
        .map(|entry| format!("{} result:\n{}", entry.name, entry.output))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("Tool results are available. Use them to answer the user.\n\n{body}")
}

pub fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
    let Some(value) = parse_jsonish(text) else {
        return Vec::new();
    };
    collect_tool_calls(&value)
}

fn parse_jsonish(text: &str) -> Option<Value> {
    serde_json::from_str::<Value>(text.trim()).ok().or_else(|| {
        let start = text.find('{')?;
        let end = text.rfind('}')?;
        serde_json::from_str::<Value>(&text[start..=end]).ok()
    })
}

fn collect_tool_calls(value: &Value) -> Vec<ToolCall> {
    if let Some(calls) = value.get("tool_calls").and_then(Value::as_array) {
        return calls.iter().filter_map(tool_call_from_value).collect();
    }
    if let Some(call) = value.get("tool_call") {
        return tool_call_from_value(call).into_iter().collect();
    }
    tool_call_from_value(value).into_iter().collect()
}

fn tool_call_from_value(value: &Value) -> Option<ToolCall> {
    let name = value
        .get("name")
        .or_else(|| value.get("tool"))
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
        })?;

    let arguments = value
        .get("arguments")
        .or_else(|| value.get("input"))
        .cloned()
        .or_else(|| {
            value
                .get("function")
                .and_then(|function| function.get("arguments"))
                .cloned()
        })
        .unwrap_or_else(|| json!({}));

    Some(ToolCall {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("tool-call-{}", uuid::Uuid::new_v4())),
        name: name.to_string(),
        arguments: normalize_arguments(arguments),
    })
}

fn normalize_arguments(arguments: Value) -> Value {
    match arguments {
        Value::String(s) => serde_json::from_str(&s).unwrap_or_else(|_| json!({ "value": s })),
        other => other,
    }
}

fn summarize_tool_outputs(prompt: &str, tool_transcript: &[ToolTranscriptEntry]) -> String {
    let tools = tool_transcript
        .iter()
        .map(|entry| format!("{}:\n{}", entry.name, entry.output))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("Agentic tool results for '{}':\n{}", prompt, tools)
}
