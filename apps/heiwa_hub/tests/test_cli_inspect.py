"""Tests for heiwa inspect CLI command."""
import pytest
from unittest.mock import AsyncMock, MagicMock, patch


class TestInspectCommand:
    """Test the /inspect CLI command."""

    @pytest.mark.asyncio
    @patch("heiwa_cli.commands.SpacetimeDB")
    async def test_inspect_model_tiers(self, mock_stdb_cls):
        from heiwa_cli.commands import cmd_inspect
        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = [
            {"model_id": "ollama/qwen3.5:4b", "effort_level": 4, "enabled": True}
        ]
        ctx = MagicMock()
        ctx.stdb = mock_stdb
        await cmd_inspect(ctx, "model_tiers")
        mock_stdb.get_model_tiers.assert_called_once()

    @pytest.mark.asyncio
    async def test_inspect_unknown_table(self):
        from heiwa_cli.commands import cmd_inspect
        ctx = MagicMock()
        ctx.stdb = MagicMock()
        # Should not raise, just print error
        await cmd_inspect(ctx, "nonexistent_table")
