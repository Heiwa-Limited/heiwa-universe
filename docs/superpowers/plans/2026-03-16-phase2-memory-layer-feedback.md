# Phase 2: Memory Layer + Feedback — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a self-improving memory layer that indexes the repository, records execution outcomes, and enriches future task dispatches with relevant context.

**Architecture:**

- **Knowledge Embeddings**: Use `qwen3-embedding:0.6b` via Ollama to index code and docs.
- **Execution Memory**: Record every task's intent, model, effort, and outcome (PASS/FAIL).
- **Context Enrichment**: Semantic search over `knowledge_embeddings` during dispatch to provide relevant context files.
- **Feedback Loop**: Automated test results and human feedback flow back to STDB to tune model selection (in Phase 3).

**Tech Stack:** Python 3.14, Ollama (qwen3-embedding:0.6b), SpacetimeDB

---

## Chunk 1: Memory Service & STDB Bridge

### Task 1: Update `spacetimedb.py` with memory operations

- [ ] **Step 1: Add methods for `execution_memory`**
  - `insert_execution_memory(task_dispatch_id, model_used, outcome, duration_ms, error_summary, feedback_score)`
  - `get_execution_memory(model_id=None, intent_class=None)`

- [ ] **Step 2: Add methods for `knowledge_embeddings`**
  - `insert_knowledge_embedding(source_type, source_id, content_hash, embedding, ttl_hours)`
  - `search_knowledge_embeddings(query_embedding, limit=5)`

### Task 2: Create `MemoryService` in `heiwa_sdk`

- [ ] **Step 1: Create `packages/heiwa_sdk/heiwa_sdk/memory.py`**
- [ ] **Step 2: Implement `generate_embedding(text)` using Ollama API**
- [ ] **Step 3: Implement `index_file(path, content)` with chunking and hash-based deduplication**

---

## Chunk 2: Indexing Pipeline

### Task 3: Implement Repository Indexer

- [ ] **Step 1: Create `apps/heiwa_hub/scripts/index_repo.py`**
- [ ] **Step 2: Scan `apps/`, `packages/`, `docs/`, `config/` for `.py`, `.md`, `.json`, `.rs` files**
- [ ] **Step 3: Index all discovered files into STDB on boot**

### Task 4: Integrate Indexing into Hub Boot

- [ ] **Step 1: Update `apps/heiwa_hub/main.py` to trigger indexing if `HEIWA_INDEX_ON_BOOT=true`**

---

## Chunk 3: Execution Feedback

### Task 5: Update `ExecutorAgent` to record results

- [ ] **Step 1: Import `MemoryService` in `ExecutorAgent`**
- [ ] **Step 2: After `Subject.TASK_EXEC_RESULT` is published, write to `execution_memory` table**

### Task 6: Automated Feedback Collector

- [ ] **Step 1: Implement a basic feedback collector that marks "PASS" if a task produced a valid result summary and "FAIL" on exceptions**
- [ ] **Step 2: Scaffold support for future automated test-runner feedback**

---

## Chunk 4: Context Enrichment

### Task 7: Implement Semantic Retrieval

- [ ] **Step 1: Add `query_knowledge(text, limit)` to `MemoryService`**
- [ ] **Step 2: Use vector similarity (cosine similarity) to find top-K chunks**

### Task 8: Enrich Task Dispatch

- [ ] **Step 1: Update `SpineAgent` or `BrokerEnrichmentService` to call `MemoryService.query_knowledge` using the task's `raw_text`**
- [ ] **Step 2: Add semantically relevant file paths to `context_files_json` in the dispatch envelope**

---

## Chunk 5: Verification

### Task 9: Integration Tests

- [ ] **Step 1: Verify a file can be indexed and then retrieved via semantic search**
- [ ] **Step 2: Verify `execution_memory` is populated after a task completes**
- [ ] **Step 3: Verify a task dispatch includes enriched context from the memory layer**
