# Competitive Landscape — 2026-05

> Engineering reference, not marketing. Operator-private. Filters every peer through Heiwa's doctrine: local-first truth, providers own inference, Heiwa owns routing/evidence/memory.
>
> Marketing-facing positioning lives in `apps/heiwa_app/clients/web/vs/` (Manifest, LiteLLM, OpenRouter) and `docs/pi_mono_comparison.md` (Pi-Mono). This file does **not** duplicate those — it covers the broader coding-agent and desktop-AI ecosystem and converts findings into Heiwa work.

## Scope

Peer products that overlap Heiwa on at least one axis: routing fabric, agent runtime, desktop AI shell, IDE-side coding agent, or local model host. Excluded: pure framework libraries (LangGraph, CrewAI, AutoGen) — different category.

## Peer Matrix

| Product                | Stack                | Distribution                      | Code-quality signal                                  | Usability today                           | Future-proofing                                     | Heiwa lesson                                                                                                          |
| ---------------------- | -------------------- | --------------------------------- | ---------------------------------------------------- | ----------------------------------------- | --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| **Aider**              | Python               | `pip` / `pipx`, no desktop bundle | Strong test discipline, mature, transparent          | Best-in-class git workflow integration    | Provider-agnostic via litellm; weak desktop story   | Git-as-evidence is a model worth copying for run records                                                              |
| **opencode**           | Rust                 | single binary                     | Clean modules, evolving fast, OpenRouter-leaning     | TUI good, no desktop shell                | Permissive license; aligned to OpenRouter economics | Read for routing/session structure; do not adopt OpenRouter assumption                                                |
| **Goose** (Block)      | Rust                 | single binary + macOS app         | High; MCP-native from day one; structured tool calls | Daemon + chat client; clean MCP UX        | MCP-first, multi-provider                           | MCP plane is a first-class capability, not a plugin afterthought                                                      |
| **Plandex**            | Go                   | single binary                     | Good; opinionated plan/branch model                  | Plan-branch-merge metaphor is sticky      | Server+client, weak local-first                     | Plan-as-artifact is a real abstraction; we have it conceptually but don't materialize it                              |
| **Cursor**             | Electron + custom    | `.dmg`, signed                    | Closed source; ships fast, polish high               | Best inline editing UX in market          | Single-vendor lock-in; cloud-coupled                | UX bar to clear; but their cloud coupling is the trap to avoid                                                        |
| **Continue.dev**       | TS                   | VS Code/JetBrains plugin          | OSS, active, getting more product-shaped             | Lives in IDE, not standalone              | Provider-agnostic; config-file driven               | Config-as-product (`config.json`) makes routing legible — port that pattern to `~/.heiwa/config.toml` discoverability |
| **Cody** (Sourcegraph) | TS + Go              | IDE plugin + cloud                | Strong; enterprise context retrieval                 | IDE-bound                                 | Enterprise-ready, code-graph dependent              | Code-graph as context source — defer until L4 routes are stable                                                       |
| **Zed**                | Rust                 | `.dmg`, signed; Linux deb/tar     | Excellent (GPUI, modular); Rust-native AI            | Editor-first, AI-augmented                | Multi-provider, LSP-strong                          | Reference for native macOS Rust packaging done right                                                                  |
| **Warp**               | Rust                 | `.dmg`, signed                    | Closed but high quality                              | AI-in-terminal best in market             | Cloud-account-required (friction)                   | AI-in-terminal UX bar; do without the cloud login requirement                                                         |
| **LM Studio**          | Electron             | `.dmg`, signed                    | Closed; UX-polished local model host                 | Best local-model UX outside Ollama        | Single-machine, no agent layer                      | Reference for "local model UX" we need parity with from `heiwa providers`                                             |
| **Ollama**             | Go                   | `.dmg`, signed; brew              | Excellent; daemon model is correct                   | Already our default local route           | Provider-side, not agent-side                       | Don't reinvent — wrap and trust                                                                                       |
| **Raycast (AI)**       | Swift (native macOS) | `.app` notarized                  | Closed but exemplary                                 | Universal launcher feel; AI is contextual | macOS-only; closed extension protocol               | Quick-action surface ("⌘-space then prompt") is a UX target for `heiwa` shell                                         |
| **Tabby**              | Rust                 | binary + container                | OSS; self-hostable                                   | Code-completion focus                     | Self-host friendly                                  | Self-hosting bar to match for on-prem buyers                                                                          |
| **Open Interpreter**   | Python               | `pip`                             | Loose; demo-quality in spots                         | "Run code from prompt" simplicity         | Sandboxing weak                                     | Anti-pattern: power without policy is what Heiwa policy/leases prevent                                                |

(Manifest, LiteLLM, OpenRouter, Pi-Mono — see existing artifacts.)

## 2026-05-26 Active Competitor Bar

This is the active context pack for developing Heiwa against the current peers.
Refresh before making parity claims.

| Peer                                                         | Verified current signal                                                                                                                                                                              | What they make feel real                                                                                                                                       | Heiwa must match or beat                                                                                                                                                                                                                                         |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Hermes Agent](https://github.com/NousResearch/hermes-agent) | GitHub API on 2026-05-26: `NousResearch/hermes-agent`, MIT, Python, latest release `v2026.5.16` / "Hermes Agent v0.14.0 (2026.5.16)", pushed `2026-05-26T06:49:03Z`, 167,955 stars and 27,811 forks. | Persistent personal agent, messaging gateway, skills, memory, cron jobs, provider/model switching, terminal backends, install/update/doctor surface.           | `heiwa` must feel like one durable operator, not a set of commands. Match: install/update/doctor, gateway-ready intake, skill/procedure memory, scheduled local jobs. Beat: typed leases, approvals, receipts, provider-owned runtime truth, STDB evidence sync. |
| [OpenHuman](https://github.com/tinyhumansai/openhuman)       | GitHub API on 2026-05-26: `tinyhumansai/openhuman`, GPL-3.0, Rust, latest release `v0.54.0` / "OpenHuman v0.54.0", pushed `2026-05-26T09:55:16Z`, 28,050 stars and 2,602 forks.                      | Desktop-native consumer UX, readable local Memory Tree, Obsidian-style wiki, 20-minute auto-fetch, one-click integrations through managed Composio/OAuth path. | Heiwa must ship native-feeling `Heiwa.app`, local readable memory/read model, freshness rules, connector setup that normal users can finish. Beat: safer privacy boundary, local execution authority, approval-gated writes, GitHub release provenance.          |

Source notes:

- Hermes README/docs position it as a self-improving agent with learning loop,
  skills, FTS5/session recall, Honcho user modeling, messaging gateway,
  scheduled automations, terminal backends, model switching, install/update/
  doctor, and MIT license. The terminal backends are execution environments for
  the agent shell, not proof of a cooperating-agent mesh.
- OpenHuman docs position Memory Tree as SQLite plus Markdown wiki under the
  local workspace, with source/topic/global summaries and automatic integration
  auto-fetch. Their README says OpenHuman uses local memory plus managed default
  services for account sign-in, model routing, web search proxying, OAuth/tool
  brokering, and Composio-backed integrations unless configured otherwise.
- OpenHuman uses Rust and vendored Tauri/CEF sources. Treat it as proof that a
  Rust desktop route is viable, not proof that plain Tauri 2 WebView is enough.
- Treat both as moving targets. Do not cite star counts, release numbers, or integration counts without refreshing.

## Heiwa Attention Contract

Before any new P0/P1 Heiwa implementation, the agent must keep these six
competitor lessons in working context:

1. **Install must be boring.** One command or signed app; `doctor` and `update`
   must prove what is installed, what is running, and where updates come from.
2. **Memory must be inspectable.** Local state must be readable as structured
   records and exportable/editable as Markdown where useful. Hidden embeddings
   are not enough.
3. **Freshness must be visible.** Users need to know what changed, what is
   stale, and what needs attention before the agent acts.
4. **Gateway intake must converge.** CLI, app, messaging, browser, and files
   should normalize into one typed intake/read-model stream instead of separate
   product silos.
5. **Autonomy must be governed.** Hermes/OpenHuman show breadth; Heiwa wins only
   if shell, browser, messaging, money, publishing, and computer-use actions are
   lease-scoped, approval-gated, and receipt-backed.
6. **Desktop UX must be real.** `Heiwa.app` cannot remain a developer cockpit
   forever. The runtime can stay local-first, but normal users need a polished
   app surface over the same truth.

Current local Heiwa context that must stay loaded for this work:

- `HEIWA.md` — architecture truth and three-plane model.
- `docs/product-contract.md` — product boundary.
- `docs/capability-fabric.md` — connector, lease, worker, evidence vocabulary.
- `docs/local-self-operation.md` — installed runtime and localhost verification.
- `apps/heiwa_shell/src/cmd/life.rs` — first personal read-model/freshness seam.
- `apps/heiwa_shell/src/cmd/app.rs` — installed runtime/app probe seam.
- `~/.heiwa/` runtime state — current local proof source.

Build order forced by this context:

1. Finish local read models: `life today`, `life freshness`, pending approvals,
   inbox/history/source refs.
2. Back them with receipts and source spans before adding broad connectors.
3. Put those read models in Heiwa.app.
4. Add one real connector lane end-to-end: auth, list, bounded action, receipt,
   revoke.
5. Package app/runtime cleanly through GitHub release authority, with Cloudflare
   only fronting docs/install/update metadata.

## Lessons lifted, ranked by signal

1. **MCP plane belongs in the kernel, not a plugin layer.** Goose's lead comes from treating MCP as a typed, first-class transport. Heiwa already has MCP in `crates/heiwa_provider`-adjacent code; the open question is whether `apps/heiwa_core/src/drex/router.rs` consults MCP capabilities when picking a route, or only after.
2. **Native Rust desktop packaging is plausible, but must be proved locally.** Zed and Warp show Rust-native `.dmg`-notarized apps are viable. OpenHuman shows a Rust desktop app can pair Tauri with CEF, but does not prove plain Tauri 2 WebView is enough. Pick **Tauri 2.x** for `apps/heiwa_app/clients/macos` because it fits Heiwa's Rust + Solid/Vite + local runtime spine, then verify bundle size, memory, panel performance, signing, notarization, and update flow before making maturity claims.
3. **Config-as-product matters.** Continue.dev's success is partly that `~/.continue/config.json` is legible and forkable. `~/.heiwa/config.toml` should be promoted to a public contract with examples — not buried.
4. **Plan-as-artifact is a real abstraction we don't fully materialize.** Plandex serializes plans to git branches; we have HEIWA.md plan docs but no first-class plan object in the runtime. Decide whether `heiwa plan` becomes a kernel concept post-L4.
5. **Cloud-account requirement is the trap.** Warp's biggest churn driver is "must sign in to Warp Cloud." Heiwa should make every paid feature available against local state alone, with cloud as opt-in evidence sync.
6. **Anti-pattern: power without leases.** Open Interpreter is a useful contrast — same intent surface, no policy plane. Heiwa's capability fabric (`docs/capability-fabric.md`) is the differentiator; protect it.

## 2026-05-24 Agentic Assistant Mining

Credits and source posture: these notes mine public positioning and repository
docs for product patterns. They are not claims that Heiwa has these features.

| Source                                                                    | Useful pattern                                                                                                                                                                        | Heiwa implication                                                                                                                                                         |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Clerk AI](https://clerk.ai/product)                                      | Voice, RCS, SMS, WhatsApp, warm transfer, unified inbox, cross-channel memory, and compliance are treated as one conversation workflow.                                               | Heiwa intake should normalize channel events into one typed `InboxItem` stream instead of building separate product silos for calls, messages, and email.                 |
| [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent) | Self-improving skills, FTS recall, scheduled automations, messaging gateway, multiple terminal backends, `doctor`, `update`, and provider switching are first-class runtime surfaces. | Heiwa should treat learned skills as evidence-backed procedural memory, keep gateway adapters isolated, and make `doctor`/`update` prove runtime authority before action. |
| [Sim / Ema](https://workflow.ema.ms/)                                     | Agent templates, workflow logs, audit trails, access control, BYOK, self-hosting, and 1,000+ integrations are packaged as an agent workspace.                                         | Heiwa should ship a small set of typed workflow templates only after the state spine exists; audit/event evidence must precede template sprawl.                           |

Pattern to preserve: the best peers make the agent reachable where the user
already works, but the durable differentiator is still typed memory,
authorization, evidence, and update provenance. For Heiwa, that means channel
breadth comes after the Intake/Execution/Evidence spine is deterministic.

## What Heiwa needs to ingest to keep deciding well

These are concrete data dependencies the router and operator surface need. None are speculative.

| Need                                                                          | Source                                                                  | Refresh cadence        | Consumer                                   |
| ----------------------------------------------------------------------------- | ----------------------------------------------------------------------- | ---------------------- | ------------------------------------------ |
| Provider model catalog (id, context window, modalities, prompt-cache support) | provider APIs + `models.dev` index                                      | weekly                 | `heiwa_provider::detect`, `models` command |
| Model capability table (tool use, JSON mode, vision, embeddings, audio)       | provider docs + own probe results                                       | monthly + on first run | `drex::router::plan_route`                 |
| Pricing feed (per-provider, per-model, input/output/cache rates)              | provider pricing pages, OpenRouter price index                          | weekly                 | `heiwa_quota` cost translation             |
| MCP server registry (name, transport, capability declaration)                 | upstream MCP registry + local install                                   | on-change              | router, `route preview`                    |
| Subscription quota semantics (Claude Pro, ChatGPT Plus, Google AI Pro limits) | provider docs + observed 429s                                           | on-change              | `heiwa_quota` rate-group cooldowns         |
| Eval corpus for routing decisions ("which tier should answer X")              | self-built fixtures + open eval suites (Aider polyglot, SWE-Bench Lite) | as written             | router regression tests                    |
| Local capability probe (VRAM, model availability, adapter health)             | runtime detection on launch                                             | per-launch             | `heiwa doctor`, `heiwa providers`          |

Without these, the router optimizes against vibes. With them, the doctrine ("smallest sufficient model, shortest sufficient context, richest sufficient evidence") becomes measurable.

## macOS-first Heiwa.app packaging path

Status today: `apps/heiwa_app/package.json` is `tsc --noEmit` only. `clients/macos`, `clients/windows`, `clients/iphone` are empty scaffolds. There is no native packaging.

**Recommended stack: Tauri 2.x + system WebView (WKWebView on macOS), pending local proof.**

Why not Electron: bigger binaries, higher RAM cost, larger Chromium/Node attack surface, and weaker Rust integration. Heiwa's center is Rust; a Rust shell is consistent. If plain WebView fails under real cockpit panels, CEF/Electron becomes an evidence-backed fallback, not the default.

Why not pure SwiftUI: locks Heiwa.app into Apple-only and replicates UI work for Linux/Windows. Tauri shares one webview-rendered cockpit across platforms while keeping the Rust kernel binary linked, not RPC'd over a socket.

Why not Slint or egui: no system webview means losing the existing `apps/heiwa_app/clients/cockpit` web UI investment.

### Competitive gaps to close before parity claims

| Gap               | Peer proof                                                                                   | Heiwa current truth                                                                 | First Heiwa slice                                                                               |
| ----------------- | -------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Connector breadth | OpenHuman claims 118+ integrations through Composio/OAuth; Hermes claims 40+ tools plus MCP. | Local read models, dispatch dirs, provider wrappers; no normal-user connector lane. | Pick one connector lane and make it product-grade: auth, list, bounded action, receipt, revoke. |
| Token compression | OpenHuman claims TokenJuice compression before LLM calls.                                    | No equivalent compression layer.                                                    | Add source-chunk compression and token accounting before cloud-provider escalation.             |
| Learning loop     | Hermes ships skill self-improvement, FTS5 recall, Honcho user modeling.                      | Static markdown and local state; no skill evolution loop.                           | Add procedure/skill capture with evidence refs and review gate.                                 |
| Gateway delivery  | Hermes gateway reaches Telegram, Discord, Slack, WhatsApp, Signal, Email.                    | `heiwa` CLI/app only; Mail bridge metadata probe only.                              | Normalize one external channel into `InboxItem` plus approval-gated outbound draft.             |

### Concrete file plan (when greenlit)

| Path                                           | Purpose                                                                                                |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `apps/heiwa_app/clients/macos/Cargo.toml`      | Rust crate that links `heiwa_core` directly + Tauri runtime                                            |
| `apps/heiwa_app/clients/macos/tauri.conf.json` | bundle id, signing identity, entitlements                                                              |
| `apps/heiwa_app/clients/macos/src/main.rs`     | thin: spawn cockpit webview, expose IPC commands that call `drex::router` and `heiwa_quota` in-process |
| `apps/heiwa_app/clients/macos/src/ipc.rs`      | typed Tauri commands (`route_preview`, `quota_status`, `providers_list`)                               |
| `apps/heiwa_app/clients/cockpit/`              | already exists; add Tauri-aware build target                                                           |
| `scripts/package_macos.sh`                     | `cargo tauri build` → `.dmg`, run `codesign` + `notarytool`                                            |
| `.github/workflows/release-macos.yml`          | tag-driven build, sign, notarize, attach to release (pairs with Codex `P2-release-workflow`)           |

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
