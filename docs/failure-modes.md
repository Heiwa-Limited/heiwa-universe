# Failure Modes

## Public shell drift

If the static site advertises surfaces or providers that the docs and CI do not verify, the public shell is wrong and must be reduced.

## Runtime unavailable

If hosted runtime/API endpoints are unavailable, status pages should degrade to an explicit warning state. Installed `heiwa` workflows, public docs, and release artifacts should remain understandable without depending on a live hosted control plane.

## WebSocket unavailable

If WebSocket status/event streaming is unavailable, the public status page may fall back to HTTP diagnostics. That fallback is diagnostic only, not the target architecture.

## Docs drift

If MkDocs pages and README disagree, treat the docs build as failed and resolve the claim mismatch before deployment.
