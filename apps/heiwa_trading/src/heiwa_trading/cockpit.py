from __future__ import annotations

import json
import subprocess
import time
import webbrowser
from dataclasses import dataclass
from datetime import datetime, timezone
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse
from uuid import uuid4

from heiwa_trading.config import PROJECT_ROOT
from heiwa_trading.market_data import fetch_markets
from heiwa_trading.supervisor import (
    build_supervisor_summary,
    create_supervisor_state,
    load_supervisor_state,
    save_supervisor_state,
    supervisor_tick,
)


def resolve_agent_root(project_root: Path) -> Path:
    """Map an app-local project root to the broader workspace root when available."""
    return project_root.parents[1] if len(project_root.parents) > 1 else project_root


MAC_AGENT_ROOT = resolve_agent_root(PROJECT_ROOT)
WEB_DIR = Path(__file__).resolve().parent / "web"
LOG_DIR = Path.home() / ".heiwa" / "logs"
OPENCLAW_STATE_DIR = Path.home() / ".heiwa" / "openclaw"
COCKPIT_SETTINGS_PATH = PROJECT_ROOT / "runtime" / "cockpit_settings.json"
COCKPIT_CHAT_PATH = PROJECT_ROOT / "runtime" / "cockpit_chat.jsonl"
DEFAULT_COCKPIT_HOST = "127.0.0.1"
DEFAULT_COCKPIT_PORT = 19890
DEFAULT_EVENT_INTERVAL_SECONDS = 3
DEFAULT_CHAT_LIMIT = 50
DEFAULT_PANEL_ORDER = (
    "hero",
    "status",
    "controls",
    "chat",
    "cohort",
    "leaderboard",
    "opportunities",
    "live",
    "movers",
    "logs",
    "openclaw",
)
DEFAULT_VISIBLE_PANELS = (
    "hero",
    "status",
    "controls",
    "chat",
    "cohort",
    "leaderboard",
    "opportunities",
    "live",
    "openclaw",
)
DEFAULT_PANEL_TITLES = {
    "hero": "Mission Brief",
    "status": "System Status",
    "controls": "Operator Controls",
    "chat": "Operator Chat",
    "cohort": "Active Cohort",
    "leaderboard": "Cohort Leaderboard",
    "opportunities": "Best Opportunities",
    "live": "Live Top 5",
    "movers": "CoinMarketCap Movers",
    "logs": "Supervisor Log",
    "openclaw": "OpenClaw Bridge",
}


def get_default_cockpit_settings() -> dict[str, object]:
    return {
        "visible_panels": list(DEFAULT_VISIBLE_PANELS),
        "panel_titles": dict(DEFAULT_PANEL_TITLES),
        "density": "dense",
    }


def load_cockpit_settings(path: Path = COCKPIT_SETTINGS_PATH) -> dict[str, object]:
    settings = get_default_cockpit_settings()
    if not path.exists():
        return settings
    try:
        payload = json.loads(path.read_text())
    except json.JSONDecodeError:
        return settings
    if not isinstance(payload, dict):
        return settings
    visible_panels = payload.get("visible_panels")
    if isinstance(visible_panels, list):
        normalized_panels = [str(item) for item in visible_panels if str(item) in DEFAULT_PANEL_TITLES]
        if normalized_panels:
            settings["visible_panels"] = normalized_panels
    panel_titles = payload.get("panel_titles")
    if isinstance(panel_titles, dict):
        settings["panel_titles"].update({str(k): str(v) for k, v in panel_titles.items() if str(k) in DEFAULT_PANEL_TITLES})
    density = payload.get("density")
    if isinstance(density, str) and density in {"dense", "comfortable"}:
        settings["density"] = density
    return settings


def save_cockpit_settings(settings: dict[str, object], path: Path = COCKPIT_SETTINGS_PATH) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(settings, indent=2) + "\n")


def load_cockpit_chat(path: Path = COCKPIT_CHAT_PATH, *, limit: int = DEFAULT_CHAT_LIMIT) -> list[dict[str, object]]:
    if not path.exists():
        return []
    messages: list[dict[str, object]] = []
    for raw_line in path.read_text(errors="replace").splitlines():
        if not raw_line.strip():
            continue
        try:
            payload = json.loads(raw_line)
        except json.JSONDecodeError:
            continue
        if isinstance(payload, dict):
            messages.append(payload)
    if limit > 0:
        return messages[-limit:]
    return messages


def save_cockpit_chat(messages: list[dict[str, object]], path: Path = COCKPIT_CHAT_PATH) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not messages:
        path.write_text("")
        return
    payload = "\n".join(json.dumps(message) for message in messages) + "\n"
    path.write_text(payload)


def append_cockpit_chat_message(
    *,
    role: str,
    text: str,
    kind: str = "note",
    action: str | None = None,
    status: str = "queued",
    path: Path = COCKPIT_CHAT_PATH,
) -> dict[str, object]:
    entry = {
        "id": f"chat-{uuid4().hex[:10]}",
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "role": role,
        "kind": kind,
        "text": text.strip(),
        "status": status,
    }
    if action:
        entry["action"] = action
    messages = load_cockpit_chat(path, limit=0)
    messages.append(entry)
    save_cockpit_chat(messages[-200:], path=path)
    return entry


def _load_json_file(path: Path, default: object) -> object:
    if not path.exists():
        return default
    try:
        payload = json.loads(path.read_text())
    except json.JSONDecodeError:
        return default
    return payload


def _device_values(payload: object) -> list[dict[str, object]]:
    if isinstance(payload, dict):
        return [item for item in payload.values() if isinstance(item, dict)]
    return []


def load_openclaw_status(*, state_dir: Path = OPENCLAW_STATE_DIR, cockpit_port: int = DEFAULT_COCKPIT_PORT) -> dict[str, object]:
    config = _load_json_file(state_dir / "openclaw.json", {})
    config = config if isinstance(config, dict) else {}
    gateway = config.get("gateway") if isinstance(config.get("gateway"), dict) else {}
    control_ui = gateway.get("controlUi") if isinstance(gateway, dict) and isinstance(gateway.get("controlUi"), dict) else {}
    allowed_origins_raw = control_ui.get("allowedOrigins", [])
    if isinstance(allowed_origins_raw, list):
        allowed_origins = [str(origin) for origin in allowed_origins_raw if isinstance(origin, (str, int))]
    else:
        allowed_origins = []
    paired = _device_values(_load_json_file(state_dir / "devices" / "paired.json", {}))
    pending = _device_values(_load_json_file(state_dir / "devices" / "pending.json", {}))
    paired_control_ui = [device for device in paired if device.get("clientId") == "openclaw-control-ui"]
    pending_control_ui = [device for device in pending if device.get("clientId") == "openclaw-control-ui"]
    cockpit_origin = f"http://127.0.0.1:{cockpit_port}"
    localhost_origin = f"http://localhost:{cockpit_port}"
    origin_allowed = cockpit_origin in allowed_origins or localhost_origin in allowed_origins

    issues: list[str] = []
    if not paired_control_ui:
        issues.append("control UI not paired")
    elif len(paired_control_ui) > 1:
        issues.append(f"{len(paired_control_ui)} control UI devices paired")
    if pending_control_ui:
        issues.append(f"{len(pending_control_ui)} pending control UI request(s)")
    if not origin_allowed:
        issues.append("cockpit origin not allowed")

    if not issues:
        status = "ok"
        message = "OpenClaw control UI paired and reachable."
    elif not paired_control_ui:
        status = "bad"
        message = "OpenClaw control UI needs pairing."
    else:
        status = "warn"
        message = "; ".join(issues)

    return {
        "status": status,
        "message": message,
        "cockpit_origin": cockpit_origin,
        "origin_allowed": origin_allowed,
        "allowed_origins": allowed_origins,
        "paired_devices": len(paired),
        "paired_control_ui_devices": len(paired_control_ui),
        "pending_requests": len(pending),
        "pending_control_ui_requests": len(pending_control_ui),
        "paired_control_ui_device_ids": [str(device.get("deviceId", "")) for device in paired_control_ui if device.get("deviceId")],
        "pending_request_ids": [str(device.get("requestId", "")) for device in pending_control_ui if device.get("requestId")],
    }


def parse_dashboard_url(output: str) -> str | None:
    prefix = "Dashboard URL: "
    for line in output.splitlines():
        if line.startswith(prefix):
            return line[len(prefix) :].strip()
    return None


def tail_log_file(path: Path, *, line_limit: int = 12) -> list[str]:
    if not path.exists():
        return []
    return path.read_text(errors="replace").splitlines()[-line_limit:]


def _safe_json_loads(raw_value: str) -> dict[str, object]:
    try:
        payload = json.loads(raw_value)
    except json.JSONDecodeError:
        return {"output": raw_value.strip()}
    return payload if isinstance(payload, dict) else {"payload": payload}


def _run_mac_agent_command(root_dir: Path, *args: str) -> str:
    try:
        completed = subprocess.run(
            [str(root_dir / "bin" / "mac-agent"), *args],
            capture_output=True,
            text=True,
            check=True,
        )
        return completed.stdout
    except FileNotFoundError:
        return ""


def load_market_supervisor_status(root_dir: Path) -> dict[str, object]:
    # On Railway (or any env without mac-agent binary), skip the subprocess call
    if not (root_dir / "bin" / "mac-agent").exists():
        return {"running": False, "mode": "on-demand"}
    try:
        return _safe_json_loads(_run_mac_agent_command(root_dir, "market-supervisor", "status"))
    except Exception:
        return {}


def build_agent_brief(
    *,
    summary: dict[str, object],
    market_supervisor_status: dict[str, object],
    openclaw_status: dict[str, object] | None = None,
) -> str:
    cohort = summary.get("cohort") or {}
    leaderboard = cohort.get("leaderboard") or []
    leader = leaderboard[0] if leaderboard else None
    opportunity = (summary.get("top_opportunities") or [None])[0]
    source_statuses = summary.get("sources") or {}
    source_note = ", ".join(f"{name}:{payload.get('status')}" for name, payload in source_statuses.items()) or "no sources"
    service_note = "running" if market_supervisor_status.get("running") else "stopped"

    parts = []
    if leader:
        parts.append(
            f"{leader.get('wallet_id')} {leader.get('label')} leads with equity {leader.get('equity')} across {leader.get('trade_count')} trades."
        )
    if opportunity:
        parts.append(
            f"Best setup is {opportunity.get('slug')} with ev {opportunity.get('avg_expected_value')} and {opportunity.get('buy_signals')} buy signals."
        )
    if openclaw_status and openclaw_status.get("status") != "ok":
        parts.append(f"OpenClaw {openclaw_status.get('status')}: {openclaw_status.get('message')}.")
    parts.append(f"Sources are {source_note}; supervisor is {service_note}.")
    return " ".join(parts)


def build_cockpit_snapshot(root_dir: Path = MAC_AGENT_ROOT) -> dict[str, object]:
    supervisor_summary = build_supervisor_summary(load_supervisor_state())
    try:
        market_supervisor_status = load_market_supervisor_status(root_dir)
    except Exception:
        market_supervisor_status = {}
    openclaw_status = load_openclaw_status()
    snapshot = {
        "timestamp": supervisor_summary.get("timestamp"),
        "settings": load_cockpit_settings(),
        "chat": {
            "messages": load_cockpit_chat(),
            "commands": [
                "/tick",
                "/init",
                "/leaderboard",
                "/top5",
                "/openclaw",
                "/start",
                "/stop",
                "/restart",
            ],
        },
        "agent_brief": build_agent_brief(
            summary=supervisor_summary,
            market_supervisor_status=market_supervisor_status,
            openclaw_status=openclaw_status,
        ),
        "best_opportunity": (supervisor_summary.get("top_opportunities") or [None])[0],
        "supervisor": supervisor_summary,
        "services": {
            "market_supervisor": market_supervisor_status,
        },
        "openclaw": {
            "profile": "heiwa-trading",
            "gateway_port": 19789,
            "state_dir": str(Path.home() / ".heiwa" / "openclaw"),
            "pairing": openclaw_status,
        },
        "logs": {
            "market_supervisor": tail_log_file(LOG_DIR / "market-supervisor.log"),
            "market_supervisor_error": tail_log_file(LOG_DIR / "market-supervisor.err.log"),
            "gateway": tail_log_file(LOG_DIR / "gateway.log"),
            "gateway_error": tail_log_file(LOG_DIR / "gateway.err.log"),
        },
    }
    return snapshot


def _do_tick() -> dict[str, object]:
    """Run a supervisor tick using the Python engine directly."""
    from datetime import datetime, timezone

    state = load_supervisor_state()
    markets = fetch_markets()
    ts = datetime.now(timezone.utc).isoformat()
    new_state, summary = supervisor_tick(state=state, markets=markets, timestamp=ts)
    save_supervisor_state(new_state)
    return summary


def run_cockpit_action(root_dir: Path, *, action: str, payload: dict[str, object] | None = None) -> dict[str, object]:
    payload = payload or {}
    if action == "tick_now":
        summary = _do_tick()
        return {"action": action, "result": summary}
    if action == "init_cohort":
        state = load_supervisor_state()
        fresh = create_supervisor_state(policy=state.policy)
        save_supervisor_state(fresh)
        return {"action": action, "result": {"message": "New supervisor state initialized"}}
    if action in ("start_supervisor", "stop_supervisor", "restart_supervisor"):
        return {
            "action": action,
            "result": {"message": f"{action} is not applicable — supervisor runs on-demand via tick"},
        }
    if action == "open_openclaw_dashboard":
        return {
            "action": action,
            "dashboard_url": None,
            "result": "OpenClaw is not available on Railway",
            "openclaw": load_openclaw_status(),
        }
    if action == "submit_chat":
        text = str(payload.get("text", "")).strip()
        if not text:
            raise ValueError("text must not be empty")
        command = _parse_chat_command(text)
        message = append_cockpit_chat_message(
            role="operator",
            text=text,
            kind="command" if command else "note",
            action=command,
            status="queued",
        )
        result: dict[str, object] = {"action": action, "message": message}
        if command:
            result["command"] = command
            if command in {"leaderboard", "top5"}:
                summary = build_supervisor_summary(load_supervisor_state())
                result["command_result"] = {
                    "leaderboard": summary.get("cohort", {}).get("leaderboard", []) if command == "leaderboard" else summary.get("live_top5", []),
                }
            else:
                result["command_result"] = run_cockpit_action(root_dir, action=command, payload=payload)
            append_cockpit_chat_message(
                role="system",
                text=f"Command {text!r} completed.",
                kind="result",
                action=command,
                status="completed",
            )
        return result
    if action == "set_visible_panels":
        visible_panels = payload.get("visible_panels")
        if not isinstance(visible_panels, list) or not visible_panels:
            raise ValueError("visible_panels must be a non-empty list")
        settings = load_cockpit_settings()
        settings["visible_panels"] = [str(item) for item in visible_panels if str(item) in DEFAULT_PANEL_TITLES]
        if not settings["visible_panels"]:
            raise ValueError("visible_panels must include at least one known panel")
        save_cockpit_settings(settings)
        return {"action": action, "settings": settings}
    if action == "set_density":
        density = str(payload.get("density", "")).strip()
        if density not in {"dense", "comfortable"}:
            raise ValueError("density must be dense or comfortable")
        settings = load_cockpit_settings()
        settings["density"] = density
        save_cockpit_settings(settings)
        return {"action": action, "settings": settings}
    raise ValueError(f"Unsupported cockpit action: {action}")


def _parse_chat_command(text: str) -> str | None:
    normalized = text.strip().lower()
    if normalized.startswith("/"):
        normalized = normalized[1:]
    normalized = normalized.replace("_", " ")
    normalized = " ".join(normalized.split())
    aliases = {
        "tick": "tick_now",
        "tick now": "tick_now",
        "init": "init_cohort",
        "init cohort": "init_cohort",
        "leaderboard": "leaderboard",
        "show leaderboard": "leaderboard",
        "top5": "top5",
        "live": "top5",
        "show top5": "top5",
        "openclaw": "open_openclaw_dashboard",
        "dashboard": "open_openclaw_dashboard",
        "open dashboard": "open_openclaw_dashboard",
        "start": "start_supervisor",
        "start supervisor": "start_supervisor",
        "stop": "stop_supervisor",
        "stop supervisor": "stop_supervisor",
        "restart": "restart_supervisor",
        "restart supervisor": "restart_supervisor",
    }
    return aliases.get(normalized)


@dataclass
class CockpitServerConfig:
    root_dir: Path = MAC_AGENT_ROOT
    host: str = DEFAULT_COCKPIT_HOST
    port: int = DEFAULT_COCKPIT_PORT
    event_interval_seconds: int = DEFAULT_EVENT_INTERVAL_SECONDS
    open_browser: bool = True


class _CockpitHTTPServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, server_address: tuple[str, int], request_handler: type[BaseHTTPRequestHandler], *, config: CockpitServerConfig) -> None:
        super().__init__(server_address, request_handler)
        self.config = config


class CockpitRequestHandler(BaseHTTPRequestHandler):
    server: _CockpitHTTPServer

    def log_message(self, format: str, *args: object) -> None:  # noqa: A003
        return

    def _send_bytes(self, status: HTTPStatus, content_type: str, payload: bytes) -> None:
        self.send_response(status.value)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(payload)

    def _send_json(self, payload: dict[str, object], status: HTTPStatus = HTTPStatus.OK) -> None:
        self._send_bytes(status, "application/json; charset=utf-8", json.dumps(payload).encode("utf-8"))

    def _serve_asset(self, filename: str, content_type: str) -> None:
        path = WEB_DIR / filename
        self._send_bytes(HTTPStatus.OK, content_type, path.read_bytes())

    def do_GET(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path in {"/", "/index.html"}:
            self._serve_asset("cockpit.html", "text/html; charset=utf-8")
            return
        if path == "/cockpit.css":
            self._serve_asset("cockpit.css", "text/css; charset=utf-8")
            return
        if path == "/cockpit.js":
            self._serve_asset("cockpit.js", "application/javascript; charset=utf-8")
            return
        if path == "/api/state":
            self._send_json(build_cockpit_snapshot(self.server.config.root_dir))
            return
        if path == "/api/settings":
            self._send_json(load_cockpit_settings())
            return
        if path == "/api/events":
            self.send_response(HTTPStatus.OK.value)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Connection", "keep-alive")
            self.end_headers()
            try:
                while True:
                    snapshot = json.dumps(build_cockpit_snapshot(self.server.config.root_dir))
                    self.wfile.write(f"data: {snapshot}\n\n".encode("utf-8"))
                    self.wfile.flush()
                    time.sleep(self.server.config.event_interval_seconds)
            except (BrokenPipeError, ConnectionResetError):
                return
        self._send_json({"error": "not_found"}, status=HTTPStatus.NOT_FOUND)

    def do_POST(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path != "/api/action":
            self._send_json({"error": "not_found"}, status=HTTPStatus.NOT_FOUND)
            return
        body = self.rfile.read(int(self.headers.get("Content-Length", "0") or "0"))
        try:
            payload = json.loads(body or "{}")
            action = str(payload["action"])
        except (json.JSONDecodeError, KeyError, TypeError):
            self._send_json({"error": "invalid_request"}, status=HTTPStatus.BAD_REQUEST)
            return
        try:
            result = run_cockpit_action(self.server.config.root_dir, action=action, payload=dict(payload))
        except ValueError as exc:
            self._send_json({"error": str(exc)}, status=HTTPStatus.BAD_REQUEST)
            return
        except subprocess.CalledProcessError as exc:
            self._send_json(
                {
                    "error": "action_failed",
                    "action": action,
                    "stdout": exc.stdout,
                    "stderr": exc.stderr,
                },
                status=HTTPStatus.INTERNAL_SERVER_ERROR,
            )
            return
        result["snapshot"] = build_cockpit_snapshot(self.server.config.root_dir)
        self._send_json(result)


def serve_cockpit(config: CockpitServerConfig) -> int:
    url = f"http://{config.host}:{config.port}"
    server = _CockpitHTTPServer((config.host, config.port), CockpitRequestHandler, config=config)
    print(f"Cockpit URL: {url}", flush=True)
    if config.open_browser:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0
