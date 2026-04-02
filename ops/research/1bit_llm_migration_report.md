# 1-Bit LLM Technology Assessment for Heiwa Rust/TypeScript Migration

> **Date**: 2026-04-01
> **Branch context**: `rust-ts-first-migration` (Tasks 1–2 complete, Task 3 pending)
> **Audience**: Devon and peer Class 3 executors evaluating local inference strategy

---

## Executive Summary

1-bit LLMs (BitNet architecture) offer 90% weight reduction and 15x memory savings over full-precision models. An 8B parameter model runs in ~1.15 GB RAM. This is a real, verified capability — not vaporware. However, the technology is **not ready to replace Ollama** in Heiwa's Rust-native stack today. The ecosystem is immature (no Rust-native 1-bit inference, limited model selection, custom llama.cpp forks required). It is a strong long-term bet worth tracking and prototyping against in H2 2026.

AnythingLLM is a monolithic Node.js/Electron app (57K+ stars, MIT, very active) — useful as a standalone tool or API sidecar but not embeddable into a Rust orchestrator. TurboQuant is a **real and significant** Google Research algorithm (ICLR 2026) for KV cache compression during inference — it is complementary to 1-bit models and directly addresses the biggest caveat about long-context memory usage. There are already Rust crates on crates.io (`tq-kv`, `turboquant`).

**Recommendation**: Add 1-bit model awareness and KV cache compression metadata to the DREX scoring system being ported in Task 3. The combination of 1-bit weights + TurboQuant KV cache compression could enable an 8B model with 65k context in ~2-3 GB total on the MacBook boost node. Do not block the migration on either technology, but design the Rust types to accommodate them.

---

## 1. BitNet Architecture (1-Bit LLMs)

### What It Is

BitNet b1.58 (Microsoft Research, 2024) replaces standard floating-point weights with ternary values: `{-1, 0, 1}`. Each weight requires 1.58 bits instead of 16 (FP16) or even 4 (Q4_K_M). The result:

| Metric | FP16 8B Model | Q4_K_M 8B Model | 1-Bit 8B Model |
|---|---|---|---|
| Weight size | ~16 GB | ~4.6 GB | ~1.15 GB |
| RAM required | ~18 GB | ~6 GB | ~2 GB (with KV cache) |
| Tokens/sec (M4 Pro) | ~20 | ~40 | ~80–130 |

### Key Technical Details

- **Ternary GEMM**: Matrix multiplication becomes add/subtract/skip operations instead of multiply-accumulate. This is fundamentally cheaper in both compute and memory bandwidth.
- **Training-aware**: Unlike post-training quantization (GPTQ, AWQ), BitNet models are trained from scratch with ternary weights. You cannot convert an existing Llama model to 1-bit — the model must be purpose-built.
- **KV cache caveat**: At long context (65k tokens), the KV cache still uses FP16 and can dominate memory. A 65k context 8B model might use 1.15 GB weights + 3-4 GB KV cache. KV cache compression patches exist but add complexity.

### Available Models

**Bonsai by Prism ML** — the first commercially viable 1-bit models:
- Bonsai 4B: ~0.6 GB weights, 65k context, 130 tok/s on M4 Pro
- Bonsai 8B: ~1.15 GB weights, 65k context, functional tool calling
- Available on Hugging Face: `PrismML/Bonsai-*`
- License: needs verification per model card (check before commercial use)

**Maturity**: Alpha/Beta. Quality is "good enough for many tasks" but a Q4_K_M Llama 3 8B still outperforms Bonsai 8B on most benchmarks despite using 4x more RAM.

### Source Repos

| Repo | URL | License | Status |
|---|---|---|---|
| Microsoft BitNet | `github.com/microsoft/BitNet` | MIT | Research reference |
| bitnet.cpp | `github.com/microsoft/BitNet` (subfolder) | MIT | Experimental inference engine |
| Bonsai models | `huggingface.co/PrismML` | Varies per model | First commercial 1-bit models |

---

## 2. llama.cpp and 1-Bit Support

### Current State

Mainline `ggerganov/llama.cpp` does **not** natively support 1-bit/ternary quantization. Running 1-bit models requires one of:

1. **Microsoft's bitnet.cpp** — standalone inference engine, separate from llama.cpp
2. **Community llama.cpp forks** — patched forks that add ternary GGML types
3. **KV cache patches** — additional modifications for compressed KV cache

There are open PRs on `ggerganov/llama.cpp` for ternary support, but maintainers have been cautious about merging due to limited model availability. This means:

- **Ollama cannot run 1-bit models today** — Ollama wraps mainline llama.cpp
- **Custom builds are required** — either build bitnet.cpp from source or use a fork
- **No `ollama run bonsai:8b` equivalent exists**

### Impact on Heiwa

The current `ai_router.json` routes local inference through Ollama. Until Ollama or mainline llama.cpp adds 1-bit support, Heiwa would need a separate inference backend for 1-bit models. This is not a blocker for the migration but affects the `provider` field in model tier records.

---

## 3. Rust Integration Options

### Option A: FFI Bindings to bitnet.cpp (Works Now)

Fork `llama-cpp-rs`, point it at the bitnet.cpp C++ code, expose a C API, call from Rust via FFI. This is the fastest path but creates a maintenance burden — you're tracking a fast-moving C++ fork.

**Effort**: Medium (1-2 weeks for initial binding, ongoing maintenance)
**Risk**: Fork divergence, C++ build complexity in CI

### Option B: Wait for Ecosystem (3-6 Months)

Two things could happen:
- Upstream llama.cpp merges ternary support → `llama-cpp-rs` gets it automatically
- Candle (Hugging Face's Rust ML framework) adds ternary quantization types

**Candle** (`github.com/huggingface/candle`):
- Apache 2.0 / MIT dual-license
- Supports GGUF Q4/Q8 quantization today
- Modular architecture makes adding new quantization types feasible
- Has WASM support (interesting for edge)
- No ternary/1-bit support yet

**mistral.rs** (`github.com/EricLBuehler/mistral.rs`):
- MIT license
- Supports GGUF, GPTQ
- No ternary support yet
- Active development, could be a good target

### Option C: Pure Rust 1-Bit Kernels (Long-Term)

Implement ternary GEMM in Rust directly. The math is simpler than full quantized matmul — it's literally add/subtract/skip per element. A focused inference-only implementation could be built in a few thousand lines of Rust using `std::arch` SIMD intrinsics.

**Effort**: High (4-8 weeks for performant implementation)
**Payoff**: Zero C++ dependencies, full control, fits the Rust-native vision

### Recommended Path for Heiwa

**Do not block the migration on 1-bit support.** Instead:

1. **Task 3 (now)**: Port DREX scoring to Rust with awareness that `cost_per_turn` and `vram_requirement` fields in model tiers will change when 1-bit models arrive. Make these fields data-driven, not hardcoded.
2. **Post-migration**: When Candle or upstream llama.cpp adds ternary support, add a new provider variant (`bitnet` or `candle-1bit`) to the Rust orchestrator's model tier system.
3. **Prototype**: On the MacBook (M4 Pro, 24GB), a 1-bit 8B model leaving 22+ GB free for other work is compelling. Worth a Saturday prototype with bitnet.cpp to validate quality for Heiwa's use cases.

---

## 4. AnythingLLM

### What It Is

AnythingLLM by Mintplex Labs is a monolithic Node.js/Electron application for local AI workflows.

| Attribute | Value |
|---|---|
| **Repo** | `github.com/Mintplex-Labs/anything-llm` |
| **Stars** | 57,357 |
| **License** | MIT |
| **Language** | JavaScript (Node.js backend, React frontend) |
| **Version** | 1.11.1 |
| **Last active** | 2026-04-02 (commits within 24h — very active) |

### Architecture

Three-component monolith:
- **Server**: Node.js + Express + Prisma ORM — LLM routing, agent execution, RAG, API
- **Frontend**: React (Vite) — desktop/web UI
- **Collector**: Node.js + Puppeteer + LangChain — document ingestion (PDF, EPUB, DOCX, YouTube, OCR)

Agent framework is called **AIbitat** — EventEmitter-based, supports both native tool calling and an "UnTooled" prompt-engineering fallback for models without function calling support. Full MCP client with `MCPHypervisor` for spawning MCP server processes. Agent Flows (visual workflow builder) is the sub-agent mechanism.

### API Surface

- **REST** (`/v1/*`): workspaces, documents, users, system, embeds
- **OpenAI-compatible** (`/v1/openai/chat/completions`): each workspace = a "model". Any OpenAI client can connect.
- **WebSocket** (`/agent-invocation/:uuid`): streaming agent sessions
- **No SDK**: no npm package, no Python package, no Rust crate

### Local LLM Support

- **Ollama**: first-class via `ollama` npm package, auth token support, keep-alive, capability detection
- **KoboldCPP / Generic OpenAI**: connects to any OpenAI-compatible endpoint — **this is how custom llama.cpp forks (including 1-bit) would work**
- **LM Studio, LocalAI, Text Generation WebUI, Docker Model Runner**: all supported

### Integration Assessment for Heiwa

| Question | Answer |
|---|---|
| Can it be embedded as a library? | **No** — monolithic app, no library exports, deep Express/Prisma coupling |
| Can its agent framework be extracted? | **No** — AIbitat references Prisma models, filesystem, Express sockets directly |
| Can Rust call it? | **Only via HTTP/WebSocket API** to a running instance |
| Does it support custom llama.cpp forks? | **Yes** — via Generic OpenAI provider pointing at any `llama-server` binary |
| Any TypeScript types exported? | **No** — codebase is JavaScript, not TypeScript |

### Verdict

AnythingLLM is a **standalone tool or sidecar**, not an embeddable component. Two usage paths for Devon:

1. **Personal tool**: Run the desktop app for local RAG/agent work alongside the Class 3 tools. Zero integration needed.
2. **API sidecar**: Run as a Docker container, call its OpenAI-compatible API from the Rust orchestrator for RAG workloads. This adds a Node.js process dependency.

Neither path is part of the Rust migration. The Heiwa orchestrator should own its routing and agent dispatch natively.

---

## 5. TurboQuant — KV Cache Compression (Google Research, ICLR 2026)

### What It Is

TurboQuant is a **vector quantization algorithm** from Google Research for compressing KV cache during LLM inference. It is *not* a weight quantization method like GPTQ/AWQ/GGUF — it compresses the activations generated during token processing, which is a complementary layer.

- **Paper**: "TurboQuant: Online Vector Quantization with Near-optimal Distortion Rate" (Zandieh et al., Google Research)
- **arXiv**: `arxiv.org/abs/2504.19874`
- **Venue**: ICLR 2026
- **Google Blog**: `research.google/blog/turboquant-redefining-ai-efficiency-with-extreme-compression/`

### Why This Matters for Heiwa

At long context (32K+), KV cache dominates VRAM — often exceeding the model weight size. This is the biggest caveat about 1-bit models: a Bonsai 8B uses 1.15 GB for weights but 4-8 GB for KV cache at 65k context. **TurboQuant directly solves this.** The combination:

| Configuration | Weights | KV Cache (65k ctx) | Total |
|---|---|---|---|
| Q4_K_M 8B, FP16 KV | ~4.6 GB | ~6 GB | ~10.6 GB |
| 1-Bit 8B, FP16 KV | ~1.15 GB | ~6 GB | ~7.15 GB |
| Q4_K_M 8B, TurboQuant K4/V4 KV | ~4.6 GB | ~1.5 GB | ~6.1 GB |
| **1-Bit 8B, TurboQuant K4/V4 KV** | **~1.15 GB** | **~1.5 GB** | **~2.65 GB** |

The last row is the endgame: a reasoning-capable 8B model with 65k context in under 3 GB. On Devon's M4 Pro (24 GB unified memory), this leaves 21+ GB for everything else.

### How It Works (3 Stages)

1. **Random Orthogonal Rotation** (Randomized Hadamard Transform): Decorrelates outlier-heavy coordinates. After rotation, each coordinate follows a predictable distribution.
2. **Lloyd-Max Optimal Scalar Quantization**: Because the post-rotation distribution is known in advance, the optimal quantization codebook is precomputed — no training data, no calibration.
3. **QJL Residual Correction** (optional): 1-bit sign-encoding of residuals. **Community consensus: skip QJL for KV cache** (softmax amplifies variance). QJL works for vector search / RAG embeddings.

Key property: **data-oblivious** — no calibration data needed. The quantizer is fully defined by `(dimension, bits, seed)`. Compression happens on-the-fly as tokens stream in.

### Compression Results (Verified by Multiple Teams)

| Configuration | Compression | Quality Impact |
|---|---|---|
| K4/V4 (4-bit symmetric) | 3.8x | +0.23% PPL vs q8_0 |
| K6/V4 + 128-token FP16 window | ~2x | Perplexity-lossless |
| K3/V3 (3-bit) | 4.6–5.1x | +1% PPL vs q8_0 |
| K1/V-Q4 (1-bit keys) | 4.9x | +0.03% PPL |

**Critical findings validated by 6+ independent teams**:
- V compression is essentially free — values can go to 2-bit with near-zero quality impact
- All quality degradation comes from K compression (keys control attention routing through softmax)
- Boundary layers (first 2 + last 2) are disproportionately sensitive — protecting them at higher precision recovers 37–91% of the quality gap

### Implementations

**Rust (directly usable in Heiwa)**:

| Crate | Stars | Key Features |
|---|---|---|
| `tq-kv` (`onur-gokyildiz-bhi/tq-kv`) | 20 | Pure Rust, CUDA + AVX2 SIMD, on crates.io, Pre-RoPE quantization, KV compaction |
| `turboquant` (`AbdelStark/turboquant`) | 19 | Research-grade, ONNX Runtime path, on crates.io |
| `turboquant-rs` (`aisar-labs/turboquant-rs`) | 18 | Correctness-focused (f64, no unsafe), clean algorithm explanation |
| `turbo-quant` (`RecursiveIntell/turbo-quant`) | 15 | TurboQuant + PolarQuant + QJL, streaming-compatible |

**C/C++**:
- `TurboQuant.cpp` (`quantumaikr/TurboQuant.cpp`, 85 stars) — standalone C engine with GGUF, CUDA/Metal/Vulkan/ROCm backends

**llama.cpp integration**:
- `TheTom/llama-cpp-turboquant` — adds `--cache-type-k turbo3 --cache-type-v turbo3` flags, Metal GPU kernels, tested up to 104B at 128K context
- `TheTom/turboquant_plus` — 4,878 stars, the most popular implementation

**Apple Silicon (M4 Pro relevant)**:
- `turboquant-mlx` — fused Metal kernels, 4.6x compression at 98% FP16 speed
- `SwiftLM` — native MLX Swift server with TurboQuant integration

**Competitor**: RotorQuant (`scrya-com/rotorquant`, 233 stars) claims to beat TurboQuant on every axis: 28% faster decode, 5.3x faster prefill, better PPL. Also has llama.cpp fork. Worth tracking alongside.

### Integration Path for Heiwa

Unlike 1-bit inference which has no production Rust path, TurboQuant **already has Rust crates on crates.io**. The integration path:

1. **Short term**: When Heiwa's Rust orchestrator manages local inference (via Ollama or direct llama.cpp), the TheTom/llama.cpp fork with TurboQuant flags could be used as-is. This is a configuration flag, not a code change.
2. **Medium term**: When the Rust orchestrator gets direct inference capabilities (via Candle or custom engine), `tq-kv` provides the KV cache compression layer.
3. **For DREX Task 3**: Add `kv_cache_strategy: String` field to model tier or routing config. Values: `"fp16"`, `"q8_0"`, `"turboquant_k4v4"`, etc. This lets the router select different KV strategies based on available memory and context length requirements.

---

## 6. Impact on the Active Migration

### What Changes in Task 3 (DREX Porting)

The 1-bit landscape affects how DREX scoring data structures should be designed in Rust:

1. **Model tier `cost_per_turn`**: 1-bit models will have near-zero cost (local, free inference) but different quality tradeoffs. The Rust `ModelTier` struct should keep `cost_per_turn` as a float, not an enum.

2. **VRAM tracking**: Current `gpu_slots` assume models consume predictable VRAM. A 1-bit 8B model uses ~1.15 GB vs ~6 GB for Q4_K_M. The capability dispatch chain (`execution_requires` → `get_worker_for_capabilities()`) should compare against actual model VRAM requirement, not just tier tags like `gpu_vram_8gb`.

3. **Provider abstraction**: Today's providers are `{ollama, local, vllm, litellm, google-gemini-cli, claude-code, ...}`. 1-bit inference might arrive as:
   - A new Ollama model type (if upstream merges ternary support)
   - A separate `bitnet` provider (if using bitnet.cpp directly)
   - A Candle provider (if using Rust-native inference)
   
   The Rust provider enum should be extensible (not a closed enum).

4. **Hot model detection**: The current Python router checks `active_loaded_models` from GPU slots and strongly prefers already-loaded models. With 1-bit models being so small, multiple models can be loaded simultaneously — the "is_hot" preference should consider total loaded VRAM, not just single-model presence.

### What Does NOT Change

- **DREX resolution levels** (macro/meso/micro) are unaffected
- **Privacy/sovereign routing** logic is unaffected
- **Rate cascade** logic is unaffected  
- **Risk scoring** is unaffected
- **Intent normalization** is unaffected

### Concrete Recommendation for Task 3

```rust
/// Model tier record — matches STDB model_tiers table
#[derive(Debug, Clone)]
pub struct ModelTier {
    pub model_id: String,
    pub provider: String,
    pub capability_class: u8,
    pub cost_per_turn: f64,           // Keep as float, not enum
    pub effort_level: u8,
    pub effort_knob: String,
    pub enabled: bool,
    pub strengths: Vec<String>,        // Intent strengths from strengths_json
    pub last_success_rate: f64,
    pub vram_requirement_mb: u32,      // NEW: actual VRAM needed (not tier tag)
    pub quantization_type: String,     // NEW: "fp16", "q4_k_m", "ternary_1bit", etc.
    pub kv_cache_strategy: String,     // NEW: "fp16", "q8_0", "turboquant_k4v4", etc.
    pub max_context_tokens: u32,       // NEW: effective max context given VRAM + KV strategy
}
```

The new fields future-proof the tier system:
- `vram_requirement_mb`: arithmetic VRAM comparison instead of boolean tier tags
- `quantization_type`: distinguishes quality profiles for same-size models
- `kv_cache_strategy`: captures TurboQuant or other KV compression, critical for long-context routing
- `max_context_tokens`: effective context given the VRAM budget and KV strategy (a 1-bit 8B with TurboQuant K4/V4 can serve 65k context in ~2.65 GB; a Q4_K_M 8B with FP16 KV might cap at 8k in the same memory)

---

## 7. Repos to Watch

| Repo | What to Watch For | Relevance |
|---|---|---|
| `github.com/microsoft/BitNet` | New model releases, performance improvements | 1-bit reference implementation |
| `github.com/ggerganov/llama.cpp` | PRs tagged "bitnet" or "ternary" | Determines when Ollama gets 1-bit support |
| `github.com/huggingface/candle` | Issues/PRs for ternary quantization | Rust-native 1-bit inference path |
| `github.com/EricLBuehler/mistral.rs` | Quantization type additions | Alternative Rust inference engine |
| `huggingface.co/PrismML` | New Bonsai model releases | Quality and size benchmarks |
| `crates.io/crates/tq-kv` | Updates, API stability | **Rust TurboQuant KV compression** |
| `github.com/TheTom/turboquant_plus` | llama.cpp integration, upstream PRs | Most popular TurboQuant impl (4.8K stars) |
| `github.com/scrya-com/rotorquant` | Performance vs TurboQuant | Potential successor, also has llama.cpp fork |
| `github.com/quantumaikr/TurboQuant.cpp` | GGUF + multi-backend support | Standalone C engine, Metal/CUDA/Vulkan |

---

## 8. Timeline Estimate

| Milestone | When | Confidence |
|---|---|---|
| TurboQuant KV usable from Rust | **Now** | High — `tq-kv` crate exists on crates.io |
| TurboQuant in mainline llama.cpp | Q2–Q3 2026 | High — active fork with upstream intent |
| Ollama supports 1-bit models | H2 2026 | Medium — depends on upstream llama.cpp merge |
| Candle adds ternary quantization | H2 2026 | Medium — HF is actively developing candle |
| 1-bit + TurboQuant combo in Ollama | H2 2026–H1 2027 | Low — requires both features merged |
| Rust-native 1-bit inference | H1 2027 | Low — ecosystem is early |
| Heiwa routes to 1-bit models in production | When Ollama or Candle adds support | Gated on ecosystem |

---

## 9. Bottom Line

**For the migration happening now**: Port DREX to Rust with extensible provider and model tier types. Do not hardcode assumptions about model sizes, quantization formats, or KV cache strategies. The current Python router's `ModelTier` is read from STDB and is already data-driven — the Rust port should preserve this flexibility and add `vram_requirement_mb`, `quantization_type`, `kv_cache_strategy`, and `max_context_tokens` fields.

**For local inference strategy**: The combination of 1-bit weights + TurboQuant KV compression is the future of edge deployment. On Devon's M4 Pro (24GB), this could mean an 8B model with 65k context in ~2.65 GB — leaving 21+ GB for orchestrator, STDB, multiple concurrent models, and other services. This fundamentally changes boost node economics.

**For today**: Keep Ollama as the local inference provider. TurboQuant KV compression is the nearest actionable technology — the `--cache-type-k turbo3` flag on a llama.cpp fork works today and would reduce long-context VRAM usage for existing Ollama-served models. Worth a quick prototype on the MacBook. 1-bit models are a longer-horizon bet.

**For AnythingLLM**: Use it as a personal RAG/agent tool if desired, but it is not part of the Heiwa migration. The Rust orchestrator should own routing and dispatch natively.

**For TurboQuant specifically**: This is the most immediately actionable finding. The `tq-kv` Rust crate exists today. When the Rust orchestrator eventually gets direct inference capabilities, TurboQuant KV compression can be integrated as a Rust dependency rather than requiring a C++ fork. This aligns with the Rust-native direction better than any other technology in this report.
