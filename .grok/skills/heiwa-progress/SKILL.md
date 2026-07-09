---
name: heiwa-progress
description: Advance Heiwa product development efficiently using multi-provider inference while respecting planes, quality bars, and local-first doctrine. Use when building features, unblocking work, or the user says progress Heiwa / keep shipping.
argument-hint: "[area or goal]"
user-invocable: true
---

# /heiwa-progress — ship Heiwa with efficient inference

You are building **Heiwa**: local-first digital OS (Intake → Execution → Evidence). Desktop first; mobile later. Inference providers stay external (BYOK / CLI / local).

## Inference routing (cheapest sufficient)

Prefer in this order unless the user pins a model:

1. **Local** (Ollama / free local) for classify, plan drafts, small edits, docs
2. **Free OpenRouter / Nous portal keys** for mid tasks
3. **Subscription CLIs** (Claude Pro, ChatGPT Plus/Codex, SuperGrok) for hard reasoning / large refactors
4. **Paid API** only when blocked or quality requires it

Never put secrets, full mail bodies, or SSH keys into remote prompts.

## Working rules

1. Read `HEIWA.md` + nearest `AGENTS.md` before large changes.
2. Classify every change: **Intake / Execution / Evidence** (or cut).
3. Prefer contracts (schemas, policy IR, connectors) over one-off glue.
4. Fail closed on sandbox / leases.
5. Keep monorepo work on **Linux FS** in WSL (`~/heiwa`), never `/mnt/c` cargo builds.
6. Run the smallest meaningful test (`cargo test -p <crate>`).
7. Update `~/.heiwa/workspace/LEARNINGS.md` only for non-obvious durable lessons (or repo docs).

## Progress loop

1. State the goal in one sentence.
2. Name plane + crate(s).
3. Implement the smallest vertical slice.
4. Test.
5. Summarize what shipped and the next 1–3 steps.

$ARGUMENTS
