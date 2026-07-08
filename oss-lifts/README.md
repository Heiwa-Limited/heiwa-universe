# Heiwa OSS lifts

This directory is the pre-implementation extraction layer for OSS value that may be implemented in Heiwa.

Canonical flow:

1. Raw upstream git clones live in `~/oss-repos/<repo>`.
2. Extracted value, lift notes, translated snippets, license notes, and implementation maps live here under `~/heiwa-universe/oss-lifts/`.
3. Production implementation lands in the appropriate Heiwa subsystem (`crates/`, `apps/`, `packages/`, `scripts/`, docs, CI, etc.).

Do not put raw upstream clones here. Do not treat root `vendor/` quarantine as the default product path.
