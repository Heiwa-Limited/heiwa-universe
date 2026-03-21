# apps/heiwa_hub/tests/test_program_validation.py
"""Tests for HeiwaClaw ExecutionProgram validation."""
import pytest
from unittest.mock import MagicMock, AsyncMock, patch
from heiwa_protocol.program import ExecutionProgram


class TestProgramValidation:
    def _make_agent(self):
        with patch("heiwa_hub.agents.heiwaclaw.RepoAuditor"):
            from heiwa_hub.agents.heiwaclaw import HeiwaClawAgent
            agent = HeiwaClawAgent()
            agent.db = MagicMock()
            agent.db.stdb = MagicMock()
            agent._llm = MagicMock()
            agent.speak = AsyncMock()
            return agent

    def test_validate_acceptance_pass(self):
        agent = self._make_agent()
        prog = ExecutionProgram(
            objective="deploy hub",
            acceptance=["returned 200", "health endpoint"],
        )
        # Both criteria are contiguous substrings of the output (case-insensitive)
        result = agent._validate_acceptance(prog, "Health endpoint returned 200 OK")
        assert result["passed"] is True
        assert result["matched"] == ["returned 200", "health endpoint"]
        assert result["unmatched"] == []

    def test_validate_acceptance_fail(self):
        agent = self._make_agent()
        prog = ExecutionProgram(
            objective="deploy hub",
            acceptance=["returned 200", "no error logs"],
        )
        result = agent._validate_acceptance(prog, "returned 200 but errors found")
        assert result["passed"] is False
        assert "returned 200" in result["matched"]
        assert "no error logs" in result["unmatched"]

    def test_validate_acceptance_empty_program(self):
        agent = self._make_agent()
        prog = ExecutionProgram(objective="chat")
        result = agent._validate_acceptance(prog, "whatever")
        assert result["passed"] is True  # no criteria = pass
        assert result["matched"] == []

    def test_validate_acceptance_none_program(self):
        agent = self._make_agent()
        result = agent._validate_acceptance(None, "output")
        assert result["passed"] is True
