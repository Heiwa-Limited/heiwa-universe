# agents/ — Hub Runtime Agents

All agents extend `BaseAgent` from `base.py` and communicate via `LocalBusTransport`.

## Agent Roster

| Agent | File | Role | Always On? |
| --- | --- | --- | --- |
| Spine | `spine.py` | Fleet orchestration, node registry, heartbeats, request routing | Yes |
| Executor | `executor.py` | Claims and executes tasks via HeiwaClaw + ToolMesh | Yes |
| Captain | `captain.py` | Event-driven orchestrator (Gemini Flash). Monitors health, delegates, communicates via Discord | Yes |
| Telemetry | `telemetry.py` | System metrics collection and reporting | Yes |
| Messenger | `messenger.py` | Discord integration (reads/writes Discord channels) | Only when `DISCORD_TOKEN` is set |

## BaseAgent Contract (`base.py`)

- `start()` — initialize transport connection
- `run()` — main async loop (must be overridden)
- `speak(subject, data)` — publish to local bus
- `listen(subject, handler)` — subscribe to local bus topic
- `think(prompt)` — internal reasoning via LLM
- `shutdown()` — graceful teardown

## Protocol Subjects

Event types are defined in `packages/heiwa_protocol/heiwa_protocol/protocol.py` as the `Subject` enum. Key subjects: `CORE_REQUEST`, `TASK_EXEC`, `TASK_EXEC_RESULT`, `TASK_STATUS`, `NODE_HEARTBEAT`, `LOG_ERROR`, `LOG_INFO`, `SWARM_STATUS_REPORT`.

## Adding a New Agent

1. Create `new_agent.py` extending `BaseAgent`
2. Implement `async def run(self)` with `await self.start()` then event loop
3. Wire into `main.py` boot sequence (follow Captain/Messenger pattern)
4. Add to the roster table above
