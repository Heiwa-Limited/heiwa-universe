# Local Self-Operation

This is the runtime contract for Heiwa on Devon's MacBook.

The goal is simple: the installed `heiwa` runtime should authenticate provider
CLIs through their owner-managed configs, read/write local state under
`~/.heiwa`, expose the cockpit on localhost, and sync evidence to SpacetimeDB
only when that path is configured.

## Required Local Inputs

| Input | Purpose |
| --- | --- |
| `~/.heiwa/config.toml` | Runtime configuration |
| `~/.heiwa/accounts.json` | Provider/account registry |
| `~/.heiwa/state/` | Local runtime state, approvals, worker heartbeats |
| `~/.claude/`, `~/.codex/`, `~/.gemini/` | Provider-owned auth and hook posture |
| `STDB_TOKEN` | Optional SpacetimeDB sync/adjudication auth |
| `CLOUDFLARE_API_TOKEN` | Optional edge work only; not needed for local user functionality |

## Boot Contract

`heiwa app start --port 7474` must:

1. Serve the cockpit and local API on `127.0.0.1`.
2. Report health at `/status/health`.
3. Write local app worker heartbeats under `~/.heiwa/state`.
4. Report provider, route, approval, worker, and hook posture without mutating provider-owned configs.
5. Keep running without public DNS, Cloudflare auth, or SpacetimeDB connectivity.

## Model Tier Matrix

| Lane | Primary | Secondary | Notes |
| --- | --- | --- | --- |
| Routine chat/status/audit | `ollama/*` where sufficient | Gemini CLI / Antigravity | Cheapest acceptable route first |
| Build/code | Codex CLI | Claude Code, Gemini CLI, Ollama coding model | Provider CLIs own their auth and quota semantics |
| Research/long context | Gemini CLI | Antigravity, Claude Code | Escalate only when local context is insufficient |
| Review/strategy | Claude Code / Gemini | Codex | Use premium lanes intentionally |
| Sovereign work | local `ollama/*` tiers | none | Local-only providers only |
| Embeddings | `ollama/qwen3-embedding:0.6b` | none | Local runtime default |

## Verification

```bash
heiwa app update --dry-run
heiwa app runtime status --json
heiwa providers
curl -fsS http://127.0.0.1:7474/status/health
```

The runtime is not ready for public access until the localhost checks pass and
Cloudflare is explicitly re-enabled with fresh targets.
