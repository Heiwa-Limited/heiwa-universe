# Heiwa.ltd Domain Strategy

`heiwa.ltd` is the public shell for the Heiwa stack. The public-first surfaces are marketing, docs, and read-only status. The control plane stays on Railway.

## 1. Root + marketing (`heiwa.ltd`)

- **Host**: Cloudflare Pages
- **Purpose**: public landing page and product positioning
- **Content**: supported surfaces, hosting model, and public-safe architecture summary

## 2. Public status (`status.heiwa.ltd`)

- **Host**: Cloudflare Pages
- **Runtime source**: Railway API + WebSocket status mirror
- **Purpose**: read-only health and status checks
- **Transport**: WebSocket-first with HTTP fallback for diagnostics

## 3. Runtime API + MCP (`api.heiwa.ltd`)

- **Host**: Railway behind Cloudflare proxy/WAF
- **Purpose**: public-safe HTTP API, MCP surface, and runtime health
- **Shape**: `/health`, `/status`, `/tools`, `/call/{tool_name}`, WebSocket status/events

## 4. Documentation (`docs.heiwa.ltd`)

- **Host**: Cloudflare Pages
- **Source**: MkDocs Material from the canonical repo docs
- **Purpose**: architecture, deployment, security, and operator guidance

## Planned, not first-class

- `auth.heiwa.ltd` may exist later, but it is not part of the supported v1 surface.
- Discord is not treated as a required public entry point in the domain plan.

## Next steps

1. Keep the public shell on Cloudflare Pages.
2. Keep the runtime on Railway with SpacetimeDB as the state layer.
3. Prefer WebSocket-backed public status/event views over poll-heavy status pages.
