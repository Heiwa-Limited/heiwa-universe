# Heiwa Hub HTTP/WS Cutover

## Why this packet exists

The Heiwa control plane moved from the legacy NATS-centric runtime path to a hub-native HTTP/WebSocket and local-bus transport model. The CLI operator surface also shifted toward a single-ingress shell with scale-to-zero fast paths for trivial turns.

## Runtime changes

- Railway hub now runs the current HTTP/WebSocket ingress path:
  - `POST /tasks`
  - `GET /tasks/{task_id}`
  - `WS /ws/tasks/{task_id}`
  - `WS /ws/worker`
- Local bus replaced NATS as the in-process transport between hub agents.
- Remote MacBook/WSL workers now connect outbound to the hub over websocket instead of relying on local broker topology.
- Provider routing is now explicit across OAuth CLI lanes and OpenClaw-backed lanes.

## Operator surface changes

- `heiwa` now behaves like a single-composer operator shell in a real terminal.
- Fresh sessions show curated suggested prompts rather than a blank surface.
- Simple turns like `hi`, `help`, and acknowledgements resolve locally without spending provider capacity.
- Route/tool/mode telemetry is visible in a compact footer instead of verbose transport logs.

## Domain intent

- `api.heiwa.ltd` remains the intended runtime ingress.
- `heiwa.ltd`, `status.heiwa.ltd`, and `docs.heiwa.ltd` are still intended to be separate public shells rather than all pointing at the Railway hub.

## Deployment note

- The current Heiwa runtime commit is on `main` at `e5f5357`.
- Railway required a new `HEIWA_MASTER_KEY` secret to boot the updated hub image.
- Cloudflare custom-domain behavior still needs human follow-up because `api.heiwa.ltd` is returning edge `403` while the Railway-hosted hub process now boots.
