"""Provider auth flows for the Heiwa CLI."""

from __future__ import annotations

import json
import os
import subprocess
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from rich.console import Console
from rich.table import Table

from heiwa_cli.context import CLIContext

console = Console(stderr=True)

SUPPORTED_PROVIDERS = ("gemini", "codex", "claude", "antigravity")
KEYCHAIN_SERVICE = "heiwa-cli-provider-auth"


class KeychainStore:
    """Store local provider handles in the OS keychain when possible."""

    def __init__(self) -> None:
        self._backend = "none"
        self._keyring = None
        try:
            import keyring  # type: ignore

            self._keyring = keyring
            self._backend = "keyring"
        except Exception:
            if sys_platform() == "darwin":
                self._backend = "security"

    def set(self, account: str, value: str) -> None:
        if self._backend == "keyring" and self._keyring is not None:
            self._keyring.set_password(KEYCHAIN_SERVICE, account, value)
            return
        if self._backend == "security":
            subprocess.run(
                ["security", "add-generic-password", "-U", "-s", KEYCHAIN_SERVICE, "-a", account, "-w", value],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )

    def get(self, account: str) -> str | None:
        if self._backend == "keyring" and self._keyring is not None:
            return self._keyring.get_password(KEYCHAIN_SERVICE, account)
        if self._backend == "security":
            proc = subprocess.run(
                ["security", "find-generic-password", "-s", KEYCHAIN_SERVICE, "-a", account, "-w"],
                check=False,
                capture_output=True,
                text=True,
            )
            if proc.returncode == 0:
                return proc.stdout.strip()
        return None

    def delete(self, account: str) -> None:
        if self._backend == "keyring" and self._keyring is not None:
            try:
                self._keyring.delete_password(KEYCHAIN_SERVICE, account)
            except Exception:
                pass
            return
        if self._backend == "security":
            subprocess.run(
                ["security", "delete-generic-password", "-s", KEYCHAIN_SERVICE, "-a", account],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )


def sys_platform() -> str:
    import sys

    return sys.platform


@dataclass(slots=True)
class ProviderSpec:
    provider: str
    cli_provider: str
    label: str
    login_cmd: list[str] | None
    status_cmd: list[str] | None
    logout_cmd: list[str] | None
    test_cmd: list[str] | None
    interactive_login: bool = False
    interactive_logout: bool = False
    cwd: Path | None = None
    extra_env: dict[str, str] | None = None


class ProviderAuthManager:
    def __init__(self, ctx: CLIContext) -> None:
        self.ctx = ctx
        self.keychain = KeychainStore()

    def _provider_spec(self, provider: str) -> ProviderSpec:
        alias = self.ctx.provider_alias(provider)
        config = self.ctx.provider_registry.resolve(alias)
        cwd = self.ctx.root
        env = dict(config.env or {})
        if alias == "codex":
            return ProviderSpec(
                provider="codex",
                cli_provider=alias,
                label="OpenAI Codex",
                login_cmd=["codex", "login"],
                status_cmd=["codex", "login", "status"],
                logout_cmd=["codex", "logout"],
                test_cmd=[
                    "codex",
                    "exec",
                    "Respond with exactly OK.",
                    "--sandbox",
                    "read-only",
                    "--ask-for-approval",
                    "never",
                    "--cd",
                    str(self.ctx.root),
                ],
                interactive_login=True,
                cwd=cwd,
            )
        if alias == "claude-code":
            return ProviderSpec(
                provider="claude",
                cli_provider=alias,
                label="Claude Code",
                login_cmd=["claude", "auth", "login"],
                status_cmd=["claude", "auth", "status", "--json"],
                logout_cmd=["claude", "auth", "logout"],
                test_cmd=[
                    "claude",
                    "-p",
                    "Respond with exactly OK.",
                    "--output-format",
                    "json",
                    "--permission-mode",
                    "default",
                    "--add-dir",
                    str(self.ctx.root),
                ],
                interactive_login=True,
                cwd=cwd,
            )
        if alias == "google-gemini-cli":
            return ProviderSpec(
                provider="gemini",
                cli_provider=alias,
                label="Gemini CLI",
                login_cmd=["gemini", "--prompt-interactive", "/auth"],
                status_cmd=None,
                logout_cmd=["gemini", "--prompt-interactive", "/auth logout"],
                test_cmd=[
                    "gemini",
                    "--prompt",
                    "Respond with exactly OK.",
                    "--output-format",
                    "json",
                    "--approval-mode",
                    "default",
                ],
                interactive_login=True,
                interactive_logout=True,
                cwd=cwd,
            )
        if alias == "google-antigravity":
            return ProviderSpec(
                provider="antigravity",
                cli_provider=alias,
                label="Antigravity via OpenClaw",
                login_cmd=["openclaw", "--profile", env.get("OPENCLAW_PROFILE", "heiwa-antigravity"), "configure", "--section", "model"],
                status_cmd=["openclaw", "--profile", env.get("OPENCLAW_PROFILE", "heiwa-antigravity"), "status", "--usage", "--json"],
                logout_cmd=None,
                test_cmd=["openclaw", "--profile", env.get("OPENCLAW_PROFILE", "heiwa-antigravity"), "status", "--usage", "--json"],
                interactive_login=True,
                cwd=cwd,
                extra_env=env,
            )
        raise ValueError(f"Unsupported provider: {provider}")

    def _handle_ref(self, provider: str) -> str:
        return f"keychain://{KEYCHAIN_SERVICE}/{self.ctx.provider_account_id(provider)}"

    def _models(self, provider: str) -> list[str]:
        return self.ctx.provider_models(provider)

    def _base_record(
        self,
        provider: str,
        *,
        status: str,
        display_name: str | None = None,
        last_error: str | None = None,
        validated: bool = False,
    ) -> dict[str, Any]:
        alias = self.ctx.provider_alias(provider)
        config = self.ctx.provider_registry.resolve(alias)
        models = self._models(alias)
        return {
            "account_id": self.ctx.provider_account_id(provider),
            "provider_id": alias,
            "node_id": self.ctx.node_id,
            "auth_kind": config.auth_kind,
            "local_handle_ref": self._handle_ref(provider),
            "status": status,
            "display_name": display_name or alias,
            "default_model": models[0] if models else None,
            "rate_group": config.rate_group,
            "available_models": models,
            "last_validated_at": datetime.now(timezone.utc).isoformat() if validated else None,
            "last_error": last_error,
            "updated_at": datetime.now(timezone.utc).isoformat(),
        }

    def _cache_record(self, record: dict[str, Any]) -> None:
        cached = self.ctx.read_provider_cache()
        cached[record["provider_id"]] = record
        self.ctx.write_provider_cache(cached)

    def _sync_record(self, record: dict[str, Any]) -> dict[str, Any]:
        handle_payload = {
            "provider_id": record["provider_id"],
            "account_id": record["account_id"],
            "updated_at": record["updated_at"],
        }
        if str(record.get("status") or "") == "connected":
            self.keychain.set(record["account_id"], json.dumps(handle_payload))
        elif str(record.get("status") or "") == "disconnected":
            self.keychain.delete(record["account_id"])
        self._cache_record(record)
        try:
            self.ctx.db.upsert_provider_account_status(record)
        except Exception:
            pass
        return record

    def _load_cached_record(self, provider: str) -> dict[str, Any] | None:
        alias = self.ctx.provider_alias(provider)
        try:
            row = self.ctx.db.get_provider_account(self.ctx.provider_account_id(provider))
            if row:
                return row
        except Exception:
            pass
        cached = self.ctx.read_provider_cache()
        row = cached.get(alias)
        return row if isinstance(row, dict) else None

    def _run_capture(self, spec: ProviderSpec, cmd: list[str]) -> tuple[int, str]:
        env = os.environ.copy()
        env.update(spec.extra_env or {})
        proc = subprocess.run(
            cmd,
            cwd=str(spec.cwd or self.ctx.root),
            env=env,
            check=False,
            capture_output=True,
            text=True,
        )
        output = "\n".join(part for part in (proc.stdout, proc.stderr) if part).strip()
        return proc.returncode, output

    def _run_interactive(self, spec: ProviderSpec, cmd: list[str]) -> int:
        env = os.environ.copy()
        env.update(spec.extra_env or {})
        proc = subprocess.run(cmd, cwd=str(spec.cwd or self.ctx.root), env=env, check=False)
        return proc.returncode

    @staticmethod
    def _json_from_output(output: str) -> dict[str, Any] | None:
        text = str(output or "").strip()
        if not text:
            return None
        start = text.find("{")
        end = text.rfind("}")
        if start == -1 or end <= start:
            return None
        try:
            return json.loads(text[start : end + 1])
        except Exception:
            return None

    def _record_from_probe(self, provider: str, exit_code: int, output: str) -> dict[str, Any]:
        spec = self._provider_spec(provider)
        parsed = self._json_from_output(output)
        display_name = spec.label
        last_error = None if exit_code == 0 else (output.strip() or f"{spec.label} auth check failed")
        if isinstance(parsed, dict):
            display_name = (
                str(parsed.get("account", {}).get("email") or "")
                or str(parsed.get("user", {}).get("email") or "")
                or str(parsed.get("email") or "")
                or display_name
            )
        status = "connected" if exit_code == 0 else "error"
        record = self._base_record(
            provider,
            status=status,
            display_name=display_name,
            last_error=last_error,
            validated=exit_code == 0,
        )
        return self._sync_record(record)

    def login(self, provider: str) -> dict[str, Any]:
        spec = self._provider_spec(provider)
        if not spec.login_cmd:
            return self._sync_record(
                self._base_record(provider, status="unsupported", display_name=spec.label, last_error="Login is not supported.")
            )
        if self._run_interactive(spec, spec.login_cmd) != 0:
            return self._sync_record(
                self._base_record(provider, status="error", display_name=spec.label, last_error=f"{spec.label} login failed.")
            )
        return self.test(provider)

    def status(self, provider: str) -> dict[str, Any]:
        spec = self._provider_spec(provider)
        if spec.status_cmd:
            code, output = self._run_capture(spec, spec.status_cmd)
            return self._record_from_probe(provider, code, output)
        return self.test(provider)

    def logout(self, provider: str) -> dict[str, Any]:
        spec = self._provider_spec(provider)
        if not spec.logout_cmd:
            return self._sync_record(
                self._base_record(
                    provider,
                    status="unsupported",
                    display_name=spec.label,
                    last_error=f"{spec.label} does not expose an official logout flow through its CLI.",
                )
            )
        if self._run_interactive(spec, spec.logout_cmd) != 0:
            return self._sync_record(
                self._base_record(provider, status="error", display_name=spec.label, last_error=f"{spec.label} logout failed.")
            )
        self.keychain.delete(self.ctx.provider_account_id(provider))
        return self._sync_record(
            self._base_record(provider, status="disconnected", display_name=spec.label, validated=False)
        )

    def test(self, provider: str) -> dict[str, Any]:
        spec = self._provider_spec(provider)
        cmd = spec.test_cmd or spec.status_cmd
        if not cmd:
            cached = self._load_cached_record(provider)
            if cached:
                return cached
            return self._sync_record(
                self._base_record(provider, status="unknown", display_name=spec.label, last_error="No probe command available.")
            )
        code, output = self._run_capture(spec, cmd)
        return self._record_from_probe(provider, code, output)


def _normalize_provider(arg: str) -> str:
    provider = str(arg or "").strip().lower()
    if provider not in SUPPORTED_PROVIDERS:
        raise ValueError(f"Unsupported provider '{provider}'. Expected one of: {', '.join(SUPPORTED_PROVIDERS)}")
    return provider


def _print_records(records: list[dict[str, Any]]) -> None:
    table = Table(title="Heiwa Provider Connections")
    table.add_column("Provider")
    table.add_column("Status")
    table.add_column("Auth Kind")
    table.add_column("Rate Group")
    table.add_column("Model")
    table.add_column("Validated")
    table.add_column("Error")
    for record in records:
        status = str(record.get("status") or "unknown")
        styled_status = {
            "connected": "[green]connected[/green]",
            "disconnected": "[yellow]disconnected[/yellow]",
            "unsupported": "[yellow]unsupported[/yellow]",
            "error": "[red]error[/red]",
        }.get(status, status)
        table.add_row(
            str(record.get("provider_id") or ""),
            styled_status,
            str(record.get("auth_kind") or ""),
            str(record.get("rate_group") or ""),
            str(record.get("default_model") or ""),
            str(record.get("last_validated_at") or "never"),
            str(record.get("last_error") or ""),
        )
    console.print(table)


async def handle_auth_command(ctx: CLIContext, args: list[str]) -> int:
    if not args:
        console.print("[yellow]Usage: heiwa auth <login|status|logout|test> <provider>[/yellow]")
        return 1

    action = str(args[0] or "").strip().lower()
    manager = ProviderAuthManager(ctx)

    if action == "status" and len(args) == 1:
        records = [manager.status(provider) for provider in SUPPORTED_PROVIDERS]
        _print_records(records)
        return 0

    if len(args) < 2:
        console.print("[yellow]Usage: heiwa auth <login|status|logout|test> <provider>[/yellow]")
        return 1

    provider = _normalize_provider(args[1])
    if action == "login":
        record = manager.login(provider)
    elif action == "status":
        record = manager.status(provider)
    elif action == "logout":
        record = manager.logout(provider)
    elif action == "test":
        record = manager.test(provider)
    else:
        console.print(f"[yellow]Unknown auth action: {action}[/yellow]")
        return 1

    _print_records([record])
    return 0 if str(record.get("status") or "") in {"connected", "disconnected"} else 1
