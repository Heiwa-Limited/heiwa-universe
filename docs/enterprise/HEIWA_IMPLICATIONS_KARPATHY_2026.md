# Heiwa Implications From Karpathy 2026

Date: 2026-03-21

## Scope

This note captures the operational implications for Heiwa from Andrej Karpathy's March 20, 2026 "No Priors" interview, the `karpathy/autoresearch` repo, and the `microgpt` writeup. It is not a general "agents are cool" summary. It is a translation into Heiwa system design.

## What Is Actually Verified

### 1. December 2025 was the workflow break

Karpathy says December was the point where his workflow flipped from mostly writing code himself to mostly delegating to agents, and by March 2026 he says he likely had not typed a line of code since December. He also frames many failures as a problem of instructions, memory, and orchestration rather than raw model incapability.

Sources:
- [No Priors transcript](https://podscripts.co/podcasts/no-priors-artificial-intelligence-technology-startups/andrej-karpathy-on-code-agents-autoresearch-and-the-loopy-era-of-ai)

### 2. The unit of work is shifting from line edits to macro actions

Karpathy describes multiple agents working in parallel on non-conflicting functionality, with the operator moving at the level of repository-scale actions instead of line-level implementation.

Sources:
- [No Priors transcript](https://podscripts.co/podcasts/no-priors-artificial-intelligence-technology-startups/andrej-karpathy-on-code-agents-autoresearch-and-the-loopy-era-of-ai)

### 3. `program.md` is the human-controlled surface in AutoResearch

The `autoresearch` README is explicit: the point is not that the human edits Python files directly, but that the human edits `program.md`, which defines the agent instructions and research-organization behavior.

Sources:
- [karpathy/autoresearch README](https://github.com/karpathy/autoresearch)

### 4. AutoResearch is a closed-loop search pattern, not just an "overnight agent"

The reusable structure is:

1. fixed task surface
2. measurable objective
3. bounded experiment loop
4. accept/reject mechanism
5. human oversight at the instruction layer

The README describes a five-minute fixed budget, agent edits to `train.py`, and a keep-or-discard loop against a measurable validation metric.

Sources:
- [karpathy/autoresearch README](https://github.com/karpathy/autoresearch)

### 5. MicroGPT matters because it compresses the primitive artifact

Karpathy's blog describes MicroGPT as a single file with 200 lines of pure Python and no dependencies. The strategic lesson is not the exact line count. The lesson is that the core artifact is tiny, inspectable, and fully legible.

Sources:
- [microgpt writeup](https://karpathy.github.io/2026/02/12/microgpt/)

### 6. Heiwa already has the right raw substrate

The current repo surface already treats machine-readable operating files as first-class:

- `docs/superpowers/status/feature_list.json`
- `docs/superpowers/status/progress.md`
- `scripts/init_env.sh`

The canonical routing and runtime posture is also already explicit:

- SpacetimeDB is the state layer
- The MacBook-first local runtime is the target runtime
- `HeiwaClaw / MCP` is the execution surface

Sources:
- [HEIWA.md](https://github.com/Strategizing/heiwa-universe/blob/main/HEIWA.md)

## What This Means For Heiwa

### 1. Governance files are code

The highest-value assets in Heiwa are not only Python files. They are the files that govern agent behavior across sessions:

- `HEIWA.md`
- `AGENTS.md`
- `SOUL.md`
- room files
- `feature_list.json`
- `progress.md`

These should be treated as governance code:

- versioned
- diff-reviewed
- linted
- benchmarked for downstream agent quality

Heiwa's moat is not agent access. It is whether these files form a deterministic operating layer across many agents and runtimes.

### 2. Heiwa should formalize macro actions as a contract

The repo already behaves like a control plane, but the contract needs to be explicit. A Heiwa macro action should minimally specify:

- objective
- scope boundaries
- files or directories in scope
- runtime target
- allowed tools
- context bundle
- acceptance checks
- verification artifact
- rollback path
- budget or token ceiling

This should become a first-class protocol object, not an implicit pattern spread across prompts and handlers.

### 3. The next durable advantage is orchestration quality

Karpathy's "skill issue" framing maps directly onto Heiwa. The highest-leverage system assets are:

- decomposition rules
- memory retrieval rules
- permission boundaries
- evidence bundle schemas
- verification harnesses
- failure taxonomy

This is operating-system engineering for agents, not prompt cosmetics.

### 4. Heiwa needs a Program layer

The most direct translation of `program.md` into Heiwa is a program-spec layer such as:

```text
program/
  build.program.md
  deploy.program.md
  audit.program.md
  research.program.md
  patch.program.md
```

Each program should compile into:

- agent roster
- sequencing rules
- memory bindings
- stop conditions
- escalation policy
- acceptance checks

That is the missing bridge between "instruction files exist" and "instruction files are executable governance."

### 5. Persistent claws should be judged as workers, not branding

HeiwaClaw should be evaluated as a persistent worker abstraction. A real claw needs:

- persistent identity
- leases or claims
- resumability
- checkpoints
- budget guardrails
- observable event stream
- evidence bundle output
- kill switch / revoke path

If those are absent, "claw" is branding. If those are present, it is product.

### 6. AutoResearch loops belong inside Heiwa, but only where the objective is measurable

The first good Heiwa applications are:

- router tuning against cost, latency, and task success
- bench-driven patch generation
- release-hardening loops
- tenant-specific optimization only when replay or simulation exists

The wrong move is unconstrained self-modification. The right move is bounded search over a fixed task surface with a hard metric.

### 7. MicroGPT argues for smaller canonical artifacts

Heiwa should aggressively compress its core primitives into small inspectable schemas:

- task contract
- routing decision record
- event envelope
- memory record
- evidence bundle

Complex orchestration is acceptable only if the primitive objects remain brutally small and legible.

## Operational Recommendations

### Immediate

1. Define a canonical Macro Action Contract in protocol/schema form.
2. Promote governance files to tested system inputs instead of passive docs.
3. Add a Program layer that compiles instruction files into runtime behavior.
4. Audit HeiwaClaw against persistent-worker requirements.
5. Restrict self-improvement loops to measurable, replayable objectives.

### Near-Term

1. Benchmark changes to `HEIWA.md`, room files, and status artifacts the way code changes are benchmarked.
2. Build one program executor end to end for `build.program.md`.
3. Emit evidence bundles for all macro actions by default.
4. Add failure taxonomy categories for memory failure, orchestration failure, permission failure, and verification failure.

### Non-Goals

- More arbitrary agent splitting.
- "Prompt improvements" without protocol or evaluation changes.
- Unbounded autonomy without keep-or-discard metrics.
- Treating persistent runtime state as optional.

## Bottom Line

Karpathy's 2026 shift does not mainly validate "using agents." It validates a specific architecture:

- instruction files as governance code
- macro actions over repositories
- persistent workers instead of chat sessions
- benchmarked autonomous loops
- human control at the governance layer

Heiwa already has the substrate for this. The missing layer is not another model. It is a tighter operating system over the agent runtime it already owns.
