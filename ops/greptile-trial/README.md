# Greptile Bot-First Trial

The trial evaluates Greptile's GitHub App as a continuous PR reviewer. The
Greptile CLI, API, and MCP lanes are out of scope and have no API key.

## Capacity Truth

- `greptile-apps` performs reviews under its GitHub App installation. On
  2026-08-08, the org API verified that installation was active, not suspended,
  and selected all Heiwa-Limited repositories.
- Deleting a Greptile API key does not disable the GitHub App.
- Current public pricing assigns one credit to a standard review and three to a
  TREX review. The account dashboard is authoritative for the remaining balance.
- A bot comment proves review activity. It does not by itself prove the exact
  remaining credit balance.
- `triggerOnUpdates: false` deliberately freezes each PR's Greptile view at its
  first reviewed head. No review after a later push is behavioral confirmation
  of that setting, not a liveness failure. The tradeoff is that fixes are never
  re-checked automatically.

## Day 7 Checkpoint

1. Do not run `greptile config`; without a key it tests an out-of-scope lane and
   an auth failure says nothing about GitHub App health.
2. Read Greptile's PR summary and inline findings with authenticated `gh` or the
   GitHub UI.
3. Capture the `Rule Used:` line when present. Treat it as real but weaker
   configuration evidence than an authenticated Greptile config endpoint.
4. Run the counterfactual review with Codex against the same diff.
5. Compare actionable findings, false positives, overlap, severity, and time to
   resolution. Do not reward comment count.

Example read-only extraction once `gh auth status` succeeds. Inline findings
are pull-review comments, not issue comments:

```bash
gh api "repos/OWNER/REPO/pulls/PR_NUMBER/comments" --paginate \
  --jq '.[] | select(.user.login | ascii_downcase | contains("greptile")) | {url: .html_url, path, line, body}'
gh pr checks PR_NUMBER -R OWNER/REPO
```

The first authenticated evidence capture is recorded in
[`verification-2026-08-08.md`](verification-2026-08-08.md).

## Day 13 Decision and Teardown

If the trial is declined:

1. Disable reviews for the repository in Greptile or uninstall
   `greptile-apps` from the GitHub organization.
2. Confirm the org installation API and GitHub settings no longer list the App.
   An update to an existing PR cannot prove teardown because
   `triggerOnUpdates: false` already suppresses that review.
3. Record the final credit count and accepted/false-positive findings.

Removing an OAuth authorization or API key is not the off-switch for the
installed GitHub App. Manage repository access through
<https://github.com/apps/greptile-apps> and review settings through the Greptile
dashboard.
