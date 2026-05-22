# Execution Room

Load this room for:

- worker node execution and hardware topology
- capability-based dispatch and GPU routing
- claim / run / result loops
- tool execution boundaries
- capability and lease enforcement

## Architectural Razor

*Works for one, doesn't block N.*

Every decision in this room must satisfy: works for Devon's current topology AND does not block multi-operator scaling.

## Asymmetric Topology

Heiwa is a sovereign control mesh with asymmetric roles, not an active-active cluster.

- **MacBook (Owner Runtime / Current Server):** M4 Pro, 24GB unified memory. Owns `~/.heiwa/`, the installed `heiwa` runtime, cockpit localhost server, provider auth posture, local SQLite/files, operator terminal, and high-trust execution.
- **WSL (Future Primary Execution Server):** Static, always-on. RTX 3060, 12GB VRAM. Fast inference for <=8B models. GPU, embeddings, media, sovereign workloads.
- **Cloudflare (Paused Public Edge):** Later public DNS/Pages/WAF only. It must not become runtime authority.
- **SpacetimeDB:** Evidence sync/adjudication plane when enabled. Local user functionality must keep working offline.

Nodes dial out to the owner runtime or STDB when online. STDB is the durable work ledger, not an external message queue.

## Dynamic Capability Dispatch

Node capabilities are detected from hardware at runtime, not hardcoded in config.

Detection chain:
1. `worker_manager.py` `_detect_gpu_hardware()` runs `sysctl` (macOS Metal) or `nvidia-smi` (Linux NVIDIA)
2. `_detect_capabilities()` adds `gpu_type`, `gpu_vram_gb`, and tier tags (`gpu_vram_16gb`, `large_model`, `gpu_vram_8gb`, `fast_inference`) to registration payload
3. `transport.py` `register()` populates real `GpuSlot` data in `PodRecord` and persists to STDB
4. Heartbeats refresh GPU data every 15 seconds

Dispatch chain:
1. `ComputeRouter._route_from_worker()` sets `execution_requires` on `ComputeRoute` based on intent, privacy, compute class
2. `BrokerRouteResult` carries `execution_requires` through the dispatch envelope
3. `Spine` passes requirements to `get_worker_for_runtime(requires=...)`
4. `get_worker_for_capabilities()` performs set intersection against `PodRecord.runtime_capabilities`
5. If no node matches, task stays pending in STDB until a matching node connects

## Live Runtime Path

- Broker decides route → `ComputeRoute` with `execution_requires`
- Spine dispatches to matching worker via local executor or future `/ws/worker`
- `HeiwaClaw` resolves execution adapter and transport
- `ToolMesh` executes the selected adapter/tool under lease enforcement
- Results written back through the state layer

## Important Files

- `legacy/apps/heiwa_cli/scripts/agents/worker_manager.py` — legacy GPU detection, capability reporting, worker lifecycle reference
- `legacy/apps/heiwa_hub/transport.py` — legacy `WorkerSessionManager`, registration, `get_worker_for_capabilities()` reference
- `legacy/apps/heiwa_hub/agents/spine.py` — legacy dispatch with `execution_requires` reference
- `apps/heiwa_core/` — active Rust execution kernel and hosted runtime path
- `apps/heiwa_orchestrator/` — active DREX scoring, persistence, and STDB-facing orchestration path
- `legacy/packages/heiwa_cognition/heiwa_cognition/router.py` — legacy `ComputeRoute` with `execution_requires` reference
- `packages/heiwa_protocol/heiwa_protocol/routing.py` — `BrokerRouteResult` with `execution_requires`
- `packages/heiwa_sdk/heiwa_sdk/heiwaclaw.py` — execution adapter resolution
- `packages/heiwa_sdk/heiwa_sdk/tool_mesh.py` — tool execution under lease
- `packages/heiwa_sdk/heiwa_sdk/proposal_dispatch.py` — `dispatch_routable_proposals()` with capability gate
- `packages/heiwa_sdk/heiwa_sdk/db.py` — `get_eligible_nodes()` set intersection

## Degraded Mode

When an execution node goes offline:
- WebSocket drops → runtime marks node offline in local state and mirrors to STDB when online
- Pending proposals requiring that node's capabilities stay in `pending` state
- Other nodes pick up work they can satisfy
- Work requiring the offline node waits until it reconnects — no unsafe rerouting

## Execution Rules

- Do not hardcode GPU capabilities in `ai_router.json` or any static config. Capabilities are detected from hardware.
- Do not add external message brokers (NATS, Redis) for task queuing. STDB proposal/lease state machine is the queue.
- Do not add mesh VPNs (Tailscale) for node connectivity. `/ws/worker` outbound dial handles all networks.
- Do not call execution nodes "boost nodes." WSL is the primary execution server. MacBook is the operator node.
- Do not route GPU workloads to public edge or remote support services. MacBook/WSL own execution.
- Every meaningful execution should run under a lease.
- `HeiwaClaw` / `ToolMesh` should reject execution without a valid lease.
- Deny-first applies to both external provider calls and internal tool execution.

## Direction

- Replace `tick.py` polling with STDB subscription-based proposal assignment
- Nodes subscribe to proposals targeting their capability set
- Per-tool lease enforcement (not just per-proposal)
- Portable agent specs: agent definition decoupled from execution substrate
- Execution outcome feedback loop: write success/failure/latency back to STDB for routing optimization
