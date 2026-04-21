"""
Heiwa Hub — Discord OAuth2 + JWT session auth.

Flow:
  1. GET /auth/discord → redirect to Discord OAuth consent
  2. Discord redirects to GET /auth/discord/callback?code=...&state=...
  3. Hub exchanges code for Discord access token
  4. Hub fetches Discord user profile
  5. Hub creates/links User + OAuthIdentity in STDB
  6. JWT utilities remain for non-browser compatibility paths
  7. Legacy browser path redirects to app.heiwa.ltd/auth/callback for SvelteKit cookie session
"""

import hashlib
import hmac
import json
import logging
import os
import time
import uuid
from typing import Any, Optional
from urllib.parse import urlencode

import httpx
from fastapi import HTTPException, Request
from fastapi.responses import RedirectResponse

logger = logging.getLogger("Hub.Auth")

# ---------------------------------------------------------------------------
#  Config
# ---------------------------------------------------------------------------

DISCORD_API = "https://discord.com/api/v10"
DISCORD_OAUTH_AUTHORIZE = "https://discord.com/oauth2/authorize"
DISCORD_OAUTH_TOKEN = f"{DISCORD_API}/oauth2/token"
DISCORD_USER_ME = f"{DISCORD_API}/users/@me"

# Scopes needed: identify (user id, username, avatar), guilds (optional)
DISCORD_SCOPES = "identify"

# JWT lifetime
JWT_LIFETIME_SEC = 3600  # 1 hour


def _env(key: str, required: bool = True) -> str:
    val = os.getenv(key, "")
    if required and not val:
        raise RuntimeError(f"Missing required env var: {key}")
    return val


def _discord_client_id() -> str:
    return _env("DISCORD_CLIENT_ID", required=False) or _env("DISCORD_APPLICATION_ID", required=False)


def _discord_client_secret() -> str:
    return _env("DISCORD_CLIENT_SECRET")


def _auth_secret() -> bytes:
    """Secret used to sign JWTs and CSRF state tokens."""
    raw = _env("HEIWA_AUTH_SECRET")
    return raw.encode("utf-8")


def _redirect_uri() -> str:
    """OAuth callback URL — must match Discord app config."""
    explicit = os.getenv("HEIWA_OAUTH_REDIRECT_URI")
    if explicit:
        return explicit
    hub_url = os.getenv("HEIWA_HUB_URL", "https://api.heiwa.ltd").rstrip("/")
    return f"{hub_url}/auth/discord/callback"


def _web_origin() -> str:
    """Where to redirect after auth completes."""
    return os.getenv("HEIWA_WEB_ORIGIN", "https://app.heiwa.ltd")


# ---------------------------------------------------------------------------
#  CSRF state token
# ---------------------------------------------------------------------------

# In-memory nonce store — short-lived, cleared on use.
# For v1 single-instance this is fine. Multi-instance would use STDB.
_pending_states: dict[str, float] = {}
_STATE_TTL = 600  # 10 minutes


def _prune_expired_states(now: float | None = None) -> None:
    cutoff = (time.time() if now is None else now) - _STATE_TTL
    expired = [k for k, v in _pending_states.items() if v < cutoff]
    for k in expired:
        _pending_states.pop(k, None)


def _generate_state() -> str:
    """Create a signed, single-use CSRF state parameter."""
    nonce = uuid.uuid4().hex
    sig = hmac.new(_auth_secret(), nonce.encode(), hashlib.sha256).hexdigest()[:16]
    token = f"{nonce}.{sig}"
    _pending_states[token] = time.time()
    _prune_expired_states()
    return token


def _validate_state(state: str) -> bool:
    """Validate and consume a CSRF state token."""
    _prune_expired_states()
    ts = _pending_states.pop(state, None)
    if ts is None:
        return False
    if time.time() - ts > _STATE_TTL:
        return False
    # Verify signature
    parts = state.split(".", 1)
    if len(parts) != 2:
        return False
    nonce, sig = parts
    expected = hmac.new(_auth_secret(), nonce.encode(), hashlib.sha256).hexdigest()[:16]
    return hmac.compare_digest(sig, expected)


# ---------------------------------------------------------------------------
#  JWT (minimal, HMAC-SHA256, no external lib needed)
# ---------------------------------------------------------------------------

def _b64url_encode(data: bytes) -> str:
    import base64
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def _b64url_decode(s: str) -> bytes:
    import base64
    padding = 4 - len(s) % 4
    if padding != 4:
        s += "=" * padding
    return base64.urlsafe_b64decode(s)


def sign_jwt(claims: dict[str, Any]) -> str:
    """Sign a JWT with HMAC-SHA256 using HEIWA_AUTH_SECRET."""
    header = {"alg": "HS256", "typ": "JWT"}
    now = int(time.time())
    claims.setdefault("iat", now)
    claims.setdefault("exp", now + JWT_LIFETIME_SEC)
    claims.setdefault("iss", "heiwa-hub")
    claims.setdefault("aud", "heiwa")

    segments = [
        _b64url_encode(json.dumps(header, separators=(",", ":")).encode()),
        _b64url_encode(json.dumps(claims, separators=(",", ":")).encode()),
    ]
    signing_input = f"{segments[0]}.{segments[1]}"
    sig = hmac.new(_auth_secret(), signing_input.encode(), hashlib.sha256).digest()
    segments.append(_b64url_encode(sig))
    return ".".join(segments)


def verify_jwt(token: str) -> Optional[dict[str, Any]]:
    """Verify a JWT. Returns claims dict or None if invalid/expired."""
    parts = token.split(".")
    if len(parts) != 3:
        return None

    signing_input = f"{parts[0]}.{parts[1]}"
    expected_sig = hmac.new(_auth_secret(), signing_input.encode(), hashlib.sha256).digest()
    actual_sig = _b64url_decode(parts[2])
    if not hmac.compare_digest(expected_sig, actual_sig):
        return None

    try:
        claims = json.loads(_b64url_decode(parts[1]))
    except Exception:
        return None

    # Check expiration
    if claims.get("exp", 0) < time.time():
        return None
    # Check issuer
    if claims.get("iss") != "heiwa-hub":
        return None

    return claims


# ---------------------------------------------------------------------------
#  Discord OAuth exchange
# ---------------------------------------------------------------------------

async def exchange_discord_code(code: str) -> dict[str, Any]:
    """Exchange an OAuth2 authorization code for Discord tokens + user profile."""
    async with httpx.AsyncClient(timeout=10) as client:
        # Exchange code for tokens
        token_resp = await client.post(
            DISCORD_OAUTH_TOKEN,
            data={
                "grant_type": "authorization_code",
                "code": code,
                "redirect_uri": _redirect_uri(),
                "client_id": _discord_client_id(),
                "client_secret": _discord_client_secret(),
            },
            headers={"Content-Type": "application/x-www-form-urlencoded"},
        )
        if token_resp.status_code != 200:
            logger.error("Discord token exchange failed: %s %s", token_resp.status_code, token_resp.text)
            raise HTTPException(status_code=502, detail="Discord token exchange failed")

        token_data = token_resp.json()
        access_token = token_data["access_token"]

        # Fetch user profile
        user_resp = await client.get(
            DISCORD_USER_ME,
            headers={"Authorization": f"Bearer {access_token}"},
        )
        if user_resp.status_code != 200:
            logger.error("Discord user fetch failed: %s", user_resp.text)
            raise HTTPException(status_code=502, detail="Discord user fetch failed")

        user_data = user_resp.json()

    return {
        "access_token": access_token,
        "refresh_token": token_data.get("refresh_token"),
        "expires_in": token_data.get("expires_in"),
        "discord_user_id": user_data["id"],
        "username": user_data.get("global_name") or user_data.get("username", ""),
        "avatar": user_data.get("avatar"),
        "email": user_data.get("email"),
    }


# ---------------------------------------------------------------------------
#  STDB user operations
# ---------------------------------------------------------------------------

def ensure_user(stdb, discord_data: dict[str, Any]) -> str:
    """Create or link a Heiwa user from Discord OAuth data. Returns user_id."""
    discord_uid = str(discord_data["discord_user_id"])
    username = discord_data.get("username", "")
    avatar = discord_data.get("avatar")
    avatar_url = (
        f"https://cdn.discordapp.com/avatars/{discord_uid}/{avatar}.png"
        if avatar else None
    )

    # Check if this Discord identity already has a linked user
    existing = stdb.query(
        f"SELECT user_id FROM oauth_identities "
        f"WHERE provider = 'discord' AND provider_user_id = '{stdb._escape_sql_literal(discord_uid)}'"
    )

    if existing and existing[0].get("user_id"):
        user_id = existing[0]["user_id"]
        stdb.call("update_user_seen", user_id)
        logger.info("Returning user (Discord %s) → %s", discord_uid, user_id)
        return user_id

    # New user
    user_id = f"user-{uuid.uuid4().hex[:12]}"
    stdb.call(
        "create_user",
        user_id,
        username,
        discord_data.get("email"),       # email (Option)
        avatar_url,                       # avatar_url (Option)
        "free",                           # tier
    )

    # Link OAuth identity
    identity_id = f"oid-{uuid.uuid4().hex[:12]}"
    stdb.call(
        "link_oauth_identity",
        identity_id,
        user_id,
        "discord",
        discord_uid,
        username,
        discord_data.get("access_token", ""),   # access_token_enc (encrypt later)
        discord_data.get("refresh_token"),       # refresh_token_enc (Option)
        None,                                     # token_expires_at (Option)
        json.dumps(["identify"]),                # scopes_json
    )

    logger.info("Created user %s from Discord %s (%s)", user_id, discord_uid, username)
    return user_id


# ---------------------------------------------------------------------------
#  Request helpers
# ---------------------------------------------------------------------------

def get_current_user(request: Request) -> Optional[dict[str, Any]]:
    """Extract and verify the JWT from Authorization header. Returns claims or None."""
    auth = request.headers.get("authorization", "")
    if not auth:
        return None
    token = auth.removeprefix("Bearer ").strip()
    if not token:
        return None
    return verify_jwt(token)


def require_user(request: Request) -> dict[str, Any]:
    """Like get_current_user but raises 401 if not authenticated."""
    user = get_current_user(request)
    if not user:
        raise HTTPException(status_code=401, detail="Authentication required")
    return user


# ---------------------------------------------------------------------------
#  Identity context
# ---------------------------------------------------------------------------

def resolve_identity_context(claims: dict[str, Any], *, autonomous: bool = False) -> dict[str, str]:
    """Resolve owner_id and principal_id from auth claims.

    For authenticated users, owner_id and principal_id both come from claims.
    For autonomous Captain actions, principal_id is 'captain'.
    """
    owner_id = str(claims.get("owner_id") or claims.get("user_id") or claims.get("sub", "unknown"))
    if autonomous:
        principal_id = "captain"
    else:
        principal_id = str(claims.get("principal_id") or claims.get("user_id") or claims.get("sub", "unknown"))
    return {"owner_id": owner_id, "principal_id": principal_id}


# ---------------------------------------------------------------------------
#  Route handlers (mounted by mcp_server.py)
# ---------------------------------------------------------------------------

def auth_discord_redirect():
    """GET /auth/discord — start OAuth flow."""
    client_id = _discord_client_id()
    if not client_id:
        raise HTTPException(status_code=500, detail="Discord OAuth not configured")

    state = _generate_state()
    params = {
        "client_id": client_id,
        "redirect_uri": _redirect_uri(),
        "response_type": "code",
        "scope": DISCORD_SCOPES,
        "state": state,
        "prompt": "consent",
    }
    url = f"{DISCORD_OAUTH_AUTHORIZE}?{urlencode(params)}"
    return RedirectResponse(url=url, status_code=302)


async def auth_discord_callback(code: str, state: str, stdb) -> RedirectResponse:
    """GET /auth/discord/callback — exchange code, create user, redirect through SvelteKit."""
    if not _validate_state(state):
        raise HTTPException(status_code=400, detail="Invalid or expired state parameter")

    discord_data = await exchange_discord_code(code)
    ensure_user(stdb, discord_data)

    # Deprecated browser fragment handoff:
    # keep this Python surface only as the legacy OAuth bridge.
    # Browser UX now lands on SvelteKit's callback route and cookie session model.
    web_origin = _web_origin()
    return RedirectResponse(url=f"{web_origin}/auth/callback", status_code=302)
