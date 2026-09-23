# System 1 shadow judgment in Work-scoped turns

Date: 2026-09-23
Plane: Execution (judgment) / Evidence (shadow records and report)
Branch: `experimental/system1-jev-engine`
Builds on: `packages/heiwa_system1/architecture.md`, `crates/heiwa_judgment`

## Question this answers

Would a System 1 classifier (TypeSafe Jev, or a capped local fallback) have
routed Work-scoped operator turns differently from the deterministic rules,
and when it disagrees, do the turns that actually ran show signs of trouble?

Shadow mode records a recommendation and its counterfactual route. It never
changes execution. Promotion to an executing policy is a later, separate
decision that needs the evidence this produces.

## What the deterministic baseline is today

- Intent: keyword rules (`heiwa_protocol::classify_intent`) produce one of
  `chat, build, deploy, audit, research, strategy, status_check`.
- Capability floor: `apply_route_policy` overwrites DREX's inferred
  `minimum_quality_class` with the turn's route policy, which defaults to `1`.
  DREX's own `code -> 3` rule cannot fire for operator turns, whose keyword
  intent key is `build`, never `code`.
- Selection: `heiwa_core::drex::plan_model_call`, cheapest admitted candidate.

So for a default operator turn the floor is always 1 and DREX picks the
cheapest model. The shadow question with a direct routing consequence is
"how much capability does this request need?"

## Question set `turn-route-v1`

| id | primitive | options | role |
|---|---|---|---|
| `intent` | Choice | the seven DREX intent keys | compared with the keyword intent; informational |
| `capability` | Score | five levels mapped to capability classes 1-5 | recommended floor = most probable level + 1 |

Two questions, one judgment each, stated literally (TypeSafe's jev-1.13
limitations page: one judgment per question, no unneeded context). The state
is the user's prompt only, bounded to 8,000 characters, never the response.

## Recommendation and counterfactual

- `recommended_floor = argmax(capability) + 1`.
- Counterfactual floor = `max(applied_floor, recommended_floor)`. Raise-only:
  an operator's floor is authority, and a model's judgment may add cost but
  never remove a constraint or grant a permission. Budget ceilings still apply
  inside DREX.
- Both plans are computed with the same `plan_model_call`, request, candidates,
  and remaining-budget adjustment the executor used, so the comparison is
  exact rather than reconstructed.
- The batch gate band is recorded. A capped fallback cannot reach `auto`, so
  its counterfactuals are "if trusted" data, never "would have acted".

## Runtime shape

- `crates/heiwa_judgment` becomes the Rust System 1 engine: validated question
  builders and wire encoding, a decoder for the documented TypeSafe contract,
  and two backends — TypeSafe HTTP and an OpenAI-compatible structured-output
  fallback (Ollama) whose confidence is capped at 0.85. One attempt, bounded
  by a wall-clock budget; every failure is a typed value.
- `OperatorTurnRunner::with_shadow_observer` receives the post-policy request
  and candidates of each Work-scoped model turn after its terminal event is
  durable. The observer is called once, after the active registration is
  released, so it cannot delay, alter, or cancel the turn.
- `heiwa_shell::system1_shadow` judges in a detached task, one at a time,
  and appends one `heiwa.system1_shadow.v1` record to the
  `system1_shadow` evidence stream. The record carries a digest and length of
  the prompt, never its text.
- Guards: a remote backend receives only `standard`-privacy prompts with no
  sensitive match; anything else is recorded as skipped with its reason.
- Off by default. Enabled by `[system1] shadow = true` in
  `~/.heiwa/config.toml` or `HEIWA_SYSTEM1_SHADOW=1`. The TypeSafe key comes
  from `TYPESAFE_API_KEY` or the `heiwa` keychain service, account `typesafe`.
  The model is pinned (`jev-1.13.0`) per TypeSafe's version guidance.

## Report

`heiwa work shadow [<work-id>] [--json]` joins shadow records with the operator
journal: coverage, skips and errors, classifier latency and tokens, gate bands,
intent agreement, would-raise-floor and would-change-model counts, and turn
outcomes (completed, interrupted, cancel requested, approval not granted,
executed cost) split by whether the counterfactual differed. Accepted means
completed with no cancel request. Quality labels are not collected yet; the
report says so rather than implying quality.

## Failure and restart behaviour

A shadow task lost to a restart leaves no record; the report counts
Work-scoped model turns without one. Shadow errors never reach the turn.
The evidence stream is append-only JSONL under the resolved evidence root.

## Out of scope

- Executing on the recommendation (promotion) and its policy review.
- Quality labels and threshold calibration from real turns.
- Retry, OpenRouter, and doctor checks for the Rust client.
- Desktop presentation of shadow data.
