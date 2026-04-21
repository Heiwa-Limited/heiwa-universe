# Provider Registry

## Active surfaces

| Role | Provider | Status | Notes |
|:-----|:---------|:-------|:------|
| Installed runtime | Local machine | Active | `heiwa` CLI and local operator surface |
| Source control / CI | GitHub | Active | repo, pull requests, Actions |
| Documentation publish | GitHub Pages | Active | MkDocs site for release-facing docs |
| State layer | SpacetimeDB | Active where wired | backend adjudication/evidence surface for current runtime paths |
| Legacy hosted paths | Various | Reference only | not the default product story for current platform work |

## Posture

- The installed runtime stays the canonical user-facing product surface.
- Docs and CI should describe only what is presently wired and verified.
- Hosted or preview infrastructure should not dominate the public repo story unless it is the active product center.
- New surfaces should not be promoted until they are verified and necessary.
