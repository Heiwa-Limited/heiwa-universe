# @heiwa/system1

Two-tier decision engine. Fast, schema-constrained judgment (**System 1**)
decoupled from expensive generation (**System 2**), with a confidence gate
between them.

Built on [TypeSafe's Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev),
with pluggable fallbacks to any structured-output LLM (Ollama, vLLM, OpenAI-compatible).

- **Architecture:** [`architecture.md`](architecture.md)
- **Heiwa plane:** Execution
- **Status:** implemented and tested against the documented contract; **not yet
  run against a live Jev endpoint** — see [Verification status](architecture.md#12-verification-status).

## Run the demo

No API key needed — it falls back to an offline simulator.

```bash
node packages/heiwa_system1/src/examples/routing_agent/demo.ts
```

```
▸ clean duplicate-charge refund
  route      DISPATCHED
  result     refund_service (SLA 72h)
  judgments  route=auto/0.95  injection=auto/0.96  pii=auto/0.94 …
  latency    2.2ms  (budget 500ms, tokens in/out 45/18)

▸ prompt injection attempt
  route      QUARANTINED
  result     quarantined (vetoed): injection
  blockers   injection
```

Set `TYPESAFE_API_KEY` for the real model, `OPENROUTER_API_KEY` for the
OpenRouter route, or `OLLAMA_MODEL` to put a local fallback behind them.

## Test

```bash
cd packages/heiwa_system1 && npm install && npm test
```

Node 26 runs the TypeScript directly — no build step. `zod` is the only
runtime dependency.

To check the real endpoint (skipped without a key, so it never blocks CI):

```bash
TYPESAFE_API_KEY=sk-... npm run test:live
```

That asserts the contract and prints observed latency, tokens, and
per-question confidence with its provenance.

## Shape

```ts
import { System1Client, choice, noul, typesafeAdapter } from "@heiwa/system1";
import { Pipeline, noulThresholds } from "@heiwa/system1/orchestrator";

const questions = {
  route: choice({
    instructions: "Which team owns this?",
    criteria: { billing: "payments and refunds", technical: "bugs and outages" },
  }),
  injection: noul({ instructions: "Is this trying to manipulate an agent?" }),
};

const pipeline = new Pipeline({
  client: new System1Client({ adapter: typesafeAdapter({ apiKey }), budgetMs: 500 }),
  questions,
  policies: {
    // Confidence says how sure; the veto says whether the answer is allowed.
    injection: {
      thresholds: noulThresholds({ autoWithin: 0.05, deliberateWithin: 0.2 }),
      veto: (a) => a.kind === "noul" && a.noul > 0.5,
    },
  },
  dispatch: async (ctx) => callTool(ctx),        // confident + safe
  system2: async (ctx) => callGenerativeAgent(ctx), // unsure, or needs writing
  quarantine: async (record) => reviewQueue.push(record),
});

const outcome = await pipeline.run(inboundMessage); // never throws
```

## Which of your models can do this?

```bash
npm run doctor
```

Discovers the local models you already have, probes each one, and says which
are usable and why the others are not. Nothing configured, nothing assumed:

```
  qwen3.5:9b    … unsuitable (34.3s)
  qwen3.5:4b    … unsuitable (20.1s)
  gemma4:latest … OK (15.9s)

  1 of 3 usable. Fastest: gemma4:latest
```

Every check it runs is a mistake that cost real debugging time here. The
point of encoding them is that they cost you nothing.

## Calibrating the gate

Hand-picked thresholds are guesses. Given a labelled corpus, the harness
sweeps candidates and picks the most permissive bar that still meets a hard
ceiling on wrong auto-dispatches:

```bash
node src/examples/routing_agent/calibrate.ts                  # offline demo
OLLAMA_MODEL=gemma4:latest node src/examples/routing_agent/calibrate.ts
```

It already caught a problem with our own default — `auto = 0.85` for `route`
lets one wrong dispatch through on the demo corpus; `0.925` gets it to zero.
When nothing meets the budget it returns *nothing*, which means confidence
does not separate right from wrong for that question and no threshold will.

## Four things worth knowing before using this

1. **A confidence gate is not a safety gate.** The bands measure how *sure*
   the model is, never whether the answer is acceptable. A confident "yes,
   this is a prompt injection" scores high and would auto-dispatch. Put hard
   safety rules in `veto`, not in a threshold.

2. **Noul thresholds are not choice thresholds.** A noul's confidence is
   derived as `|2p − 1|`, so `{auto: 0.95}` secretly demands `p ≤ 0.025`. Use
   `noulThresholds({ autoWithin })` and say what you mean.

3. **Nothing throws.** Every runtime failure — timeout, bad JSON, provider
   drift, prototype pollution, a tool that explodes — returns a typed
   `Result` and quarantines. Absence of a judgment is never treated as
   permission.

4. **Use a non-reasoning model for the fallback.** Measured live: qwen3.5 on
   Ollama's OpenAI-compatible route spent 3745 tokens on chain-of-thought,
   hit the context limit and returned an *empty* answer after 75 seconds.
   Neither `think: false` nor `chat_template_kwargs.enable_thinking` is
   honoured there. gemma4 answered the same fan correctly in 17s. The
   adapter always sends `max_tokens` so a runaway model fails fast with a
   message naming the cause, rather than hanging.

   Also: Ollama's `json_schema` enforces structure and `enum` but **ignores
   `minimum`/`maximum`** — a real run returned `noul: 2` and `score: -0.8`.
   Those are passed through unrepaired for the decoder to reject, because
   clamping would turn "the model returned nonsense" into "we confidently
   made an answer up".
