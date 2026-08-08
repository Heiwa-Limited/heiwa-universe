# Greptile 13-day trial — 2026-08-07 → 2026-08-19

Owner: Devon. Repo: `Heiwa-Limited/heiwa-universe`.
Decision date: **2026-08-19 (Wed)**. Mid-point recalibration: **2026-08-13 (Thu)**.

## Why this trial is not the usual one

Greptile's marketed value is replacing human reviewer bandwidth on a team. This
repo has no team — `git log` over the last 120 days is 108 commits, all Devon
(`Strategizing` + `Devon`, same person, two identities). Every non-Devon PR is a
bot. So the standard pitch does not apply and the standard metrics
(review turnaround, reviewer hours saved) measure nothing here.

What is actually being tested is narrower and sharper:

> Heiwa already routes work to Claude Code, Codex, and Gemini CLI, with Grok
> and Ollama wrapped at varying depth and Antigravity still an authenticated
> interactive executor rather than a headless adapter. Those agents write the
> code **and** currently self-review it.
> Self-review by the authoring agent is structurally weak: it shares the
> author's context, its blind spots, and its assumptions. Does an independent
> reviewer with a whole-repo graph catch things the authoring agent
> demonstrably did not?

That is falsifiable in 13 days. Everything below is built to answer it.

**Primary metric — novel-catch rate.** Of the findings Greptile posts, what
fraction are (a) real, and (b) *not* already raised by the agent that wrote the
code in its own handoff? A finding that is real but the authoring agent already
flagged is worth ~nothing here, because you already had it for free.

**Kill criterion.** Fewer than 3 novel, real, high-or-medium findings over 13
days ⇒ do not convert. State that up front so the decision is not made on vibes
at the end.

## Known risks going in

**PR size.** Recent PRs run 87 / 170 / 616 / 1169 changed files. Any reviewer,
human or model, degrades to skimming at that size. `fileChangeLimit` is set to
150, so the consolidation PRs will be *skipped, not badly reviewed* — that is
deliberate. It also means the trial only gets signal if some normal-sized PRs
land during it. Plan for that (see Day 2).

**Overlap with existing CI.** Trivy, clippy across three OSes, biome, and
Dependabot already run. Config sets `commentTypes: ["logic","syntax"]` and the
instructions explicitly forbid style/lint/CVE findings. If Greptile still
reports them, that is itself a finding — it means the config surface is weaker
than advertised.

**Credits.** $30/seat/mo including 50 credits, $1/credit beyond. One active
developer. `triggerOnUpdates` is `false`, so each PR costs 1 credit on open, not
1 per push. Expected trial burn: well under 20. **Set the flex limit to $0** in
Organization Settings → Billing so an accident cannot bill.

**Branch model is mid-migration.** PR #52 is codifying a two-branch dev→main
flow and has been open since 2026-07-30. `includeBranches` covers both `main`
and `dev`. CI itself still only triggers on PRs to `main` — worth reconciling
when #52 lands, independently of this trial.

---

## Schedule

### Day 1 — 2026-08-07 (Fri) — install and cold-start

1. Install the GitHub app on `Heiwa-Limited` and grant it `heiwa-universe` only.
2. Organization Settings → Billing → **Flex Usage Limit = $0**.
3. Commit the config that is already written into the working tree:
   `.greptile/rules.md`, `.greptile/config.json`,
   `.greptile/files.json`. Commit these paths *only* — the tree has unrelated
   uncommitted work in it right now.
4. Install the CLI and the MCP server (commands in `SETUP.md`).
5. Let it index the repo. It is a ~3.5MB polyglot tree (Rust-dominant, plus
   Python, TypeScript, Shell, HCL); indexing is not instant.

**Cold-start honesty:** Greptile's own docs say the learning system needs 2–3
weeks of feedback to adapt. A 13-day trial does not reach that. So judge Day
1–13 as *floor* performance, not steady state, and weight the trend across the
two halves more than the absolute Day-1 quality.

### Day 2 — 2026-08-08 (Sat) — backfill, and make sure there is signal to read

The trial fails silently if no reviewable PRs land in 13 days. Two moves:

- **Backfill the open work.** Comment `@greptileai` on PR
  [#52](https://github.com/Heiwa-Limited/heiwa-universe/pull/52) (87 files —
  under the limit, so it will review). This is the single best available test
  case: it touches CI, branch flow, and DREX goldens at once.
- **Retro-review merged history with the CLI.** No PR needed, and it produces
  ground truth you can check, because you already know what broke afterward:

  ```bash
  git checkout -b trial/retro-51 8435983
  greptile review --branch main --json > ops/greptile-trial/raw/retro-51.json
  ```

  Do this for PR #51 (operator stream + auth hardening, 170 files) and PR #49
  (agent configs + monitor read model, 8 files). If Greptile flags something
  you later had to fix by hand, that is the strongest possible evidence — and
  it is available on day 2 instead of day 13.

- **Change PR habit for 13 days.** Break work into PRs under ~150 files. This
  is worth doing regardless of the Greptile outcome; it is the precondition for
  *any* reviewer, agentic or human, to be useful to this repo.

### Days 3–6 — 2026-08-09 → 08-12 — run live, log everything

For every PR: before opening it, save the authoring agent's own self-review /
handoff notes to `ops/greptile-trial/raw/pr-<N>-agent.md`. Then open the PR and
let Greptile review.

Score each Greptile comment in `scorecard.md` with one tag:

| Tag | Meaning |
| --- | --- |
| `NOVEL` | Real problem, authoring agent did not raise it. **This is the metric.** |
| `DUP-AGENT` | Real, but the authoring agent already called it out. |
| `DUP-CI` | Real, but clippy/Trivy/biome would have caught it. Config failure. |
| `WRONG` | Factually incorrect about the code. |
| `NOISE` | Technically true, not worth acting on. |

Use 👍/👎 on every comment as you tag it. That is what trains the learning
system, and 13 days of feedback is the only shot at seeing any adaptation.

### Day 7 — 2026-08-13 (Thu) — mid-point recalibration

**This is a tuning checkpoint, not a decision point.** Read the scorecard and
make exactly one class of change, then hold it fixed for the second half so the
halves are comparable:

- `NOISE` > 40% → raise `strictness` to 3, or narrow `commentTypes` to
  `["logic"]`.
- `NOVEL` ≈ 0 but nothing is wrong either → *lower* `strictness` to 1. Silence
  can mean the threshold is too high, not that the code is clean.
- `DUP-CI` appearing at all → tighten `instructions`; note that the config
  surface underdelivers.
- `WRONG` clustering in one area → that area's context is missing. Add the file
  to `.greptile/files.json`.

Record what changed and why, in the scorecard. Then leave it alone until Day 13.

### Days 8–12 — 2026-08-14 → 08-18 — second half + the counterfactual

Same logging. Plus the experiment that actually settles the question:

**The blind counterfactual.** On one substantive PR, run all three, in this
order, without letting any see the others' output:

1. The agent that wrote the code self-reviews its diff.
2. A *different* provider in the stack (Codex if Claude wrote it, or the
   reverse) reviews the same diff cold.
3. Greptile reviews the PR.

Then diff the three finding sets into `ops/greptile-trial/raw/counterfactual.md`.

This separates two things the trial would otherwise confuse. If (2) catches
most of what (3) catches, the value was **independence** — and you can get that
for free by routing review to a non-authoring provider, which Heiwa is already
built to do. If (3) catches things (2) missed, the value was the **whole-repo
graph**, which your local agents genuinely do not have, and which is worth
paying for.

That distinction is the whole trial. Do not skip this step.

### Day 13 — 2026-08-19 (Wed) — decide

Fill in the verdict block in `scorecard.md`. Export the Greptile analytics
dashboard (PRs reviewed, addressed rate, comment sentiment, critical bugs
caught) and file it next to the hand-scored data — then check whether the
vendor's numbers and yours agree. If addressed-rate reads high while your
`NOVEL` count is near zero, the dashboard is measuring your politeness, not its
value.

**Convert if:** ≥3 `NOVEL` high/medium findings, *and* the counterfactual shows
Greptile beating a cold non-authoring provider. $30/mo against one prevented
evidence-plane corruption or one leaked credential is trivially worth it.

**Do not convert if:** the novel catches are all reproducible by pointing Codex
at a diff Claude wrote. Then the correct outcome is not a subscription — it is
a `heiwa` routing rule that sends every diff to a non-authoring provider, which
is a better fit for the local-first thesis anyway and costs nothing.

**Genuine third option:** Greptile is right but the PRs are too big for it to
matter. If that is the read, the finding is about PR hygiene, not the vendor.
Fix that first and re-trial later.

---

## Note on the API key

The key was pasted in plaintext into a chat transcript. Treat it as exposed:
rotate it at app.greptile.com after setup, keep the new one in
`crates/heiwa_vault` or the OS keychain, and export it from there rather than
putting it in `.mcp.json` or any tracked file. `.mcp.json` is tracked in this
repo — the MCP entry must use `${GREPTILE_API_KEY}`, never a literal.
