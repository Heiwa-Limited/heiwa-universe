# Security

## Public-safe posture

- no secrets on public pages
- no write-capable controls on the public status surface
- docs and marketing stay separate from privileged runtime state
- prefer canonical domains over direct provider URLs in the public shell

## Runtime guardrails

- fail closed on missing secrets and identities
- redact transport and provider credentials in logs
- keep portability boundaries intact

## Non-goals for public docs

This doc set does not claim that Discord or experimental canvases are required public entry points.
