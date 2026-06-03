# Heiwa Capability Ecosystem Handoff - 2026-06-03

Purpose: hand off the current Heiwa capability-ecosystem work to another agent
or future memory update without requiring the chat transcript.

## Current Branch And Commits

- Worktree: `/Users/dmcgregsauce/heiwa-universe/.worktrees/private-server-provider-resolution`
- Branch: `codex/private-server-provider-resolution`
- Latest commits:
  - `f52cf26 feat: model capability source packs`
  - `ed9aed8 feat: surface local app capability counts`
  - `198f0c6 feat: expose local capability catalogs`
  - `e809375 docs: capture local provider capability inventory`
  - `5557ce5 fix: prefer macOS memory pressure for local resource admission`
  - `b12223b feat: expose local resource admission state`
  - `6e1266a feat: add local resource admission policy`
  - `f5ce5ec fix: resolve provider CLIs for local runtime`

## User Correction To Preserve

Devon corrected the product framing: Heiwa is not mainly a GitHub repo,
dashboard, hosted product, or one-provider wrapper. It should be a local-first
MacBook operating layer that wraps provider apps, provider CLIs, local models,
files, accounts, agents, and approved integrations into one governed ecosystem.

Heiwa should:

- use local models first when hardware/resource policy allows
- treat Codex.app, Claude.app, Gemini.app, Antigravity, Ollama, and future
  provider surfaces as ecosystem participants
- ingest redacted capability metadata from apps/configs/plugins/MCP/skills
- keep raw auth, private transcripts, token-bearing headers, and provider
  internals provider-owned
- route work through Intake, Execution, and Evidence
- expose fast local read models for capability/routing/status metadata
- make source/reference ingestion modular enough for official docs and OSS repos
- promote references to executable adapters only through manifests, tests,
  scopes, leases, approvals, receipts, and revocation posture

## Repo Files Touched

- `apps/heiwa_shell/src/cmd/app.rs`
  - `/api/v1/capabilities` reads JSON catalogs from
    `~/.heiwa/state/capabilities`.
  - It now exposes bounded counts for:
    - `providers`
    - `gemini_extensions`
    - `codex_plugins_observed`
    - `codex_mcp_servers`
    - `claude_plugins_observed`
    - `gemini_skills_observed`
    - `installed_apps_observed`
    - `peer_handoff_findings`
    - `reference_sources`
    - `integration_families`
    - `runtime_targets`
    - `performance_targets`
  - Test fixture: `capability_catalogs_read_sanitized_local_state`.

- `docs/capability-fabric.md`
  - Added `References` as a first-class capability material.
  - Added `Source Pack Contract`.
  - Added source-pack promotion path:
    1. reference pack
    2. capability manifest
    3. adapter or connector
    4. tool lease
    5. product-grade integration
  - Added `Runtime Modularity Targets`:
    Rust authority layer, TypeScript client contracts, Shell bootstrap,
    Python compatibility workers, SpacetimeDB reducers/clients, WASM plugin
    sandbox, Ollama/local model lane, provider-owned agent runtimes.

- `docs/research/local-capability-inventory-2026-06-03.md`
  - Captures local Codex, Claude, Gemini, plugin, skill, app-bundle, MCP, and
    handoff observations.
  - Added official source pack direction from live source checks.
  - States that source packs are read-only T0 until promoted into
    connector/tool manifests.

## Local State Outside Git

Catalog:

- `/Users/dmcgregsauce/.heiwa/state/capabilities/local-capability-inventory-2026-06-03.json`

Current counts in that catalog:

- schema: `heiwa_local_capability_inventory_v1`
- generated_at: `2026-06-03T00:00:00-07:00`
- providers: `3`
- Codex plugins: `32`
- Codex MCP servers: `6`
- installed AI apps: `7`
- Claude plugins observed: `18`
- Gemini extensions: `6`
- Gemini skills observed: `22`
- reference sources: `11`
- integration families: `10`
- runtime targets: `8`
- performance targets: `5`

Installed AI app bundles observed:

- `/Applications/Codex.app` - `com.openai.codex`
- `/Applications/Claude.app` - `com.anthropic.claudefordesktop`
- `/Applications/Gemini.app` - `com.google.GeminiMacOS`
- `/Applications/Antigravity.app` - `com.google.antigravity`
- `/Applications/ChatGPT.app` - `com.openai.chat`
- `/Applications/ChatGPT Atlas.app` - `com.openai.atlas`
- `/Users/dmcgregsauce/Applications/Claude Code URL Handler.app` -
  `com.anthropic.claude-code-url-handler`

Codex config metadata observed, redacted:

- `~/.codex/config.toml` has keys for model, reasoning effort, personality,
  approval policy, sandbox mode, MCP servers, projects, features, plugins,
  marketplaces, apps, desktop, and hooks.
- Observed values included model `gpt-5.5`, reasoning effort `xhigh`,
  sandbox `danger-full-access`, approval policy `never`.
- Raw `~/.codex/auth.json`, process logs, transcripts, and token-bearing headers
  were intentionally not ingested.

## Official Sources Checked

Use these as source-pack seeds, not execution authority:

- OpenAI Agents SDK tools:
  `https://openai.github.io/openai-agents-js/guides/tools/`
  - Tool taxonomy, hosted tools, MCP, shell/computer concepts, tool search.
- OpenAI Agents SDK evolution:
  `https://openai.com/index/the-next-evolution-of-the-agents-sdk/`
  - Model-native harness, files, tools, native sandbox reference.
- Claude Code MCP:
  `https://code.claude.com/docs/en/mcp`
  - MCP connections, resources/tool search, server trust warnings.
- Gemini CLI extensions:
  `https://google-gemini.github.io/gemini-cli/docs/extensions/`
  - Extensions can package prompts, MCP servers, custom commands, and release
    metadata.
- Ollama docs:
  `https://docs.ollama.com/index`
  - Local model docs plus official Python and JavaScript/TypeScript libraries.
- GitHub MCP server:
  `https://github.com/github/github-mcp-server`
  - Official GitHub MCP server, remote/local modes, OAuth/PAT posture.
- SpacetimeDB SDKs:
  `https://spacetimedb.com/docs/1.12.0/sdks/`
  - Rust/TypeScript/C#/Unreal client SDKs, subscriptions, callbacks, reducers.
- WebAssembly 3.0:
  `https://webassembly.github.io/spec/core/intro/introduction.html`
  - Portable sandboxed code model; capabilities imported by embedder.
- TypeScript project references:
  `https://www.typescriptlang.org/docs/handbook/project-references`
  - Modular TypeScript project/build boundaries.
- Python `concurrent.futures`:
  `https://docs.python.org/3.14/library/concurrent.futures.html`
  - Worker pools and interpreter isolation references.
- Node worker threads:
  `https://nodejs.org/api/worker_threads.html`
  - Resource-limited JS worker execution.

## Runtime Proof Already Run

Commands run on the checkout worktree:

```bash
cargo test -p heiwa-shell
git diff --check
cargo run -q -p heiwa-shell --bin heiwa -- app runtime status --json
cargo run -q -p heiwa-shell --bin heiwa -- app update --source checkout --dry-run
cargo run -q -p heiwa-shell --bin heiwa -- app start --port 7475 --no-open
curl -fsS http://127.0.0.1:7475/status/health
curl -fsS http://127.0.0.1:7475/api/v1/capabilities
curl -fsS http://127.0.0.1:7475/api/v1/runtime/snapshot
```

Observed live endpoint proof:

- `/api/v1/capabilities` on temp checkout runtime returned:
  - `reference_sources: 11`
  - `integration_families: 10`
  - `runtime_targets: 8`
  - `performance_targets: 5`
  - `codex_plugins_observed: 32`
  - `codex_mcp_servers: 6`
  - `installed_apps_observed: 7`
- `/api/v1/runtime/snapshot` showed providers connected for Ollama, Gemini CLI,
  Claude Code, and Codex. Antigravity remained discovered but CLI-unlinked
  (`last_error: not_installed`).
- Resource policy allowed `local_model_large` using
  `macos_memory_pressure_free_percentage`.
- Temp runtime on `7475` was stopped and port verified closed.

## Open Work

Next best executable slice:

1. Add `heiwa capabilities refresh`.
2. Refresh local provider/app/source-pack metadata from redacted local sources
   and official URLs or ignored Git mirrors.
3. Write a local evidence receipt for the refresh.
4. Keep runtime API bounded: counts, source refs, and evidence pointers by
   default, not raw mirrored documents.
5. Add tests proving invalid catalogs are ignored and sensitive paths are not
   surfaced.

Potential later slices:

- Promote `reference_sources` into a typed Rust struct instead of ad hoc JSON.
- Add source-pack freshness and evidence refs to `/api/v1/capabilities`.
- Wire capability cards into Heiwa.app.
- Add local app/CLI scraper modules for Codex, Claude, Gemini, Antigravity, and
  Ollama.
- Decide whether to ingest Claude-built `heiwa_receipts` from the main checkout
  into this branch before receipt-dependent capability refresh work.

## Memory Update Candidates

- Heiwa should model official docs, OSS repos, SDKs, specs, model cards, and
  examples as source packs.
- Source packs are read-only Intake/Evidence material until promoted through
  manifest, adapter, tool lease, and product-grade integration gates.
- Capability inventory currently lives in
  `~/.heiwa/state/capabilities/local-capability-inventory-2026-06-03.json` and
  is exposed through `/api/v1/capabilities` as bounded counts.
- Current source-pack counts are `11` reference sources, `10` integration
  families, `8` runtime targets, and `5` performance targets.
- Heiwa should target microsecond-class responses for cached local read models
  and routing metadata, while provider/model/GUI/network work remains async,
  leased, observable, and resource-gated.
