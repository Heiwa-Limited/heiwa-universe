# Heiwa Tool-Call Runtime and Fragmentation Audit

Date: 2026-06-11
Status: checkout audit after Hermes Tauri/app work, before device promotion

## Result

Heiwa has a real but narrow tool-call substrate:

- executable now: `fs.list`, `fs.read`, `repo.grep`
- evidence shape: `ToolCallReceipt` with `success`, `failure`, `denied`
- approval gate exists for high-risk unknown calls
- runtime API now exposes the capability/tool-call contract at `/api/v1/capabilities`

Heiwa does not yet have product-grade shell, isolated browser, full computer-use, or calendar tool-call adapters. Those are target lanes and must remain marked as target-only or declared-without-adapter until wired and tested.

## Verified Facts

- Installed product runtime on `7474` is reachable but stale relative to this checkout.
- Before promotion, `http://127.0.0.1:7474/api/v1/capabilities` returned `index.html`, proving the running process did not have the new endpoint behavior.
- Checkout runtime on `7475` returned JSON for `/status/health`, `/api/v1/repl`, and `/api/v1/capabilities`.
- `agentic_smoke_dispatches_fs_list_and_records_receipt` passes and proves one model-style `fs.list` tool call executes through the local registry and records a receipt.
- `apps/heiwa_app/desktop/*` was hidden by global ignore rule `/Users/dmcgregsauce/.gitignore_global:12: Desktop/`.
- This checkout now overrides that ignore for desktop source while keeping `node_modules`, `dist`, generated Tauri schemas, and build targets ignored.

## Wrong Dev Perception Found

1. Native app package looked created, but source files were ignored.
   - `Cargo.toml` now references `apps/heiwa_app/desktop/src-tauri`.
   - Normal `git status` did not show `apps/heiwa_app/desktop/*`.
   - A commit without ignore correction would have created a broken workspace for other machines.
   - Fixed in checkout by unignoring intended desktop source in `.gitignore`.

2. `7474` health did not prove current checkout behavior.
   - Health was ok.
   - New `/api/v1/capabilities` behavior was absent and fell through to SPA `index.html`.
   - Runtime verification must use an alternate checkout port before install/update.

3. Capability inventory was mistaken for tool readiness.
   - Existing capability catalog exposed provider/plugin/source counts.
   - It did not expose executable tool schemas, risk classes, leases, or target-only gaps.

4. `shell` looked available because a lease is granted in REPL setup.
   - Agentic registry currently wires only `fs.list`, `fs.read`, and `repo.grep`.
   - Shell has no local MCP adapter yet, so it is now surfaced as `declared_no_adapter`.

5. Desktop/app docs are split across maturity eras.
   - Some docs say `Heiwa.app` is installed primary input/display.
   - Other docs still say `apps/heiwa_app` is only companion visual shell or not native yet.
   - Current truth: Tauri app exists and was built locally, but source is ignored/untracked and not yet a reviewed committed foundation.

6. Provider status has mixed semantics.
   - Registry can show Antigravity connected as an account/provider surface.
   - CLI discovery can still say `antigravity` is not installed.
   - UI must distinguish provider account presence, desktop app presence, CLI adapter presence, and loop-capable execution.

7. Hermes skills are local Hermes memory, not Heiwa product capability.
   - `~/.hermes/skills/software-development/heiwa-runtime-development`
   - `~/.hermes/skills/software-development/heiwa-local-runtime-operations`
   - Useful as handoff notes, but not runtime features until copied into Heiwa docs/skills/capability manifests.

8. `heiwa app update --source checkout` is binary-first despite the command name.
   - Dry-run target is `~/.heiwa/bin/heiwa`.
   - Tauri bundle install logic lives under `heiwa_install::write_home_app_launcher_internal`.
   - Before calling this a true app update, update plan must include `~/.heiwa/app/Heiwa.app`, bundle source, codesign state, and app receipt.

## Updated Work Queue

### P0 - Do Before Next App Promotion

1. Commit intended `apps/heiwa_app/desktop/*` source.
   - Source now appears in `git status`.
   - Keep `node_modules`, `dist`, generated schemas, and bundle artifacts ignored.

2. Keep checkout verification mandatory.
   - Run current checkout on `7475`.
   - Probe `/status/health`, `/api/v1/capabilities`, `/api/v1/repl`.
   - Only then run `heiwa app update --source checkout`.

3. Add shell capability registry, not raw shell execution.
   - Start with read-only commands: `git.status`, `git.diff.stat`, `cargo.test.focused`, `npm.typecheck`.
   - Each command needs id, command template, risk class, approval class, cwd policy, output cap, and evidence mapping.

4. Add app/device promotion receipt.
   - Record source commit, dirty state, bundle id, app path, DMG path, codesign state, installed binary, runtime process state, and verification probes.

5. Make `heiwa app update --source checkout --dry-run --json` app-aware.
   - Report installed binary and installed app separately.
   - Report built desktop bundle source if present.
   - Do not silently restart `7474`.

### P1 - Tool-Call Product Lanes

6. Build isolated browser lane.
   - Per-task profile.
   - Screenshot/evidence capture.
   - Approval gate for logged-in workflows and form submits.

7. Build computer-use lane.
   - Inspect/propose/approve/execute/receipt.
   - No direct side effects without approval.

8. Build calendar/scheduling read model.
   - Read-only first.
   - Expose today/upcoming/prep windows.
   - Mutations later through approvals.

9. Add Skills/Workflows surface.
   - Treat as reusable capability manifests.
   - Link each skill to tools, risk, approval, evidence, and source docs.

### P2 - Cleanup and Product Coherence

10. Reconcile docs to one app maturity statement.
   - Installed `Heiwa.app` target is primary input/display.
   - Tauri package is emerging foundation, not product-complete until tracked, signed/notarized, and runtime-start UX exists.

11. Retire or rename fragmented app clients.
    - `clients/web`: public/static web surface.
    - `clients/cockpit`: localhost app client.
    - `desktop`: native wrapper over runtime.
    - Remove old references to `clients/macos` once `desktop` is committed.

12. Normalize provider capability semantics.
    - Separate account auth, installed CLI, installed desktop app, local model runtime, and loop-capable adapter.

13. Add capability contract to app UI.
    - Render executable tools vs target-only tools.
    - Show risk, approval, adapter, evidence receipt shape, and next missing adapter.
