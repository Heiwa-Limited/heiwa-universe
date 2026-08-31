# Provider Truth Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make provider candidate ranking and adapter construction consume one evidence-backed, expiring, unforgeable lane witness, with no background inference spend against metered provider quota.

**Architecture:** PTC-1 first introduces a fallible admission boundary over the current account registry, removes every unconditional adapter fallback, binds DREX candidates to private-field witnesses, and lands the structural gate in the same commit. PTC-2 adds typed provider observations to the existing `heiwa_evidence` journal, projects `ProviderTruthSnapshot` deterministically, swaps the admission predicate behind the unchanged caller interface, adds one-call proving authorization, and makes explicit verification preview and receipt every call.

**Tech Stack:** Rust 2021, `serde`, `serde_json`, `thiserror`, `tokio`, existing `heiwa_evidence` JSONL transport/replay, existing DREX and `ModelCallExecutor`, Bash structural gates, Cargo integration tests.

**Spec:** `docs/superpowers/specs/2026-08-31-provider-truth-contract-design.md`

## Global Constraints

- Base work on `origin/dev` commit `d9b18f467443f5bc96bafd5e7546889c77e036ba` or its accepted descendant; do not reuse `.worktrees/claude/`.
- Use `HEIWA_BRANCH_MODE=experimental` on the short-lived provider-truth branch.
- `ProviderAccount` remains identity and non-secret configuration; mutable runtime truth moves to evidence projection.
- Ordinary admission is exactly: account exists AND credential usable AND proof unexpired for the requested lane AND requested model present in verified inventory.
- `AdmittedProviderLane` and `ProvingProviderLane` have private fields and private constructors, and implement neither `Default` nor `Deserialize`.
- The structural gate lands in the same commit as `AdmittedProviderLane`.
- Provider observations append as `provider_observations` through the existing evidence root, transport, envelope, lock, replay, and sensitive-material gate; no second journal is permitted.
- Metered/cloud inference proof expires after seven days; local unmetered proof expires after twenty-four hours.
- No background inference call may consume metered provider quota. Bounded local proving is permitted only under machine resource policy.
- Explicit verification prints the exact account/model/mode/effort lane set and maximum call count before execution, cannot expand that set after confirmation, and uses normal call receipts.
- Automated tests use loopback mocks, fake executables, injected clocks, and disposable evidence roots; they make no live provider inference calls.
- Installed runtime port `7474` and installed state remain untouched; checkout acceptance uses disposable `HEIWA_EVIDENCE_DIR`, `HEIWA_STATE_DIR`, and port `7475` only when runtime verification is reached.
- PTC-3 bounded provider-process execution is a separate security implementation plan. PTC-1/PTC-2 preserve current Work, `ExecutionScope`, lease, and approval checks and do not claim truth admission alone provides process sandboxing.

## File Structure

| Path                                                   | Responsibility                                                                            |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| `crates/heiwa_provider/src/admission.rs`               | lane requests, typed denials, private-field witnesses, legacy/evidence admission, reports |
| `crates/heiwa_provider/src/truth.rs`                   | projection policy, snapshot, proof decay/revocation, four-conjunct evaluation             |
| `crates/heiwa_provider/src/observations.rs`            | classified/redacted observation builders and sink                                         |
| `crates/heiwa_provider/src/routing.rs`                 | witness-only adapter construction and canonical provider/account mapping                  |
| `crates/heiwa_evidence/src/records.rs`                 | durable provider observation wire types                                                   |
| `crates/heiwa_evidence/src/journal.rs`                 | typed append through the existing transport                                               |
| `crates/heiwa_evidence/src/state.rs`                   | typed replay view for `provider_observations`                                             |
| `apps/heiwa_shell/src/model_calls.rs`                  | witness-bound candidates, call observations, one-call proving                             |
| `apps/heiwa_shell/src/cmd/providers.rs`                | provider listing plus verification plan/execute UX                                        |
| `scripts/check_provider_truth_contract.sh`             | witness and adapter-bypass structural gate                                                |
| `crates/heiwa_provider/tests/provider_admission.rs`    | PTC-1 admission/witness tests                                                             |
| `crates/heiwa_provider/tests/provider_truth.rs`        | projection, decay, and revocation tests                                                   |
| `crates/heiwa_evidence/tests/provider_observations.rs` | append/replay/corruption/security tests                                                   |
| `apps/heiwa_shell/tests/provider_verification.rs`      | spend preview, fixed-lane execution, receipt tests                                        |

---

### Task 1: Land the PTC-1 witness, fallible callers, adapter closure, and gate in one commit

**Files:**

- Create: `crates/heiwa_provider/src/admission.rs`
- Create: `crates/heiwa_provider/tests/provider_admission.rs`
- Create: `scripts/check_provider_truth_contract.sh`
- Modify: `crates/heiwa_provider/src/lib.rs`
- Modify: `crates/heiwa_provider/src/routing.rs`
- Modify: `crates/heiwa_provider/src/providers/anthropic_api.rs`
- Modify: `crates/heiwa_provider/src/providers/openai_api.rs`
- Modify: `crates/heiwa_provider/src/providers/gemini_api.rs`
- Modify: `crates/heiwa_provider/src/providers/openrouter.rs`
- Modify: `crates/heiwa_provider/src/providers/claude_code.rs`
- Modify: `crates/heiwa_provider/src/providers/codex_cli.rs`
- Modify: `crates/heiwa_provider/src/providers/gemini_cli.rs`
- Modify: `crates/heiwa_provider/src/providers/ollama.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `apps/heiwa_shell/src/model_calls.rs`
- Modify: `apps/heiwa_shell/tests/model_call_executor.rs`
- Modify: `apps/heiwa_shell/tests/fresh_install.rs`
- Modify: `scripts/check_agent_baseline.sh`
- Modify: `scripts/check_ci_local.sh`
- Modify: `scripts/ci_rust_test_group.sh`

**Interfaces:**

- Consumes: current `AccountRegistry`, `AccountHealth::project_with`, `DetectedModel`, `ModelCallCandidate`, and `ProviderAdapter`.
- Produces: `ProviderLaneRequest`, private-field `AdmittedProviderLane`, `ProviderAdmissionDenial`, `ProviderAdmissionReport`, `admitted_candidates_with`, `resolve_admitted_adapter_with`, and shell-local `AdmittedModelCallCandidate`.

- [ ] **Step 1: Write failing admission tests**

Create `provider_admission.rs` with verified API/CLI/local fixtures and these core assertions:

```rust
#[test]
fn empty_registry_admits_no_lane() {
    let report = admitted_candidates_with(
        &AccountRegistry::default(),
        "default",
        1_000,
        |_| true,
    );
    assert!(report.admitted.is_empty());
}

#[test]
fn inferred_inventory_cannot_mint_a_witness() {
    let registry = registry_with_cli_model(InventoryTruth::Inferred);
    let denial = admit_legacy_lane_with(
        &registry,
        &ProviderLaneRequest::new("openai-cli", "codex", "gpt-5.6-sol", "high"),
        1_000,
        |_| true,
    )
    .expect_err("inferred inventory must deny");
    assert!(matches!(
        denial,
        ProviderAdmissionDenial::ModelInventoryUnverified { .. }
    ));
}

#[test]
fn missing_cli_binary_denies_before_adapter_construction() {
    let registry = registry_with_cli_model(InventoryTruth::Verified);
    let denial = admit_legacy_lane_with(
        &registry,
        &ProviderLaneRequest::new("openai-cli", "codex", "gpt-5.6-sol", "high"),
        1_000,
        |_| false,
    )
    .expect_err("missing executable must deny");
    assert!(matches!(
        denial,
        ProviderAdmissionDenial::ExecutionSubstrateUnavailable { .. }
    ));
}
```

- [ ] **Step 2: Run the focused test to prove the API is absent**

```bash
cargo test -p heiwa-provider --test provider_admission
```

Expected: compilation fails on missing admission symbols.

- [ ] **Step 3: Implement the private witness and transitional predicate**

Create `admission.rs` with these shapes:

```rust
pub const LEGACY_WITNESS_TTL_MS: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderLaneRequest {
    pub account_id: String,
    pub canonical_provider_id: String,
    pub provider_model_id: String,
    pub effort_tier: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionTruthSource {
    LegacyCurrentTruth,
    EvidenceSnapshot,
}

#[derive(Debug, Clone)]
pub struct AdmittedProviderLane {
    account: ProviderAccount,
    model: DetectedModel,
    canonical_provider_id: String,
    effort_tier: String,
    valid_until_ms: u64,
    truth_source: AdmissionTruthSource,
}

impl AdmittedProviderLane {
    fn from_legacy(
        account: ProviderAccount,
        model: DetectedModel,
        canonical_provider_id: String,
        effort_tier: String,
        valid_until_ms: u64,
    ) -> Self {
        Self {
            account,
            model,
            canonical_provider_id,
            effort_tier,
            valid_until_ms,
            truth_source: AdmissionTruthSource::LegacyCurrentTruth,
        }
    }

    pub fn account_id(&self) -> &str { &self.account.account_id }
    pub fn provider(&self) -> &str { &self.canonical_provider_id }
    pub fn provider_model_id(&self) -> &str { &self.model.provider_model_id }
    pub fn effort_tier(&self) -> &str { &self.effort_tier }
    pub fn valid_until_ms(&self) -> u64 { self.valid_until_ms }
    pub fn truth_source(&self) -> AdmissionTruthSource { self.truth_source }
    pub(crate) fn account(&self) -> &ProviderAccount { &self.account }
    pub(crate) fn model(&self) -> &DetectedModel { &self.model }
}
```

Add typed denial variants `AccountMissing`, `CredentialUnusable`, `ExecutionSubstrateUnavailable`, `ModelInventoryUnverified`, and `ModelUnavailable`. `ProviderLaneRequest::new(account_id, provider, provider_model_id, effort_tier)` canonicalizes only the provider name. `admit_legacy_lane_with` must select the named account and exact model, require `InventoryTruth::Verified`, apply `AccountHealth::project_with`, and mint a 60-second legacy witness. `admitted_candidates_with(registry, effort_tier, now_ms, is_installed)` iterates each account/model pair exactly once and retains both admitted candidates and denials.

- [ ] **Step 4: Replace string resolution with witness-only construction**

Delete production `resolve_adapter(provider, model)` and `resolve_adapter_with(registry, provider, model, base)`. Add:

```rust
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ProviderAdapterError {
    #[error("provider witness expired at {valid_until_ms}; now is {now_ms}")]
    WitnessExpired { valid_until_ms: u64, now_ms: u64 },
    #[error("no adapter implementation for admitted provider {provider}")]
    UnsupportedProvider { provider: String },
}

pub fn resolve_admitted_adapter_with(
    lane: &AdmittedProviderLane,
    base_url_override: Option<&str>,
    now_ms: u64,
) -> Result<Arc<dyn ProviderAdapter>, ProviderAdapterError> {
    if now_ms >= lane.valid_until_ms() {
        return Err(ProviderAdapterError::WitnessExpired {
            valid_until_ms: lane.valid_until_ms(),
            now_ms,
        });
    }
    build_adapter_from_lane(lane, base_url_override)
}
```

`build_adapter_from_lane` is the only production constructor and never reloads the registry. Make API and CLI/local constructors `pub(crate)`, remove every API/OpenRouter `from_registry` constructor, and remove CLI/local `Default` implementations. Convert the public unit structs `ClaudeCodeCliAdapter;`, `CodexCliAdapter;`, and `GeminiCliAdapter;` into structs with a private field so external callers cannot instantiate them with a unit-struct expression after `new()` becomes private.

- [ ] **Step 5: Bind ModelCallExecutor candidates to witnesses**

In `model_calls.rs` define:

```rust
pub type AdapterResolver = dyn Fn(
        &AdmittedProviderLane,
    ) -> Result<Arc<dyn ProviderAdapter>, ProviderAdapterError>
    + Send
    + Sync
    + 'static;

#[derive(Clone)]
pub struct AdmittedModelCallCandidate {
    pub candidate: ModelCallCandidate,
    pub lane: AdmittedProviderLane,
}
```

Change `ModelCallExecution.candidates` to `Vec<AdmittedModelCallCandidate>`. Pass only inner DREX candidates into `plan_model_call`, then find the selected witness by `tier.id`. Resolver denial appends a route failure with `provider_invoked: false`. Update the exact constructors in `main.rs`, `model_calls.rs`, `operator.rs`, and `tests/model_call_executor.rs`. Fresh-install tests obtain a witness through admission and never construct an adapter directly.

- [ ] **Step 6: Add the structural gate in the same commit**

Create a self-testing Bash gate that fails on:

- public witness fields or constructors;
- `Default` or `Deserialize` on either witness;
- production `ClaudeCodeCliAdapter::new`, `CodexCliAdapter::new`, `GeminiCliAdapter::new`, `OllamaCliAdapter::new`, or `OpenRouterAdapter::from_registry` outside the sole factory;
- production `AnthropicApiAdapter::from_registry`, `OpenAiApiAdapter::from_registry`, or `GeminiApiAdapter::from_registry`;
- a public unit-struct declaration for the Claude Code, Codex, or Gemini CLI adapter;
- a resolver branch that constructs a CLI after admission miss;
- string-only production resolver calls.

Its success line is:

```bash
printf 'provider_truth_contract=ok sha=%s\n' "$(git rev-parse --short=12 HEAD)"
```

`--self-test` scans temporary safe and violating fixtures and proves the violating fixtures fail. Wire the gate into `check_agent_baseline.sh` and `check_ci_local.sh`. Add `provider_admission` to `runtime_b_targets`.

- [ ] **Step 7: Run PTC-1 tests and gates**

```bash
cargo fmt --all -- --check
cargo test -p heiwa-provider --test provider_admission
cargo test -p heiwa-provider
cargo test -p heiwa-shell --test model_call_executor
cargo test -p heiwa-shell --test fresh_install
bash scripts/check_provider_truth_contract.sh --self-test
bash scripts/check_provider_truth_contract.sh
bash scripts/check_model_call_boundary.sh
bash scripts/ci_rust_test_group.sh --check
```

Expected: all pass without a live provider call.

- [ ] **Step 8: Commit the entire structural slice together**

```bash
git add crates/heiwa_provider apps/heiwa_shell scripts/check_provider_truth_contract.sh scripts/check_agent_baseline.sh scripts/check_ci_local.sh scripts/ci_rust_test_group.sh
git commit -m "fix: require provider admission before adapter construction"
```

Do not split this commit: witness, constructor closure, caller propagation, and gate are one atomic boundary.

### Task 2: Add typed provider observations to the existing journal

**Files:**

- Modify: `crates/heiwa_evidence/src/records.rs`
- Modify: `crates/heiwa_evidence/src/journal.rs`
- Modify: `crates/heiwa_evidence/src/state.rs`
- Modify: `crates/heiwa_evidence/src/lib.rs`
- Create: `crates/heiwa_evidence/tests/provider_observations.rs`
- Modify: `scripts/ci_rust_test_group.sh`

**Interfaces:**

- Consumes: `JsonlTransport`, `EvidenceTransport`, `read_stream`, `find_sensitive`.
- Produces: `PersistedProviderObservation`, typed payload enums, `record_provider_observation`, `ProviderObservationView::replay`.

- [ ] **Step 1: Write failing append, replay, security, corruption, and concurrency tests**

Use this safe fixture:

```rust
fn completed_observation(id: &str) -> PersistedProviderObservation {
    PersistedProviderObservation {
        observation_id: id.to_string(),
        schema_version: PROVIDER_OBSERVATION_SCHEMA_VERSION,
        account_id: "openai-cli".to_string(),
        lane: Some(PersistedProviderLaneKey {
            account_id: "openai-cli".to_string(),
            canonical_provider_id: "codex".to_string(),
            execution_mode: PersistedExecutionMode::OauthCli,
            provider_model_id: "gpt-5.6-sol".to_string(),
            effort_tier: "high".to_string(),
        }),
        occurred_at_ms: 1_000,
        source: ProviderObservationSource::RealCall,
        payload: ProviderObservationPayload::InferenceCompleted {
            context_bucket: ProviderContextBucket::UpTo16k,
            tool_mode: ProviderToolMode::None,
            streaming: true,
            modalities: vec!["text".to_string()],
            receipt_ref: "operator_events:evt-1".to_string(),
        },
        call_id: Some("call-1".to_string()),
        work_id: Some("work-1".to_string()),
        evidence_refs: vec!["operator_events:evt-1".to_string()],
        redaction_applied: true,
    }
}
```

Assert the stream kind is exactly `provider_observations`, the shared envelope version is used, sensitive auth paths cause rejection before file creation, two transports append 200 untorn lines, and corrupt/truncated lines increment `skipped_lines` while valid records replay.

- [ ] **Step 2: Run the test to prove the schema is absent**

```bash
cargo test -p heiwa_evidence --test provider_observations
```

Expected: compilation fails on missing observation types.

- [ ] **Step 3: Add the durable wire schema**

Add typed enums for `PersistedExecutionMode`, `ProviderObservationSource`, `ProviderContextBucket`, `ProviderToolMode`, `ProviderCredentialState`, and this failure class. `PersistedExecutionMode` derives `PartialOrd` and `Ord` because the complete lane key is a `BTreeMap` key:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFailureClass {
    AuthenticationRejected,
    IneligibleTier,
    ModelNotFound,
    RateLimited,
    QuotaExhausted,
    EndpointUnreachable,
    Timeout,
    ProtocolError,
    ProviderInternal,
    Cancelled,
}
```

Define `PersistedProviderLaneKey { account_id, canonical_provider_id, execution_mode, provider_model_id, effort_tier }` and `PersistedProviderObservation` with `observation_id`, schema version, account/lane, `occurred_at_ms`, source, typed payload, optional call/work ids, evidence refs, and `redaction_applied`. Payload variants are substrate, credential, inventory, inference completed, inference failed, and quota observed. The record-level account id and lane-key account id must match or append is rejected. No variant accepts raw stderr, response bodies, prompt text, auth paths, or tokens.

- [ ] **Step 4: Add typed append and replay without another transport**

Extend `EvidenceTransport`:

```rust
fn record_provider_observation(
    &self,
    observation: PersistedProviderObservation,
) -> Result<()>;
```

`JsonlTransport` rejects `redaction_applied == false`, serializes the complete observation, rejects `find_sensitive` matches, then calls existing `append("provider_observations", &observation)`. `NoopTransport` returns success.

Add `ProviderObservationView` in `state.rs`:

```rust
#[derive(Debug, Default, Clone)]
pub struct ProviderObservationView {
    pub observations: Vec<PersistedProviderObservation>,
    pub skipped_lines: usize,
}
```

`replay` uses `read_stream`, accepts only the known provider observation schema, counts malformed/future records, and stable-sorts by `occurred_at_ms`. Re-export types from `lib.rs` and add the integration target to `foundation_a_targets`.

- [ ] **Step 5: Run and commit**

```bash
cargo fmt --all -- --check
cargo test -p heiwa_evidence --test provider_observations
cargo test -p heiwa_evidence --test journal
cargo test -p heiwa_evidence --test state
bash scripts/ci_rust_test_group.sh --check
git add crates/heiwa_evidence scripts/ci_rust_test_group.sh
git commit -m "feat: journal typed provider observations"
```

### Task 3: Project deterministic truth and evaluate the four conjuncts

**Files:**

- Create: `crates/heiwa_provider/src/truth.rs`
- Create: `crates/heiwa_provider/tests/provider_truth.rs`
- Modify: `crates/heiwa_provider/Cargo.toml`
- Modify: `crates/heiwa_provider/src/lib.rs`
- Modify: `Cargo.lock`

**Interfaces:**

- Consumes: `AccountRegistry`, `ProviderObservationView`, injected `at_ms`.
- Produces: `ProviderTruthPolicy::v1`, `ProviderTruthSnapshot::project`, `InferenceProof`, and `evaluate_lane`.

- [ ] **Step 1: Write failing projection tests**

Cover all four missing conjuncts, exact account/model/mode/effort identity, deterministic projection, auth rejection revoking all account proofs, ineligible-tier/model-not-found revoking only the exact lane, rate limit preserving proof while setting a transient constraint, and corrupt observations failing the affected lane closed.

Use exact expiry boundaries:

```rust
#[test]
fn metered_proof_is_invalid_at_valid_until() {
    let fixture = metered_lane_fixture(1_000);
    let policy = ProviderTruthPolicy::v1();
    let before = fixture.snapshot_at(1_000 + policy.metered_proof_ttl_ms - 1);
    assert!(before.evaluate_lane(&fixture.request).is_ok());
    let expired = fixture.snapshot_at(1_000 + policy.metered_proof_ttl_ms);
    assert!(matches!(
        expired.evaluate_lane(&fixture.request),
        Err(ProviderAdmissionDenial::InferenceProofExpired { .. })
    ));
}
```

- [ ] **Step 2: Run the test to prove the projection is absent**

```bash
cargo test -p heiwa-provider --test provider_truth
```

- [ ] **Step 3: Add the evidence dependency and policy**

Add `heiwa_evidence = { path = "../heiwa_evidence" }`. Define:

```rust
pub const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderTruthPolicy {
    pub version: String,
    pub metered_proof_ttl_ms: u64,
    pub local_proof_ttl_ms: u64,
}

impl ProviderTruthPolicy {
    pub fn v1() -> Self {
        Self {
            version: "provider_truth_v1".to_string(),
            metered_proof_ttl_ms: 7 * DAY_MS,
            local_proof_ttl_ms: DAY_MS,
        }
    }
}
```

Define `InferenceProof` with `verified_at_ms`, `valid_until_ms`, context bucket, tool mode, and receipt ref. Define orthogonal account/lane projections and `ProviderTruthSnapshot { at_ms, policy_version, accounts, lanes, skipped_lines }`.

- [ ] **Step 4: Implement deterministic fold and evaluator**

Apply observations oldest-to-newest. Derive expiry from completion time and policy; never persist an `inference_verified` boolean. Add final denial variants `InferenceProofMissing`, `InferenceProofExpired`, and `SnapshotAmbiguous`. In v1, any skipped provider-observation line makes all witness minting return `SnapshotAmbiguous`, because a corrupt line cannot be safely attributed to only one account or lane.

`evaluate_lane` checks, in order:

1. configured account exists;
2. credential and required CLI/local substrate are usable;
3. exact lane proof exists and `at_ms < valid_until_ms`;
4. exact requested model has verified inventory.

Quota/rate pressure remains a post-truth route constraint and does not become a fifth truth conjunct.

- [ ] **Step 5: Run and commit without switching production yet**

```bash
cargo fmt --all -- --check
cargo test -p heiwa-provider --test provider_truth
cargo test -p heiwa-provider
git add Cargo.lock crates/heiwa_provider
git commit -m "feat: project expiring provider truth"
```

Production stays on `LegacyCurrentTruth` until observation writers and proving authorization exist.

### Task 4: Record classified observations from discovery and call outcomes

**Files:**

- Create: `crates/heiwa_provider/src/observations.rs`
- Modify: `crates/heiwa_provider/src/lib.rs`
- Modify: `crates/heiwa_provider/src/detect/mod.rs`
- Modify: `crates/heiwa_provider/src/detect/ollama.rs`
- Modify: `apps/heiwa_shell/src/model_calls.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `apps/heiwa_shell/tests/model_call_executor.rs`
- Modify: `crates/heiwa_provider/tests/provider_auth.rs`

**Interfaces:**

- Consumes: typed evidence records, `JsonlTransport`, discovery results, route receipts, provider errors.
- Produces: `ProviderObservationSink`, `JournalProviderObservationSink`, safe builders, durable success/failure observations.

- [ ] **Step 1: Write failing classification and receipt-linkage tests**

```rust
#[test]
fn ineligible_tier_text_becomes_safe_typed_failure() {
    let failure = classify_provider_failure(
        "IneligibleTierError: unsupported client for this account",
    );
    assert_eq!(failure.class, ProviderFailureClass::IneligibleTier);
    assert_eq!(failure.detail_code.as_deref(), Some("ineligible_tier"));
    assert!(!serde_json::to_string(&failure).unwrap().contains("unsupported client"));
}
```

Add executor tests proving one completed call appends one `InferenceCompleted` referencing its route receipt and one failed invoked call appends one classified failure without raw adapter text. Resolver denial with `provider_invoked: false` appends no inference failure.

- [ ] **Step 2: Implement the sink and safe classifier**

```rust
pub trait ProviderObservationSink: Send + Sync {
    fn record(&self, observation: PersistedProviderObservation) -> anyhow::Result<()>;
}

pub struct JournalProviderObservationSink {
    transport: JsonlTransport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClassifiedProviderFailure {
    pub class: ProviderFailureClass,
    pub detail_code: Option<String>,
    pub retry_after_ms: Option<u64>,
}
```

Classifier order is: ineligible tier, model not found, authentication, rate limit, quota, timeout, connection, protocol, provider internal. It stores only a bounded code and retry metadata.

- [ ] **Step 3: Dual-write discovery observations during compatibility**

Add `auto_discover_with_sink(registry, sink, now_ms)` and keep `auto_discover(registry)` as the production wrapper. Record substrate presence, credential state, and exact inventory/provenance after each discovery. Keep current `status/models` mutation until Task 7. Observation append failure prevents any evidence-backed success claim.

- [ ] **Step 4: Record call outcomes after durable route evidence**

Add `provider_observations: Arc<dyn ProviderObservationSink>` to `ModelCallExecutor`. On completion, append `InferenceCompleted` only after `route_completed` returns a durable event ref. Derive context bucket/tool mode from the actual request, messages, and stream. On invoked failure append the classified enum. Extend shell failure classification with explicit `IneligibleTier` and `ModelNotFound` variants.

- [ ] **Step 5: Run and commit**

```bash
cargo fmt --all -- --check
cargo test -p heiwa-provider observations
cargo test -p heiwa-provider --test provider_auth
cargo test -p heiwa-shell --test model_call_executor
bash scripts/check_model_call_boundary.sh
git add crates/heiwa_provider apps/heiwa_shell
git commit -m "feat: record provider call truth"
```

### Task 5: Switch admission to evidence and add one-call proving

**Files:**

- Modify: `crates/heiwa_provider/src/admission.rs`
- Modify: `crates/heiwa_provider/src/truth.rs`
- Modify: `crates/heiwa_provider/src/routing.rs`
- Modify: `apps/heiwa_shell/src/model_calls.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `crates/heiwa_provider/tests/provider_admission.rs`
- Modify: `crates/heiwa_provider/tests/provider_truth.rs`
- Modify: `apps/heiwa_shell/tests/model_call_executor.rs`

**Interfaces:**

- Consumes: evidence snapshots and durable observation writers.
- Produces: evidence-backed production admission, private `ProvingProviderLane`, `ProvingAuthority`, `ModelCallExecutor::execute_proving`.

- [ ] **Step 1: Write failing unproven/proving tests**

Assert that verified inventory without a completion produces `InferenceProofMissing` and no ordinary witness. Assert three-conjunct proving can mint only for a concrete call id and accepted authority. Executor test consumes the proving witness, records success, reprojects, then mints an ordinary witness for the next call.

- [ ] **Step 2: Add the non-cloneable proving witness**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvingAuthority {
    UserSelected,
    NoProvenLaneUnderBudget,
    ExplicitVerification,
    LocalBackground,
}

#[derive(Debug)]
pub struct ProvingProviderLane {
    account: ProviderAccount,
    model: DetectedModel,
    canonical_provider_id: String,
    effort_tier: String,
    call_id: String,
    valid_until_ms: u64,
    authority: ProvingAuthority,
}
```

Do not derive `Clone`, `Default`, `Serialize`, or `Deserialize`. The private constructor requires account, usable credential/substrate, exact verified inventory, a non-empty call id, and short expiry. `LocalBackground` rejects non-local rate groups.

- [ ] **Step 3: Swap the production predicate behind the unchanged caller function**

Keep `admitted_candidates_with(registry, effort_tier, now_ms, is_installed)`. Change its body to resolve the evidence root, replay `ProviderObservationView`, project `ProviderTruthSnapshot` with policy v1, evaluate exact account/model/mode/effort lanes, and use the injected executable probe as a final fresh CLI/local substrate check.

`AdmittedProviderLane::from_snapshot` stays private, marks `EvidenceSnapshot`, and uses the earliest applicable proof expiry as witness expiry.

- [ ] **Step 4: Add one-call proving execution**

```rust
pub struct ProvingModelCallExecution {
    pub request: ModelCallRequest,
    pub candidate: ModelCallCandidate,
    pub lane: ProvingProviderLane,
    pub messages: Vec<Message>,
    pub remaining_budget_usd: Option<f64>,
    pub cancel: watch::Receiver<bool>,
    pub delta_tx: Option<mpsc::Sender<StreamEvent>>,
}

impl ModelCallExecutor {
    pub async fn execute_proving(
        &self,
        execution: ProvingModelCallExecution,
    ) -> Result<ModelCallResult, ModelCallError>;
}
```

Consume the witness, require matching call ids, resolve through separately named `resolve_proving_adapter`, execute exactly once, and use the ordinary cost/cancel/route receipt/observation path. Never place the proving candidate in ordinary DREX ranking.

A real user task may use `NoProvenLaneUnderBudget` only after no proven route exists and the request cost ceiling admits the selected lane. Explicit provider/model selection uses `UserSelected`. Schedulers cannot use either.

- [ ] **Step 5: Run and commit**

```bash
cargo fmt --all -- --check
cargo test -p heiwa-provider --test provider_admission
cargo test -p heiwa-provider --test provider_truth
cargo test -p heiwa-shell --test model_call_executor
bash scripts/check_provider_truth_contract.sh
bash scripts/check_model_call_boundary.sh
git add crates/heiwa_provider apps/heiwa_shell
git commit -m "feat: admit providers from expiring evidence"
```

### Task 6: Add spend-previewed verification and local-only background proving

**Files:**

- Create: `apps/heiwa_shell/src/cmd/providers.rs`
- Create: `apps/heiwa_shell/tests/provider_verification.rs`
- Modify: `apps/heiwa_shell/src/cmd/mod.rs`
- Modify: `apps/heiwa_shell/src/cli.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `apps/heiwa_shell/tests/smoke.rs`
- Modify: `scripts/ci_rust_test_group.sh`

**Interfaces:**

- Consumes: truth snapshot, proving authorization, `execute_proving`, call receipts, machine resource policy.
- Produces: `ProviderVerificationPlan`, `ProviderVerificationLane`, injected `VerificationExecutor`, `heiwa providers verify`.

- [ ] **Step 1: Write failing spend-safety tests**

Assert preview lists exact lanes and makes zero calls; `--execute --yes` calls exactly the printed lane keys; changed snapshot/lane set aborts before the first call; metered `LocalBackground` is rejected; every successful executed lane returns a route receipt.

- [ ] **Step 2: Move provider listing into a focused command**

Register `cmd::providers` in `cmd/mod.rs` and `cli.rs`. Move the existing top-level listing behavior from `main.rs` without changing list semantics, then remove the old arm after smoke coverage passes.

- [ ] **Step 3: Implement immutable plans and representative profiles**

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ProviderVerificationLane {
    pub lane_key: PersistedProviderLaneKey,
    pub account_id: String,
    pub metered: bool,
    pub proof_state: String,
    pub reason: String,
    pub context_bucket: ProviderContextBucket,
    pub tool_mode: ProviderToolMode,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderVerificationPlan {
    pub plan_digest: String,
    pub snapshot_at_ms: u64,
    pub policy_version: String,
    pub lanes: Vec<ProviderVerificationLane>,
    pub max_calls: usize,
    pub receipt_stream: String,
}
```

Profiles are exact:

| Profile        | Context   | Tool mode |
| -------------- | --------- | --------- |
| `routine`      | up to 4k  | none      |
| `build`        | up to 16k | none      |
| `long-context` | up to 64k | none      |

Without `--profile`, use the latest completed proof traits; if none exist, require one of the three names. `--effort` selects the exact effort tier; absent that flag, reuse the most recent tier for the account/model or use `default` when no proof exists. Sort lanes before hashing/printing. `--execute` prints first, reprojects, aborts on any lane/policy/account mapping change, and executes only the printed vector.

The verification command must not claim provider-native tool proof. Until the separate PTC-3 sandbox/tool-lease design supplies a no-effect tool contract, explicit profiles record `ProviderToolMode::None`; real user-authorized calls are the only source of `Offered` or `Exercised` proof.

- [ ] **Step 4: Implement confirmation and receipt discipline**

Grammar:

```text
heiwa providers verify [--provider ID] [--model ID] [--effort NAME] [--profile routine|build|long-context] [--execute] [--yes] [--json]
```

Without `--execute`, return after printing. Interactive execution confirms after print; noninteractive execution requires `--yes`. Each lane gets a unique call id and `ExplicitVerification` witness, then `execute_proving`. Print route receipt ref and final proof expiry. Evidence failure leaves the lane unproven.

- [ ] **Step 5: Add local-only background proving**

`authorize_background_proving` rejects any rate group other than `local`/`local_ollama` before evaluating machine policy. Require `MachineResourceDecision::Allow`, one concurrent background call, cancellation on battery/thermal/load pressure, and no user-call preemption. Add no metered scheduler branch.

- [ ] **Step 6: Run and commit**

```bash
cargo fmt --all -- --check
cargo test -p heiwa-shell --test provider_verification
cargo test -p heiwa-shell --test smoke providers
cargo test -p heiwa-shell --test model_call_executor
bash scripts/check_provider_truth_contract.sh
bash scripts/ci_rust_test_group.sh --check
git add apps/heiwa_shell scripts/ci_rust_test_group.sh
git commit -m "feat: verify provider lanes with spend preview"
```

### Task 7: Migrate legacy state, remove dual routing truth, and certify

**Files:**

- Modify: `crates/heiwa_provider/src/registry.rs`
- Modify: `crates/heiwa_provider/src/lib.rs`
- Modify: `crates/heiwa_provider/src/detect/mod.rs`
- Modify: `crates/heiwa_provider/src/health.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `apps/heiwa_shell/tests/local_boot.rs`
- Modify: `crates/heiwa_provider/tests/registry_test.rs`
- Modify: `scripts/check_provider_truth_contract.sh`
- Create: `scripts/check_provider_truth_acceptance.sh`
- Modify: `scripts/check_ci_local.sh`
- Modify: `HEIWA.md`
- Modify: `docs/local-self-operation.md`

**Interfaces:**

- Consumes: stable evidence-backed admission and proving/verification.
- Produces: configuration-only accounts, conservative idempotent import, no routing reads/writes of `provider_connections.json`, SHA-stamped acceptance.

- [ ] **Step 1: Write migration tests proving no success is invented**

```rust
#[test]
fn connected_legacy_account_imports_no_inference_success() {
    let observations = import_legacy_provider_account(
        &connected_account_with_verified_model(),
        10_000,
    );
    assert!(observations.iter().any(|observation| matches!(
        observation.payload,
        ProviderObservationPayload::InventoryObserved { .. }
    )));
    assert!(!observations.iter().any(|observation| matches!(
        observation.payload,
        ProviderObservationPayload::InferenceCompleted { .. }
    )));
}
```

Also prove a `provider_connections.json` entry never changes admission.

- [ ] **Step 2: Make registry writes configuration-only**

Introduce a versioned wire format whose new writes omit `status` and `models` while old files still deserialize for one compatibility window. On first legacy load, append `LegacyImport` substrate/credential/inventory observations, never inference completion, save configuration-only state only after all required appends succeed, and persist an idempotent import marker.

Delete runtime routing methods based on stored status/models and replace UI reads with `ProviderTruthSnapshot`.

- [ ] **Step 3: Retire provider_connections routing and writes**

Remove connection-file writers and make the one remaining reader `legacy_provider_connections_for_migration`. It can appear in migration diagnostics only. Extend the structural gate to reject the filename outside that module/tests.

- [ ] **Step 4: Add SHA-stamped acceptance**

`check_provider_truth_acceptance.sh` runs:

```bash
bash scripts/check_provider_truth_contract.sh
bash scripts/check_model_call_boundary.sh
cargo test -p heiwa_evidence --test provider_observations
cargo test -p heiwa-provider --test provider_admission
cargo test -p heiwa-provider --test provider_truth
cargo test -p heiwa-shell --test model_call_executor
cargo test -p heiwa-shell --test provider_verification
cargo test -p heiwa-shell --test fresh_install
```

A clean pass writes HEAD to `.claude/provider-truth-accept-sha`; a dirty pass does not stamp. Wire it into `check_ci_local.sh`.

- [ ] **Step 5: Update canonical docs to implemented truth only**

Document configuration-only accounts, shared-journal observations, four-conjunct admission, proof TTLs, verification syntax, no metered background inference, local background limits, and `available_unproven`. Do not claim PTC-3 process sandbox completion.

- [ ] **Step 6: Run full branch verification**

```bash
cargo fmt --all -- --check
deno fmt --check docs/superpowers/specs/2026-08-31-provider-truth-contract-design.md docs/superpowers/plans/2026-08-31-provider-truth-contract.md HEIWA.md docs/local-self-operation.md
cargo test -p heiwa_evidence --test provider_observations
cargo test -p heiwa-provider
cargo test -p heiwa-shell --test model_call_executor
cargo test -p heiwa-shell --test provider_verification
cargo test -p heiwa-shell --test fresh_install
bash scripts/check_provider_truth_contract.sh --self-test
bash scripts/check_provider_truth_acceptance.sh
bash scripts/check_branch_topology.sh --mode experimental
bash scripts/ci_rust_test_group.sh --check
```

Expected: all pass without network inference. Full local CI, when run, uses `bash scripts/check_ci_local.sh`; unrelated pre-existing repository formatting/toolchain failures are reported separately.

- [ ] **Step 7: Commit migration/certification**

```bash
git add crates/heiwa_provider apps/heiwa_shell scripts/check_provider_truth_contract.sh scripts/check_provider_truth_acceptance.sh scripts/check_ci_local.sh HEIWA.md docs/local-self-operation.md
git commit -m "refactor: make provider evidence the routing truth"
```

- [ ] **Step 8: Re-run clean-tree acceptance and capture SHA**

```bash
bash scripts/check_provider_truth_acceptance.sh
git rev-parse HEAD
git status --porcelain=v1 -uall
```

Expected: acceptance passes, stamp equals HEAD, worktree is clean except ignored stamps.

## Plan Completion Gate

```bash
rg -n 'T[B]D|T[O]DO|implement[[:space:]]+later|fill[[:space:]]+in|as[[:space:]]+appropriate|similar[[:space:]]+to[[:space:]]+Task' docs/superpowers/plans/2026-08-31-provider-truth-contract.md
rg -n '^### Task' docs/superpowers/plans/2026-08-31-provider-truth-contract.md
deno fmt --check docs/superpowers/plans/2026-08-31-provider-truth-contract.md
git diff --check
```

The first command returns no matches. Tasks 1-7 each end in focused tests and a commit. PTC-3 remains outside this plan and requires a separate accepted sandbox design before code changes.
