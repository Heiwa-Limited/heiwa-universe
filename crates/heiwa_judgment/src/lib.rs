//! Confidence-gated judgment.
//!
//! A fast, cheap, schema-constrained answer is only safe to act on when the
//! model is sure *and* the answer itself is acceptable. This crate turns a
//! typed answer into one of three operational verdicts:
//!
//! | band         | when                        | meaning                        |
//! |--------------|-----------------------------|--------------------------------|
//! | `Auto`       | confidence > 0.85           | dispatch to a tool directly    |
//! | `Deliberate` | 0.50 <= confidence <= 0.85  | escalate to a generative tier  |
//! | `Quarantine` | below, or vetoed, or tied   | park for a human, with reasons |
//!
//! # Why this is in Rust as well as TypeScript
//!
//! The same gate exists in `packages/heiwa_system1`, where the calibration
//! and model-doctor tooling use it outside the runtime. It lives here too
//! because provider credentials live in Rust and must never reach the
//! frontend, so the decision that gates a real action has to be made here.
//!
//! Two gates that can disagree about whether an action is safe would be a
//! serious bug. Both read `testdata/gate_cases.json` and must return
//! identical verdicts; see `tests/gate_contract.rs`. **This implementation is
//! authoritative** — it is the one gating real actions.
//!
//! # Three separate ways to refuse the fast path
//!
//! These are distinct on purpose, because they call for different remedies.
//!
//! - **Veto** — a rule fired on the answer's *content*. Confidence measures
//!   how sure the model is, never whether the answer is acceptable. A
//!   confident "yes, this IS a prompt injection" scores 0.94 and would
//!   otherwise sail into the auto lane. Certainty about a dangerous answer
//!   is the most dangerous case, not the safest.
//! - **Ambiguity** — the top two outcomes are nearly tied. `{a: 0.50,
//!   b: 0.48}` is a coin flip wearing a confident face. Only detectable
//!   because the full distribution is returned. The remedy is to separate
//!   the criteria, not to gather more evidence.
//! - **Low confidence** — the ordinary case. The remedy is more evidence.
//!
//! # One deliberate difference from the TypeScript gate
//!
//! There, a `veto` predicate that *throws* is treated as a veto: a guard
//! that cannot run has not passed, and exceptions are ordinary enough in
//! JavaScript that failing closed is the right call.
//!
//! Here a panicking predicate unwinds. Catching it would need
//! `catch_unwind` and `UnwindSafe` bounds on a closure, which is
//! un-idiomatic and hides a genuine bug: in Rust a panic in a policy
//! predicate is a defect to fix, not a runtime condition to route around.
//! The shared fixture cannot express a throwing predicate, so this
//! divergence is intentional and recorded rather than silently present.

use serde::{Deserialize, Serialize};

pub mod backend;
pub mod decode;
mod error;
pub mod question;

pub use error::{ErrorKind, JudgmentError};

/// A decoded answer from a schema-constrained judgment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Answer {
    /// One option from a fixed set, with the distribution over all options.
    Choice {
        choice: String,
        probabilities: Vec<(String, f64)>,
        confidence: f64,
    },
    /// A probability-weighted position on an ordered scale. `score` is NOT an
    /// index: a three-level scale can legitimately answer 1.5.
    Score {
        score: f64,
        legend: Vec<String>,
        probabilities: Vec<f64>,
        confidence: f64,
    },
    /// A single probability in [0, 1]. Carries no model confidence — see
    /// [`Answer::confidence`].
    Noul { noul: f64 },
}

/// Whether the gated confidence came from the model or was derived here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceSource {
    Model,
    Derived,
}

impl Answer {
    /// The [0, 1] confidence the gate compares against its thresholds.
    ///
    /// Choice and Score report their own, summarising how concentrated the
    /// distribution is. A Noul reports none — a single probability has no
    /// distribution to concentrate — so it is derived as distance from a
    /// coin flip:
    ///
    /// ```text
    /// confidence = |2p - 1|      p=0.5 -> 0.0   p=0.0 and p=1.0 -> 1.0
    /// ```
    ///
    /// Being certain something is false is still being certain.
    pub fn confidence(&self) -> f64 {
        match self {
            Answer::Choice { confidence, .. } | Answer::Score { confidence, .. } => *confidence,
            Answer::Noul { noul } => (2.0 * noul - 1.0).abs(),
        }
    }

    pub fn confidence_source(&self) -> ConfidenceSource {
        match self {
            Answer::Noul { .. } => ConfidenceSource::Derived,
            _ => ConfidenceSource::Model,
        }
    }

    /// How separated the top two outcomes are.
    ///
    /// For a Noul this is distance from 0.5, which is the same quantity: its
    /// implicit two-outcome distribution is {p, 1-p}, whose gap is |2p - 1|.
    pub fn margin(&self) -> f64 {
        let mut values: Vec<f64> = match self {
            Answer::Noul { noul } => return (2.0 * noul - 1.0).abs(),
            Answer::Choice { probabilities, .. } => probabilities.iter().map(|(_, p)| *p).collect(),
            Answer::Score { probabilities, .. } => probabilities.clone(),
        };
        if values.len() < 2 {
            return 1.0;
        }
        values.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        values[0] - values[1]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Auto,
    Deliberate,
    Quarantine,
}

impl Band {
    /// Higher is safer. Used to take the most conservative band in a batch.
    fn severity(self) -> u8 {
        match self {
            Band::Auto => 2,
            Band::Deliberate => 1,
            Band::Quarantine => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Confident,
    Uncertain,
    LowConfidence,
    Ambiguous,
    Vetoed,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    /// Strictly above this dispatches automatically.
    pub auto: f64,
    /// At or above this deliberates; below this quarantines.
    pub deliberate: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            auto: 0.85,
            deliberate: 0.5,
        }
    }
}

impl Thresholds {
    /// Express a Noul's bars as probability distance instead of confidence.
    ///
    /// `Thresholds { auto: 0.95 }` looks like the Choice equivalent but
    /// silently demands `p <= 0.025`, because a Noul's confidence is derived
    /// as `|2p - 1|`. That mismatch is easy to write and hard to see in
    /// review. This states the intent directly:
    ///
    /// ```
    /// # use heiwa_judgment::Thresholds;
    /// // auto-dispatch when p is within 0.05 of certain; deliberate to 0.2
    /// let t = Thresholds::for_noul(0.05, 0.2);
    /// assert!(t.auto < 0.9);
    /// ```
    ///
    /// The bar sits one epsilon below the exact boundary so an answer landing
    /// *at* the stated distance passes the gate's strict `>`.
    pub fn for_noul(auto_within: f64, deliberate_within: f64) -> Self {
        assert!(
            deliberate_within >= auto_within,
            "deliberate_within must be at least auto_within"
        );
        const EPS: f64 = 1e-9;
        Thresholds {
            auto: 1.0 - 2.0 * auto_within - EPS,
            deliberate: 1.0 - 2.0 * deliberate_within - EPS,
        }
    }
}

/// Per-question gate policy.
#[derive(Default)]
pub struct Policy<'a> {
    pub thresholds: Option<Thresholds>,
    /// Minimum gap between the top two outcomes. `None` disables the check.
    pub min_margin: Option<f64>,
    /// Judged and reported, but excluded from a batch verdict.
    pub informational: bool,
    /// Content-based hard stop, outranking every band. Safety rules live
    /// here, because they are facts about the answer rather than about how
    /// sure the model was.
    pub veto: Option<&'a dyn Fn(&Answer) -> bool>,
}

impl std::fmt::Debug for Policy<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Policy")
            .field("thresholds", &self.thresholds)
            .field("min_margin", &self.min_margin)
            .field("informational", &self.informational)
            .field("veto", &self.veto.is_some())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub band: Band,
    pub reason: Reason,
    pub confidence: f64,
    pub confidence_source: ConfidenceSource,
    pub margin: f64,
    pub thresholds: Thresholds,
}

/// Gate one answer.
pub fn gate_answer(answer: &Answer, policy: &Policy<'_>) -> Decision {
    let thresholds = policy.thresholds.unwrap_or_default();
    let confidence = answer.confidence();
    let margin = answer.margin();
    let base = Decision {
        band: Band::Quarantine,
        reason: Reason::Vetoed,
        confidence,
        confidence_source: answer.confidence_source(),
        margin,
        thresholds,
    };

    // The veto runs first: it is the only check whose answer does not depend
    // on confidence, and it outranks every other verdict.
    if let Some(veto) = policy.veto {
        if veto(answer) {
            return base;
        }
    }

    // Ambiguity next. Reported distinctly from low confidence because the
    // remedy differs: separate the criteria, rather than gather evidence.
    if let Some(min_margin) = policy.min_margin {
        if min_margin > 0.0 && margin < min_margin {
            return Decision {
                reason: Reason::Ambiguous,
                ..base
            };
        }
    }

    let (band, reason) = if confidence > thresholds.auto {
        (Band::Auto, Reason::Confident)
    } else if confidence >= thresholds.deliberate {
        (Band::Deliberate, Reason::Uncertain)
    } else {
        (Band::Quarantine, Reason::LowConfidence)
    };

    Decision {
        band,
        reason,
        ..base
    }
}

#[derive(Debug, Clone)]
pub struct BatchGate {
    pub band: Band,
    pub decisions: Vec<(String, Decision)>,
    /// Gating questions whose band equals the overall verdict. Empty when the
    /// verdict is `Auto`, because nothing is blocking.
    pub blockers: Vec<String>,
}

/// Gate a set of answers conservatively: the least safe gating question wins.
///
/// Reporting *which* judgment stopped the fast path is what makes a
/// quarantine actionable rather than merely obstructive.
pub fn gate_batch(answers: &[(String, Answer)], policies: &[(String, Policy<'_>)]) -> BatchGate {
    let default = Policy::default();
    let policy_for = |id: &str| -> &Policy<'_> {
        policies
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, p)| p)
            .unwrap_or(&default)
    };

    let mut worst = Band::Auto;
    let mut decisions = Vec::with_capacity(answers.len());

    for (id, answer) in answers {
        let policy = policy_for(id);
        let decision = gate_answer(answer, policy);
        if !policy.informational && decision.band.severity() < worst.severity() {
            worst = decision.band;
        }
        decisions.push((id.clone(), decision));
    }

    let blockers = if worst == Band::Auto {
        Vec::new()
    } else {
        decisions
            .iter()
            .filter(|(id, d)| !policy_for(id).informational && d.band == worst)
            .map(|(id, _)| id.clone())
            .collect()
    };

    BatchGate {
        band: worst,
        decisions,
        blockers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noul(p: f64) -> Answer {
        Answer::Noul { noul: p }
    }

    #[test]
    fn a_certainly_false_noul_is_as_confident_as_a_certainly_true_one() {
        assert_eq!(noul(0.0).confidence(), noul(1.0).confidence());
        assert_eq!(noul(0.0).confidence(), 1.0);
    }

    #[test]
    fn noul_confidence_is_symmetric_about_the_coin_flip() {
        assert!((noul(0.9).confidence() - noul(0.1).confidence()).abs() < 1e-12);
    }

    #[test]
    fn for_noul_states_the_probability_distance_an_operator_means() {
        // "auto when p is within 0.05 of certain" -> a 0.9 confidence bar,
        // nudged below so p = 0.95 exactly still clears the strict `>`.
        let t = Thresholds::for_noul(0.05, 0.25);
        assert!((t.auto - 0.9).abs() < 1e-6);
        assert!(t.auto < 0.9, "must sit below the nominal bar");

        let policy = Policy {
            thresholds: Some(t),
            ..Policy::default()
        };
        assert_eq!(gate_answer(&noul(0.95), &policy).band, Band::Auto);
        assert_eq!(gate_answer(&noul(0.93), &policy).band, Band::Deliberate);
        assert_eq!(gate_answer(&noul(0.70), &policy).band, Band::Quarantine);
    }

    #[test]
    #[should_panic(expected = "deliberate_within must be at least auto_within")]
    fn for_noul_rejects_an_inverted_pair() {
        Thresholds::for_noul(0.3, 0.1);
    }

    #[test]
    fn a_batch_verdict_is_the_worst_gating_band() {
        let answers = vec![
            (
                "route".to_string(),
                Answer::Choice {
                    choice: "billing".into(),
                    probabilities: vec![("billing".into(), 0.95), ("technical".into(), 0.05)],
                    confidence: 0.95,
                },
            ),
            ("pii".to_string(), noul(0.55)),
        ];
        let result = gate_batch(&answers, &[]);
        assert_eq!(result.band, Band::Quarantine);
        assert_eq!(result.blockers, vec!["pii".to_string()]);
        // Every question is still reported, blocker or not.
        assert_eq!(result.decisions.len(), 2);
    }

    #[test]
    fn a_score_margin_reads_the_top_two_levels_not_the_score() {
        let answer = Answer::Score {
            score: 1.0,
            legend: vec!["a".into(), "b".into(), "c".into()],
            probabilities: vec![0.1, 0.5, 0.4],
            confidence: 0.9,
        };
        assert!((answer.margin() - 0.1).abs() < 1e-12);
    }

    #[test]
    fn an_unknown_question_falls_back_to_default_policy_rather_than_panicking() {
        let answers = vec![("never_configured".to_string(), noul(0.99))];
        assert_eq!(gate_batch(&answers, &[]).band, Band::Auto);
    }
}
