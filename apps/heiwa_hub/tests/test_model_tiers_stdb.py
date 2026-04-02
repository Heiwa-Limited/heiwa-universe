"""Tests for model_tiers STDB operations."""
import pytest
from unittest.mock import patch, MagicMock
from heiwa_sdk.spacetimedb import SpacetimeDB


class TestModelTiersSTDB:
    """Test model tier STDB bridge operations."""

    def setup_method(self):
        self.stdb = SpacetimeDB.__new__(SpacetimeDB)
        self.stdb.db_identity = "test-identity"
        self.stdb.server = "local"

    @patch.object(SpacetimeDB, "query")
    def test_get_model_tiers_returns_list(self, mock_query):
        mock_query.return_value = [
            {
                "model_id": "ollama/qwen3.5:4b",
                "provider_model_id": "qwen3.5:4b",
                "provider": "ollama",
                "rate_group": "local_ollama",
                "capability_class": 2,
                "effort_knob": "thinking:on",
                "effort_level": 4,
                "cost_per_turn": 0.0,
                "max_context_tokens": 32768,
                "vram_requirement_mb": 4096,
                "quantization_type": "q4_k_m",
                "kv_cache_strategy": "turboquant",
                "strengths_json": '["code_generation","research"]',
                "enabled": True,
                "last_success_rate": 1.0,
                "avg_latency_ms": 500,
                "latency_p95_ms": 1200,
            }
        ]
        result = self.stdb.get_model_tiers()
        assert len(result) == 1
        assert result[0]["model_id"] == "ollama/qwen3.5:4b"
        assert result[0]["effort_level"] == 4
        assert result[0]["vram_requirement_mb"] == 4096
        assert result[0]["quantization_type"] == "q4_k_m"
        assert result[0]["kv_cache_strategy"] == "turboquant"

    @patch.object(SpacetimeDB, "query")
    def test_get_model_tiers_by_capability_class(self, mock_query):
        mock_query.return_value = []
        result = self.stdb.get_model_tiers(capability_class=3)
        mock_query.assert_called_once()
        call_sql = mock_query.call_args[0][0]
        assert "capability_class = 3" in call_sql

    @patch.object(SpacetimeDB, "query")
    def test_get_model_tier_by_id(self, mock_query):
        mock_query.return_value = [{"model_id": "codex/gpt-4.1"}]
        result = self.stdb.get_model_tier("codex/gpt-4.1")
        assert result is not None
        assert result["model_id"] == "codex/gpt-4.1"

    @patch.object(SpacetimeDB, "query")
    def test_get_model_tier_not_found(self, mock_query):
        mock_query.return_value = []
        result = self.stdb.get_model_tier("nonexistent/model")
        assert result is None

    @patch.object(SpacetimeDB, "call")
    def test_upsert_model_tier(self, mock_call):
        self.stdb.upsert_model_tier(
            model_id="ollama/qwen3.5:4b",
            provider_model_id="qwen3.5:4b",
            provider="ollama",
            rate_group="local_ollama",
            capability_class=2,
            effort_knob="thinking:on",
            effort_level=4,
            cost_per_turn=0.0,
            max_context_tokens=32768,
            vram_requirement_mb=4096,
            quantization_type="q4_k_m",
            kv_cache_strategy="turboquant",
            strengths=["code_generation", "research"],
            enabled=True,
        )
        mock_call.assert_called_once()
        call_args = mock_call.call_args[0]
        assert call_args[0] == "upsert_model_tier"
        assert call_args[10] == 4096
        assert call_args[11] == "q4_k_m"
        assert call_args[12] == "turboquant"

    @patch.object(SpacetimeDB, "call")
    def test_update_model_tier_stats(self, mock_call):
        self.stdb.update_model_tier_stats(
            model_id="ollama/qwen3.5:4b",
            success_rate=0.85,
            avg_latency_ms=600,
            latency_p95_ms=1500,
        )
        mock_call.assert_called_once()
        assert mock_call.call_args[0][0] == "update_model_tier_stats"

    @patch.object(SpacetimeDB, "query")
    def test_get_model_usage_summary(self, mock_query):
        mock_query.return_value = [
            {"model_id": "claude", "tokens_total": 100, "cost": 0.01},
            {"model_id": "claude", "tokens_total": 50, "cost": 0.005},
            {"model_id": "gemini", "tokens_total": 200, "cost": 0.0},
        ]
        result = self.stdb.get_model_usage_summary(minutes=60)
        # Sort results for consistent assertion
        result = sorted(result, key=lambda x: x["model_id"])
        
        assert len(result) == 2
        assert result[0]["model_id"] == "claude"
        assert result[0]["request_count"] == 2
        assert result[0]["total_tokens"] == 150
        assert result[0]["total_cost"] == 0.015
        
        assert result[1]["model_id"] == "gemini"
        assert result[1]["request_count"] == 1
        assert result[1]["total_tokens"] == 200
