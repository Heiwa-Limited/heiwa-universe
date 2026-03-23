import asyncio
import logging
import sys
import os
from pathlib import Path

# Ensure monorepo roots are on sys.path
ROOT = Path(__file__).resolve().parents[2]
_PYTHONPATH_ENTRIES = [
    ROOT / "packages" / "heiwa_cli",
    ROOT / "packages" / "heiwa_cognition",
    ROOT / "packages" / "heiwa_sdk",
    ROOT / "packages" / "heiwa_protocol",
    ROOT / "packages" / "heiwa_identity",
    ROOT / "packages" / "heiwa_ui",
    ROOT / "apps",
]

for entry in reversed(_PYTHONPATH_ENTRIES):
    entry_str = str(entry)
    if entry_str not in sys.path:
        sys.path.insert(0, entry_str)

from heiwa_sdk.config import load_swarm_env
load_swarm_env()

from heiwa_hub.agents.spine import SpineAgent
from heiwa_hub.agents.heiwaclaw import HeiwaClawAgent
from heiwa_hub.agents.messenger import MessengerAgent
from heiwa_hub.agents.telemetry import TelemetryAgent
from heiwa_hub.mcp_server import app as hub_app

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)


async def _start_server(port: int):
    import uvicorn
    logger = logging.getLogger("Hub.Server")
    logger.info("Heiwa Hub booting on port %s...", port)
    config = uvicorn.Config(hub_app, host="0.0.0.0", port=port, log_level="info")
    server = uvicorn.Server(config)
    await server.serve()


async def main():
    splash = """
    █░█ █▀▀ █ █░█░█ ▄▀█
    █▀█ ██▄ █ ▀▄▀▄▀ █▀█
    [ HEIWA HUB v3.0 — HeiwaClaw + OpenClaw ]
    """
    logger = logging.getLogger("Hub")
    print(splash)

    # Initialize persistence
    from heiwa_sdk.db import Database
    db = Database()
    
    # Ensure runtime spool exists for durable dead-lettering
    os.makedirs("runtime/spool", exist_ok=True)
    
    if db.state_backend != "spacetimedb":
        asyncio.create_task(asyncio.to_thread(db.init_db))
    else:
        logger.info("[BOOT] SpacetimeDB selected as authoritative state layer.")
        # Auto-publish schema only for local dev (requires Rust toolchain).
        # Remote servers (maincloud) have the module published via CI.
        stdb_server = os.getenv("STDB_SERVER", "local")
        if db.stdb and stdb_server == "local":
            stdb_path = str(ROOT / "apps/heiwa_hub/spacetimedb")
            if os.path.isdir(stdb_path):
                logger.info("[BOOT] Syncing SpacetimeDB schema...")
                db.stdb.publish_schema(stdb_path)
        else:
            logger.info("[BOOT] STDB server is '%s' — skipping schema publish.", stdb_server)

        # Seed STDB tables from checked-in config (first boot only)
        from heiwa_sdk.seed import SeedLoader
        seed_loader = SeedLoader(stdb=db.stdb)
        seed_loader.seed_model_tiers(ROOT / "config" / "seeds" / "model_tiers.json")
        seed_loader.seed_rate_groups(ROOT / "config" / "swarm" / "ai_router.json")

        # Optional repository re-indexing on boot
        if os.getenv("HEIWA_INDEX_ON_BOOT", "").lower() == "true":
            from apps.heiwa_hub.scripts.index_repo import index_repo
            asyncio.create_task(index_repo())

    # Register MCP servers from ai_router.json
    async def _register_mcp_servers():
        import json
        router_path = ROOT / "config/swarm/ai_router.json"
        if router_path.exists():
            config = json.loads(router_path.read_text())
            for name in config.get("mcp_servers", {}):
                logger.info("[MCP] Registered '%s' via ai_router.json", name)

    asyncio.create_task(_register_mcp_servers())

    # Boot agents — all use local bus transport
    # HeiwaClaw is the unified living agent (observation + execution via OpenClaw)
    try:
        spine = SpineAgent()
        heiwaclaw = HeiwaClawAgent()
        telemetry = TelemetryAgent()
    except Exception as e:
        logger.error("[BOOT_FATAL] Failed to instantiate core agents: %s", e)
        sys.exit(1)

    port = int(os.getenv("PORT", "8080"))
    tasks = [
        asyncio.create_task(spine.run()),
        asyncio.create_task(heiwaclaw.run()),
        asyncio.create_task(telemetry.run()),
        asyncio.create_task(_start_server(port=port)),
    ]

    # Broker enrichment is now a direct service call from Spine — no separate agent.

    # Messenger is optional: run only when Discord token exists.
    messenger_mode = os.getenv("HEIWA_ENABLE_MESSENGER", "auto").strip().lower()
    has_discord_token = bool(os.getenv("DISCORD_TOKEN") or os.getenv("DISCORD_BOT_TOKEN"))
    if messenger_mode == "true" or (messenger_mode == "auto" and has_discord_token):
        messenger = MessengerAgent()
        tasks.append(asyncio.create_task(messenger.run()))
    else:
        logger.info("[BOOT] Messenger disabled (no Discord token).")

    logger.info("[BOOT] All services dispatched (%d tasks).", len(tasks))
    await asyncio.gather(*tasks)


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\nShutting down...")
