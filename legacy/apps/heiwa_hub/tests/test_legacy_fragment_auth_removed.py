from __future__ import annotations

from urllib.parse import urlsplit

import pytest


@pytest.mark.asyncio
async def test_browser_auth_redirect_uses_sveltekit_callback_without_fragment(monkeypatch):
    monkeypatch.setenv("HEIWA_AUTH_SECRET", "test-auth-secret")
    monkeypatch.setenv("HEIWA_WEB_ORIGIN", "https://app.heiwa.ltd")

    from apps.heiwa_hub import auth as auth_module

    state = auth_module._generate_state()
    monkeypatch.setattr(auth_module, "ensure_user", lambda stdb, discord_data: "user-123")

    async def fake_exchange_discord_code(code: str):
        assert code == "discord-code"
        return {
            "discord_user_id": "discord-123",
            "username": "devon",
            "access_token": "discord-access-token",
        }

    monkeypatch.setattr(auth_module, "exchange_discord_code", fake_exchange_discord_code)

    response = await auth_module.auth_discord_callback("discord-code", state, stdb=object())

    location = response.headers["location"]
    parsed = urlsplit(location)

    assert parsed.scheme == "https"
    assert parsed.netloc == "app.heiwa.ltd"
    assert parsed.path == "/auth/callback"
    assert parsed.fragment == ""
    assert "#token=" not in location
