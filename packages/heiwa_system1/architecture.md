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

**Survives: schema conformity.** `questionsToJsonSchema` emits `enum` over
exactly the offered keys, numeric bounds on scores, `required` on every
question, `additionalProperties: false`. A constrained decoder cannot produce
an unoffered option, and `decodeResponse` re-checks anyway.

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

Two seams that are deliberately not built yet, to avoid overstating maturity:

- **DREX.** `crates/heiwa_drex` already chooses a provider by capability floor
  and price. A System One tier belongs in that ladder as the cheapest rung
  above local, and `Choice` is a natural fit for DREX's own routing decision.
  Not wired — that is a Rust-side change with its own design.
- **Evidence.** `CallTelemetry` and `QuarantineRecord` are shaped to serialise
  straight into `crates/heiwa_evidence` JSONL. No writer is included here;
  this package stays sink-agnostic.

Consistent with the repo's Optimization Doctrine (`quality + accuracy +
efficiency`, local models as the default working tier), the
`structuredLlmAdapter` lets the fast path run entirely on local Ollama when
the network is gone — degraded, capped, and honest about being so.

---

## 12. Verification status

**Verified.**
- 162 tests pass; typecheck clean under `strict` + `noUncheckedIndexedAccess`
  + `erasableSyntaxOnly`.
- HTTP adapters exercised against a real `node:http` server: real sockets,
  real status codes, real aborts.
- Zero parse failures across the hostile-input corpus (see §7).
- End-to-end routing agent completes well inside the 500ms budget.
- The TypeSafe request/response contract matches TypeSafe's published API
  reference.

**Not verified.**
- **No call has been made to a live Jev endpoint.** Everything provider-facing
  is tested against the documented contract and local servers. Real latency,
  real calibration quality, and real error behaviour are unmeasured.
- The **OpenRouter route is unverified** (§8).
- The offline simulator is keyword heuristics, not a model. It produces
  well-formed answers for deterministic tests. It says nothing about Jev's
  accuracy.
- Thresholds in `ROUTING_POLICIES` are **reasoned defaults, not calibrated**.
  Calibration needs labelled outcomes: sweep thresholds against a scored
  corpus and pick the point where the quarantine rate is affordable and the
  false-auto-dispatch rate is near zero. Until then treat them as a starting
  point.

**Next steps to close the gap.**
1. Run the fan against live Jev with a real API key; record observed latency
   distribution and compare against the 500ms budget.
2. Confirm or correct the OpenRouter path and model id.
3. Build a labelled corpus and calibrate thresholds per question.
4. Wire `CallTelemetry` into `crates/heiwa_evidence`.
