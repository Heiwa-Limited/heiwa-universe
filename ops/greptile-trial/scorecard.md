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
| 08-07 | #54 | `ops/greptile-trial/SETUP.md:84` | P2 | Teardown uses `git rm --cached`, which clears only Git's index and leaves unignored copies in the working tree for local tooling to read and a later bulk stage to re-add | `NOVEL` | yes | Bot only. Correct, and supplied the right form. Teardown now uses `git rm -r` and leads with uninstalling the GitHub App, which is the actual off-switch |

### PR #52 — the first review on real code

Two P1s, 87 files, bot surface. **Both verified against the source before
logging.** These are the first findings on Rust written by Devon's agents
rather than on trial paperwork, and they are the trial's first genuine
evidence.

| Date | PR | File:line | Severity | Greptile's claim | Tag | Real? | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 08-08 | #52 | `apps/heiwa_shell/src/cmd/auto.rs:335` | P1 | `auto tick` derives `~/.heiwa/state` while the app daemon honors `HEIWA_STATE_DIR`, so the CLI drains a different queue than the one in use | `NOVEL` | **yes, verified** | `app.rs:4877` reads `HEIWA_STATE_DIR` and falls back to `~/.heiwa/state`. `auto.rs:389` defines a **second, private `state_dir()`** in a sibling module of the same crate that omits the env check entirely. Both functions are individually correct; only the pair is wrong. Note `tests/smoke.rs`, `tests/local_boot.rs`, and `tests/operator_api.rs` all set `HEIWA_STATE_DIR` |
| 08-08 | #52 | `crates/heiwa_automations/src/executor/mod.rs:215` | P1 | A claim that dies before completion strands the row in `running` forever, because later drains select only `pending` | `NOVEL` | **yes, verified** | `storage/mod.rs:245` `list_pending_executions` is `WHERE status = 'pending'`; `:258` `claim_pending_execution` flips the row to `Running`. Grepped the whole crate for `reap`/`requeue`/`stale_running` — **no recovery path exists**, no lease timeout, no startup reconciliation. The doc comment above `run_pending` reasons carefully about double-execution and is silent on the crash case. `started_at` is already set at claim time, so a lease-expiry reaper is the natural fix |

**Why these two matter more than the four on PR #54.** Neither is reachable by
a per-file linter, and that is not a judgment call — it is structural. The
first requires noticing that two same-named private functions in sibling
modules of one crate diverge on an env var. The second requires holding the
SQL, the drain loop, and the *absence* of a reaper in mind simultaneously.
Clippy cannot express either; both functions and every individual statement are
locally correct. This is the whole-repo-graph claim doing exactly what it says
on the tin.

That is 2 novel P1s on real code, against a kill criterion of 3 across 13 days.

### Re-review on push is off by design, and its absence is useful evidence

Independent verification on 2026-08-08 found that PR #54's head advanced with
no newer Greptile check, and recorded automatic re-review as unproven. It is
not unproven — it is **switched off**. `triggerOnUpdates: false` in
`.greptile/config.json` is exactly what suppresses re-review on push, and it is
the setting the whole credit argument rests on: one credit per PR opened rather
than one per push.

That reframes a null result into a positive one. Since the API key was deleted,
`greptile config` can no longer be run, so there is no direct way to confirm the
committed config is live. The absent re-review is now **independent proof that
`.greptile/config.json` is being read and honored** — a behavior that only
happens if the file loaded. Together with the `Rule Used:` citations in the PR
#54 comments, that is two separate confirmations from different mechanisms.

The cost is real though: with re-review off, **a finding fixed on a later push
is never re-checked**. Greptile's view of PR #54 is frozen at the first head.
Any "did the fix work" question has to be answered by hand for the rest of the
trial.

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

## Counterfactual — run early, 2026-08-08, on PR #52

Full writeup and verification: `raw/counterfactual.md`. Run on Day 2 rather
than Days 8–12 because PR #52 was a better test case than anything likely to
land later — 87 files, authored in July by Devon's agents, not for this trial.

| Finding | Cold non-authoring provider (Codex) | Greptile | Verified |
| --- | --- | --- | --- |
| Split `state_dir()` ignoring `HEIWA_STATE_DIR` | *missed* | **P1** | yes |
| Claimed executions stranded in `running`, no reaper | P2 | **P1** | yes |
| `ApprovalRequested` unhandled, blocks the sequential drain | **P1** | *missed* | yes |

Findings only Greptile had: **1**
Findings the cold provider also had: **1**
Findings only the cold provider had: **1**

Codex ran with `--base`, which refuses a custom prompt, so it had **no steer and
no repo context** while Greptile had tuned config, 10 scoped rules, and 6
context files. The comparison favored Greptile on setup and still came out even.

**Read: substantially independence, not uniquely the graph.** Greptile's unique
find is genuinely cross-module and a per-file linter could never produce it —
but so is Codex's, and Codex is already paid for. The two are complementary
(either alone caught 2 of 3; together 3 of 3), which is a real argument for
running both, and a weak one for paying for one.

**n = 1.** Single diff, single run each, both tools non-deterministic. Rerun on
the next substantive PR before the Day 13 verdict leans on this.

**The kill criterion is met but is now the wrong test.** It asks whether
Greptile is useful — plainly yes, 6 novel real findings including 2 verified
P1s on live Rust. The question that decides the money is whether it is useful
*beyond a non-authoring provider Devon already runs*, which currently measures
at one finding in three.

## Day 13 verdict — 2026-08-19

Novel high/medium findings: ____ (kill criterion: <3 ⇒ do not convert)

Best single catch — the one thing that most justifies $30/mo:

Worst miss — something you found by hand that Greptile reviewed and did not flag:

Greptile's own analytics export vs. this hand-scored log — do they agree?

**Decision:** convert / decline / re-trial after fixing PR size

**If declining:** what routing rule replaces it in `heiwa`?
