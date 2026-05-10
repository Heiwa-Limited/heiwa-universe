from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


def test_smoke_script_imports_from_repo_root():
    env = os.environ.copy()
    env.pop("PYTHONPATH", None)

    result = subprocess.run(
        [
            sys.executable,
            "-c",
            "import runpy; runpy.run_path('apps/heiwa_hub/actions/smoke_test.py', run_name='__probe__')",
        ],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0, (
        "Smoke script should import cleanly from the repo root without an "
        f"externally seeded PYTHONPATH.\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
    )
