"""Heiwa CLI logging configuration — file-based logging to ~/.heiwa/logs/."""

from __future__ import annotations

import logging
from pathlib import Path

_configured = False


def configure_logging(level: str = "DEBUG") -> Path:
    """Set up file-based logging to ~/.heiwa/logs/heiwa.log.

    Safe to call multiple times — only configures once.
    Returns the log file path.
    """
    global _configured
    log_dir = Path.home() / ".heiwa" / "logs"
    log_dir.mkdir(parents=True, exist_ok=True)
    log_file = log_dir / "heiwa.log"

    if _configured:
        return log_file

    handler = logging.handlers.RotatingFileHandler(
        log_file,
        maxBytes=5 * 1024 * 1024,  # 5 MB
        backupCount=3,
        encoding="utf-8",
    )
    handler.setFormatter(logging.Formatter(
        "%(asctime)s %(name)-20s %(levelname)-7s %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    ))

    # Configure root logger for all Heiwa modules
    for name in ("CLI", "CLI.Stream", "SDK", "SDK.HeiwaClaw", "SDK.ToolMesh",
                 "SDK.Database", "SDK.OllamaStream"):
        logger = logging.getLogger(name)
        logger.setLevel(getattr(logging, level.upper(), logging.DEBUG))
        logger.addHandler(handler)

    _configured = True
    return log_file


# Need the import here since it's used above
import logging.handlers
