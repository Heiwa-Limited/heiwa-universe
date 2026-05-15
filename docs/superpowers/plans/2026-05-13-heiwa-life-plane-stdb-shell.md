# Heiwa Life Plane STDB/Shell Implementation Plan

Status: active plan, narrow P0.

Goal: turn Devon/Claude/Codex scheduling and brief signals into a Heiwa-native
life plane using Rust, Python, TypeScript, SpacetimeDB, and shell commands.

## Sequence

1. STDB schema
   - Add `life_sources`, `life_memory_events`, `life_schedule_windows`,
     `life_briefs`, `life_action_items`, `life_readmodel_snapshots`, and
     `life_automation_sources`.
   - Add reducers for upsert/append/retire/refresh.
   - Keep raw source refs private and sanitized readmodel rows public-safe.

2. Protocol schemas
   - Add JSON schemas for life memory events, briefs, and readmodel snapshots.
   - Use schemas as importer and TypeScript contract.

3. Python importer
   - Read only configured local sources.
   - Parse home markdown, Claude scheduled-task JSON, and Codex automation TOML.
   - Emit schema-valid JSONL.
   - Default command: dry run, no STDB mutation.

4. Rust shell/runtime
   - Add `heiwa life status`.
   - Add `heiwa life today`.
   - Add `heiwa life freshness`.
   - Add `heiwa life approvals`.
   - Add `heiwa life import ... --dry-run`.
   - Add STDB sync only after dry-run proof.

5. TypeScript Heiwa.app
   - Show sanitized today readmodel.
   - Show source freshness.
   - Show brief cards and approval cards.
   - Do not read home files directly.

6. Shell command receipts
   - Keep `!command` under tool lease.
   - Log shell command, risk class, cwd, exit code, stdout/stderr hash, and
     receipt id into STDB/evidence.

## First Build Slice

Build this first:

```bash
heiwa life import home --dry-run
heiwa life import claude --dry-run
heiwa life import codex --dry-run
heiwa life status --json
```

Expected output:

- no network required
- no secrets printed
- no external side effects
- source paths and freshness listed
- rows counted by table
- privacy/approval tier shown
- STDB mode shown as `connected` or `offline-spool`

## Source Map

Private local inputs:

- `~/plans/ultimate_devon/current_state_register.md`
- `~/plans/ultimate_devon/heiwa_life_project.md`
- `~/plans/ultimate_devon/heiwa_background_machine_plane_2026-05-13.md`
- `~/plans/ultimate_devon/schedule_and_automation_map.md`
- `~/plans/ultimate_devon/life_os_roi_cadence_2026-05-12.md`
- `~/plans/ultimate_devon/work_schedule_may_2026.md`
- `~/plans/ultimate_devon/daily_scorecard.md`
- Claude scheduled-task JSON under `~/Library/Application Support/Claude/...`
- Codex automations under `~/.codex/automations/`

Repo stores paths/contracts, not raw imported rows.

## Command Contract

```bash
heiwa life status [--json]
heiwa life today [--json]
heiwa life freshness [--json]
heiwa life approvals [--json]
heiwa life brief --am|--pm|--weekly [--dry-run]
heiwa life log <domain> <type> [--field key=value ...]
heiwa life import home|claude|codex|calendar [--dry-run] [--jsonl]
heiwa life sync [--dry-run]
```

## Done Means

- `cargo check -p heiwa_protocol` passes after schema references, if Rust types
  are touched.
- STDB module builds after table/reducer additions.
- importer dry-runs produce JSON that validates against protocol schemas.
- shell output is concise enough for daily use.
- no raw personal data or secrets are added to Git.
