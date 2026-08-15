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
| L1.3 | Account-aware adapter resolution: ApiKey account → direct adapter; OauthCli → CLI adapter; several accounts per provider | `heiwa_provider::routing` owns selection (moved out of the shell binary so every surface and the harness resolve identically); health-gated so an expired key does not beat a working CLI seat; selection is model-aware, so a vendor with several keys routes to the seat that lists the model | `crates/heiwa_provider/src/routing.rs`, `apps/heiwa_shell/src/main.rs` | L1.1 | routing unit tests + harness + L1 gate check 5 (shell holds no alias table) | done |
| L1.4 | Account health projection: user sees which accounts healthy and why one was skipped | `HealthState` (healthy/unauthenticated/rate-limited/unreachable/not-installed), per-account detail, `FleetHealth` partition and guidance text. Every state has a real producer: rate-limit comes from a typed 429, not-installed from CLI accounts registered by discovery and from a local runtime whose binary is absent | `crates/heiwa_provider/src/health.rs`, `detect/mod.rs` | L1.1 | 12 health tests + 3 CLI-discovery tests | done |
| L1.5 | Failure semantics: unauthenticated/rate-limited/unreachable = routing constraint, not crash; zero healthy accounts → app opens and guides | Zero-candidate routing now returns `FleetHealth::guidance()` naming each skipped account and its reason, or how to connect a first provider | shell routing + `health.rs` | L1.3, L1.4 | harness: zero-account, expired-credential, and rate-limited cases | done |
| L1.6 | Fresh-install contract: no provider CLI on PATH + one API key completes a turn end-to-end, automated | `fresh_install.rs` spawns the built `heiwa` binary with an emptied `PATH`, temp state root, one API-key account, key in the environment (no keychain), and both the provider and the local runtime pointed at loopback; asserts the model's text on stdout and the `x-api-key` header on the request. The first version constructed the adapter directly and so proved only that the adapter worked — see the review findings below | `apps/heiwa_shell/tests/fresh_install.rs`, `apps/heiwa_shell/src/main.rs` (`heiwa ask`) | L1.1–L1.5 | `scripts/check_l1_acceptance.sh` passes | done |
| L1.7 | Credential resolution on a machine with no OS keychain | `resolve_secret` falls back to the provider's conventional variable (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`/`GOOGLE_API_KEY`, `OPENROUTER_API_KEY`); stored secrets still win. Base URLs accept `ANTHROPIC_BASE_URL`/`OPENAI_BASE_URL`/`GEMINI_BASE_URL` for gateways | `crates/heiwa_provider/src/registry.rs`, `routing.rs` | L1.1 | 4 secret-resolution tests; exercised by the harness | done |
| L1.8 | Non-interactive turn entry point | `heiwa ask <prompt>` runs one turn through the real operator pipeline and prints the reply; the fresh-install contract is asserted against it rather than against a reconstruction of it | `apps/heiwa_shell/src/main.rs` | L1.5 | fresh-install harness | done |

## L1 review findings and repairs (2026-08-14)

An independent review of the L1 commit found that L1 was **not** complete when
it was first marked done. Recorded here because the ledger claimed otherwise.

| ID | Finding | Repair | Regression guard |
|---|---|---|---|
| C1 | The shell kept its own `canonical_provider_id` that never mapped vendor names onto route names, so `get_live_model_tiers` dropped every direct-API model and a user with a valid key saw "No models with working adapters". The harness could not catch it because it bypassed the turn path | Deleted the shell-local table; the shell now uses `heiwa_provider::routing` | `vendor_names_resolve_to_the_same_adapters_as_route_names`, `direct_api_accounts_survive_the_live_model_tier_filter`, L1 gate check 5 |
| H3 | A mid-stream network error returned via `?` without emitting a terminal event, so the turn ended silently | Shared `stream::pump`: every return *after the response is in hand* emits a terminal event. The pre-pump `?`s (client build, `send()`) still return bare; `model_calls.rs` compensates, so no user-visible gap, but the module doc overstated it | provider stream tests |
| H4 | No idle timeout: a half-open socket hung the turn indefinitely | 180s idle timeout around each chunk read | `stream.rs` |
| H5 | Credential validity was classified by substring-matching the raw response body; an Anthropic 500 whose request id contains "401" marked a working key invalid and cleared its inventory | Discovery returns a typed `DiscoveryError` carrying the HTTP status; classification is status-driven | `a_server_error_whose_body_mentions_401_does_not_invalidate_the_key` |
| H6 | `RateLimited` and `NotInstalled` had no producers — vocabulary without behavior | 429 now produces a rate-limit status; discovery registers installed provider CLIs as optional accounts, so a CLI that disappears projects `NotInstalled` | health + detect tests |
| H2 | Adapter selection ignored the model, so a vendor with two keys could route a model to a seat that does not serve it | `routable_api_key_account_for` prefers the account listing the model | `the_account_that_serves_the_model_wins_over_registry_order` |
| M7 | SSE decode was quadratic — 20k events in one chunk took 10.0s | Single scan with `find_separator` + one `drain` per push; same input now 0.02s | `decodes_a_large_burst_in_linear_time` |
| M8/M9 | Empty tool input serialized as nothing; concurrent tool blocks collided | `"{}"` for empty input; tool state keyed by block index | anthropic adapter tests |
| M10 | The harness mutated process-global `PATH`/`HOME` with no panic-safe restore, racing other tests | The harness spawns a child process with a built environment; nothing global is mutated | — |
| M11 | The harness never reached the real turn path, and four places overstated what it proved | Harness drives the shipped binary; claims corrected here, in `fresh_install.rs`, `docs/current-capability.md`, and `check_l1_acceptance.sh` | — |
| M12 | L1 gate check 5 asserted two files exist — it would pass on empty files | Replaced with a grep asserting the shell defines no provider alias table and does use `heiwa_provider::routing`; verified by planting the defect | L1 gate check 5 |
| L14 | The lenient path resolver backed the identity file, so an auth token could be written under the process working directory | `save_identity` uses the strict resolver and refuses when no per-user root exists. **The first fix reintroduced the defect**: `try_resolve_from` counted `HEIWA_STATE_DIR`/`HEIWA_EVIDENCE_DIR` as roots while `runtime_root` still fell back to `./.heiwa`. The runtime root is now derived from those keys when no home exists | `a_state_dir_override_with_no_home_never_resolves_under_the_working_directory` |

Found by the repaired harness, not by review: a local runtime discovered over
HTTP but executed as a subprocess could report Healthy while its binary was
absent, and the turn died on a raw OS error instead of routing elsewhere —
a direct violation of L1.5.

## Second review (2026-08-14) — the repairs were themselves incomplete

A second independent review, with wire captures against the built binary,
found that the projection had been fixed while routing still ignored it, and
that the harness could not fail the checks it claimed to make. Repaired:

| ID | Finding | Repair | Regression guard |
|---|---|---|---|
| H-1 | The L14 repair reintroduced the cwd-relative root it fixed (above) | Runtime root derives from `HEIWA_STATE_DIR`/`HEIWA_EVIDENCE_DIR` when no home exists | `a_state_dir_override_with_no_home_never_resolves_under_the_working_directory` |
| H-2 | Health had **zero** effect on routing: `get_live_model_tiers` filtered on stored status, so the `NotInstalled` work changed no route. Reproduced: `route preview` chose a CLI seat whose binary was absent, and the turn then failed | `AccountRegistry::routable_models` filters on health; tier selection uses it | `a_cli_seat_whose_binary_is_gone_offers_no_route` |
| H-3 | The harness's no-CLI assertion resolved with empty search lists — true for any input — while the child still found `/usr/local/bin/claude` and registered it | `HEIWA_BIN_DIRS` makes the search path configurable; the harness empties it and asserts on the registry the child actually wrote. Verified by removing the isolation and watching the assertion fail | `fresh_install_with_one_api_key_and_no_cli_completes_a_turn` |
| H-4 | The harness used account id `anthropic-api-1` — the id `add-key` mints — so it would read the developer's real key from the keychain and fail the gate for an unrelated reason | Harness uses `anthropic-api-harness` | — |
| H-5 | Verification hardcoded the vendor base URL while turns honored the override, so a gateway user's credential was sent to the vendor, rejected, and the account permanently marked invalid | `routing::api_base_url` is shared by both paths | — |
| H-6 | Every turn sent the prompt twice — the transcript already held it and it was appended again. Billed twice, on the message most likely to carry pasted context | Append only when the transcript does not already end with it | `the_current_prompt_is_not_repeated_when_the_transcript_already_holds_it`; harness counts occurrences on the wire |
| H-7 | A `Connected` account with an empty inventory reported Healthy and dead-ended the turn with a message naming neither the account nor a way out | Empty inventory is `Unreachable` with a remedy in the detail | `a_connected_account_with_no_models_is_reported_rather_than_silently_dead` |
| M-2 | Model-aware selection fell back to any healthy API-key account, so a subscription seat's model could execute on the metered key while quota debited the seat | Fall back only when no other account claims the model | `a_subscription_seats_model_does_not_get_billed_to_a_metered_key` |
| — | CLI discovery assigned rate group `google_sub`, which `heiwa_drex` does not know — it fell through to a 200k ceiling instead of Google's 2M | Use `google` | — |

Open, accepted for now (recorded rather than fixed):

- `String::from_utf8_lossy` per chunk in `stream.rs` corrupts a multi-byte
  character split across a read. Pre-existing; the decoder carries a `String`,
  so fixing it means carrying bytes instead.
- The L1 gate's alias-table grep covers `main.rs` and two identifier names
  only; a table elsewhere in the shell, or under another name, passes.
- `AccountHealth::project` does filesystem I/O per account per projection.
- `env_base_url` accepts `http://` to any host and no route event records the
  destination, so an ambient variable can redirect a keychain-stored key.
- `keychain::load_secret(..).ok()` collapses "not found" and "keychain
  locked" into `None`, silently substituting an ambient env key.
- `SseDecoder.carry` is unbounded and there is no whole-stream timeout.


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
