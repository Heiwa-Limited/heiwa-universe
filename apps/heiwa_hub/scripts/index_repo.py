"""Script to index the Heiwa monorepo into STDB knowledge embeddings."""
import asyncio
import logging
import os
from pathlib import Path
from heiwa_sdk.db import Database
from heiwa_sdk.memory import MemoryService

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("Indexer")

# Configurable paths and extensions
INDEX_PATHS = ["apps", "packages", "docs", "config", "rooms"]
EXTENSIONS = {".py", ".md", ".json", ".rs", ".sh", ".toml", ".yaml", ".yml"}
EXCLUDE_DIRS = {"__pycache__", ".git", ".venv", ".pytest_cache", "target", "node_modules"}


async def index_repo():
    root = Path(__file__).resolve().parents[3]
    logger.info("Starting repository index at %s", root)

    db = Database()
    if not db.stdb:
        logger.error("STDB not configured. Set STDB_IDENTITY.")
        return

    memory = MemoryService(stdb=db.stdb)
    indexed_count = 0

    for path_str in INDEX_PATHS:
        base_path = root / path_str
        if not base_path.exists():
            continue

        for file_path in base_path.rglob("*"):
            if file_path.is_dir() and file_path.name in EXCLUDE_DIRS:
                # This doesn't actually stop rglob from entering, 
                # but we'll filter files below
                continue
            
            if not file_path.is_file():
                continue
            
            if file_path.suffix not in EXTENSIONS:
                continue

            # Skip if file is in excluded directory
            if any(part in EXCLUDE_DIRS for part in file_path.parts):
                continue

            rel_path = file_path.relative_to(root)
            logger.info("Indexing %s...", rel_path)
            
            try:
                content = file_path.read_text(errors="ignore")
                success = await memory.index_file(str(rel_path), content)
                if success:
                    indexed_count += 1
            except Exception as e:
                logger.error("Failed to index %s: %s", rel_path, e)

    logger.info("Indexing complete. Processed %d files.", indexed_count)


if __name__ == "__main__":
    asyncio.run(index_repo())
