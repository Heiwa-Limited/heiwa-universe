# Greptile setup — exact commands

Run from `~/heiwa` inside WSL Ubuntu.

## Status as of 2026-08-07

| Step | State |
| --- | --- |
| CLI installed (3.3.1, node 26.5.0) | done |
| API key authenticates, org "Heiwa Limited" resolves | done |
| GitHub App installed on the org | done |
| `origin` remote corrected to `Heiwa-Limited/…` | done — was the real blocker |
| Config committed, opened as PR #54 | done |
| Config verified live via `greptile config` | done |
| First review completed (3 findings, all real) | done |
| **Flex limit set to $0** | **not done — dashboard only, do this** |
| **API key rotated** | **not done — do this** |
| MCP server added to Claude Code | not done |
| `@greptileai` run on PR #52 | not done |

The trial is live. The two bold rows are yours and neither can wait long.

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
app.greptile.com.

Do **not** append it to `~/.bashrc`. That is what this file said first, and
Greptile flagged it P1 on the very PR that introduced it: a literal credential
in a dotfile is exposed to backups, dotfile sync, and anything that reads
`$HOME`, which contradicts the vault contract stated two sections down. The
catch was correct.

Use the OS keychain. `libsecret` is already a build dependency here — the
workspace links it through `keyring` — so `secret-tool` is available:

```bash
secret-tool store --label="Greptile API key" service greptile account "$USER"
```

Then resolve it per-shell, so the secret is never written to a dotfile:

```bash
echo 'export GREPTILE_API_KEY="$(secret-tool lookup service greptile account "$USER")"' >> ~/.bashrc
```

That line contains a lookup, not a credential, and is safe to sync.

Verify:

```bash
greptile whoami
```

Longer term the right home is `crates/heiwa_vault`, same as provider auth
material. The keychain is the interim.

## 3. Config (done — PR #54)

`.greptile/` and `ops/greptile-trial/` are committed on branch
`ops/greptile-trial`, built in a worktree at
`.worktrees/claude/greptile-trial/` per the `CONTRIBUTING.md` convention, so
the ~70 uncommitted files in the main checkout were left alone.

## 4. Connect the repo (done — with a trap worth knowing)

`greptile onboard` or the dashboard installs the GitHub App on the org. Scope
it to this repo only.

**The App being installed is not sufficient.** The CLI identifies the repo by
the `origin` remote URL, and ours still read
`git@github.com:Strategizing/heiwa-universe.git` from before the transfer to
the org. GitHub redirects that; Greptile does not. Every command kept failing
with `this repository is not connected to Greptile yet` — an error that names
the wrong cause. Fixed with:

```bash
git remote set-url origin git@github.com:Heiwa-Limited/heiwa-universe.git
```

**Still outstanding:** in Organization Settings → Billing, set **Flex Usage
Limit = $0**. Billing is $30/seat/month including 50 credits, $1/credit beyond,
and the cap is the only thing that makes an accident impossible.

## 4a. Verify the config is actually live — do this after every change

```bash
greptile config
```

This is not a formality. The config surface fails silently in at least three
ways, all found on Day 1:

- **Wrong file.** If `.greptile/` and `greptile.json` both exist, `.greptile/`
  wins and `greptile.json` is *ignored entirely* — not merged. Shipping both
  left the whole tuning layer dead with no warning.
- **Wrong key name.** `includeSequenceDiagram`, `includeIssuesTable`, and
  `includeConfidenceScore` are not in the schema; the `…Section` objects are.
  Unrecognized keys are discarded without complaint.
- **Wrong remote.** See above.

Read the output and confirm each setting you care about appears. A committed
setting is not a live setting.

Known display gap: the inline `rules` array in `.greptile/config.json` does not
show up in this output — only org-level rules do. `rules.md` and `files.json`
both appear and are confirmed loaded. The standards are duplicated across both
files on purpose, so behavior is covered either way.

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
`.greptile/config.json` (which only governs the PR bot). Oversized reviews are refused,
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
git rm -r --cached .greptile/ && claude mcp remove greptile && npm uninstall -g greptile
```

Then uninstall the GitHub App from the org and revoke the API key. Keep
`ops/greptile-trial/` — the scorecard is the record of why, and the baseline if
you re-trial after fixing PR size.
