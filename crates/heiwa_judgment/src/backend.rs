//! System 1 transports: TypeSafe's native endpoint, and an OpenAI-compatible
//! structured-output fallback (Ollama and the like).
//!
//! One attempt per evaluation, bounded by a wall-clock budget. Every outcome
//! is an [`Evaluation`] carrying either decoded answers or a typed
//! [`JudgmentError`]; nothing here panics on a runtime condition, and nothing
//! here retries — a caller that needs retries owns that policy and its budget.
//!
//! Requests go only through [`System1Client`], which owns the HTTP policy:
//! redirects are never followed, and a backend classified local is never
//! reached through a proxy. The endpoint [`Backend::is_remote`] judged is the
//! only one that can receive the state.
//!
//! Every [`JudgmentError`] message is generated here. Response bodies,
//! echoed values, and transport library text never enter one, because callers
//! persist these messages as evidence.
//!
//! The fallback is a worse System 1 than Jev and is labelled as one. Its
//! schema constraint honours `enum` but, measured against a live Ollama, not
//! numeric bounds; its self-reported confidence is uncalibrated; and its
//! self-reported distributions are not coherent (a live gemma4 answer put 1.9
//! of probability mass on a five-level scale). So it is asked only for what
//! its grammar enforces — one option, or one level, as an `enum` — plus a
//! confidence, and the answer is recorded as a point mass on that selection:
//! the fallback has no distribution, and says so rather than inventing one.
//! Values pass through unrepaired for the decoder to reject, and confidence
//! is capped at [`FALLBACK_CONFIDENCE_CEILING`] — the gate's `auto` bar,
//! compared with a strict `>` — so a fallback answer can never reach the auto
//! band. See `packages/heiwa_system1/architecture.md` §8 and §14.

use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::decode::{decode_response, Decoded};
use crate::error::{ErrorKind, JudgmentError};
use crate::question::{Question, QuestionSet};

pub const TYPESAFE_BASE_URL: &str = "https://api.typesafe.ai";
pub const TYPESAFE_PATH: &str = "/v1/systemone";
/// Pinned, per TypeSafe's version guidance: thresholds tuned against one
/// version do not transfer when an alias moves (docs.typesafe.ai/models).
pub const TYPESAFE_DEFAULT_MODEL: &str = "jev-1.13.0";
pub const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";
/// Equal to the gate's default `auto` threshold; see the module note.
pub const FALLBACK_CONFIDENCE_CEILING: f64 = 0.85;
/// Output allowance for the fallback: a backstop against a runaway model,
/// not the runaway control (the wall clock is). Sized from live gemma4 runs,
/// where a tighter cap cut answers off mid-way.
pub const BASE_OUTPUT_TOKENS: u32 = 512;
pub const TOKENS_PER_QUESTION: u32 = 256;

#[derive(Debug, Clone, PartialEq)]
pub enum Backend {
    TypeSafe {
        base_url: String,
        api_key: String,
        model: String,
    },
    StructuredLlm {
        base_url: String,
        api_key: Option<String>,
        model: String,
        confidence_ceiling: f64,
    },
}

/// One System 1 call, successful or not, with what it cost in time.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub backend: &'static str,
    pub requested_model: String,
    pub latency_ms: u64,
    pub budget_ms: u64,
    pub outcome: Result<Decoded, JudgmentError>,
}

impl Evaluation {
    /// The fast path stopped being fast. This invalidates the economics of a
    /// System 1 tier before it invalidates correctness.
    pub fn over_budget(&self) -> bool {
        self.latency_ms > self.budget_ms
    }
}

impl Backend {
    pub fn typesafe(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Backend::typesafe_at(TYPESAFE_BASE_URL, api_key, model)
    }

    pub fn typesafe_at(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Backend::TypeSafe {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
        }
    }

    /// An OpenAI-compatible endpoint, e.g. Ollama at `http://127.0.0.1:11434`.
    pub fn structured_llm(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Backend::StructuredLlm {
            base_url: base_url.into(),
            api_key: None,
            model: model.into(),
            confidence_ceiling: FALLBACK_CONFIDENCE_CEILING,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Backend::TypeSafe { .. } => "typesafe",
            Backend::StructuredLlm { .. } => "structured_llm",
        }
    }

    pub fn model(&self) -> &str {
        match self {
            Backend::TypeSafe { model, .. } | Backend::StructuredLlm { model, .. } => model,
        }
    }

    /// The most confidence this backend may report, when it is capped.
    pub fn confidence_ceiling(&self) -> Option<f64> {
        match self {
            Backend::TypeSafe { .. } => None,
            Backend::StructuredLlm {
                confidence_ceiling, ..
            } => Some(*confidence_ceiling),
        }
    }

    /// Whether the state leaves this machine. Only a loopback host is local;
    /// anything unparseable is treated as remote.
    pub fn is_remote(&self) -> bool {
        match self {
            Backend::TypeSafe { .. } => true,
            Backend::StructuredLlm { base_url, .. } => !reqwest::Url::parse(base_url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
                .is_some_and(|host| {
                    matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]" | "::1")
                }),
        }
    }

    async fn evaluate_with(
        &self,
        client: &reqwest::Client,
        state: &str,
        questions: &QuestionSet,
        budget: Duration,
    ) -> Evaluation {
        let started = Instant::now();
        let outcome = match self {
            Backend::TypeSafe {
                base_url,
                api_key,
                model,
            } => typesafe(client, base_url, api_key, model, state, questions, budget).await,
            Backend::StructuredLlm {
                base_url,
                api_key,
                model,
                confidence_ceiling,
            } => {
                structured(
                    client,
                    base_url,
                    api_key.as_deref(),
                    model,
                    *confidence_ceiling,
                    state,
                    questions,
                    budget,
                )
                .await
            }
        };
        Evaluation {
            backend: self.name(),
            requested_model: self.model().to_string(),
            latency_ms: started.elapsed().as_millis() as u64,
            budget_ms: budget.as_millis() as u64,
            outcome,
        }
    }
}

/// A backend and the HTTP policy every judgment request to it uses.
///
/// Built once per backend. The policy is the point: a redirect could carry
/// the state to an origin the locality decision never saw, and so could a
/// proxy taken from the environment or the OS. So redirects are never
/// followed — a redirect is a typed [`ErrorKind::Redirected`] failure — and a
/// local backend is always reached directly.
#[derive(Debug, Clone)]
pub struct System1Client {
    backend: Backend,
    http: reqwest::Client,
}

impl System1Client {
    pub fn new(backend: Backend) -> Result<Self, JudgmentError> {
        let mut builder = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none());
        if !backend.is_remote() {
            builder = builder.no_proxy();
        }
        let http = builder.build().map_err(|_| {
            JudgmentError::new(
                ErrorKind::NotConfigured,
                "the judgment HTTP client could not be built",
            )
        })?;
        Ok(System1Client { backend, http })
    }

    pub fn backend(&self) -> &Backend {
        &self.backend
    }

    pub async fn evaluate(
        &self,
        state: &str,
        questions: &QuestionSet,
        budget: Duration,
    ) -> Evaluation {
        self.backend
            .evaluate_with(&self.http, state, questions, budget)
            .await
    }
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}{path}", base_url.trim_end_matches('/'))
}

/// Send one JSON request; return the parsed JSON body or a typed failure.
async fn post_json(
    request: reqwest::RequestBuilder,
    body: &Value,
    budget: Duration,
    what: &str,
) -> Result<Value, JudgmentError> {
    let response = request
        .timeout(budget)
        .json(body)
        .send()
        .await
        .map_err(|error| transport_error(&error, what))?;
    let status = response.status();
    if status.is_redirection() {
        // Never followed (see `System1Client`), and the Location is not read:
        // it is the provider's text, and following it is the risk.
        return Err(JudgmentError::new(
            ErrorKind::Redirected,
            format!(
                "{what} answered with a redirect (HTTP {}); judgment requests never follow one",
                status.as_u16()
            ),
        )
        .with_status(status.as_u16()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| transport_error(&error, what))?;
    if !status.is_success() {
        // The status is the diagnostic. The body is the provider's text and
        // can echo the request, so it never enters the message.
        return Err(JudgmentError::new(
            ErrorKind::for_status(status.as_u16()),
            format!("{what} returned HTTP {}", status.as_u16()),
        )
        .with_status(status.as_u16()));
    }
    // A 200 carrying HTML is a proxy or a captive portal, not an answer.
    serde_json::from_slice(&bytes)
        .map_err(|_| JudgmentError::schema(format!("{what} returned a non-JSON 200 body")))
}

/// Classify a transport failure with a message written here. The library's
/// own text can include the URL, and a configured URL can carry credentials.
fn transport_error(error: &reqwest::Error, what: &str) -> JudgmentError {
    let (kind, reason) = if error.is_timeout() {
        (ErrorKind::Timeout, "exceeded its latency budget")
    } else if error.is_connect() {
        (ErrorKind::Transport, "could not be connected to")
    } else if error.is_body() || error.is_decode() {
        (
            ErrorKind::Transport,
            "sent a response body that could not be read",
        )
    } else {
        (ErrorKind::Transport, "could not be reached")
    };
    JudgmentError::new(kind, format!("{what} {reason}"))
}

async fn typesafe(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    state: &str,
    questions: &QuestionSet,
    budget: Duration,
) -> Result<Decoded, JudgmentError> {
    if api_key.trim().is_empty() {
        return Err(JudgmentError::new(
            ErrorKind::NotConfigured,
            "TypeSafe backend has no API key",
        ));
    }
    let body = json!({ "state": state, "model": model, "questions": questions.to_wire() });
    let request = client
        .post(endpoint(base_url, TYPESAFE_PATH))
        .bearer_auth(api_key)
        .header(reqwest::header::ACCEPT, "application/json");
    let response = post_json(request, &body, budget, "TypeSafe").await?;
    decode_response(questions, &response)
}

#[allow(clippy::too_many_arguments)]
async fn structured(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    ceiling: f64,
    state: &str,
    questions: &QuestionSet,
    budget: Duration,
) -> Result<Decoded, JudgmentError> {
    let what = format!("structured-llm({model})");
    let max_tokens = BASE_OUTPUT_TOKENS + TOKENS_PER_QUESTION * questions.len() as u32;
    let body = json!({
        "model": model,
        "temperature": 0,
        "max_tokens": max_tokens,
        "messages": [
            {
                "role": "system",
                "content": "You are a classification engine. You emit only JSON conforming to the supplied schema."
            },
            { "role": "user", "content": render_prompt(state, questions) }
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": { "name": "system_one_answers", "strict": true, "schema": json_schema(questions) }
        }
    });
    let mut request = client.post(endpoint(base_url, CHAT_COMPLETIONS_PATH));
    if let Some(key) = api_key.filter(|key| !key.is_empty()) {
        request = request.bearer_auth(key);
    }
    let completion = post_json(request, &body, budget, &what).await?;

    let choice = &completion["choices"][0];
    let content = choice["message"]["content"].as_str().unwrap_or("");
    // Diagnose before parsing: an empty answer from a truncated response is
    // not a syntax error, and calling it one sends the operator to the wrong
    // fix — raise the budget or change model, not repair the schema.
    if content.trim().is_empty() {
        let reasoning = choice["message"]["reasoning"]
            .as_str()
            .map(str::len)
            .unwrap_or(0);
        let truncated = choice["finish_reason"].as_str() == Some("length");
        return Err(JudgmentError::schema(match (truncated, reasoning > 0) {
            (true, true) => format!(
                "{what} hit its output budget while still reasoning ({reasoning} chars of chain-of-thought, no answer); use a non-reasoning model or raise the budget"
            ),
            (true, false) => format!("{what} was truncated at its output budget before answering"),
            (false, true) => format!("{what} returned only reasoning ({reasoning} chars) and no answer"),
            (false, false) => format!("{what} returned no assistant content"),
        }));
    }
    let raw: Value = serde_json::from_str(content)
        .map_err(|_| JudgmentError::schema(format!("{what} emitted content that is not JSON")))?;

    let usage = &completion["usage"];
    let documented = json!({
        "model": model,
        "answers": translate(&raw, questions, ceiling),
        "usage": {
            "input_tokens": usage["prompt_tokens"].as_u64().unwrap_or(0),
            "output_tokens": usage["completion_tokens"].as_u64().unwrap_or(0),
        }
    });
    decode_response(questions, &documented)
}

/// A JSON Schema the fallback is decoded against. `enum` over exactly the
/// offered keys — and over exactly the scale's levels — is what keeps an
/// unoffered answer unreachable; a confidence is required with each.
fn json_schema(questions: &QuestionSet) -> Value {
    let unit = json!({ "type": "number", "minimum": 0, "maximum": 1 });
    let mut properties = Map::new();
    let mut required = Vec::new();
    for (id, question) in questions.iter() {
        required.push(Value::String(id.to_string()));
        let property = match question {
            Question::Choice {
                instructions,
                options,
            } => {
                let keys: Vec<&str> = options.iter().map(|(key, _)| key.as_str()).collect();
                json!({
                    "type": "object",
                    "description": instructions,
                    "properties": {
                        "choice": { "type": "string", "enum": keys },
                        "confidence": unit
                    },
                    "required": ["choice", "confidence"],
                    "additionalProperties": false
                })
            }
            Question::Score {
                instructions,
                levels,
            } => json!({
                "type": "object",
                "description": instructions,
                "properties": {
                    "level": { "type": "integer", "enum": (0..levels.len()).collect::<Vec<_>>() },
                    "confidence": unit
                },
                "required": ["level", "confidence"],
                "additionalProperties": false
            }),
            Question::Noul { instructions, .. } => json!({
                "type": "object",
                "description": instructions,
                "properties": { "noul": unit },
                "required": ["noul"],
                "additionalProperties": false
            }),
        };
        properties.insert(id.to_string(), property);
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn render_prompt(state: &str, questions: &QuestionSet) -> String {
    let mut lines = vec![
        "Evaluate the STATE below against each question independently.".to_string(),
        "Answer only from the state. Do not explain. Emit JSON matching the schema.".to_string(),
        "For each choice pick one option, for each score pick one level, and give each a confidence from 0 to 1."
            .to_string(),
        String::new(),
        "STATE:".to_string(),
        state.to_string(),
        String::new(),
        "QUESTIONS:".to_string(),
    ];
    for (id, question) in questions.iter() {
        match question {
            Question::Choice {
                instructions,
                options,
            } => {
                lines.push(format!("- {id} (choice): {instructions}"));
                for (key, description) in options {
                    lines.push(format!("    {key}: {description}"));
                }
            }
            Question::Score {
                instructions,
                levels,
            } => {
                lines.push(format!(
                    "- {id} (score, one level 0..{}): {instructions}",
                    levels.len() - 1
                ));
                for (level, description) in levels.iter().enumerate() {
                    lines.push(format!("    {level}: {description}"));
                }
            }
            Question::Noul {
                instructions,
                when_true,
                when_false,
            } => {
                lines.push(format!(
                    "- {id} (noul, probability 0..1 that this is true): {instructions}"
                ));
                if let Some(text) = when_true {
                    lines.push(format!("    true means: {text}"));
                }
                if let Some(text) = when_false {
                    lines.push(format!("    false means: {text}"));
                }
            }
        }
    }
    lines.join("\n")
}

/// Reshape the model's selections into TypeSafe's documented answer shape:
/// a point mass on the selected option or level.
///
/// Deliberately permissive: a malformed value passes through as-is so the
/// decoder rejects it with a precise message, rather than being patched into
/// something that merely looks valid. Only confidence is changed, and only
/// ever lowered.
fn translate(raw: &Value, questions: &QuestionSet, ceiling: f64) -> Value {
    let mut answers = Map::new();
    let Some(source) = raw.as_object() else {
        return Value::Object(answers);
    };
    for (id, question) in questions.iter() {
        let Some(answer) = source.get(id).and_then(Value::as_object) else {
            continue;
        };
        let capped = match answer.get("confidence").and_then(Value::as_f64) {
            Some(confidence) => json!(confidence.min(ceiling)),
            None => answer.get("confidence").cloned().unwrap_or(Value::Null),
        };
        let translated = match question {
            Question::Choice { options, .. } => {
                let choice = answer.get("choice").cloned().unwrap_or(Value::Null);
                // An unoffered choice gets no mass; the decoder rejects the
                // choice itself by name.
                let probabilities: Map<String, Value> = options
                    .iter()
                    .map(|(key, _)| {
                        let mass = if choice.as_str() == Some(key.as_str()) {
                            1.0
                        } else {
                            0.0
                        };
                        (key.clone(), json!(mass))
                    })
                    .collect();
                json!({
                    "type": "choice",
                    "choice": choice,
                    "probabilities": probabilities,
                    "confidence": capped,
                })
            }
            Question::Score { levels, .. } => {
                let level = answer.get("level").cloned().unwrap_or(Value::Null);
                // Mass is keyed by the level exactly as given, so a level off
                // the scale lands off the scale and the decoder names it.
                let mut probabilities: Map<String, Value> = (0..levels.len())
                    .map(|index| (index.to_string(), json!(0.0)))
                    .collect();
                let key = match &level {
                    Value::Number(number) => number.to_string(),
                    other => other.to_string(),
                };
                probabilities.insert(key, json!(1.0));
                let score = level
                    .as_u64()
                    .map(|index| json!(index as f64))
                    .unwrap_or_else(|| level.clone());
                json!({
                    "type": "score",
                    "score": score,
                    "legend": by_level(levels.iter().map(|level| Value::String(level.clone()))),
                    "probabilities": probabilities,
                    "confidence": capped,
                })
            }
            Question::Noul { .. } => json!({
                "type": "noul",
                "noul": answer.get("noul").cloned().unwrap_or(Value::Null),
            }),
        };
        answers.insert(id.to_string(), translated);
    }
    Value::Object(answers)
}

/// Key level-ordered values by level index, as TypeSafe does on the wire.
fn by_level(values: impl Iterator<Item = Value>) -> Value {
    Value::Object(
        values
            .enumerate()
            .map(|(level, value)| (level.to_string(), value))
            .collect(),
    )
}
