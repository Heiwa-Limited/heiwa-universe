# Herd Workflows — herdr + Deno for BYOK/S Agent Development

Status: verified on 2026-07-05 against herdr 0.7.1 and Deno 2.9.1 (macOS, dmac.local). Every
command below was run headless before being written down.

The herd = provider CLIs you already pay for or run free (Claude Code, Codex, Gemini CLI, Ollama),
each in a persistent herdr pane. herdr keeps sessions alive across laptop close and ssh drops;
Deno provides the typed, least-privilege glue. Heiwa owns sessions and sandboxes; providers own
auth and inference.

## Shared primitives (human and agent)

| Primitive                      | Command                                                                                                                 |
| ------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| Herd status (table)            | `deno task herd`                                                                                                        |
| Herd status (JSON, for agents) | `deno task herd --json`                                                                                                 |
| Create project workspace       | `herdr workspace create --cwd ~/heiwa-universe --label heiwa --no-focus`                                                |
| Launch an agent pane           | `herdr agent start <name> --workspace w1 --cwd <path> --env PATH=/opt/homebrew/bin:/usr/bin:/bin -- <absolute-argv...>` |
| Run a command in a pane        | `herdr pane run <pane_id> "just check-fmt-docs"`                                                                        |
| Block until output appears     | `herdr wait output <pane_id> --match "Checked" --timeout 60000`                                                         |
| Block until agent state        | `herdr wait agent-status <pane_id> --status idle --timeout 600000`                                                      |
| Read what happened             | `herdr pane read <pane_id> --source visible`                                                                            |
| Send text to an agent          | `herdr agent send <name> "<prompt>"` then `herdr pane send-keys <pane_id> enter`                                        |

Gotcha (verified): the brew-service herdr server runs with launchd's bare PATH
(`/usr/bin:/bin:/usr/sbin:/sbin`). Pass absolute binary paths to `agent start` and set `--env PATH=...`.

## Operator workflows (Devon)

**Daily attach.** `herdr` in any terminal attaches the persistent session — same panes after
reboot of the terminal app, over ssh, or from mobile. `deno task herd` first if you only want the
map, not the cockpit.

**Standing herd.** One workspace per project (`heiwa`, `ai-dj`, ...). Inside each: one pane per
provider CLI you want warm. Claude Code and Codex panes report live state (working/blocked/done/idle)
via the integration hooks installed 2026-07-05; you see at a glance who is blocked on an approval.
Ollama and Gemini panes work fine but show `unknown` state — no integration at herdr 0.7.1.

**Walk-away runs.** Kick an agent, close the laptop; the pane survives server-side. On return:
`deno task herd` → `herdr agent read <name> --lines 50` to catch up, `herdr agent attach <name>`
to take over interactively.

**Parallel feature work.** `herdr worktree create --workspace w1 --branch <name>` gives an agent
an isolated git worktree tied to a workspace — several agents on the same repo without stepping
on each other's tree.

## Agent workflows (Claude Code, Codex, and peers)

**Orient.** `deno task herd --json` is the machine view: workspaces + per-pane agent/state/cwd in
one stable document. Prefer it over screen-scraping.

**Delegate to a cheaper model (BYOK routing).** Local models cost nothing; spend subscription
quota only where marginal value is high. Verified loop:

```sh
herdr agent start qwen-local --workspace w1 --cwd ~/heiwa-universe \
  --env PATH=/opt/homebrew/bin:/usr/bin:/bin -- /opt/homebrew/bin/ollama run qwen3.5:4b
herdr wait output <pane_id> --match ">>>" --timeout 60000
herdr agent send qwen-local "<prompt>"
herdr pane send-keys <pane_id> enter
herdr pane read <pane_id> --source visible
```

**Drive long tasks without babysitting.** `pane run` + `wait output --match` turns any pane into
a structured job: the wait call returns JSON containing the matched line and surrounding text.
Verified: `herdr pane run w1:p1 "just check-fmt-docs"` then
`herdr wait output w1:p1 --match "Checked"` returned `"matched_line":"Checked 188 files"`.

**Coordinate peers.** `herdr wait agent-status <pane> --status idle` blocks until a Claude/Codex
pane finishes its turn — a real synchronization point between peer agents, no polling loop.

**Write new glue in Deno, least-privilege.** Follow `scripts/herd.ts`: declare exactly what the
script may touch (`--allow-run=herdr --allow-env=HOME`), register it as a task in `deno.json`.
`deno check` gates types; `deno fmt` covers `scripts/` and authored docs.

## Boundaries

- herdr is a peer tool / candidate substrate; the in-flight `heiwa_terminal` tmux plan
  (`docs/superpowers/plans/2026-06-20-heiwa-tmux-multiplexer.md`) is unchanged. Decision open.
- Provider CLIs keep their own auth, quotas, and prompts. herdr adds persistence and state
  visibility only.
- `deno desktop` is experimental in 2.9 — evaluate for a future Heiwa desktop surface, do not
  build product on it yet.
