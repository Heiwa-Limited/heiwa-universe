# Security

Heiwa is an installed, local-first operator runtime. The supported public surface is static documentation, release distribution, installation, and read-only status. There is no hosted multi-tenant operator control plane.

## Supported public surfaces

- `heiwa.ltd`: static product, install, support, and release links
- `docs.heiwa.ltd`: public documentation
- `status.heiwa.ltd`: read-only public status when deployed
- GitHub Releases: signed workflow provenance, checksums, archives, and installer authority

`app.heiwa.ltd`, `auth.heiwa.ltd`, and `trade.heiwa.ltd` are not supported public surfaces. Legacy operator HTML remains source-only until it is either retired or redesigned around the installed loopback runtime. `scripts/package_public_web.sh` is the deploy allowlist and excludes those files.

## Trust boundaries

### Public browser -> static support surface

- Public pages contain no provider keys, operator tokens, authenticated mutations, or private runtime data.
- Endpoint overrides are restricted to the official HTTPS/WSS API host.
- Untrusted status and domain data is rendered through text nodes, not HTML parsing.
- The deployed artifact is built from an explicit file allowlist and checked against inline scripts and private operator assets.

### Installed UI or CLI -> loopback runtime

- The installed `heiwa` process owns sessions, approvals, routing, evidence, and local app APIs.
- Operator HTTP and WebSocket surfaces bind to loopback by default; they are not public identity endpoints.
- Mutating actions pass through the approval and execution contract instead of trusting a browser-only decision.

### Runtime -> local evidence plane

- Versioned JSONL under `~/.heiwa/evidence/` is canonical execution evidence.
- SQLite holds bounded hot state; Lance is derived, local recall and can be rebuilt.
- Raw journals, provider secrets, and private operator state are not published to GitHub or the public web package.

### Runtime -> providers

- Providers own authentication and inference internals.
- Heiwa discovers provider-owned accounts, routes calls, and records bounded receipts without claiming account usability from credential presence alone.
- Provider credentials remain local and must be redacted from logs, diagnostics, evidence, and generated artifacts.

### Runtime -> execution

- Tool leases, scoped paths, approval requirements, and receipts constrain nondeterministic execution.
- Stale automation executions use a bounded lease-recovery path; exhausted work fails closed instead of retrying forever.
- Untrusted third-party code belongs in an isolated sandbox, not the owner runtime.

## Assets that matter

- provider OAuth sessions, API keys, and local model endpoints
- operator prompts, outputs, evidence, recall, and life projections
- approval state, execution leases, and receipts
- GitHub release permissions, workflow tokens, tags, checksums, and artifacts
- installer source URLs and installed runtime paths

## Current guardrails

- blocking full-history gitleaks scan with reviewed historical fingerprints
- Cargo, npm, and Python dependency audits in the local and hosted security gate
- immutable commit pins for every GitHub Action
- least-privilege workflow permissions and explicit release/container permissions
- public web allowlist, strict CSP, HSTS, frame denial, and no inline scripts
- closed-schema private projections and local state-directory overrides
- release metadata, product-surface, backend-transition, runtime-pin, and machine-security gates

## Accepted transitive warning

The Linux Tauri desktop graph currently inherits GTK3 and `glib 0.18.5`. RustSec flags the unmaintained GTK3 bindings and `RUSTSEC-2024-0429`, which affects `glib::VariantStrIter`. Heiwa does not call `VariantStrIter`; the dependency is absent from non-Linux targets and remains constrained by Tauri's current Linux WebKit stack. This is an informational transitive exception, not a suppressed vulnerability. It must be removed when Tauri's Linux stack moves to `glib >=0.20` or the Linux desktop target is retired.

## Reporting

Follow [the repository security policy](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/SECURITY.md). Do not open public issues containing exploit details, secrets, private logs, or operator data.
