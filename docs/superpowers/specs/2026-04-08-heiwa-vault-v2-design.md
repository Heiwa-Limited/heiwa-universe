# Heiwa Vault V2 Design

> **Status:** Draft approved in-session for planning
> **Date:** 2026-04-08
> **Scope:** `heiwa-universe`

## Goal

Replace Heiwa's thin memory helpers with a real per-user vault system that combines Obsidian-style editable Markdown, MemPalace-style retrieval structure, and Heiwa-native cross-node synchronization.

The vault must be:

- user-scoped
- available on MacBook, WSL PC, and Railway by default
- full-text across nodes by default
- conversation-aware and filesystem-aware from day one
- Markdown-first and user-editable
- grounded in Heiwa's actual architecture: Rust runtime, SpacetimeDB authority, provider-owned inference

## One-Sentence Truth

Heiwa Vault V2 is a per-user distributed memory vault: `heiwa` exposes a Markdown workspace, trusted nodes hydrate fast local read models, SpacetimeDB replicates canonical vault records, and MemPalace ideas are translated into Heiwa-native retrieval rather than copied as a separate Python sidecar.

## Why This Exists

Current Heiwa memory is not the product the operator wants:

- [`packages/heiwa_sdk/heiwa_sdk/agent_memory.py`](../../../packages/heiwa_sdk/heiwa_sdk/agent_memory.py) is a session-memory helper around `captain_messages`, summaries, and focus.
- [`packages/heiwa_sdk/heiwa_sdk/memory.py`](../../../packages/heiwa_sdk/heiwa_sdk/memory.py) is a thin embedding service over `knowledge_embeddings`.
- [`apps/heiwa_hub/agents/heiwaclaw.py`](../../../apps/heiwa_hub/agents/heiwaclaw.py) integrates those pieces, but the overall system is still a narrow memory loop, not a durable vault.

MemPalace is stronger in the local memory layer:

- raw verbatim storage
- conversation and filesystem mining
- layered recall
- taxonomy-based filtering
- local knowledge graph
- MCP-facing retrieval tools

But MemPalace is not a replacement for Heiwa's distributed truth plane. Heiwa still needs:

- SpacetimeDB as canonical adjudicated state
- DREX-aware routing and evidence
- multi-node synchronization
- operator-facing `heiwa` runtime as the product center

So the correct move is:

1. keep SpacetimeDB as authority
2. replace Heiwa's current memory logic aggressively
3. translate MemPalace concepts into Rust/STDB/TypeScript-friendly Heiwa subsystems

## Locked Product Decisions

These are accepted design constraints for Vault V2:

- **User-scoped first:** vault state belongs to a concrete `owner_id` / `user_id`.
- **Conversations + filesystem from day one:** both are first-class ingest surfaces.
- **Cross-node full-text by default:** memory continuity must work across trusted nodes.
- **Markdown-first workspace:** user edits and machine edits must converge on the same visible vault.
- **Live providers only:** routing and provider inventory must normalize around providers Devon actually has, not hypothetical vendor matrices.
- **Heiwa-native translation:** MemPalace logic is a reference architecture, not a drop-in dependency strategy.

## Architecture Overview

Vault V2 has four layers.

### 1. User Vault Surface

The visible vault is a Markdown workspace rooted under a Heiwa-owned path such as:

`~/.heiwa/vault/<owner_id>/`

This workspace contains:

- user-authored notes
- machine-authored notes and summaries
- project pages
- people/context pages
- generated index pages
- machine metadata in hidden subdirectories

The vault is not a report export. It is the editable working memory surface.

### 2. SpacetimeDB Authority Layer

SpacetimeDB is the canonical record for:

- documents known to the vault
- chunk identities and ordering
- source lineage
- graph links and derived relationships
- synchronization state across nodes
- ownership and access scope

SpacetimeDB is not the local search engine. It is the replicated source of truth.

### 3. Node-Local Read Model

Each trusted node maintains a local read model hydrated from the canonical vault stream.

This read model is responsible for:

- fast full-text retrieval
- local semantic ranking
- wake-up context generation
- graph traversal and topic expansion
- filesystem-to-vault reconciliation

This is the Heiwa-native equivalent of the MemPalace local palace store.

### 4. Agent/Tool Interface Layer

Heiwa agents and operator surfaces query the vault through a stable interface that supports:

- recall before answering
- wake-up context
- filtered retrieval by user/project/topic
- ingest status
- note materialization and sync status

This interface feeds `heiwa`, Heiwa agents, and later remote workers and MCP-style tools.

## Canonical Heiwa Vocabulary

MemPalace terms translate into Heiwa terms as follows:

| MemPalace | Heiwa Vault V2 |
| --- | --- |
| wing | domain scope such as user, project, device, or person |
| room | topic slug within a domain |
| closet | generated compact context or materialized note |
| drawer | raw chunk payload |
| hall | relationship class or retrieval facet |
| tunnel | cross-domain link |

Heiwa may still expose "wing" and "room" in user-facing search or note metadata because the concept is useful, but the canonical storage model stays Heiwa-native.

## Data Model

Vault V2 introduces canonical tables in the STDB module.

### `vault_documents`

One row per logical document or transcript.

Required fields:

- `document_id`
- `owner_id`
- `user_id`
- `source_type` (`markdown_note`, `conversation_log`, `code_file`, `doc_file`, `generated_note`)
- `domain_slug`
- `topic_slug`
- `title`
- `canonical_path`
- `content_hash`
- `visibility`
- `created_at`
- `updated_at`

### `vault_chunks`

One row per raw chunk, ordered within a document.

Required fields:

- `chunk_id`
- `document_id`
- `owner_id`
- `user_id`
- `chunk_index`
- `chunk_hash`
- `ciphertext_json`
- `plaintext_len`
- `token_estimate`
- `domain_slug`
- `topic_slug`
- `source_locator_json`
- `created_at`
- `updated_at`

Design assumption: canonical chunk payloads replicate through STDB as encrypted blobs for trusted-node hydration. Trusted nodes hold local plaintext caches for indexing and workspace materialization.

### `vault_links`

Cross-document and cross-topic structural links.

Required fields:

- `link_id`
- `owner_id`
- `user_id`
- `from_document_id`
- `to_document_id`
- `link_type`
- `evidence_chunk_id`
- `weight`
- `created_at`

### `vault_entities`

Entity registry for people, projects, tools, devices, and named concepts.

Required fields:

- `entity_id`
- `owner_id`
- `user_id`
- `entity_type`
- `display_name`
- `normalized_name`
- `properties_json`
- `created_at`
- `updated_at`

### `vault_facts`

Temporal relationship records derived from user notes, conversations, and mined files.

Required fields:

- `fact_id`
- `owner_id`
- `user_id`
- `subject_entity_id`
- `predicate`
- `object_entity_id`
- `valid_from`
- `valid_to`
- `confidence`
- `source_chunk_id`
- `created_at`

### `vault_sync_state`

Per-node synchronization cursor and materialization status.

Required fields:

- `sync_id`
- `owner_id`
- `user_id`
- `node_id`
- `cursor`
- `workspace_revision`
- `index_revision`
- `status`
- `last_heartbeat_at`
- `updated_at`

### Compatibility With Existing Tables

Current memory tables remain during transition:

- `captain_messages`
- `captain_summaries`
- `captain_focus`
- `knowledge_embeddings`

They become compatibility tables, not the future product shape.

Migration rule:

- no new product capability should depend on `captain_*` or `knowledge_embeddings` once equivalent `vault_*` primitives exist

## Workspace Contract

The user-visible vault and the canonical vault state must round-trip.

### Markdown-first behavior

- User edits to a note must update canonical vault records.
- Internal Heiwa changes that produce new durable memory must materialize back into user-visible Markdown.
- Machine-authored notes must be visible and editable unless explicitly marked generated-only.

### Workspace layout

A concrete vault may look like:

```text
~/.heiwa/vault/<owner_id>/
  inbox/
  projects/
  people/
  sessions/
  generated/
  archive/
  .heiwa/
    state/
    indexes/
    manifests/
```

### Materialization rules

- Human-authored notes are primary sources.
- Conversation logs may materialize as Markdown transcripts or structured daily/session notes.
- Generated summaries, indexes, and "closet" notes live in `generated/` unless promoted by the user.
- Internal machine metadata must not pollute normal note content.

## Ingest Model

Vault V2 supports two ingestion classes from day one.

### 1. Conversation ingest

Sources include:

- provider session logs
- Discord/operator interactions
- Heiwa internal mission/task transcripts
- imported exports when needed

Conversation ingest requirements:

- preserve raw verbatim turns
- scope every record to `owner_id` / `user_id`
- track source runtime and source surface
- materialize meaningful session notes

### 2. Filesystem ingest

Sources include:

- Markdown notes
- code and config files
- docs and specifications
- selected project artifacts

Filesystem ingest requirements:

- respect `.gitignore` and Heiwa-configured ignore rules
- hash and chunk file content deterministically
- track source path and source revision
- avoid duplicating unchanged content
- reflect user edits back into canonical state

## Retrieval Model

Vault V2 adopts a Heiwa translation of the MemPalace layered model.

### Layer 0: Identity and operator frame

Always-available user and runtime frame:

- who the operator is
- what projects/domains matter
- which devices and workers are active

### Layer 1: Essential vault context

Compact always-ready digest built from:

- recent sessions
- active projects
- current focuses
- important unresolved facts

### Layer 2: Filtered topic recall

Fast retrieval scoped by:

- owner
- domain
- topic
- source type

This is the main equivalent of wing/room retrieval.

### Layer 3: Deep recall

Full-text and semantic retrieval over the hydrated local read model.

This layer returns:

- verbatim raw chunks
- linked notes
- related entities/facts
- source documents for inspection

## Search and Ranking

Vault V2 does not adopt ChromaDB as the required canonical engine.

Instead:

- canonical replication stays in STDB
- each trusted node hydrates a local read model
- the local read model is free to use Rust-friendly indexing primitives

Phase 1 target:

- exact and fuzzy full-text first
- topic/domain filtering
- graph-aware expansion
- optional local embedding rerank where Ollama is available

This is acceptable because the critical product requirement is cross-node full-text continuity first, not benchmark theater.

## Provider and Routing Truth

Vault V2 planning assumes Heiwa is normalized around the providers Devon actually has now.

### Canonical live-only provider set

| Canonical provider id | Account/auth shape | Canonical rate group(s) | Canonical models |
| --- | --- | --- | --- |
| `ollama` | local runtime | `local_ollama` | `qwen3.5:9b`, `qwen3.5:4b`, `gemma4`, `qwen3-embedding:0.6b` |
| `google-gemini-cli` | OAuth CLI via `gemini` | `google_gemini_cli` | `gemini-cli/gemini-3.1-pro` |
| `google-antigravity` | OAuth CLI / wrapped Antigravity surface | `antigravity_flash`, `antigravity_pro` | `google-antigravity/gemini-3-flash`, `google-antigravity/gemini-3.1-pro` |
| `claude-code` | OAuth CLI via `claude` | `claude_code` | `claude/haiku-4-5`, `claude/sonnet-4-6`, `claude/opus-4-6` |
| `codex` | OAuth CLI via `codex` | `openai_codex` | `codex/gpt-5.4` |

### Normalization rules

- Heiwa must stop presenting generic provider ids like `google`, `openai`, and `anthropic` in runtime-facing routing state when the real surface is `google-gemini-cli`, `google-antigravity`, `claude-code`, or `codex`.
- `gpt-4.1` remains historical, not primary.
- `gemini-2.5-*` remains historical drift, not canonical current state.
- Antigravity must remain split into `flash` and `pro` lanes.
- Routing config and generated local account state must converge on the same provider vocabulary.

## Node Topology

Vault V2 treats devices as trusted vault hydrators.

### MacBook

- primary operator surface
- primary local editing surface
- primary local Ollama indexing and rerank node

### WSL PC

- secondary trusted vault hydrator
- heavier filesystem/project indexing node
- local worker and code execution surface

### Railway

- always-on coordination node
- trusted vault hydrator for long-running sessions
- canonical background sync and materialization worker
- not required to run local inference

## Security and Trust Model

Vault V2 assumes:

- cross-node full-text is the default for trusted nodes
- scoping is always explicit at `owner_id` / `user_id`
- node trust must be explicit for vault hydration
- raw full-text replication is a product feature, not an accidental side effect

Design assumption for this spec:

- canonical STDB payloads store encrypted chunk bodies
- trusted nodes materialize plaintext into local workspace and local indexes

This keeps cross-node continuity without making the replicated truth plane plain-text by default.

## Relation to Existing Heiwa Principles

Vault V2 does not change the core product truth in [`HEIWA.md`](../../../HEIWA.md):

- `heiwa` remains the installed product surface
- DREX remains the execution kernel
- SpacetimeDB remains the authority plane
- providers still own their inference internals

Vault V2 strengthens the product center instead of diluting it:

- memory becomes a first-class product subsystem
- local runtime becomes more useful because it remembers actual work
- cross-node coordination becomes meaningful because the same user context follows the operator

## What This Does Not Attempt

- It does not replace provider inference with self-hosted frontier inference.
- It does not make Railway the product center.
- It does not replace STDB with ChromaDB or SQLite as the authority plane.
- It does not require AAAK or any lossy compression layer as the default storage format.

## Execution Order

Vault V2 should be implemented in this order:

1. normalize live provider truth and routing vocabulary
2. land canonical `vault_*` STDB schema
3. build node-local vault hydration and Markdown workspace sync
4. ingest conversations and filesystem from day one
5. replace current Heiwa memory services with vault-backed retrieval
6. expose vault search and wake-up behavior through `heiwa`

## Success Criteria

Vault V2 is successful when:

- the same user can search and inspect their vault from MacBook, WSL, and Railway-backed Heiwa sessions
- filesystem changes on a trusted node appear in canonical vault state and materialize back into the workspace
- conversation transcripts and file-derived memory coexist in one searchable model
- `heiwa` can recall past work before answering, not just summarize recent turns
- routing config only exposes live provider/model truth
- the system feels like a real knowledge workspace, not a hidden memory cache
