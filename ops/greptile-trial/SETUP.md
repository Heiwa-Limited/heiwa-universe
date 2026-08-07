# Greptile setup — exact commands

Run from `~/heiwa` inside WSL Ubuntu. Steps 1–3 are done; step 4 is the gate.

## Status as of 2026-08-07

| Step | State |
| --- | --- |
| CLI installed (3.3.1, node 26.5.0) | done |
| API key authenticates, org "Heiwa Limited" resolves | done |
| Config committed and opened as PR #54 | done |
| **Repo connected to Greptile** | **blocked — needs your browser** |
| GitHub App installed on the org | blocked, same step |
| Flex limit set to $0 | blocked, dashboard only |
| Key rotated | not done |

Everything after the gate is written out below and ready to run.

## 1. CLI (done)

```bash
npm install -g greptile@latest && greptile --version
```

The CLI sends anonymous usage telemetry by default (it states never code or
repo contents). To opt out permanently:

```bash
greptile settings set telemetry false
```

## 2. Key handling

The original key was pasted into a chat transcript — rotate it at
app.greptile.com, then keep the replacement out of tracked files:

```bash
echo 'export GREPTILE_API_KEY="<rotated-key>"' >> ~/.bashrc && source ~/.bashrc
```

Verify:

```bash
greptile whoami
```

## 3. Config (done — PR #54)

`greptile.json`, `.greptile/`, and `ops/greptile-trial/` are committed on branch
`ops/greptile-trial`, built in a worktree at
`.worktrees/claude/greptile-trial/` per the `CONTRIBUTING.md` convention, so
the ~70 uncommitted files in the main checkout were left alone.

## 4. Connect the repo — this is the gate

`greptile review` and `greptile config` both hard-fail with
`this repository is not connected to Greptile yet` until this is done. There is
no non-interactive path; the CLI exposes only an interactive wizard.

```bash
greptile onboard
```

Or connect `Heiwa-Limited/heiwa-universe` from the dashboard. Either way it
installs a GitHub App on the org — scope it to this repo only.

Immediately afterward, in Organization Settings → Billing, set **Flex Usage
Limit = $0**. Billing is $30/seat/month including 50 credits, $1/credit beyond,
and the cap is the only thing that makes an accident impossible.

Then confirm the committed rules are actually in effect:

```bash
greptile config
```

If `.greptile/config.json`'s rules do not appear in the merged output, nothing
downstream in the trial is measuring what you think it is.

## 5. MCP server for Claude Code

Deferred until step 4 lands — the MCP tools hit the same connection gate.

```bash
claude mcp add --transport http greptile https://api.greptile.com/mcp --header "Authorization: Bearer $GREPTILE_API_KEY"
```

Do **not** add Greptile to this repo's tracked `.mcp.json` with a literal key.
If you want it project-scoped, use the variable form:

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

## 6. Running a review — two gotchas that cost real time

**Always pass an explicit base.** `greptile review` resolves its base against
the *local* ref, and local `main` is currently 762 files behind `origin/main`.
Without `--branch origin/main` you get a diff of PRs #50 and #51 in reverse.

**There is a hard file cap around 760**, separate from `fileChangeLimit` in
`greptile.json` (which only governs the PR bot). Oversized reviews are refused,
not truncated.

A helper that archives raw output for the scorecard:

```bash
bash ops/greptile-trial/raw/run-review.sh <label> <workdir> origin/main
```

## 7. Trigger a review on an existing PR

Comment `@greptileai` on the PR. Order to work through once connected:

1. [PR #54](https://github.com/Heiwa-Limited/heiwa-universe/pull/54) — this
   config PR. Small, known contents, so cold-start quality is readable.
2. [PR #52](https://github.com/Heiwa-Limited/heiwa-universe/pull/52) — 87 files,
   under the limit, touches CI and DREX goldens. The real test.
3. Retro-review merged history for ground truth you can check, because you
   already know what broke afterward:

```bash
git worktree add -b trial/retro-51 .worktrees/claude/retro-51 8435983 && bash ops/greptile-trial/raw/run-review.sh retro-51 .worktrees/claude/retro-51 origin/main
```

## 8. Agent skills — hold until after the counterfactual

The CLI bundles two installable skills, `greptile-cli` and `greploop` (review →
fix → review until clean). Installing writes to `.agents/skills/`,
`.claude/skills/`, and `.codex/skills/`.

```bash
greptile skills list
```

Do not install before the Days 8–12 counterfactual is recorded — it lets the
authoring agent see Greptile's findings mid-review and destroys the comparison.
After that, evaluate it on its own: an autonomous fix-review loop is a different
product from PR comments, and the more interesting one for this repo.

## Teardown if you do not convert (2026-08-19)

```bash
git rm -r --cached greptile.json .greptile/ && claude mcp remove greptile && npm uninstall -g greptile
```

Then uninstall the GitHub App from the org and revoke the API key. Keep
`ops/greptile-trial/` — the scorecard is the record of why, and the baseline if
you re-trial after fixing PR size.
