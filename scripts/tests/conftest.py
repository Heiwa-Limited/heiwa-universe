from __future__ import annotations

from pathlib import Path


TARGET = Path(__file__).with_name("test_class3_runtime_safety.py").resolve()


def pytest_ignore_collect(collection_path: Path, config) -> bool:
    if collection_path.resolve() != TARGET:
        return False

    explicit_targets = {
        Path(arg).resolve()
        for arg in config.invocation_params.args
        if not str(arg).startswith("-")
    }
    return TARGET not in explicit_targets
