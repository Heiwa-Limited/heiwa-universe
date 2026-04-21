# Failure Modes

## Public shell drift

If the static site advertises surfaces or providers that the docs and CI do not verify, the public shell is wrong and must be reduced.

## Runtime unavailable

If the installed runtime or a required backend/state dependency is unavailable, status reporting should degrade to an explicit warning state. Documentation should still build and publish independently of runtime health.

## WebSocket unavailable

If WebSocket status/event streaming is unavailable, the public status page may fall back to HTTP diagnostics. That fallback is diagnostic only, not the target architecture.

## Docs drift

If MkDocs pages and README disagree, treat the docs build as failed and resolve the claim mismatch before deployment.
