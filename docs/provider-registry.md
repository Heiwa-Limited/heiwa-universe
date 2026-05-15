# Provider Registry

## Public/runtime providers

| Role | Provider | Status | Notes |
|:-----|:---------|:-------|:------|
| Source control / CI | GitHub | Active | Repo, pull requests, Actions |
| Release distribution | GitHub | Active target | Releases, install artifacts, public repo front page |
| Public marketing | Cloudflare Pages | Active target | `heiwa.ltd` shell |
| Companion client | Cloudflare Pages | Active target | `app.heiwa.ltd` safe client shell, not a privileged control plane |
| Public status | Cloudflare Pages | Active target | `status.heiwa.ltd` read-only shell |
| Public docs | Cloudflare Pages | Active target | `docs.heiwa.ltd` from MkDocs Material |
| State/evidence layer | SpacetimeDB Maincloud | Active target | Authoritative state ledger on `maincloud.spacetimedb.com` |
| Local runtime | User device | Active | Installed `heiwa` owns local side effects, provider subprocesses, local secrets, and local models |
| External account connectors | Apple / Google / Microsoft / GitHub | Target | OAuth/API/local-bridge capability lanes with explicit scopes, leases, and revocation |
| Hosted Rust runtime | None in v0.1 | Deferred | No cloud Rust service tier sits in the inference/shell hot path |
| Internal vertical runtime | Local/private preview | Internal preview | `heiwa-trading` stays isolated until it graduates into a first-class surface |

## Public-safe posture

- Cloudflare Pages should serve marketing, the authenticated app shell, docs, and read-only status views while deferring privileged decisions to the runtime/API boundary and SpacetimeDB reducers.
- `app.heiwa.ltd` is a safe companion client over runtime/STDB-backed state, not a second privileged control plane.
- External account providers are product-grade only after auth, resource listing, bounded action execution, evidence, and revocation are implemented.
- Hosted Rust services are deferred until a later control-plane stage proves they are needed.
- New providers should not be added to the public story until they are verified and necessary.
