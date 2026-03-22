# Heiwa Self-Operation Design

> **Status:** Approved design — ready for implementation planning
> **Author:** Codex (Class 3 Executor) with operator direction from Devon
> **Date:** 2026-03-22

## Goal

Make Heiwa a Railway-resident executive that can continuously research, improve, operate, and mutate its own code and production control plane while using local and sovereign resources as managed limbs. Devon steers by Discord DMs; Heiwa executes end to end.

## Architecture Overview

Heiwa should run as a `Railway-first executive with sovereign limbs`.

- **Railway is the executive plane.** It owns the always-on loop, Discord intake, planning, repo mutation, PR and deploy workflows, provider selection, environment promotion, and runtime reporting.
- **Local and sovereign resources are limbs, not a second operator console.** Ollama, boost nodes, and other local runtimes remain available for private, cheap, parallel, or hardware-specific work, but they are dispatched and reconciled by the Railway-resident executive.
- **Autonomy is broad but policy-bound.** Heiwa may create branches, worktrees, PRs, deployments, and production config mutations, but every mutation must flow through explicit verification, persistent state, and notification rules.

This design turns the current runtime contract work into a larger self-operation system instead of leaving branch/worktree/deploy behavior as human shell habits.

---

## 1. Operating Model

Heiwa runs four standing loops:

1. **Observe**
   - Watch Discord DMs and server events
   - Inspect repo state, worktrees, branches, PRs, CI, deployments, provider health, and environment drift
   - Track incidents, failed verifications, research opportunities, and app/runtime degradation
2. **Decide**
   - Classify work as `research`, `build`, `ops`, `review`, or `report-only`
   - Choose whether work is inline, isolated, staged, or production-impacting
   - Select the execution substrate: Railway frontier providers, local/Ollama, or mixed-provider pipelines
3. **Act**
   - Create branches and worktrees
   - Run research and implementation loops
   - Open/update/merge PRs
   - Mutate Railway, GitHub, and Cloudflare control-plane state
   - Deploy and promote applications across environments
4. **Reconcile**
   - Verify outcomes
   - Post reports and docs
   - Update internal memory and policy state
   - Clean merged or abandoned lanes, branches, and worktrees

This is one unified system for app delivery and operations. Heiwa does not maintain a separate workflow for “product changes” versus “ops changes.”

## 2. Lane And Worktree Topology

Heiwa should treat repo state as a managed fleet of lanes instead of ad hoc branches.

### Branch and worktree classes

- `main`
  - Production truth
  - Railway deploy source
  - Only verified, merged work lands here
- `.worktrees/research-*`
  - Karpathy/autoresearch loops
  - Used for experiments, benchmarks, provider comparisons, speculative ideas, and architecture probes
- `.worktrees/build-*`
  - Concrete implementation lanes promoted from research, operator instruction, incidents, or backlog work
- `.worktrees/ops-*`
  - Runtime, config, deployment, provider, or infra repair lanes
- `.worktrees/review-*`
  - Optional isolated audit or adversarial validation lanes
- archive lanes or archival metadata
  - Preserve noteworthy experiment history without keeping every worktree on disk indefinitely

### Lane lifecycle rules

- Heiwa defaults to isolated lanes for any risky, long-running, experimental, or deploy-sensitive task.
- Inline work on `main` should be limited to trivial low-risk edits if policy explicitly allows it.
- Every lane must have a recorded purpose, owner, environment target, and cleanup status.
- Every merged lane must trigger branch and worktree reconciliation.
- Failed, stale, or abandoned lanes must be archived or pruned by policy, not left to accumulate.
- App changes and ops changes use the same lifecycle and the same lane model.

### Lane registry

Heiwa needs one authoritative lane registry in SpacetimeDB. Git-visible manifests or repo snapshots may exist for operator visibility, recovery aids, or documentation, but they are derived from STDB and must never become a competing source of truth.

Each lane record should include:

| Field | Purpose |
| --- | --- |
| `lane_id` | Stable identity for the lane |
| `lane_type` | `research`, `build`, `ops`, `review` |
| `branch_name` | Git branch |
| `worktree_path` | Filesystem location if checked out |
| `origin_trigger` | DM, incident, autoresearch finding, CI failure, deploy drift, etc. |
| `owning_agent` | Which agent/workflow is responsible |
| `target_surface` | App, package, doc set, infra surface, provider layer |
| `target_environment` | `dev`, `staging`, `prod` |
| `provider_plan` | Which providers are expected for planning, build, review, and ops |
| `status` | `active`, `blocked`, `verifying`, `merged`, `cleanup_pending`, `archived` |
| `pr_url` / `deploy_ref` | Remote reconciliation pointers |
| `rollback_context` | Recovery notes for risky mutations |

The STDB-backed registry is the source of truth for “what Heiwa is doing” and “what can be safely cleaned up.”

### Lane residency and persistence

Railway is the executive and orchestration plane, not the durable git worktree host. Durable worktrees live on designated sovereign executors that Heiwa controls, such as Devon's local Heiwa workspace or other trusted boost nodes with git write capability. Railway creates, rehydrates, and reconciles those worktrees through the lane registry and remote executor tools; it does not treat the Railway container filesystem as durable worktree state.

## 3. Control-Plane Ownership

Heiwa should own the following control planes directly:

- **GitHub**
  - branch creation, commits, pushes, PR creation, review follow-up, merges, branch deletion
- **Railway**
  - env vars, service config, deploys, logs, environment promotion, service health checks
- **Cloudflare**
  - resource/config mutation for Workers or Pages when relevant
- **Discord**
  - DM intake, critical acknowledgements, incident delivery, server-side reporting and summaries
- **Local and sovereign limbs**
  - local file/system work, Ollama execution, private audits, boost-node tasks

### Mutation ladder

Heiwa should not mutate production by improvisation. It should move through an explicit ladder:

1. `research`
   - isolated lane, dev target, benchmark or idea generation
2. `build`
   - implementation lane with focused scope and verification
3. `staging`
   - deployment validation against a non-production environment
4. `promotion`
   - production deploy and/or config mutation when internal policy gates pass
5. `reconcile`
   - evidence capture, reporting, cleanup, and rollback bookkeeping

Staging validation is mandatory before any production-impacting mutation. If a change affects production code, deploy artifacts, or control-plane configuration, Heiwa must first validate the equivalent mutation in staging. If a staging analogue does not exist yet, creating it becomes part of the lane before production mutation proceeds.

## 4. Provider And Runtime Strategy

The existing runtime contract and router logic become one part of a broader execution planner.

### Executive policy

- Railway-hosted Heiwa uses frontier remote providers for:
  - planning
  - long-context reasoning
  - code review
  - cloud-control-plane mutation
  - strategic/adversarial analysis
- Local and sovereign limbs use Ollama or other local runtimes for:
  - private or sovereign work
  - cheap parallel research
  - embeddings and local indexing
  - fallback continuity
  - hardware-specific execution

### Routing rules

- Railway runtime lanes must never select local-only tiers that do not exist on Railway.
- Sovereign or boost-only lanes must never be forced onto remote providers when local execution is required by policy.
- Provider choice must be:
  - environment-aware
  - privacy-aware
  - cost-aware
  - rate-group-aware
  - recorded in lane state
- Heiwa should be able to split one workflow across providers intentionally:
  - one provider plans
  - another implements
  - another reviews
  - another performs an adversarial pass

This preserves the current router contract while extending it into full self-operation behavior.

## 5. Required Subsystems

Heiwa needs explicit internal subsystems instead of shell scripts and ad hoc operator habits.

### 5.1 Executive loop

The always-on Railway-resident control loop that watches inputs, plans work, dispatches lanes, and reconciles outcomes.

### 5.2 Lane registry

Persistent state for branches, worktrees, PRs, deployments, environment targets, provider plans, and cleanup status.

### 5.3 Repo and worktree manager

Capabilities:

- create policy-compliant branch and worktree names
- open new lanes from incidents, DM directives, or autoresearch findings
- detect stale or already-merged lanes
- delete worktrees and branches after merge or abandonment
- keep git state and registry state synchronized

### 5.4 Environment promotion manager

Maps lanes to `dev`, `staging`, and `prod`, records what is deployed where, and enforces the rule that every production-impacting lane must pass through staging before production mutation.

### 5.5 Provider orchestration layer

Extends the current routing system into a full execution ledger:

- selected planner provider
- selected implementation provider
- selected review provider
- selected local/remote runtime
- fallback history
- rate-limit or failure reasons

### 5.6 Self-improvement engine

The baked-in autoresearch loop that:

- identifies improvement opportunities from incidents, drift, inefficiencies, and operator direction
- opens `research-*` lanes autonomously
- benchmarks multiple providers or strategies
- promotes promising results into `build-*` or `ops-*` lanes
- records findings back into memory and docs

### 5.7 Ops and app manager

A unified surface for operating deployed apps and infra. This should inspect and mutate application state, deployments, env vars, provider accounts, and service health as part of the same lane lifecycle used for code changes.

### 5.8 Reporting and notification layer

Two distinct outputs:

- **Devon DMs**
  - critical errors
  - important acknowledgements
  - incidents
  - production-impacting changes
  - major completions or state changes that materially affect operator awareness
- **Discord server**
  - routine summaries
  - research reports
  - docs links
  - merge/deploy digests
  - ambient operational visibility

### 5.9 Policy and memory layer

Machine-readable policy for:

- when to open a worktree
- when prod mutation is allowed
- cleanup thresholds
- cost ceilings
- provider preferences
- DM versus server notification rules
- rollback expectations

This layer also stores learned outcomes so Heiwa can evolve its own operating heuristics.

## 6. Organization And Effectiveness Criteria

Heiwa should be considered “organized and effective” only if the following invariants hold.

### Repo invariants

- Every active lane is discoverable from the authoritative STDB lane registry.
- Every active branch/worktree has a purpose, owner, environment target, and status.
- Merged work does not leave orphaned worktrees behind.
- Stale research work is archived or removed by policy.
- Runtime and operator docs stay wired to the authoritative system state.

### Runtime invariants

- Railway boots with the provider/tool/runtime contract visible at startup.
- Heiwa can inspect provider auth, deploy health, env topology, and repo mutation state on demand.
- `dev`, `staging`, and `prod` are explicit tracked targets, not implicit operator memory.
- Production config changes are preceded by staging validation and have evidence, result tracking, and rollback context.

### Execution-quality invariants

- Provider choice reflects cost, privacy, runtime availability, and task class.
- Ollama and other sovereign resources are first-class execution limbs.
- Research, coding, review, and ops can be split across providers intentionally.
- Heiwa can explain which experiments improved outcomes and encode that into policy.

### Operator-experience invariants

- Critical errors, acknowledgements, and major changes arrive in Devon’s DMs.
- Routine summaries and documentation updates land in the server.
- Heiwa can answer:
  - what it is doing
  - why it opened a lane
  - why it chose a provider
  - what changed in code or config
  - why a lane was cleaned up or promoted

## 7. Current-System Alignment

This design builds on current repo reality instead of replacing it wholesale.

- The Railway runtime contract already exists in:
  - `apps/heiwa_hub/Dockerfile`
  - `apps/heiwa_hub/start.sh`
  - `packages/heiwa_cognition/heiwa_cognition/router.py`
  - `config/seeds/model_tiers.json`
  - `docs/railway-self-operation.md`
- The operator runbook already points at the Railway contract via `docs/operator-runbook.md`.
- The repo already uses a project-local `.worktrees/` directory on a sovereign executor and currently has an active autonomous lane in `.worktrees/heiwa-autoresearch`.

The implementation should consolidate around these surfaces instead of introducing a parallel system.

## 8. Non-Goals

This design does not assume:

- blind direct-to-prod mutation without verification
- a second human-driven console for routine Heiwa execution
- removal of sovereign execution in favor of cloud-only operation
- scattered one-off scripts as the long-term interface for self-operation

## 9. Success Criteria

This design is successful when Heiwa can:

1. Detect a research or operational opportunity and open the correct lane automatically.
2. Choose Railway or sovereign execution intentionally based on policy and runtime fit.
3. Run staged validation through dev and staging before every production-impacting mutation.
4. Create PRs, merge validated work, deploy, and clean up related branches/worktrees without operator babysitting.
5. Keep Devon informed via DMs for critical state changes and the Discord server for routine reporting.
6. Explain its own state and recent decisions from persistent system records rather than ad hoc memory.

## 10. Recommended Implementation Direction

Implement this as a repo-native self-operation layer with explicit state and interfaces, not as distributed shell glue.

The implementation plan should decompose into:

- persistent lane registry and manifests
- repo/worktree lifecycle manager
- environment and promotion registry
- provider execution ledger
- notification policy layer
- integration with current Railway runtime contract and routing logic

That keeps Heiwa legible while expanding autonomy.
