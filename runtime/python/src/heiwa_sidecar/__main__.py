"""Entry point: `python -m heiwa_sidecar` or the installed `heiwa-sidecar` script."""

from __future__ import annotations

import sys

from .server import run


def main() -> int:
    return run()


if __name__ == "__main__":
    sys.exit(main())
