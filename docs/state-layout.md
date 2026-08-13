# Heiwa State Layout

Canonical layout for the local-first state root used by the `heiwa` runtime.

Default location: `~/.heiwa/` (override via `HEIWA_HOME`).

## Top-Level

| Path                     | Owner    | Purpose                                                         |
| ------------------------ | -------- | --------------------------------------------------------------- |
| `~/.heiwa/config.toml`   | operator | Local profile, route prefs, BYOX registration defaults          |
| `~/.heiwa/accounts.json` | runtime  | Connected provider accounts (status, models, expiry refs)       |
| `~/.heiwa/identity.json` | runtime  | Operator identity bound to this machine                         |
| `~/.heiwa/machine.json`  | runtime  | Device manifest (id, hostname, os, arch, installed_at)          |
| `~/.heiwa/evidence/`     | runtime  | Canonical versioned JSONL evidence journals                     |
| `~/.heiwa/secrets/`      | runtime  | OS-keychain-backed secret refs; never raw secrets in plain JSON |
| `~/.heiwa/state/`        | runtime  | Mutable runtime state (see below)                               |
| `~/.heiwa/sessions/`     | runtime  | Session transcripts and per-session metadata                    |
| `~/.heiwa/logs/`         | runtime  | Rotating runtime logs                                           |
| `~/.heiwa/cache/`        | runtime  | Provider response caches, model lists, expensive lookups        |
| `~/.heiwa/bin/`          | install  | Helper binaries (`heiwa-route`, etc.)                           |
| `~/.heiwa/app/Heiwa.app` | install  | HOME-local primary user input/display launcher for Heiwa.app    |
| `~/.heiwa/state.db`      | runtime  | Optional SQLite ledger (quotas, evidence)                       |
| `~/.heiwa/state/lance/`  | runtime  | Derived local recall index; safe to rebuild from text truth     |

## `~/.heiwa/state/` Subtree

This subtree is the only place runtime mutation happens for life/workers/approvals/evidence.

| Path                                  | Writer                           | Reader                                             |
| ------------------------------------- | -------------------------------- | -------------------------------------------------- |
| `state/workers.json`                  | `heiwa workers heartbeat`        | `heiwa workers status`, `heiwa app runtime status` |
| `state/dispatch/requests/`            | runtime, brokers                 | `heiwa approvals list`, `heiwa approvals show`     |
| `state/dispatch/approvals/decisions/` | `heiwa approvals decide`         | runtime brokers, audit                             |
| `state/dispatch/results/`             | runtime                          | audit                                              |
| `state/evidence/<utc-date>/`          | runtime, brokers                 | audit                                              |
| `state/health/doctor_latest.json`     | `heiwa doctor`                   | UI, CI                                             |
| `state/inventory/`                    | runtime                          | `heiwa providers`, `heiwa models`                  |
| `state/schedulers/`                   | scheduler                        | audit                                              |
| `state/life/readmodel.json`           | `heiwa life import` (when wired) | `heiwa life today`, `heiwa life status`            |
| `state/mail/headers.jsonl`            | mail bridge (planned)            | `heiwa life today`, urgency triage                 |
| `state/locks/`                        | runtime                          | runtime                                            |
| `state/net/`                          | runtime                          | telemetry                                          |
| `state/resources/`                    | runtime                          | scheduler                                          |

## Hard Rules

1. **No raw provider secrets in `state/`**. Use `~/.heiwa/secrets/` keychain refs only.
2. **Probe-only by default**. CLI commands must not write under `state/` unless an explicit subcommand or `--write`/non-`--dry-run` invocation says so.
3. **JSON Lines for append-only**. Logs and headers append to `.jsonl`; index/snapshot files use `.json`.
4. **UTC-stamped subdirs**. Time-bucketed evidence uses `YYYY-MM-DD/` UTC.
5. **Container-friendly**. The whole `~/.heiwa/` tree is mountable into a container so the same binary works on host and in Docker.

## Container Mount

```bash
docker run --rm \
  -v "$HOME/.heiwa:/root/.heiwa" \
  ghcr.io/strategizing/heiwa:dev app runtime status --json
```

The container ships with `HEIWA_HOME=/root/.heiwa` and `HEIWA_DEFAULT_POLICY=local-only-no-side-effects`.

## Distribution

GitHub is the source of truth for source and binaries.

- Source: <https://github.com/Heiwa-Limited/heiwa-universe>
- Container: `ghcr.io/strategizing/heiwa:<tag>` (built from `apps/heiwa_shell/Dockerfile`)
- Binary releases: GitHub Releases on tag push (see `.github/workflows/release.yml`)

## Filesystem Hygiene

Treat `state/` as recreatable. The only paths that contain authoritative truth are:

- `accounts.json`
- `identity.json`
- `machine.json`
- `connection.json`
- `secrets/` (keychain references)

Everything in `state/`, `cache/`, `logs/`, and `sessions/` is recoverable from the
backend (when connected) or rebuildable by running `heiwa doctor`, `heiwa life import`,
and the relevant heartbeats.
