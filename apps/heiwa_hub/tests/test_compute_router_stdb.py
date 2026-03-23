"""Tests for ComputeRouter reading from STDB model_tiers."""
import pytest
from unittest.mock import MagicMock, patch
from heiwa_cognition.router import ComputeRouter


class TestComputeRouterSTDB:
    """Test that router reads model tiers from STDB when available."""

    def _mock_tiers(self):
        return [
            {
                "model_id": "ollama/qwen3.5:4b",
                "provider": "ollama",
                "rate_group": "local_ollama",
                "capability_class": 2,
                "effort_knob": "thinking:on",
                "effort_level": 4,
                "strengths_json": '["build","files","research","general"]',
                "enabled": True,
                "cost_per_turn": 0.0,
                "last_success_rate": 1.0,
            },
            {
                "model_id": "gemini-cli/gemini-3-flash",
                "provider": "google-gemini-cli",
                "rate_group": "google_gemini_cli",
                "capability_class": 2,
                "effort_knob": "thinking:on",
                "effort_level": 4,
                "strengths_json": '["research","audit","status_check"]',
                "enabled": True,
                "cost_per_turn": 0.0,
                "last_success_rate": 0.95,
            },
            {
                "model_id": "codex/gpt-4.1",
                "provider": "codex",
                "rate_group": "openai_codex",
                "capability_class": 2,
                "effort_knob": "reasoning:medium",
                "effort_level": 3,
                "strengths_json": '["build","refactor","self_buff"]',
                "enabled": True,
                "cost_per_turn": 0.6,
                "last_success_rate": 0.97,
            },
            {
                "model_id": "claude-code/opus-4.6",
                "provider": "claude-code",
                "rate_group": "anthropic_claude",
                "capability_class": 3,
                "effort_knob": "thinking:high",
                "effort_level": 5,
                "strengths_json": '["build","research","refactor","audit"]',
                "enabled": True,
                "cost_per_turn": 0.8,
                "last_success_rate": 0.99,
            },
        ]

    def test_router_uses_stdb_tiers_for_model_selection(self):
        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = self._mock_tiers()
        router = ComputeRouter(stdb=mock_stdb)
        route = router.route("audit", "low")
        assert route.target_model is not None

    def test_router_prefers_local_model_for_sovereign_execution(self):
        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = self._mock_tiers()
        router = ComputeRouter(stdb=mock_stdb)
        route = router.route("build", "low", privacy_level="sovereign")
        assert "ollama" in route.target_model

    def test_router_skips_local_only_models_for_railway_runtime(self):
        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = self._mock_tiers()
        router = ComputeRouter(stdb=mock_stdb)
        route = router.route("build", "medium")

        assert route.target_runtime == "railway"
        assert "ollama" not in route.target_model

    def test_router_falls_back_to_json_if_no_stdb(self):
        router = ComputeRouter(stdb=None)
        route = router.route("audit", "low")
        assert route.target_model is not None

    def test_route_includes_effort_knob(self):
        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = self._mock_tiers()
        router = ComputeRouter(stdb=mock_stdb)
        # Use "research"/"low" which routes to Class 3 where mock tiers match
        route = router.route("research", "low")
        assert hasattr(route, "effort_knob")
        assert route.effort_knob is not None
        assert route.effort_knob != ""
