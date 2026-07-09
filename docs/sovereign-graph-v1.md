# Sovereign Graph v1

**Status:** Implemented (schema + SQLite store)  
**Crate:** `heiwa-graph`  
**Protocol types:** `heiwa-protocol::sovereign_graph`  
**Default path:** `~/.heiwa/graph/sovereign.db`

## Purpose

On-device consolidation of a user's authorized digital life so Heiwa can replace multi-app thrash (except inference providers). Data stays local; connectors *pull* into the graph.

## Entity kinds

`person`, `account`, `thread`, `message`, `event`, `note`, `file`, `web_doc`, `task`, `device`, `project`, `receipt`, `memory`

## Evidence

Every consequential node can attach `source_spans` (`message_id`, `event_id`, `file`, `url`, …).

## Calendar proposals

`calendar_proposals` store staged schedule suggestions with **confidence + sources**. Status machine: `draft` → `pending_approval` → `approved|rejected` → `written`. **No silent external writes.**

## Usage (Rust)

```rust
use heiwa_graph::{GraphStore, new_node};
use heiwa_protocol::GraphEntityKind;

let store = GraphStore::open(dirs::home_dir().unwrap().join(".heiwa/graph/sovereign.db"))?;
let node = new_node(GraphEntityKind::Note, "hello");
store.upsert_node(&node)?;
println!("{}", store.doctor_summary()?);
```

## Plane map

| Plane | Graph role |
|-------|------------|
| Intake | Nodes/edges from connectors |
| Execution | Proposals + leases reference graph ids |
| Evidence | Source spans + receipt nodes |
