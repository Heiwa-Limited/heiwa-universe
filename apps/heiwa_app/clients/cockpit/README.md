# Heiwa Cockpit

Local operator cockpit. SPA served on `localhost` by the installed `heiwa` runtime.

- **Framework:** Vite + Solid + TypeScript
- **Served by:** `heiwa app` → `heiwa_core` HTTP server (default `:8787`)
- **Not served by:** `heiwa.ltd` or any hosted Heiwa surface

## Dev

```bash
npm install
npm run dev        # Vite on :5173, proxies /api and /ws to :8787
npm run build      # static bundle in dist/
npm run typecheck  # tsc --noEmit
```

## Shape

```
src/
  main.tsx        # Router entry
  App.tsx         # Topbar + nav + outlet
  routes/         # One file per route
  lib/
    api.ts        # fetch + WS helpers against heiwa_core
    providers.ts  # typed loader for web/assets/providers.json
```

The `/hooks` route is read-only. It shows live local hook posture from provider
config files plus Heiwa audit paths; it does not mutate provider-owned hook
registries.

## Styles

Reuses tokens and component CSS from the marketing surface via `@import` of
`../../web/assets/styles.css`. Shared vocabulary is intentional; marketing and
cockpit should visually rhyme even though they're separate deploys.

## Data source

`providers.json` at `../../web/assets/providers.json` is the single source of
truth for provider rows. The marketing `/providers` page and the cockpit
`/providers` view must stay in sync — edit the JSON, not the pages.

## Scope guardrails

The cockpit is self-hosted. It must never:

- call out to `heiwa.ltd` for user data
- store operator secrets outside `~/.heiwa/`
- require a network connection for local-only operations

## Runtime contract

API shape lives in [docs/design/refs/API.md](/Users/dmcgregsauce/heiwa-universe/docs/design/refs/API.md).
