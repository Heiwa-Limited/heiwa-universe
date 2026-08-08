# Counterfactual — PR #52, run 2026-08-08

The experiment PLAN.md calls the crux, run early because PR #52 was a better
test case than anything likely to land later: 87 files, authored in July by
Devon's agents for their own reasons, not written for this trial.

## Method

Same diff, `origin/dev` vs `origin/main`, reviewed twice.

- **Greptile** — GitHub bot, triggered with `@greptileai`, using `.greptile/`
  config: strictness 2, `commentTypes` logic+syntax, 10 scoped rules, 6 context
  files including `HEIWA.md` and `AGENTS.md`.
- **Codex** — `codex exec review --base origin/main`, run in a detached
  worktree at `.worktrees/claude/cf-52`. `--base` refuses a custom prompt, so
  this ran on Codex's **own default review instructions with no steer at all**
  and no repo context beyond the checkout. That is the coldest possible form,
  and it disadvantages Codex relative to Greptile's tuned setup.

Codex had not seen Greptile's comments. Neither agent authored the diff.

Raw output: `counterfactual-codex-52.txt`.

## Result

| # | Finding | Greptile | Codex | Verified |
| --- | --- | --- | --- | --- |
| 1 | Second private `state_dir()` in `cmd/auto.rs` omits the `HEIWA_STATE_DIR` check that `cmd/app.rs` honors, so every `auto` subcommand drains a different queue than the daemon | **P1** | *missed* | yes |
| 2 | A claim that dies before completion strands the row in `running` forever; drains select only `pending`, and no reaper exists | **P1** | P2 | yes |
| 3 | `ApprovalRequested` has no arm in the automation runner's match, so an approval-gated tool call blocks `approvals.wait` and stalls the sequential drain behind it | *missed* | **P1** | yes |

**Overlap: 1 of 3. Each found exactly one the other missed. All three are real.**

Finding 3 was verified to the same standard as Greptile's:
`operator.rs:1198` emits `ApprovalRequested`, then stages and blocks in
`spawn_blocking(|| approvals.wait(...))`; `main.rs:4222` matches only
`AssistantCompleted`, `TurnCompleted`, and `Blocker`. `run_pending` iterates
`for row in pending` sequentially, so the queue stalls behind the blocked row.
Unlike finding 2 it needs no crash — it fires on the normal path whenever an
automation touches an approval-gated tool. Arguably the most severe of the three.

On finding 2 the two disagreed on severity and on where to anchor it: Greptile
at the claim call site in `executor/mod.rs:215`, Codex at the claim commit in
`storage/mod.rs:279`. Greptile's P1 is the better read — a permanently wedged
automation queue is not a P2.

## What this means

**Greptile is not dominant here.** A cold, unsteered, already-paid-for provider
matched it on one finding and beat it on another. The `$30/seat/month` question
is therefore not "does Greptile find real bugs" — it clearly does, two verified
P1s — but "does it find enough that Codex does not, to be worth the money and
the second surface?" On this diff the honest answer is one finding out of three.

**They are complementary, not redundant.** Either alone caught 2 of 3; together
they caught 3. If the goal is defect detection rather than vendor selection,
running both is strictly better than either — and running Codex is free.

**The whole-repo-graph claim is only partly supported.** Greptile's unique find
(finding 1) is squarely in that category: two same-named private functions in
sibling modules diverging on an env check, invisible to any per-file view. But
Codex's unique find is also cross-file — an unhandled event variant whose
consequence lives two modules away. So "sees across files" does not by itself
separate the two products.

**The custom rules did not drive these.** All three findings are generic
correctness. None cites one of the 10 scoped rules in `.greptile/config.json`,
unlike the PR #54 findings, which cited `no-secrets-in-tree` and
`no-maturity-overstatement` explicitly. The elaborate rule set has so far
proven itself on prose and paperwork, not on Rust. Worth watching: it may be
that the rules mostly matter for conventions a general reviewer would not guess.

## Caveats

**n = 1.** One diff, one run each, no reruns to test variance. Both tools are
non-deterministic; a second pass might reorder the whole table. This is
suggestive, not settled, and the Day 13 verdict should say so.

**The comparison favored Greptile on setup and still came out even.** Greptile
had tuned config, scoped rules, and six context files; Codex had its defaults
and nothing else. If anything that understates Codex.

**Greptile has not had its learning loop.** Its docs claim 2–3 weeks of
feedback adaptation. This is floor performance.

## Consequence for the Day 13 decision

The pre-registered kill criterion — 3 novel real high/medium findings — is met
on Greptile's side (4 on PR #54, 2 on PR #52). But the criterion was written
before there was a measured alternative, and it now reads as the wrong test. It
asks whether Greptile is *useful*, and the answer is plainly yes. The question
that decides the money is whether it is useful **beyond a non-authoring
provider Devon already runs**, and that margin currently measures at one
finding in three on a single diff.

Do not settle this on n=1. The right move is a second counterfactual on the
next substantive PR that lands. If Greptile's unique-find rate holds near a
third, converting is defensible on complementarity alone. If it drops toward
zero, the correct outcome is not a subscription but a `heiwa` routing rule that
sends every diff to a provider other than the one that authored it — which is
the local-first answer and costs nothing.
