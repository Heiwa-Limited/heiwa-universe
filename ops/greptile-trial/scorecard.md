# Greptile trial scorecard

Fill this in as you go. Tags: `NOVEL` / `DUP-AGENT` / `DUP-CI` / `WRONG` / `NOISE`
(defined in `PLAN.md`). `NOVEL` is the only one that counts toward the decision.

## Day 1 log — 2026-08-07

Setup got as far as it can without Devon's browser. What was verified:

- CLI 3.3.1 installed via npm (node 26.5.0). `greptile whoami` authenticates as
  `devonrmcgregor@gmail.com`, org **Heiwa Limited**
  (`2f505953-89ca-401e-b6e5-00148157cfa9`). The account and org exist.
- Config committed on branch `ops/greptile-trial`, opened as
  [PR #54](https://github.com/Heiwa-Limited/heiwa-universe/pull/54).
- Telemetry disabled via `GREPTILE_TELEMETRY_DISABLED=1` (the CLI shares
  anonymous usage data by default; it states never code or repo contents).

**Blocked on:** `greptile config` and `greptile review` both fail with
`this repository is not connected to Greptile yet`. The only paths through are
the dashboard or `greptile onboard`, an interactive browser OAuth wizard that
installs a GitHub App on the org. That is Devon's step. Nothing else in the
trial can start until it is done.

### Findings before Greptile reviewed a single line

Three came out of setup itself. They are not Greptile findings — Greptile has
not run — but two of them change how the trial should be read.

**1. The CLI has a hard file cap well below anything tunable.** A review of 760
files was refused outright: `error: this review touches 760 files. Split it
into smaller commits and try again.` This is independent of `fileChangeLimit`
in `greptile.json`, which only governs the PR bot. It confirms that PR size is
a **wall, not a tuning knob** — PRs #48 (1169 files) and #50 (616 files) could
not have been reviewed by this tool at any setting. If work keeps arriving in
that shape the trial cannot produce signal, and the honest conclusion will be
about PR hygiene rather than about Greptile.

**2. The local checkout is 762 files behind origin.** Local `main` sits at
`ce43e88` (PR #49 merge); `origin/main` is at `b90c6c3` (PR #51 merge). The
first review ran against stale local `main` and produced a 762-file diff that
was really PRs #50 and #51 in reverse. `greptile review` resolves its base
against the *local* ref by default, so `--branch origin/main` is mandatory here
until the checkout is current. Worth fixing regardless of this trial: the WSL
working tree also carries ~70 uncommitted files.

**3. `greptile skills` ships an agentic review loop.** Two bundled skills,
installable to `.claude/skills/` and `.codex/skills/`: `greptile-cli` (commands,
JSON shapes, exit codes) and **`greploop`** — review the branch, fix findings,
review again, until clean. This is the closest thing the product has to a
Heiwa-native integration and it is not in the published docs index. Evaluate it
during Days 8–12 alongside the counterfactual: an autonomous fix-review loop is
a different value proposition from PR comments, and a more interesting one for
this repo. Do not install it until after the counterfactual is recorded — it
would contaminate the comparison by letting the authoring agent see Greptile's
findings mid-review.

## Findings log

| Date | PR | File:line | Severity | Greptile's claim | Tag | Real? | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| | | | | | | | |

## Running tally

| Tag | Days 1–7 | Days 8–13 | Total |
| --- | --- | --- | --- |
| `NOVEL` | | | |
| `DUP-AGENT` | | | |
| `DUP-CI` | | | |
| `WRONG` | | | |
| `NOISE` | | | |
| **Total comments** | | | |

Credits consumed: ____ / 50. Reviews skipped by `fileChangeLimit`: ____.

## Day 7 recalibration — 2026-08-13

What the first half showed:

Single change made (and why):

Prediction for the second half — write it down *before* Day 8, so Day 13 cannot
rationalize whatever happened:

## Counterfactual — Days 8–12

PR used: #____

| Finding | Authoring agent | Cold non-authoring provider | Greptile |
| --- | --- | --- | --- |
| | | | |

Findings only Greptile had: ____
Findings the cold provider also had: ____

Read: was the value **independence** (reproducible free, via Heiwa routing) or
the **whole-repo graph** (not reproducible locally)?

## Day 13 verdict — 2026-08-19

Novel high/medium findings: ____ (kill criterion: <3 ⇒ do not convert)

Best single catch — the one thing that most justifies $30/mo:

Worst miss — something you found by hand that Greptile reviewed and did not flag:

Greptile's own analytics export vs. this hand-scored log — do they agree?

**Decision:** convert / decline / re-trial after fixing PR size

**If declining:** what routing rule replaces it in `heiwa`?
