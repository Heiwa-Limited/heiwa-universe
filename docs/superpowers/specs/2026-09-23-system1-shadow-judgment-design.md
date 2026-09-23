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
  by a wall-clock budget; every failure is a typed value. Requests go only
  through `System1Client`, which never follows a redirect (a redirect is a
  typed `redirected` failure) and reaches a local backend without any proxy,
  so the endpoint `Backend::is_remote` classified is the only one that can
  receive the prompt.
- `OperatorTurnRunner::with_shadow_observer` receives the post-policy request
  and candidates of each Work-scoped model turn after its terminal event is
  durable. The observer is called once, before the turn is released, and
  only spawns its work; a panic in it is contained, so it cannot delay,
  alter, cancel, or strand the turn.
- `heiwa_shell::system1_shadow` judges in a detached task, one at a time,
  and appends one `heiwa.system1_shadow.v1` record to the
  `system1_shadow` evidence stream. Nothing a provider sends is persisted as
  text: the prompt appears only as a digest and length; errors as kind,
  status, and a message the judgment crate generates (never a response body,
  echoed value, or transport-library text); answers only as offered options
  and numbers; the provider's `model` string only as provenance (requested
  model, whether the returned id matched or is a version of it, else a
  digest). A final sensitive-pattern screen replaces any record that still
  matches with a `withheld` record that keeps the turn accountable; it is a
  defense for credential-shaped material, not a detector of all private text.
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
completed without a cancel) split by whether the counterfactual differed.
Completion is not acceptance: no quality labels exist yet, and the report
says so.

Execution cost is an episode: every provider attempt of every model call in
the turn, read from each attempt's own `route_completed` or `route_failed`
event and keyed by `(call_id, attempt)`. A tool turn's receipt carries only
its follow-up call, and per-call cumulative fields are never summed. Each
amount keeps its cost truth: known zero, exact, estimated (price-list or
proxy), or unknown (`cannot_confirm`, a missing amount, or an attempt with no
recorded end). A total and a cost per completed turn exist only when nothing
is unknown; otherwise the known and estimated subtotals are shown, labelled,
and the total is unavailable. Classifier tokens are reported apart from
execution cost.

## Failure and restart behaviour

A shadow task lost to a restart leaves no record; the report counts
Work-scoped model turns without one. Shadow errors never reach the turn.
The evidence stream is append-only JSONL under the resolved evidence root.

## Out of scope

- Executing on the recommendation (promotion) and its policy review.
- Quality labels and threshold calibration from real turns.
- Retry, OpenRouter, and doctor checks for the Rust client.
- Desktop presentation of shadow data.
