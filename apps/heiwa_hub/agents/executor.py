"""Backward-compatible alias. See heiwaclaw.py."""
from heiwa_hub.agents.heiwaclaw import HeiwaClawAgent as ExecutorAgent
from heiwa_sdk.db import Database  # re-export for test patch paths

__all__ = ["ExecutorAgent", "Database"]
