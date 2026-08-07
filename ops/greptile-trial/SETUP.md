# Greptile setup — exact commands

Run from `~/heiwa` inside WSL Ubuntu.

## 1. Commit only the config (the tree has unrelated dirty work in it)

```bash
git add greptile.json .greptile/ ops/greptile-trial/ && git commit -m "ops: add Greptile trial config and scoring scaffold"
```

Check `git status` first. As of 2026-08-07 the working tree has uncommitted
changes to `.github/workflows/`, `AGENTS.md`, `Cargo.toml`, `Cargo.lock`, and
several `apps/heiwa_app` files — do not sweep those in.

## 2. GitHub app

Install at app.greptile.com, scope it to `Heiwa-Limited/heiwa-universe` only,
then set Organization Settings → Billing → **Flex Usage Limit = $0**.

## 3. Key handling

Rotate the key first — the original was pasted into a chat transcript. Then
keep it out of tracked files:

```bash
echo 'export GREPTILE_API_KEY="<rotated-key>"' >> ~/.bashrc && source ~/.bashrc
```

## 4. CLI

```bash
npm install -g greptile@latest
```

```bash
greptile whoami && greptile config
```

`greptile config` prints the merged effective settings from `.greptile/`,
dashboard, and org rules. Run it once after install to confirm the committed
rules are actually being picked up — if `.greptile/config.json` rules do not
appear, nothing downstream in the trial is measuring what you think.

## 5. MCP server for Claude Code

```bash
claude mcp add --transport http greptile https://api.greptile.com/mcp --header "Authorization: Bearer $GREPTILE_API_KEY"
```

Do **not** add Greptile to this repo's tracked `.mcp.json` with a literal key.
If you want it project-scoped for the trial, use the variable form:

```json
{
  "mcpServers": {
    "greptile": {
      "type": "http",
      "url": "https://api.greptile.com/mcp",
      "headers": { "Authorization": "Bearer ${GREPTILE_API_KEY}" }
    }
  }
}
```

Verify with `claude mcp list`.

## 6. Local review on the current branch

```bash
greptile review --branch main --json > ops/greptile-trial/raw/$(git rev-parse --abbrev-ref HEAD | tr / -).json
```

Add `--agent` instead of `--json` when you want output shaped for another
coding agent to consume rather than for scoring.

## 7. Trigger a review on an existing PR

Comment `@greptileai` on the PR. For the trial, start with
[PR #52](https://github.com/Heiwa-Limited/heiwa-universe/pull/52).

## Teardown if you do not convert (2026-08-19)

```bash
git rm -r --cached greptile.json .greptile/ && claude mcp remove greptile && npm uninstall -g greptile
```

Then uninstall the GitHub app from the org and revoke the API key. Keep
`ops/greptile-trial/` — the scorecard is the record of why, and it is the
baseline if you re-trial after fixing PR size.
