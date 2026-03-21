# Provider Registry

## Public/runtime providers

| Role | Provider | Status | Notes |
|:-----|:---------|:-------|:------|
| Hub runtime | Railway | Active | `heiwa-cloud-hq` for HTTP API, MCP, health/status endpoints |
| Trading runtime | Railway | Active | `heiwa-trading` for `trade.heiwa.ltd` cockpit/service |
| State layer | SpacetimeDB maincloud | Active target | External authoritative state ledger on `maincloud.spacetimedb.com` |
| Public marketing | Cloudflare Pages | Active | `heiwa.ltd` shell |
| Public docs | Cloudflare Pages | Active | `docs.heiwa.ltd` from MkDocs Material |
| Source control / CI | GitHub | Active | Repo, pull requests, Actions |

## Public-safe posture

- Cloudflare Pages should serve only marketing, docs, and read-only status views.
- Railway remains the live runtime provider, but hub and trading should stay split by hostname and service ownership.
- New providers should not be added to the public story until they are verified and necessary.
