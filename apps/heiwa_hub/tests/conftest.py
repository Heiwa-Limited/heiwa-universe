"""
Test-suite conftest.py for apps/heiwa_hub/tests/.

Provides the common ROOT path that all test files compute via
Path(__file__).resolve().parents[3]. Individual tests can import this
instead of recomputing it, though existing standalone tests still work
because they compute ROOT themselves.

The PYTHONPATH setup is handled by the repo-root conftest.py, so tests
running under pytest do not need the sys.path.insert() boilerplate.
"""
from __future__ import annotations

from pathlib import Path

# Repo root — equivalent to Path(__file__).resolve().parents[3] that every
# test file computes individually.
ROOT = Path(__file__).resolve().parents[3]
