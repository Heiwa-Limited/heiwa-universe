# Operator stream final fix report

Date: 2026-07-26

Status: complete in atomic commit.

## Closed findings

- Unified plain REPL, cockpit agentic, routed model, and loop execution around the canonical operator turn lifecycle. Live compatibility transcript appends were removed.
- Preserved provider `Done` usage/cost truth across cancel races and made invoked cancellation attempts durable as cancelled/uncertain outcomes.
- Expanded the evidence sensitive-material gate for embedded/indented authorization, API keys, private keys, JWT/OAuth values, recursive sensitive object keys, and explicit redaction markers.
- Bound browser bootstrap/session grants to the exact loopback origin and serving port, enforced exact Host/Origin checks, removed wildcard CORS, and retained native HMAC plus compatibility bearer auth.
- Made shell smoke/local-boot tests hermetic with disposable HOME/evidence/state/index roots, cleared provider/auth inheritance, controlled fixture PATH, and no live Ollama endpoint.
- Replaced retired `heiwa life` STDB authority language with the local approval journal and honest preview-only import status. Expanded the backend-transition gate across active shell source.
- Documented operator stream append-forever, sole-writer, canonical runner, and narrow restart-recovery ownership in `HEIWA.md`.

## Verification

- `cargo test -p heiwa_evidence` — 51 passed.
- `cargo test -p heiwa-shell --test model_call_executor` — 22 passed.
- `cargo test -p heiwa-shell --test smoke` — 30 passed.
- `cargo test -p heiwa-shell --test operator_api` — 18 passed.
- `cargo test -p heiwa-shell --test app_api` — 3 passed.
- `cargo test -p heiwa-shell --test local_boot` — 1 passed.
- `npm --prefix apps/heiwa_app/desktop test` — 42 passed.
- `npm --prefix apps/heiwa_app/desktop run typecheck` — passed.
- `npm --prefix apps/heiwa_app/desktop run build` — passed.
- `cargo test --workspace --all-features` — passed.
- `cargo fmt --all -- --check` — passed.
- `git diff --check` — passed.
- `bash scripts/check_model_call_boundary.sh` — `model_call_boundary=ok`.
- `bash scripts/check_backend_transition.sh` — passed.
- `bash scripts/audit_product_surface.sh` — zero unclassified tracked files.
- `cargo tree -p heiwa-shell -e features -i heiwa_embed` — the shell default feature graph includes `heiwa_embed/lance`.
- Clean post-commit `bash scripts/check_agent_baseline.sh --branch feature/backend-lance-github` passed: correct branch, no tracked or untracked changes, one branch worktree, and all runtime/backend/model-boundary/release/product-surface gates green.

## Fresh isolated 7475 acceptance

- Started checkout `target/debug/heiwa app start --port 7475 --no-open` as PID `43403` with disposable root `/private/tmp/heiwa-final-fix-7475.wXmEIz`.
- Missing auth returned `401`; a valid bearer relayed through hostile `Host: 127.0.0.1:9999` returned `401`.
- Two identical submissions returned one stable turn id with duplicate flags `false` then `true`.
- Replay contained exactly one `user_message`, one `assistant_completed`, and one terminal event for the submitted turn.
- The ignored native external-runtime acceptance passed against `ws://127.0.0.1:7475`, proving signed native HTTP submission, authenticated WebSocket replay/live tail, forced reconnect, cursor resume without duplicate durable IDs, and one deterministic planned/completed route pair.
- Final replay showed both acceptance turns with `user=1`, `assistant=1`, `terminal=1`; no open turn remained.
- Sent Ctrl-C only to the checkout runtime session; it reported `heiwa app stopped`. Port `7475` was closed, the disposable root was removed, and installed port `7474` remained running and untouched.

No push, merge, promotion, external provider call, or installed-runtime mutation was performed.

## Known concerns

- The fresh live acceptance intentionally used deterministic local turns. Provider streaming, cancellation, and cost evidence are covered by focused fake-adapter tests rather than a billable external provider call.
- Historical compatibility readers remain for old transcript data, but current interactive writes now use the operator journal lifecycle.
