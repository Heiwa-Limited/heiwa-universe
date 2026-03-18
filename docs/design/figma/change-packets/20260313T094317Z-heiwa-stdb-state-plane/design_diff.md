# Design Drift Summary

- Add a distinct **State Plane** block for the Rust SpacetimeDB module under `apps/heiwa_hub/spacetimedb`.
- Show the **Python bridge** as a typed adapter layer, not as the state authority.
- Show **generated Rust and TypeScript bindings** as downstream artifacts of the module.
- Show **route decisions**, **run records**, **node heartbeats**, and **liveness state** as explicit entities in the fast state path.
- Keep **public status** on the WebSocket path from Railway to the read-only Cloudflare shell.
- Mark **proposals / lease / RFC flows** as a remaining compatibility boundary, not yet fully moved.
