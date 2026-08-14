# L0/L1 Task Ledger

Date opened: 2026-08-14
Contract: `docs/superpowers/specs/2026-08-14-heiwa-app-product-roadmap-design.md`
Acceptance gates: `scripts/check_l0_acceptance.sh`, `scripts/check_l1_acceptance.sh`
Plane: Execution (build) + Evidence (gates)

Status vocabulary: `todo` / `doing` / `done` / `blocked(<on>)`.
This ledger is repository truth for L0/L1 progress. Update it in the same
commit as the work it records.

## Verified baseline (2026-08-14)

- Roadmap factual audit: all claims verified accurate against tree.
- Desktop tests pre-migration: 42/42 pass (`store.test.ts` 16, `client.test.ts` 26),
  `tsc --noEmit` clean.
- Operator seam test checksums (frozen for L0 gate):
  - `store.test.ts` `7f68b72bc113940349648ef505bc49b52ecd11d21410b046b05fee06b8e6b2a0`
  - `client.test.ts` `a162fe8e094baf8f497504c9e99761ad069b8e5c614321efea4a34ab0ebb8470`

## L0 — UI foundation and N-user config root

| # | Requirement (roadmap) | Implementation tasks | Files/modules | Depends on | Verification | Status |
|---|---|---|---|---|---|---|
| L0.1 | ConfigRoot: single resolver owns per-user state root, platform-correct, first-run creation, sole authority | `HeiwaPaths` rewritten: `runtime_root`/`state_dir`/`evidence_dir`/`sessions_dir`/`config_path`, `HEIWA_STATE_DIR`+`HEIWA_EVIDENCE_DIR` support, injectable `resolve_from`, `ensure()`, `receipts_dir()` | `crates/heiwa_config/src/lib.rs` | — | `cargo test -p heiwa_config` (12 pass, TDD) | done |
| L0.2 | Single-seat audit: no hardcoded operator identity, machine name, or home path outside resolver | 8 resolvers collapsed onto ConfigRoot (shell/core/drex/evidence/install/provider/quota/loop); identity removals: `devon-canonical`→`local-user`, `ultimate_devon`→`state/life/plans`, cockpit chip→"Local operator", `~` API literals→resolved paths, repo-checkout fallback dropped, fixtures neutralized, herd bridge URL overridable | 31 files (see commit) | L0.1, audit report | 717 workspace + 314 shell + 42 desktop tests, clippy -D warnings, machete clean | done |
| L0.3 | SolidJS adoption (matches cockpit idiom, `solid-js ^1.9`) | `solid-js` + `vite-plugin-solid` added; entry is `main.tsx`; vitest keeps `node` default with per-file jsdom opt-in so seam tests stay untouched | `desktop/{package.json,vite.config.ts,tsconfig.json,index.html,src/main.tsx}` | — | `npm run build` (43 modules, 67 kB js) | done |
| L0.4 | Decompose ten surfaces into component modules, each owning render + local state, consuming operator store through typed interface | Ten modules under `src/surfaces/<id>/`; `SurfaceModule` contract + registry; shell chrome split into `Rail`/`Composer`; `Dynamic` mount so the shell holds no per-surface branch | `desktop/src/surfaces/*`, `src/shell/*`, `src/state/*`, `src/app.tsx` | L0.3 | `app.test.tsx`: 17 tests, mounts all ten | done |
| L0.5 | Operator seam preserved: `store.ts`, `client.ts`, `types.ts` retained; their tests pass unmodified | `state/operator.ts` adapts the seam into signals, coalescing frames through a scheduler; zero edits to seam files | `desktop/src/state/operator.ts`; seam files untouched | L0.3 | checksum guard + 42 seam tests green | done |
| L0.6 | Token design system: color/type/spacing/motion/elevation; light+dark as token sets; surfaces consume tokens only | Two-tier tokens (primitives + semantics), light and dark as complete sets, reduced-motion honored; `styles.css` (1030 lines) deleted; per-surface CSS consumes `var(--*)` only | `desktop/src/theme/{tokens,base}.css`, per-surface `*.css` | L0.3 | L0 gate: no raw hex outside `theme/` | done |
| L0.7 | D2 repository truth update: revise single-seat statements | `ops/context/HEIWA.md` (ConfigRoot + N-user hard rules), `AGENTS.md` (per-user root, dev-machine vs product separation), `docs/current-capability.md` (N-user + Solid claims; L2/L3/L4 gaps stated) | those three files | L0.2 landed | `check_agent_baseline.sh` | done |
| L0.8 | Acceptance: ten surfaces render via component layer, no behavior regression; seam tests pass unmodified; no home path outside resolver | `scripts/check_l0_acceptance.sh`: typecheck, build, vitest, seam checksums, ten-surface presence, render test, ConfigRoot sole-resolver grep (test modules skipped), identity grep, token discipline; stamps HEAD on pass | `scripts/check_l0_acceptance.sh` | L0.1–L0.7 | gate passes at HEAD | done |

## L1 — BYOK provider tier

| # | Requirement (roadmap) | Implementation tasks | Files/modules | Depends on | Verification | Status |
|---|---|---|---|---|---|---|
| L1.1 | Direct-API adapters alongside CLI adapters (Anthropic, OpenAI, Gemini families) | `anthropic_api.rs` (Messages API SSE), `openai_api.rs` (Chat Completions SSE), `gemini_api.rs` (streamGenerateContent), shared `sse.rs` framer; OpenRouter consolidated onto the OpenAI-compat parser; constructor-injected base URL and optional caller-supplied credential | `crates/heiwa_provider/src/providers/*` | — | 98 provider tests (pure stream state machines, TDD) | done |
| L1.2 | Model inventory discovered, never invented; discovery ≠ execution support | Per-adapter `discover_models()` against each provider's list endpoint; `InventoryTruth::Verified` only from a live probe. Replaced `anthropic_known_models()` — a hardcoded, already-stale list whose verification probe named a model id that does not exist | providers + `detect/mod.rs` | L1.1 | unit tests with mock list payloads; harness discovery test | done |
| L1.3 | Account-aware adapter resolution: ApiKey account → direct adapter; OauthCli → CLI adapter; several accounts per provider | `heiwa_provider::routing` owns selection (moved out of the shell binary so every surface and the harness resolve identically); health-gated so an expired key does not beat a working CLI seat | `crates/heiwa_provider/src/routing.rs`, `apps/heiwa_shell/src/main.rs` | L1.1 | routing unit tests + harness | done |
| L1.4 | Account health projection: user sees which accounts healthy and why one was skipped | `HealthState` (healthy/unauthenticated/rate-limited/unreachable/not-installed), per-account detail, `FleetHealth` partition and guidance text | `crates/heiwa_provider/src/health.rs` | L1.1 | 11 health tests | done |
| L1.5 | Failure semantics: unauthenticated/rate-limited/unreachable = routing constraint, not crash; zero healthy accounts → app opens and guides | Zero-candidate routing now returns `FleetHealth::guidance()` naming each skipped account and its reason, or how to connect a first provider | shell routing + `health.rs` | L1.3, L1.4 | harness: zero-account, expired-credential, and rate-limited cases | done |
| L1.6 | Fresh-install contract: no provider CLI on PATH + one API key completes a turn end-to-end, automated | `fresh_install.rs`: loopback mock speaking the Messages API, scrubbed PATH, temp state root, empty system bin dirs, one API-key account → Token stream + terminal Done. Surfaced that PATH-scrubbing alone was insufficient (discovery also probes `/opt/homebrew/bin`), so the system bin set is now injectable | `apps/heiwa_shell/tests/fresh_install.rs` | L1.1–L1.5 | `scripts/check_l1_acceptance.sh` passes | done |

## Single-seat audit findings (2026-08-14, drives L0.1/L0.2)

Eight independent `~/.heiwa` resolvers with incompatible env precedence; only
`heiwa_session` and `heiwa_embed` use `heiwa_config` today. `HeiwaPaths` ignores
`HEIWA_STATE_DIR` and misnames the runtime root `state_dir`.

Reroute onto ConfigRoot (category b): `apps/heiwa_shell/src/home.rs`,
`apps/heiwa_core/src/config.rs:52`, `crates/heiwa_drex/src/drex_gate.rs:40`,
`crates/heiwa_evidence/src/lib.rs:62-86`, `crates/heiwa_install/src/lib.rs:124`,
`crates/heiwa_provider/src/lib.rs:40`, `crates/heiwa_quota/src/lib.rs:107,428`,
`crates/heiwa_loop/src/fanout.rs:45`; `heiwa_shell` additionally uses
`heiwa_install::get_heiwa_dir()` at 11 sites as a second resolver.

Remove/fix (category c):
- `ultimate_devon` plans dir: `apps/heiwa_shell/src/cmd/life.rs:410,1003` + cockpit `Today.tsx:42`
- `devon-canonical` fixed identity: `crates/heiwa_provider/src/lib.rs:84-87`
- Hardcoded "Devon" operator chip: `apps/heiwa_app/clients/cockpit/src/App.tsx:392-396`
- `~` literals in API payloads: `apps/heiwa_shell/src/cmd/app.rs:3827,3864`
- Repo-checkout-under-$HOME fallback: `apps/heiwa_shell/src/cmd/schedule.rs:176,187`
- Build-machine repo_root baked into launchers: `crates/heiwa_install/src/lib.rs:442,504`
- TS bridge fetches without override: `apps/heiwa_app/desktop/src/runtime.ts:180,249,310`

Deferred with reason: `"macbook"` locality strings in DREX routers are internal
tier labels, not operator identity; renaming is routing-vocabulary churn outside
L0 scope. macOS absolute paths in capability detection are platform probes, kept.

Implementation order (dependency-safe): extend `HeiwaPaths` first (env precedence
+ naming), then collapse resolvers, then delete `heiwa_shell`'s second resolver,
then the identity removals.

## Architectural decisions (recorded as taken)

- **AD-1** ConfigRoot extends `crates/heiwa_config::HeiwaPaths` in place; root stays
  `~/.heiwa` (HEIWA.md install contract) with `HEIWA_HOME` override as the
  platform/packaging escape hatch. Platform-correctness = correct per-user home
  resolution per OS, not a new directory convention. Reversible.
- **AD-2** Keychain stays the secret store; mock/testing path uses
  `HEIWA_HOME`-scoped file fallback only where keychain is unavailable
  (existing `keychain.rs` behavior governs; no new secret surface).
- **AD-3** Direct-API adapters take `base_url` at construction (default = provider
  endpoint) so the fresh-install harness can point them at a local mock server.
  No env-var magic inside adapters.
- **AD-4** (revised in implementation) View ids unify on the roadmap's product
  names: `home|ai|windows|calendar|mail|finance|social|workers|browser|files`.
  The old `chat`/`agents` ids were internal only — no router, no persistence,
  no external consumer — so carrying them forward would have meant maintaining
  two vocabularies for the same ten surfaces. Reversible.
- **AD-5** Seam wrapper: `OperatorStore.snapshot()` behind a Solid signal
  published on `OperatorClient.onChange`, coalesced through an injectable
  scheduler (one animation frame by default). Snapshots deep-clone, so
  per-frame publishing would clone at token rate; message lists use `<Index>`
  rather than `<For>` because the clone breaks referential identity and `For`
  would rebuild every row per streamed token.
- **AD-6** The legacy `/ws/v1/events` socket (approvals + goal refresh) is kept
  in `state/legacy-events.ts` rather than dropped. It is the one raw socket
  left in the shell; removing it would silently stop inbox/approval refresh,
  which the L0 no-regression criterion forbids. It folds into L3.
- **AD-7** `heiwa_install::resolve_heiwa_dir` deleted rather than rerouted: it
  was a second pure implementation of ConfigRoot's precedence, reachable only
  from its own tests once `get_heiwa_dir` began delegating.
- **AD-8** `HeiwaPaths::try_resolve()` added alongside `resolve()`. The
  lenient form falls back to a cwd-relative `./.heiwa`; anything that reads
  secrets or appends to the evidence journal uses the strict form, because a
  root that follows the process working directory would let whatever can
  write that directory supply the JWT signing secret, and would split one
  machine's append-only history across directories silently.
- **AD-9** Adapters accept a caller-supplied credential
  (`with_api_key`) in addition to the keychain default. Not a test hook: a
  headless server or container has no OS keychain and holds its secrets
  elsewhere, and only the embedder knows where.
- **AD-10** Adapter selection moved from the shell binary into
  `heiwa_provider::routing`. Selection is provider knowledge, not CLI
  knowledge — and a binary-local function is unreachable from the
  integration test that has to prove the fresh-install path.
- **AD-11** CLI discovery's system bin directories
  (`DEFAULT_SYSTEM_BIN_DIRS`) are a parameter, not a constant baked into the
  probe. Scrubbing `PATH` does not model a machine without provider CLIs,
  because discovery also probes `/opt/homebrew/bin` and friends — the
  fresh-install harness passes an empty set. Every input to command
  resolution, including the runtime root, is now explicit.
- **AD-12** Mail draft signatures come from the per-install identity or are
  omitted. Drafts are outgoing text written on the user's behalf, so the
  binary must never supply a name; the prompt and template take the
  signature as a parameter, which makes "no name is baked in" testable
  without depending on whose machine runs the suite.

## Independent review (2026-08-14)

An adversarial review of the L0 commits found defects that this session then
fixed. Recording them because several were in code this session wrote, and
because two of them show the acceptance gate itself was the thing at fault:

- **Secrets read from the working directory.** `heiwa_core` adopted the
  cwd-relative fallback, so a container with no `HOME` would read its JWT
  signing secret from `./.heiwa/secrets/`. Fixed by AD-8.
- **The gate was blind to 31% of the code it claimed to scan.** Its awk
  exited at the *first* `#[cfg(test)]` and skipped to EOF, missing 3,674
  lines of `cmd/app.rs` — including every home-path call site in that file.
  Now skips test modules by their column-0 close brace. Verified by planting
  a `env::var("HOME")` read at `app.rs:4500` and watching the gate fail.
- **The identity check was a three-literal grep** that missed a bare
  "Devon" in the mail draft prompt and template — meaning every N-user
  install produced outgoing mail signed by the maintainer, while
  `docs/current-capability.md` claimed the gate enforced the opposite. Both
  the code and the check are fixed (AD-12).
- **A stale ref could run a command the user never typed.** The Windows
  surface picked the input-vs-select ref by mode while the markup picked by
  mode *and* catalog contents; Solid never clears refs on removal, so an
  emptied catalog left a stale selection that Run would submit. One
  union-typed ref now serves both controls.
- Also fixed: Home tiles dead-ending at Windows instead of their own
  surfaces, navigation outside the rail skipping the surface refresh,
  `subApps` rebuilding derived lists on every poll, the provider PATH probe
  reading ambient env instead of its injected home, absolute home paths
  serialized into fan-out plans, `ensure()` being dead code, three vacuous
  surface-render assertions that passed on a gutted component, the
  color-token check seeing only hex, the seam checksum pinning only the
  tests and not the implementation, and the acceptance stamp being writable
  from a dirty tree.
