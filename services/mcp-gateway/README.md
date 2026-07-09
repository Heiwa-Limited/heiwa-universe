# Heiwa MCP Gateway

Expose **Heiwa** as a connector for:

| Client | How |
|--------|-----|
| **Grok Build** (this CLI) | stdio MCP in `~/.grok/config.toml` |
| **Grok.com** custom connector | streamable HTTP MCP URL (tunnel if local) |
| **Claude Code** | `.mcp.json` / Claude settings stdio or HTTP |
| **Codex** | MCP server config |
| **Any OpenAI-compatible API client** | `POST /v1/chat/completions` |

## Install (WSL)

```bash
cd ~/heiwa   # or copy this package into services/mcp-gateway
# if using staged copy:
cd /mnt/c/Users/devon/bin/heiwa-mcp-gateway
uv pip install -e .
# or:
pip install -e .
```

## Auth / keys

Create `~/heiwa/.env` (never commit):

```bash
# Recommended single key for multi-model routing
OPENROUTER_API_KEY=sk-or-...

# Optional direct providers
OPENAI_API_KEY=
XAI_API_KEY=
ANTHROPIC_API_KEY=
GEMINI_API_KEY=
GROQ_API_KEY=
CEREBRAS_API_KEY=

HEIWA_DEFAULT_PROVIDER=auto
HEIWA_DEFAULT_MODEL=
HEIWA_OLLAMA_URL=http://127.0.0.1:11434
HEIWA_MCP_TOKEN=          # optional bearer for HTTP mode
```

Claude / Codex **subscriptions** are detected from Windows auth stores and CLI binaries; full chat via those paths needs `claude` / `codex` on PATH.

## Run

```bash
# stdio MCP
python -m heiwa_mcp_gateway

# HTTP: MCP + OpenAI proxy
python -m heiwa_mcp_gateway --http --host 127.0.0.1 --port 8742
```

## Grok Build

```toml
# ~/.grok/config.toml
[mcp_servers.heiwa]
command = "wsl"
args = ["-d", "Ubuntu", "--", "bash", "-lc", "cd /mnt/c/Users/devon/bin/heiwa-mcp-gateway && python -m heiwa_mcp_gateway"]
enabled = true
```

## Grok.com custom connector

1. Start HTTP: `python -m heiwa_mcp_gateway --http --port 8742 --token YOUR_SECRET`
2. Tunnel: `cloudflared tunnel --url http://127.0.0.1:8742` (or ngrok)
3. Add Custom MCP at https://grok.com/connectors → URL `https://<tunnel>/mcp` + bearer token

## OpenAI-compatible clients

```bash
export OPENAI_BASE_URL=http://127.0.0.1:8742/v1
export OPENAI_API_KEY=YOUR_HEIWA_MCP_TOKEN_OR_any
# model: openrouter/anthropic/claude-sonnet-4  or  ollama/qwen3.5:4b
```

## Tools

- `heiwa_status` — node health
- `list_providers_tool` — routes
- `chat` — routed completion
- `route_plan` — dry-run route pick
- `heiwa_repo_info` — git checkout summary
- `heiwa_cli` — shell out to `heiwa` binary
