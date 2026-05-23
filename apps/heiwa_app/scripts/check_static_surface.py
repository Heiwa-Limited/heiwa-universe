#!/usr/bin/env python3
"""Validate the Heiwa static public shell against current self-hosted public-surface claims."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
WEB_ROOT = ROOT / "apps" / "heiwa_app" / "clients" / "web"


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

    platform = data.get("platform", {})
    if platform.get("state_ledger") != "spacetimedb_maincloud":
        problems.append(
            f"{path}: expected platform.state_ledger to be 'spacetimedb_maincloud', got {platform.get('state_ledger')!r}"
        )
    if platform.get("state_endpoint") != "maincloud.spacetimedb.com":
        problems.append(
            f"{path}: expected platform.state_endpoint to be 'maincloud.spacetimedb.com', got {platform.get('state_endpoint')!r}"
        )

    hosts = {entry.get("host") for entry in data.get("domains", [])}
    expected_hosts = {"heiwa.ltd", "status.heiwa.ltd", "api.heiwa.ltd", "docs.heiwa.ltd"}
    missing = sorted(expected_hosts - hosts)
    if missing:
        problems.append(f"{path}: missing expected hosts {', '.join(missing)}")

    if "app.heiwa.ltd" in hosts:
        problems.append(f"{path}: app.heiwa.ltd should not be presented as an active public surface")
    if "auth.heiwa.ltd" in hosts:
        problems.append(f"{path}: auth.heiwa.ltd should not be presented as an active public surface")
    if "trade.heiwa.ltd" in hosts:
        problems.append(f"{path}: trade.heiwa.ltd should not be presented as an active public surface")

    return problems


def main() -> int:
    problems: list[str] = []

    required_files = [
        WEB_ROOT / "index.html",
        WEB_ROOT / "providers.html",
        WEB_ROOT / "download.html",
        WEB_ROOT / "status.html",
        WEB_ROOT / "domains.html",
        WEB_ROOT / "governance.html",
        WEB_ROOT / "install",
        WEB_ROOT / "install.sh",
        WEB_ROOT / "_headers",
        WEB_ROOT / "_redirects",
        WEB_ROOT / "assets" / "status.js",
        WEB_ROOT / "assets" / "domains.bootstrap.json",
        WEB_ROOT / "assets" / "providers.json",
        WEB_ROOT / "vs" / "manifest.html",
        WEB_ROOT / "vs" / "openrouter.html",
        WEB_ROOT / "vs" / "litellm.html",
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
            "Reuse the AI subscriptions you already pay for.",
            "Self-hosted local AI runtime",
            "curl -fsSL https://heiwa.ltd/install | sh",
            "You host everything. Heiwa ships software, not a hosted operator app.",
        )
    )
    problems.extend(
        require_absent(
            WEB_ROOT / "index.html",
            "https://api.heiwa.ltd/auth/discord",
            "Every AI provider. One orchestration layer. Your keys.",
            "hosted control plane",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "providers.html",
            "OAuth CLI",
            "API key",
            "Providers still own auth and inference internals.",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "download.html",
            "GitHub Releases remain the source of truth",
            "heiwa doctor",
            "heiwa app",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "install",
            "HEIWA_REPO_URL",
            "cargo install --path",
            "heiwa app start --no-open",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "governance.html",
            "installed local runtime",
            "Cloudflare Pages",
            "Cloudflare Pages",
            "SpacetimeDB",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "domains.html",
            "Cloudflare Pages",
            "public-safe routing intent only",
            "status.heiwa.ltd",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "_headers",
            "Content-Security-Policy",
            "Strict-Transport-Security",
            "Content-Type: text/x-shellscript; charset=utf-8",
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
    problems.extend(
        require_contains(
            WEB_ROOT / "vs" / "manifest.html",
            "Different categories, honestly.",
            "local operator runtime",
            "smart model router",
        )
    )
    problems.extend(
        require_absent(
            WEB_ROOT / "vs" / "manifest.html",
            "Manifest is bad",
            "we are better",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "vs" / "openrouter.html",
            "Local runtime vs remote gateway.",
            "Cloud proxy",
            "5% fee",
        )
    )
    problems.extend(
        require_contains(
            WEB_ROOT / "vs" / "litellm.html",
            "Different layers of the stack.",
            "compatibility library",
            "operator runtime",
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
