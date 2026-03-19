from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

_REQUIRED_FIELDS = {
    "id",
    "title",
    "applies_to_intents",
    "trigger_terms",
    "rule",
    "why",
    "confidence",
    "status",
    "source_task_ids",
    "evidence_paths",
    "last_validated_at",
}
_SECRET_PATH_MARKERS = (
    "infra/local/sovereign",
    ".codex",
    ".gemini",
    ".claude",
    ".ssh",
    ".env",
    "credentials",
    "token",
    "secret",
)


@dataclass(slots=True)
class InstinctEntry:
    id: str
    title: str
    applies_to_intents: list[str]
    trigger_terms: list[str]
    rule: str
    why: str
    confidence: int
    status: str
    source_task_ids: list[str]
    evidence_paths: list[str]
    last_validated_at: str
    body: str = ""
    path: str = ""
    rule_hash: str = ""


class KnowledgeRegistry:
    """Repo-authored procedural knowledge mirrored into STDB for retrieval."""

    def __init__(self, root_dir: Path | None = None, stdb: Any | None = None) -> None:
        self.root = (root_dir or Path(__file__).resolve().parents[3]).resolve()
        self.entries_dir = self.root / "packages" / "heiwa_knowledge" / "entries"
        self.instincts_path = self.root / "config" / "identities" / "persona" / "instincts.md"
        self.stdb = stdb

    def load_entries(self) -> list[InstinctEntry]:
        entries: list[InstinctEntry] = []
        if not self.entries_dir.exists():
            return entries
        for path in sorted(self.entries_dir.glob("*.md")):
            if path.name.lower() == "readme.md":
                continue
            entries.append(self.parse_entry(path))
        return entries

    def parse_entry(self, path: Path) -> InstinctEntry:
        text = path.read_text(encoding="utf-8")
        metadata, body = self._split_frontmatter(text)
        entry = self._entry_from_metadata(metadata, body=body, path=str(path.relative_to(self.root)))
        entry.rule_hash = self.rule_hash(entry.rule)
        return entry

    def upsert_candidate(self, candidate: dict[str, Any]) -> InstinctEntry:
        self.entries_dir.mkdir(parents=True, exist_ok=True)
        existing = {entry.rule_hash: entry for entry in self.load_entries()}
        candidate_rule = str(candidate.get("rule") or "").strip()
        candidate_hash = self.rule_hash(candidate_rule)
        existing_entry = existing.get(candidate_hash)

        merged = dict(candidate)
        merged["rule"] = candidate_rule
        merged["trigger_terms"] = self._normalize_list(candidate.get("trigger_terms"))
        merged["applies_to_intents"] = self._normalize_list(candidate.get("applies_to_intents"))
        merged["source_task_ids"] = self._normalize_list(candidate.get("source_task_ids"))
        merged["evidence_paths"] = self._normalize_list(candidate.get("evidence_paths"), lower=False)
        merged["status"] = str(candidate.get("status") or "active").strip().lower() or "active"
        merged["confidence"] = max(0, min(100, int(candidate.get("confidence") or 0)))

        if existing_entry:
            merged["id"] = existing_entry.id
            merged["title"] = str(candidate.get("title") or existing_entry.title).strip() or existing_entry.title
            merged["rule"] = existing_entry.rule
            merged["why"] = str(candidate.get("why") or existing_entry.why).strip() or existing_entry.why
            merged["status"] = existing_entry.status if existing_entry.status == "retired" else merged["status"]
            merged["confidence"] = max(existing_entry.confidence, merged["confidence"])
            path = self.root / existing_entry.path
        else:
            merged["id"] = str(candidate.get("id") or f"instinct-{candidate_hash[:12]}")
            merged["title"] = str(candidate.get("title") or "Heiwa Procedural Instinct").strip() or "Heiwa Procedural Instinct"
            merged["why"] = str(candidate.get("why") or "").strip()
            path = self.entries_dir / f"{merged['id']}.md"

        entry = self._entry_from_metadata(merged, body=str(candidate.get("body") or "").strip(), path=str(path.relative_to(self.root)))
        entry.rule_hash = candidate_hash
        path.write_text(self._render_entry(entry), encoding="utf-8")
        self.write_instincts_digest()
        return entry

    def write_instincts_digest(self) -> None:
        entries = [entry for entry in self.load_entries() if entry.status == "active"]
        self.instincts_path.parent.mkdir(parents=True, exist_ok=True)
        lines = ["# Heiwa Instincts", "", "Compact digest of repo-authored procedural knowledge.", ""]
        if not entries:
            lines.append("- No active instincts recorded yet.")
        else:
            for entry in sorted(entries, key=lambda item: (-int(item.confidence), item.title.lower())):
                intents = ", ".join(entry.applies_to_intents)
                lines.append(f"- `{entry.id}`: {entry.rule} ({intents}; {entry.path})")
        self.instincts_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    @staticmethod
    def rule_hash(rule: str) -> str:
        normalized = re.sub(r"\s+", " ", str(rule or "").strip().lower())
        normalized = re.sub(r"[^a-z0-9\s:/_-]", "", normalized)
        return hashlib.sha256(normalized.encode("utf-8")).hexdigest()

    @staticmethod
    def _is_secret_path(value: str) -> bool:
        lowered = str(value or "").strip().lower()
        return any(marker in lowered for marker in _SECRET_PATH_MARKERS)

    @staticmethod
    def _normalize_list(value: Any, *, lower: bool = True) -> list[str]:
        if not isinstance(value, list):
            return []
        out: list[str] = []
        for item in value:
            text = str(item or "").strip()
            if not text:
                continue
            out.append(text.lower() if lower else text)
        return out

    @staticmethod
    def _split_frontmatter(text: str) -> tuple[dict[str, Any], str]:
        if not text.startswith("---\n"):
            raise ValueError("missing YAML frontmatter")
        _, remainder = text.split("---\n", 1)
        frontmatter, body = remainder.split("\n---\n", 1)
        metadata = yaml.safe_load(frontmatter) or {}
        if not isinstance(metadata, dict):
            raise ValueError("frontmatter must parse to an object")
        return metadata, body.strip()

    def _entry_from_metadata(self, metadata: dict[str, Any], *, body: str, path: str) -> InstinctEntry:
        missing = sorted(_REQUIRED_FIELDS - set(metadata))
        if missing:
            raise ValueError(f"missing required fields: {', '.join(missing)}")
        return InstinctEntry(
            id=str(metadata.get("id") or "").strip(),
            title=str(metadata.get("title") or "").strip(),
            applies_to_intents=self._normalize_list(metadata.get("applies_to_intents")),
            trigger_terms=self._normalize_list(metadata.get("trigger_terms")),
            rule=str(metadata.get("rule") or "").strip(),
            why=str(metadata.get("why") or "").strip(),
            confidence=int(metadata.get("confidence") or 0),
            status=str(metadata.get("status") or "").strip().lower(),
            source_task_ids=self._normalize_list(metadata.get("source_task_ids")),
            evidence_paths=self._normalize_list(metadata.get("evidence_paths"), lower=False),
            last_validated_at=str(metadata.get("last_validated_at") or "").strip(),
            body=body,
            path=path,
        )

    def _render_entry(self, entry: InstinctEntry) -> str:
        metadata = {
            "id": entry.id,
            "title": entry.title,
            "applies_to_intents": entry.applies_to_intents,
            "trigger_terms": entry.trigger_terms,
            "rule": entry.rule,
            "why": entry.why,
            "confidence": entry.confidence,
            "status": entry.status,
            "source_task_ids": entry.source_task_ids,
            "evidence_paths": entry.evidence_paths,
            "last_validated_at": entry.last_validated_at,
        }
        frontmatter = yaml.safe_dump(metadata, sort_keys=False).strip()
        return f"---\n{frontmatter}\n---\n\n{entry.body.strip()}\n"
