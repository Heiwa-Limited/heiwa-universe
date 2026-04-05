# Heiwa Terminal v1 Design: The Native Cockpit

**Date:** 2026-04-05  
**Status:** Approved  
**Nature:** Architectural Evolution & UI/UX Redesign  

## 1. Product Identity & Architecture

Heiwa is a local-first, sovereign AI operator cockpit. The primary product surface is the `heiwa` binary installed on the user's machine.

- **`heiwa` Binary**: The single, authoritative Rust entry point.
- **Native Cockpit (TUI)**: The default interactive interface, built in Rust (using `ratatui` or similar), providing a multi-pane operator experience.
- **Zero JS Runtime**: No TypeScript or JavaScript is involved in the terminal execution path. Rust owns the engine, the TUI, and the provider supervision.
- **Offline-First**: Immediate utility with local models (Ollama, etc.) and on-device provider CLIs. No Heiwa account is required for core functionality.
- **Additive Cloud Layer**: A Heiwa account provides transient sync for settings, device registry, and hosted history, but remains non-blocking and optional.

## 2. Deterministic Routing & Precedence

Heiwa uses a strict, predictable precedence model. Routing is a transparent feasibility check, not a "fuzzy" or hidden auto-substitution.

### Precedence Order
1. **Direct Turn Instruction**: Specific `@model` or `@provider` tags in the current prompt.
2. **Session/Manual Override**: Pins or modes set for the duration of the current session.
3. **Local Workspace Profile**: Configuration defined in the current project/directory.
4. **Synced Workspace/Account Baseline**: Transient profile data pulled from the Heiwa account.
5. **Device Capability Discovery**: Hardware/env-level filter (e.g., "Is Ollama running?", "Is GPU available?").
6. **DREX Automatic Selection**: Kernel-level choice based on cost, privacy, and capability among remaining valid candidates.

### Routing Behavior
- **Fail-Fast**: If a requested target is unavailable, Heiwa reports the reason rather than silently switching.
- **Explicit Proposal**: Heiwa suggests substitutes if the primary target fails feasibility checks.
- **Provider Truth**: The UI explicitly displays provider status: Discovered, Authenticated, Executable, Loop-Capable, and Verified.

## 3. Native Cockpit UX (TUI)

The `heiwa` interactive session moves from a simple REPL loop to a full-screen cockpit layout.

### Layout Components
- **Top Header**: Active session name, workspace profile, current target (Provider/Model), sync status, device ID, and CWD.
- **Center Transcript**: Streamed conversation with rich Markdown rendering (headers, lists, tables), syntax-highlighted code blocks, diffs, and tool/evidence callouts.
- **Bottom Composer**: A persistent, multi-line capable input bar positioned above the footer.
- **Footer Strip**: Compact status bar showing routing mode, cost/token counters, active warnings, and keybindings.
- **Inspector Panes**: Toggleable side panels (Left: Sessions/Devices, Right: Route Decisions/Evidence/Receipts).

### Customization
Profiles (Local Workspace > Synced Account > Device) define:
- Layout and Pane visibility.
- Keybindings and Themes (Base neutral, Plum/Lavender accents).
- Routing defaults, Privacy postures, and Approval requirements.

## 4. Technical Migration Path

The implementation is an evolutionary extraction from the current `apps/heiwa_shell/src/main.rs` monolith.

### Crate Decomposition
- `apps/heiwa_shell`: CLI entry, boot logic, and Plain-mode fallback.
- `crates/heiwa_protocol`: New! Shared typed state, event models, transcript blocks, and command metadata.
- `crates/heiwa_tui`: New! The native cockpit renderer and terminal event loop.
- `crates/heiwa_router`: Logic for DREX route preparation and feasibility checking.
- `crates/heiwa_profile`: Layered config management (Device + Workspace + Sync).

### Implementation Phases
1. **Protocol Extraction**: Move `TelemetryState` and `ReplCommand` to `heiwa_protocol` and expand into a full UI/Session state model.
2. **TUI Scaffolding**: Build the basic `ratatui` frame, panes, and composer.
3. **Controller Refactor**: Update `run_repl()` to act as a controller emitting protocol events to the TUI.
4. **Profile & Routing Logic**: Implement the layered precedence and feasibility checks.
5. **Plain-Mode Consistency**: Update the non-interactive output to use the same `heiwa_protocol` event stream for its line-based rendering.

## 5. Boot Posture & Sync

### Boot Detection
- **Interactive TTY**: Boots the full native Cockpit (TUI) by default.
- **Non-Interactive**: Auto-demotes to Plain mode (piped output, CI, redirected streams) to preserve automation utility.
- **Escape Hatches**: `--plain`, `--offline`, and `--no-sync` flags allow manual overrides.

### Connectivity & Sync
- **Non-Blocking Init**: The UI renders immediately from local state. Background tasks handle SpacetimeDB attachment and profile sync.
- **Transient Account State**: Account-synced settings are pulled into memory; they do not overwrite local workspace files blindly.
- **Feature Marking**: Cloud-dependent skills or providers are visible in the UI but clearly marked as "Unavailable" when offline.
