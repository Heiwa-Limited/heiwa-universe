//! The one place an untrusted System 1 body becomes typed answers.
//!
//! TypeSafe constrains its *model* to the schema; that guarantee does not
//! cover the bytes between the model and this process, nor a fallback backed
//! by an ordinary LLM. So every body is checked against the questions that
//! produced it — option keys, the level keys of a Score, value ranges — and a
//! body that fails is a typed `SchemaViolation`, never a panic.
//!
//! Semantics match `packages/heiwa_system1/src/core/system1/schema.ts`:
//! an answer to a question never asked is dropped, an asked question with no
//! answer is listed in `missing` (whether that matters is policy, decided by
//! the gate), and a malformed answer fails the whole body, because a wrong
//! answer is more dangerous than an absent one.
//!
//! Messages name the question id, what was offered, and counts or numbers —
//! never a string the provider sent. They are persisted as evidence, and a
//! provider can echo the request into any string field. The provider's
//! `model` string is the same kind of value: persist it only through
//! [`model_provenance`].

use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::JudgmentError;
use crate::question::{Question, QuestionSet};
use crate::Answer;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// The model id the provider says answered; `"unknown"` when absent.
    /// Provider-controlled text: persist it through [`model_provenance`].
    pub model: String,
    /// Answers in question-set order.
    pub answers: Vec<(String, Answer)>,
    /// Asked questions that came back with no answer.
    pub missing: Vec<String>,
    pub usage: Usage,
}

impl Decoded {
    pub fn answer(&self, id: &str) -> Option<&Answer> {
        self.answers
            .iter()
            .find(|(candidate, _)| candidate == id)
            .map(|(_, answer)| answer)
    }
}

pub fn decode_response(questions: &QuestionSet, body: &Value) -> Result<Decoded, JudgmentError> {
    let envelope = body
        .as_object()
        .ok_or_else(|| JudgmentError::schema("response body is not a JSON object"))?;
    let answers = envelope
        .get("answers")
        .and_then(Value::as_object)
        .ok_or_else(|| JudgmentError::schema("response has no `answers` object"))?;
    let model = match envelope.get("model") {
        None | Some(Value::Null) => "unknown".to_string(),
        Some(Value::String(model)) => model.clone(),
        Some(_) => return Err(JudgmentError::schema("response `model` is not a string")),
    };
    let usage = decode_usage(envelope.get("usage"))?;

    let mut decoded = Vec::with_capacity(questions.len());
    let mut missing = Vec::new();
    for (id, question) in questions.iter() {
        match answers.get(id) {
            None => missing.push(id.to_string()),
            Some(raw) => decoded.push((id.to_string(), decode_answer(id, question, raw)?)),
        }
    }

    Ok(Decoded {
        model,
        answers: decoded,
        missing,
        usage,
    })
}

fn decode_usage(raw: Option<&Value>) -> Result<Usage, JudgmentError> {
    let Some(raw) = raw.filter(|value| !value.is_null()) else {
        return Ok(Usage::default());
    };
    let usage = raw
        .as_object()
        .ok_or_else(|| JudgmentError::schema("response `usage` is not an object"))?;
    let count = |field: &str| -> Result<u64, JudgmentError> {
        match usage.get(field) {
            None | Some(Value::Null) => Ok(0),
            Some(value) => value.as_u64().ok_or_else(|| {
                JudgmentError::schema(format!("usage `{field}` is not a non-negative integer"))
            }),
        }
    };
    Ok(Usage {
        input_tokens: count("input_tokens")?,
        output_tokens: count("output_tokens")?,
    })
}

fn decode_answer(id: &str, question: &Question, raw: &Value) -> Result<Answer, JudgmentError> {
    let object = raw
        .as_object()
        .ok_or_else(|| JudgmentError::schema(format!("answer {id:?} is not an object")))?;
    if let Some(tag) = object.get("type") {
        if tag.as_str() != Some(question.kind()) {
            return Err(JudgmentError::schema(format!(
                "answer {id:?} is tagged with a type other than {}",
                question.kind()
            )));
        }
    }

    match question {
        Question::Choice { options, .. } => {
            let choice = string_field(id, object, "choice")?;
            if !options.iter().any(|(key, _)| *key == choice) {
                let offered: Vec<&str> = options.iter().map(|(key, _)| key.as_str()).collect();
                return Err(JudgmentError::schema(format!(
                    "choice answer {id:?} selected an option that was not offered ({})",
                    offered.join(", ")
                )));
            }
            let distribution = probability_map(id, object)?;
            if distribution.len() != options.len()
                || !options
                    .iter()
                    .all(|(key, _)| distribution.contains_key(key))
            {
                let offered: Vec<&str> = options.iter().map(|(key, _)| key.as_str()).collect();
                return Err(JudgmentError::schema(format!(
                    "choice answer {id:?} returned a distribution over {} key(s) that are not exactly the offered options ({})",
                    distribution.len(),
                    offered.join(", ")
                )));
            }
            let mut probabilities = Vec::with_capacity(options.len());
            for (key, _) in options {
                probabilities.push((key.clone(), probability(id, key, &distribution[key])?));
            }
            sums_to_one(id, probabilities.iter().map(|(_, p)| *p))?;
            Ok(Answer::Choice {
                choice,
                probabilities,
                confidence: unit_field(id, object, "confidence")?,
            })
        }
        Question::Score { levels, .. } => {
            let level_keys: Vec<String> =
                (0..levels.len()).map(|level| level.to_string()).collect();
            let distribution = probability_map(id, object)?;
            let legend = object
                .get("legend")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    JudgmentError::schema(format!("score answer {id:?} has no `legend` map"))
                })?;
            for (name, map) in [("probabilities", distribution), ("legend", legend)] {
                if map.len() != level_keys.len()
                    || !level_keys.iter().all(|key| map.contains_key(key))
                {
                    return Err(JudgmentError::schema(format!(
                        "score answer {id:?} returned {name} for {} key(s) that are not exactly the levels 0..{} of a {}-level scale",
                        map.len(),
                        levels.len() - 1,
                        levels.len()
                    )));
                }
            }
            let score = finite_field(id, object, "score")?;
            let top = (levels.len() - 1) as f64;
            if !(0.0..=top).contains(&score) {
                return Err(JudgmentError::schema(format!(
                    "score answer {id:?} returned {score}, outside the [0, {top}] scale"
                )));
            }
            let mut probabilities = Vec::with_capacity(levels.len());
            let mut descriptions = Vec::with_capacity(levels.len());
            for key in &level_keys {
                probabilities.push(probability(id, key, &distribution[key])?);
                descriptions.push(
                    legend[key]
                        .as_str()
                        .ok_or_else(|| {
                            JudgmentError::schema(format!(
                                "score answer {id:?} legend level {key} is not a string"
                            ))
                        })?
                        .to_string(),
                );
            }
            sums_to_one(id, probabilities.iter().copied())?;
            Ok(Answer::Score {
                score,
                legend: descriptions,
                probabilities,
                confidence: unit_field(id, object, "confidence")?,
            })
        }
        Question::Noul { .. } => Ok(Answer::Noul {
            noul: unit_field(id, object, "noul")?,
        }),
    }
}

fn string_field(
    id: &str,
    object: &Map<String, Value>,
    field: &str,
) -> Result<String, JudgmentError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| JudgmentError::schema(format!("answer {id:?} has no string `{field}`")))
}

fn finite_field(id: &str, object: &Map<String, Value>, field: &str) -> Result<f64, JudgmentError> {
    object
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| JudgmentError::schema(format!("answer {id:?} has no numeric `{field}`")))
}

fn unit_field(id: &str, object: &Map<String, Value>, field: &str) -> Result<f64, JudgmentError> {
    let value = finite_field(id, object, field)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(JudgmentError::schema(format!(
            "answer {id:?} `{field}` is {value}, outside [0, 1]"
        )));
    }
    Ok(value)
}

fn probability_map<'a>(
    id: &str,
    object: &'a Map<String, Value>,
) -> Result<&'a Map<String, Value>, JudgmentError> {
    object
        .get("probabilities")
        .and_then(Value::as_object)
        .ok_or_else(|| JudgmentError::schema(format!("answer {id:?} has no `probabilities` map")))
}

/// TypeSafe documents every distribution as "floats that sum to 1". The only
/// slack allowed is what rounding each value to two decimals can produce, so
/// an incoherent distribution — which makes any estimate read from it
/// meaningless — is a schema violation rather than an answer.
fn sums_to_one(
    id: &str,
    probabilities: impl ExactSizeIterator<Item = f64>,
) -> Result<(), JudgmentError> {
    let count = probabilities.len();
    let total: f64 = probabilities.sum();
    let slack = 0.005 * count as f64 + 1e-9;
    if (total - 1.0).abs() > slack {
        return Err(JudgmentError::schema(format!(
            "answer {id:?} probabilities sum to {total}, not 1"
        )));
    }
    Ok(())
}

fn probability(id: &str, key: &str, value: &Value) -> Result<f64, JudgmentError> {
    value
        .as_f64()
        .filter(|p| p.is_finite() && (0.0..=1.0).contains(p))
        .ok_or_else(|| {
            JudgmentError::schema(format!(
                "answer {id:?} probability for {key:?} is not a number in [0, 1]"
            ))
        })
}

/// How a provider's own `model` string relates to the model that was asked
/// for, in terms safe to persist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnedModel {
    /// Exactly the requested id.
    Requested,
    /// A version id of the requested family, such as `jev-1.13.0` answering
    /// for `jev-latest`.
    Version,
    /// Anything else. Only its digest is kept.
    Unrecognised,
}

/// Provenance of an answer: the requested model (this process's own
/// configuration) and what the provider claims, reduced to a value that
/// cannot carry echoed text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelProvenance {
    pub requested: String,
    pub returned: ReturnedModel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_digest: Option<String>,
}

/// Reduce a provider-reported model id to safe provenance. A version is
/// accepted only as `<family>-<digits and dots>`, where the family is the
/// requested id's own prefix, so it cannot carry prose or a credential.
pub fn model_provenance(requested: &str, returned: &str) -> ModelProvenance {
    let mut provenance = ModelProvenance {
        requested: requested.to_string(),
        returned: ReturnedModel::Unrecognised,
        returned_version: None,
        returned_digest: None,
    };
    if returned == requested {
        provenance.returned = ReturnedModel::Requested;
        return provenance;
    }
    let family = requested.split('-').next().unwrap_or("");
    let version = returned
        .strip_prefix(family)
        .and_then(|rest| rest.strip_prefix('-'))
        .filter(|_| !family.is_empty() && family.chars().all(|c| c.is_ascii_alphanumeric()));
    let is_version = version.is_some_and(|version| {
        !version.is_empty()
            && version
                .split('.')
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
    });
    if is_version {
        provenance.returned = ReturnedModel::Version;
        provenance.returned_version = Some(returned.to_string());
    } else {
        provenance.returned_digest =
            Some(format!("sha256:{:x}", Sha256::digest(returned.as_bytes())));
    }
    provenance
}
