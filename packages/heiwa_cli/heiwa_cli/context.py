"""Auto-load workspace and operator context on CLI startup."""

from __future__ import annotations

import datetime as dt
import json
import logging
import os
import sys
import uuid
from pathlib import Path
from typing import Any

logger = logging.getLogger("CLI")


def find_root(start: Path | None = None) -> Path:
    """Walk up from start to find the Heiwa monorepo root."""
    explicit = os.environ.get("HEIWA_ROOT")
    if explicit:
        p = Path(explicit).resolve()
        if (p / "apps").exists() and (p / "packages").exists():
            return p
    current = (start or Path.cwd()).resolve()
    for _ in range(6):
        if (current / "apps").exists() and (current / "packages").exists():
            return current
        current = current.parent
    return Path.cwd()


def ensure_sys_path(root: Path) -> None:
    """Add package and app directories to sys.path for direct imports."""
    paths = [
        root / "packages" / "heiwa_cognition",
        root / "packages" / "heiwa_sdk",
        root / "packages" / "heiwa_protocol",
        root / "packages" / "heiwa_identity",
        root / "packages" / "heiwa_ui",
        root / "apps",
    ]
    for p in paths:
        s = str(p)
        if s not in sys.path:
            sys.path.insert(0, s)


def _load_json(path: Path, fallback: Any) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        if path.exists():
            logger.debug("Failed to parse JSON from %s", path, exc_info=True)
        return fallback


def load_ai_router(root: Path) -> dict[str, Any]:
    """Load config/swarm/ai_router.json."""
    return _load_json(root / "config" / "swarm" / "ai_router.json", {})


class CLIContext:
    """Loaded once at CLI startup. Shared by repl, oneshot, auth, and commands."""

    PROVIDER_ALIASES = {
        "gemini": "google-gemini-cli",
        "google-gemini-cli": "google-gemini-cli",
        "codex": "codex",
        "claude": "claude-code",
        "claude-code": "claude-code",
        "antigravity": "google-antigravity",
        "google-antigravity": "google-antigravity",
    }

    def __init__(self, root: Path | None = None) -> None:
        self.root = root or find_root()
        os.environ.setdefault("HEIWA_ROOT", str(self.root))
        os.environ.setdefault("HEIWA_WORKSPACE_ROOT", str(self.root))
        ensure_sys_path(self.root)

        self.ai_router = load_ai_router(self.root)
        self.node_id = os.getenv("HEIWA_NODE_ID", "macbook@heiwa-agile")
        self.auth_token = os.getenv("HEIWA_AUTH_TOKEN", "")
        self.trace = os.getenv("HEIWA_TRACE", "").strip().lower() in {"1", "true", "yes", "on"}
        self.session_id = os.getenv("HEIWA_SESSION_ID", f"cli-session-{uuid.uuid4().hex[:10]}")

        # HEIWA_MODE: single_node (default) skips hub entirely;
        # distributed probes hub URLs before local fallback.
        _mode_raw = os.getenv("HEIWA_MODE", "").strip().lower()
        if not _mode_raw:
            # Treat HEIWA_LLM_MODE=local_only as compatibility alias
            _llm_mode = os.getenv("HEIWA_LLM_MODE", "local_only").strip().lower()
            _mode_raw = "single_node" if _llm_mode == "local_only" else "distributed"
        self.mode = _mode_raw if _mode_raw in {"single_node", "distributed"} else "single_node"

        self.state_dir = Path.home() / ".heiwa"
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self._cache_dir = self.state_dir / "cache"
        self._cache_dir.mkdir(parents=True, exist_ok=True)
        self._provider_cache_path = self._cache_dir / "provider_accounts.json"
        self._summary_cache_path = self._cache_dir / "session_summaries.json"
        self._unsynced_approvals_path = self._cache_dir / "unsynced_approvals.json"

        self.context_paths = {
            "SOUL.md": self.root / "SOUL.md",
            "AGENTS.md": self.root / "AGENTS.md",
            "HEIWA.md": self.root / "HEIWA.md",
            "ai_router.json": self.root / "config" / "swarm" / "ai_router.json",
            "profiles.json": self.root / "config" / "identities" / "profiles.json",
        }

        self.turn_history: list[dict[str, Any]] = []
        self.loaded_summaries = self.load_recent_session_summaries(limit=5)

        # Lazy-loaded to avoid import cost on simple commands
        self._enrichment = None
        self._gateway = None
        self._identity_selector = None
        self._db = None
        self._provider_registry = None

    @property
    def operator_name(self) -> str:
        from heiwa_sdk.operator_surface import operator_display_name

        return operator_display_name(self.node_id)

    @property
    def enrichment(self):
        if self._enrichment is None:
            from heiwa_cognition.enrichment import BrokerEnrichmentService

            self._enrichment = BrokerEnrichmentService(
                identity_selector=self.identity_selector,
            )
        return self._enrichment

    @property
    def gateway(self):
        if self._gateway is None:
            from heiwa_sdk.heiwaclaw import HeiwaClawGateway

            self._gateway = HeiwaClawGateway(self.root)
        return self._gateway

    @property
    def identity_selector(self):
        if self._identity_selector is None:
            from heiwa_cognition.identity import IdentitySelector

            self._identity_selector = IdentitySelector(
                self.root / "config" / "identities" / "profiles.json"
            )
        return self._identity_selector

    @property
    def db(self):
        if self._db is None:
            from heiwa_sdk.db import Database

            self._db = Database()
            # Ensure schema is initialized for SQLite/Postgres backends.
            # Safe to call repeatedly — all DDL uses IF NOT EXISTS.
            if self._db.state_backend != "spacetimedb":
                try:
                    self._db.init_db()
                except Exception:
                    logger.debug("Failed to initialize database", exc_info=True)
        return self._db

    @property
    def provider_registry(self):
        if self._provider_registry is None:
            from heiwa_sdk.provider_registry import ProviderRegistry

            self._provider_registry = ProviderRegistry(self.root)
        return self._provider_registry

    def hub_url_candidates(self) -> list[str]:
        from heiwa_sdk.config import hub_url_candidates

        return hub_url_candidates()

    def provider_alias(self, provider: str) -> str:
        return self.PROVIDER_ALIASES.get(str(provider or "").strip().lower(), str(provider or "").strip())

    def provider_account_id(self, provider: str) -> str:
        return f"{self.node_id}:{self.provider_alias(provider)}"

    def provider_models(self, provider: str) -> list[str]:
        normalized = self.provider_alias(provider)
        registry = self.ai_router.get("models", {}).get("registry", {})
        models: list[str] = []
        for entry in registry.values():
            if str(entry.get("provider") or "") == normalized:
                model_id = str(entry.get("id") or "")
                if model_id:
                    models.append(model_id)
        return sorted(dict.fromkeys(models))

    def read_provider_cache(self) -> dict[str, Any]:
        return _load_json(self._provider_cache_path, {})

    def write_provider_cache(self, payload: dict[str, Any]) -> None:
        self._provider_cache_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")

    def read_unsynced_approval_events(self) -> list[dict[str, Any]]:
        return _load_json(self._unsynced_approvals_path, [])

    def append_unsynced_approval_event(self, event: dict[str, Any]) -> None:
        events = self.read_unsynced_approval_events()
        events.append(event)
        self._unsynced_approvals_path.write_text(json.dumps(events, indent=2), encoding="utf-8")

    def remember_turn(self, role: str, content: str, **extra: Any) -> None:
        self.turn_history.append(
            {
                "role": role,
                "content": str(content or "").strip(),
                "timestamp": dt.datetime.now(dt.timezone.utc).isoformat(),
                **extra,
            }
        )
        if len(self.turn_history) > 200:
            self.turn_history = self.turn_history[-200:]

    def build_session_summary(self, summary_text: str | None = None) -> dict[str, Any]:
        created_at = dt.datetime.now(dt.timezone.utc).isoformat()
        recent_turns = self.turn_history[-12:]
        if summary_text:
            text = summary_text.strip()
        else:
            bullets = []
            for turn in recent_turns:
                role = turn.get("role", "system")
                content = str(turn.get("content") or "").strip().replace("\n", " ")
                if content:
                    bullets.append(f"{role}: {content[:220]}")
            text = "\n".join(bullets) if bullets else "No conversation content to summarize."
        return {
            "summary_id": f"summary-{uuid.uuid4().hex[:12]}",
            "session_id": self.session_id,
            "node_id": self.node_id,
            "created_at": created_at,
            "summary_text": text,
            "metadata": {
                "turn_count": len(self.turn_history),
                "context_paths": {name: str(path) for name, path in self.context_paths.items() if path.exists()},
            },
        }

    def save_session_summary(self, summary_data: dict[str, Any]) -> bool:
        saved = False
        try:
            saved = bool(self.db.write_session_summary(summary_data))
        except Exception:
            logger.debug("Failed to write session summary to DB", exc_info=True)
            saved = False

        cached = _load_json(self._summary_cache_path, [])
        cached.insert(0, summary_data)
        self._summary_cache_path.write_text(json.dumps(cached[:100], indent=2), encoding="utf-8")
        self.loaded_summaries = self.load_recent_session_summaries(limit=5)
        return saved

    def drain_unsynced_approvals(self) -> int:
        """Attempt to sync any locally-queued approval decisions to the hub.

        Returns the number of events successfully synced. Safe to call at startup.
        """
        events = self.read_unsynced_approval_events()
        if not events:
            return 0

        synced = 0
        remaining: list[dict[str, Any]] = []
        hub_urls = self.hub_url_candidates()
        if not hub_urls:
            return 0

        for event in events:
            if event.get("synced"):
                continue
            task_id = event.get("task_id")
            status = event.get("status")
            if not task_id or status not in {"approved", "rejected"}:
                remaining.append(event)
                continue

            endpoint = "approve" if status == "approved" else "reject"
            pushed = False
            for hub_url in hub_urls:
                try:
                    import urllib.request
                    body = json.dumps({
                        "actor": self.operator_name,
                        "reason": event.get("reason", "deferred_sync"),
                    }).encode()
                    req = urllib.request.Request(
                        f"{hub_url}/tasks/{task_id}/{endpoint}",
                        data=body,
                        headers={
                            "Content-Type": "application/json",
                            "Authorization": f"Bearer {self.auth_token}",
                        },
                        method="POST",
                    )
                    with urllib.request.urlopen(req, timeout=5):
                        pushed = True
                        synced += 1
                        break
                except Exception:
                    logger.debug("Failed to sync approval %s to %s", task_id, hub_url, exc_info=True)
                    continue
            if not pushed:
                remaining.append(event)

        # Overwrite with only the events that failed to sync
        self._unsynced_approvals_path.write_text(
            json.dumps(remaining, indent=2), encoding="utf-8"
        )
        return synced

    def load_recent_session_summaries(self, limit: int = 5) -> list[dict[str, Any]]:
        try:
            rows = self.db.list_session_summaries(node_id=self.node_id, limit=limit)
            if rows:
                return rows
        except Exception:
            logger.debug("Failed to load session summaries from DB", exc_info=True)
        return _load_json(self._summary_cache_path, [])[:limit]

    def build_system_prompt(
        self,
        identity_id: str | None = None,
        intent_class: str | None = None,
        risk_level: str | None = None,
    ) -> str:
        """Build a Heiwa-aware system prompt for the local LLM.

        Includes persona/soul, current session identity, and recent memory.
        Kept concise to avoid eating into the model's context window.
        """
        parts: list[str] = []

        # Core soul/persona (read once per session from file)
        soul_path = self.root / "config" / "identities" / "soul" / "core.md"
        if soul_path.exists():
            try:
                soul = soul_path.read_text(encoding="utf-8").strip()
                # Trim to first 800 chars — enough to capture voice + rules
                parts.append(soul[:800])
            except Exception:
                logger.debug("Failed to read soul from %s", soul_path, exc_info=True)

        # Operational context
        node = self.node_id
        parts.append(
            f"You are Heiwa, running on {node}. "
            f"You are a distributed AI operating system, not a generic chatbot. "
            f"Stay grounded in actual system state. Skip filler; be direct and useful."
        )

        if identity_id:
            parts.append(f"Active cell identity: {identity_id}")
        if intent_class:
            parts.append(f"Task intent: {intent_class}" + (f" | risk: {risk_level}" if risk_level else ""))

        # Recent session memory (last 2 summaries, trimmed)
        summaries = (self.loaded_summaries or [])[:2]
        if summaries:
            mem_lines = []
            for s in summaries:
                text = str(s.get("summary_text") or "").strip()[:300]
                if text:
                    mem_lines.append(text)
            if mem_lines:
                parts.append("Recent session context:\n" + "\n---\n".join(mem_lines))

        # Recent turn history (last 4 turns, trimmed)
        recent_turns = self.turn_history[-4:] if self.turn_history else []
        if recent_turns:
            history_lines = []
            for turn in recent_turns:
                role = turn.get("role", "user")
                content = str(turn.get("content") or "").strip()[:200]
                if content:
                    history_lines.append(f"{role}: {content}")
            if history_lines:
                parts.append("Recent conversation:\n" + "\n".join(history_lines))

        return "\n\n".join(p for p in parts if p.strip())
