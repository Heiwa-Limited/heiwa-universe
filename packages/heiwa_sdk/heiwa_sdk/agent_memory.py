"""Heiwa Agent Memory: persistent conversation memory backed by SpacetimeDB."""
from __future__ import annotations

import logging
import time
import uuid
from typing import Any

from heiwa_sdk.spacetimedb import SpacetimeDB

logger = logging.getLogger("SDK.AgentMemory")

# Complexity detection keywords
_COMPLEX_KEYWORDS = frozenset({
    "design", "refactor", "deploy", "strategy", "architecture",
    "migrate", "redesign", "overhaul", "integrate", "think hard",
})

_CHAR_LENGTH_COMPLEX = 200


class AgentMemory:
    """High-level memory service for Heiwa Agent conversations.

    Wraps STDB captain_messages/captain_summaries/captain_focus tables
    with token budgeting, compression detection, and context assembly.
    """

    def __init__(self, stdb: SpacetimeDB, session_id: str | None = None, token_budget: int = 8000):
        self.stdb = stdb
        self.session_id = session_id or str(uuid.uuid4())
        self._token_budget = token_budget

    @staticmethod
    def estimate_tokens(text: str) -> int:
        """Approximate token count: len(text) // 4."""
        return len(text) // 4

    def store_message(self, role: str, content: str, source: str = "discord_dm") -> bool:
        """Store a raw message in captain_messages."""
        return self.stdb.insert_captain_message(
            message_id=str(uuid.uuid4()),
            session_id=self.session_id,
            role=role,
            content=content,
            timestamp=int(time.time() * 1000),
            source=source,
        )

    def load_context_window(self) -> dict[str, Any]:
        """Build the active context window for LLM input."""
        messages = self.stdb.get_uncompressed_messages(limit=100)
        focuses = self.stdb.get_active_focuses()
        summaries = self.stdb.get_recent_summaries(limit=3)
        return {
            "messages": messages,
            "focuses": focuses,
            "summaries": summaries,
        }

    def needs_compression(self, messages: list[dict[str, Any]]) -> bool:
        """Check if uncompressed messages exceed the token budget (~32K chars = ~8K tokens)."""
        total_chars = sum(len(m.get("content", "")) for m in messages)
        return (total_chars // 4) > self._token_budget

    def detect_complexity(self, text: str) -> bool:
        """Detect if a message warrants model escalation."""
        if len(text) > _CHAR_LENGTH_COMPLEX:
            return True
        text_lower = text.lower()
        matches = sum(1 for kw in _COMPLEX_KEYWORDS if kw in text_lower)
        if matches >= 2:
            return True
        question_count = text.count("?")
        if question_count >= 3:
            return True
        return False

    def store_summary(
        self,
        summary_type: str,
        content: str,
        range_start: int,
        range_end: int,
        messages_compressed: int,
    ) -> bool:
        """Store a compression summary."""
        return self.stdb.insert_captain_summary(
            summary_id=str(uuid.uuid4()),
            summary_type=summary_type,
            content=content,
            range_start=range_start,
            range_end=range_end,
            messages_compressed=messages_compressed,
        )

    def mark_compressed(self, before_timestamp: int) -> bool:
        """Mark messages as compressed after summary is stored."""
        return self.stdb.mark_messages_compressed(
            session_id=self.session_id,
            before_timestamp=before_timestamp,
        )

    def upsert_focus(self, topic: str, context: dict[str, Any], priority: int = 3, focus_id: str | None = None) -> str:
        """Create or update a focus tracking entry."""
        import json
        focus_id = focus_id or str(uuid.uuid4())
        self.stdb.upsert_captain_focus(
            focus_id=focus_id,
            topic=topic,
            context_json=json.dumps(context),
            priority=priority,
        )
        return focus_id

    def resolve_focus(self, focus_id: str) -> bool:
        """Mark a focus entry as resolved."""
        return self.stdb.resolve_captain_focus(
            focus_id=focus_id,
            resolved_at=int(time.time() * 1000),
        )
