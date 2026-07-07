# heiwa-desk — spike, not product

Status: prototype (2026-07-05). Validates one architecture question: can a Deno front-end give
Heiwa.app built-in agent orchestration — herd visibility, rapid pane switching, prompt/run into
any pane — on top of the herdr multiplexer, while `deno desktop` packaging stays optional?

- Web mode (works today): `deno task desk` then open <http://127.0.0.1:7480>
- Desktop shell read mode: `apps/heiwa_app/desktop` can read
  herd pane state through its native Tauri `herd_panes` command. That command
  prefers this Deno bridge when running, then falls back to the local `herdr`
  CLI. CORS is restricted to local dev/Tauri origins for browser-preview fallback.
- Desktop mode (verified 2026-07-05): `deno desktop --allow-run=herdr --allow-net=127.0.0.1:7480
  prototypes/heiwa-desk/main.ts` builds `heiwa-desk.app` (gitignored); `open heiwa-desk.app` runs it.

Desktop-mode findings (verified):

- The desktop runtime ignores the `Deno.serve` port and rebinds to an ephemeral localhost port the
  webview navigates to — same entrypoint works in both modes, don't hardcode URLs in the UI.
- `deno desktop` roots embedding at the workspace: built from repo root, the bundle vacuumed
  `apps/` + `node_modules` (~203MB files, 269MB .app). Needs include/exclude scoping or a
  standalone project dir before this is a distributable.

Boundaries:

- `deno desktop` is experimental in Deno 2.9 — do not promote this to product until it stabilizes.
- herdr is a candidate substrate; the `heiwa_terminal` tmux plan is unchanged. This spike informs
  that decision, it does not make it.
- Read/send only. True interactive attach stays in a real terminal (`herdr`).
