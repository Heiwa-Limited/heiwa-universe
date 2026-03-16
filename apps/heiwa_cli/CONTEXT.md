# apps/heiwa_cli — Operator CLI

The local command-line interface for interacting with Heiwa.

## Key Files

| File | Purpose |
| --- | --- |
| `heiwa` | Main CLI entrypoint (executable script) |

## Commands

- `heiwa cells` — View HeiwaCells agent catalog
- `heiwa bench` — Run release gate benchmarks
- `heiwa status` — System health check

## Notes

- CLI is an operator surface, not the primary execution path
- Hub runs on Railway — CLI connects to it via HTTP/WebSocket
- For local dev, run `python -m apps.heiwa_hub.main` to start the hub
