from __future__ import annotations

import asyncio
import logging
from pathlib import Path
from typing import Any

from heiwa_hub.agents.base import BaseAgent
from heiwa_protocol.protocol import Subject
from heiwa_knowledge.learning import extract_instinct_candidates, should_learn_from_event
from heiwa_knowledge.registry import KnowledgeRegistry
from heiwa_sdk.db import Database
from heiwa_sdk.memory import MemoryService

logger = logging.getLogger("Heiwa.Learning")


class LearningAgent(BaseAgent):
    """Asynchronously converts successful Heiwa work into procedural knowledge."""

    def __init__(
        self,
        root_dir: Path | None = None,
        *,
        registry: KnowledgeRegistry | None = None,
        memory_service: MemoryService | None = None,
    ) -> None:
        super().__init__(name="heiwa-learning")
        self.root = (root_dir or Path(__file__).resolve().parents[3]).resolve()
        self.db = None
        if registry is not None:
            self.registry = registry
        else:
            stdb = None
            try:
                self.db = Database()
                stdb = self.db.stdb
            except Exception:
                logger.debug("LearningAgent database bootstrap unavailable.", exc_info=True)
            self.registry = KnowledgeRegistry(root_dir=self.root, stdb=stdb)
        self.memory = memory_service
        if self.memory is None and self.db and self.db.stdb:
            try:
                self.memory = MemoryService(stdb=self.db.stdb)
            except Exception:
                logger.debug("LearningAgent memory indexing unavailable.", exc_info=True)

    async def run(self):
        await self.start()
        await self.listen(Subject.KNOWLEDGE_LEARN, self._handle_learning_event)
        logger.info("[%s] Learning lane active.", self.name)
        while self.running:
            await asyncio.sleep(1)

    async def _handle_learning_event(self, data: dict[str, Any]) -> None:
        payload = data.get("data", data)
        if not should_learn_from_event(payload):
            return

        candidates = extract_instinct_candidates(payload)
        if not candidates:
            return

        created: list[str] = []
        for candidate in candidates:
            try:
                entry = self.registry.upsert_candidate(candidate)
                await self._mirror_entry(entry)
                created.append(entry.path)
            except Exception as exc:
                logger.warning("Skipping learning candidate for %s: %s", payload.get("task_id"), exc)

        if not created:
            return

        await self.speak(
            Subject.LOG_INFO,
            {
                "agent": self.name,
                "status": "PASS",
                "intent_class": "self_buff",
                "task_id": payload.get("task_id"),
                "content": (
                    "Knowledge substrate updated from successful work:\n"
                    + "\n".join(f"- {path}" for path in created)
                )[:1800],
            },
        )

    async def _mirror_entry(self, entry) -> None:
        if not self.memory:
            return
        try:
            path = self.root / entry.path
            content = path.read_text(encoding="utf-8")
            await self.memory.index_file(path.relative_to(self.root).as_posix(), content, source_type="knowledge_entry")
        except Exception:
            logger.debug("Knowledge mirror failed for %s", getattr(entry, "path", "<unknown>"), exc_info=True)
