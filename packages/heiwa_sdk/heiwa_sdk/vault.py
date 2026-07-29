import os
import base64
import logging
from pathlib import Path
from typing import Any
from cryptography.fernet import Fernet, InvalidToken
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
        except InvalidToken:
            logger.error("Decryption failed: Invalid token")
            return "[DECRYPTION_ERROR]"
        except Exception as e:
            logger.error(f"Decryption failed with unexpected error: {e}")
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

    def export_for_remote_env(self, service: str | None = None) -> tuple[int, str]:
        """Return a clear boundary for remote env sync; local files remain authority."""
        data = self._read_all()
        if not data:
            return 1, "Vault is empty -- nothing to export."
        target = f" for {service}" if service else ""
        return 0, f"Prepared {len(data)} secret(s){target}; remote mutation requires explicit platform approval."


class UserVault:
    """
    Legacy compatibility surface for an explicitly injected credential backend.
    """

    def __init__(self, stdb: Any):
        self.stdb = stdb
        self.cipher = InstanceVault()

    def store_credential(
        self,
        owner_id: str,
        provider_id: str,
        credential_kind: str,
        plaintext_value: str,
        rate_group: str,
        display_label: str | None = None,
    ) -> str:
        """Encrypt and store a credential through the injected backend."""
        import uuid

        credential_id = f"cred-{owner_id[:8]}-{provider_id[:8]}-{uuid.uuid4().hex[:6]}"
        encrypted = self.cipher.encrypt(plaintext_value)

        self.stdb.update_provider_credential(
            credential_id=credential_id,
            user_id=owner_id,
            provider_id=provider_id,
            credential_kind=credential_kind,
            credential_enc=encrypted,
            rate_group=rate_group,
            display_label=display_label,
        )
        return credential_id

    def list_credentials(self, owner_id: str) -> list[dict[str, Any]]:
        """Returns metadata for all active credentials for an owner."""
        creds = self.stdb.get_provider_credentials(owner_id)
        # Redact the actual encrypted string for list views
        for c in creds:
            c["credential_enc"] = "[ENCRYPTED]"
        return creds

    def resolve_credential(self, owner_id: str, provider_id: str) -> str | None:
        """Finds and decrypts the active credential for a provider."""
        # Compatibility query returns list[dict].
        esc_owner = self.stdb._escape_sql_literal(owner_id)
        esc_provider = self.stdb._escape_sql_literal(provider_id)

        rows = self.stdb.query(
            f"SELECT * FROM provider_credentials "
            f"WHERE user_id = '{esc_owner}' AND provider_id = '{esc_provider}' "
            f"AND status = 'active' LIMIT 1"
        )
        if not rows:
            return None

        return self.cipher.decrypt(rows[0]["credential_enc"])
