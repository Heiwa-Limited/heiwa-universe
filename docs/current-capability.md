# Current Capability Truth

## Supported now

- **CLI**: supported operator surface through the installed `heiwa` runtime
- **Provider discovery/adapters**: real but uneven across local, OAuth CLI, and API-key modes
- **MCP/tool contracts**: supported integration direction with explicit lease boundaries
- **Static public shell/docs**: supported public-safe presentation surface
- **Rust runtime substrate**: active product path for routing, sessions, providers, and loops

## Supported architecture claims

- GitHub is the source, CI, release, and public repo front-page layer.
- Cloudflare is the target public edge for marketing, docs, static clients, status, and later Workers.
- SpacetimeDB Maincloud is the intended authoritative state, reducer, subscription, and evidence layer.
- Local `heiwa` runtimes own local side effects, provider subprocesses, local secrets, and local model calls.
- No hosted Rust service tier is required for the v0.1 topology; the installed runtime owns the inference/shell hot path.

## Not presented as complete

- Discord as a required ingress surface
- iMessage as a productized ingress surface
- broad computer-use automation
- `Heiwa.app` as a fully native desktop runtime
- a Cloudflare/STDB/GitHub backbone with local runtime hot-path execution
- `heiwa-limited` as an active product target
- experimental canvases as part of the supported stack
- placeholder agent personas as productized capabilities

## Evidence rule

README, MkDocs pages, and the static web shell should all agree on this boundary. CI exists to prevent public claims from drifting ahead of verified surfaces.
