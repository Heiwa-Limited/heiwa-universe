"""Public package exports for the Python compatibility SDK.

Keep this module lightweight. Importing a narrow surface such as
``heiwa_sdk.vault`` must not force the legacy OpenClaw gateway or optional
protocol packages into memory.
"""

from __future__ import annotations

from importlib import import_module
from typing import Any

__version__ = "0.4.0"

_EXPORTS: dict[str, tuple[str, str]] = {
    "settings": ("heiwa_sdk.config", "settings"),
    "load_swarm_env": ("heiwa_sdk.config", "load_swarm_env"),
    "Database": ("heiwa_sdk.db", "Database"),
    "OpenClaw": ("heiwa_sdk.heiwaclaw", "OpenClaw"),
    "OpenClawDispatch": ("heiwa_sdk.heiwaclaw", "OpenClawDispatch"),
    "HeiwaClawGateway": ("heiwa_sdk.heiwaclaw", "HeiwaClawGateway"),
    "HeiwaClawDispatch": ("heiwa_sdk.heiwaclaw", "HeiwaClawDispatch"),
    "ModelRouter": ("heiwa_sdk.routing", "ModelRouter"),
    "MCPBridge": ("heiwa_sdk.mcp", "MCPBridge"),
    "MemoryService": ("heiwa_sdk.memory", "MemoryService"),
    "MissionService": ("heiwa_sdk.mission", "MissionService"),
    "redact_any": ("heiwa_sdk.security", "redact_any"),
    "redact_text": ("heiwa_sdk.security", "redact_text"),
    "HubStateService": ("heiwa_sdk.state", "HubStateService"),
    "run_cmd": ("heiwa_sdk.utils", "run_cmd"),
    "InstanceVault": ("heiwa_sdk.vault", "InstanceVault"),
    "HeiwaBench": ("heiwa_sdk.bench", "HeiwaBench"),
    "HeiwaCellCatalog": ("heiwa_sdk.cells", "HeiwaCellCatalog"),
    "FastPathTurn": ("heiwa_sdk.operator_surface", "FastPathTurn"),
    "WELCOME_SUGGESTIONS": ("heiwa_sdk.operator_surface", "WELCOME_SUGGESTIONS"),
    "maybe_fast_path_turn": ("heiwa_sdk.operator_surface", "maybe_fast_path_turn"),
    "operator_display_name": ("heiwa_sdk.operator_surface", "operator_display_name"),
    "ProviderRegistry": ("heiwa_sdk.provider_registry", "ProviderRegistry"),
}

__all__ = sorted(_EXPORTS) + ["__version__"]


def __getattr__(name: str) -> Any:
    try:
        module_name, attr_name = _EXPORTS[name]
    except KeyError as exc:
        raise AttributeError(f"module 'heiwa_sdk' has no attribute {name!r}") from exc
    module = import_module(module_name)
    value = getattr(module, attr_name)
    globals()[name] = value
    return value
