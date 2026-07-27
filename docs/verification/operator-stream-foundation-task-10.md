# Operator Stream Foundation — Task 10 Verification Record

**Recorded:** 2026-07-26
**Checkout under test:** `cce53322`
**Scope:** isolated checkout runtime only; no installed runtime promotion.

This is tracked, sanitized acceptance evidence. It records no machine-auth
token, bearer value, bootstrap value, cursor, or provider credential.

## Isolated runtime and cleanup

```bash
WORK_ROOT="$(mktemp -d /private/tmp/heiwa-task10-e2e.XXXXXX)"
HEIWA_EVIDENCE_DIR="$WORK_ROOT/evidence" \
HEIWA_STATE_DIR="$WORK_ROOT/state" \
HEIWA_MACHINE_AUTH_TOKEN='<isolated-test-token>' \
cargo run -q -p heiwa-shell --bin heiwa -- app start --port 7475 --no-open
```

- Result: checkout runtime started on `127.0.0.1:7475`; exact runtime PID was
  `54124`.
- `7474` was not requested, stopped, restarted, or otherwise touched.
- Cleanup: `kill -TERM 54124`; `curl --connect-timeout 1 --max-time 2
  http://127.0.0.1:7475/` failed to connect; only
  `/private/tmp/heiwa-task10-e2e.b946t3` was removed.

## Acceptance results

| Proof | Reconstructable command or probe | Result |
| --- | --- | --- |
| Idempotency | Bearer-authenticated `POST /api/v1/operator/threads`, then identical `POST /api/v1/operator/threads/task10-native/turns` twice with one `client_request_id` | Thread create `200`; submits `202`/`202`; same turn id; `duplicate` false then true. |
| Signed native replay/reconnect | `HEIWA_OPERATOR_E2E_WS_BASE_URL=ws://127.0.0.1:7475 HEIWA_OPERATOR_E2E_TOKEN='<isolated-test-token>' HEIWA_OPERATOR_E2E_THREAD_ID=task10-native HEIWA_OPERATOR_E2E_START_CURSOR='<cursor-from-first-submit>' cargo test -p heiwa-desktop operator_stream::tests::native_operator_external_runtime_replays_then_resumes_without_duplicates -- --ignored --exact` | Passed: signed native HTTP/WS, durable replay, forced reconnect, no duplicate durable event id. |
| Restart recovery | `cargo test -p heiwa-shell --test operator_api` | Passed: `app_boot_recovers_open_turn_exactly_once_across_restarts` verifies one `RUNTIME_RESTART` interruption. |
| Invalid cursor | Authenticated `GET /api/v1/operator/threads/task10-native/events?after=not-a-cursor` | `400`, `{"error":{"code":"invalid_cursor"},"ok":false}`. |
| FTS/Lance equivalence | `cargo test -p heiwa-session --all-features --test operator_service deleting_derived_indexes_rebuilds_identical_fts_and_lance_event_sets_from_journal` | Passed: one test; FTS and Lance event IDs identical before and after derived-index rebuild. |

Focused supporting gates also passed:

```bash
cargo test -p heiwa-provider --lib detect::ollama::tests
cargo test -p heiwa-shell --bin heiwa ollama_models_payload_uses_resolved_override_not_live_default
bash scripts/check_model_call_boundary.sh
bash scripts/check_backend_transition.sh
cargo fmt --check
git diff --check
cargo test --workspace --all-features
```

The normal workspace suite leaves the external-runtime Desktop test ignored
because it needs an explicitly isolated endpoint. The command above ran that
test against `7475` and passed.
