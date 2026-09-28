# Heiwa Provider Truth Contract Design

Date: 2026-08-31
Status: Approved design basis — Approach A and the four amendments accepted by Devon on 2026-08-31
Base: `origin/dev` at `d9b18f467443f5bc96bafd5e7546889c77e036ba`
Scope: provider identity, observations, projections, route admission, adapter construction, proving calls, and provider-truth evidence
Planes: Execution and Evidence

## Decision

Make provider routing an evidence-backed admission system over the existing
Heiwa journal.

`ProviderAccount` remains durable identity and non-secret configuration.
`ProviderObservation` records append-only facts in the existing
`heiwa_evidence` journal. `ProviderTruthSnapshot` deterministically projects
those facts at a supplied time. Ordinary routing and adapter construction both
require the same unforgeable `AdmittedProviderLane` witness.

The architecture is:

```text
ProviderAccount configuration
          +
ProviderObservation events in the existing evidence journal
          |
          v
ProviderTruthSnapshot(at, policy)
          |
          v
four-conjunct admission
          |
          v
AdmittedProviderLane
          |
          +------> DREX candidate ranking
          |
          +------> adapter construction
```

This is not a new evidence subsystem. `heiwa_evidence` already provides the
required `records.rs` -> `journal.rs` -> `replay.rs` -> `state.rs` path:
typed durable records, versioned append, corruption-tolerant replay, and
materialized views. Provider truth must stop bypassing that path.

The migration is deliberately split:

1. **Callers first:** close the unconditional CLI fallback and make every
   adapter resolver caller handle denial under the current registry-backed
   truth model.
2. **Truth model second:** add provider observations and the deterministic
   snapshot, then replace the admission predicate behind the already-fallible
   interface without changing callers.

The witness and its structural repository gate ship in the same commit.

## Authority Split

Provider-owned apps and provider adapters occupy different authority domains.

Outside Heiwa, owner-managed Codex, Claude Code, Gemini, and similar apps may
act as executive workers beside Devon: they can reason across the machine,
GitHub, and the internet within the permissions Devon grants them. They remain
provider-owned runtimes and are not Heiwa project-scoped subtools.

Inside Heiwa, the same providers are bounded workers. They receive only the
context, data, model lane, tools, filesystem roots, network posture, work
identity, budget, and approvals granted by Heiwa. They do not inherit ambient
authority merely because their owner-managed app is broadly capable.

This design establishes resource truth. It does not treat resource truth as
execution authority:

- `AdmittedProviderLane` proves that a provider lane is real and currently
  eligible for ordinary inference.
- the existing Work-bound execution context and `ExecutionScope` prove what a
  particular call may read or do;
- leases and approvals govern external effects;
- provider-owned authentication remains outside the journal and outside the
  model prompt.

The provider-truth implementation must not weaken those existing boundaries.
The separate provider-sandbox work must make provider process construction and
tool execution consume the Work-bound scope; a truth witness alone never grants
filesystem, network, shell, or external-effect authority.

## Current Failure

At the accepted base SHA, Heiwa computes incompatible answers to “is this lane
real?”:

- `AccountRegistry` persists mutable `status` and `models` in
  `~/.heiwa/accounts.json`;
- legacy provider discovery separately persists
  `provider_connections.json` and consults provider-owned auth files;
- `AccountHealth::project` checks stored state and executable presence;
- `get_live_model_tiers_with` filters candidates through one health path;
- `resolve_adapter_with` consults another path and then unconditionally
  constructs Claude Code, Codex, and Gemini CLI adapters;
- the CLI adapters' public, zero-argument `new()` constructors do not consult
  the registry, check executable presence, verify authentication, verify model
  inventory, or prove inference eligibility;
- OpenRouter construction is fallible, but `from_registry()` accepts the first
  provider-name match without checking status, credential usability, or the
  requested model.

Therefore candidate ranking and execution can disagree. A lane may disappear
from the health-filtered list and still be instantiated by fallback. Gemini's
real `IneligibleTierError` demonstrates why installation and configuration are
not inference proof.

The defect is structural: the result of provider admission is recomputed as
loosely related booleans instead of being passed as one value.

## Goals

- Make provider admission a single typed decision shared by ranking and
  adapter construction.
- Deny before any process spawn or metered request when the requested lane is
  not proven.
- Keep account identity/configuration separate from observed runtime truth.
- Record provider truth in the existing evidence journal, with deterministic
  replay and no provider-only journal.
- Make inference proof time-bounded and immediately revocable by classified
  failure.
- Never spend metered provider inference quota in the background.
- Permit bounded local Ollama proving when power, thermal, and load policy
  allow it.
- Let a real user-authorized call prove an unproven lane without weakening the
  ordinary-admission witness.
- Make explicit verification preview every lane and potential spend before
  execution and produce normal call receipts.
- Preserve provider-owned auth, quota, and model ownership.
- Preserve honest UI states: configured, available-unproven, admitted,
  temporarily constrained, and unavailable are not synonyms.

## Non-Goals

- Replacing provider-owned apps, auth stores, inference internals, prompts, or
  subscription semantics.
- Introducing a hosted inference proxy or hosted control plane.
- Adding a second provider evidence root, database, journal implementation, or
  mutable connection-status file.
- Making a trivial synthetic canary the definition of inference readiness.
- Treating executable discovery, successful login metadata, or model listing
  as proof that representative inference works.
- Repairing the entire DREX quality metric loop in this migration. Closed-loop
  `last_success_rate`, minimum-success-rate defaults, and evaluator provenance
  remain a follow-up founded on truthful call receipts.
- Claiming that provider truth alone sandboxes provider processes. Call scope,
  tool leases, and process containment remain independently enforced
  contracts.
- Depending on the unmerged claim-registry work. Provider truth may later
  publish claims into that system after it is accepted, but this design is
  complete without it.

## Existing Substrate

The implementation extends these current seams:

- `crates/heiwa_evidence/src/records.rs` owns durable typed records.
- `crates/heiwa_evidence/src/journal.rs` owns the versioned JSONL envelope,
  cross-process stream lock, append, and fsync.
- `crates/heiwa_evidence/src/replay.rs` owns corruption-tolerant replay.
- `crates/heiwa_evidence/src/state.rs` owns deterministic materialized views.
- `crates/heiwa_evidence/src/sensitive.rs` detects credential-shaped material
  and rejects it before durable append.
- `crates/heiwa_provider/src/registry.rs` owns account configuration and
  provider-owned secret references.
- `crates/heiwa_provider/src/health.rs` is the current transitional health
  projection.
- `crates/heiwa_provider/src/routing.rs` is the current account-selection and
  adapter-construction boundary.
- `crates/heiwa_provider/src/adapter.rs` is the normalized inference transport
  contract.

The existing evidence journal uses one stream per record kind under the same
root. `ProviderObservation` therefore rides that journal as the
`provider_observations` record kind. A dedicated stream file inside the shared
journal is normal journal partitioning; a second root, transport, replay
implementation, or provider-owned ledger is prohibited.

## Core Domain Contracts

### Provider Account

`ProviderAccount` is durable configuration, not observed truth.

It retains:

- `account_id`;
- canonical provider/vendor identity;
- credential reference kind, never the secret;
- execution mode (`api_key`, `oauth`, `oauth_cli`, `local_runtime`);
- rate group;
- configured endpoint or executable name where applicable;
- user preferences and enabled/disabled intent.

It no longer owns:

- mutable connection status;
- latest error text;
- inferred health;
- current inventory;
- `inference_verified`;
- last-success rate;
- quota availability.

Those are observations or projections. `accounts.json` remains the durable
identity/configuration source during this design; it must not become a second
runtime-truth ledger.

### Provider Lane Key

Provider truth is keyed more narrowly than a provider name:

```rust
ProviderLaneKey {
    account_id,
    canonical_provider_id,
    execution_mode,
    provider_model_id,
    effort_tier,
}
```

The account distinguishes subscription seats from metered API keys. Execution
mode distinguishes a provider CLI from a direct API. Model and effort tier
prevent a success on a cheap/default path from proving a materially different
lane.

The completed call observation also records bounded request traits:

- input-context bucket, not raw prompt content;
- whether provider-native tool use was requested and observed;
- streaming mode;
- modality requirements;
- the existing `call_id`, `work_id`, and receipt reference when present.

These traits make the proof representative and auditable. They do not duplicate
the transcript or the full model-call receipt.

### Provider Observation

`ProviderObservation` is an append-only, versioned record with:

- `observation_id`;
- `schema_version`;
- `account_id`;
- optional `ProviderLaneKey`;
- `occurred_at`;
- typed source/provenance;
- typed observation payload;
- optional existing `call_id`, `work_id`, and `evidence_refs`;
- `redaction_applied: true`.

The initial payload families are:

| Observation                    | Durable fact                                                                              |
| ------------------------------ | ----------------------------------------------------------------------------------------- |
| `execution_substrate_observed` | configured executable/endpoint is present or absent                                       |
| `credential_observed`          | usable, absent, rejected, expired, or unknown; never the credential                       |
| `inventory_observed`           | exact provider model inventory and `Verified`, `Inferred`, or `UserConfigured` provenance |
| `inference_completed`          | a real call for one lane completed with bounded request traits and a receipt reference    |
| `inference_failed`             | a real call failed with a typed, redacted failure class                                   |
| `quota_observed`               | available, temporarily constrained, exhausted, or unknown, with bounded retry metadata    |

Raw provider stderr, HTTP bodies, auth paths, tokens, prompt content, and
provider-owned config are forbidden. Writers first map raw failures into a
small typed class and bounded safe detail, then call `find_sensitive` over the
complete serialized observation. A sensitive match aborts the append.

`sensitive.rs` is a final rejection gate, not a redaction engine. Redaction is
the writer's responsibility.

### Provider Truth Snapshot

`ProviderTruthSnapshot::project(journal, accounts, at, policy)` is a pure,
deterministic fold. Tests inject `at`; production supplies the current time.

The snapshot keeps orthogonal dimensions instead of collapsing them into one
status string:

- account/configuration presence;
- execution-substrate presence;
- credential state and observation time;
- verified model inventory with provenance;
- inference proof and proof scope per lane;
- transient quota/rate-limit constraint;
- last classified failure;
- observation and policy versions;
- skipped/corrupt-line count.

For identical account configuration, journal bytes, projection time, and policy
version, projection must return identical output.

Unknown future observation variants are skipped and counted. Corrupt lines are
reported, not fatal. A snapshot with corruption may display known state, but it
must not mint a new witness from an ambiguous affected lane.

### Proof Freshness

`inference_verified` is derived and decays. It is never persisted as a sticky
boolean.

Initial policy defaults are:

- metered/cloud inference proof: seven days;
- local unmetered inference proof: twenty-four hours;
- any typed credential rejection, ineligible-tier response, model-not-found
  response, or execution-substrate disappearance revokes the affected proof
  immediately;
- a rate limit records a temporary constraint but does not rewrite credential
  or model truth;
- a successful real call creates or refreshes proof only for its exact lane.

The TTLs live in one versioned `ProviderTruthPolicy`, use an injected clock,
and may later become user policy. The snapshot exposes `verified_at`,
`valid_until`, and the request traits that produced the proof.

A historical completed call remains evidence after expiry. Only its ability to
authorize ordinary routing decays.

### Four-Conjunct Admission

Ordinary provider admission is exactly:

```text
account exists
AND credential is usable
AND inference proof covers the requested lane and is unexpired
AND requested model is present in verified inventory
```

`credential is usable` is a derived term. For API accounts it requires a
resolvable non-secret credential reference with no newer rejection. For CLI
and local-runtime accounts it also requires the configured execution substrate
to be present. It never exposes or persists the secret.

Known quota pressure, privacy policy, cost ceilings, Work scope, and task
capability requirements remain route constraints applied after resource-truth
admission. They must not be folded into a misleading account “connected”
boolean.

If any conjunct fails, admission returns a typed denial before adapter
construction or process spawn.

### Unforgeable Witness

`AdmittedProviderLane` is public as a type but not constructible by callers:

- every field is private;
- its constructor is private to the admission module;
- it does not implement `Deserialize` or `Default`;
- it cannot be created with a public struct literal;
- its lane key, account reference, snapshot revision, policy version, and
  `valid_until` are read through bounded accessors;
- adapter construction rechecks `valid_until` against the injected/current
  clock to close the delay between selection and execution;
- it is never persisted across restart. A recovered route is admitted again.

Candidate generation returns an internal `AdmittedProviderCandidate` that
holds both the normal `ModelTier` projection and the witness. DREX ranks that
candidate. The chosen witness is passed unchanged into adapter construction.
Ranking and instantiation therefore cannot independently answer lane reality.

The durable route receipt stores the lane key and snapshot/evidence references,
not a serialized witness.

### Proving Witness

An unproven lane cannot satisfy the four-conjunct contract, so it cannot be
smuggled through `AdmittedProviderLane` for its first call.

`ProvingProviderLane` is a separate unforgeable, one-call authorization. It
requires:

- account exists;
- credential is usable;
- the requested model is present in verified inventory;
- a concrete `call_id` and short expiry;
- a user-authorized real task, an explicitly approved verification command, or
  an unmetered local background policy decision.

It is bound to one lane and one call, is not `Clone`, is never emitted as an
ordinary DREX candidate, and cannot be converted into
`AdmittedProviderLane`. Only a durable successful call observation can make a
later snapshot admit the lane.

For metered lanes, a proving call is allowed only when the user explicitly
selects the lane or when a real user task has no satisfying proven lane and the
router presents the proving/fallback decision under the task's existing spend
authorization. It is never background work.

For local unmetered lanes, Heiwa may mint a proving witness in the background
only under battery, thermal, load, concurrency, and cancellation limits.

## Adapter Construction Contract

The production construction path becomes:

```rust
admit(snapshot, request, now) -> Result<AdmittedProviderLane, ProviderAdmissionDenial>
resolve_admitted_adapter(&AdmittedProviderLane, now) -> Result<Arc<dyn ProviderAdapter>, ProviderAdapterError>
```

The proving path is separately named and accepts only
`ProvingProviderLane`.

Production rules:

- Claude Code, Codex, Gemini, Ollama, OpenRouter, and direct-API adapters are
  constructed from a witness-bound account/lane description.
- Public zero-argument CLI `new()` and `Default` paths are removed or made
  test-only/private where they could bypass admission.
- `OpenRouterAdapter::from_registry()` is removed from routing; its provider
  name match is not admission.
- An unknown or unavailable lane returns a typed denial. There is no implicit
  CLI fallback.
- Explicit provider override uses the same admission API and cannot bypass it.
- Test constructors may accept fixtures but remain unavailable to production
  modules through `cfg(test)` or a private test-support module.

The adapter remains a transport. It does not reload `accounts.json`, recompute
health, or discover a different account after the witness is minted.

## Data Flow

### Metadata path

```text
bounded local/provider metadata probe
          |
          v
typed + redacted ProviderObservation
          |
          v
heiwa_evidence::JsonlTransport / provider_observations.jsonl
          |
          v
ProviderTruthSnapshot::project(at, policy)
```

Safe metadata refresh may run without inference when the provider operation is
known not to consume inference quota. It must remain bounded and respect rate
limits. Metadata success never creates inference proof.

### Ordinary call path

```text
snapshot -> admit -> AdmittedProviderCandidate -> DREX rank
         -> chosen AdmittedProviderLane -> adapter -> real call
         -> existing model-call receipt -> ProviderObservation reference
         -> next snapshot
```

### First-real-call path

```text
available_unproven lane -> proving policy -> ProvingProviderLane
         -> one real call -> normal spend/usage receipt
         -> success observation OR classified failure observation
         -> next snapshot
```

No success observation is appended until the provider's terminal success and
the existing model-call receipt are durable. Observation append failure never
creates proof. The user-visible call follows the existing receipt failure
policy, but later routing treats the lane as unproven until durable evidence
exists.

## Verification Command and Spend Policy

The explicit command is two-stage:

```text
heiwa providers verify [filters]
```

prints a spend-free plan containing:

- every account, execution mode, model, and effort tier it proposes to call;
- whether the lane is metered or local;
- proof state and reason for verification;
- maximum calls;
- applicable provider/Heiwa budget class;
- receipt destination.

Execution requires an explicit second action such as `--execute`; interactive
mode confirms the already printed plan, and automation additionally requires
the normal noninteractive approval flag. The executed command cannot expand
the printed lane set. If live state changes, it aborts and requires a new plan.

Every executed verification call receives a `call_id`, uses the same metering,
usage, cancellation, and receipt path as a real task, and emits a provider
observation by reference. A synthetic “reply OK” can test transport but is not
representative proof; verification must use the lane's configured
representative request class and report its bounded traits.

Background policy:

- no inference call against a metered provider quota;
- bounded Ollama/local inference is permitted under machine-resource policy;
- non-inference metadata refresh is permitted only when it is known not to
  create inference spend and remains rate-bounded;
- the first metered call after proof expiry is a user-authorized proving call,
  not hidden maintenance.

## Failure Semantics

Admission denials are typed and normal:

- `AccountMissing`;
- `CredentialUnusable`;
- `ExecutionSubstrateUnavailable`;
- `InferenceProofMissing`;
- `InferenceProofExpired`;
- `ModelInventoryUnverified`;
- `ModelUnavailable`;
- `SnapshotAmbiguous`;
- `WitnessExpired`.

Provider-call failures are normalized before observation:

- `authentication_rejected`;
- `ineligible_tier`;
- `model_not_found`;
- `rate_limited`;
- `quota_exhausted`;
- `endpoint_unreachable`;
- `timeout`;
- `protocol_error`;
- `provider_internal`;
- `cancelled`.

Failure updates are dimension-specific:

- auth rejection invalidates credential usability and all proofs for that
  account/execution mode;
- ineligible tier invalidates the exact lane proof and records the model/tier
  constraint;
- model not found invalidates the exact inventory entry and lane proof;
- executable disappearance invalidates CLI/local credential usability;
- rate limit or quota exhaustion constrains routing until its bounded retry
  time but does not rewrite account identity or credential truth;
- timeout or provider internal failure records reliability evidence without
  automatically declaring the credential invalid.

The first failure after a silent provider change is visible to the user. That
is the accepted cost of refusing background metered inference.

## Migration

### Release PTC-1 — Callers First

PTC-1 closes the live bypass before the journal schema migration.

1. Add the admission module and typed denials in `heiwa_provider`.
2. Mint the private-field witness from the current registry/health projection,
   using an explicitly named `LegacyCurrentTruth` source.
3. Require an exact configured account and exact verified model inventory.
4. Remove unconditional CLI and OpenRouter fallback construction.
5. Make `resolve_adapter*` and every shell/operator/executor caller propagate or
   render typed denial.
6. Pass the same witness from candidate generation to adapter construction.
7. Ship the structural gate in the same commit as the witness.

The transitional source does not publish `inference_verified=true`; it only
closes construction bypass using the best current facts. The witness carries
its source/version so receipts and diagnostics do not misrepresent legacy
state as evidence-backed proof.

PTC-1 is allowed to make previously implicit CLI fallbacks unavailable. That
is the intended fail-closed behavior. It must return actionable provider/account
guidance instead of “no working adapters.”

### Release PTC-2 — Evidence Truth

1. Add typed provider observation records to `heiwa_evidence::records`.
2. Add a dedicated typed transport method that appends the
   `provider_observations` kind through the existing `JsonlTransport`.
3. Add replay/projector code and `ProviderTruthSnapshot` in the evidence or
   provider truth module without introducing another journal implementation.
4. Add classified/redacted observation writers at discovery, auth, inventory,
   and model-call completion/failure boundaries.
5. Replace `LegacyCurrentTruth` behind the admission module with the
   evidence-backed projector. No adapter caller changes.
6. Add proof decay, revocation, the proving witness, and the explicit
   verification plan/execute flow.
7. Remove `status` and `models` as mutable runtime truth from
   `ProviderAccount` after compatibility migration.
8. Stop `provider_connections.json` from influencing routing and stop writing
   it. Retain a bounded read-only migration diagnostic for one compatibility
   window, then delete it.

Legacy import must not invent proof:

- account identity and credential reference import as configuration;
- exact `InventoryTruth::Verified` models may import as
  `inventory_observed` with `legacy_import` provenance;
- inferred and user-configured models retain those truth classes;
- `Connected` never imports as an `inference_completed` observation;
- un-timestamped errors import only as historical diagnostics, not current
  revocation facts;
- every imported metered lane starts `available_unproven` until a real call
  succeeds.

### Release PTC-3 — Bounded Provider Execution

Provider truth and provider process scope meet at execution.

1. Bind every prepared provider call to `work_id`, `call_id`, lane witness,
   budget, and the existing `ExecutionScope`.
2. Ensure provider-native tool exposure is derived from explicit tool leases;
   no provider process receives broad tools merely because the host app has
   them.
3. Enforce filesystem root, working-directory, network, sandbox, and environment
   projection before provider process spawn.
4. Keep raw provider-owned auth in the provider/OS-owned channel, never the
   prompt, journal, or renderer.
5. Record the effective bounded scope by digest/reference in the call receipt.

This release completes the inside-Heiwa sandbox boundary. It does not reduce
the authority of provider apps when Devon runs them directly outside Heiwa.

## Structural Gate

`scripts/check_provider_truth_contract.sh` lands in the same commit as
`AdmittedProviderLane` and is wired into the relevant local/CI check groups.

It fails when production code contains any of these patterns:

- a public field or public constructor for `AdmittedProviderLane` or
  `ProvingProviderLane`;
- `Default`, `Deserialize`, or a public struct literal path for either
  witness;
- production `ClaudeCodeCliAdapter::new()`, `CodexCliAdapter::new()`,
  `GeminiCliAdapter::new()`, `OllamaCliAdapter::new()`, or
  `OpenRouterAdapter::from_registry()` construction outside the admission
  factory;
- a `resolve_adapter*` implementation that constructs a CLI adapter after an
  admission miss;
- a production candidate builder that creates provider `ModelTier` values
  without the admission factory;
- routing reads from `provider_connections.json` after the compatibility
  cutoff;
- a provider observation writer targeting any root outside the configured
  `heiwa_evidence` journal.

Rust field privacy supplies the type-level guarantee. The source gate protects
the architectural shape and catches future constructors or bypass factories
that would compile while defeating it.

The gate output includes the checked commit SHA. The witness is not complete
until the same commit passes the gate.

## Verification

### PTC-1 tests

- Empty registry denies Claude, Codex, Gemini, Ollama, and OpenRouter before
  process spawn.
- Missing executable denies a CLI/local lane even when stored status says
  connected.
- Unusable credential denies direct API and CLI lanes.
- Unknown, inferred, or user-configured requested model cannot satisfy the
  exact verified-inventory predicate.
- Multiple accounts select the exact account that owns the requested model and
  never silently move a subscription model onto a metered key.
- Explicit provider override cannot bypass admission.
- Candidate ranking and adapter construction receive the same witness identity.
- An expired in-memory witness is rejected at construction.
- Structural gate rejects a fixture that makes fields/constructors public or
  restores an infallible adapter path.
- Every existing resolver caller renders or propagates a typed denial.

### PTC-2 tests

- Provider observation append/replay round-trips through the existing journal
  envelope.
- Concurrent append remains one-line atomic and fsynced through the shared
  transport.
- Projection is deterministic for injected time and policy.
- Corrupt and unknown records are counted; ambiguous affected lanes fail
  closed.
- Credential-shaped payloads and auth paths are rejected before append.
- Raw failure text is never serialized; classified safe details survive.
- A successful metered call proves only its exact account/model/mode/effort
  lane.
- Proof is valid immediately before `valid_until` and unavailable at or after
  it.
- Auth, ineligible-tier, model-not-found, executable, rate-limit, timeout, and
  cancellation failures update only their specified dimensions.
- Legacy import preserves identity/inventory provenance and invents no
  inference success.
- Restart replay reconstructs the same snapshot.
- A metered background scheduler cannot mint a proving witness.
- Local background proving obeys resource policy and cancellation.
- Verification preview makes zero calls, execution cannot expand the preview,
  and every executed lane has a normal call receipt.
- A proving witness is one-call, non-cloneable, and cannot appear in normal
  candidate ranking.

### PTC-3 tests

- A prepared provider call without `work_id`, `call_id`, truth witness, spend
  authorization, or execution scope cannot spawn.
- Provider-native tools are absent without an explicit lease.
- Filesystem and working-directory roots match the admitted Work scope.
- Network-denied or sandbox-denied calls fail before provider process spawn.
- Receipt scope references are bounded and contain no secrets or raw prompt.

### Acceptance and spend safety

Automated tests use fixtures, loopback mocks, fake executables, injected clocks,
and disposable evidence roots. They never spend live provider quota.

Live metered verification is manual/explicit only and starts with the printed
verification plan. Checkout acceptance uses disposable `HEIWA_EVIDENCE_DIR`
and `HEIWA_STATE_DIR` roots and an alternate port. Installed `7474` remains
untouched until promotion is separately authorized.

## Acceptance Criteria

The Provider Truth Contract is complete when:

1. Ranking and adapter construction cannot disagree because both consume the
   same unforgeable witness.
2. No supported provider has an infallible production fallback after admission
   denial.
3. Ordinary admission is the accepted four-conjunct predicate.
4. `inference_verified` is a derived, expiring projection over durable real-call
   evidence.
5. A first real call uses a separate bounded proving authorization and normal
   receipt path.
6. Metered provider inference never runs in the background.
7. Local unmetered proving is resource-bounded and visible.
8. Explicit verification previews exact lanes and possible spend before any
   call.
9. Provider observations use the existing evidence journal and pass the shared
   sensitive-material gate.
10. Legacy registry/connection state cannot influence routing after the
    compatibility cutoff.
11. Inside-Heiwa provider calls are bound to Work, execution scope, leases,
    budget, and evidence without reducing the authority of directly used
    provider apps.
12. The structural gate and tests pass at the exact implementation SHA.

## Review Resolution Record

The accepted amendments are resolved as follows:

| Amendment                            | Resolution                                                                                                      |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| callers first, truth model second    | PTC-1 closes construction bypass; PTC-2 swaps the predicate behind the stable fallible interface                |
| four-conjunct admission              | specified exactly under Four-Conjunct Admission; OpenRouter's provider-name match is explicitly insufficient    |
| unforgeable witness                  | private fields/private constructor/no deserialization, persistence, or public literal; structural gate required |
| gate with witness                    | same commit and SHA-stamped acceptance requirement                                                              |
| existing journal or provider journal | existing `heiwa_evidence` journal, `provider_observations` record kind; no second root/transport/replay         |
| no background spend                  | no metered background inference; bounded local inference allowed; verification previews and receipts spend      |
| proof decay                          | seven-day metered and twenty-four-hour local defaults, immediate typed revocation                               |

## Definition of Done

This design is implemented only when the source contract, evidence contract,
runtime behavior, denial UX, sandbox boundary, tests, structural gate, and
SHA-bound receipts all agree. A green metadata probe, installed binary, merged
branch, or successful trivial prompt alone is not Provider Truth.
