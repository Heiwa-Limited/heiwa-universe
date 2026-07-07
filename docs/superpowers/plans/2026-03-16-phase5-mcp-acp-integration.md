# Phase 5: MCP & ACP Protocol Integration — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate the Agentic Control Protocol (ACP) for multi-agent coordination and expand MCP support to allow Heiwa to act as both a client and a server.

**Architecture:**

- **ACP Adapter**: New `heiwaclaw` adapter for cross-node agent communication.
- **MCP Server**: Expose Heiwa's core tools (routing, state, memory) as an MCP-compliant server.
- **Mission Layer**: Track complex, multi-step goals in STDB `missions` table.

**Tech Stack:** Python 3.14, MCP, SpacetimeDB

---

## Chunk 1: ACP Integration

### Task 1: Create ACP Adapter

- [ ] **Step 1: Create `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/adapters/acp.py`**
- [ ] **Step 2: Implement `ACPAdapter`**: uses WebSocket/HTTP to communicate with remote Heiwa instances.
- [ ] **Step 3: Register in `HeiwaClawGateway`** under the tool name `heiwa_acp`.

---

## Chunk 2: Heiwa as an MCP Server

### Task 2: Implement MCP Entrypoint

- [ ] **Step 1: Create `apps/heiwa_hub/mcp_entrypoint.py`**
- [ ] **Step 2: Expose `heiwa_resolve_route`**, `heiwa_query_memory`, and `heiwa_read_state` as MCP tools.
- [ ] **Step 3: Support STDIO and HTTP transports** for the MCP server.

---

## Chunk 3: Mission Layer (STDB)

### Task 3: Mission Table & Service

- [ ] **Step 1: Add `missions` and `mission_steps` tables** to STDB Rust module.
- [ ] **Step 2: Create `packages/heiwa_sdk/heiwa_sdk/mission.py`** to manage goal lifecycle.
- [ ] **Step 3: Implement `create_mission(title, goal)`** and `append_step(mission_id, tool, result)`.

---

## Chunk 4: Verification

### Task 4: Integration Tests

- [ ] **Step 1: Verify Heiwa MCP server** can be queried by an external MCP client.
- [ ] **Step 2: Verify `ACPAdapter`** correctly wraps requests for remote nodes.
- [ ] **Step 3: Verify missions** are correctly recorded and retrievable from STDB.
