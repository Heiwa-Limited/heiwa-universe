//! Typed System 1 questions and their wire encoding.
//!
//! Nothing else in the crate writes the request shape by hand. Builders check
//! the limits TypeSafe documents (docs.typesafe.ai/api) so a question the API
//! would reject with a 422 is refused where it is written instead.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// Jev's documented ceiling on options in one Choice.
pub const MAX_CHOICE_OPTIONS: usize = 255;
/// The most levels the API accepts on a Score.
pub const MAX_SCORE_LEVELS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionError {
    EmptyInstructions,
    TooFewOptions(usize),
    TooManyOptions(usize),
    DuplicateOption(String),
    TooFewLevels(usize),
    TooManyLevels(usize),
    DuplicateId(String),
    Empty,
}

impl std::fmt::Display for QuestionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuestionError::EmptyInstructions => write!(f, "instructions must not be empty"),
            QuestionError::TooFewOptions(n) => {
                write!(f, "a choice needs at least 2 options, received {n}")
            }
            QuestionError::TooManyOptions(n) => write!(
                f,
                "a choice accepts at most {MAX_CHOICE_OPTIONS} options, received {n}"
            ),
            QuestionError::DuplicateOption(key) => write!(f, "choice option {key:?} is repeated"),
            QuestionError::TooFewLevels(n) => {
                write!(f, "a score needs at least 2 levels, received {n}")
            }
            QuestionError::TooManyLevels(n) => write!(
                f,
                "a score accepts at most {MAX_SCORE_LEVELS} levels, received {n}"
            ),
            QuestionError::DuplicateId(id) => write!(f, "question id {id:?} is repeated"),
            QuestionError::Empty => write!(f, "a question set needs at least one question"),
        }
    }
}

impl std::error::Error for QuestionError {}

/// One typed question. See `packages/heiwa_system1/architecture.md` §3 for
/// which primitive fits which decision.
#[derive(Debug, Clone, PartialEq)]
pub enum Question {
    /// Mutually exclusive options, in the order they were written.
    Choice {
        instructions: String,
        options: Vec<(String, String)>,
    },
    /// An ordered scale, low to high. Level `i` is `levels[i]`.
    Score {
        instructions: String,
        levels: Vec<String>,
    },
    /// A yes/no claim. TypeSafe names the outcomes `true` and `false`.
    Noul {
        instructions: String,
        when_true: Option<String>,
        when_false: Option<String>,
    },
}

fn instructions(value: impl Into<String>) -> Result<String, QuestionError> {
    let value = value.into();
    if value.trim().is_empty() {
        return Err(QuestionError::EmptyInstructions);
    }
    Ok(value)
}

impl Question {
    pub fn choice<K, D>(
        instructions_text: impl Into<String>,
        options: impl IntoIterator<Item = (K, D)>,
    ) -> Result<Self, QuestionError>
    where
        K: Into<String>,
        D: Into<String>,
    {
        let instructions = instructions(instructions_text)?;
        let mut collected: Vec<(String, String)> = Vec::new();
        for (key, description) in options {
            let key = key.into();
            if collected.iter().any(|(existing, _)| *existing == key) {
                return Err(QuestionError::DuplicateOption(key));
            }
            collected.push((key, description.into()));
        }
        if collected.len() < 2 {
            return Err(QuestionError::TooFewOptions(collected.len()));
        }
        if collected.len() > MAX_CHOICE_OPTIONS {
            return Err(QuestionError::TooManyOptions(collected.len()));
        }
        Ok(Question::Choice {
            instructions,
            options: collected,
        })
    }

    pub fn score<L: Into<String>>(
        instructions_text: impl Into<String>,
        levels: impl IntoIterator<Item = L>,
    ) -> Result<Self, QuestionError> {
        let instructions = instructions(instructions_text)?;
        let levels: Vec<String> = levels.into_iter().map(Into::into).collect();
        if levels.len() < 2 {
            return Err(QuestionError::TooFewLevels(levels.len()));
        }
        if levels.len() > MAX_SCORE_LEVELS {
            return Err(QuestionError::TooManyLevels(levels.len()));
        }
        Ok(Question::Score {
            instructions,
            levels,
        })
    }

    pub fn noul(instructions_text: impl Into<String>) -> Result<Self, QuestionError> {
        Ok(Question::Noul {
            instructions: instructions(instructions_text)?,
            when_true: None,
            when_false: None,
        })
    }

    pub fn noul_with_criteria(
        instructions_text: impl Into<String>,
        when_true: impl Into<String>,
        when_false: impl Into<String>,
    ) -> Result<Self, QuestionError> {
        Ok(Question::Noul {
            instructions: instructions(instructions_text)?,
            when_true: Some(when_true.into()),
            when_false: Some(when_false.into()),
        })
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Question::Choice { .. } => "choice",
            Question::Score { .. } => "score",
            Question::Noul { .. } => "noul",
        }
    }

    pub fn instructions(&self) -> &str {
        match self {
            Question::Choice { instructions, .. }
            | Question::Score { instructions, .. }
            | Question::Noul { instructions, .. } => instructions,
        }
    }

    /// The object TypeSafe expects under `questions[<id>]`.
    pub fn to_wire(&self) -> Value {
        match self {
            Question::Choice {
                instructions,
                options,
            } => {
                let criteria: Map<String, Value> = options
                    .iter()
                    .map(|(key, description)| (key.clone(), Value::String(description.clone())))
                    .collect();
                json!({ "type": "choice", "instructions": instructions, "criteria": criteria })
            }
            Question::Score {
                instructions,
                levels,
            } => json!({ "type": "score", "instructions": instructions, "criteria": levels }),
            Question::Noul {
                instructions,
                when_true,
                when_false,
            } => {
                let mut wire = json!({ "type": "noul", "instructions": instructions });
                if when_true.is_some() || when_false.is_some() {
                    let mut criteria = Map::new();
                    if let Some(text) = when_true {
                        criteria.insert("true".into(), Value::String(text.clone()));
                    }
                    if let Some(text) = when_false {
                        criteria.insert("false".into(), Value::String(text.clone()));
                    }
                    wire["criteria"] = Value::Object(criteria);
                }
                wire
            }
        }
    }
}

/// Named questions asked together in one request, in a fixed order.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestionSet {
    questions: Vec<(String, Question)>,
}

impl QuestionSet {
    pub fn new(questions: Vec<(String, Question)>) -> Result<Self, QuestionError> {
        if questions.is_empty() {
            return Err(QuestionError::Empty);
        }
        for (index, (id, _)) in questions.iter().enumerate() {
            if questions[..index].iter().any(|(earlier, _)| earlier == id) {
                return Err(QuestionError::DuplicateId(id.clone()));
            }
        }
        Ok(QuestionSet { questions })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Question)> {
        self.questions
            .iter()
            .map(|(id, question)| (id.as_str(), question))
    }

    pub fn get(&self, id: &str) -> Option<&Question> {
        self.questions
            .iter()
            .find(|(candidate, _)| candidate == id)
            .map(|(_, question)| question)
    }

    pub fn len(&self) -> usize {
        self.questions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.questions.is_empty()
    }

    /// The `questions` map of a request body.
    pub fn to_wire(&self) -> Value {
        Value::Object(
            self.questions
                .iter()
                .map(|(id, question)| (id.clone(), question.to_wire()))
                .collect(),
        )
    }

    /// A content digest of the exact wording asked. Thresholds calibrated
    /// against one wording do not transfer to another, so records carry this
    /// alongside the question-set name.
    pub fn digest(&self) -> String {
        let encoded: Vec<Value> = self
            .questions
            .iter()
            .map(|(id, question)| json!([id, question.to_wire()]))
            .collect();
        let bytes = serde_json::to_vec(&encoded).unwrap_or_default();
        format!("sha256:{:x}", Sha256::digest(bytes))
    }
}
