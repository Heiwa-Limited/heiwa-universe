from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "packages/heiwa_sdk"))
sys.path.insert(0, str(ROOT / "packages/heiwa_protocol"))
sys.path.insert(0, str(ROOT / "packages/heiwa_identity"))
sys.path.insert(0, str(ROOT / "apps"))


def main() -> int:
    failures: list[str] = []

    # 1. Strict STDB Enforcement
    os.environ["HEIWA_STATE_BACKEND"] = "spacetimedb"
    os.environ["STDB_IDENTITY"] = ""
    
    from heiwa_sdk.db import Database
    
    try:
        db = Database()
        failures.append("Database should fail closed when STDB backend is selected without STDB_IDENTITY")
    except ValueError:
        # This is the expected failure
        pass
    except Exception as e:
        failures.append(f"Unexpected error in STDB mode: {e}")

    # 2. Stateless mode check
    os.environ["HEIWA_STATE_BACKEND"] = "stateless"
    try:
        db = Database()
        if hasattr(db, "get_connection"):
            failures.append("Database should NOT have get_connection even in stateless mode")
    except Exception as e:
        failures.append(f"Stateless mode error: {e}")

    if failures:
        print("State service test FAILED")
        for failure in failures:
            print(f" - {failure}")
        return 1

    print("State service test PASSED")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
