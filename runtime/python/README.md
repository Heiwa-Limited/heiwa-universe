# heiwa-sidecar

Python sidecar that the Rust Heiwa runtime spawns as a subprocess and talks to over stdio.

## Wire protocol

- **Framing:** one JSON object per line on stdin / stdout (JSONL). stderr is free-form logs.
- **Request:** `{"id": str, "op": str, "args": {...}}`
- **Response:** `{"id": str, "status": "ok", "result": any}` or `{"id": str, "status": "err", "code": str, "message": str}`

## Built-in ops

| op           | purpose                                           |
| ------------ | ------------------------------------------------- |
| `health`     | Liveness probe. Returns `{"status": "ok"}`.       |
| `version`    | Sidecar, Python, and platform versions.           |
| `check_deps` | Probe importability of langgraph/optional llama_index. |
| `echo`       | Return `args` verbatim (wire-test helper).        |
| `shutdown`   | Reply and exit loop cleanly.                      |

Add new ops in `src/heiwa_sidecar/handlers.py` and register in `HANDLERS`.

## Local dev

```bash
cd runtime/python
uv sync --extra dev
uv sync --extra dev --extra llama   # optional LlamaIndex probe surface
uv run python -m heiwa_sidecar   # serves on stdin/stdout
uv run --extra dev python -m pytest # run tests
```

The Rust side launches this module via `python -m heiwa_sidecar`. No entry-point script is required for the spawn path, but one is provided (`heiwa-sidecar`) for interactive debugging.
