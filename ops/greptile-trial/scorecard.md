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

- Repo connected, first review completed. **Trial is live as of Day 1**, not
  Day 2 as planned.

### Findings from setup, before Greptile reviewed a line

Five came out of standing the thing up. Two of them change how the trial should
be read; two were self-inflicted and are worth recording because they are the
kind of silent failure that would have invalidated the whole exercise.

**1. The CLI has a hard file cap well below anything tunable.** A review of 760
files was refused outright: `error: this review touches 760 files. Split it
into smaller commits and try again.` This is independent of `fileChangeLimit`
in `.greptile/config.json`, which only governs the PR bot. It confirms that PR size is
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

**4. The CLI identifies the repo by git remote URL, and ours was stale.** After
the GitHub App was installed on the org, every command still failed with `this
repository is not connected to Greptile yet`. The App was installed correctly —
`gh api orgs/Heiwa-Limited/installations` listed `greptile-apps` — but `origin`
still pointed at `git@github.com:Strategizing/heiwa-universe.git` from before
the repo moved to the org. GitHub redirects that transparently; Greptile does
not. The error names the wrong cause: it reads as "you have not connected this
repo" when the truth is "the name I resolved does not match the one you
connected." Fixed with `git remote set-url origin
git@github.com:Heiwa-Limited/heiwa-universe.git`. Anything else keyed on the
remote URL was silently resolving to the old name too.

**5. Shipping both `greptile.json` and `.greptile/` silently voided the entire
config.** The first `greptile config` against the connected repo showed
`strictness: 1` (config said 2), `commentTypes: logic, syntax, style` (config
excluded style), `Filters: (none)` despite four filters being set, and no
instructions. The docs are explicit: *"If both `.greptile/` and `greptile.json`
exist in the same directory, `.greptile/` takes precedence and `greptile.json`
is ignored."* Not merged — ignored. No warning, no parse error, no diagnostic.
The tuning layer was completely dead and the trial would have measured stock
defaults while the scorecard claimed otherwise.

The same silent-drop applies to key names. `includeSequenceDiagram`,
`includeIssuesTable`, and `includeConfidenceScore` are not in the schema; the
`sequenceDiagramSection` / `issuesTableSection` / `confidenceScoreSection`
objects are. The unrecognized spellings were discarded without complaint.

**Standing lesson:** this config surface fails silently in at least three ways
(wrong file, wrong key name, wrong remote). `greptile config` is the only thing
that tells you the truth. Run it after every config change, and never assume a
committed setting is a live setting.

**Resolved — inline rules are applied, just not displayed.** `greptile config`
reports only the two org-level rules and never lists the `rules` array from
`.greptile/config.json`, which looked like it might not be parsed. The GitHub
bot review settled it: its comments carry a `**Rule Used:**` line quoting
`config.json` verbatim — "No credentials, tokens, API keys, or private
endpo…" (`no-secrets-in-tree`) and "Docs, comments, log strings, and
user-facing messa…" (`no-maturity-overstatement`), each attributed to
`([source](.greptile))`. Those are the config.json strings, not the `rules.md`
prose, which phrases both differently. So the rules work; `greptile config`
just under-reports. Do not use its rule count as a health check.

## Findings log

Tagging convention note: findings 1–3 are Greptile reviewing a diff **I** wrote
(the trial config itself). That makes me the authoring agent, so NOVEL here
means "the agent that wrote this did not catch it before submitting" — the same
test the trial applies to Devon's agents, just at small scale and on
docs/config rather than Rust. Treat it as a cold-start sanity check, not as
evidence about the tool's value on the repo's actual substance.

| Date | PR | File:line | Severity | Greptile's claim | Tag | Real? | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 08-07 | #54 | `ops/greptile-trial/raw/run-review.sh:9` | P1 | `shift 3 \|\| true` does not shift when fewer than 3 args are passed, so the label and workdir are forwarded to `greptile review` as stray positionals | `NOVEL` | yes | Correct, and the documented 2-arg form was the failing case. Supplied a working fix. Real bug in code I wrote and did not catch. Fixed in `f6997df`+ |
| 08-07 | #54 | `ops/greptile-trial/SETUP.md:49` | P2 | Setup names `greptile.json` as committed, but the PR contains no such file; PLAN and teardown repeat the stale reference | `NOVEL` | yes | Correct. I removed the file one commit earlier and left five references behind. Caught the inconsistency across three files. Fixed |
| 08-07 | #54 | `ops/greptile-trial/PLAN.md:16-17` | P2 | Claims Heiwa routes work to all five named providers; Antigravity is an authenticated interactive executor, not a headless adapter | `NOVEL` | yes | Correct per `AGENTS.md`. This is the repo's own maturity-overstatement rule firing against my prose — the exact class of finding the rules were written to catch. Fixed |
| 08-07 | #54 | `ops/greptile-trial/SETUP.md:25-27` | **P1 security** | The key-handling step appends the rotated API key literally to `~/.bashrc`, exposing it to backups and dotfile sync, contradicting the vault contract stated later in the same file | `NOVEL` | yes | **Found by the GitHub bot, not the CLI.** Correct and self-contradiction-aware — it caught the doc arguing against itself two sections apart. Rewritten to store in the OS keychain via `secret-tool` and export a lookup rather than a secret |

### The two surfaces do not return the same findings

Same diff, same config, same day. The CLI returned the `shift 3` bug, the stale
`greptile.json` references, and Antigravity. The GitHub bot returned the
`~/.bashrc` credential and Antigravity. Overlap: one of four.

Do not treat `greptile review` and the PR bot as interchangeable. For the rest
of the trial, run both on any PR that matters, and log which surface produced
each finding — if that asymmetry holds, it is a material fact about the product
and belongs in the Day 13 verdict. The bot's unique find was also the most
severe of the four, which is the opposite of what "the CLI is the power-user
surface" would predict.

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
