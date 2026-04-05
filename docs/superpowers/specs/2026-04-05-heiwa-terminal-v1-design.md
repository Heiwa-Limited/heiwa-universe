# Heiwa Terminal v1 Design: The Native Cockpit

**Date:** 2026-04-05  
**Status:** Approved  
**Nature:** Architectural Evolution & UI/UX Redesign  

## 1. Product Identity & Architecture

Heiwa is a local-first, sovereign AI operator cockpit. The primary product surface is the `heiwa` binary installed on the user's machine.

- **Rust-First Sovereignty**: The `heiwa` binary is the single, authoritative Rust entry point.
- **Native Cockpit (TUI)**: The default interactive interface, built in Rust, providing a multi-pane operator experience.
- **JS/TS Boundary**: 
    - No raw JavaScript in the Heiwa Terminal runtime path.
    - TypeScript is allowed for tooling, hosted/web surfaces, generated bindings, and optional non-authoritative clients.
    - If TypeScript ever wraps the terminal experience, Rust remains the engine and source of truth.
- **Offline-First**: Immediate utility with local models and on-device provider CLIs. No Heiwa account is required for core functionality.
- **Additive Cloud Layer**: A Heiwa account provides transient sync for settings, device registry, and hosted history, but remains non-blocking and optional.

## 2. Deterministic Routing & Precedence

Heiwa uses a strict, predictable precedence model. Routing is a transparent feasibility check, not a "fuzzy" or hidden auto-substitution.

### Precedence Order
1. **Direct Turn Instruction**: Specific `@model`/@`provider` tags, natural-language directives (e.g., "use opus 4.6 with my oauth key"), and explicit slash/manual pins.
2. **Session/Manual Override**: Pins or modes set for the duration of the current session.
3. **Local Workspace Profile**: Authoritative project intent; defined in the current project/directory.
4. **Synced Account Profile**: Transient baseline; data pulled from the Heiwa account.
5. **Device State**: Local-only overlay and hardware feasibility context (e.g., "Is Ollama running?", "Is GPU available?").
6. **DREX Automatic Selection**: Kernel-level choice based on cost, privacy, and capability among remaining valid candidates.

### Routing Behavior
- **No Silent Auto-Fallback**: Automatic substitution is disabled by default. If a requested target is unavailable, Heiwa fails fast and reports the exact reason.
- **Full-Auto Opt-In**: Silent fallback occurs only when the user has explicitly enabled "full-auto" behavior via command or settings.
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
- `crates/heiwa_router` (Optional): Extraction for DREX route preparation if logic grows substantial.
- `crates/heiwa_profile` (Optional): Extraction for layered config management if complexity warrants it.

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
