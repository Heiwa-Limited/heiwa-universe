//! DREX routing: binds [`heiwa_provider`], [`heiwa_quota`], and [`heiwa_vault`]
//! into a single routing decision.
//!
//! ## Scope
//!
//! This crate is the **integration seam** between three pieces that already
//! exist independently:
//!
//! - [`heiwa_provider::AccountRegistry`] — what provider accounts the operator
//!   has configured, and what models each has detected.
//! - [`heiwa_provider::ProviderVault`] — whether a stored OAuth secret actually
//!   exists for a given account, and (via [`heiwa_provider::needs_refresh`])
//!   whether it is expiring soon.
//! - [`heiwa_quota::QuotaLedger`] — per-rate-group token and request usage in
//!   the current window.
//!
//! The produced decision implements [`heiwa_mcp::tools::Router`] so MCP's
//! `route_request` tool can call a real router, not just [`FakeRouter`].
//!
//! ## What this crate deliberately does **not** do
//!
//! - It does not run the full DREX scoring vector (see
//!   `apps/heiwa_core/src/drex/` for that heavier machinery). A follow-up
//!   task can wire `plan_route` in once the vault/quota ladder has shown
//!   itself stable.
//! - It does not perform the HTTP refresh dance for expired OAuth tokens; it
//!   only surfaces that a refresh is needed via the `needs_refresh` flag on
//!   returned candidates.
//! ## Selection policy
//!
//! **Cheapest model that clears the capability bar.** [`RouteInput`] carries a
//! capability floor (explicit `min_capability`, else inferred from `intent`).
//! Candidates are filtered to models at or above that floor, priced with
//! `cost_per_1k_input`/`cost_per_1k_output` against the estimated turn size,
//! and the cheapest wins. Saturation, then the rate-group ladder, then
//! `account_id` break ties.
//!
//! Local models cost 0.0, so they still win whenever they are genuinely good
//! enough — which is the honest version of "local first". They no longer win a
//! task they cannot do.
//!
//! This replaced a ladder that sorted by rate group and then took
//! `max_by_key(capability_class)` within the winner. That policy overspent on
//! easy work and underserved hard work, and never read the cost fields at all.

pub mod drex_gate;

use std::sync::Arc;

use async_trait::async_trait;
use heiwa_mcp::tools::{RouteDecision, RouteInput, Router};
use heiwa_provider::{
    needs_refresh, AccountRegistry, AccountStatus, Credential, DetectedModel, OAuthBridgeError,
    ProviderAccount, ProviderVault,
};
use heiwa_quota::QuotaLedger;
use thiserror::Error;

/// Skew, in seconds, used when asking the vault "is this OAuth token about
/// to expire?" A token whose expiry is within this many seconds of `now` is
/// treated as already expired for routing purposes.
pub const DEFAULT_REFRESH_SKEW_SECONDS: u64 = 120;

/// Token usage at or above this fraction of a rate group's configured budget
/// causes the router to skip candidates in that group when alternatives
/// exist. The operator can still land on a saturated group if nothing else
/// is eligible (degraded routing beats no routing).
pub const DEFAULT_QUOTA_SATURATION_RATIO: f64 = 0.95;

/// Rate-group budgets the router treats as hard ceilings for saturation
/// math. Budgets are intentionally conservative; tighten in production.
///
/// This is a stop-gap until a proper budget config lands.
fn default_budget_for(rate_group: &str) -> Option<RateGroupBudget> {
    match rate_group {
        "anthropic_api" => Some(RateGroupBudget {
            token_ceiling: 400_000,
            request_ceiling: 1_000,
        }),
        "anthropic_sub" => Some(RateGroupBudget {
            token_ceiling: 1_000_000,
            request_ceiling: 10_000,
        }),
        "openai_api" => Some(RateGroupBudget {
            token_ceiling: 400_000,
            request_ceiling: 1_000,
        }),
        "openai_sub" => Some(RateGroupBudget {
            token_ceiling: 1_000_000,
            request_ceiling: 10_000,
        }),
        "google" | "google_bonus" => Some(RateGroupBudget {
            token_ceiling: 2_000_000,
            request_ceiling: 100_000,
        }),
        "local" => None, // local runtime has no budget ceiling
        _ => Some(RateGroupBudget {
            token_ceiling: 200_000,
            request_ceiling: 500,
        }),
    }
}

#[derive(Debug, Clone, Copy)]
struct RateGroupBudget {
    token_ceiling: i64,
    request_ceiling: i64,
}

/// Estimated USD for one turn on `model`, given input/output size in *thousands*
/// of tokens. Local models report 0.0 for both rates and therefore price at 0.
pub fn estimate_turn_cost(model: &DetectedModel, in_ktok: f64, out_ktok: f64) -> f64 {
    model.cost_per_1k_input * in_ktok + model.cost_per_1k_output * out_ktok
}

/// The core selection policy, factored out so it is testable without a
/// registry, vault, or quota ledger.
///
/// Returns the **cheapest** model in `models` whose `capability_class` is at
/// least `need`, subject to an optional per-turn cost ceiling.
///
/// Ties on price prefer the model that most closely *meets* the bar —
/// smallest sufficient, not most capable. Dollars are not the only cost:
/// every local model prices at 0.0, but a 9B model is materially slower than
/// a 4B one, and latency is part of return-per-turn. Oversizing a trivial
/// task buys nothing and costs seconds.
///
/// Returns `None` when nothing clears the capability bar or the ceiling.
pub fn cheapest_qualifying(
    models: &[DetectedModel],
    need: u8,
    in_ktok: f64,
    out_ktok: f64,
    max_cost_usd: Option<f64>,
) -> Option<(&DetectedModel, f64)> {
    models
        .iter()
        .filter(|m| m.capability_class >= need)
        .map(|m| (m, estimate_turn_cost(m, in_ktok, out_ktok)))
        .filter(|(_, cost)| max_cost_usd.is_none_or(|ceiling| *cost <= ceiling))
        .min_by(|(ma, ca), (mb, cb)| {
            ca.partial_cmp(cb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(ma.capability_class.cmp(&mb.capability_class))
        })
}

#[derive(Debug, Error)]
pub enum RoutingError {
    #[error("no connected provider accounts are registered")]
    NoAccounts,
    #[error("no candidate survived credential and quota filtering: {reason}")]
    NoEligibleCandidate { reason: String },
    #[error("vault lookup failed: {0}")]
    Vault(#[from] OAuthBridgeError),
    #[error("quota lookup failed: {0}")]
    Quota(#[from] heiwa_quota::QuotaError),
}

pub type Result<T> = std::result::Result<T, RoutingError>;

/// The wiring the router needs to make a decision. Constructed by the
/// hosting runtime and shared as `Arc<DrexRouter>`.
pub struct DrexRouter {
    registry: Arc<AccountRegistry>,
    vault: Arc<ProviderVault>,
    quota: Arc<QuotaLedger>,
    /// `now_unix` is injectable so tests don't have to mock system time.
    now_unix: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl DrexRouter {
    pub fn new(
        registry: Arc<AccountRegistry>,
        vault: Arc<ProviderVault>,
        quota: Arc<QuotaLedger>,
    ) -> Self {
        Self {
            registry,
            vault,
            quota,
            now_unix: Arc::new(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system clock before unix epoch")
                    .as_secs()
            }),
        }
    }

    /// Test helper — inject a deterministic clock.
    pub fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.now_unix = Arc::new(clock);
        self
    }

    /// Enumerate candidates with a per-account eligibility verdict.
    /// Callers can inspect the rejections for debugging; `decide` uses only
    /// the eligible subset.
    pub fn candidates(&self) -> Vec<Candidate> {
        let now = (self.now_unix)();
        self.registry
            .connected_providers()
            .iter()
            .flat_map(|p| {
                self.registry
                    .accounts_for(p)
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .chain(
                // `connected_providers` filters out Disconnected/NeedsAuth; include the full
                // set so we can report rejections as well.
                self.registry
                    .accounts_for("ollama")
                    .iter()
                    .map(|a| (*a).clone())
                    .filter(|a| matches!(a.status, AccountStatus::Disconnected)),
            )
            .map(|account| self.evaluate(&account, now))
            .collect()
    }

    fn evaluate(&self, account: &ProviderAccount, now: u64) -> Candidate {
        // 1. Status gate
        match &account.status {
            AccountStatus::Connected => {}
            AccountStatus::Disconnected => {
                return Candidate::ineligible(account.clone(), Rejection::Disconnected);
            }
            AccountStatus::NeedsAuth => {
                return Candidate::ineligible(account.clone(), Rejection::NeedsAuth);
            }
            AccountStatus::Error(msg) => {
                return Candidate::ineligible(
                    account.clone(),
                    Rejection::ProviderError(msg.clone()),
                );
            }
        }

        // 2. Credential presence — OAuth accounts need a stored secret; OAuthCli
        //    has the provider's own binary; API keys are in the legacy keychain
        //    module but we don't probe them here (registry already knows if
        //    `has_secret` was set at add time).
        let needs_refresh_flag = match &account.credential {
            Credential::OAuth { .. } => match self.vault.load(&account.account_id) {
                Ok(secret) => needs_refresh(&secret, now, DEFAULT_REFRESH_SKEW_SECONDS),
                Err(OAuthBridgeError::NotFound { .. }) => {
                    return Candidate::ineligible(account.clone(), Rejection::OAuthMissingSecret);
                }
                Err(e) => {
                    return Candidate::ineligible(
                        account.clone(),
                        Rejection::VaultError(e.to_string()),
                    );
                }
            },
            _ => false,
        };

        // 3. Quota headroom
        let saturation = match self.quota_saturation(&account.provider, &account.rate_group) {
            Ok(s) => s,
            Err(e) => {
                return Candidate::ineligible(
                    account.clone(),
                    Rejection::QuotaError(e.to_string()),
                );
            }
        };

        let rejection = if saturation >= 1.0 {
            Some(Rejection::QuotaExhausted { saturation })
        } else {
            None
        };

        Candidate {
            account: account.clone(),
            saturation,
            needs_refresh: needs_refresh_flag,
            rejection,
        }
    }

    fn quota_saturation(&self, provider: &str, rate_group: &str) -> heiwa_quota::Result<f64> {
        let Some(budget) = default_budget_for(rate_group) else {
            return Ok(0.0); // no ceiling → never saturated
        };
        let Some(state) = self.quota.get_quota(provider, rate_group)? else {
            return Ok(0.0);
        };
        let token_ratio = state.tokens_used as f64 / budget.token_ceiling.max(1) as f64;
        let req_ratio = state.requests as f64 / budget.request_ceiling.max(1) as f64;
        Ok(token_ratio.max(req_ratio))
    }

    /// Priority rank for a rate group. Lower number = preferred.
    ///
    /// The ladder reflects operator-intent-first routing:
    /// 1. `local` — always try what's free and private first
    /// 2. OAuth-CLI subscriptions — the user already pays flat-rate for these
    /// 3. API keys — metered spend
    fn rank(account: &ProviderAccount) -> u8 {
        if account.rate_group == "local" {
            return 0;
        }
        match &account.credential {
            Credential::LocalRuntime { .. } => 0,
            Credential::OauthCli { .. } => 1,
            Credential::OAuth { .. } => 2,
            Credential::ApiKey => 3,
        }
    }

    /// Make a routing decision. The returned [`RouteDecision`] carries a
    /// rationale string suitable for audit/debug output.
    pub fn decide(&self, input: &RouteInput) -> Result<RouteDecision> {
        let now = (self.now_unix)();
        let mut all: Vec<Candidate> = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // Walk every account across every known provider, de-duplicating.
        let providers: Vec<String> = {
            let mut names: Vec<String> = self.registry.connected_providers().into_iter().collect();
            names.sort();
            names.dedup();
            names
        };
        for provider in providers {
            for account in self.registry.accounts_for(&provider) {
                if seen_ids.insert(account.account_id.clone()) {
                    all.push(self.evaluate(account, now));
                }
            }
        }

        if all.is_empty() {
            return Err(RoutingError::NoAccounts);
        }

        let eligible: Vec<&Candidate> = all.iter().filter(|c| c.rejection.is_none()).collect();

        if eligible.is_empty() {
            let summary = all
                .iter()
                .filter_map(|c| {
                    c.rejection
                        .as_ref()
                        .map(|r| format!("{}={}", c.account.account_id, r))
                })
                .collect::<Vec<_>>()
                .join(", ");
            return Err(RoutingError::NoEligibleCandidate { reason: summary });
        }

        // ── capability-matched selection ────────────────────────────────
        //
        // The old policy sorted by rate-group rank and then took
        // `max_by_key(capability_class)` within the winner — "cheapest
        // provider, then max it out". That is not ROI: it overspends on easy
        // work (a trivial task got the biggest local model) and underserves
        // hard work (a frontier task went local first, then burned a turn
        // failing). It also never read cost_per_1k_* at all.
        //
        // New policy: pick the CHEAPEST model that CLEARS the capability bar.
        // Local models are 0.0 cost, so they still win whenever they are
        // actually good enough — which is the honest version of "local first".
        let need = input.required_capability();
        let in_tok = input.est_input_tokens() as f64 / 1000.0;
        let out_tok = input.output_tokens() as f64 / 1000.0;

        let mut priced: Vec<(&Candidate, &DetectedModel, f64)> = Vec::new();
        for cand in &eligible {
            if let Some((m, cost)) = cheapest_qualifying(
                &cand.account.models,
                need,
                in_tok,
                out_tok,
                input.max_cost_usd,
            ) {
                priced.push((cand, m, cost));
            }
        }

        if priced.is_empty() {
            let have = eligible
                .iter()
                .flat_map(|c| c.account.models.iter())
                .map(|m| m.capability_class)
                .max();
            return Err(RoutingError::NoEligibleCandidate {
                reason: match (have, input.max_cost_usd) {
                    (Some(h), _) if h < need => {
                        format!("task needs capability_class >= {need}, best available is {h}")
                    }
                    (_, Some(c)) => format!("no candidate at capability >= {need} under ${c}"),
                    _ => format!("no model at capability_class >= {need}"),
                },
            });
        }

        // Cost first, then saturation, then the rate-group ladder as a tiebreak,
        // then account_id for stable output.
        priced.sort_by(|(ca, _, cost_a), (cb, _, cost_b)| {
            cost_a
                .partial_cmp(cost_b)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    ca.saturation
                        .partial_cmp(&cb.saturation)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
                .then(DrexRouter::rank(&ca.account).cmp(&DrexRouter::rank(&cb.account)))
                .then_with(|| ca.account.account_id.cmp(&cb.account.account_id))
        });

        let (chosen, model, est_cost) = priced[0];

        let rationale = format!(
            "intent={} need_cap={} chose_cap={} est_cost=${:.5} rank={} rate_group={} \
             saturation={:.2} account={} needs_refresh={} considered={}",
            input.intent,
            need,
            model.capability_class,
            est_cost,
            DrexRouter::rank(&chosen.account),
            chosen.account.rate_group,
            chosen.saturation,
            chosen.account.account_id,
            chosen.needs_refresh,
            priced.len(),
        );

        Ok(RouteDecision {
            provider: chosen.account.provider.clone(),
            model_id: model.model_id.clone(),
            rationale,
        })
    }
}

#[async_trait]
impl Router for DrexRouter {
    async fn route(&self, req: RouteInput) -> std::result::Result<RouteDecision, String> {
        self.decide(&req).map_err(|e| e.to_string())
    }
}

// ─── evaluation output ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Candidate {
    pub account: ProviderAccount,
    pub saturation: f64,
    pub needs_refresh: bool,
    pub rejection: Option<Rejection>,
}

impl Candidate {
    fn ineligible(account: ProviderAccount, rejection: Rejection) -> Self {
        Self {
            account,
            saturation: 0.0,
            needs_refresh: false,
            rejection: Some(rejection),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Rejection {
    Disconnected,
    NeedsAuth,
    ProviderError(String),
    OAuthMissingSecret,
    VaultError(String),
    QuotaError(String),
    QuotaExhausted { saturation: f64 },
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rejection::Disconnected => write!(f, "disconnected"),
            Rejection::NeedsAuth => write!(f, "needs_auth"),
            Rejection::ProviderError(msg) => write!(f, "provider_error:{msg}"),
            Rejection::OAuthMissingSecret => write!(f, "oauth_missing_secret"),
            Rejection::VaultError(msg) => write!(f, "vault_error:{msg}"),
            Rejection::QuotaError(msg) => write!(f, "quota_error:{msg}"),
            Rejection::QuotaExhausted { saturation } => {
                write!(f, "quota_exhausted:{saturation:.2}")
            }
        }
    }
}

// Expose saturation ratio for tests / introspection.
pub fn saturation_ratio() -> f64 {
    DEFAULT_QUOTA_SATURATION_RATIO
}

// ─── selection policy tests ───────────────────────────────────────────────
//
// These cover the policy directly. Before this change the router had ZERO
// tests of its own (all five in the crate lived in `drex_gate`), which is how
// "cheapest rate group, then max_by_key(capability_class)" survived while the
// cost fields went unread.

#[cfg(test)]
mod selection_tests {
    use super::*;
    use heiwa_mcp::tools::RouteInput;
    use heiwa_provider::InventoryTruth;

    fn model(id: &str, class: u8, cin: f64, cout: f64) -> DetectedModel {
        DetectedModel {
            model_id: id.to_string(),
            provider_model_id: id.to_string(),
            provider: "p".into(),
            account_id: "a".into(),
            rate_group: "g".into(),
            capability_class: class,
            context_window: 100_000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: false,
            supports_audio: false,
            cost_per_1k_input: cin,
            cost_per_1k_output: cout,
            inventory_truth: InventoryTruth::Verified,
        }
    }

    /// local small + local big (both free) + a paid frontier model
    fn fleet() -> Vec<DetectedModel> {
        vec![
            model("gemma4", 2, 0.0, 0.0),
            model("qwen3.5:9b", 3, 0.0, 0.0),
            model("opus-5", 5, 0.015, 0.075),
            model("sonnet-5", 4, 0.003, 0.015),
        ]
    }

    fn req(intent: &str, prompt: &str) -> RouteInput {
        RouteInput {
            intent: intent.into(),
            prompt: prompt.into(),
            hints: serde_json::Value::Null,
            min_capability: None,
            max_cost_usd: None,
            est_output_tokens: None,
        }
    }

    #[test]
    fn easy_task_takes_the_small_local_model_not_the_big_one() {
        // The old policy took max_by_key(capability_class) and would have
        // returned qwen3.5:9b here — paying local compute for nothing.
        let f = fleet();
        let (m, cost) = cheapest_qualifying(&f, 2, 1.0, 0.8, None).unwrap();
        assert_eq!(m.model_id, "gemma4");
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn hard_task_skips_free_models_that_cannot_do_it() {
        // Both local models are free, but neither clears class 5. Free is not
        // a reason to route work somewhere it will fail.
        let f = fleet();
        let (m, _) = cheapest_qualifying(&f, 5, 1.0, 0.8, None).unwrap();
        assert_eq!(m.model_id, "opus-5");
    }

    #[test]
    fn picks_the_cheapest_that_clears_the_bar_not_the_best_available() {
        let f = fleet();
        let (m, _) = cheapest_qualifying(&f, 4, 1.0, 0.8, None).unwrap();
        assert_eq!(
            m.model_id, "sonnet-5",
            "opus-5 also clears 4 but costs more"
        );
    }

    #[test]
    fn free_local_wins_whenever_it_genuinely_qualifies() {
        let f = fleet();
        let (m, cost) = cheapest_qualifying(&f, 3, 1.0, 0.8, None).unwrap();
        assert_eq!(m.model_id, "qwen3.5:9b");
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn cost_ceiling_rejects_rather_than_silently_overspending() {
        // sonnet-5 on this turn: 0.003*1 + 0.015*0.8 = 0.015
        let f = fleet();
        assert!(cheapest_qualifying(&f, 4, 1.0, 0.8, Some(0.01)).is_none());
        assert!(cheapest_qualifying(&f, 4, 1.0, 0.8, Some(0.02)).is_some());
    }

    #[test]
    fn no_candidate_when_nothing_clears_the_bar() {
        let weak = vec![model("tiny", 1, 0.0, 0.0)];
        assert!(cheapest_qualifying(&weak, 4, 1.0, 0.8, None).is_none());
    }

    #[test]
    fn equal_cost_prefers_smallest_sufficient_not_most_capable() {
        // Two free models. Dollars tie at 0.0, so latency decides: a 9B model
        // is slower than a 4B one and buys nothing on a task class 3 clears.
        // An earlier version preferred the *bigger* model here, which
        // contradicted `easy_task_takes_the_small_local_model_not_the_big_one`
        // — the two tests disagreeing is what surfaced the real policy.
        let free = vec![model("small", 3, 0.0, 0.0), model("big", 5, 0.0, 0.0)];
        let (m, _) = cheapest_qualifying(&free, 3, 1.0, 0.8, None).unwrap();
        assert_eq!(m.model_id, "small");
    }

    #[test]
    fn intent_infers_a_capability_floor_and_explicit_beats_inferred() {
        assert_eq!(req("classify", "x").required_capability(), 1);
        assert_eq!(req("chat", "x").required_capability(), 2);
        assert_eq!(req("code", "x").required_capability(), 3);
        assert_eq!(req("architect", "x").required_capability(), 4);
        assert_eq!(
            req("who-knows", "x").required_capability(),
            2,
            "safe default"
        );

        let mut r = req("classify", "x");
        r.min_capability = Some(5);
        assert_eq!(r.required_capability(), 5, "explicit wins over inferred");
        r.min_capability = Some(9);
        assert_eq!(r.required_capability(), 5, "clamped to the real ceiling");
    }

    #[test]
    fn turn_cost_scales_with_both_directions() {
        let m = model("x", 3, 0.01, 0.03);
        assert!((estimate_turn_cost(&m, 2.0, 1.0) - 0.05).abs() < 1e-9);
        assert_eq!(
            estimate_turn_cost(&model("local", 3, 0.0, 0.0), 99.0, 99.0),
            0.0
        );
    }
}
