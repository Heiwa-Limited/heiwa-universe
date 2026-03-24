# Architecture Review: Heiwa vs. Pi-Mono

**Date:** March 23, 2026
**Subject:** Architectural comparison between the Heiwa Distributed OS and the Pi-Mono Agent Toolkit.

## Executive Summary

While **Heiwa** is a distributed AI Operating System focused on sovereignty and multi-node orchestration, **Pi-Mono** is a high-fidelity toolkit for building interactive AI agents. Heiwa excels in state management (SpacetimeDB) and compute routing, but can learn significantly from Pi-Mono's **UI fluidity**, **provider abstraction maturity**, and **agentic development hygiene**.

---

## 1. Core Architectural Differences

| Feature | Heiwa (Distributed OS) | Pi-Mono (Agent Toolkit) |
| --- | --- | --- |
| **Philosophy** | Opinionated OS; Swarm of peers. | Modular blocks; reusable packages. |
| **State** | **SpacetimeDB** (Authoritative Ledger). | In-memory `Agent` state + Local DBs. |
| **LLM Layer** | **Tiered/Routed** (Cheapest/Free first). | **Unified API** (Multi-provider abstraction). |
| **UI Surface** | CLI + Web Status + Discord Hub. | High-fidelity **TUI** + Web Components. |
| **Deployment** | Railway + Local Boost Nodes. | **vLLM Pods** on GPU providers. |
| **Hygiene** | Session-based `HEIWA.md` rules. | Strict `AGENTS.md` for Class 3 agents. |

---

## 2. Critical Layers for Heiwa Alignment

To elevate Heiwa to the next level of agentic maturity, the following layers should be refined or introduced:

### A. High-Fidelity TUI/UX Layer
Pi-Mono's `@mariozechner/pi-tui` provides a differential rendering engine that makes the CLI feel like a modern application.
- **Heiwa Gap:** The `heiwa` CLI is functional but lacks the "fluidity" of a dedicated TUI library.
- **Recommendation:** Implement a `packages/heiwa_ui` (likely Rust-based with Python bindings) to provide a rich TUI experience for the operator.

### B. Agentic Development Hygiene (Class 3 Standards)
Pi-Mono has a very strict `AGENTS.md` that governs how AI agents (Claude Code, Gemini CLI, etc.) interact with the codebase.
- **Heiwa Gap:** Rules are currently scattered across `HEIWA.md` and `CONTEXT.md`.
- **Recommendation:** Consolidate an "Agentic Standard" that enforces strict typing (No `any`), automated checks (`npm run check` equivalent), and structured changelogs.

### C. Unified Tool Lifecycle Hooks
Pi-Mono's `Agent` class has explicit `beforeToolCall` and `afterToolCall` hooks.
- **Heiwa Gap:** Risk scoring is handled at the `ComputeRouter` level, but granular per-agent tool interception is less formalized.
- **Recommendation:** Standardize these hooks in `BaseAgent` to allow the operator to audit, block, or modify tool execution in real-time.

### D. Formal Compute Leasing (HeiwaPods)
Pi-Mono's `pi-pods` provides a CLI for managing vLLM deployments on GPU pods.
- **Heiwa Gap:** "Boost nodes" are currently ad-hoc local machines.
- **Recommendation:** Formalize the "Boost Node" protocol into a `HeiwaPods` or `ComputeLease` layer that can dynamically spin up/down inference on remote GPU providers when local power is insufficient.

---

## 3. The "State Sovereignty" Advantage

Heiwa's use of **SpacetimeDB** is its greatest differentiator. Unlike Pi-Mono, which is primarily focused on the interaction loop, Heiwa's architecture allows for:
- **Resilient Memory:** State survives agent crashes and network partitions.
- **Multi-Node Sync:** The "MacBook" and "Railway" nodes are always in sync regarding the system's "Soul" and "Intent."
- **Auditability:** Every system state change is recorded in the ledger.

## Conclusion

Heiwa should continue to lean into its **Distributed OS** identity while adopting the **Developer Experience (DX)** and **UI/UX polish** found in Pi-Mono. The goal is a system that is as robust as a sovereign OS, but as delightful to use as a high-end agentic toolkit.
