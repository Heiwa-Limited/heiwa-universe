from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


def test_hub_main_imports_from_source_without_preseeded_pythonpath():
    env = os.environ.copy()
    env.pop("PYTHONPATH", None)

    result = subprocess.run(
        [
            sys.executable,
            "-c",
            "import runpy; runpy.run_module('apps.heiwa_hub.main', run_name='__probe__')",
        ],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0, (
        "apps.heiwa_hub.main should import cleanly from the repo root without "
        f"an externally seeded PYTHONPATH.\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
    )
