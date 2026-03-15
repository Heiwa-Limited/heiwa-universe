# Heiwa SpacetimeDB Bindings

This directory is the regeneration target for Heiwa's typed SpacetimeDB client bindings.

## Generate

```bash
/Users/dmcgregsauce/heiwa/apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh
```

The script currently generates:

- Rust client bindings in [`rust/`](/Users/dmcgregsauce/heiwa/packages/heiwa_bindings/rust)
- TypeScript client bindings in [`typescript/`](/Users/dmcgregsauce/heiwa/packages/heiwa_bindings/typescript)

## Python

Heiwa's Python runtime currently uses the typed control-plane wrapper in [`packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`](/Users/dmcgregsauce/heiwa/packages/heiwa_sdk/heiwa_sdk/spacetimedb.py) as the canonical bridge. Keep its methods aligned with the Rust module reducers/tables until an official generator path for Python is adopted in the repo.
