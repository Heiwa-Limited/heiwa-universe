import os
import base64
import logging
from pathlib import Path
from cryptography.fernet import Fernet
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC

logger = logging.getLogger("SDK.Vault")

VAULT_ENV_PATH = Path.home() / ".heiwa" / "vault.env"


class InstanceVault:
    """
    Handles encryption/decryption for Agent Instances.
    Uses a master key to derive instance-specific keys.
    """
    def __init__(self, master_key: str = None):
        self.master_key = master_key or os.getenv("HEIWA_MASTER_KEY")
        if not self.master_key:
            raise ValueError("HEIWA_MASTER_KEY is required for InstanceVault.")
        self._fernet = self._derive_fernet(self.master_key)

    def _derive_fernet(self, secret: str) -> Fernet:
        salt = b'heiwa-swarm-salt'
        kdf = PBKDF2HMAC(
            algorithm=hashes.SHA256(),
            length=32,
            salt=salt,
            iterations=100000,
        )
        key = base64.urlsafe_b64encode(kdf.derive(secret.encode()))
        return Fernet(key)

    def encrypt(self, data: str) -> str:
        if not data: return ""
        return self._fernet.encrypt(data.encode()).decode()

    def decrypt(self, token: str) -> str:
        if not token: return ""
        try:
            return self._fernet.decrypt(token.encode()).decode()
        except Exception as e:
            logger.error(f"Decryption failed: {e}")
            return "[DECRYPTION_ERROR]"

    @staticmethod
    def generate_master_key():
        return Fernet.generate_key().decode()


class VaultStore:
    """
    Plaintext key=value store at ~/.heiwa/vault.env.

    Secrets live outside the repo (never committed) and are loaded by
    load_swarm_env() at the highest priority slot. Use `heiwa vault` CLI
    to manage. Use export/import for cross-machine sync.
    """

    def __init__(self, path: Path = VAULT_ENV_PATH):
        self.path = path

    def _read_all(self) -> dict[str, str]:
        if not self.path.exists():
            return {}
        out: dict[str, str] = {}
        for line in self.path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if "=" in line:
                k, _, v = line.partition("=")
                out[k.strip()] = v.strip()
        return out

    def _write_all(self, data: dict[str, str]) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        lines = ["# Heiwa vault — managed by `heiwa vault set`. DO NOT commit.\n"]
        for k, v in sorted(data.items()):
            lines.append(f"{k}={v}\n")
        self.path.write_text("".join(lines), encoding="utf-8")
        self.path.chmod(0o600)

    def set(self, key: str, value: str) -> None:
        data = self._read_all()
        data[key] = value
        self._write_all(data)

    def get(self, key: str) -> str | None:
        return self._read_all().get(key)

    def delete(self, key: str) -> bool:
        data = self._read_all()
        if key not in data:
            return False
        del data[key]
        self._write_all(data)
        return True

    def list_keys(self) -> list[str]:
        return sorted(self._read_all().keys())

    def export_env(self) -> str:
        """Return all secrets as KEY=VALUE lines (for piping / scp to another node)."""
        data = self._read_all()
        return "\n".join(f"{k}={v}" for k, v in sorted(data.items()))

    def sync_to_railway(self, service: str | None = None) -> tuple[int, str]:
        """Push all vault secrets to Railway via CLI. Returns (exit_code, message)."""
        import shutil
        import subprocess
        import tempfile

        if not shutil.which("railway"):
            return 1, "railway CLI not found. Install: npm i -g @railway/cli"

        data = self._read_all()
        if not data:
            return 1, "Vault is empty — nothing to sync."

        with tempfile.NamedTemporaryFile(mode="w", suffix=".env", delete=False) as f:
            for k, v in data.items():
                f.write(f"{k}={v}\n")
            tmp = f.name

        try:
            cmd = ["railway", "variables", "import", tmp]
            if service:
                cmd += ["--service", service]
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
            if result.returncode == 0:
                return 0, f"Synced {len(data)} secret(s) to Railway."
            return 1, result.stderr.strip() or "railway import failed"
        finally:
            os.unlink(tmp)
