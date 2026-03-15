# Heiwa Python State Bridge

Python currently uses the control-plane adapter in [`packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`](/Users/dmcgregsauce/heiwa/packages/heiwa_sdk/heiwa_sdk/spacetimedb.py).

This directory is reserved for a future generated Python client once the repo adopts a stable SpacetimeDB Python codegen path. Until then, keep the Python bridge methods aligned with the Rust module reducers and tables:

- `record_route_decision`
- `record_run`
- `upsert_node_heartbeat`
- `upsert_liveness_state`
- `get_runs`
- `get_model_usage_summary`
- `list_nodes`
