//! What first run still needs (L2).
//!
//! The roadmap's L2 requirement is that first run establishes a config root,
//! a local identity, and at least one provider account *inside the
//! application*. This module is the single answer to "is that done, and if
//! not, what is missing" — so the CLI, the desktop surface, and the
//! acceptance gate cannot disagree about whether a user is onboarded.
//!
//! Each gap carries its own remedy. A first-run screen that says "not ready"
//! without saying what to do is documentation the user does not have.

use serde::{Deserialize, Serialize};

/// One thing first run still needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStep {
    /// No per-user state root could be resolved.
    StateRoot,
    /// No local identity has been established yet.
    Identity,
    /// No account can serve a turn.
    Provider,
}

impl OnboardingStep {
    pub fn label(self) -> &'static str {
        match self {
            OnboardingStep::StateRoot => "state root",
            OnboardingStep::Identity => "identity",
            OnboardingStep::Provider => "provider",
        }
    }
}

/// A gap and the action that closes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnboardingGap {
    pub step: OnboardingStep,
    /// What is missing, in the user's terms.
    pub detail: String,
    /// The concrete next action. Never empty.
    pub remedy: String,
}

/// Whether this installation is ready, and what is left if not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnboardingState {
    pub complete: bool,
    pub gaps: Vec<OnboardingGap>,
    /// Display name once identity exists, for a first run that can greet.
    pub display_name: Option<String>,
}

/// What the caller observed. Passing these in keeps the projection pure, so
/// every surface can be tested against states that are awkward to produce.
#[derive(Debug, Clone)]
pub struct OnboardingFacts<'a> {
    pub has_state_root: bool,
    pub identity: Option<&'a str>,
    /// Whether any account can serve a turn right now.
    pub has_routable_account: bool,
    /// Why accounts were skipped, when none is routable. Empty means no
    /// account is registered at all, which is a different message: nothing
    /// is broken, nothing is connected.
    pub provider_guidance: String,
}

impl OnboardingState {
    pub fn project(facts: &OnboardingFacts<'_>) -> Self {
        let mut gaps = Vec::new();

        if !facts.has_state_root {
            gaps.push(OnboardingGap {
                step: OnboardingStep::StateRoot,
                detail: "no per-user state directory could be resolved".to_string(),
                remedy: "set HEIWA_HOME (or HOME) to a directory Heiwa may write to, \
                         then run `heiwa setup` again"
                    .to_string(),
            });
        }

        if facts.identity.is_none() {
            gaps.push(OnboardingGap {
                step: OnboardingStep::Identity,
                detail: "this installation has no local identity yet".to_string(),
                remedy: "run `heiwa setup --name \"<your name>\"` to create one; \
                         it stays on this machine"
                    .to_string(),
            });
        }

        if !facts.has_routable_account {
            let detail = if facts.provider_guidance.is_empty() {
                "no provider account is connected".to_string()
            } else {
                facts.provider_guidance.clone()
            };
            gaps.push(OnboardingGap {
                step: OnboardingStep::Provider,
                detail,
                // Both paths are named because they are not interchangeable:
                // `add-key` stores through the OS keychain, which a
                // container or CI runner does not have, and there the
                // environment variable is the only way in.
                remedy: "add a key with `heiwa auth add-key <provider> <key>`, or set the \
                         provider's own variable (ANTHROPIC_API_KEY, OPENAI_API_KEY, \
                         GEMINI_API_KEY, OPENROUTER_API_KEY), or start a local runtime \
                         such as Ollama"
                    .to_string(),
            });
        }

        Self {
            complete: gaps.is_empty(),
            display_name: facts.identity.map(str::to_string),
            gaps,
        }
    }

    /// The first thing to do, or `None` when onboarding is complete.
    pub fn next_step(&self) -> Option<&OnboardingGap> {
        self.gaps.first()
    }

    /// The whole state as text, for a terminal that has no UI to render.
    pub fn report(&self) -> String {
        if self.complete {
            return match &self.display_name {
                Some(name) => format!("Heiwa is set up for {name}."),
                None => "Heiwa is set up.".to_string(),
            };
        }
        let mut lines = vec!["Heiwa is not set up yet:".to_string()];
        for gap in &self.gaps {
            // The detail can be multi-line — the provider gap carries the
            // per-account health text — so indent continuations rather than
            // letting them fall back to column zero under a bullet.
            let mut detail = gap.detail.lines();
            lines.push(format!(
                "  {}: {}",
                gap.step.label(),
                detail.next().unwrap_or_default()
            ));
            for line in detail {
                lines.push(format!("    {}", line.trim_start()));
            }
            lines.push(format!("    -> {}", gap.remedy));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> OnboardingFacts<'static> {
        OnboardingFacts {
            has_state_root: true,
            identity: Some("Ada"),
            has_routable_account: true,
            provider_guidance: String::new(),
        }
    }

    #[test]
    fn an_installation_with_root_identity_and_a_provider_is_complete() {
        let state = OnboardingState::project(&ready());
        assert!(state.complete);
        assert!(state.gaps.is_empty());
        assert_eq!(state.next_step(), None);
        assert!(state.report().contains("Ada"));
    }

    #[test]
    fn a_completely_fresh_install_names_all_three_gaps_in_dependency_order() {
        // Order matters: a user told to add a provider key before a state
        // root exists is being sent to do something that cannot work.
        let state = OnboardingState::project(&OnboardingFacts {
            has_state_root: false,
            identity: None,
            has_routable_account: false,
            provider_guidance: String::new(),
        });

        assert!(!state.complete);
        let steps: Vec<OnboardingStep> = state.gaps.iter().map(|gap| gap.step).collect();
        assert_eq!(
            steps,
            vec![
                OnboardingStep::StateRoot,
                OnboardingStep::Identity,
                OnboardingStep::Provider
            ]
        );
        assert_eq!(state.next_step().unwrap().step, OnboardingStep::StateRoot);
    }

    #[test]
    fn every_gap_carries_an_action_the_user_can_take() {
        // A first-run screen that reports a problem without a remedy is the
        // documentation the user is supposed to no longer need.
        let state = OnboardingState::project(&OnboardingFacts {
            has_state_root: false,
            identity: None,
            has_routable_account: false,
            provider_guidance: String::new(),
        });

        for gap in &state.gaps {
            assert!(
                !gap.remedy.trim().is_empty(),
                "{:?} has no remedy",
                gap.step
            );
            assert!(
                !gap.detail.trim().is_empty(),
                "{:?} has no detail",
                gap.step
            );
        }
    }

    #[test]
    fn a_provider_that_exists_but_is_unusable_reports_its_own_reason() {
        // "No provider connected" is wrong when the user connected one and it
        // is rate-limited or its key expired — the L1 health text is what
        // tells them which.
        let state = OnboardingState::project(&OnboardingFacts {
            has_state_root: true,
            identity: Some("Ada"),
            has_routable_account: false,
            provider_guidance: "anthropic-api-1 (anthropic, api_key): credential rejected"
                .to_string(),
        });

        let gap = state.next_step().expect("a gap");
        assert_eq!(gap.step, OnboardingStep::Provider);
        assert!(gap.detail.contains("credential rejected"));
    }

    #[test]
    fn identity_alone_does_not_make_an_installation_ready() {
        let state = OnboardingState::project(&OnboardingFacts {
            identity: Some("Ada"),
            has_routable_account: false,
            ..ready()
        });

        assert!(!state.complete);
        assert_eq!(state.next_step().unwrap().step, OnboardingStep::Provider);
        assert_eq!(state.display_name.as_deref(), Some("Ada"));
    }

    #[test]
    fn a_provider_alone_does_not_make_an_installation_ready() {
        let state = OnboardingState::project(&OnboardingFacts {
            identity: None,
            ..ready()
        });

        assert!(!state.complete);
        assert_eq!(state.next_step().unwrap().step, OnboardingStep::Identity);
    }

    #[test]
    fn the_report_names_every_gap_and_its_remedy() {
        let state = OnboardingState::project(&OnboardingFacts {
            has_state_root: true,
            identity: None,
            has_routable_account: false,
            provider_guidance: String::new(),
        });

        let report = state.report();
        for gap in &state.gaps {
            assert!(report.contains(&gap.detail), "report omits {:?}", gap.step);
            assert!(
                report.contains(&gap.remedy),
                "report omits remedy for {:?}",
                gap.step
            );
        }
    }

    #[test]
    fn a_multi_line_detail_stays_indented_under_its_gap() {
        // The provider gap carries per-account health text, one line each.
        // Left alone those lines fall to column zero and read as new gaps.
        let state = OnboardingState::project(&OnboardingFacts {
            has_state_root: true,
            identity: Some("Ada"),
            has_routable_account: false,
            provider_guidance: "Connect a provider:\n  acct-1 (anthropic): rejected".to_string(),
        });

        for line in state.report().lines().skip(1) {
            assert!(
                line.starts_with("  "),
                "line escaped its gap's indentation: {line:?}"
            );
        }
    }
}
