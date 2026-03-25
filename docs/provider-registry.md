# Provider Registry

## Public/runtime providers

| Role | Provider | Status | Notes |
|:-----|:---------|:-------|:------|
| Hub runtime | Railway | Active | `heiwa-cloud-hq` for HTTP API, MCP, health/status endpoints |
| Internal vertical runtime | Railway | Internal preview | `heiwa-trading` stays isolated from the public host map until it graduates into a first-class surface |
| State layer | SpacetimeDB maincloud | Active target | External authoritative state ledger on `maincloud.spacetimedb.com` |
| Public marketing | Cloudflare Pages | Active | `heiwa.ltd` shell |
| Product shell | Cloudflare Pages | Active | `app.heiwa.ltd` authenticated dashboard shell |
| Public status | Cloudflare Pages | Active | `status.heiwa.ltd` read-only shell |
| Public docs | Cloudflare Pages | Active | `docs.heiwa.ltd` from MkDocs Material |
| Source control / CI | GitHub | Active | Repo, pull requests, Actions |

## Public-safe posture

- Cloudflare Pages should serve marketing, the authenticated app shell, docs, and read-only status views while deferring privileged decisions to the hub API.
- `app.heiwa.ltd` stays the canonical user home even when internal runtimes remain split by Railway service ownership.
- Railway remains the live runtime provider, but internal preview services should stay off the public domain story until they are secure, supported product surfaces.
- New providers should not be added to the public story until they are verified and necessary.
