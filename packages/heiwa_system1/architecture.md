# System 1 / System 2 — architecture

Status: implemented and tested against the documented TypeSafe contract.
Not yet exercised against a live Jev endpoint. See [Verification status](#verification-status).

Package: `packages/heiwa_system1` (`@heiwa/system1`)
Heiwa plane: **Execution** — it routes, gates, and stages work.

---

## 1. The problem this shape solves

An agent that uses one auto-regressive model for everything pays generation
prices for decisions that are not generation. "Which team owns this ticket?"
has four possible answers; asking a 70B model to emit the word `billing` costs
a full forward pass per token, arrives in seconds, and returns a string that
still has to be parsed and validated — with a non-zero chance of
`"Billing"`, `"billing team"`, or a paragraph explaining the choice.

A **System One model** takes application state plus a typed question and
returns a typed value with a calibrated probability. It does not generate
text. Its output is constrained to the schema you supplied, so an unoffered
option is not merely unlikely — it is unreachable. TypeSafe's Jev is the
first of these; it answers in 70–500ms at $0.042/M input tokens with output
tokens free.

That changes what is worth asking. When a judgment costs ~1ms of budget and
fractions of a cent, you stop asking one question and branching, and start
asking every question at once.

```
  Traditional agent                    Two-tier pipeline
  ─────────────────                    ─────────────────
  LLM: classify        ~1200ms         Jev: 6 judgments    ~150ms
  parse + validate      (may fail)     in ONE request,     (cannot fail
  LLM: security check  ~1100ms         evaluated in         by construction)
  parse + validate      (may fail)     parallel
  LLM: urgency         ~1000ms                │
  parse + validate      (may fail)            ▼
  branch                               confidence gate
  ─────────────────                           │
  ~3.3s, 3 parse risks                 ┌──────┼──────┐
                                       ▼      ▼      ▼
                                     tool   System 2  review
                                    (~5ms)  (~1200ms) (human)
```

---

## 2. Topology

```
                         ┌────────────────────────────────────────────┐
  inbound state ────────►│  ORCHESTRATOR                              │
  (string | object)      │                                            │
                         │  speculative.ts                            │
                         │    chunk the fan (≤20 questions/request)   │
                         │    dispatch chunks CONCURRENTLY            │
                         └───────────────────┬────────────────────────┘
                                             │
                         ┌───────────────────▼────────────────────────┐
                         │  SYSTEM 1 CORE                             │
                         │                                            │
                         │  primitives.ts   Choice · Score · Noul     │
                         │       │          (typed builders → wire)   │
                         │  client.ts       budget · retry · telemetry│
                         │       │          NEVER THROWS              │
                         │  adapters/       ┌──────────────────────┐  │
                         │       ├─────────►│ typesafe   /v1/system│  │
                         │       ├─────────►│ openrouter /decisions│  │
                         │       ├─────────►│ structured-llm       │  │
                         │       │          │   (Ollama, vLLM, …)  │  │
                         │       │          └──────────────────────┘  │
                         │       ▼                                    │
                         │  schema.ts       zod + question contract   │
                         │       │          → Result<…>, never throws │
                         │  confidence.ts   one [0,1] scale for all   │
                         └───────────────────┬────────────────────────┘
                                             │  DecodedAnswer[]
                         ┌───────────────────▼────────────────────────┐
                         │  GATE  (gate.ts)                           │
                         │    veto?      → QUARANTINE  (content)      │
                         │    margin?    → QUARANTINE  (ambiguous)    │
                         │    c > 0.85   → AUTO                       │
                         │    c ≥ 0.50   → DELIBERATE                 │
                         │    else       → QUARANTINE                 │
                         │    aggregate: worst band wins, named       │
                         └───────┬───────────┬────────────┬───────────┘
                                 │           │            │
                    ┌────────────▼──┐  ┌─────▼──────┐  ┌──▼──────────────┐
                    │ dispatch      │  │ SYSTEM 2   │  │ quarantine      │
                    │ tool /        │  │ generative │  │ review queue    │
                    │ microservice  │  │ tier       │  │ + full telemetry│
                    └───────────────┘  └────────────┘  └─────────────────┘
```

**Layering rule.** Dependencies point inward and never back out.
`primitives → types → schema → client → orchestrator → examples`.
Adapters depend only on `errors` and `types`; they know nothing about the
gate. That is what lets a fallback adapter be swapped in without changing
the resilience posture — the client owns budget, retry, decoding, and
telemetry for *every* provider identically.

---

## 3. The three primitives

Each maps to a different decision geometry. Picking the wrong one is the most
common design error.

| Primitive | Question shape | Returns | Use when |
|---|---|---|---|
| `Choice` | "Which of these?" | `choice`, `probabilities` (map), `confidence` | Mutually exclusive destinations. You get the full distribution, so near-ties are detectable. |
| `Score` | "Which level?" | `score` (float), `legend`, `probabilities` (array), `confidence` | An ordered spectrum. `score` is probability-weighted, so a 3-level scale legitimately returns 1.5 — it is **not** an index. |
| `Noul` | "Is this true?" | `noul` ∈ [0,1] — **no confidence field** | A binary claim where the probability itself is the signal. |

```ts
choice({ instructions: "Which team?", criteria: { billing: "…", technical: "…" } })
score({  instructions: "How urgent?",  criteria: ["can wait", "today", "now"] })
noul({   instructions: "Refund requested?" })
```

Builders validate eagerly and **throw** — a Choice with one option is a
programming error in the caller's own stack frame. Nothing else in the
codebase hand-writes the wire shape.

---

## 4. Confidence: one scale, two provenances

The gate compares everything against the same thresholds, so everything must
expose a comparable `confidence`. Jev supplies one for `choice` and `score`.
It supplies **none for `noul`** — a single probability has no distribution to
concentrate.

We derive it:

```
confidence(noul) = |2p − 1|        p=0.5 → 0.0 (coin flip)
                                   p=0.0 → 1.0 (certainly false is still certain)
                                   p=1.0 → 1.0
```

Every judgment therefore carries `confidenceSource: "model" | "derived"`, and
telemetry records it. A dashboard that treats the two as interchangeable is
reading a derived number as a trained one.

> ### Trap: noul thresholds are not choice thresholds
>
> Both live in [0,1] and they are **not the same scale**. Writing
> `{ auto: 0.95 }` for a noul demands `p ≤ 0.025 ∨ p ≥ 0.975` — far stricter
> than it reads next to a choice's 0.95. This cost two debugging rounds
> during implementation. Use the helper, which states the intent directly:
>
> ```ts
> noulThresholds({ autoWithin: 0.05, deliberateWithin: 0.2 })
> // auto-dispatch when p is within 0.05 of certain; deliberate out to 0.2.
> ```

---

## 5. The gate

Three outcomes, and **three independent reasons** to refuse the fast path.

| Check | Fires when | Reason | Why it is separate |
|---|---|---|---|
| **Veto** | A policy predicate on the answer *value* returns true | `vetoed` | Confidence measures how sure the model is, never whether the answer is acceptable. A confident `injection: 0.97` — "yes, this **is** an attack" — derives confidence 0.94 and would otherwise auto-dispatch. Certainty about a dangerous answer is the most dangerous case. |
| **Ambiguity** | Top-two gap < `minMargin` | `ambiguous` | `{billing: 0.50, technical: 0.48}` is a coin flip wearing a confident face. Only detectable because Jev returns the distribution. |
| **Confidence** | `c > auto` / `c ≥ deliberate` / else | `confident` · `uncertain` · `low_confidence` | The ordinary band check. |

Defaults: `auto = 0.85`, `deliberate = 0.50`, `minMargin` off.

A veto predicate that **throws is treated as a veto**. A guard that cannot run
has not passed.

**Aggregation is conservative and named.** `gateBatch` takes the worst band
across all gating questions and reports `blockers` — *which* judgment stopped
the fast path. Questions marked `informational` are still evaluated and
reported but do not vote, so losing a sentiment read cannot stall a refund.

---

## 6. Request lifecycle

```
t=0     Pipeline.run(state)
        │
        ├─ speculate()
        │    chunk questions into groups of ≤20
        │    Promise.allSettled over chunks          ← concurrent, not serial
        │      │
        │      └─ System1Client.evaluate() per chunk
        │           AbortController armed at budgetMs (default 500)
        │           adapter.evaluate(body, signal)
        │             └─ POST {state, model, questions}
        │           decodeResponse(questions, body)  ← zod + contract check
        │           emit CallTelemetry
        │           return Result — NEVER throws
        │
        ├─ merge answers · sum usage · collect failures · compute `missing`
        │
        ├─ gatingUnanswered.length > 0 ──────────────► QUARANTINE (incomplete)
        ├─ gate.band === "quarantine" ───────────────► QUARANTINE (vetoed |
        │                                                ambiguous |
        │                                                low_confidence)
        ├─ requiresGeneration(ctx) ──────────────────► SYSTEM 2
        ├─ gate.band === "auto" ─────────────────────► DISPATCH
        └─ else ─────────────────────────────────────► SYSTEM 2
```

`latencyMs` for a fan is the **slowest chunk**, not the sum, because chunks
are concurrent. Token usage *is* summed.

### Fail-closed

Three conditions force quarantine, each separately tested:

1. A gating question came back below threshold.
2. A gating question came back ambiguous or vetoed.
3. A gating question has **no answer at all** — chunk failed, provider down,
   or the body violated the schema.

**Absence of a judgment is never treated as permission.** This is the whole
safety argument: a gate that opens when it cannot read the confidence is not
a gate.

---

## 7. Zero parse failures — how the claim is actually made

TypeSafe's guarantee is that the *model* is constrained to the schema. That
covers the model. It does not cover the bytes between the model and this
process: a proxy returning HTML, a truncated body, an OpenRouter envelope
change, version skew after a provider deploy, or a fallback adapter backed by
an ordinary LLM that has no such guarantee.

So the claim is made **structurally**, not by trusting the provider:

1. **Nothing throws across a module boundary on a runtime condition.**
   `Result<T, System1Error>` is the only return shape. The success value is
   unreachable until the union is narrowed, so a caller cannot forget to
   handle failure. Throwing is reserved for construction-time programming
   errors.
2. **Every body passes `decodeResponse`.** zod validates the envelope and each
   answer; a failure becomes a typed `schema_violation`.
3. **The decoder cross-checks against the questions asked.** zod proves shape;
   it cannot know `choice: "legal"` is illegal unless told which options were
   offered. Option keys, distribution domain, and scale length are all
   verified against the originating question. This is what catches provider
   drift rather than mere malformed JSON.
4. **Untrusted keys cannot masquerade as answers.** Answer maps are
   `Object.create(null)` and membership uses `Object.hasOwn`, so a body
   carrying `__proto__` yields zero answers rather than an inherited one.
   Both the object-literal and `JSON.parse` attack shapes are tested.
5. **A schema violation quarantines**, exactly like a low-confidence judgment.
   "The provider returned nonsense" and "the model was unsure" are the same
   class of event to the pipeline. Neither crashes it.

The integration suite fires HTML, truncated JSON, empty bodies, wrong types,
out-of-range probabilities, prototype-pollution attempts, five HTTP error
statuses, and 100 randomised junk bodies at the full pipeline. **Zero reach a
tool; zero throw.**

---

## 8. Adapters

| Adapter | Route | Provenance |
|---|---|---|
| `typesafeAdapter` | `POST https://api.typesafe.ai/v1/systemone` | TypeSafe's published API reference. Reliable. |
| `openRouterAdapter` | `POST https://openrouter.ai/api/alpha/decisions`, model `typesafe/jev-latest` | **UNVERIFIED.** Assembled from community integrations; OpenRouter's own reference was unreachable. `path` and `model` are constructor options so a live check can correct them without a code change. The `alpha` path implies it may move. |
| `structuredLlmAdapter` | Any OpenAI-compatible `/v1/chat/completions` | Ollama, vLLM, llama.cpp, commercial APIs. |
| `failover([...])` | Tries in order | Inherits the caller's budget; one adapter to the client. |

### What survives translation to an ordinary LLM — and what does not

**Partly survives: schema conformity.** `questionsToJsonSchema` emits `enum`
over exactly the offered keys, `required` on every question, and
`additionalProperties: false`. Those *are* enforced — verified live against
Ollama, which respected the `enum` on every run.

**Numeric bounds are not enforced.** Measured against live Ollama: its
schema constraint honours structure and `enum` but ignores `minimum` /
`maximum`. One real run returned `noul: 2`, `noul: 4` and `score: -0.8`.
So this adapter's guarantee is **weaker than Jev's** and must not be
described as equivalent. Out-of-range values are passed through unrepaired
for `decodeResponse` to reject and quarantine — clamping them would convert
"the model returned nonsense" into "we confidently made an answer up", which
is strictly worse. Two tests pin the no-repair behaviour.

**Does not survive: calibration.** Jev's probabilities are trained against
outcomes. An LLM asked "how confident are you?" produces a number correlated
with fluency, not correctness, and skews high. Trusting it would let a
degraded path open the auto gate — the single most dangerous thing this
package could do.

So fallback confidence is **capped, not trusted**:

```
reported = min(model_confidence, FALLBACK_CONFIDENCE_CEILING)   // 0.85
```

The ceiling equals the gate's `auto` threshold, which is a strict `>`
comparison, so **a fallback answer lands in `deliberate` at best** — System 2
or a human reviews it. The cap only ever lowers: a model admitting low
confidence is passed through unchanged, because that is the one number here
worth believing. A test asserts `FALLBACK_CONFIDENCE_CEILING <=
DEFAULT_THRESHOLDS.auto`, so retuning the gate fails loudly.

Raising `confidenceCeiling` is possible, and requires saying so out loud.

### Reasoning models are the wrong tool for this path

Measured live. A reasoning model (`qwen3.5:4b`) on Ollama's OpenAI-compatible
route spent **3745 completion tokens on chain-of-thought**, hit the context
limit, and returned an **empty `content`** with its thinking in a separate
`reasoning` field — after **75 seconds**, for a six-question fan. Neither
`think: false` nor `chat_template_kwargs.enable_thinking` is honoured on that
route. A non-reasoning model of similar size (`gemma4`) answered the same fan
correctly and in range in 17s.

Two consequences, both implemented:

- **`max_tokens` is always sent**, defaulting to `256 + 96 × questions`. An
  unbounded output budget is not a default, it is a hang.
- **Truncation is diagnosed distinctly from malformed JSON.** An empty body
  with `finish_reason: "length"` reports the chain-of-thought length and says
  to raise the budget or change model. Calling that "not JSON" sends the
  operator to the wrong fix.

Prefer a non-reasoning model for the fallback path.

---

## 9. Failure mitigation

| Failure | Detection | Mitigation | Terminal state |
|---|---|---|---|
| Provider timeout | `AbortController` at `budgetMs` | Abort socket; retry only if backoff fits the remaining budget | `timeout` → quarantine |
| Rate limited (429) | HTTP status | Retryable; exponential backoff 25/50/100ms; failover to next adapter | `rate_limited` → quarantine |
| Provider overloaded (529/5xx) | HTTP status | Retryable; failover | `provider_unavailable` → quarantine |
| Auth failure (401/403) | HTTP status | **Not** retried — no backoff fixes a bad key; failover may still help | `unauthorized` → quarantine |
| Malformed request (422) | HTTP status | Not retried; `failover` policy should not burn the fallback (a malformed request is malformed everywhere) | `invalid_request` → quarantine |
| Non-JSON 200 (proxy/captive portal) | `response.json()` throws | Caught; classified | `schema_violation` → quarantine |
| Schema violation / provider drift | `decodeResponse` cross-check | **Not** retried — the same body fails identically | `schema_violation` → quarantine |
| Prototype pollution in body | `Object.create(null)` + `Object.hasOwn` | Hostile keys yield zero answers | quarantine (incomplete) |
| Adapter throws (bug) | `try/catch` + `Promise.resolve().then` for sync throws | Converted to typed error | `transport` → quarantine |
| Partial fan failure | Per-chunk `allSettled` | Keep arrived answers; name missing ids | quarantine iff a **gating** question is missing |
| Handler (tool) throws | `try/catch` in `Pipeline.run` | Route reported honestly with error attached | `ok: false`, route preserved |
| Quarantine sink unreachable | `try/catch` | Swallowed; verdict stands | quarantine |
| Telemetry sink throws | `try/catch` | Swallowed | decision unaffected |
| Veto predicate throws | `try/catch` | **Treated as a veto** | quarantine (`vetoed`) |
| Latency budget exhausted mid-retry | Deadline check before each attempt | No further attempt | `timeout` → quarantine |

Every row ends in a typed value. None ends in an unhandled exception.

---

## 10. Observability

One `CallTelemetry` record per System 1 call:

```ts
{ adapter, model, questionCount, latencyMs, budgetMs, overBudget,
  attempts, inputTokens, outputTokens,
  questions: [{ id, kind, confidence, confidenceSource }],
  error? }
```

The default sink is a **no-op** — this package never writes to stdout or disk
on its own. `memorySink()` buffers for tests and hosts.

In Heiwa terms these are Evidence-plane records: a host can forward them into
`~/.heiwa/evidence/` as JSONL without reshaping. `overBudget` is the field to
alert on — it means the fast path stopped being fast, which invalidates the
economics before it invalidates correctness.

---

## 11. Heiwa integration

Classified **Execution** under `HEIWA.md`'s three-plane rule.

**The runtime engine is Rust.** Provider credentials live in Rust, so
`crates/heiwa_judgment` carries the engine the runtime uses: question
builders, a decoder for TypeSafe's documented contract, the TypeSafe and
structured-output backends, and the authoritative gate. This package remains
the calibration, doctor, and demonstration tooling around it.

**Shadow judgment in Work-scoped turns is wired (2026-09-23).** After each
Work-scoped operator model turn is terminal, `OperatorTurnRunner` offers its
exact DREX inputs to `heiwa_shell::system1_shadow`, which asks question set
`turn-route-v1` (intent; capability class), replays DREX's plan with a
raise-only floor, and appends one `heiwa.system1_shadow.v1` record to the
`system1_shadow` evidence stream. `heiwa work shadow [<work-id>]` joins those
records with what the turns actually did. It never changes execution, and it
is off unless `[system1] shadow = true`. Design:
`docs/superpowers/specs/2026-09-23-system1-shadow-judgment-design.md`.

**What the runtime path now enforces (review round 1, 2026-09-23).** Astra's
independent review reproduced three gaps. Each is fixed and pinned by a
regression that failed first:

- *Provider text never reaches the journal.* Error messages are generated in
  `heiwa_judgment` (no response bodies, echoed values, or transport-library
  text); decoder diagnostics name only the question and what was offered;
  the provider's `model` string is reduced to provenance. A final
  sensitive-pattern screen withholds any record that still matches. It is a
  defense for credential-shaped material, not proof that all private text is
  detected.
- *The classified endpoint is the only destination.* `System1Client` never
  follows a redirect, and never sends a local backend's request through an
  environment or system proxy. This package's `fetch` adapters refuse
  redirects too (`redirect: "error"`).
- *Unknown cost is never zero.* The report keeps cost truth per attempt
  across every call of a turn, including a tool turn's follow-up. It gives a
  total only when nothing is unknown.

This package's decoder still quotes provider values in its diagnostics. It
persists nothing; the calibration tools print to a terminal. The Rust
runtime path persists, so there the rule is structural.

**The Rust fallback answers with selections, not distributions.** The first
live shadow run showed `gemma4` returning 1.9 of probability mass on a
five-level scale, so the Rust structured-output backend asks only for what its
grammar enforces — one option, or one level as an integer `enum` — plus a
confidence, and records a point mass. Its decoder rejects any distribution
that does not sum to 1 beyond two-decimal rounding. This package's
`structuredLlmAdapter` still requests distributions; per §14 a capped fallback
cannot be calibrated either way.

Still deliberately not built, to avoid overstating maturity:

- **Executing on a judgment.** DREX does not act on a System 1 recommendation.
  Promotion from shadow to an executing floor is a separate decision that
  needs shadow evidence, quality labels, and a live-calibrated primary model.
- **This package's telemetry.** `CallTelemetry` and `QuarantineRecord` remain
  sink-agnostic; the runtime's evidence comes from the Rust shadow records.

Consistent with the repo's Optimization Doctrine (`quality + accuracy +
efficiency`, local models as the default working tier), the
`structuredLlmAdapter` lets the fast path run entirely on local Ollama when
the network is gone — degraded, capped, and honest about being so.

---

## 12. Verification status

**Verified.**
- 253 tests pass offline (unit + integration), plus 4 against live Ollama;
  typecheck clean under `strict` + `noUncheckedIndexedAccess` +
  `erasableSyntaxOnly`; biome clean.
- HTTP adapters exercised against a real `node:http` server: real sockets,
  real status codes, real aborts.
- Zero parse failures across the hostile-input corpus (see §7).
- End-to-end routing agent completes well inside the 500ms budget.
- **The wire contract is checked against TypeSafe's verbatim examples.** An
  earlier note here claimed it matched the published reference; it did not.
  Read on 2026-09-23 (docs.typesafe.ai/api), the reference keys a Score
  answer's `legend` and `probabilities` by level index, names Noul criteria
  `true`/`false`, and caps a Score at 10 levels. The decoder demanded arrays,
  so every live Score answer would have been quarantined. All three are fixed
  in both languages, each pinned by a test that failed first.
- **Shadow judgment ran live inside the real Work flow.** A checkout runtime
  on port 7475 (disposable evidence and state, `local_only` turns, `gemma4`
  judge) shadowed five Work-scoped operator turns sent through the
  authenticated operator API: 5/5 judged, none failed; judge latency p50
  18.0s, p95 30.8s; every judgment `deliberate` (capped, as designed). Intent
  agreed with the keyword rules on 2 of 5, and the judge read two of the
  misses more plausibly than the rules ("review this function for bugs":
  keyword `build`, judged `audit`; "capital of France": keyword `research`,
  judged `chat`). It would have raised the floor on 3 of 5 and changed the
  model on none: with only local candidates, DREX's $0 tie-break already
  picks the most capable local model, so a floor below its class cannot move
  the route. Five turns is a smoke test of the loop, not evidence of value.
- **The fallback path is verified against a live model.** `gemma4:latest` via
  local Ollama answered the full six-judgment fan in 17.5s (1295 in / 113
  out), stayed inside the offered options, and — the property that matters —
  **could not reach the auto band**. Every model-reported confidence came
  back at exactly the 0.850 ceiling, i.e. the model claimed ≥0.85 on
  everything and the cap did all the work. That is the predicted
  overconfidence, caught by construction.
- **The latency budget works against a genuinely slow provider.** A 500ms
  budget against that same 17s model aborted after 507ms as a typed
  `timeout`, rather than blocking.

**Not verified.**
- **No call has been made to a live Jev endpoint** from either language. The
  TypeScript adapter and the Rust backend are tested against the documented
  contract and local servers only. Real latency, real calibration quality,
  and real error behaviour are unmeasured. (The *fallback* path is
  live-verified; the primary one is not.)
- **No quality labels exist.** The shadow report's "accepted" means completed
  with no cancel request, not judged correct, and it says so.
- The **OpenRouter route is unverified** (§8).
- The offline simulator is keyword heuristics, not a model. It produces
  well-formed answers for deterministic tests. It says nothing about Jev's
  accuracy.
- Thresholds in `ROUTING_POLICIES` are **reasoned defaults, not calibrated**
  against production traffic. They are, however, no longer unmeasurable —
  see §13.

**Closing the gap is one command.** `test/live/jev.live.test.ts` drives the
real endpoint and is skipped unless a key is present, so it never blocks CI:

```bash
TYPESAFE_API_KEY=sk-...   npm --prefix packages/heiwa_system1 run test:live
OPENROUTER_API_KEY=or-... npm --prefix packages/heiwa_system1 run test:live

# the fallback path needs no account — it skips cleanly with no Ollama running
OLLAMA_MODEL=gemma4:latest npm --prefix packages/heiwa_system1 run test:live
```

It asserts the **contract, never a specific answer** — a model may disagree
with us about a ticket; it may not return a shape we cannot decode, omit a
question, emit a distribution that does not sum to 1, or blow the budget. It
prints observed latency, token usage, and per-question confidence with its
provenance, which is the data needed for calibration. Running it with
`OPENROUTER_API_KEY` is what confirms or refutes the unverified route in §8;
a failure there means the constants in `adapters/http.ts` need correcting,
not that the engine is broken.

**Remaining steps.**
1. Run `test:live` against real Jev; record the latency distribution. Then
   run the runtime shadow with `[system1] backend = "typesafe"` and a key in
   `TYPESAFE_API_KEY` or the `heiwa` keychain (account `typesafe`).
2. Confirm or correct the OpenRouter path and model id (same command).
3. Label shadowed turns (accepted, and the floor that was actually needed),
   turn failures and corrections into evaluation cases, and calibrate
   thresholds per question — sweep to the point where the quarantine rate is
   affordable and false auto-dispatch is near zero.
4. Shadow `standard`-privacy turns, where remote candidates make a floor
   above the best local class change the route; only those disagreements can
   show whether the judgment predicts trouble.
5. Consider promoting the package to a root npm workspace. It currently
   self-installs (its own `package.json` + lockfile) to avoid rewriting the
   shared root lockfile while other agents are working in the tree; root
   scripts `test:system1` / `typecheck:system1` / `demo:system1` bridge the
   gap in the meantime.

---

## 13. Preflight: is this model usable?

`core/system1/adapters/probe.ts` answers that in one small request, and
`npm run doctor` runs it across every model a user already has.

Each check exists because it cost real debugging time during development:

| Finding | Severity | Why |
|---|---|---|
| `reasons_before_answering` | blocker | A reasoning model on an OpenAI-compatible route spends its output budget on chain-of-thought and returns empty `content`. Measured: qwen3.5 burned 3745 tokens and answered nothing, in 75s. `think:false` is not honoured there. |
| `ignores_enum` | blocker | If the decoder is not genuinely constrained, "cannot emit an unoffered option" is gone and the model cannot be trusted for classification. |
| `ignores_numeric_bounds` | warning | Measured: Ollama honours `enum` but ignores `minimum`/`maximum`. The decoder rejects those downstream, so it costs throughput, not safety. |
| `emits_reasoning` | warning | Answered, but thought first — slower and nearer the budget than it looks. |
| `unparseable` / `unreachable` / `timeout` | blocker | — |

It deliberately does **not** check accuracy or calibration. One request
cannot measure either, and a probe that guessed would be worse than one that
stays silent. That is §14's job.

Real output, three local models, no configuration:

```
qwen3.5:9b:    NOT SUITABLE (34307ms)
  [blocker] reasons_before_answering: answered nothing after 4803 chars of
            chain-of-thought and hit the output budget
gemma4:latest: SUITABLE (15858ms)
  [warning] emits_reasoning: answered, but emitted 1366 chars first
```

This is the design principle the package tries to hold to generally: **a
lesson learned once should cost the next operator zero.** A finding written
only into a document is a finding the next person rediscovers the slow way.

---

## 14. Calibrating the thresholds

Shipping a gate with hand-picked thresholds is shipping a guess. `0.85` is
plausible, not measured, and the right value is a property of *your*
questions, *your* model, and *your* tolerance for a wrong auto-dispatch.

`orchestrator/calibration.ts` turns the guess into a measurement. Given a
corpus labelled with ground truth, it sweeps candidate thresholds and reports
what you actually trade off at each one:

| Metric | Meaning |
|---|---|
| `autoRate` | throughput — the share taking the cheap fast path |
| `falseAutoRate` | **danger** — the share auto-dispatched *and wrong* |
| `autoPrecision` | how right the fast path was, when it fired |
| `deliberateRate` | System 2 spend |
| `quarantineRate` | human review load |

### The asymmetry that drives it

These costs are not comparable. A quarantine costs a human a minute. A false
auto-dispatch refunds an attacker, routes a security incident to billing, or
emails the wrong customer. So this does **not** maximise accuracy and does
**not** balance precision against recall. It maximises throughput **subject
to a hard ceiling on false auto-dispatch**.

When no threshold meets the budget, `recommendThresholds` returns
`undefined` rather than a best-effort suggestion. That is a finding, not a
failure: confidence does not separate right from wrong for that question on
that corpus, and no threshold will fix it — the criteria need separating or
the question splitting.

`autoPrecision` is `undefined`, never `1`, when nothing dispatched. Reporting
1 there would make the *safest possible* threshold look like the best
performing one. For the same reason a bar that dispatches
*nothing* is never recommended, even though it trivially has zero false
dispatches: "0 wrong out of 0" is a disabled fast path dressed up as a
perfect score.

Only the `auto` bar is swept; `deliberate` is held fixed. A full 2-D sweep
would also tune the deliberate/quarantine split — System 2 spend against
human review load — but that split does not affect `falseAutoRate`, so it is
left to the operator, who knows what an hour of review costs relative to a
System 2 call.

### Running it

```bash
node packages/heiwa_system1/src/examples/routing_agent/calibrate.ts          # offline simulator
OLLAMA_MODEL=gemma4:latest node .../calibrate.ts                             # a real local model
TYPESAFE_API_KEY=sk-...   node .../calibrate.ts                              # real Jev
```

It prints a per-question sweep table and a recommendation, and it warns
loudly when the backend is the simulator — a calibration table is exactly the
kind of output that gets screenshotted out of context.

### A sweep is only meaningful if the signal varies

`confidenceSpread` is checked before every table, because a flat signal
produces a normal-looking sweep that discriminates nothing. Two ways that
happens, and the first is self-inflicted:

- **A capped adapter.** `structuredLlmAdapter` clamps confidence to a
  ceiling, so every sufficiently-confident answer reports the *identical*
  number. Measured against gemma4, all six judgments came back at exactly
  0.850. The sweep then shows a cliff at the ceiling and nothing else — not
  because 0.85 is correct, but because the cap destroyed the signal.
  **Corollary: you cannot calibrate a capped fallback, and you should not
  try.** Calibrate against the primary model; the fallback's job is to stay
  out of the auto band, which the cap already guarantees.
- A model that reports the same confidence for everything.

When the signal is degenerate the script says so above the table rather than
letting the numbers be read as calibration.

### Measured: gemma4 is not calibrated for this task

Run against a real local model (`gemma4:latest`, all 16 corpus tickets
evaluated), the harness returns **no safe operating point for any
question**:

| question | accuracy at bar 0 | at bar 0.90 | false dispatches at 0.90 |
|---|---|---|---|
| `route` | 87.5% | — (signal flat, capped) | — |
| `injection` | 87.5% | 62.5% dispatched | **1** |
| `pii` | 81.3% | 68.8% dispatched | **3** |
| `automatable` | 75.0% | 56.3% dispatched | **3** |

Read the `injection`, `pii` and `automatable` rows carefully: those are noul
questions, whose confidence is *derived* rather than capped, so the signal
genuinely varies. Raising the bar from 0 to 0.90 throws away a third to a
half of the throughput and **still does not remove the errors**. For `pii`
and `automatable` the false-dispatch count does not drop at all; precision
gets *worse*, because the answers being held back are disproportionately the
correct ones.

That is the definition of an uncalibrated model: its confidence carries
almost no information about whether it is right. 75–87% raw accuracy is not
the problem — the problem is that no threshold can find the wrong 13–25%.

This is the empirical case for the confidence cap in §8. It was argued from
first principles ("an LLM's self-reported confidence tracks fluency, not
correctness"); this measures it for one specific model on one specific task
and finds exactly that. A local model is a usable System 1 *only* behind a
gate that refuses to let it auto-dispatch.

### It also found a problem with our own defaults

Against the 16-ticket demo corpus on the simulator, the shipped default of
`auto = 0.85` for `route` dispatches 81% of tickets but lets **one wrong
dispatch** through. The recommended bar is **0.925**: 63% fast path, **zero**
wrong dispatches, 19% quarantined.

That is the tool working. It is also why `ROUTING_POLICIES` should be treated
as a starting point rather than a setting: 16 hand-written tickets scored by
a keyword simulator is a demonstration of the workflow, not evidence about
production. Run it against real traffic and real Jev before trusting a
number.
