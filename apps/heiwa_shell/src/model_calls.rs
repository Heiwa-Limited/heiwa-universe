use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use heiwa_core::drex::{
    default_policy, plan_model_call, CostTruth, ModelCallCandidate, ModelCallPlan,
    ModelCallRequest, SafetyClass,
};
use heiwa_evidence::{
    now_iso, OperatorActor, OperatorEvent, OperatorEventType, OperatorRisk, OperatorSensitivity,
    OPERATOR_EVENT_SCHEMA_VERSION,
};
use heiwa_provider::adapter::{Message, ProviderAdapter, StreamEvent, TokenUsage};
use heiwa_session::operator::{OperatorSessionService, StartTurnRequest};
use serde_json::json;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

pub type AdapterResolver =
    dyn Fn(&str, &str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync + 'static;

pub struct ModelCallExecution {
    pub request: ModelCallRequest,
    pub candidates: Vec<ModelCallCandidate>,
    pub messages: Vec<Message>,
    pub remaining_budget_usd: Option<f64>,
    pub max_attempts: usize,
    pub cancel: watch::Receiver<bool>,
}

#[derive(Debug, Clone)]
pub struct ModelCallResult {
    pub provider: String,
    pub model_id: String,
    pub text: String,
    pub usage: TokenUsage,
    pub attempts: usize,
    pub failed_models: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFailureClass {
    RateLimited,
    Authentication,
    QuotaExhausted,
    Timeout,
    Availability,
    InvalidUsage,
    Provider,
}

impl ProviderFailureClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RateLimited => "rate_limited",
            Self::Authentication => "authentication",
            Self::QuotaExhausted => "quota_exhausted",
            Self::Timeout => "timeout",
            Self::Availability => "availability",
            Self::InvalidUsage => "invalid_usage",
            Self::Provider => "provider",
        }
    }
}

#[derive(Debug)]
pub enum ModelCallError {
    Cancelled,
    InvalidBudget(String),
    MaxAttemptsZero,
    Planning(String),
    NoRoute(ModelCallPlan),
    EvidenceAppend {
        phase: &'static str,
        source: anyhow::Error,
    },
    AttemptsExhausted {
        attempts: usize,
        class: ProviderFailureClass,
        message: String,
    },
}

impl fmt::Display for ModelCallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "model call cancelled"),
            Self::InvalidBudget(reason) => write!(f, "invalid model call budget: {reason}"),
            Self::MaxAttemptsZero => write!(f, "model call max_attempts must be at least one"),
            Self::Planning(error) => write!(f, "model call planning failed: {error}"),
            Self::NoRoute(plan) => write!(f, "no model call route: {}", plan.selection_reason),
            Self::EvidenceAppend { phase, source } => {
                write!(
                    f,
                    "model call evidence append failed during {phase}: {source}"
                )
            }
            Self::AttemptsExhausted {
                attempts,
                class,
                message,
            } => write!(
                f,
                "model call attempts exhausted after {attempts} attempt(s): {}: {message}",
                class.as_str()
            ),
        }
    }
}

impl std::error::Error for ModelCallError {}

pub struct ModelCallExecutor {
    resolver: Arc<AdapterResolver>,
    sessions: Arc<OperatorSessionService>,
}

impl ModelCallExecutor {
    pub fn new(resolver: Arc<AdapterResolver>, sessions: Arc<OperatorSessionService>) -> Self {
        Self { resolver, sessions }
    }

    pub async fn execute(
        &self,
        mut execution: ModelCallExecution,
    ) -> Result<ModelCallResult, ModelCallError> {
        if execution.max_attempts == 0 {
            return Err(ModelCallError::MaxAttemptsZero);
        }
        if execution
            .remaining_budget_usd
            .is_some_and(|budget| !budget.is_finite() || budget < 0.0)
        {
            return Err(ModelCallError::InvalidBudget(
                "remaining_budget_usd must be finite and non-negative".to_string(),
            ));
        }
        if *execution.cancel.borrow() {
            return Err(ModelCallError::Cancelled);
        }

        let max_attempts = execution.max_attempts.min(3);
        let mut attempts = 0usize;
        let mut remaining_budget = execution.remaining_budget_usd;
        let mut last_failure = None;
        let mut failed_models = Vec::new();

        while attempts < max_attempts {
            apply_remaining_budget(&mut execution.request, remaining_budget);
            let plan =
                plan_model_call(&execution.request, &execution.candidates, &default_policy())
                    .map_err(|error| ModelCallError::Planning(error.to_string()))?;

            let selected = plan.selected.clone();
            self.append_route_event(
                &execution.request,
                OperatorEventType::RoutePlanned,
                "route_planned",
                planned_payload(&plan, selected.as_ref(), attempts + 1),
            )?;

            let Some(candidate) = selected else {
                return Err(ModelCallError::NoRoute(plan));
            };

            attempts += 1;
            let provider = candidate.tier.provider.clone();
            let model_id = candidate.tier.model_id.clone();
            let provider_model_id = candidate.tier.provider_model_id.clone();
            self.append_route_event(
                &execution.request,
                OperatorEventType::RouteAttempted,
                "route_attempted",
                json!({
                    "attempt": attempts,
                    "provider": provider,
                    "model": model_id,
                    "provider_model": provider_model_id,
                }),
            )?;

            let Some(adapter) = (self.resolver)(&provider, &provider_model_id) else {
                let failure = (
                    ProviderFailureClass::Availability,
                    format!("provider resolver missing for {provider}/{provider_model_id}"),
                );
                self.append_failure(&execution.request, &candidate, attempts, &failure)?;
                execution
                    .request
                    .excluded_models
                    .push(candidate.tier.model_id.clone());
                failed_models.push(candidate.tier.model_id.clone());
                last_failure = Some(failure);
                continue;
            };

            let started = Instant::now();
            match run_adapter(
                adapter,
                &provider_model_id,
                &execution.messages,
                &mut execution.cancel,
            )
            .await
            {
                Ok((text, usage)) => {
                    let charged_cost = completed_cost(&candidate, &usage).map_err(|message| {
                        ModelCallError::AttemptsExhausted {
                            attempts,
                            class: ProviderFailureClass::InvalidUsage,
                            message,
                        }
                    });
                    let charged_cost = match charged_cost {
                        Ok(cost) => cost,
                        Err(ModelCallError::AttemptsExhausted { class, message, .. }) => {
                            let failure = (class, message);
                            self.append_failure(
                                &execution.request,
                                &candidate,
                                attempts,
                                &failure,
                            )?;
                            execution
                                .request
                                .excluded_models
                                .push(candidate.tier.model_id.clone());
                            failed_models.push(candidate.tier.model_id.clone());
                            last_failure = Some(failure);
                            continue;
                        }
                        Err(other) => return Err(other),
                    };
                    remaining_budget = subtract_budget(remaining_budget, charged_cost.0);
                    self.append_route_event(
                        &execution.request,
                        OperatorEventType::RouteCompleted,
                        "route_completed",
                        json!({
                            "attempt": attempts,
                            "provider": provider,
                            "model": model_id,
                            "provider_model": provider_model_id,
                            "usage": usage,
                            "latency_ms": started.elapsed().as_millis(),
                            "cost_usd": charged_cost.0,
                            "cost_truth": charged_cost.1,
                            "remaining_budget_usd": remaining_budget,
                            "receipt_ref": serde_json::Value::Null,
                        }),
                    )?;
                    return Ok(ModelCallResult {
                        provider,
                        model_id,
                        text,
                        usage,
                        attempts,
                        failed_models,
                    });
                }
                Err(AdapterRunError::Cancelled) => return Err(ModelCallError::Cancelled),
                Err(AdapterRunError::Failed(message)) => {
                    let failure = (normalize_failure(&message), message);
                    self.append_failure(&execution.request, &candidate, attempts, &failure)?;
                    execution
                        .request
                        .excluded_models
                        .push(candidate.tier.model_id.clone());
                    failed_models.push(candidate.tier.model_id.clone());
                    last_failure = Some(failure);
                }
            }
        }

        let (class, message) = last_failure.unwrap_or((
            ProviderFailureClass::Provider,
            "no provider attempt completed".to_string(),
        ));
        Err(ModelCallError::AttemptsExhausted {
            attempts,
            class,
            message,
        })
    }

    fn append_failure(
        &self,
        request: &ModelCallRequest,
        candidate: &ModelCallCandidate,
        attempt: usize,
        failure: &(ProviderFailureClass, String),
    ) -> Result<(), ModelCallError> {
        self.append_route_event(
            request,
            OperatorEventType::RouteFailed,
            "route_failed",
            json!({
                "attempt": attempt,
                "provider": candidate.tier.provider,
                "model": candidate.tier.model_id,
                "provider_model": candidate.tier.provider_model_id,
                "failure_class": failure.0.as_str(),
                "message": failure.1,
            }),
        )
    }

    fn append_route_event(
        &self,
        request: &ModelCallRequest,
        event_type: OperatorEventType,
        phase: &'static str,
        payload: serde_json::Value,
    ) -> Result<(), ModelCallError> {
        let risk_class = match request.risk {
            heiwa_core::drex::CallRisk::Low => OperatorRisk::Low,
            heiwa_core::drex::CallRisk::Medium => OperatorRisk::Medium,
            heiwa_core::drex::CallRisk::High => OperatorRisk::High,
            heiwa_core::drex::CallRisk::Critical => OperatorRisk::Critical,
        };
        self.sessions
            .append_event(OperatorEvent {
                schema_version: OPERATOR_EVENT_SCHEMA_VERSION,
                event_id: format!("evt-{}", Uuid::new_v4()),
                thread_id: request.thread_id.clone(),
                turn_id: Some(request.turn_id.clone()),
                run_id: None,
                call_id: Some(request.call_id.clone()),
                event_type,
                occurred_at: now_iso(),
                actor: OperatorActor {
                    kind: "runtime".to_string(),
                    id: "model-call-executor".to_string(),
                },
                risk_class,
                sensitivity: OperatorSensitivity::LocalPrivate,
                parent_event_id: None,
                correlation_id: Some(request.call_id.clone()),
                source_refs: vec![],
                evidence_refs: vec![],
                payload,
            })
            .map(|_| ())
            .map_err(|source| ModelCallError::EvidenceAppend { phase, source })
    }
}

pub struct ExecutorLoopCaller {
    executor: Arc<ModelCallExecutor>,
}

impl ExecutorLoopCaller {
    pub fn new(executor: Arc<ModelCallExecutor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl heiwa_loop::LoopModelCaller for ExecutorLoopCaller {
    async fn call(
        &self,
        request: heiwa_loop::LoopCallRequest,
    ) -> anyhow::Result<heiwa_loop::LoopCallResult> {
        let mut submission =
            StartTurnRequest::auto(request.call_id.clone(), request.raw_text.clone());
        submission.route_policy.excluded_models = request.prior_failed_models.clone();
        submission.route_policy.turn_budget_usd = request.remaining_budget_usd;
        submission.route_policy.privacy = request.privacy.as_str().to_string();
        let turn = self
            .executor
            .sessions
            .start_turn(&request.thread_id, submission)?;
        let (_cancel_tx, cancel) = watch::channel(false);
        let result = self
            .executor
            .execute(ModelCallExecution {
                request: ModelCallRequest {
                    thread_id: request.thread_id,
                    turn_id: turn.turn_id,
                    call_id: request.call_id,
                    intent: request.intent,
                    stage: request.stage,
                    raw_text: request.raw_text,
                    privacy: request.privacy,
                    risk: request.risk,
                    safety: SafetyClass::Approved,
                    required_capabilities: vec![],
                    required_context_tokens: 1,
                    minimum_quality_class: 1,
                    minimum_success_rate: 0.0,
                    maximum_marginal_cost_usd: request.remaining_budget_usd,
                    preferred_provider: None,
                    preferred_model: None,
                    allowed_models: vec![],
                    excluded_models: request.prior_failed_models,
                },
                candidates: request.candidates,
                messages: request.messages,
                remaining_budget_usd: request.remaining_budget_usd,
                max_attempts: request.max_attempts,
                cancel,
            })
            .await?;
        Ok(heiwa_loop::LoopCallResult {
            provider: result.provider,
            model_id: result.model_id,
            text: result.text,
            usage: result.usage,
            attempts: result.attempts,
            failed_models: result.failed_models,
        })
    }
}

fn planned_payload(
    plan: &ModelCallPlan,
    selected: Option<&ModelCallCandidate>,
    attempt: usize,
) -> serde_json::Value {
    json!({
        "attempt": attempt,
        "provider": selected.map(|candidate| candidate.tier.provider.as_str()),
        "model": selected.map(|candidate| candidate.tier.model_id.as_str()),
        "provider_model": selected.map(|candidate| candidate.tier.provider_model_id.as_str()),
        "stage": plan.stage,
        "selected_id": plan.selected_id,
        "cost_truth": plan.selected_cost_truth,
        "selection_reason": plan.selection_reason,
        "policy_version": plan.policy_version,
        "rejections": plan.rejected,
    })
}

fn apply_remaining_budget(request: &mut ModelCallRequest, remaining: Option<f64>) {
    if let Some(remaining) = remaining {
        request.maximum_marginal_cost_usd = Some(
            request
                .maximum_marginal_cost_usd
                .map(|current| current.min(remaining))
                .unwrap_or(remaining),
        );
    }
}

fn completed_cost(
    candidate: &ModelCallCandidate,
    usage: &TokenUsage,
) -> Result<(f64, CostTruth), String> {
    if !usage.cost_usd.is_finite() || usage.cost_usd < 0.0 {
        return Err("provider returned non-finite or negative cost_usd".to_string());
    }
    if usage.cost_usd > 0.0 {
        return Ok((usage.cost_usd, CostTruth::ExactProviderReport));
    }
    match (candidate.marginal_cost_usd, &candidate.cost_truth) {
        (Some(cost), truth) if cost.is_finite() && cost >= 0.0 => Ok((cost, truth.clone())),
        (None, CostTruth::CannotConfirm) => Ok((0.0, CostTruth::CannotConfirm)),
        _ => Err("candidate has invalid completion cost truth".to_string()),
    }
}

fn subtract_budget(remaining: Option<f64>, cost: f64) -> Option<f64> {
    remaining.map(|budget| (budget - cost).max(0.0))
}

fn normalize_failure(message: &str) -> ProviderFailureClass {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("rate_limit")
        || normalized.contains("rate limit")
        || normalized.contains("too many requests")
        || normalized.contains("429")
    {
        ProviderFailureClass::RateLimited
    } else if normalized.contains("auth")
        || normalized.contains("unauthorized")
        || normalized.contains("forbidden")
        || normalized.contains("401")
        || normalized.contains("403")
    {
        ProviderFailureClass::Authentication
    } else if normalized.contains("quota") || normalized.contains("credit") {
        ProviderFailureClass::QuotaExhausted
    } else if normalized.contains("timeout") || normalized.contains("timed out") {
        ProviderFailureClass::Timeout
    } else if normalized.contains("unavailable")
        || normalized.contains("connection")
        || normalized.contains("not found")
    {
        ProviderFailureClass::Availability
    } else {
        ProviderFailureClass::Provider
    }
}

enum AdapterRunError {
    Cancelled,
    Failed(String),
}

async fn run_adapter(
    adapter: Arc<dyn ProviderAdapter>,
    model: &str,
    messages: &[Message],
    cancel: &mut watch::Receiver<bool>,
) -> Result<(String, TokenUsage), AdapterRunError> {
    if *cancel.borrow() {
        return Err(AdapterRunError::Cancelled);
    }

    let (stream_tx, mut stream_rx) = mpsc::channel(32);
    let adapter_task = adapter.clone();
    let model = model.to_string();
    let messages = messages.to_vec();
    let task = tokio::spawn(async move {
        let error_tx = stream_tx.clone();
        if let Err(error) = adapter_task.send(&model, &messages, stream_tx).await {
            let _ = error_tx
                .send(StreamEvent::Error(format!("adapter error: {error}")))
                .await;
        }
    });

    let mut text = String::new();
    let mut cancel_open = true;
    loop {
        tokio::select! {
            changed = cancel.changed(), if cancel_open => {
                match changed {
                    Ok(()) if *cancel.borrow() => {
                        task.abort();
                        let _ = tokio::time::timeout(Duration::from_millis(250), adapter.interrupt()).await;
                        return Err(AdapterRunError::Cancelled);
                    }
                    Ok(()) => {}
                    Err(_) => cancel_open = false,
                }
            }
            event = stream_rx.recv() => {
                match event {
                    Some(StreamEvent::Token(token)) => text.push_str(&token),
                    Some(StreamEvent::ToolUse { name, input }) => {
                        text.push_str(&json!({
                            "tool_calls": [{
                                "name": name,
                                "arguments": input,
                            }]
                        }).to_string());
                    }
                    Some(StreamEvent::Done(usage)) => {
                        task.abort();
                        return Ok((text, usage));
                    }
                    Some(StreamEvent::Error(error)) => {
                        task.abort();
                        return Err(AdapterRunError::Failed(error));
                    }
                    None => {
                        task.abort();
                        return Err(AdapterRunError::Failed(
                            "provider stream ended without completion".to_string(),
                        ));
                    }
                }
            }
        }
    }
}
