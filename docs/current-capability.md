# Current Capability Truth

## Supported now

- **Installed `heiwa` CLI/cockpit**: supported operator surface
- **MCP/tool registry**: supported integration surface with scoped local tools
- **Connector manifests**: validated manifest surface with negative audit coverage
- **HTTP API**: supported public-safe runtime ingress where hosted services are deployed
- **Docs and release artifacts**: supported GitHub-native publication surfaces

## Supported architecture claims

- The installed runtime is the current product center of gravity.
- DREX routing, provider/session/protocol crates, execution scopes, tool leases, and receipts are the live runtime spine.
- SpacetimeDB is the backend adjudication, subscription, and evidence plane.
- GitHub Actions, Pages, and Releases are the current repo-native validation and publication path.
- Cloudflare is optional support infrastructure for public edge needs; hosted services do not define the default operator experience.
- Public status is event-first when exposed, with HTTP diagnostics as fallback.

## Not presented as complete

- Discord as a required ingress surface
- iMessage as a productized ingress surface
- broad computer-use automation
- `Heiwa.app` as a fully native desktop runtime
- a Cloudflare/STDB/GitHub backbone with local runtime hot-path execution
- `heiwa-limited` as an active product target
- experimental canvases as part of the supported stack
- placeholder agent personas as productized capabilities
- full provider-normalized multi-turn tool calling across every provider
- executable connector capability truth beyond manifest validation

## Evidence rule

README, MkDocs pages, and the static web shell should all agree on this boundary. CI exists to prevent public claims from drifting ahead of verified surfaces.
