# Design Drift Summary

- Add a distinct **Eval Plane** for `HeiwaBench`.
- Add a distinct **Agent Product Plane** for `HeiwaCells`.
- Add a **Memory Plane** that is explicitly SpacetimeDB + subscriptions first, with local materialized caches as secondary.
- Add a **UI Generation Plane** for `HeiwaUI` with Playwright verification loops.
- Add an **Intelligence Plane** for `HeiwaPulse` as optional and async.
- Add a **Containment Plane** for `HeiwaSafehouse` around tool execution.
- Add an **R&D Plane** for `HeiwaLab`, visually separated from the production path.
- Keep the current fast runtime path centered on Broker -> HeiwaClaw -> MCP/Adapters -> STDB/WebSockets.
- Visually de-emphasize or mark as transitional any compatibility SQL or polling-oriented paths.
