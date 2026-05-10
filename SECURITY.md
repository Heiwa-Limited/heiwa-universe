# Security Policy

## Supported scope

Security reports should target the current public contract:

- installed `heiwa` runtime and Rust workspace
- provider discovery, routing, local execution, and evidence paths
- GitHub Actions, release packaging, docs publishing, and installer flows
- local secret handling and generated artifacts that may expose operator data

Legacy hosted, trading, and experimental surfaces may still exist in the tree. Report issues there if they are reachable from the current runtime, release process, or documented public surfaces.

## Reporting

Do not open a public issue for exploitable vulnerabilities, leaked secrets, credential material, private logs, or local operator data.

Use GitHub private vulnerability reporting for `Strategizing/heiwa-universe` when available. If that is unavailable, contact the repository owner privately and include:

- affected commit, tag, or release
- exact reproduction steps
- impacted surface
- expected impact
- whether secrets or private data were exposed

## Expectations

- Do not exfiltrate secrets, tokens, local keys, or private user data.
- Do not run destructive proof-of-concept commands against machines or services you do not own.
- Keep reports scoped to the minimum evidence needed to prove the issue.

## Release response

Accepted fixes should ship through normal GitHub review and release channels. Security-sensitive release notes should describe impact and upgrade guidance without publishing exploit-ready private details.
