# Heiwa review standards

These are the repo's own rules, restated in reviewable form. They come from
`CLAUDE.md`, `AGENTS.md`, and `CONTRIBUTING.md` — those files are the source of
truth; this file exists so a reviewer can check changes against them.

## Honesty over completeness theater

`CLAUDE.md` lists this as a hard rule, and it is enforceable on diffs.

Flag any added doc, README line, comment, log string, or user-facing message
that claims a capability the diff does not implement. Concretely:

- Describing a provider as "supported" or "wired" when the adapter only does
  discovery, not execution. Provider maturity is not uniform: Claude Code,
  Codex, Gemini CLI, Grok, Antigravity, and Ollama are at different stages.
- Describing `apps/heiwa_app` as a native desktop runtime. It is a companion
  visual shell (web client today).
- Claiming parity with Hermes or OpenHuman. Do not assert parity that the code
  in the diff does not demonstrate.
- Framing Heiwa as owning provider inference internals, system prompts, auth
  semantics, model inventory, or quota behavior. Those stay provider-owned.
- Web-first or hosted-control-plane framing where local-first framing is
  correct. GitHub is the distribution surface; a cloud/VPS plane is deferred.

## Legacy surfaces are not work targets

Legacy Hub, old CLI, and limb surfaces were removed from the tree on
2026-07-06. SpacetimeDB was extracted on 2026-07-15.

Flag new production code that takes a dependency on `archive/`, on
`apps/heiwa_hub`, or on `crates/heiwa_stdb` as if STDB were a normal operator
surface. STDB is a backend adjudication/evidence plane, not something an
operator should have to think about. Repairs to legacy tests are fine when the
PR says that is the intent.

## Evidence-plane correctness

`crates/heiwa_evidence` owns the JSONL journal: envelopes, locking, replay,
recovery, and compaction. Core and orchestrator consume it through `evidence/`
shims.

Hold changes here to a higher bar than the rest of the tree:

- A write path that can leave a partial or unparseable JSONL line after a crash
  or a mid-write cancellation.
- Lock acquisition that is not released on every error path, or a lock ordering
  that differs between two call sites.
- Replay or compaction that can drop, reorder, or duplicate envelopes.
- Anything that deletes durable `~/.heiwa/state` evidence without an explicit
  operator-approval path.

## Runtime paths must not panic

In `apps/heiwa_core`, `apps/heiwa_orchestrator`, `apps/heiwa_shell`, and
`crates/**` non-test code, flag `unwrap()`, `expect()`, `panic!`, indexing that
can go out of bounds, and integer arithmetic that can overflow in release,
whenever the value derives from config, provider output, network data, user
input, or on-disk state. Tests, build scripts, and `const` contexts are exempt.

Also flag a swallowed error: a `let _ =`, an empty `Err` arm, or a `catch` that
neither propagates nor logs, on a path that mutates state.

## Cross-platform: CI builds Linux, macOS, and Windows

Devon develops on Windows + WSL and the matrix is real, so POSIX assumptions
break the build for the person who wrote them.

Flag hardcoded `/` path separators built by string concatenation instead of
`Path`/`PathBuf` joins, hardcoded `/tmp` or `~` expansion, assumptions that a
`HOME` env var exists, shelling out to binaries not guaranteed on all three
runners, and case-sensitive filename assumptions.

Note that the workspace needs `protoc` (via lance) and, on Linux, `libdbus`
(via keyring). A change that adds a new system dependency must also update
`.github/workflows/ci.yml`, or CI passes locally and fails on a fresh runner.

## Runtime identity confusion

From `AGENTS.md`: port `7474` is the installed product runtime; a checkout
under verification uses a temporary alternate port such as `7475`.

Flag code, scripts, or docs that hardcode `7474` for verification of a local
build, or that treat any reachable localhost app as the binary just built. If a
new API endpoint can fall through to the SPA and return `index.html`, say so —
that failure mode reads as success.

## Secrets and vault

Flag credentials, tokens, or API keys added to tracked files, to `.env.example`
with real values, to log or error strings, or to test fixtures. Provider auth
material belongs behind `crates/heiwa_vault`. Also flag a new log line that
prints a whole config struct or provider response that could carry a token.

## Scope discipline

`CONTRIBUTING.md` asks for changes scoped to one build-matrix task. If a diff
mixes an unrelated refactor into a feature change, say which files look
unrelated — do not block on it, just name them.
