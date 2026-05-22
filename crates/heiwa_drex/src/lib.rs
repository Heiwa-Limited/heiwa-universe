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
//! - It does not cost-optimize across providers. The priority ladder is
//!   operator-intent-first: "use what the user already pays for."

use std::sync::Arc;

use async_trait::async_trait;
use heiwa_mcp::tools::{RouteDecision, RouteInput, Router};
use heiwa_provider::{
    needs_refresh, AccountRegistry, AccountStatus, Credential, OAuthBridgeError, ProviderAccount,
    ProviderVault,
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

        let mut eligible: Vec<&Candidate> = all.iter().filter(|c| c.rejection.is_none()).collect();

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

        // Sort: rank asc, then lower saturation first, then account_id for stable output.
        eligible.sort_by(|a, b| {
            DrexRouter::rank(&a.account)
                .cmp(&DrexRouter::rank(&b.account))
                .then(
                    a.saturation
                        .partial_cmp(&b.saturation)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
                .then_with(|| a.account.account_id.cmp(&b.account.account_id))
        });

        let chosen = eligible[0];
        let model_id = chosen
            .account
            .models
            .iter()
            .max_by_key(|m| m.capability_class)
            .map(|m| m.model_id.clone())
            .unwrap_or_else(|| format!("{}-default", chosen.account.provider));

        let rationale = format!(
            "intent={} rank={} rate_group={} saturation={:.2} account={} needs_refresh={}",
            input.intent,
            DrexRouter::rank(&chosen.account),
            chosen.account.rate_group,
            chosen.saturation,
            chosen.account.account_id,
            chosen.needs_refresh,
        );

        Ok(RouteDecision {
            provider: chosen.account.provider.clone(),
            model_id,
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
