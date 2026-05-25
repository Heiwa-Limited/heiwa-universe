# Heiwa Cockpit

Local Heiwa.app client. The installed user path is the HOME-local
`~/.heiwa/app/Heiwa.app` launcher over the installed `heiwa` runtime. The
localhost/browser surface is a per-user support console and development view, not
the target primary display.

- **Framework:** Vite + Solid + TypeScript
- **Installed by:** `heiwa install` → `~/.heiwa/app/Heiwa.app`
- **Served by:** `heiwa app` → local HTTP server (default `:8787`)
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
