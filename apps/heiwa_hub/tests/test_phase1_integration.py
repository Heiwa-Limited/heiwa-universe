"""Phase 1 integration test: seed → route → dispatch with effort knobs."""
import json
import pytest
from pathlib import Path
from unittest.mock import MagicMock, patch


class TestPhase1Integration:
    """Verify the full Phase 1 pipeline: seed → STDB → router → dispatch."""

    def _seed_mock_stdb(self):
        """Create a mock STDB with seeded model tiers."""
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        with open(seed_path) as f:
            tiers = json.load(f)

        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = [
            {**t, "strengths_json": json.dumps(t["strengths"]),
             "last_success_rate": 1.0, "avg_latency_ms": 0, "latency_p95_ms": 0}
            for t in tiers
        ]
        return mock_stdb

    def test_audit_routes_stay_on_deterministic_ops_path(self):
        from heiwa_cognition.router import ComputeRouter
        stdb = self._seed_mock_stdb()
        router = ComputeRouter(stdb=stdb)
        route = router.route("audit", "low")

        assert route.target_model is not None
        assert route.target_tool == "heiwa_ops"
        assert route.compute_class == 1
        assert route.effort_knob == ""

    def test_research_routes_to_capable_model_with_thinking(self):
        from heiwa_cognition.router import ComputeRouter
        stdb = self._seed_mock_stdb()
        router = ComputeRouter(stdb=stdb)
        route = router.route("research", "medium")
        assert route.target_model is not None
        assert route.effort_knob != ""
        # Research should get thinking enabled based on our seed data for gemini/ollama
        assert "thinking" in route.effort_knob or "effort" in route.effort_knob or "reasoning" in route.effort_knob

    def test_build_routes_with_code_gen_strength(self):
        from heiwa_cognition.router import ComputeRouter
        stdb = self._seed_mock_stdb()
        router = ComputeRouter(stdb=stdb)
        route = router.route("build", "medium")
        assert route.target_model is not None

    def test_seed_file_has_all_required_fields(self):
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        with open(seed_path) as f:
            tiers = json.load(f)

        required = ["model_id", "provider_model_id", "provider", "rate_group",
                     "capability_class", "effort_knob", "effort_level",
                     "cost_per_turn", "max_context_tokens", "vram_requirement_mb",
                     "quantization_type", "kv_cache_strategy", "strengths", "enabled"]
        for tier in tiers:
            for field in required:
                assert field in tier, f"Missing {field} in tier {tier.get('model_id', '?')}"
            assert 1 <= tier["effort_level"] <= 5
            assert tier["capability_class"] in (1, 2, 3)

    def test_all_providers_represented_in_seed(self):
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        with open(seed_path) as f:
            tiers = json.load(f)
        providers = {t["provider"] for t in tiers}
        expected = {"ollama", "google-gemini-cli", "google-antigravity", "codex", "claude-code"}
        assert expected.issubset(providers), f"Missing providers: {expected - providers}"
