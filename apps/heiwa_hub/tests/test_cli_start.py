"""Tests for heiwa start CLI command."""
import pytest
from unittest.mock import AsyncMock, MagicMock, patch


class TestStartCommand:
    """Test the /start CLI command."""

    @pytest.mark.asyncio
    @patch("heiwa_cli.commands.subprocess")
    async def test_start_sets_stdb_backend(self, mock_subprocess):
        from heiwa_cli.commands import cmd_start
        import os
        ctx = MagicMock()
        mock_subprocess.Popen.return_value = MagicMock(pid=12345)
        # Don't use patch.dict on os.environ if we want to see actual updates
        # instead just check if it was updated after call
        orig_backend = os.environ.get("HEIWA_STATE_BACKEND")
        try:
            await cmd_start(ctx)
            assert os.environ.get("HEIWA_STATE_BACKEND") == "spacetimedb"
        finally:
            if orig_backend:
                os.environ["HEIWA_STATE_BACKEND"] = orig_backend
            elif "HEIWA_STATE_BACKEND" in os.environ:
                del os.environ["HEIWA_STATE_BACKEND"]

    @pytest.mark.asyncio
    @patch("heiwa_cli.commands.subprocess")
    async def test_start_launches_hub_process(self, mock_subprocess):
        from heiwa_cli.commands import cmd_start
        ctx = MagicMock()
        mock_subprocess.Popen.return_value = MagicMock(pid=12345)
        await cmd_start(ctx)
        assert mock_subprocess.Popen.called
