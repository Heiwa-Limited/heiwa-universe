# Current Capability Truth

## Supported now

- **Installed `heiwa` CLI/cockpit**: supported operator surface
- **MCP/tool registry**: supported integration surface with scoped local tools
- **Connector manifests**: validated manifest surface with negative audit coverage
- **HTTP API**: supported public-safe runtime ingress where hosted services are deployed
- **Docs and release artifacts**: supported GitHub-native publication surfaces

## Supported architecture claims

- The application is written for N users. `crates/heiwa_config::HeiwaPaths`
  (ConfigRoot) is the single resolver for per-user state, and
  `scripts/check_l0_acceptance.sh` fails on any independent home/state-root
  resolution or hardcoded operator identity in runtime code. The check greps
  source rather than proving absence by construction, so it is a guard
  against regression, not a proof.
- A user supplies one API key and the application works with no provider CLI
  installed: direct-API adapters for the Anthropic, OpenAI, and Google
  families run alongside the CLI adapters, and
  `apps/heiwa_shell/tests/fresh_install.rs` completes a turn end to end on a
  machine with no reachable provider CLI.
- Provider failure is a routing constraint: `heiwa_provider::health` reports
  which accounts are usable and why one was skipped, and a zero-provider
  install opens with actionable guidance rather than an error.
- The desktop shell is a SolidJS component layer: ten surface modules behind a
  `SurfaceModule` contract over a tokenized design system, with the operator
  stream seam (`store.ts` / `client.ts` / `types.ts`) preserved unmodified.
- The installed runtime is the current product center of gravity.
- DREX routing, provider/session/protocol crates, execution scopes, tool leases, and receipts are the live runtime spine.
- Local JSONL is the canonical evidence plane; Lance is the derived local recall index.
- GitHub Actions, Pages, and Releases are the current repo-native validation and publication path.
- Cloudflare is optional support infrastructure for public edge needs; hosted services do not define the default operator experience.
- Public status is event-first when exposed, with HTTP diagnostics as fallback.

## Not presented as complete

- Discord as a required ingress surface
- iMessage as a productized ingress surface
- broad computer-use automation
- `Heiwa.app` as a fully native desktop runtime
- first-run onboarding inside the application (roadmap L2)
- live read models behind the Calendar, Mail, Finance, and Social surfaces —
  they state their pending status honestly and land on the L3 connector plane
- the Browser surface as an actionable, approval-gated automation surface; it
  is an iframe until the L4 runtime-owned browser lands
- cross-device evidence sync or a hosted state backbone
- `heiwa-limited` as an active product target
- experimental canvases as part of the supported stack
- placeholder agent personas as productized capabilities
- full provider-normalized multi-turn tool calling across every provider
- executable connector capability truth beyond manifest validation

## Evidence rule

README, MkDocs pages, and the static web shell should all agree on this boundary. CI exists to prevent public claims from drifting ahead of verified surfaces.
