# Greptile trial scorecard

Fill this in as you go. Tags: `NOVEL` / `DUP-AGENT` / `DUP-CI` / `WRONG` / `NOISE`
(defined in `PLAN.md`). `NOVEL` is the only one that counts toward the decision.

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
