# FFF → Heiwa repo-context lift

Source clone: `~/oss-repos/fff`  
Upstream: `https://github.com/dmtrKovalenko/fff`  
License: MIT — direct implementation/lift allowed with attribution when copied.

## Extracted value

FFF is useful to Heiwa in two different layers:

1. **Global development tool** — `fff-mcp` is installed at `~/.local/bin/fff-mcp` and connected to Claude Code + Codex MCP for agent/developer search efficiency.
2. **Heiwa product implementation** — Heiwa should not depend on the user's global `fff-mcp` as a shipped product boundary. The product lift belongs in `crates/heiwa_mcp` as scoped, read-only repo context tools that preserve `ExecutionScope`, tool leases, receipts, and capabilities.

## Upstream implementation truths inspected

- `crates/fff-core/src/file_picker.rs`
  - `FilePicker::collect_files` performs a synchronous index.
  - `FilePicker::fuzzy_search` provides fuzzy path search.
  - `FilePicker::grep` provides content search.
- `crates/fff-core/src/grep.rs`
  - grep supports `PlainText`, `Regex`, and `Fuzzy` modes.
  - result items include file path refs, line number, byte column, byte offset, context lines, definition classification, and pagination-ish offsets.
- `crates/fff-query-parser/`
  - query parsing is separate and tested upstream.

## Heiwa first implementation slice

Target files:

- `crates/heiwa_mcp/src/local_tools.rs`
- `crates/heiwa_mcp/tests/local_tools.rs`
- `apps/heiwa_shell/src/main.rs`

First product behavior:

- Add `repo.find` as a read-only, lease-gated, execution-scope-gated file finder.
- Return relative paths, file names, simple fuzzy scores, and total scanned/matched counts.
- Respect `max_results`.
- Keep `repo.grep` intact for now; replace it with indexed/content-aware search in a later slice.

Why this shape first:

- It gives agents deterministic repo/file context without trusting global MCP config.
- It advances Heiwa's **Execution** plane: safer tool routing under leases.
- It creates a tested local foundation before pulling larger FFF internals into the product.

## Later slice

When the first contract is green, either:

- translate the relevant FFF algorithms into a small Heiwa-owned module, or
- vendor/import a minimal MIT-attributed subset with a `NOTICE`/attribution path.

Do not expose a user-facing dependency on `fff-mcp` for shipped Heiwa runtime behavior.
