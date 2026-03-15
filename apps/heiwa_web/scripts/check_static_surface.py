#!/usr/bin/env python3
"""Validate the Heiwa static public shell against current public-surface claims."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
WEB_ROOT = ROOT / "apps" / "heiwa_web" / "clients" / "web"


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def require_contains(path: Path, *snippets: str) -> list[str]:
    text = read_text(path)
    problems: list[str] = []
    for snippet in snippets:
        if snippet not in text:
            problems.append(f"{path}: missing required snippet {snippet!r}")
    return problems


def require_absent(path: Path, *snippets: str) -> list[str]:
    text = read_text(path)
    problems: list[str] = []
    for snippet in snippets:
        if snippet in text:
            problems.append(f"{path}: contains retired or inflated claim {snippet!r}")
    return problems


def check_domain_manifest(path: Path) -> list[str]:
    problems: list[str] = []
    data = json.loads(read_text(path))

    public_web = data.get("platform", {}).get("public_web")
    if public_web != "cloudflare_pages":
        problems.append(f"{path}: expected platform.public_web to be 'cloudflare_pages', got {public_web!r}")

    hosts = {entry.get("host") for entry in data.get("domains", [])}
    expected_hosts = {"heiwa.ltd", "status.heiwa.ltd", "api.heiwa.ltd", "docs.heiwa.ltd"}
    missing = sorted(expected_hosts - hosts)
    if missing:
        problems.append(f"{path}: missing expected hosts {', '.join(missing)}")

    if "auth.heiwa.ltd" in hosts:
        problems.append(f"{path}: auth.heiwa.ltd should not be presented as an active public surface")

    return problems


def main() -> int:
    problems: list[str] = []

    required_files = [
        WEB_ROOT / "index.html",
        WEB_ROOT / "status.html",
        WEB_ROOT / "domains.html",
        WEB_ROOT / "governance.html",
        WEB_ROOT / "_headers",
        WEB_ROOT / "assets" / "status.js",
        WEB_ROOT / "assets" / "domains.bootstrap.json",
    ]
    for path in required_files:
        if not path.exists():
            problems.append(f"missing required file {path}")

    if problems:
        print("\n".join(problems))
        return 1

    problems.extend(
        require_contains(
            WEB_ROOT / "index.html",
            "CLI, MCP, HTTP API, and docs",
            "Railway",
            "SpacetimeDB",
            "WebSockets first",
        )
    )
    problems.extend(
        require_absent(
            WEB_ROOT / "index.html",
            "Autonomous AI-Dentity Enterprise",
            "NATS",
            "PostgreSQL sovereignty",
            "OpenClaw Integration",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "status.html",
            "WebSocket-first",
            "transport-mode",
            "last-updated",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "governance.html",
            "CLI, MCP, HTTP API, and docs",
            "Cloudflare Pages",
            "SpacetimeDB",
        )
    )
    problems.extend(
        require_absent(
            WEB_ROOT / "governance.html",
            "enterprise platform foundation",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "domains.html",
            "Cloudflare Pages",
            "Railway runtime",
            "domains.js",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "_headers",
            "Content-Security-Policy",
            "Strict-Transport-Security",
            "wss://api.heiwa.ltd",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "assets" / "status.js",
            "wss://api.heiwa.ltd/status/ws",
            "http-fallback",
            "websocket-live",
        )
    )
    problems.extend(check_domain_manifest(WEB_ROOT / "assets" / "domains.bootstrap.json"))

    if problems:
        print("\n".join(problems))
        return 1

    print("Static public surface checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
