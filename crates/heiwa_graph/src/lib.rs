//! Local Sovereign Graph store (SQLite).
//!
//! Path convention: `~/.heiwa/graph/sovereign.db` (callers choose path).

mod schema;
mod store;

pub use schema::{SCHEMA_SQL, SCHEMA_VERSION};
pub use store::{new_node, GraphStore, GraphStoreError};

/// Default relative path under `~/.heiwa/`.
pub const DEFAULT_GRAPH_REL: &str = "graph/sovereign.db";
