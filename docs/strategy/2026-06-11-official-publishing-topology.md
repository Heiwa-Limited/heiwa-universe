# Official Publishing Topology — SpacetimeDB + GitHub + Cloudflare

Date: 2026-06-11
Classification: Evidence / Execution boundary

## Product boundary

Heiwa.app and the installed `heiwa` runtime run solely on user-owned devices. Heiwa does **not** provide a hosted app/runtime service. Hosted infrastructure exists only for:

- SpacetimeDB Maincloud: evidence sync/adjudication when enabled.
- GitHub: source, CI, tags, releases, release assets, and checksums.
- Cloudflare: public docs, marketing/install pages, status/update-manifest edge, and optional static assets.

No Cloudflare Worker, Pages app, or hosted Rust service should become the product runtime authority.

## SpacetimeDB official-docs posture

Sources:

- https://spacetimedb.com/docs/
- https://spacetimedb.com/docs/how-to/deploy/maincloud
- https://spacetimedb.com/docs/cli-reference/

Official docs establish this flow:

1. Install the `spacetime` CLI.
2. Authenticate the CLI using GitHub:
   ```bash
   spacetime login
   ```
3. Publish/update a database to Maincloud:
   ```bash
   spacetime publish heiwaproductiondb --server maincloud
   ```
4. Connect clients to:
   - URI: `https://maincloud.spacetimedb.com`
   - database/module: `heiwaproductiondb`

Maincloud is SpacetimeDB's managed serverless platform. Publishing with the same command updates existing modules and hot-swaps module code without disconnecting clients, subject to schema migration rules.

### Heiwa decision

Heiwa should treat STDB publisher/operator auth as **shell CLI auth**, not a Heiwa-owned API key plane:

```bash
spacetime login show
spacetime publish heiwaproductiondb --server maincloud
```

`STDB_TOKEN` remains legacy/compat material only. It must not be the canonical operator auth requirement for the local installed product.

## GitHub official-docs posture

Sources:

- https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases
- https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository
- https://docs.github.com/en/rest/releases/assets

Official docs establish GitHub Releases as deployable software iterations based on Git tags, with release notes and binary assets. Release assets can be uploaded and managed through GitHub UI, `gh release`, or the REST API; public release assets can be downloaded without auth.

### Heiwa decision

GitHub is the binary/source authority:

- tags define release identity
- release assets contain platform artifacts (`Heiwa.app`, DMG/zip/tarball, checksums)
- checksums and CI evidence live with the release
- Heiwa update checks may read GitHub/manifest metadata, but Cloudflare must not become a second binary authority

## Cloudflare official-docs posture

Sources:

- https://developers.cloudflare.com/pages/
- https://developers.cloudflare.com/pages/configuration/custom-domains/
- https://developers.cloudflare.com/workers/configuration/routing/
- https://developers.cloudflare.com/workers/configuration/routing/custom-domains/

Official docs establish:

- Cloudflare Pages deploys projects via Git provider integration, Direct Upload, or C3.
- Pages can serve custom domains/subdomains.
- Workers Custom Domains make the Worker the origin for a hostname.
- Production Workers should use routes/custom domains rather than `workers.dev`.

### Heiwa decision

Cloudflare is public edge only:

- docs / marketing / install page
- status page
- optional update manifest cache pointing back to GitHub release identity/checksums
- no hosted Heiwa app/runtime service
- no runtime state authority
- no inference/proxy hot path

## Solo-publisher release shape

For a solo publisher, the robust path is:

1. Build and test locally from `~/heiwa-universe`.
2. Publish/update STDB module through shell-authenticated CLI:
   ```bash
   spacetime login show
   spacetime publish heiwaproductiondb --server maincloud
   ```
3. Tag and release through GitHub:
   ```bash
   gh release create vX.Y.Z <assets...> --notes-file RELEASE.md
   ```
4. Put docs/install/update-manifest material on Cloudflare, with GitHub release URLs and checksums as the authority.
5. Users install/run `Heiwa.app` and `heiwa` locally; their devices connect to Maincloud only for evidence sync/adjudication when configured.

## Anti-drift rules

- Do not describe `app.heiwa.ltd` as the hosted Heiwa product runtime.
- Do not require a Heiwa-owned STDB API token for ordinary operator auth; prefer `spacetime login` shell identity.
- Do not move provider execution, filesystem side effects, local approvals, or local model routing into Cloudflare.
- Do not imply Cloudflare Pages/Workers host user sessions unless the explicit feature is public static/docs/install/status material.
