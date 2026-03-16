# Phase 4: HeiwaClaw Decomposition & Provider Adapters — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the `heiwaclaw.py` gateway into a modular package with specialized adapters for each provider, improving maintainability and making it easier to add new execution engines (like ACP).

**Architecture:**
- `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/`: New package directory.
- `gateway.py`: Main entry point (the dispatcher).
- `adapters/base.py`: Abstract Base Class for all adapters.
- `adapters/reflex.py`: Local Ollama/vLLM/LiteLLM adapter.
- `adapters/gemini.py`: Google Gemini CLI adapter.
- `adapters/claude.py`: Claude Code CLI adapter.
- `adapters/codex.py`: OpenAI Codex CLI adapter.

**Tech Stack:** Python 3.14

---

## Chunk 1: Package Structure & Base Adapter

### Task 1: Initialize `heiwaclaw` package

- [ ] **Step 1: Create directory structure**
  - `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/`
  - `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/adapters/`
- [ ] **Step 2: Create `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/adapters/base.py`**
  - Define `BaseClawAdapter` ABC with `async def execute(route, instruction, env)` method.

### Task 2: Implement Registry & Gateway

- [ ] **Step 1: Create `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/gateway.py`**
  - Implement `HeiwaClawGateway` that maps `adapter_tool` to specific adapter classes.
- [ ] **Step 2: Create `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/__init__.py`**
  - Export `HeiwaClawGateway` and `HeiwaClawDispatch` for backward compatibility.

---

## Chunk 2: Provider Adapters

### Task 3: Local Reflex Adapter

- [ ] **Step 1: Create `adapters/reflex.py`**
  - Port logic for local HTTP/Reflex calls (Ollama, vLLM).
  - Include the "runtime engine fallback" logic here.

### Task 4: CLI-based Adapters (Gemini, Claude, Codex)

- [ ] **Step 1: Create `adapters/cli_adapter.py`**
  - Generic adapter for STDIO-based CLI tools.
- [ ] **Step 2: Specialize for each provider**
  - `adapters/gemini.py`, `adapters/claude.py`, `adapters/codex.py`.

---

## Chunk 3: Integration & Retirement

### Task 5: Switch SDK to new Gateway

- [ ] **Step 1: Update `packages/heiwa_sdk/heiwa_sdk/__init__.py`** to point to the new package.
- [ ] **Step 2: Verify `ExecutorAgent` and `SpineAgent`** work with the new structure.

### Task 6: Retire old `heiwaclaw.py`

- [ ] **Step 1: Delete `packages/heiwa_sdk/heiwa_sdk/heiwaclaw.py`** after all tests pass.

---

## Chunk 4: Verification

### Task 7: Integration Tests

- [ ] **Step 1: Mock each adapter** and verify the gateway correctly dispatches to them.
- [ ] **Step 2: Verify "runtime engine" logic** still works correctly within the Reflex adapter.
