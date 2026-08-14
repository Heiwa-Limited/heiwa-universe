# Greptile Verification — 2026-08-08

## Acquired Data

- WSL `gh` is authenticated as `Strategizing` with private-repository and
  organization-read access.
- `greptile-apps` is installed on Heiwa-Limited with `repository_selection=all`
  and `suspended_at=null`.
- [PR 52](https://github.com/Heiwa-Limited/heiwa-universe/pull/52) has a
  successful `Greptile Review` check completed at 2026-08-08T00:14:00Z and two
  P1 inline findings.
- [PR 54](https://github.com/Heiwa-Limited/heiwa-universe/pull/54) has a
  Greptile review submitted at 2026-08-07T23:44:40Z with two P1 findings and
  one P2 finding. At least two comments include a `Rule Used:` line.
- `.greptile/config.json` sets `triggerOnUpdates: false`. PR 54's head advanced
  after its first review without another Greptile check. That is the configured
  behavior: one review on PR open rather than another credit-consuming review
  after each push.

## Interpretation

The absent re-review is positive behavioral evidence that
`triggerOnUpdates: false` is live, not evidence of an unknown trigger state.
The `Rule Used:` citations independently confirm that the same committed
configuration supplied Greptile's review rules.

The cost is frozen review state. A finding fixed on a later push is not
re-checked by Greptile, so confirmation that the fix landed is manual for the
rest of this trial.

## Missing Data

- Exact remaining Greptile credits are not exposed through GitHub.
- GitHub evidence does not confirm whether fixes for first-head findings landed
  on a later head; that requires a manual diff or independent review.

## Needed Data

- A read of the Greptile dashboard's current credit balance at the Day 7 and
  Day 13 checkpoints.
- Manual disposition of each finding as fixed, open, duplicate, or false
  positive against the current PR head.

## Executable Next Action

At Day 7, record the dashboard balance and manually compare each first-head
finding with the current PR head. Do not use `greptile config`; that probes the
removed API-key lane. Keep `triggerOnUpdates: false` during the trial so the
credit model remains one standard review per opened PR.

## Verification Commands

```bash
gh api /orgs/Heiwa-Limited/installations \
  --jq '.installations[] | select(.app_slug == "greptile-apps") | {repository_selection, suspended_at}'
gh api 'repos/Heiwa-Limited/heiwa-universe/contents/.greptile/config.json?ref=ops/greptile-trial' \
  --jq .content | base64 -d
gh pr checks 52 -R Heiwa-Limited/heiwa-universe
gh api "repos/Heiwa-Limited/heiwa-universe/pulls/52/comments" --paginate \
  --jq '.[] | select(.user.login | ascii_downcase | contains("greptile")) | {url: .html_url, path, line, body}'
```
