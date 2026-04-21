# Rust + TypeScript-First Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Heiwa's production stack from a Python-first Hub to a Rust control plane with a TypeScript operator surface, while keeping shell as the deployment and operator glue layer.

**Architecture:** Yes, this migration is viable. The repo already has the right nucleus: the authoritative state layer is Rust-native in `apps/heiwa_hub/spacetimedb/src/lib.rs`, generated Rust and TypeScript bindings already exist under `packages/heiwa_bindings/`, and shell bootstrapping in `apps/heiwa_hub/start.sh` can preserve deploy continuity while Rust replaces the Python Hub incrementally. The migration must be staged rather than a flag-day rewrite: first make the bindings consumable as real Rust/TypeScript packages, then stand up a Rust orchestration binary beside the Python Hub, then port DREX/routing/state transitions into Rust, then introduce the TypeScript operator app, and only then demote Python to regression-only status.

**Tech Stack:** Rust (tokio, serde, reqwest, tracing), SpacetimeDB, TypeScript (SvelteKit + generated STDB SDK), npm, bash, Railway, Cloudflare Pages

**Reference Docs:**
- `docs/enterprise/HEIWA_AGENTIC_DIGITAL_ENTITY_DREX_2026-04-01.md`
- `docs/superpowers/plans/2026-04-01-drex-runtime-routing.md`
- `config/swarm/END_STATE_2026-03.md`
- `ops/research/1bit_llm_migration_report.md`

---

## Scope and sequencing

This is a whole-stack migration, but it should still land as four shippable tracks:

1. **Track A: Foundation** — make STDB bindings consumable from Rust and TypeScript, add a real Rust orchestration crate, keep Python untouched
2. **Track B: Control plane** — port DREX scoring, routing, and STDB persistence into Rust
3. **Track C: Runtime cutover** — move Railway/local boot from Python entrypoints to the Rust orchestrator while keeping shell glue
4. **Track D: Operator surface + retirement** — introduce the TypeScript app, freeze Python as regression-only, and update product docs/verification

Each track must leave the repo in a working state. Do not delete Python runtime code until Rust parity is proven under tests and boot smoke checks.

## Current reality that drives the plan

- `apps/heiwa_hub/spacetimedb/src/lib.rs` is already the authoritative state layer in Rust.
- `packages/heiwa_bindings/rust/` and `packages/heiwa_bindings/typescript/` already contain generated STDB clients, but they are not yet first-class consumable packages.
- `apps/heiwa_hub/main.py` is still the active orchestration/runtime entrypoint.
- `apps/heiwa_hub/start.sh` is the real deployment glue and must survive the migration.
- `apps/heiwa_web/` is still a static HTML surface, not a typed application.
- `config/swarm/END_STATE_2026-03.md` still documents the Python-era flow (`IntentNormalizer -> RiskScorer -> ComputeRouter -> HeiwaClaw`).

## File Structure

### New Files
| File | Responsibility |
|------|----------------|
| `apps/heiwa_orchestrator/Cargo.toml` | Rust orchestration binary crate manifest |
| `apps/heiwa_orchestrator/src/lib.rs` | Shared orchestration modules exposed to tests and `main.rs` |
| `apps/heiwa_orchestrator/src/main.rs` | Production Rust control-plane entrypoint |
| `apps/heiwa_orchestrator/src/config.rs` | Environment/config parsing now spread across `start.sh` + Python boot |
| `apps/heiwa_orchestrator/src/stdb/mod.rs` | STDB connection, subscription, reducer bridge layer |
| `apps/heiwa_orchestrator/src/runtime/mod.rs` | Runtime supervisor, task loops, worker orchestration |
| `apps/heiwa_orchestrator/src/drex/mod.rs` | DREX module root |
| `apps/heiwa_orchestrator/src/drex/vector.rs` | Rust `DrexVector` and supporting types |
| `apps/heiwa_orchestrator/src/drex/policy.rs` | Policy loading, weight matrices, authority gates |
| `apps/heiwa_orchestrator/src/drex/scorer.rs` | DREX scoring and route evaluation |
| `apps/heiwa_orchestrator/src/drex/router.rs` | Macro/meso/micro selection and route decision assembly |
| `apps/heiwa_orchestrator/tests/bootstrap_smoke.rs` | Crate-level boot/config smoke tests |
| `apps/heiwa_orchestrator/tests/drex_scoring.rs` | DREX axis/scoring parity tests |
| `apps/heiwa_orchestrator/tests/drex_persistence.rs` | DREX decision/failure persistence tests |
| `packages/heiwa_bindings/rust/Cargo.toml` | Make generated Rust bindings importable as a crate |
| `packages/heiwa_bindings/rust/src/lib.rs` | Stable crate entrypoint over generated Rust bindings |
| `packages/heiwa_bindings/typescript/package.json` | Make generated TS bindings importable from the web app |
| `packages/heiwa_bindings/typescript/tsconfig.json` | Local TS compiler settings for generated bindings |
| `apps/heiwa_web/package.json` | TypeScript web app manifest |
| `apps/heiwa_web/tsconfig.json` | TypeScript compiler configuration |
| `apps/heiwa_web/svelte.config.js` | SvelteKit configuration |
| `apps/heiwa_web/vite.config.ts` | Vite/SvelteKit build configuration |
| `apps/heiwa_web/src/routes/+page.svelte` | Initial operator landing page |
| `apps/heiwa_web/src/routes/routing/+page.svelte` | Route/DREX decision view |
| `apps/heiwa_web/src/lib/stdb/client.ts` | TS STDB client wrapper over generated bindings |
| `apps/heiwa_web/src/lib/types/drex.ts` | TS mirror types for DREX decision records |
| `apps/heiwa_hub/scripts/run_legacy_python_hub.sh` | Explicit fallback launcher during cutover |

### Modified Files
| File | Changes |
|------|---------|
| `Cargo.toml` | Add new Rust workspace members (`apps/heiwa_orchestrator`, `packages/heiwa_bindings/rust`) |
| `apps/heiwa_hub/spacetimedb/src/lib.rs` | Add DREX decision/failure tables and reducer extensions needed by the Rust router |
| `apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh` | Generate bindings into package/crate-friendly layouts |
| `apps/heiwa_hub/start.sh` | Switch from `python apps/heiwa_hub/main.py` boot to Rust orchestrator boot, keep shell env/bootstrap logic |
| `apps/heiwa_hub/Dockerfile` | Build and ship the Rust orchestrator binary alongside the STDB CLI and shell scripts |
| `apps/heiwa_hub/main.py` | Demote to compatibility shim / explicit legacy runtime |
| `railway.toml` | Point Railway deploy to the Rust orchestrator path once parity is proven |
| `justfile` | Add Rust/TS verification targets and separate legacy Python regression targets |
| `config/swarm/END_STATE_2026-03.md` | Replace Python-era architecture language with Rust + TypeScript + Shell end-state |
| `apps/heiwa_web/wrangler.toml` | Align Pages build output with the TypeScript app build |
| `apps/heiwa_hub/tests/test_cloud_hq_start_script.py` | Assert shell boot now targets the Rust binary |
| `apps/heiwa_hub/tests/test_hub_bootstrap_imports.py` | Convert from "Python imports boot" to "legacy Python shim remains available" |
| `apps/heiwa_hub/tests/test_compute_router.py` | Keep as Python regression only until retirement |
| `apps/heiwa_hub/tests/test_compute_router_stdb.py` | Keep as Python regression only until retirement |

---

## Track A: Foundation

### Task 1: Promote generated STDB bindings into first-class Rust and TypeScript packages

**Files:**
- Modify: `Cargo.toml`
- Modify: `apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh`
- Create: `packages/heiwa_bindings/rust/Cargo.toml`
- Create: `packages/heiwa_bindings/rust/src/lib.rs`
- Create: `packages/heiwa_bindings/typescript/package.json`
- Create: `packages/heiwa_bindings/typescript/tsconfig.json`
- Test: `packages/heiwa_bindings/rust/tests/generated_bindings_smoke.rs`

- [ ] **Step 1: Write a failing Rust smoke test for the bindings crate**

```rust
use heiwa_bindings::route_decision_type::RouteDecision;

#[test]
fn generated_route_decision_type_is_importable() {
    let _ = std::any::type_name::<RouteDecision>();
}
```

- [ ] **Step 2: Run the failing smoke test**

Run: `cargo test -p heiwa-bindings generated_route_decision_type_is_importable -- --exact`
Expected: FAIL because `heiwa-bindings` does not exist yet

- [ ] **Step 3: Turn `packages/heiwa_bindings/rust` into a real crate and add it to the workspace**

Create a minimal manifest and library entrypoint:

```toml
[package]
name = "heiwa-bindings"
version = "0.1.0"
edition = "2021"

[dependencies]
spacetimedb-sdk = "2.0.3"
serde = { version = "1", features = ["derive"] }
```

```rust
#[path = "../generated/mod.rs"]
pub mod generated;

pub use generated::*;
```

Also add `packages/heiwa_bindings/rust` to the root `Cargo.toml` workspace members.

- [ ] **Step 4: Reshape the binding generator output layout**

Update `apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh` to emit:

```bash
RUST_OUT="$ROOT/packages/heiwa_bindings/rust/generated"
TS_OUT="$ROOT/packages/heiwa_bindings/typescript/generated"
```

Do not keep generating flat files into package roots.

- [ ] **Step 5: Turn `packages/heiwa_bindings/typescript` into a real package**

Create:

```json
{
  "name": "@heiwa/bindings",
  "private": true,
  "type": "module",
  "exports": {
    ".": "./generated/index.ts"
  },
  "scripts": {
    "typecheck": "tsc --noEmit -p tsconfig.json"
  }
}
```

- [ ] **Step 6: Regenerate bindings and run both smoke checks**

Run:
- `bash apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh`
- `cargo test -p heiwa-bindings generated_route_decision_type_is_importable -- --exact`
- `npm --prefix packages/heiwa_bindings/typescript run typecheck`

Expected: all PASS

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml \
  apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh \
  packages/heiwa_bindings/rust \
  packages/heiwa_bindings/typescript
git commit -m "feat(bindings): package STDB bindings for Rust and TypeScript"
```

### Task 2: Scaffold the Rust orchestrator crate beside the Python Hub

**Files:**
- Modify: `Cargo.toml`
- Create: `apps/heiwa_orchestrator/Cargo.toml`
- Create: `apps/heiwa_orchestrator/src/lib.rs`
- Create: `apps/heiwa_orchestrator/src/main.rs`
- Create: `apps/heiwa_orchestrator/src/config.rs`
- Create: `apps/heiwa_orchestrator/src/stdb/mod.rs`
- Create: `apps/heiwa_orchestrator/src/runtime/mod.rs`
- Test: `apps/heiwa_orchestrator/tests/bootstrap_smoke.rs`

- [ ] **Step 1: Write the failing boot/config smoke test**

```rust
use heiwa_orchestrator::config::RuntimeConfig;

#[test]
fn runtime_config_reads_expected_defaults() {
    let cfg = RuntimeConfig::from_env();
    assert_eq!(cfg.port, 8080);
    assert_eq!(cfg.state_backend, "spacetimedb");
}
```

- [ ] **Step 2: Run the focused failing test**

Run: `cargo test -p heiwa-orchestrator runtime_config_reads_expected_defaults -- --exact`
Expected: FAIL because the crate does not exist yet

- [ ] **Step 3: Add the new crate to the workspace**

Add `apps/heiwa_orchestrator` to the root `Cargo.toml` workspace members.

- [ ] **Step 4: Create the minimal orchestrator library and binary**

`src/main.rs` should be thin:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = heiwa_orchestrator::config::RuntimeConfig::from_env();
    heiwa_orchestrator::runtime::run(cfg).await
}
```

- [ ] **Step 5: Implement the first-pass config contract from current shell/Python defaults**

Capture at least:
- `PORT`
- `HEIWA_STATE_BACKEND`
- `STDB_SERVER`
- `STDB_IDENTITY`
- `LOG_LEVEL`

- [ ] **Step 6: Re-run the focused smoke test and a full crate check**

Run:
- `cargo test -p heiwa-orchestrator runtime_config_reads_expected_defaults -- --exact`
- `cargo check -p heiwa-orchestrator`

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml apps/heiwa_orchestrator
git commit -m "feat(rust): scaffold orchestrator crate"
```

---

## Track B: Control Plane

### Task 3: Port DREX scoring and routing into Rust

**Files:**
- Create: `apps/heiwa_orchestrator/src/drex/mod.rs`
- Create: `apps/heiwa_orchestrator/src/drex/vector.rs`
- Create: `apps/heiwa_orchestrator/src/drex/policy.rs`
- Create: `apps/heiwa_orchestrator/src/drex/scorer.rs`
- Create: `apps/heiwa_orchestrator/src/drex/router.rs`
- Modify: `apps/heiwa_orchestrator/src/lib.rs`
- Modify: `apps/heiwa_orchestrator/src/runtime/mod.rs`
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh`
- Modify: `packages/heiwa_bindings/rust`
- Modify: `packages/heiwa_bindings/typescript`
- Test: `apps/heiwa_hub/tests/test_model_tiers_stdb.py`
- Test: `apps/heiwa_orchestrator/tests/drex_scoring.rs`

**Task 3 note:** The Rust DREX port must treat local inference metadata as first-class routing input. `ModelTier.max_context_tokens` already exists in the STDB schema today and should be preserved as the effective context ceiling for a given execution profile. Add these new `ModelTier` fields during this task:

- `vram_requirement_mb: u32`
- `quantization_type: String`
- `kv_cache_strategy: String`

This keeps the router compatible with standard GGUF quantization, TurboQuant-style KV compression, and future 1-bit providers without blocking migration on any one inference engine.

- [ ] **Step 1: Write the failing DREX parity tests**

Use the approved 7-axis model:

```rust
#[test]
fn code_edit_task_scores_micro_highest() {
    let vector = DrexVector {
        scope: 0.30,
        abstraction: 0.20,
        context_span: 0.55,
        execution_proximity: 0.95,
        reversibility: 0.45,
        coordination_load: 0.25,
        latency_pressure: 0.80,
    };
    let result = evaluate_drex(&vector, &policy());
    assert_eq!(result.active_tier, ResolutionTier::Micro);
}
```

- [ ] **Step 2: Run the failing parity tests**

Run: `cargo test -p heiwa-orchestrator --test drex_scoring`
Expected: FAIL because DREX types/scoring are not implemented

- [ ] **Step 3: Implement Rust DREX types and policy loading**

Create:
- `DrexVector`
- `DrexModifiers`
- `DrexAuthorityGate`
- `DrexScoreCard`
- `DrexDecision`
- `ResolutionTier`

The decision function must keep the paper's form:

```rust
score_tier = dot(weight_row, drex_vector) + bias
active_tier = argmax([macro_score, meso_score, micro_score])
```

- [ ] **Step 4: Port the heuristic table from the DREX plan into Rust tests first, then code**

Use `docs/superpowers/plans/2026-04-01-drex-runtime-routing.md` as the normative source. Do not invent new axes or scoring logic during implementation.

- [ ] **Step 5: Extend the `ModelTier` schema for inference-aware routing**

Write a failing schema/read test in `apps/heiwa_hub/tests/test_model_tiers_stdb.py`, then update `apps/heiwa_hub/spacetimedb/src/lib.rs` and the generated bindings so `ModelTier` carries:

```rust
pub vram_requirement_mb: u32,
pub quantization_type: String,
pub kv_cache_strategy: String,
```

Keep `cost_per_turn` as `f64`, and keep the existing `max_context_tokens: u32` field. Treat `max_context_tokens` as the effective context ceiling under the selected KV strategy rather than adding a duplicate field.

- [ ] **Step 6: Wire the scorer into a route-decision assembly path**

The orchestrator runtime should be able to turn task ingress into:
- DREX vector
- active tier
- scorecard
- approval gate result
- assigned execution runtime hint
- model-tier selection that can distinguish VRAM fit, quantization profile, and KV cache strategy

- [ ] **Step 7: Re-run the DREX scoring tests**

Run: `cargo test -p heiwa-orchestrator --test drex_scoring`
Expected: PASS

- [ ] **Step 8: Rebuild bindings and verify model-tier coverage**

Run:
- `CARGO_NET_OFFLINE=true bash apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh`
- `cargo test --offline -p heiwa-orchestrator --test drex_scoring`
- `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_model_tiers_stdb.py -q`

Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add apps/heiwa_orchestrator/src \
  apps/heiwa_orchestrator/tests/drex_scoring.rs \
  apps/heiwa_hub/spacetimedb/src/lib.rs \
  apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh \
  packages/heiwa_bindings/rust \
  packages/heiwa_bindings/typescript \
  apps/heiwa_hub/tests/test_model_tiers_stdb.py
git commit -m "feat(rust): port drex scoring and routing"
```

### Task 4: Extend the STDB module for DREX persistence and consume it from Rust

**Files:**
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh`
- Modify: `packages/heiwa_bindings/rust`
- Modify: `packages/heiwa_bindings/typescript`
- Modify: `apps/heiwa_orchestrator/src/stdb/mod.rs`
- Create: `apps/heiwa_orchestrator/tests/drex_persistence.rs`

- [ ] **Step 1: Write the failing persistence test**

```rust
#[tokio::test]
async fn record_drex_decision_writes_scores_and_axes() {
    let decision = sample_drex_decision();
    let client = TestStdbClient::new();
    client.record_drex_decision(&decision).await.unwrap();
    let stored = client.last_drex_decision().await.unwrap();
    assert_eq!(stored.active_tier, "micro");
    assert_eq!(stored.scope, decision.vector.scope);
    assert_eq!(stored.micro_score, decision.scorecard.micro_score);
}
```

- [ ] **Step 2: Run the failing persistence test**

Run: `cargo test -p heiwa-orchestrator --test drex_persistence`
Expected: FAIL because the STDB contract does not exist yet

- [ ] **Step 3: Add DREX tables and reducer/query hooks to the STDB module**

At minimum:
- `drex_decisions`
- `drex_failures`

Each decision row must store:
- typed scalar axes
- macro/meso/micro scores
- confidence
- approval requirement
- JSON snapshots for forward compatibility

- [ ] **Step 4: Link DREX decisions to the existing `route_decisions` record**

Do not create disconnected audit trails. Extend the routing reducer flow so a route decision can point at its DREX source record.

- [ ] **Step 5: Rebuild the STDB module and regenerate bindings**

Run:
- `cd apps/heiwa_hub/spacetimedb && spacetime build`
- `bash apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh`

Expected: PASS

- [ ] **Step 6: Implement the Rust STDB bridge methods and re-run persistence tests**

Run: `cargo test -p heiwa-orchestrator --test drex_persistence`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs \
  apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh \
  packages/heiwa_bindings/rust \
  packages/heiwa_bindings/typescript \
  apps/heiwa_orchestrator/src/stdb/mod.rs \
  apps/heiwa_orchestrator/tests/drex_persistence.rs
git commit -m "feat(stdb): persist drex decisions and failures"
```

---

## Track C: Runtime Cutover

### Task 5: Replace Python boot with the Rust orchestrator while preserving shell glue

**Files:**
- Modify: `apps/heiwa_hub/start.sh`
- Modify: `apps/heiwa_hub/Dockerfile`
- Modify: `railway.toml`
- Modify: `justfile`
- Create: `apps/heiwa_hub/scripts/run_legacy_python_hub.sh`
- Modify: `apps/heiwa_hub/tests/test_cloud_hq_start_script.py`

- [ ] **Step 1: Write the failing shell boot test**

Add an assertion that the start script launches the Rust orchestrator, not `python apps/heiwa_hub/main.py`.

- [ ] **Step 2: Run the focused shell test**

Run: `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_cloud_hq_start_script.py -q`
Expected: FAIL because the shell script still targets Python

- [ ] **Step 3: Keep shell responsible only for environment/bootstrap concerns**

`apps/heiwa_hub/start.sh` should continue to own:
- Tailscale setup
- optional Ollama boot
- STDB local boot/publish
- auth file materialization

It should stop owning orchestration logic.

- [ ] **Step 4: Launch the Rust binary from the shell script**

The final handoff should look like:

```bash
exec /app/target/release/heiwa-orchestrator
```

Keep `apps/heiwa_hub/scripts/run_legacy_python_hub.sh` as an explicit fallback, not the default.

- [ ] **Step 5: Update the Dockerfile to build and ship the Rust binary**

Add a Rust builder stage or install Rust in the current builder stage. Do not remove the Python environment yet; it is still needed for regression tests and legacy tools during transition.

- [ ] **Step 6: Add product verification commands for the new stack**

Add to `justfile`:
- `test-rust`
- `check-web-ts`
- `verify-rust-stack`
- `test-python-legacy`

- [ ] **Step 7: Re-run the shell test plus a Rust build**

Run:
- `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_cloud_hq_start_script.py -q`
- `cargo test -p heiwa-orchestrator`

Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add apps/heiwa_hub/start.sh \
  apps/heiwa_hub/Dockerfile \
  railway.toml \
  justfile \
  apps/heiwa_hub/scripts/run_legacy_python_hub.sh \
  apps/heiwa_hub/tests/test_cloud_hq_start_script.py
git commit -m "feat(runtime): boot rust orchestrator from shell"
```

---

## Track D: Operator Surface and Python Retirement

### Task 6: Introduce the TypeScript operator app on top of generated STDB bindings

**Files:**
- Create: `apps/heiwa_web/package.json`
- Create: `apps/heiwa_web/tsconfig.json`
- Create: `apps/heiwa_web/svelte.config.js`
- Create: `apps/heiwa_web/vite.config.ts`
- Create: `apps/heiwa_web/src/routes/+page.svelte`
- Create: `apps/heiwa_web/src/routes/routing/+page.svelte`
- Create: `apps/heiwa_web/src/lib/stdb/client.ts`
- Create: `apps/heiwa_web/src/lib/types/drex.ts`
- Modify: `apps/heiwa_web/wrangler.toml`

- [ ] **Step 1: Write a failing TypeScript typecheck target**

Add a minimal page that imports the bindings package:

```ts
import type { RouteDecision } from "@heiwa/bindings";
```

- [ ] **Step 2: Run the failing web build/typecheck**

Run: `npm --prefix apps/heiwa_web run check`
Expected: FAIL because the app does not exist yet

- [ ] **Step 3: Scaffold the SvelteKit app with local package imports**

The app should start with:
- status page
- route decisions page
- DREX decision details view

Do not rebuild the entire static site first. Start with a typed operator shell that reads real STDB data.

- [ ] **Step 4: Mirror the DREX decision contract in TypeScript**

Create a thin app-local type wrapper:

```ts
export type DrexDecisionView = {
  activeTier: "macro" | "meso" | "micro";
  scope: number;
  abstraction: number;
  contextSpan: number;
  executionProximity: number;
  reversibility: number;
  coordinationLoad: number;
  latencyPressure: number;
  macroScore: number;
  mesoScore: number;
  microScore: number;
  confidence: number;
  requiresApproval: boolean;
};
```

- [ ] **Step 5: Update Cloudflare Pages config for the new build output**

`apps/heiwa_web/wrangler.toml` must stop assuming `clients/web` is a hand-authored static directory once the typed app is in place.

- [ ] **Step 6: Run typecheck and production build**

Run:
- `npm --prefix apps/heiwa_web install`
- `npm --prefix apps/heiwa_web run check`
- `npm --prefix apps/heiwa_web run build`

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_web
git commit -m "feat(web): add typed operator app"
```

### Task 7: Freeze Python to regression-only status and update the repo's declared architecture

**Files:**
- Modify: `apps/heiwa_hub/main.py`
- Modify: `config/swarm/END_STATE_2026-03.md`
- Modify: `justfile`
- Modify: `apps/heiwa_hub/tests/test_hub_bootstrap_imports.py`
- Modify: `apps/heiwa_hub/tests/test_compute_router.py`
- Modify: `apps/heiwa_hub/tests/test_compute_router_stdb.py`

- [ ] **Step 1: Write the failing regression-status test**

Add an assertion that Python runtime files are marked as legacy/compatibility surfaces rather than primary architecture.

- [ ] **Step 2: Run the focused docs/bootstrap tests**

Run:
- `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_hub_bootstrap_imports.py -q`
- `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_compute_router.py apps/heiwa_hub/tests/test_compute_router_stdb.py -q`

Expected: FAIL because the current architecture docs and boot assumptions still treat Python as primary

- [ ] **Step 3: Convert `apps/heiwa_hub/main.py` into an explicit legacy shim**

It should:
- keep working if invoked directly
- log that Rust is the primary runtime
- delegate only to legacy paths

It should not continue to grow new features.

- [ ] **Step 4: Rewrite `config/swarm/END_STATE_2026-03.md` around the actual target stack**

The control flow should become:

```text
Ingress -> Rust Orchestrator -> DREX Router -> STDB reducers/subscriptions -> TS operator surface
```

Shell remains the bootstrap/operator layer, not the cognitive layer.

- [ ] **Step 5: Split verification into primary vs legacy lanes**

`justfile` should make the distinction explicit:
- primary: Rust + STDB + TypeScript
- legacy: Python regression only

- [ ] **Step 6: Re-run the focused regression/docs tests**

Run:
- `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_hub_bootstrap_imports.py -q`
- `.venv/bin/python -m pytest apps/heiwa_hub/tests/test_compute_router.py apps/heiwa_hub/tests/test_compute_router_stdb.py -q`

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_hub/main.py \
  config/swarm/END_STATE_2026-03.md \
  justfile \
  apps/heiwa_hub/tests/test_hub_bootstrap_imports.py \
  apps/heiwa_hub/tests/test_compute_router.py \
  apps/heiwa_hub/tests/test_compute_router_stdb.py
git commit -m "docs(runtime): demote python to legacy regression status"
```

---

## Success criteria

The migration is successful when all of the following are true:

- Railway boots the Rust orchestrator by default
- STDB remains authoritative and is mutated through Rust-owned routing/persistence logic
- DREX scoring and route decisions are computed in Rust and persisted to STDB
- The TypeScript operator surface consumes generated TS bindings rather than ad hoc JSON contracts
- Shell remains the deployment/operator glue, not the cognitive runtime
- Python still runs only as a legacy compatibility/regression lane, not the primary architecture

## Explicit non-goals for this plan

- Do not port every Python utility before the Rust orchestrator exists
- Do not delete `apps/heiwa_hub/main.py` during the first cutover
- Do not rebuild the entire web surface before DREX and routing land in Rust
- Do not introduce a second message bus; keep STDB authoritative
- Do not claim production parity until Railway boot, Rust tests, TS build, and legacy regression checks all pass
