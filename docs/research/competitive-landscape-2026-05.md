# Competitive Landscape — 2026-05

> Engineering reference, not marketing. Operator-private. Filters every peer through Heiwa's doctrine: local-first truth, providers own inference, Heiwa owns routing/evidence/memory.
>
> Marketing-facing positioning lives in `apps/heiwa_app/clients/web/vs/` (Manifest, LiteLLM, OpenRouter) and `docs/pi_mono_comparison.md` (Pi-Mono). This file does **not** duplicate those — it covers the broader coding-agent and desktop-AI ecosystem and converts findings into Heiwa work.

## Scope

Peer products that overlap Heiwa on at least one axis: routing fabric, agent runtime, desktop AI shell, IDE-side coding agent, or local model host. Excluded: pure framework libraries (LangGraph, CrewAI, AutoGen) — different category.

## Peer Matrix

| Product | Stack | Distribution | Code-quality signal | Usability today | Future-proofing | Heiwa lesson |
|---|---|---|---|---|---|---|
| **Aider** | Python | `pip` / `pipx`, no desktop bundle | Strong test discipline, mature, transparent | Best-in-class git workflow integration | Provider-agnostic via litellm; weak desktop story | Git-as-evidence is a model worth copying for run records |
| **opencode** | Rust | single binary | Clean modules, evolving fast, OpenRouter-leaning | TUI good, no desktop shell | Permissive license; aligned to OpenRouter economics | Read for routing/session structure; do not adopt OpenRouter assumption |
| **Goose** (Block) | Rust | single binary + macOS app | High; MCP-native from day one; structured tool calls | Daemon + chat client; clean MCP UX | MCP-first, multi-provider | MCP plane is a first-class capability, not a plugin afterthought |
| **Plandex** | Go | single binary | Good; opinionated plan/branch model | Plan-branch-merge metaphor is sticky | Server+client, weak local-first | Plan-as-artifact is a real abstraction; we have it conceptually but don't materialize it |
| **Cursor** | Electron + custom | `.dmg`, signed | Closed source; ships fast, polish high | Best inline editing UX in market | Single-vendor lock-in; cloud-coupled | UX bar to clear; but their cloud coupling is the trap to avoid |
| **Continue.dev** | TS | VS Code/JetBrains plugin | OSS, active, getting more product-shaped | Lives in IDE, not standalone | Provider-agnostic; config-file driven | Config-as-product (`config.json`) makes routing legible — port that pattern to `~/.heiwa/config.toml` discoverability |
| **Cody** (Sourcegraph) | TS + Go | IDE plugin + cloud | Strong; enterprise context retrieval | IDE-bound | Enterprise-ready, code-graph dependent | Code-graph as context source — defer until L4 routes are stable |
| **Zed** | Rust | `.dmg`, signed; Linux deb/tar | Excellent (GPUI, modular); Rust-native AI | Editor-first, AI-augmented | Multi-provider, LSP-strong | Reference for native macOS Rust packaging done right |
| **Warp** | Rust | `.dmg`, signed | Closed but high quality | AI-in-terminal best in market | Cloud-account-required (friction) | AI-in-terminal UX bar; do without the cloud login requirement |
| **LM Studio** | Electron | `.dmg`, signed | Closed; UX-polished local model host | Best local-model UX outside Ollama | Single-machine, no agent layer | Reference for "local model UX" we need parity with from `heiwa providers` |
| **Ollama** | Go | `.dmg`, signed; brew | Excellent; daemon model is correct | Already our default local route | Provider-side, not agent-side | Don't reinvent — wrap and trust |
| **Raycast (AI)** | Swift (native macOS) | `.app` notarized | Closed but exemplary | Universal launcher feel; AI is contextual | macOS-only; closed extension protocol | Quick-action surface ("⌘-space then prompt") is a UX target for `heiwa` shell |
| **Tabby** | Rust | binary + container | OSS; self-hostable | Code-completion focus | Self-host friendly | Self-hosting bar to match for on-prem buyers |
| **Open Interpreter** | Python | `pip` | Loose; demo-quality in spots | "Run code from prompt" simplicity | Sandboxing weak | Anti-pattern: power without policy is what Heiwa policy/leases prevent |

(Manifest, LiteLLM, OpenRouter, Pi-Mono — see existing artifacts.)

## Lessons lifted, ranked by signal

1. **MCP plane belongs in the kernel, not a plugin layer.** Goose's lead comes from treating MCP as a typed, first-class transport. Heiwa already has MCP in `crates/heiwa_provider`-adjacent code; the open question is whether `apps/heiwa_core/src/drex/router.rs` consults MCP capabilities when picking a route, or only after.
2. **Native Rust desktop packaging is solved.** Zed and Warp ship `.dmg`-notarized binaries from Rust. We do not need Electron. Pick **Tauri 2.x** for `apps/heiwa_app/clients/macos` (Rust core, system webview, ~10 MB binary, signed/notarized via `cargo-tauri`).
3. **Config-as-product matters.** Continue.dev's success is partly that `~/.continue/config.json` is legible and forkable. `~/.heiwa/config.toml` should be promoted to a public contract with examples — not buried.
4. **Plan-as-artifact is a real abstraction we don't fully materialize.** Plandex serializes plans to git branches; we have HEIWA.md plan docs but no first-class plan object in the runtime. Decide whether `heiwa plan` becomes a kernel concept post-L4.
5. **Cloud-account requirement is the trap.** Warp's biggest churn driver is "must sign in to Warp Cloud." Heiwa should make every paid feature available against local state alone, with cloud as opt-in evidence sync.
6. **Anti-pattern: power without leases.** Open Interpreter is a useful contrast — same intent surface, no policy plane. Heiwa's capability fabric (`docs/capability-fabric.md`) is the differentiator; protect it.

## What Heiwa needs to ingest to keep deciding well

These are concrete data dependencies the router and operator surface need. None are speculative.

| Need | Source | Refresh cadence | Consumer |
|---|---|---|---|
| Provider model catalog (id, context window, modalities, prompt-cache support) | provider APIs + `models.dev` index | weekly | `heiwa_provider::detect`, `models` command |
| Model capability table (tool use, JSON mode, vision, embeddings, audio) | provider docs + own probe results | monthly + on first run | `drex::router::plan_route` |
| Pricing feed (per-provider, per-model, input/output/cache rates) | provider pricing pages, OpenRouter price index | weekly | `heiwa_quota` cost translation |
| MCP server registry (name, transport, capability declaration) | upstream MCP registry + local install | on-change | router, `route preview` |
| Subscription quota semantics (Claude Pro, ChatGPT Plus, Google AI Pro limits) | provider docs + observed 429s | on-change | `heiwa_quota` rate-group cooldowns |
| Eval corpus for routing decisions ("which tier should answer X") | self-built fixtures + open eval suites (Aider polyglot, SWE-Bench Lite) | as written | router regression tests |
| Local capability probe (VRAM, model availability, adapter health) | runtime detection on launch | per-launch | `heiwa doctor`, `heiwa providers` |

Without these, the router optimizes against vibes. With them, the doctrine ("smallest sufficient model, shortest sufficient context, richest sufficient evidence") becomes measurable.

## macOS-first Heiwa.app packaging path

Status today: `apps/heiwa_app/package.json` is `tsc --noEmit` only. `clients/macos`, `clients/windows`, `clients/iphone` are empty scaffolds. There is no native packaging.

**Recommended stack: Tauri 2.x + system WebView (WKWebView on macOS).**

Why not Electron: 100+ MB binaries, RAM cost, Chromium attack surface, no Rust integration. Heiwa's center is Rust; a Rust shell is consistent.

Why not pure SwiftUI: locks Heiwa.app into Apple-only and replicates UI work for Linux/Windows. Tauri shares one webview-rendered cockpit across platforms while keeping the Rust kernel binary linked, not RPC'd over a socket.

Why not Slint or egui: no system webview means losing the existing `apps/heiwa_app/clients/cockpit` web UI investment.

### Concrete file plan (when greenlit)

| Path | Purpose |
|---|---|
| `apps/heiwa_app/clients/macos/Cargo.toml` | Rust crate that links `heiwa_core` directly + Tauri runtime |
| `apps/heiwa_app/clients/macos/tauri.conf.json` | bundle id, signing identity, entitlements |
| `apps/heiwa_app/clients/macos/src/main.rs` | thin: spawn cockpit webview, expose IPC commands that call `drex::router` and `heiwa_quota` in-process |
| `apps/heiwa_app/clients/macos/src/ipc.rs` | typed Tauri commands (`route_preview`, `quota_status`, `providers_list`) |
| `apps/heiwa_app/clients/cockpit/` | already exists; add Tauri-aware build target |
| `scripts/package_macos.sh` | `cargo tauri build` → `.dmg`, run `codesign` + `notarytool` |
| `.github/workflows/release-macos.yml` | tag-driven build, sign, notarize, attach to release (pairs with Codex `P2-release-workflow`) |

### Sequencing (gated on existing work)

1. **Block until** `worktree-agent-a3bf…` quarantine is merged — bundle would otherwise pull legacy crates (`heiwa_hub`, `heiwa_skills`, `heiwa_cognition`).
2. **Block until** L4 quota-as-gate lands — the cockpit's first useful screen is route preview + quota state.
3. macOS Tauri scaffold + signed/notarized DMG via local Apple ID first; pipeline second.
4. Linux: Tauri produces `.deb` and `AppImage` with same code path; CI added after macOS proven.
5. Windows: Tauri produces `.msi`. Defer until macOS+Linux are green; signing on Windows requires EV cert decision.
6. Mobile (`clients/iphone`): out of scope for this round. Tauri 2 mobile is workable but unproven for Heiwa's IPC patterns; revisit after desktop stabilizes.

### Distribution truth check

- **Code signing**: Apple Developer ID for macOS notarization is required ($99/yr). Without it, Gatekeeper blocks the binary.
- **Updates**: `tauri-plugin-updater` with signed manifests; updates served from GitHub Releases (matches Codex `P2-release-workflow` direction).
- **Telemetry**: opt-in only; reuse `heiwa_quota` ledger for local introspection rather than home-call.

## Open questions

1. **MCP plane scope** — does the router consult MCP capabilities pre-route, or only inside execution? Decides whether MCP belongs in `heiwa_core` or stays in `heiwa_provider`.
2. **Plan-as-artifact** — adopt Plandex's mental model (plans are durable, versioned, branchable) or keep plans as freeform markdown? Affects whether STDB grows a `plan` table.
3. **Cockpit IPC** — Tauri commands (sync) vs. spawned shell daemon over Unix socket (async, streamable). Streaming favors socket; latency favors in-process. Probably both, with route preview in-process and execution streams over socket.
4. **Cloud sync** — when (if ever) does Heiwa optionally push evidence to STDB cloud for cross-device continuity? Not now, but the schema decision to make it possible should be made before L5/L8.

## Cross-references

- `HEIWA.md` — canonical architecture
- `docs/capability-fabric.md` — typed capability/lease/evidence model
- `docs/provider-registry.md` — current adapter inventory
- `docs/superpowers/plans/2026-04-26-litellm-sidecar-adoption.md` — sidecar discipline
- `apps/heiwa_app/clients/web/vs/{manifest,litellm,openrouter}.html` — public positioning
- `docs/pi_mono_comparison.md` — Pi-Mono detail

## Next decisions this artifact unblocks

1. Greenlight Tauri 2.x for `clients/macos` (or pick alternative).
2. Decide MCP-in-router boundary (touches L4 design).
3. Approve the seven data feeds in "What Heiwa needs to ingest" as router/quota inputs and assign owners.
