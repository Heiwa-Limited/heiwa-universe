# Local Capability Inventory — 2026-06-03

> Operator-private research artifact. Purpose: turn the current Codex, Claude,
> Gemini, plugin, skill, and peer-handoff surfaces into Heiwa-ingestable route
> and context data without copying secrets, OAuth tokens, telemetry payloads, or
> raw private transcripts.

## Source Posture

Read-only sources used:

- Codex plugin/skill inventory from current session metadata and local
  `~/.codex/skills`, `~/.agents/skills`, and `~/.codex/plugins/cache`.
- Codex app/config posture from `/Applications/Codex.app`,
  `~/.codex/config.toml` key names and redacted values, MCP server names, app
  connector posture, workspace dependency paths, and session-visible tools.
- Installed AI app bundle IDs from `/Applications/*.app` and
  `~/Applications/*.app`.
- Live official-source scan on 2026-06-03 for OpenAI Agents SDK tools,
  Anthropic Claude Code MCP, Gemini CLI extensions, Ollama docs, GitHub MCP,
  SpacetimeDB SDKs, WebAssembly 3.0, TypeScript project references, Python
  concurrency, and Node worker threads.
- Claude Code CLI metadata from `claude --version`, `claude --help`,
  `claude plugin list`, `claude agents --help`, `claude mcp --help`, and
  `claude plugins --help`.
- Claude local config posture from `~/.claude/CLAUDE.md`, settings key names,
  plugin cache paths, and installed plugin list output.
- Gemini CLI metadata from `gemini --version`, `gemini --help`,
  `gemini skills list`, `gemini mcp --help`, `gemini hooks --help`, and
  `gemini skills --help`.
- Gemini local config posture from `~/.gemini/GEMINI.md`, settings key names,
  extension manifests, and enabled skills.
- Pasted Claude handoff at
  `/Users/dmcgregsauce/.codex/attachments/829bc60e-d76b-49f5-a60b-5820535ffcea/pasted-text.txt`.

Skipped on purpose:

- `~/.gemini/oauth_creds.json`, `~/.gemini/gemini-credentials.json`,
  `~/.gemini/google_accounts.json`, Claude/Gemini telemetry payloads, raw task
  transcripts, token-bearing MCP headers, Codex `auth.json`, Codex raw
  process/chat logs, and provider-owned auth internals.

## Heiwa Ingestion Model

These surfaces should land in Heiwa as a local capability catalog, not as raw
provider control:

| Heiwa field | Meaning |
| --- | --- |
| `source_provider` | `codex`, `claude`, `gemini`, `antigravity`, `ollama`, or `heiwa` |
| `surface_type` | `plugin`, `skill`, `mcp_server`, `cli_command`, `policy`, `handoff`, `local_model` |
| `capability_id` | Stable local name such as `gemini.chrome-devtools-mcp` |
| `intake_use` | What signal it can ingest: browser, file, media, repo, issue, design, docs |
| `execution_use` | What work it can safely perform: code, review, browser debug, design, docs |
| `evidence_use` | What proof it can emit: diff, screenshot, transcript, test output, receipt |
| `risk_tier` | Heiwa T0-T3 boundary before executing through this surface |
| `secret_policy` | `no_secret`, `provider_owned_secret`, or `approval_required_secret` |
| `enabled_state` | `enabled`, `disabled`, `available`, `missing`, or `unknown` |

Source/reference packs use the same Intake/Evidence posture but are not
executable by default:

| Heiwa field | Meaning |
| --- | --- |
| `reference_source.id` | Stable source id such as `official.openai.agents-sdk.tools` |
| `authority` | `official`, `official_oss`, `official_standard`, `community_oss`, `private` |
| `source_url` | Canonical URL or local mirror path |
| `use` | What the source teaches: tool schema, SDK, runtime, model, integration, spec |
| `refresh_mode` | `web_snapshot_then_local_cache`, `git_mirror_then_manifest`, or `manual_note` |
| `risk_tier` | Always T0 until promoted into a connector/tool manifest |

Promotion rule: source packs can inform Heiwa, but only connector/tool manifests
can execute.

## Official Source Pack Direction

Live source check, 2026-06-03:

- OpenAI Agents SDK tools document hosted tools, built-in execution tools,
  function tools, agents as tools, MCP servers, sandbox capabilities, and an
  experimental Codex tool. Heiwa should map that to a generic tool taxonomy
  rather than an OpenAI-only pathway.
- OpenAI Agents SDK evolution post emphasizes a model-native harness with files,
  tools, and native sandbox execution. Heiwa should treat this as validation for
  workspace-scoped execution lanes, while keeping local runtime authority.
- Claude Code MCP docs confirm MCP can connect to external tools/data, support
  resources/tool search, and warn that external-content servers carry prompt
  injection risk. Heiwa should represent MCP servers as leased tools, not trusted
  ambient access.
- Gemini CLI extensions package prompts, MCP servers, and custom commands. Heiwa
  should ingest extension metadata as capability packs.
- Ollama docs expose local model usage plus official Python and JavaScript /
  TypeScript libraries. Heiwa should keep Ollama as the default private local
  model lane and record model capability truth per installed model.
- GitHub's official MCP server supports remote and local modes, with OAuth/PAT
  host differences. Heiwa should treat GitHub as both a repo/reference source
  and a write-capable integration behind explicit scopes.
- SpacetimeDB SDK docs expose Rust/TypeScript/C#/Unreal client SDKs,
  subscriptions, local caches, callbacks, and reducers. Heiwa should keep STDB
  as sync/adjudication/evidence, not local side-effect executor.
- WebAssembly 3.0 describes safe, portable, sandboxed execution with no ambient
  environment access unless the embedder imports capabilities. Heiwa should use
  this model for future plugin sandboxes.
- TypeScript project references support smaller projects, faster builds, and
  logical separation. Heiwa.app and connector clients should use this modular
  shape instead of one large frontend package.
- Python 3.14 concurrency docs add `InterpreterPoolExecutor` for isolated
  interpreters and true multi-core parallelism. Python remains compatibility/R&D
  unless isolated behind worker contracts.
- Node worker threads support resource limits and parallel JavaScript execution.
  TypeScript workers can serve CPU-bound client-side or local-service tasks, but
  resource limits must be explicit.

Heiwa implication:

- The capability fabric needs `reference_sources`, `integration_families`,
  `runtime_targets`, and `performance_targets` as first-class catalog fields.
- Runtime APIs should expose bounded counts and pointers; detailed source
  mirrors stay local and redacted.
- Microsecond responses should target cached read models, routing metadata, and
  local status views. Provider calls, model inference, GUI automation, and web
  fetches are asynchronous work with receipts.

## Codex Capability Surface

Observed app and runtime surfaces:

- Installed app: `/Applications/Codex.app`, bundle id `com.openai.codex`.
- CLI path observed earlier in this session:
  `/Users/dmcgregsauce/.npm-global/bin/codex`. `codex --help` and
  `codex --version` produced no useful output in this harness, so CLI command
  metadata is not yet a strong source.
- Local config posture from `~/.codex/config.toml`: current model
  `gpt-5.5`, reasoning effort `xhigh`, approval policy `never`, sandbox
  `danger-full-access`, personality `pragmatic`.
- Enabled config features include multi-agent, apps, memories,
  prevent-idle-sleep, and desktop wake posture.
- Redacted MCP server names: `playwright`, `figma`, `notion`, `railway`,
  `git`, and `node_repl`. Disabled MCP entries observed: `MCP_DOCKER` and
  `codebase-retrieval`.
- Workspace dependency runtime exposed by Codex.app: bundled Node, Python,
  native binaries, and document/spreadsheet/slide/PDF libraries.
- Installed adjacent app surfaces: `ChatGPT.app` (`com.openai.chat`) and
  `ChatGPT Atlas.app` (`com.openai.atlas`).

Current Codex environment exposes plugin categories useful to Heiwa:

- **Runtime/app control:** Browser, Chrome, Computer Use.
- **Build lanes:** Build Web Apps, Build macOS Apps, Game Studio, Cloudflare,
  OpenAI Developers.
- **Knowledge/doc lanes:** Documents, Presentations, Spreadsheets, PDF,
  Notion, Google Drive, Gmail.
- **Code/repo lanes:** GitHub, github, Codex Security, Figma, Hugging Face,
  plugin-dev.
- **Workflow lanes:** Superpowers, local Heiwa skills, screenshots, speech,
  transcribe, deployment skills.

Codex local skill roots include:

- `~/.codex/skills/*` for Heiwa, deploy, docs, frontend, security, Figma,
  GitHub, Notion, media, and utility skills.
- `~/.agents/skills/heiwa-concise-mode/SKILL.md` as provider-agnostic concise
  behavior.
- OpenAI bundled/curated plugin caches for browser, Chrome, Figma, GitHub,
  Gmail, Google Drive, Hugging Face, Notion, OpenAI Developers, documents,
  presentations, spreadsheets, Cloudflare, and Superpowers.

Heiwa implication:

- Codex should be modeled as a strong implementation/review worker with rich
  local tool access, app connectors, MCP servers, and a desktop app surface, not
  as the sole orchestrator.
- Heiwa should ingest Codex.app/config metadata directly into a local
  capability catalog. It should not ingest raw `auth.json`, task transcripts,
  process-manager payloads, or token-bearing MCP headers.
- Skills are route hints: Heiwa should attach only task-relevant skill context
  to avoid context bloat.
- Codex.app is a Heiwa ecosystem participant: useful for background delegated
  code/docs/app work and tool access, while Codex still owns its own harness
  policy, prompts, auth, and model inventory.

## Installed App Surfaces

Observed local app bundles:

| App | Bundle id | Heiwa use |
| --- | --- | --- |
| `Codex.app` | `com.openai.codex` | Codex app capability, tool, MCP, and session surface |
| `Claude.app` | `com.anthropic.claudefordesktop` | Claude desktop capability and handoff surface |
| `Gemini.app` | `com.google.GeminiMacOS` | Gemini desktop capability and handoff surface |
| `Antigravity.app` | `com.google.antigravity` | Antigravity desktop capability surface; CLI currently missing |
| `ChatGPT.app` | `com.openai.chat` | ChatGPT desktop capability surface |
| `ChatGPT Atlas.app` | `com.openai.atlas` | Browser/app capability surface |
| `Claude Code URL Handler.app` | `com.anthropic.claude-code-url-handler` | Claude Code URL handoff surface |

Heiwa implication:

- These apps should become local inventory records and controlled handoff
  targets before Heiwa attempts GUI automation.
- GUI/app scraping should use redacted metadata first. Accessibility,
  AppleScript, URL handlers, browser profiles, or computer-use control become
  approval-staged execution lanes when they can mutate state or expose private
  content.

## Claude Capability Surface

Observed:

- Version: `2.1.142 (Claude Code)`.
- Non-interactive output: `claude -p/--print`.
- Structured output: `--output-format json|stream-json`, `--json-schema`.
- Session controls: `--resume`, `--continue`, `--session-id`, `--fork-session`,
  `--worktree`, `--tmux`.
- Agent controls: `claude agents`, `--agent`, `--agents <json>`, background
  sessions, model/effort/permission defaults.
- Tool controls: `--tools`, `--allowedTools`, `--disallowedTools`,
  `--permission-mode`.
- MCP controls: `claude mcp add/list/get/remove/serve`.
- Plugin controls: `claude plugins list/details/enable/disable/install/update`.
- Extra surfaces: Chrome integration, ultrareview, auto-mode, plugin dirs,
  remote-control.

Enabled Claude plugin signal from `claude plugin list`:

- Development/review: `agent-sdk-dev`, `code-review`, `code-simplifier`,
  `feature-dev`, `pr-review-toolkit` disabled, `ultrareview` command available.
- Repo/platform: `github`, `commit-commands`.
- Frontend/design: `frontend-design`.
- Communication: `discord`.
- Security: `security-guidance`.
- Language servers: `pyright-lsp`, `rust-analyzer-lsp`, `typescript-lsp`.
- Workflow/context: `superpowers`, `claude-md-management`, `hookify`,
  `skill-creator`, `plugin-dev`.
- Disabled or non-core: `claude-code-setup`, `greptile`, `huggingface-skills`,
  `ralph-loop`, `zoominfo`.

Claude local config signal:

- `~/.claude/CLAUDE.md` carries Heiwa concise context.
- Settings key names include `effortLevel`, `enabledPlugins`, `hooks`, `model`,
  `permissions`, `remoteControlAtStartup`, and dangerous-mode prompt posture.

Heiwa implication:

- Claude is strong for background review, language-server-aware code tasks,
  plugin-backed repo work, Discord-integrated workflows, and multi-agent review.
- Heiwa should ingest Claude plugin/agent state as worker capability metadata.
- Do not ingest Claude settings values blindly; key names are enough unless a
  redacted config parser is built.

## Gemini Capability Surface

Observed:

- Version: `0.38.2`.
- Non-interactive output: `gemini -p/--prompt`.
- Structured output: `--output-format text|json|stream-json`.
- Execution controls: `--worktree`, `--sandbox`, `--approval-mode`,
  `--policy`, `--admin-policy`, `--allowed-mcp-server-names`.
- Capability managers: `gemini mcp`, `gemini extensions`, `gemini skills`,
  `gemini hooks`.
- Session controls: `--resume`, `--list-sessions`, `--delete-session`.

Enabled Gemini extensions:

- `chrome-devtools-mcp` with MCP server `chrome-devtools`.
- `elevenlabs` with MCP server `ElevenLabs` for TTS, voice design,
  conversational AI, music, sound effects, and audio processing.
- `github` with hosted MCP endpoint configured through `$GITHUB_MCP_PAT`.
- `heiwa-concise-mode`.
- `superpowers`.
- `terraform` with `terraform-mcp-server` via Docker/env-gated credentials.

Enabled Gemini skills:

- Browser/debug: `chrome-devtools`, `chrome-devtools-cli`, `a11y-debugging`,
  `debug-optimize-lcp`, `memory-leak-debugging`, `troubleshooting`.
- Workflow: Superpowers skills including brainstorming, executing-plans,
  TDD, systematic-debugging, verification, subagent-driven-development,
  dispatching-parallel-agents, worktrees, code review, and writing-plans.
- Heiwa behavior: `heiwa-concise-mode`.
- Media scrape: `supadata-media-scraper`.

Gemini local config signal:

- `~/.gemini/GEMINI.md` carries Heiwa autonomy boundary, life constraints,
  provider truth, routing priorities, and project landscape.
- Settings key names include `agents`, `context`, `hooks`, `model`,
  `security`, `tools`, `ui`, `advanced`, and `experimental`.

Heiwa implication:

- Gemini is strong for browser debugging, policy-governed CLI work, MCP
  extension management, media scraping, and Google-rate-group execution.
- Its config already carries the richest Heiwa personal context; Heiwa should
  treat it as a local context source, redacted before display.

## Pasted Claude Handoff Data

Useful data extracted from the handoff:

- Claude built a hash-chained `heiwa_receipts` ledger in the main checkout,
  including migration v1 to v2, `head_hash()`, `verify_chain()`, and tests.
- `heiwa_receipts` was reported as untracked and initially not present in the
  isolated worktree.
- Verification claimed by Claude: `cargo test -p heiwa_receipts` with 9 unit
  and 3 smoke tests, plus clippy and whitespace checks.
- Open call: decide whether to commit `heiwa_receipts` plus workspace
  membership.
- Critical finding against Codex resource slice: macOS raw `vm_stat`
  free/inactive/speculative under-reported memory and falsely denied heavy local
  model admission. Fix target: prefer macOS memory-pressure-derived availability.

Heiwa implication:

- Peer handoffs should become first-class `InboxItem` or `AgentStatus` events
  with source path, claims, verification, blockers, and requested decision.
- Capability catalog should include peer-handoff artifacts as `surface_type =
  handoff` so Heiwa can route follow-up work to the right provider/branch.

## Immediate Build Order

1. Fix macOS resource availability source so local-model admission does not
   false-deny under normal memory pressure.
2. Keep this capability inventory as the seed for a future
   `/api/v1/capabilities` read model.
3. Inspect and decide on `heiwa_receipts` workspace membership before building
   new evidence features that depend on receipts.
4. Build local agent bus after provider resolution + resource policy are both
   trustworthy.
