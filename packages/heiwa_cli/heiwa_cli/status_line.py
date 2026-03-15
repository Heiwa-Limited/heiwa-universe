"""Bottom status bar for the REPL — identity, route, rate capacity, hub status."""

from __future__ import annotations

from dataclasses import dataclass
from prompt_toolkit.formatted_text import HTML


_STATUS_COLORS = {
    "ready": "ansigray",
    "pass": "ansigreen",
    "fail": "ansired",
    "running": "ansiyellow",
    "accepted": "ansicyan",
    "waiting_approval": "ansiyellow",
    "rejected": "ansired",
    "error": "ansired",
}


@dataclass
class StatusLine:
    """Mutable state for the REPL bottom toolbar."""
    model: str = "auto"
    intent: str = "idle"
    tool: str = "idle"
    status: str = "ready"
    hub: str = "offline"
    turns: int = 0
    latency_ms: int | None = None
    identity: str = ""

    def toolbar(self) -> HTML:
        latency = f"{self.latency_ms}ms" if self.latency_ms is not None else "--"
        status_color = _STATUS_COLORS.get(self.status, "ansiwhite")

        parts = []

        # Identity (if set)
        if self.identity:
            parts.append(
                f"<style fg='ansigray'> {self.identity} </style>"
                "<style fg='ansigray'>\u2502</style>"
            )

        # Route: intent -> tool
        parts.append(
            f"<style fg='ansigray'> {self.intent}</style>"
            "<style fg='ansigray'>\u2192</style>"
            f"<style fg='ansicyan'>{self.tool}</style>"
        )

        # Model
        if self.model and self.model != "auto":
            short_model = self.model.split("/")[-1] if "/" in self.model else self.model
            parts.append(
                "<style fg='ansigray'>\u2502</style>"
                f"<style fg='ansiwhite'>{short_model}</style>"
            )

        # Status
        parts.append(
            "<style fg='ansigray'>\u2502</style>"
            f"<style fg='{status_color}'>{self.status}</style>"
        )

        # Latency
        if self.latency_ms is not None:
            parts.append(
                "<style fg='ansigray'> {latency}</style>".format(latency=latency)
            )

        # Turns
        parts.append(
            "<style fg='ansigray'>\u2502</style>"
            f"<style fg='ansigray'>t{self.turns}</style>"
        )

        # Hub
        hub_color = "ansigreen" if self.hub not in {"offline", "unreachable"} else "ansigray"
        parts.append(
            "<style fg='ansigray'>\u2502</style>"
            f"<style fg='{hub_color}'>hub:{self.hub}</style>"
        )

        return HTML("".join(parts))

    def update(
        self,
        *,
        model: str | None = None,
        intent: str | None = None,
        tool: str | None = None,
        status: str | None = None,
        hub: str | None = None,
        latency_ms: int | None = None,
        identity: str | None = None,
    ) -> None:
        if model is not None:
            self.model = model
        if intent is not None:
            self.intent = intent
        if tool is not None:
            self.tool = tool
        if status is not None:
            self.status = status
        if hub is not None:
            self.hub = hub
        if latency_ms is not None:
            self.latency_ms = latency_ms
        if identity is not None:
            self.identity = identity
