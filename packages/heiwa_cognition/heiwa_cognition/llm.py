"""
Heiwa LLM Engine — API Inference Router

Used for lightweight tasks (enrichment, classification, chat) on Railway.
Class 3 agentic sessions (Claude Code, Gemini CLI, Codex) are handled by
ToolMesh/HeiwaClaw, not this engine.

Tier 1: Gemini Flash  (Google AI Studio — free)
Tier 2: Gemini Pro    (Google AI Studio — free, heavy reasoning)
Tier 3: Ollama        (boost node only — when MacBook/WSL online)

No paid API tiers. All inference is subscription-included or free.
"""
from __future__ import annotations

import json
import logging
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional

import requests
from tenacity import (
    retry,
    stop_after_attempt,
    wait_exponential,
    retry_if_exception_type,
)

try:
    from heiwa_sdk.heiwa_net import HeiwaNetProxy
    _NET_PROXY = HeiwaNetProxy(origin_surface="runtime", agent_id="llm-engine")
except ImportError:
    _NET_PROXY = None

try:
    from heiwa_sdk.rate_ledger import get_rate_ledger
    _RATE_LEDGER = get_rate_ledger()
except ImportError:
    _RATE_LEDGER = None

PROJECT_ROOT = Path(__file__).resolve().parents[3]
logger = logging.getLogger("LLMEngine")

_ENGINE_CACHE: dict[str, LocalLLMEngine] = {}


def get_llm_engine(stdb: Any | None = None) -> LocalLLMEngine:
    """Get or create a cached LocalLLMEngine instance."""
    # Using string representation of stdb's memory address as key if it exists,
    # otherwise using 'default'.
    cache_key = f"stdb_{id(stdb)}" if stdb is not None else "default"
    if cache_key not in _ENGINE_CACHE:
        _ENGINE_CACHE[cache_key] = LocalLLMEngine(stdb=stdb)
    return _ENGINE_CACHE[cache_key]


class LLMPolicyError(RuntimeError):
    """Raised when runtime config violates policy."""


@dataclass
class LLMResult:
    text: str
    provider: str
    model: str
    tier: int


class LocalLLMEngine:
    """
    Tiered multi-provider LLM engine.

    Routes requests through the cheapest viable provider first,
    escalating only when lower tiers are unavailable or the task
    demands higher capability.
    """

    def __init__(self, stdb: Any | None = None) -> None:
        self._stdb = stdb
        self.host_runtime = self._detect_host_runtime()

        # --- Ollama (Tier 1: Free, local inference) ---
        self.ollama_url = os.getenv(
            "HEIWA_OLLAMA_URL", "http://127.0.0.1:11434"
        ).rstrip("/")
        self.ollama_model = os.getenv("HEIWA_OLLAMA_MODEL", "qwen3.5:4b")
        self.ollama_timeout = float(os.getenv("HEIWA_OLLAMA_TIMEOUT_SEC", "60"))
        self.ollama_enabled_env = os.getenv("HEIWA_ENABLE_OLLAMA", "true").strip().lower() == "true"
        self.ollama_allowed_by_runtime = self._runtime_allows_ollama(self.host_runtime)
        self.ollama_enabled = self.ollama_enabled_env and self.ollama_allowed_by_runtime
        if self.ollama_enabled_env and not self.ollama_allowed_by_runtime:
            logger.warning(
                "Ollama disabled by runtime policy (host_runtime=%s). "
                "Railway/cloud executors must use remote providers (e.g., Gemini via Google API key).",
                self.host_runtime,
            )

        # --- Gemini (Tier 2/3: Google AI Pro plan) ---
        self.gemini_key = os.getenv("GEMINI_API_KEY")
        self.gemini_flash_model = os.getenv(
            "HEIWA_GEMINI_FLASH_MODEL", "gemini-2.5-flash"
        )
        self.gemini_pro_model = os.getenv(
            "HEIWA_GEMINI_PRO_MODEL", "gemini-2.5-pro"
        )
        self.gemini_timeout = float(os.getenv("HEIWA_GEMINI_TIMEOUT_SEC", "15"))

        # --- Rate limiting (optional Redis) ---
        self._redis = None
        redis_url = os.getenv("REDIS_URL")
        if redis_url:
            try:
                import redis
                self._redis = redis.Redis.from_url(redis_url, decode_responses=True)
                self._redis.ping()
            except Exception:
                logger.warning("Redis unavailable — rate limits will not be tracked.")
                self._redis = None

        # --- Provider Registry (cached) ---
        self._provider_registry = None
        try:
            from heiwa_sdk.provider_registry import ProviderRegistry
            self._provider_registry = ProviderRegistry(root_dir=PROJECT_ROOT)
        except Exception as exc:
            logger.warning("Provider registry unavailable: %s", exc)

        logger.info(
            "LLMEngine initialized | host_runtime=%s ollama=%s gemini=%s",
            self.host_runtime,
            "ON" if self._ollama_available(runtime=self.host_runtime) else "OFF",
            "ON" if self.gemini_key else "OFF",
        )

    def _resolve_api_key(self, provider_id: str, owner_id: str = "operator") -> str | None:
        """Resolve API key for a provider, scoped to an owner."""
        # 1. User-scoped credential from STDB
        if owner_id != "operator" and self._stdb:
            try:
                from heiwa_sdk.vault import UserVault
                vault = UserVault(self._stdb)
                key = vault.resolve_credential(owner_id, provider_id)
                if key:
                    return key
            except Exception as e:
                logger.debug("Failed to resolve user credential for %s: %s", provider_id, e)

        # 2. Operator fallback (Environment variables)
        if provider_id == "google":
            return self.gemini_key
        if provider_id == "anthropic":
            return os.getenv("ANTHROPIC_API_KEY")
        if provider_id == "openai":
            return os.getenv("OPENAI_API_KEY")

        return None

    # ------------------------------------------------------------------ #
    #  Availability checks                                                 #
    # ------------------------------------------------------------------ #

    @staticmethod
    def _detect_host_runtime() -> str:
        explicit = str(os.getenv("HEIWA_EXECUTOR_RUNTIME", "")).strip().lower()
        if explicit:
            return explicit
        if os.getenv("RAILWAY_ENVIRONMENT") or os.getenv("RAILWAY_ENVIRONMENT_NAME") or os.getenv("RAILWAY_PROJECT_ID"):
            return "railway"
        if os.getenv("HEIWA_LLM_MODE", "").strip().lower() == "local_only":
            return "macbook"
        return "auto"

    @staticmethod
    def _normalize_runtime(runtime: str | None) -> str:
        value = str(runtime or "auto").strip().lower()
        return value or "auto"

    @staticmethod
    def _runtime_allows_ollama(runtime: str | None) -> bool:
        value = LocalLLMEngine._normalize_runtime(runtime)
        return value not in {"railway", "cloud"}

    def _effective_runtime(self, runtime: str = "auto") -> str:
        value = self._normalize_runtime(runtime)
        return self.host_runtime if value == "auto" else value

    def _ollama_available(self, runtime: str = "auto") -> bool:
        effective_runtime = self._effective_runtime(runtime)
        if not self.ollama_enabled:
            return False
        if not self._runtime_allows_ollama(effective_runtime):
            return False
        try:
            if _NET_PROXY:
                resp = _NET_PROXY.get(
                    f"{self.ollama_url}/api/tags",
                    purpose="ollama availability check",
                    purpose_class="health_check",
                    timeout=int(self.ollama_timeout),
                )
            else:
                resp = requests.get(
                    f"{self.ollama_url}/api/tags", timeout=self.ollama_timeout
                )
            return resp.status_code == 200
        except (requests.RequestException, PermissionError):
            return False

    def is_available(self, runtime: str = "auto") -> bool:
        """Returns True if at least one provider is reachable."""
        if self.gemini_key:
            return True
        if self._ollama_available(runtime=runtime):
            return True
        return False

    # ------------------------------------------------------------------ #
    #  Provider calls                                                      #
    # ------------------------------------------------------------------ #

    def _call_ollama(
        self, prompt: str, system: Optional[str] = None
    ) -> LLMResult:
        payload: dict[str, Any] = {
            "model": self.ollama_model,
            "prompt": prompt,
            "stream": False,
        }
        if system:
            payload["system"] = system

        if _NET_PROXY:
            resp = _NET_PROXY.post(
                f"{self.ollama_url}/api/generate",
                purpose="ollama inference",
                purpose_class="model_inference",
                json=payload,
                timeout=int(self.ollama_timeout),
            )
        else:
            resp = requests.post(
                f"{self.ollama_url}/api/generate",
                json=payload,
                timeout=self.ollama_timeout,
            )
        resp.raise_for_status()
        data = resp.json()
        text = str(data.get("response") or "").strip()
        return LLMResult(
            text=text, provider="ollama-local-http", model=self.ollama_model, tier=1
        )

    _GEMINI_RATE_GROUP = "google_gemini_api"

    @retry(
        stop=stop_after_attempt(3),
        wait=wait_exponential(multiplier=2, min=2, max=30),
        retry=retry_if_exception_type(requests.exceptions.HTTPError),
    )
    def _call_gemini(
        self,
        prompt: str,
        model_name: str,
        tier: int,
        system: Optional[str] = None,
        api_key: Optional[str] = None,
    ) -> LLMResult:
        # Check rate ledger before calling
        if _RATE_LEDGER and not _RATE_LEDGER.has_capacity(self._GEMINI_RATE_GROUP):
            logger.warning("Gemini rate-limited by ledger — skipping %s call", model_name)
            return LLMResult(text="", provider="gemini", model=model_name, tier=tier)

        key = api_key or self.gemini_key
        if not key:
            logger.warning("No API key available for Gemini call")
            return LLMResult(text="", provider="gemini", model=model_name, tier=tier)

        url = (
            f"https://generativelanguage.googleapis.com/v1beta/models/"
            f"{model_name}:generateContent?key={key}"
        )
        payload: dict[str, Any] = {
            "contents": [{"parts": [{"text": prompt}]}],
            "generationConfig": {"temperature": 0.2},
        }
        if system:
            payload["systemInstruction"] = {"parts": [{"text": system}]}

        if _NET_PROXY:
            resp = _NET_PROXY.post(
                url, purpose=f"gemini {model_name} inference",
                purpose_class="model_inference", json=payload,
                timeout=int(self.gemini_timeout),
            )
        else:
            resp = requests.post(url, json=payload, timeout=self.gemini_timeout)

        if resp.status_code == 429:
            logger.warning("Gemini 429 on %s — recording throttle and backing off", model_name)
            if _RATE_LEDGER:
                _RATE_LEDGER.record_throttle(self._GEMINI_RATE_GROUP)
            resp.raise_for_status()
        resp.raise_for_status()

        # Record successful call
        if _RATE_LEDGER:
            _RATE_LEDGER.record(self._GEMINI_RATE_GROUP)

        data = resp.json()
        text = ""
        if "candidates" in data and data["candidates"]:
            text = str(
                data["candidates"][0]["content"]["parts"][0]["text"]
            ).strip()
        return LLMResult(text=text, provider="gemini", model=model_name, tier=tier)

    def _call_cli_tool(self, tool: str, prompt: str, system: Optional[str] = None) -> LLMResult:
        """Invoke a CLI tool (gemini/claude) synchronously for inference fallback.

        These use separate OAuth rate groups from the API, so they're
        available even when the API tier is exhausted.
        """
        import subprocess
        import shutil

        rate_group = {"gemini": "google_gemini_cli", "claude": "claude_code"}.get(tool)
        if _RATE_LEDGER and rate_group and not _RATE_LEDGER.has_capacity(rate_group):
            logger.warning("CLI tool %s rate-limited by ledger — skipping", tool)
            return LLMResult(text="", provider=f"{tool}-cli", model=tool, tier=3)

        binary = shutil.which(tool)
        if not binary:
            return LLMResult(text="", provider=f"{tool}-cli", model=tool, tier=3)

        full_prompt = f"{system}\n\n{prompt}" if system else prompt

        if tool == "gemini":
            cmd = [binary, "--prompt", full_prompt, "--output-format", "text"]
        elif tool == "claude":
            cmd = [binary, "-p", full_prompt, "--output-format", "text"]
        else:
            return LLMResult(text="", provider=f"{tool}-cli", model=tool, tier=3)

        try:
            result = subprocess.run(
                cmd, capture_output=True, text=True, timeout=90, cwd=str(PROJECT_ROOT)
            )
            text = result.stdout.strip()
            if result.returncode == 0 and text:
                if _RATE_LEDGER and rate_group:
                    _RATE_LEDGER.record(rate_group)
                return LLMResult(text=text, provider=f"{tool}-cli", model=tool, tier=3)
            logger.warning("CLI tool %s returned code %d", tool, result.returncode)
        except subprocess.TimeoutExpired:
            logger.warning("CLI tool %s timed out", tool)
        except Exception as e:
            logger.warning("CLI tool %s failed: %s", tool, e)
        return LLMResult(text="", provider=f"{tool}-cli", model=tool, tier=3)

    def execute(self, target: Any, prompt: str, system: Optional[str] = None, owner_id: str = "operator") -> str:
        """Execute one routed inference target."""
        if not self._provider_registry:
            logger.warning("Provider registry unavailable")
            return ""

        provider_id = str(getattr(target, "provider", "") or "")
        provider_cfg = self._provider_registry.resolve(provider_id)
        transport = str(getattr(target, "transport", "") or provider_cfg.transport or "").strip().lower()
        model_id = str(getattr(target, "provider_model_id", "") or getattr(target, "model_id", "") or "").strip()

        # Resolve API key for this provider and owner
        api_key = self._resolve_api_key(provider_id, owner_id)

        try:
            if transport == "local_http":
                return self._call_ollama(prompt, system).text
            if transport == "cli_stdio":
                command = provider_cfg.cli_command or provider_cfg.adapter_tool
                tool = command.split()[0].strip() if command else provider_cfg.adapter_tool
                return self._call_cli_tool(tool, prompt, system).text
            if "gemini" in provider_cfg.name or "gemini" in model_id:
                return self._call_gemini(
                    prompt,
                    model_id or self.gemini_flash_model,
                    tier=int(getattr(target, "capability_class", 1) or 1),
                    system=system,
                    api_key=api_key,
                ).text
            if provider_cfg.direct_execution and provider_cfg.adapter_tool:
                return self._call_cli_tool(provider_cfg.adapter_tool, prompt, system).text
            return self._call_cli_tool(provider_cfg.adapter_tool or provider_cfg.name, prompt, system).text
        except Exception as exc:
            logger.warning("Target execution failed for %s: %s", provider_cfg.name, exc)
            return ""

    def generate(
        self,
        prompt: str,
        intent: str = "general",
        risk: str = "low",
        *,
        owner_id: str = "operator",
        privacy: str | None = None,
        runtime: str | None = None,
        system: str | None = None,
    ) -> InferenceResult:
        """High-level entry point for routed inference."""
        from heiwa_cognition.router import ComputeRouter, InferenceResult

        router = ComputeRouter(stdb=self._stdb)
        plan = router.route_inference(
            intent=intent,
            risk=risk,
            privacy=privacy,
            runtime=runtime,
            owner_id=owner_id,
        )

        attempts = 0
        seen: set[str] = set()
        for target in [plan.primary, *plan.fallbacks]:
            attempts += 1
            seen.add(target.model_id)
            text = self.execute(target, prompt, system=system, owner_id=owner_id)
            if text:
                return InferenceResult(
                    text=text,
                    provider=target.provider,
                    model=target.model_id,
                    attempts=attempts,
                    rerouted=attempts > 1,
                )

        if plan.retry_policy == "exhaust_then_reroute":
            # Re-route once if primary/fallbacks are exhausted
            rerouted_plan = router.route_inference(
                intent=intent,
                risk=risk,
                privacy=privacy,
                runtime=runtime,
                owner_id=owner_id,
            )
            if rerouted_plan.primary.model_id not in seen:
                attempts += 1
                text = self.execute(
                    rerouted_plan.primary, prompt, system=system, owner_id=owner_id
                )
                if text:
                    return InferenceResult(
                        text=text,
                        provider=rerouted_plan.primary.provider,
                        model=rerouted_plan.primary.model_id,
                        attempts=attempts,
                        rerouted=True,
                    )

        return InferenceResult(
            text="", provider="", model="", attempts=attempts, rerouted=attempts > 1
        )

    async def generate_async(
        self,
        prompt: str,
        intent: str = "general",
        risk: str = "low",
        *,
        owner_id: str = "operator",
        privacy: str | None = None,
        runtime: str | None = None,
        system: str | None = None,
    ) -> InferenceResult:
        import asyncio

        return await asyncio.to_thread(
            self.generate,
            prompt,
            intent=intent,
            risk=risk,
            owner_id=owner_id,
            privacy=privacy,
            runtime=runtime,
            system=system,
        )

    def generate_json(
        self,
        prompt: str,
        intent: str = "general",
        risk: str = "low",
        *,
        owner_id: str = "operator",
        privacy: str | None = None,
        runtime: str | None = None,
        system: str | None = None,
    ) -> dict[str, Any]:
        result = self.generate(
            prompt,
            intent=intent,
            risk=risk,
            owner_id=owner_id,
            privacy=privacy,
            runtime=runtime,
            system=system,
        )
        text = result.text
        if not text:
            return {}

        text = text.strip()
        if text.startswith("```"):
            text = text.strip("`").replace("json", "", 1).strip()

        try:
            return json.loads(text)
        except json.JSONDecodeError:
            start = text.find("{")
            end = text.rfind("}")
            if start != -1 and end != -1 and end > start:
                try:
                    return json.loads(text[start : end + 1])
                except json.JSONDecodeError:
                    return {}
        return {}

def llm_generate_with_plan(
    prompt: str,
    intent: str = "general",
    risk: str = "low",
    *,
    owner_id: str = "operator",
    privacy: str | None = None,
    runtime: str | None = None,
    system: str | None = None,
    stdb: Any | None = None,
) -> tuple[Any, Any]:
    from heiwa_cognition.router import ComputeRouter

    engine = get_llm_engine(stdb=stdb)
    # We re-run route_inference here just to return the plan to the caller
    # for transparency, although generate() also does it.
    router = ComputeRouter(stdb=stdb)
    plan = router.route_inference(
        intent=intent,
        risk=risk,
        privacy=privacy,
        runtime=runtime,
        owner_id=owner_id,
    )
    result = engine.generate(
        prompt,
        intent=intent,
        risk=risk,
        owner_id=owner_id,
        privacy=privacy,
        runtime=runtime,
        system=system,
    )
    return plan, result


def llm_generate(
    prompt: str,
    intent: str = "general",
    risk: str = "low",
    *,
    owner_id: str = "operator",
    privacy: str | None = None,
    runtime: str | None = None,
    system: str | None = None,
    stdb: Any | None = None,
) -> str:
    engine = get_llm_engine(stdb=stdb)
    result = engine.generate(
        prompt,
        intent=intent,
        risk=risk,
        owner_id=owner_id,
        privacy=privacy,
        runtime=runtime,
        system=system,
    )
    return result.text


async def llm_generate_async(
    prompt: str,
    intent: str = "general",
    risk: str = "low",
    *,
    owner_id: str = "operator",
    privacy: str | None = None,
    runtime: str | None = None,
    system: str | None = None,
    stdb: Any | None = None,
) -> str:
    engine = get_llm_engine(stdb=stdb)
    result = await engine.generate_async(
        prompt,
        intent=intent,
        risk=risk,
        owner_id=owner_id,
        privacy=privacy,
        runtime=runtime,
        system=system,
    )
    return result.text


def llm_generate_json(
    prompt: str,
    intent: str = "general",
    risk: str = "low",
    *,
    owner_id: str = "operator",
    privacy: str | None = None,
    runtime: str | None = None,
    system: str | None = None,
    stdb: Any | None = None,
) -> dict[str, Any]:
    engine = get_llm_engine(stdb=stdb)
    return engine.generate_json(
        prompt,
        intent=intent,
        risk=risk,
        owner_id=owner_id,
        privacy=privacy,
        runtime=runtime,
        system=system,
    )


def llm_is_available(runtime: str = "auto") -> bool:
    try:
        return get_llm_engine().is_available(runtime=runtime)
    except Exception:
        return False
