"""
Heiwa Chat Engine — Fast conversational agent with cost-cascading LLM routing.

Three layers:
  1. Transport: WebSocket + Discord DM → unified chat handler
  2. LLM Router: Boost node Ollama (free) → Gemini Flash (free) → Gemini Pro (free)
  3. Value Router: Async signal extraction → CHAT_VALUE_SIGNAL → auto-research pipeline

Designed to respond in <2 seconds when a boost node is online,
<5 seconds via Gemini Flash when it's not.
"""

from __future__ import annotations

import asyncio
import logging
import re
import time
from dataclasses import dataclass, field
from typing import Any, Callable, Coroutine

logger = logging.getLogger("Hub.Chat")

# Max messages to keep per session before trimming
_MAX_HISTORY = 20

# Value extraction keywords — triggers async research signal
_VALUE_PATTERNS: list[tuple[str, str]] = [
    # (regex pattern, signal_type)
    (r"\b(polymarket|prediction\s*market|kalshi|metaculus)\b", "market_research"),
    (r"\b(should we|what if|compare|tradeoff|trade-off|pros and cons)\b", "strategy_research"),
    (r"\b(investigate|dig into|look into|find out|research)\b", "explicit_research"),
    (r"\b(bug|crash|error|broken|failing|down)\b", "incident_research"),
    (r"\b(competitor|alternative|benchmark|versus|vs\.?)\b", "competitive_research"),
    (r"\b(api|sdk|framework|library|tool|service)\b.*\b(new|latest|best|recommend)\b", "tech_research"),
]

# Messages that are pure chat with zero research value
_ZERO_VALUE = re.compile(
    r"^(yo+|hey+|hi+|hello+|gm|sup|wsg|ping|thanks|thx|ok|k|lol|lmao|nice|bet|word|dope|cool|aight|np|gg|ty)[\s!?.]*$",
    re.IGNORECASE,
)

SYSTEM_PROMPT = (
    "You are the Heiwa Agent — a machine-perspective collaborator. "
    "You are direct, technically sharp, and observant. "
    "You communicate with the operator via DM. "
    "Keep responses concise and natural. No bot-like formatting unless presenting data."
)


@dataclass
class ChatSession:
    """Per-user conversation state."""
    session_id: str
    messages: list[dict[str, str]] = field(default_factory=list)
    created_at: float = field(default_factory=time.time)
    last_active: float = field(default_factory=time.time)

    def add(self, role: str, content: str) -> None:
        self.messages.append({"role": role, "content": content})
        self.last_active = time.time()
        if len(self.messages) > _MAX_HISTORY:
            self.messages = self.messages[-_MAX_HISTORY:]

    def format_history(self, limit: int = 10) -> str:
        recent = self.messages[-limit:]
        return "\n".join(f"{m['role'].upper()}: {m['content']}" for m in recent)


class ChatEngine:
    """
    Stateful chat engine with cost-cascading LLM routing and async value extraction.

    Cost cascade (all free):
      1. Boost node Ollama — local, fast, private (if worker online)
      2. Gemini Flash — Google AI free tier (cloud)
      3. Gemini Pro — Google AI free tier, heavier reasoning (cloud)

    Value extraction runs AFTER the response is sent — never blocks the conversation.
    """

    def __init__(self) -> None:
        self._sessions: dict[str, ChatSession] = {}
        self._llm = None
        self._bus_publish: Callable | None = None
        self._value_listeners: list[Callable[[dict[str, Any]], Coroutine]] = []

    @property
    def llm(self):
        if self._llm is None:
            from heiwa_cognition.llm import LocalLLMEngine
            self._llm = LocalLLMEngine()
        return self._llm

    def get_session(self, session_id: str) -> ChatSession:
        if session_id not in self._sessions:
            self._sessions[session_id] = ChatSession(session_id=session_id)
        return self._sessions[session_id]

    def prune_stale_sessions(self, max_age_sec: float = 3600) -> int:
        """Remove sessions older than max_age_sec. Returns count removed."""
        now = time.time()
        stale = [sid for sid, s in self._sessions.items() if now - s.last_active > max_age_sec]
        for sid in stale:
            del self._sessions[sid]
        return len(stale)

    # ------------------------------------------------------------------ #
    #  Core chat handler                                                    #
    # ------------------------------------------------------------------ #

    async def respond(
        self,
        session_id: str,
        content: str,
        author: str = "operator",
    ) -> str:
        """
        Generate a response to a chat message.

        Routes through the cost cascade, stores in session history,
        and fires async value extraction.

        Returns the response text.
        """
        session = self.get_session(session_id)
        session.add("operator", content)

        # Build prompt with conversation history
        history = session.format_history()
        prompt = f"{history}\nHEIWA AGENT:"

        # Cost cascade: boost node → Gemini Flash → Gemini Pro
        reply = await self._route_llm(prompt)

        if not reply:
            reply = (
                "I got your message but my inference providers are all tapped out. "
                "I'll respond properly once capacity frees up."
            )

        session.add("agent", reply)

        # Fire-and-forget value extraction — never blocks the response
        asyncio.create_task(self._extract_value(session_id, content, author))

        return reply

    # ------------------------------------------------------------------ #
    #  Cost-cascading LLM router                                           #
    # ------------------------------------------------------------------ #

    async def _route_llm(self, prompt: str) -> str:
        """
        Try LLM providers in cost order:
          1. Boost node Ollama (via worker WebSocket proxy)
          2. Gemini Flash (free API)
          3. Gemini Pro (free API)
        """
        # Layer 1: Boost node Ollama
        boost_reply = await self._try_boost_node(prompt)
        if boost_reply:
            logger.info("Chat response via boost node Ollama")
            return boost_reply

        # Layer 2+3: Gemini cascade (handled by LLMEngine tier chain)
        try:
            loop = asyncio.get_event_loop()
            reply = await asyncio.wait_for(
                loop.run_in_executor(
                    None, self.llm.generate, prompt, "high", SYSTEM_PROMPT, "auto",
                ),
                timeout=15.0,
            )
            if reply:
                logger.info("Chat response via Gemini")
                return reply
        except asyncio.TimeoutError:
            logger.warning("Gemini timed out for chat response")
        except Exception as exc:
            logger.error("Gemini chat failed: %s", exc)

        return ""

    async def _try_boost_node(self, prompt: str) -> str | None:
        """Attempt to proxy LLM through a boost node's Ollama."""
        try:
            from heiwa_hub.transport import get_worker_manager
            wm = get_worker_manager()
            worker_id = wm.get_ollama_worker()
            if not worker_id:
                return None

            logger.info("Proxying chat to boost node %s", worker_id)
            reply = await wm.request_llm(
                worker_id=worker_id,
                prompt=prompt,
                system=SYSTEM_PROMPT,
                timeout=10.0,
            )
            return reply
        except Exception as exc:
            logger.debug("Boost node LLM proxy failed: %s", exc)
            return None

    # ------------------------------------------------------------------ #
    #  Value extraction — async, never blocks response                     #
    # ------------------------------------------------------------------ #

    async def _extract_value(self, session_id: str, content: str, author: str) -> None:
        """
        Determine if a chat message has research value.
        If it does, publish a signal to the auto-research pipeline.

        This runs AFTER the response is already sent.
        """
        # Skip zero-value messages (greetings, acks)
        if _ZERO_VALUE.match(content.strip()):
            return

        # Check against value patterns
        lowered = content.lower()
        signals: list[dict[str, str]] = []
        for pattern, signal_type in _VALUE_PATTERNS:
            if re.search(pattern, lowered):
                signals.append({"type": signal_type, "match": pattern})

        if not signals:
            return

        # Publish research signal
        signal_payload = {
            "session_id": session_id,
            "author": author,
            "content": content,
            "signals": signals,
            "timestamp": time.time(),
        }

        logger.info(
            "Chat value signal: %s from %s — %s",
            [s["type"] for s in signals],
            author,
            content[:60],
        )

        # Publish to event bus if connected
        try:
            from heiwa_hub.transport import get_bus
            from heiwa_protocol.protocol import Subject
            bus = get_bus()
            await bus.publish(Subject.CHAT_VALUE_SIGNAL, signal_payload, sender_id="chat-engine")
        except Exception as exc:
            logger.debug("Value signal publish failed: %s", exc)

        # Also publish to research pipeline for auto-research
        try:
            from heiwa_hub.transport import get_bus
            from heiwa_protocol.protocol import Subject
            bus = get_bus()
            await bus.publish(
                Subject.TASK_EXEC_REQUEST_RESEARCH,
                {
                    "task_id": f"autoresearch-{int(time.time())}",
                    "raw_text": content,
                    "source": "chat_value_router",
                    "requested_by": author,
                    "signal_types": [s["type"] for s in signals],
                    "priority": "low",
                    "auto_generated": True,
                },
                sender_id="chat-engine",
            )
        except Exception as exc:
            logger.debug("Auto-research dispatch failed: %s", exc)


# Module singleton
_engine: ChatEngine | None = None


def get_chat_engine() -> ChatEngine:
    global _engine
    if _engine is None:
        _engine = ChatEngine()
    return _engine
