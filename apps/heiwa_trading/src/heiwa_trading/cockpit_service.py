from __future__ import annotations

import argparse
import json
import os
import plistlib
import subprocess
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parents[2]
LABEL = "com.heiwa.mac-agent.cockpit"
PLIST_PATH = Path.home() / "Library" / "LaunchAgents" / f"{LABEL}.plist"
LOG_DIR = Path.home() / ".heiwa" / "logs"


def _launchctl_domain() -> str:
    return f"gui/{os.getuid()}"


def _launchctl_target() -> str:
    return f"{_launchctl_domain()}/{LABEL}"


def build_launch_agent_plist(*, port: int, host: str = "127.0.0.1", root_dir: Path = ROOT_DIR) -> bytes:
    LOG_DIR.mkdir(parents=True, exist_ok=True)
    payload = {
        "Label": LABEL,
        "ProgramArguments": [
            str(root_dir / "bin" / "mac-agent"),
            "cockpit",
            "serve",
            "--host",
            host,
            "--port",
            str(port),
            "--no-open",
        ],
        "WorkingDirectory": str(root_dir),
        "RunAtLoad": True,
        "KeepAlive": True,
        "StandardOutPath": str(LOG_DIR / "cockpit.log"),
        "StandardErrorPath": str(LOG_DIR / "cockpit.err.log"),
    }
    return plistlib.dumps(payload)


def _run(args: list[str], *, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, capture_output=True, text=True, check=check)


def install_service(*, port: int, host: str = "127.0.0.1", root_dir: Path = ROOT_DIR) -> dict[str, object]:
    PLIST_PATH.parent.mkdir(parents=True, exist_ok=True)
    PLIST_PATH.write_bytes(build_launch_agent_plist(port=port, host=host, root_dir=root_dir))
    _run(["launchctl", "bootout", _launchctl_target()], check=False)
    _run(["launchctl", "bootstrap", _launchctl_domain(), str(PLIST_PATH)])
    _run(["launchctl", "kickstart", "-k", _launchctl_target()])
    return service_status()


def stop_service() -> dict[str, object]:
    result = _run(["launchctl", "bootout", _launchctl_target()], check=False)
    return {
        "installed": PLIST_PATH.exists(),
        "running": False,
        "stdout": result.stdout.strip(),
        "stderr": result.stderr.strip(),
    }


def start_service() -> dict[str, object]:
    if not PLIST_PATH.exists():
        raise SystemExit(f"LaunchAgent not installed at {PLIST_PATH}")
    _run(["launchctl", "bootstrap", _launchctl_domain(), str(PLIST_PATH)], check=False)
    _run(["launchctl", "kickstart", "-k", _launchctl_target()])
    return service_status()


def uninstall_service() -> dict[str, object]:
    _run(["launchctl", "bootout", _launchctl_target()], check=False)
    if PLIST_PATH.exists():
        PLIST_PATH.unlink()
    return {
        "installed": False,
        "running": False,
        "plist_path": str(PLIST_PATH),
    }


def service_status() -> dict[str, object]:
    result = _run(["launchctl", "print", _launchctl_target()], check=False)
    return {
        "installed": PLIST_PATH.exists(),
        "running": result.returncode == 0,
        "plist_path": str(PLIST_PATH),
        "stdout": result.stdout.strip(),
        "stderr": result.stderr.strip(),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Manage the mac-agent cockpit LaunchAgent")
    subparsers = parser.add_subparsers(dest="command", required=True)

    install_parser = subparsers.add_parser("install", help="Install and start the LaunchAgent")
    install_parser.add_argument("--port", type=int, default=19890)
    install_parser.add_argument("--host", default="127.0.0.1")

    subparsers.add_parser("start", help="Start the existing LaunchAgent")
    subparsers.add_parser("stop", help="Stop the LaunchAgent")
    subparsers.add_parser("status", help="Show LaunchAgent status")
    subparsers.add_parser("uninstall", help="Unload and remove the LaunchAgent")

    args = parser.parse_args()
    if args.command == "install":
        payload = install_service(port=args.port, host=args.host)
    elif args.command == "start":
        payload = start_service()
    elif args.command == "stop":
        payload = stop_service()
    elif args.command == "status":
        payload = service_status()
    elif args.command == "uninstall":
        payload = uninstall_service()
    else:  # pragma: no cover
        raise SystemExit(f"Unknown command {args.command}")
    print(json.dumps(payload, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
