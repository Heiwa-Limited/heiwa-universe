"""Backward-compatible alias. See heiwaclaw.py."""
from heiwa_hub.agents.heiwaclaw import HeiwaClawAgent as HeiwaAgent
from heiwa_sdk.audit import RepoAuditor  # re-export for test patch paths

__all__ = ["HeiwaAgent", "RepoAuditor"]
