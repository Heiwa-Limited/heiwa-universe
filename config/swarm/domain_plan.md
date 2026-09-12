# Heiwa public domain plan

Status: publishing contract, 2026-09-07. Plane: Evidence.

Each user's installed runtime and resolved configuration root own private
identity, state, sessions, approvals, and execution. Public pages distribute
software and documentation; they do not host the operator runtime.

| Surface | Publishing owner | Contract |
| --- | --- | --- |
| Installed `heiwa` | User's machine | Local runtime, cockpit, and private evidence |
| `heiwa.ltd` | Cloudflare Pages, project `heiwa-clients` | Allowlisted public shell and installer glue via `deploy.yml` |
| `docs.heiwa.ltd` | GitHub Pages | Locked MkDocs build via `pages.yml` |
| GitHub Releases | GitHub | Sole release binary, checksum, and provenance authority |
| `status.heiwa.ltd` | Cloudflare Pages public shell alias | Static status page at `/status.html`; static reachability does not prove live runtime health |
| `api.heiwa.ltd` | Paused | Future public-safe helper paths require their own acceptance |

Cloudflare owns DNS and the static public edge. GitHub owns source, CI, releases,
and documentation publication. Local JSONL is canonical evidence; Lance is
rebuildable recall. Future redacted evidence sync is not a live service.

`apps/heiwa_app/clients/web/assets/domains.bootstrap.json` is the public-safe
projection of this plan, consumed by the static site and local cockpit. Its
`platform.public_shell` and `platform.public_docs` fields model independent
hosting. The static-surface validator rejects the old single-host field.
This plan and manifest express publishing ownership and intent, not measured
availability or a DNS inventory. Verify endpoints after each deployment.

Do not expose private operator routes, credentials, or runtime state in public
artifacts. `scripts/package_public_web.sh` is the public deployment allowlist.
Do not present `app.heiwa.ltd`, `auth.heiwa.ltd`, or `trade.heiwa.ltd` as active.
