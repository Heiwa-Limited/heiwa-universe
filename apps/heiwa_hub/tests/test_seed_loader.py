"""Tests for STDB seed loader."""
import json
import pytest
from pathlib import Path
from unittest.mock import patch, MagicMock, call
from heiwa_sdk.seed import SeedLoader


class TestSeedLoader:
    """Test seed loading from JSON to STDB."""

    def test_load_model_tiers_seed_file(self):
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        assert seed_path.exists(), f"Seed file not found: {seed_path}"
        with open(seed_path) as f:
            tiers = json.load(f)
        assert len(tiers) >= 10
        for tier in tiers:
            assert "model_id" in tier
            assert "provider" in tier
            assert "effort_level" in tier
            assert 1 <= tier["effort_level"] <= 5

    @patch("heiwa_sdk.seed.SpacetimeDB")
    def test_seed_model_tiers_calls_upsert(self, mock_stdb_cls):
        mock_stdb = MagicMock()
        mock_stdb_cls.return_value = mock_stdb
        mock_stdb.get_model_tiers.return_value = []  # empty = needs seeding

        loader = SeedLoader(stdb=mock_stdb)
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        loader.seed_model_tiers(seed_path)

        assert mock_stdb.upsert_model_tier.call_count >= 10

    @patch("heiwa_sdk.seed.SpacetimeDB")
    def test_seed_skips_if_already_populated(self, mock_stdb_cls):
        mock_stdb = MagicMock()
        mock_stdb_cls.return_value = mock_stdb
        mock_stdb.get_model_tiers.return_value = [{"model_id": "existing"}]

        loader = SeedLoader(stdb=mock_stdb)
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        loader.seed_model_tiers(seed_path)

        mock_stdb.upsert_model_tier.assert_not_called()
