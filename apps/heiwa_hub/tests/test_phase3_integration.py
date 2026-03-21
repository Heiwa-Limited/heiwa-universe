"""Phase 3 integration test: audit → directive → self-tuning router."""
import json
import pytest
from unittest.mock import MagicMock, patch, AsyncMock
from heiwa_hub.agents.heiwaclaw import HeiwaClawAgent as HeiwaAgent
from heiwa_cognition.router import ComputeRouter


class TestPhase3Integration:
    """Verify Phase 3: Captain autonomy and self-tuning routing."""

    @pytest.mark.asyncio
    async def test_captain_triggers_audit_and_directive(self):
        with patch("heiwa_hub.agents.heiwaclaw.RepoAuditor") as mock_auditor_cls:
            mock_auditor = MagicMock()
            # Simulate audit failure
            mock_auditor.run_audit = AsyncMock(return_value=MagicMock(
                passed=False,
                summary="Lint failed",
                errors=["error in config.py"]
            ))
            mock_auditor_cls.return_value = mock_auditor
            
            agent = HeiwaAgent()
            agent.db.stdb = MagicMock()
            
            # Run audit cycle
            await agent._run_periodic_audit()
            
            # Verify directive created in STDB
            agent.db.stdb.insert_captain_directive.assert_called_once()
            call_args = agent.db.stdb.insert_captain_directive.call_args[1]
            assert call_args["directive_type"] == "repair_repo"
            assert "Lint failed" in call_args["payload"]["summary"]

    @pytest.mark.asyncio
    async def test_captain_tunes_model_tiers(self):
        agent = HeiwaAgent()
        agent.db.stdb = MagicMock()
        agent.memory = MagicMock()
        
        # Mock enough stats to trigger update
        agent.memory.get_execution_stats.return_value = {
            "success_rate": 0.9,
            "avg_latency_ms": 500,
            "p95_latency_ms": 1200,
            "count": 5
        }
        agent.db.stdb.get_model_tiers.return_value = [{"model_id": "test-model"}]
        
        await agent._tune_model_tiers()
        
        # Verify stats updated in STDB
        agent.db.stdb.update_model_tier_stats.assert_called_once_with(
            model_id="test-model",
            success_rate=0.9,
            avg_latency_ms=500,
            latency_p95_ms=1200
        )

    def test_router_prioritizes_success_rate(self):
        mock_stdb = MagicMock()
        # Two models, same cost (0.0), but one has better success rate
        mock_stdb.get_model_tiers.return_value = [
            {
                "model_id": "model-a-reliable",
                "capability_class": 2,
                "cost_per_turn": 0.0,
                "last_success_rate": 0.95,
                "effort_level": 1,
                "strengths_json": '["general"]',
                "enabled": True,
                "effort_knob": "none"
            },
            {
                "model_id": "model-b-flaky",
                "capability_class": 2,
                "cost_per_turn": 0.0,
                "last_success_rate": 0.4, # Should be penalized
                "effort_level": 1,
                "strengths_json": '["general"]',
                "enabled": True,
                "effort_knob": "none"
            }
        ]
        
        router = ComputeRouter(stdb=mock_stdb)
        route = router.route("chat", "low")
        
        assert route.target_model == "model-a-reliable"
