# Provider Registry

## Public/runtime providers

| Role | Provider | Status | Notes |
|:-----|:---------|:-------|:------|
| Runtime host | Railway | Active | Hub runtime, HTTP API, MCP, health/status endpoints |
| State layer | SpacetimeDB | Active target | Fast state and real-time sync direction |
| Public marketing | Cloudflare Pages | Active | `heiwa.ltd` shell |
| Public docs | Cloudflare Pages | Active | `docs.heiwa.ltd` from MkDocs Material |
| Source control / CI | GitHub | Active | Repo, pull requests, Actions |

## Public-safe posture

- Cloudflare Pages should serve only marketing, docs, and read-only status views.
- Railway remains the only public runtime ingress described in this doc set.
- New providers should not be added to the public story until they are verified and necessary.
