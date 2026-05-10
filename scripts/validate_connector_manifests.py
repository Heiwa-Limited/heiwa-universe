#!/usr/bin/env python3
"""Heiwa connector truth gate.

Validates every connectors/*.connector.json file against a fixed set of
governance rules and emits deterministic JSON. Designed to run offline so
audit tests can call it without network access.

Rules enforced (fail-closed):

    1.  Manifest must declare every required field listed in the schema.
    2.  support_level must be a known enum value.
    3.  Every capability must declare risk_class and a known support_status.
    4.  Capabilities with side_effect "write" or "destructive" must set
        receipt_required: true (manifest-level receipt_required does not
        relax this; per-capability receipt is required for written work).
    5.  support_level "official_api" must include a revocation_path.
    6.  support_level "unsupported" must not contain capabilities marked
        support_status "live".
    7.  support_level "target" must not contain capabilities marked
        support_status "live".
    8.  Capability ids must be unique within a manifest.
    9.  Each capability must declare permissions or permission_notes.

Documentation claim guard:

    Scans docs/product-contract.md and docs/capability-fabric.md for narrow
    product-grade claim phrasings (for example "the GitHub connector is
    live" or "supported as product-grade"). A claim about a connector that
    has no validated manifest fails closed.

    The guard is intentionally narrow. Capability-fabric language about
    "lanes" and "first useful actions" is explicit future-target prose and
    is not treated as a product claim. Tighten the patterns once Heiwa
    adopts a structured connector-claim block in product docs.

    TODO(connector-claim-guard): replace regex pass with a typed
    connector-claim block parsed from product docs once the canonical
    claim grammar lands. The current pass is conservative on purpose.

Output shape (stable):

    {
      "ok": bool,
      "checked": ["connectors/<file>", ...],
      "errors": [
        {"file": "<path>", "code": "<code>", "message": "..."},
        ...
      ]
    }

Exit status is 0 when ok is true, 1 otherwise.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


SUPPORT_LEVELS = {
    "official_api",
    "local_os_bridge",
    "user_mediated",
    "webhook",
    "polling",
    "third_party_bridge",
    "unsupported",
    "target",
}

RISK_CLASSES = {"silent", "notify", "approve", "forbidden"}

SUPPORT_STATUSES = {"live", "target", "unsupported"}

SIDE_EFFECTS = {"read", "write", "destructive"}

ID_PATTERN = re.compile(r"^[a-z0-9][a-z0-9_.-]*$")

AUTH_MODES = {
    "fine_grained_pat",
    "classic_pat",
    "github_app",
    "oauth_cli",
    "oauth_device",
    "oauth_authcode",
    "api_key",
    "local_runtime",
    "local_os_bridge",
    "webhook_signature",
}

REQUIRED_TOP_FIELDS = (
    "id",
    "name",
    "support_level",
    "auth",
    "capabilities",
    "receipt_required",
)

REQUIRED_CAPABILITY_FIELDS = (
    "id",
    "description",
    "risk_class",
    "receipt_required",
    "support_status",
)

TOP_ALLOWED_FIELDS = {
    "$schema",
    "id",
    "name",
    "vendor",
    "summary",
    "support_level",
    "auth",
    "revocation_path",
    "receipt_required",
    "risk_classes",
    "source_notes",
    "capabilities",
    "claim_aliases",
}

CAPABILITY_ALLOWED_FIELDS = {
    "id",
    "description",
    "risk_class",
    "receipt_required",
    "support_status",
    "side_effect",
    "permissions",
    "permission_notes",
    "scopes",
    "rate_hint",
}

DOC_CLAIM_FILES = ("docs/product-contract.md", "docs/capability-fabric.md")

# Narrow phrasings that count as a product-grade claim. Must reference a
# connector by alias or id (case-insensitive). Anything else is treated as
# directional language and ignored.
DOC_CLAIM_PATTERNS = (
    r"{name}\s+connector\s+is\s+(live|reference|product[- ]grade|production[- ]ready)",
    r"{name}\s+is\s+(live|production[- ]ready|product[- ]grade)\s+as\s+a\s+connector",
    r"officially\s+supports?\s+{name}\s+as\s+a\s+connector",
)


def repo_root() -> Path:
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--show-toplevel"],
            stderr=subprocess.DEVNULL,
        )
        return Path(out.decode().strip())
    except Exception:
        return Path(__file__).resolve().parent.parent


def load_json(path: Path) -> tuple[Any, str | None]:
    try:
        with path.open("r", encoding="utf-8") as fh:
            return json.load(fh), None
    except json.JSONDecodeError as exc:
        return None, f"invalid JSON: {exc.msg} (line {exc.lineno}, col {exc.colno})"
    except OSError as exc:
        return None, f"cannot read file: {exc}"


def add_error(errors: list[dict], file: str, code: str, message: str) -> None:
    errors.append({"file": file, "code": code, "message": message})


def validate_manifest(rel_path: str, data: Any, errors: list[dict]) -> None:
    if not isinstance(data, dict):
        add_error(errors, rel_path, "shape", "manifest must be a JSON object")
        return

    for key in data:
        if key not in TOP_ALLOWED_FIELDS:
            add_error(errors, rel_path, "unknown_field", f"unknown top-level field '{key}'")

    for field in REQUIRED_TOP_FIELDS:
        if field not in data:
            add_error(errors, rel_path, "missing_field", f"missing required field '{field}'")

    manifest_id = data.get("id")
    if manifest_id is not None:
        if not isinstance(manifest_id, str) or not ID_PATTERN.fullmatch(manifest_id):
            add_error(errors, rel_path, "invalid_id", "id must match ^[a-z0-9][a-z0-9_.-]*$")

    support_level = data.get("support_level")
    if support_level is not None and support_level not in SUPPORT_LEVELS:
        add_error(
            errors,
            rel_path,
            "invalid_enum",
            f"support_level '{support_level}' not in {sorted(SUPPORT_LEVELS)}",
        )

    auth = data.get("auth")
    if isinstance(auth, list):
        if not auth:
            add_error(errors, rel_path, "shape", "auth must be a non-empty array")
        seen_auth: set[str] = set()
        for mode in auth:
            if not isinstance(mode, str):
                add_error(errors, rel_path, "shape", "auth entries must be strings")
                continue
            if mode in seen_auth:
                add_error(errors, rel_path, "duplicate_auth", f"duplicate auth mode '{mode}'")
            seen_auth.add(mode)
            if mode not in AUTH_MODES:
                add_error(
                    errors,
                    rel_path,
                    "invalid_enum",
                    f"auth mode '{mode}' not in {sorted(AUTH_MODES)}",
                )
    elif "auth" in data:
        add_error(errors, rel_path, "shape", "auth must be an array")

    if data.get("receipt_required") is not True and data.get("receipt_required") is not False:
        if "receipt_required" in data:
            add_error(errors, rel_path, "shape", "receipt_required must be boolean")

    aliases = data.get("claim_aliases")
    if aliases is not None:
        if not isinstance(aliases, list):
            add_error(errors, rel_path, "shape", "claim_aliases must be an array")
        else:
            seen_aliases: set[str] = set()
            for alias in aliases:
                if not isinstance(alias, str) or not alias.strip():
                    add_error(errors, rel_path, "shape", "claim_aliases entries must be non-empty strings")
                    continue
                lowered = alias.lower()
                if lowered in seen_aliases:
                    add_error(errors, rel_path, "duplicate_alias", f"duplicate claim alias '{alias}'")
                seen_aliases.add(lowered)

    if support_level == "official_api":
        rev = data.get("revocation_path")
        if not isinstance(rev, str) or not rev.strip():
            add_error(
                errors,
                rel_path,
                "missing_revocation",
                "official_api connector must declare a non-empty revocation_path",
            )

    capabilities = data.get("capabilities")
    if not isinstance(capabilities, list) or not capabilities:
        add_error(errors, rel_path, "shape", "capabilities must be a non-empty array")
        return

    seen_ids: set[str] = set()
    for index, cap in enumerate(capabilities):
        prefix = f"capabilities[{index}]"
        if not isinstance(cap, dict):
            add_error(errors, rel_path, "shape", f"{prefix} must be an object")
            continue

        for key in cap:
            if key not in CAPABILITY_ALLOWED_FIELDS:
                add_error(errors, rel_path, "unknown_field", f"{prefix} unknown field '{key}'")

        for field in REQUIRED_CAPABILITY_FIELDS:
            if field not in cap:
                add_error(
                    errors,
                    rel_path,
                    "missing_field",
                    f"{prefix} missing required field '{field}'",
                )

        cap_id = cap.get("id")
        if isinstance(cap_id, str):
            if not ID_PATTERN.fullmatch(cap_id):
                add_error(
                    errors,
                    rel_path,
                    "invalid_id",
                    f"{prefix} id must match ^[a-z0-9][a-z0-9_.-]*$",
                )
            if cap_id in seen_ids:
                add_error(
                    errors,
                    rel_path,
                    "duplicate_capability",
                    f"{prefix} duplicate capability id '{cap_id}'",
                )
            seen_ids.add(cap_id)
        elif cap_id is not None:
            add_error(errors, rel_path, "invalid_id", f"{prefix} id must be a string")

        risk = cap.get("risk_class")
        if risk is None:
            add_error(errors, rel_path, "missing_risk_class", f"{prefix} missing risk_class")
        elif risk not in RISK_CLASSES:
            add_error(
                errors,
                rel_path,
                "invalid_enum",
                f"{prefix} risk_class '{risk}' not in {sorted(RISK_CLASSES)}",
            )

        status = cap.get("support_status")
        if status is not None and status not in SUPPORT_STATUSES:
            add_error(
                errors,
                rel_path,
                "invalid_enum",
                f"{prefix} support_status '{status}' not in {sorted(SUPPORT_STATUSES)}",
            )

        side_effect = cap.get("side_effect", "read")
        if side_effect not in SIDE_EFFECTS:
            add_error(
                errors,
                rel_path,
                "invalid_enum",
                f"{prefix} side_effect '{side_effect}' not in {sorted(SIDE_EFFECTS)}",
            )

        if side_effect in {"write", "destructive"} and cap.get("receipt_required") is not True:
            add_error(
                errors,
                rel_path,
                "missing_receipt_for_write",
                f"{prefix} side_effect '{side_effect}' requires receipt_required=true",
            )

        if support_level == "unsupported" and status == "live":
            add_error(
                errors,
                rel_path,
                "unsupported_with_live_capability",
                f"{prefix} support_status 'live' is not allowed on an unsupported connector",
            )

        if support_level == "target" and status == "live":
            add_error(
                errors,
                rel_path,
                "target_with_live_capability",
                f"{prefix} support_status 'live' is not allowed on a target connector",
            )

        permissions = cap.get("permissions")
        permission_notes = cap.get("permission_notes")
        has_perms = isinstance(permissions, list) and len(permissions) > 0
        has_notes = isinstance(permission_notes, str) and permission_notes.strip() != ""
        if permissions is not None and not has_perms:
            add_error(errors, rel_path, "shape", f"{prefix} permissions must be a non-empty array")
        if not has_perms and not has_notes:
            add_error(
                errors,
                rel_path,
                "missing_permission_evidence",
                f"{prefix} must declare permissions[] or permission_notes",
            )

        if cap.get("receipt_required") is not True and cap.get("receipt_required") is not False:
            if "receipt_required" in cap:
                add_error(errors, rel_path, "shape", f"{prefix} receipt_required must be boolean")


def collect_doc_claim_aliases(manifests: list[dict]) -> dict[str, str]:
    """Return alias -> manifest_id for every connector that has a manifest."""
    aliases: dict[str, str] = {}
    for manifest in manifests:
        manifest_id = manifest.get("id")
        if not isinstance(manifest_id, str):
            continue
        aliases[manifest_id.lower()] = manifest_id
        for alias in manifest.get("claim_aliases") or []:
            if isinstance(alias, str) and alias.strip():
                aliases[alias.lower()] = manifest_id
    return aliases


def manifests_by_id(manifests: list[dict]) -> dict[str, dict]:
    result: dict[str, dict] = {}
    for manifest in manifests:
        manifest_id = manifest.get("id")
        if isinstance(manifest_id, str):
            result[manifest_id] = manifest
    return result


def manifest_has_live_capability(manifest: dict) -> bool:
    capabilities = manifest.get("capabilities")
    if not isinstance(capabilities, list):
        return False
    return any(
        isinstance(cap, dict) and cap.get("support_status") == "live"
        for cap in capabilities
    )


def scan_doc_claims(
    root: Path,
    known_alias_to_id: dict[str, str],
    known_manifests: dict[str, dict],
    errors: list[dict],
) -> None:
    """Narrow scan for product-grade connector claims in repo docs."""
    for rel in DOC_CLAIM_FILES:
        doc_path = root / rel
        if not doc_path.exists():
            continue
        try:
            text = doc_path.read_text(encoding="utf-8")
        except OSError as exc:
            add_error(errors, rel, "doc_read_failure", f"cannot read doc: {exc}")
            continue

        # Build candidate list: every alphabetic token longer than two chars
        # that could plausibly be a connector name. We only flag matches that
        # hit a DOC_CLAIM_PATTERN with a known-claim verb.
        candidates: set[str] = set()
        for match in re.finditer(r"[A-Z][A-Za-z0-9_-]{2,}", text):
            candidates.add(match.group(0))

        for candidate in candidates:
            for pattern in DOC_CLAIM_PATTERNS:
                regex = pattern.format(name=re.escape(candidate))
                claim_match = re.search(regex, text, flags=re.IGNORECASE)
                if claim_match:
                    manifest_id = known_alias_to_id.get(candidate.lower())
                    if manifest_id is None:
                        add_error(
                            errors,
                            rel,
                            "unbacked_product_claim",
                            (
                                f"docs claim '{candidate}' as a product-grade connector "
                                "but no validated manifest exists in connectors/"
                            ),
                        )
                        continue
                    claim_status = claim_match.group(1).lower() if claim_match.groups() else ""
                    if claim_status in {"live", "product-grade", "product grade", "production-ready", "production ready"}:
                        manifest = known_manifests.get(manifest_id, {})
                        if not manifest_has_live_capability(manifest):
                            add_error(
                                errors,
                                rel,
                                "overstated_product_claim",
                                (
                                    f"docs claim '{candidate}' as '{claim_status}' but "
                                    "the manifest has no live capability"
                                ),
                            )


def main() -> int:
    root = repo_root()
    connectors_dir = root / "connectors"
    schema_path = connectors_dir / "schema" / "connector.schema.json"

    errors: list[dict] = []
    checked: list[str] = []
    manifests: list[dict] = []

    if not schema_path.exists():
        add_error(
            errors,
            str(schema_path.relative_to(root)),
            "missing_schema",
            "connectors/schema/connector.schema.json is missing",
        )

    if not connectors_dir.exists():
        result = {
            "ok": False,
            "checked": [],
            "errors": errors
            or [
                {
                    "file": "connectors/",
                    "code": "missing_directory",
                    "message": "connectors/ directory does not exist",
                }
            ],
        }
        print(json.dumps(result, indent=2, sort_keys=True))
        return 1

    manifest_paths = sorted(connectors_dir.glob("*.connector.json"))
    if not manifest_paths:
        result = {
            "ok": False,
            "checked": [],
            "errors": [
                {
                    "file": "connectors/",
                    "code": "no_manifests",
                    "message": "no *.connector.json files found in connectors/",
                }
            ],
        }
        print(json.dumps(result, indent=2, sort_keys=True))
        return 1

    for path in manifest_paths:
        rel_path = str(path.relative_to(root))
        checked.append(rel_path)
        data, parse_error = load_json(path)
        if parse_error is not None:
            add_error(errors, rel_path, "parse_error", parse_error)
            continue
        validate_manifest(rel_path, data, errors)
        if isinstance(data, dict):
            manifests.append(data)

    alias_to_id = collect_doc_claim_aliases(manifests)
    scan_doc_claims(root, alias_to_id, manifests_by_id(manifests), errors)

    ok = len(errors) == 0
    result = {
        "ok": ok,
        "checked": checked,
        "errors": errors,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
