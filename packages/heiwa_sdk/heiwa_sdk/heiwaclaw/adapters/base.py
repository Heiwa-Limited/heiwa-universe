"""Base class for HeiwaClaw provider adapters."""
from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any
from heiwa_protocol.routing import BrokerRouteResult


class BaseClawAdapter(ABC):
    """Abstract base class for all execution adapters."""

    @abstractmethod
    async def execute(
        self,
        route: BrokerRouteResult,
        instruction: str,
        env: dict[str, str],
        model: str | None = None,
    ) -> tuple[int, str]:
        """Execute the instruction via the specific provider tool."""
        pass
