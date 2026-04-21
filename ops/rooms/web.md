# Room: Web — SvelteKit Migration Spec (DRAFT)

> **Status:** Draft — 2026-03-29
> **Owner:** Devon
> **Scope:** Migrate heiwa.ltd + app.heiwa.ltd from static HTML to SvelteKit on Cloudflare Pages

---

## 1. Problem

The current web surface (`apps/heiwa_web/clients/web/`) is ~15 hand-written HTML files with vanilla JS.
Each page duplicates the nav, styles, and API wiring. Adding a feature means touching raw HTML, duplicating
the auth redirect dance, and hoping the CSP headers stay in sync. The static shell served
its purpose for bootstrapping — now it's a drag on velocity.

The hub (`mcp_server.py`) also mirrors every HTML page as a route (lines 445–522), creating dual
maintenance. Browser auth now belongs to the SvelteKit shell, not `dashboard.html#token=<jwt>`.

## 2. Goals

1. **Single SvelteKit app** at `apps/heiwa_web/app/` replacing both heiwa.ltd (marketing) and app.heiwa.ltd (product)
2. **Cloudflare Pages deployment** via `wrangler pages deploy` (adapter-cloudflare)
3. **Consume api.heiwa.ltd** exclusively — no SSR data fetching, no server functions hitting STDB directly
4. **Discord OAuth flow** stays on the hub; SvelteKit handles the callback landing + secure cookie session
5. **Kill the static HTML routes** in mcp_server.py once migration is validated
6. **Zero new infra cost** — Cloudflare Pages free tier is sufficient

## 3. Domain Topology (Post-Migration)

| Domain | Source | Deploy Target | Notes |
|--------|--------|---------------|-------|
| `heiwa.ltd` | `apps/heiwa_web/app/` (marketing routes) | Cloudflare Pages | Homepage, pricing, signup CTA |
| `app.heiwa.ltd` | `apps/heiwa_web/app/` (auth'd routes) | Cloudflare Pages | Dashboard, missions, tools, etc. |
| `status.heiwa.ltd` | Same app or keep static | Cloudflare Pages | Could be a SvelteKit route or stay static |
| `api.heiwa.ltd` | `apps/heiwa_hub/` (unchanged) | Railway | Hub API, MCP, WebSockets |
| `docs.heiwa.ltd` | `docs/` (unchanged) | Cloudflare Pages | MkDocs Material |

### Domain Routing Strategy

**Option A — Single SvelteKit project, two domains:**
One SvelteKit app serves both `heiwa.ltd` and `app.heiwa.ltd`. Route groups split public vs. auth'd.
Cloudflare Pages custom domains point both at the same project. The app checks hostname to apply
auth guards on `app.heiwa.ltd` routes.

**Option B — Two Cloudflare Pages projects:**
Separate deploys. Simpler auth boundary but duplicates layout/components.

**Recommendation: Option A.** SvelteKit route groups handle this cleanly. One deploy, two domains,
shared design system.

## 4. Route Map

### Public (heiwa.ltd)

| Route | Replaces | Purpose |
|-------|----------|---------|
| `/` | `index.html` | Hero, product pitch, CTA → Discord OAuth |
| `/pricing` | (new) | Tier comparison (free/pro) |
| `/status` | `status.html` | Public system status |
| `/domains` | `domains.html` | Domain topology viewer |

### Authenticated (app.heiwa.ltd)

| Route | Replaces | Auth | Purpose |
|-------|----------|------|---------|
| `/` | redirect → `/dashboard` | Required | Landing after login |
| `/dashboard` | `dashboard.html` | Required | System overview, recent tasks |
| `/missions` | `missions.html` | Required | Mission list + detail |
| `/missions/[id]` | (new) | Required | Single mission view |
| `/tasks/[id]` | (new via API) | Required | Task detail + approval |
| `/approvals` | `approvals.html` | Required | Pending approval queue |
| `/tools` | (new) | Required | Registered MCP tools |
| `/connections` | `connections.html` | Required | Provider connections + BYOK vault |
| `/cells` | `cells.html` | Required | HeiwaCells catalog |
| `/rate-groups` | `rate-groups.html` | Required | Rate group status |
| `/history` | `history.html` | Required | Execution history |
| `/live` | `live.html` | Required | Live event stream |
| `/governance` | `governance.html` | Required | Governance policies |
| `/canvas` | `canvas/index.html` | Required | Visual canvas |
| `/settings` | (new) | Required | User settings, keys, profile |

### Auth Routes

| Route | Purpose |
|-------|---------|
| `/auth/callback` | Finalizes the browser session in SvelteKit, sets the secure cookie, redirects to dashboard |
| `/auth/logout` | Clears session, redirects to heiwa.ltd |

## 5. Auth Flow (Revised)

```
User clicks "Sign in" on heiwa.ltd
  → redirects to api.heiwa.ltd/auth/discord
  → Discord OAuth consent
  → api.heiwa.ltd/auth/discord/callback
  → hub redirects to app.heiwa.ltd/auth/callback
  → SvelteKit finalizes the secure session cookie
  → redirects to /dashboard
```

**Changes from current:**
- Hub redirect target changes from `/dashboard.html#token=` to `/auth/callback`
- SvelteKit layout guard checks the secure session on every auth'd route (not per-page JS)
- Browser session authority moves out of fragments/localStorage and into the cookie session

**Hub change required:** Update `HEIWA_WEB_ORIGIN` redirect in `auth.py` from
`/dashboard.html#token=` to `/auth/callback`.

## 6. API Client

Single `$lib/api.ts` module:

```typescript
// Base URL from env
const API = import.meta.env.VITE_API_URL ?? 'https://api.heiwa.ltd';

// Typed fetch wrapper — attaches JWT, handles 401 → redirect to login
export async function api<T>(path: string, init?: RequestInit): Promise<T>;

// WebSocket factory — attaches token as query param
export function ws(path: string): WebSocket;
```

**Endpoints to type (from mcp_server.py):**
- `GET /auth/me` → user profile
- `GET /status` → system status
- `GET /tools` → MCP tool list
- `POST /tasks` → create task
- `GET /tasks/{id}` → task detail
- `POST /tasks/{id}/approve` / `/reject`
- `GET /approvals` → pending approvals
- `GET /missions` / `GET /missions/{id}`
- `POST /missions/{id}/pause` / `/resume`
- `GET /auth/providers` → provider list
- `GET /auth/providers/{id}/status`
- `POST /auth/credentials` → BYOK key submission
- `GET /rate-groups` → rate group status
- `GET /history` → execution history
- `WS /ws/status` — system status stream
- `WS /ws/chat` — chat interface
- `WS /ws/operator` — operator dashboard stream
- `WS /ws/tasks/{id}` — task event stream

## 7. Tech Stack

| Layer | Choice | Why |
|-------|--------|-----|
| Framework | SvelteKit 2 | Fast, small bundles, adapter-cloudflare, Devon's preference |
| Styling | Tailwind CSS 4 | Replaces hand-written styles.css, utility-first |
| Fonts | Space Grotesk + IBM Plex Mono | Keep existing brand |
| Icons | Lucide Svelte | Clean, tree-shakeable |
| State | Svelte 5 runes ($state, $derived) | No external state lib needed |
| WebSocket | Custom store wrapping native WS | Svelte store per WS channel |
| Deploy | Cloudflare Pages (adapter-cloudflare) | Free tier, existing DNS setup |
| Package manager | pnpm | Monorepo-friendly, fast |

## 8. Project Structure

```
apps/heiwa_web/app/
├── src/
│   ├── lib/
│   │   ├── api.ts              # Typed API client
│   │   ├── ws.ts               # WebSocket store factory
│   │   ├── auth.ts             # JWT handling, guards
│   │   └── components/
│   │       ├── Nav.svelte      # Top nav (shared)
│   │       ├── StatusBadge.svelte
│   │       └── ...
│   ├── routes/
│   │   ├── (marketing)/        # Route group: heiwa.ltd public pages
│   │   │   ├── +layout.svelte  # Marketing layout (no auth)
│   │   │   ├── +page.svelte    # Homepage hero
│   │   │   ├── pricing/
│   │   │   └── status/
│   │   ├── (app)/              # Route group: app.heiwa.ltd auth'd pages
│   │   │   ├── +layout.svelte  # App layout (auth guard, sidebar)
│   │   │   ├── +layout.ts      # Auth check → redirect if no JWT
│   │   │   ├── dashboard/
│   │   │   ├── missions/
│   │   │   ├── approvals/
│   │   │   ├── tools/
│   │   │   ├── connections/
│   │   │   ├── cells/
│   │   │   ├── rate-groups/
│   │   │   ├── history/
│   │   │   ├── live/
│   │   │   ├── governance/
│   │   │   ├── canvas/
│   │   │   └── settings/
│   │   ├── auth/
│   │   │   ├── callback/+page.ts   # Callback landing; finalizes secure cookie session
│   │   │   └── logout/+page.ts
│   │   └── +layout.svelte     # Root layout
│   ├── app.html
│   └── app.css                # Tailwind imports
├── static/
│   ├── favicon.ico
│   └── robots.txt
├── svelte.config.js           # adapter-cloudflare
├── vite.config.ts
├── tailwind.config.ts
├── package.json
├── tsconfig.json
└── wrangler.toml              # Cloudflare Pages config
```

## 9. Migration Strategy

### Phase 1 — Scaffold + Auth (this PR)
- `pnpm create svelte@latest` in `apps/heiwa_web/app/`
- Install adapter-cloudflare, tailwind, base dependencies
- Implement auth callback route + secure session cookie
- Port homepage (index.html → marketing route group)
- Port dashboard (dashboard.html → app route group)
- Deploy to Cloudflare Pages as new project
- Verify Discord OAuth flow end-to-end

### Phase 2 — Port Remaining Pages
- Port each static page to SvelteKit route (one PR per batch)
- Replace vanilla JS API calls with typed `$lib/api.ts`
- Replace inline WebSocket code with Svelte WS stores
- Priority order: dashboard, missions, approvals, tools, connections, history, live

### Phase 3 — Cutover
- Point `app.heiwa.ltd` DNS to new Cloudflare Pages project
- Point `heiwa.ltd` to same project (marketing routes)
- Update hub's `HEIWA_WEB_ORIGIN` to confirm auth redirect target
- Remove static HTML routes from `mcp_server.py`
- Delete `apps/heiwa_web/clients/web/` (old static shell)
- Update CI to deploy SvelteKit build

### Phase 4 — Enhancements (post-migration)
- Real-time dashboard with WS stores
- BYOK vault UI in /connections
- Dark/light theme toggle
- PWA manifest for mobile access
- Canvas visualization upgrade

## 10. Hub Changes Required

1. **CORS middleware** — Add FastAPI CORSMiddleware allowing `heiwa.ltd` + `app.heiwa.ltd` origins
2. **Auth redirect** — Change redirect target from `/dashboard.html#token=` to `/auth/callback`
3. **Remove static routes** — Delete HTML-serving routes (Phase 3, after cutover validated)
4. **API-only surface** — Hub becomes pure API; no static assets served

## 11. CI/CD Changes

```yaml
# New job in deploy.yml
deploy-web:
  runs-on: ubuntu-latest
  needs: [hub-smoke-tests]
  steps:
    - uses: actions/checkout@v4
    - uses: pnpm/action-setup@v4
    - run: cd apps/heiwa_web/app && pnpm install && pnpm build
    - uses: cloudflare/wrangler-action@v3
      with:
        workingDirectory: apps/heiwa_web/app
        command: pages deploy
```

## 12. Open Questions

1. **Cookie vs localStorage for JWT?** Resolved: secure httpOnly cookie only; browser should not use fragments or localStorage as auth authority.
2. **Should status.heiwa.ltd become a SvelteKit route or stay static?** Lean toward SvelteKit route — it's simple and benefits from shared layout.
3. **Hostname-based routing or path-based?** SvelteKit can detect hostname in hooks. Need to verify adapter-cloudflare supports this cleanly.
4. **Do we need SSR at all?** Marketing pages benefit from SSR (SEO). App pages are client-only. SvelteKit handles this per-route with `export const ssr = false` on app routes.

## 13. Non-Goals

- No SSR data fetching — all data comes from api.heiwa.ltd client-side
- No BFF (backend-for-frontend) — hub is the API
- No database access from SvelteKit — STDB is hub-only
- No breaking API changes — SvelteKit consumes existing endpoints
- No mobile app — PWA stretch goal only
