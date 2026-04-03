# Railway Self-Operation

This is the runtime contract for Heiwa Cloud HQ on Railway.

The goal is simple: a fresh Railway deployment should be able to authenticate its own control-plane CLIs, reach SpacetimeDB, and choose models that actually exist on the runtime where the work will execute.

## Required Railway variables

These must exist on the `heiwa-core` production service before Cloud HQ can self-operate fully:

| Variable | Purpose |
| --- | --- |
| `CLAUDE_OAUTH_ACCESS_TOKEN` | Claude Code headless auth bootstrap |
| `CLAUDE_OAUTH_REFRESH_TOKEN` | Claude Code refresh path |
| `CODEX_OAUTH_REFRESH_TOKEN` | Codex headless auth bootstrap |
| `CODEX_ACCOUNT_ID` | Codex account binding |
| `GEMINI_OAUTH_REFRESH_TOKEN` | Gemini CLI headless auth bootstrap |
| `GH_TOKEN` or `GITHUB_TOKEN` | GitHub CLI auth for `gh` workflows |
| `CLOUDFLARE_API_TOKEN` | Wrangler auth for Pages/Workers operations |
| `CLOUDFLARE_ACCOUNT_ID` | Cloudflare account target for deploys and API calls |
| `RAILWAY_TOKEN` | Railway CLI auth for self-managed env/service workflows |
| `SPACETIMEDB_TOKEN` or `STDB_AUTH_TOKEN` | Headless `spacetime login --token ...` bootstrap |
| `STDB_SERVER` | SpacetimeDB target nickname or host |
| `STDB_IDENTITY` | SpacetimeDB database identity |

## Boot contract

`apps/heiwa_core/Dockerfile` now installs:

- `gh`
- `@railway/cli`
- `wrangler`
- existing Claude Code, Codex, Gemini CLI, and SpacetimeDB CLIs

`apps/heiwa_core/start.sh` now does four things during boot:

1. Normalizes env-backed auth for `gh`, `railway`, `wrangler`, and `spacetime`
2. Performs headless SpacetimeDB login when a token is present
3. Verifies each CLI with a non-interactive status command
4. Emits warnings instead of silently pretending the control plane is ready

That means a Railway deploy now surfaces missing auth explicitly instead of drifting into a half-configured state.

## Model tier matrix

This is the intended routing policy for Heiwa operating itself:

| Lane | Primary | Secondary | Notes |
| --- | --- | --- | --- |
| `chat`, `status_check`, `audit` on Railway | `google-antigravity/gemini-3-flash` | `gemini-cli/gemini-3-flash` | Zero-marginal-cost routine cognition first |
| `research` on Railway | `gemini-cli/gemini-3.1-pro` | `google-antigravity/gemini-3.1-pro` | Long-context first, then second Google lane |
| `build` on Railway | `codex/gpt-4.1` | `codex/gpt-5.4`, `claude/sonnet-4-6` | Default coding lane stays Codex-first |
| `strategy` / adversarial review | `google-antigravity/gemini-3.1-pro` | `claude/opus-4-6`, `codex/gpt-5.4` | Use the second Google lane first, then premium review models |
| `review` / code review | `claude/sonnet-4-6` | `claude/opus-4-6` | Claude remains the main review lane |
| Sovereign or boost-only work | local `ollama/*` tiers | none | Local-only tiers are preserved for trusted local execution |
| Embeddings | `ollama/qwen3-embedding:0.6b` | none | Still boost/local only |

## Routing rules that matter

- Railway execution lanes must not select local-only providers from STDB tier overrides.
- Sovereign and boost/macbook lanes must not select remote providers from STDB tier overrides.
- Exact intent strengths beat `"general"` strengths during tier selection.
- `heiwa_ops` routes stay deterministic and do not receive STDB model overrides.

Those rules exist because the STDB tier table is global, but runtime availability is not. Without runtime-aware filtering, Railway can select a local Ollama tier that only exists on a boost node.

## Verification

Use these checks after the next deploy:

```bash
uv run pytest apps/heiwa_hub/tests/test_cloud_hq_start_script.py
PYTHONPATH=/Users/dmcgregsauce/heiwa-universe/packages/heiwa_cli:/Users/dmcgregsauce/heiwa-universe/packages/heiwa_cognition:/Users/dmcgregsauce/heiwa-universe/packages/heiwa_sdk:/Users/dmcgregsauce/heiwa-universe/packages/heiwa_protocol:/Users/dmcgregsauce/heiwa-universe/packages/heiwa_identity:/Users/dmcgregsauce/heiwa-universe/packages/heiwa_ui:/Users/dmcgregsauce/heiwa-universe/apps uv run pytest apps/heiwa_hub/tests/test_compute_router_stdb.py
PYTHONPATH=/Users/dmcgregsauce/heiwa-universe/packages/heiwa_cli:/Users/dmcgregsauce/heiwa-universe/packages/heiwa_cognition:/Users/dmcgregsauce/heiwa-universe/packages/heiwa_sdk:/Users/dmcgregsauce/heiwa-universe/packages/heiwa_protocol:/Users/dmcgregsauce/heiwa-universe/packages/heiwa_identity:/Users/dmcgregsauce/heiwa-universe/packages/heiwa_ui:/Users/dmcgregsauce/heiwa-universe/apps uv run pytest apps/heiwa_hub/tests/test_phase1_integration.py
railway logs --service heiwa-core
```

At runtime, the log should show authenticated status lines for `claude`, `codex`, `gemini`, `gh`, `railway`, `wrangler`, and `spacetime`.
