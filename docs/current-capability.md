# Current Capability Truth

## Supported now

- **CLI**: supported operator surface
- **MCP**: supported integration surface
- **Rust workspace**: supported implementation surface
- **Docs**: supported public documentation surface

## Supported architecture claims

- The installed `heiwa` runtime is the current product center.
- SpacetimeDB is the intended authoritative state layer where the current runtime still depends on it.
- GitHub Actions and GitHub Pages are the active repo-native distribution surfaces.
- Public claims should stay behind verified local/runtime and docs build checks.

## Not presented as complete

- Discord as a required ingress surface
- `heiwa-limited` as an active product target
- experimental canvases as part of the supported stack
- placeholder agent personas as productized capabilities

## Evidence rule

README, MkDocs pages, and the static web shell should all agree on this boundary. CI exists to prevent public claims from drifting ahead of verified surfaces.
