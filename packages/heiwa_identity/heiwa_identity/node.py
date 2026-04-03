import json
import os
from pathlib import Path
from typing import Dict, Any


def _is_monorepo_root(path: Path) -> bool:
    return (path / "apps").exists() and (path / "packages").exists()


def _candidate_monorepo_roots() -> list[Path]:
    candidates: list[Path] = []
    seen: set[Path] = set()
    for key in ("HEIWA_WORKSPACE_ROOT", "HEIWA_ROOT", "HEIWA_ROOT_DIR"):
        raw = os.getenv(key)
        if not raw:
            continue
        path = Path(raw).expanduser().resolve()
        if path in seen:
            continue
        candidates.append(path)
        seen.add(path)
    for path in (Path.home() / "heiwa-universe", Path.home() / "heiwa"):
        resolved = path.expanduser().resolve()
        if resolved in seen:
            continue
        candidates.append(resolved)
        seen.add(resolved)
    return candidates


def discover_monorepo_root(start_path: Path | None = None) -> Path:
    current = (start_path or Path(__file__).resolve()).resolve()
    for candidate in _candidate_monorepo_roots():
        if _is_monorepo_root(candidate):
            return candidate
    for _ in range(6):
        probe = current if current.is_dir() else current.parent
        if _is_monorepo_root(probe):
            return probe
        if probe.parent == probe:
            break
        current = probe.parent
    for candidate in _candidate_monorepo_roots():
        if candidate.exists():
            return candidate
    return current if current.is_dir() else current.parent


def get_monorepo_root() -> Path:
    return discover_monorepo_root()

def load_node_identity() -> Dict[str, Any]:
    """
    Load the current node's identity from identity.json.
    Checks common locations (root, ~/.heiwa, /app).
    """
    root = get_monorepo_root()
    search_paths = [
        root / "identity.json",
        Path.home() / ".heiwa" / "identity.json",
        Path("/app/identity.json")
    ]
    
    for path in search_paths:
        if path.exists():
            try:
                with open(path, "r") as f:
                    return json.load(f)
            except: pass
            
    return {"uuid": "unknown", "name": "ghost-node", "role": "worker", "capabilities": []}

def get_tailscale_ip() -> str:
    """Get the current node's Tailscale IP."""
    import subprocess
    try:
        # Try local first
        result = subprocess.run(["tailscale", "ip", "-4"], capture_output=True, text=True, timeout=2)
        if result.returncode == 0:
            return result.stdout.strip()
        # Try Railway socket
        result = subprocess.run(["tailscale", "--socket=/tmp/tailscaled.sock", "ip", "-4"], capture_output=True, text=True, timeout=2)
        if result.returncode == 0:
            return result.stdout.strip()
    except: pass
    return "127.0.0.1"
