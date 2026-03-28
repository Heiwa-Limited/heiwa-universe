"""Heiwa Memory Service: Embeddings, Knowledge Indexing, and Semantic Search."""
from __future__ import annotations

import hashlib
import json
import logging
import os
from pathlib import Path
from typing import Any, List, Dict

import aiohttp
from heiwa_sdk.spacetimedb import SpacetimeDB

logger = logging.getLogger("SDK.Memory")


class MemoryService:
    """
    Manages the knowledge layer and execution memory.
    Uses qwen3-embedding:0.6b via Ollama for vector generation.
    """

    def __init__(self, stdb: SpacetimeDB, ollama_url: str | None = None) -> None:
        self.stdb = stdb
        # On Railway, Ollama is OFF by default unless HEIWA_OLLAMA_URL is provided.
        # On Boost nodes, it defaults to localhost.
        host_runtime = os.getenv("HEIWA_RUNTIME", "local").lower()
        default_url = "http://127.0.0.1:11434" if host_runtime != "railway" else None
        
        self.ollama_url = ollama_url or os.getenv("HEIWA_OLLAMA_URL") or default_url
        self.model = "qwen3-embedding:0.6b"
        
        if not self.ollama_url:
            logger.info("MemoryService: Ollama disabled (Railway/Cloud mode). Using degraded embedding path.")

    async def generate_embedding(self, text: str) -> List[float]:
        """Generate vector embedding for the given text using Ollama with Cloud Fallback."""
        if not self.ollama_url:
            # Future: add direct Gemini/OpenAI embedding fallback here
            return []

        url = f"{self.ollama_url}/api/embeddings"
        payload = {"model": self.model, "prompt": text}

        try:
            async with aiohttp.ClientSession() as session:
                async with session.post(url, json=payload, timeout=5) as resp:
                    if resp.status != 200:
                        logger.warning("Ollama unavailable (status %d). Memory operating in degraded mode.", resp.status)
                        return []
                    data = await resp.json()
                    return data.get("embedding") or []
        except Exception as exc:
            logger.warning("Ollama connection failed: %s. Memory operating in degraded mode.", exc)
            return []

    async def index_file(self, file_path: str, content: str, source_type: str = "code_file") -> bool:
        """Chunk a file and index its embeddings in STDB."""
        if not content.strip():
            return False

        chunks = self._chunk_text(content)
        success = True

        for i, chunk in enumerate(chunks):
            chunk_hash = hashlib.sha256(chunk.encode()).hexdigest()
            source_id = f"{file_path}#chunk{i}"
            
            # TODO: In Phase 2, we could check if hash already exists to skip re-embedding
            embedding = await self.generate_embedding(chunk)
            if not embedding:
                success = False
                continue

            # SpacetimeDB call (sync bridge)
            self.stdb.insert_knowledge_embedding(
                source_type=source_type,
                source_id=source_id,
                content_hash=chunk_hash,
                embedding=embedding,
                ttl_hours=0,
            )
        
        return success

    async def query_knowledge(self, query_text: str, limit: int = 5) -> List[Dict[str, Any]]:
        """Search for semantically similar knowledge chunks."""
        query_embedding = await self.generate_embedding(query_text)
        if not query_embedding:
            return []

        # Fetch all embeddings from STDB (SQL doesn't support vector search yet)
        all_entries = self.stdb.search_knowledge_embeddings(limit=500)
        if not all_entries:
            return []

        results = []
        for entry in all_entries:
            try:
                emb_json = entry.get("embedding_json")
                if not emb_json:
                    continue
                embedding = json.loads(emb_json)
                score = self._cosine_similarity(query_embedding, embedding)
                results.append({**entry, "score": score})
            except Exception:
                continue

        # Sort by score descending
        results.sort(key=lambda x: x["score"], reverse=True)
        return results[:limit]

    def record_execution(
        self,
        task_id: str,
        lease_id: str,
        model: str,
        outcome: str,
        duration_ms: int,
        error: str | None = None,
        execution_program_json: str | None = None,
    ) -> bool:
        """Record task outcome in execution_memory.

        execution_program_json is accepted for future persistence but NOT forwarded
        to STDB — the insert_execution_memory reducer has fixed arity.
        The program is logged here; STDB schema migration comes in v2.
        """
        if execution_program_json:
            import logging
            logging.getLogger(__name__).debug(
                "ExecutionProgram for %s logged (STDB persistence deferred to v2)", task_id,
            )
        return self.stdb.insert_execution_memory(
            task_dispatch_id=task_id,
            lease_id=lease_id,
            model_used=model,
            outcome=outcome,
            duration_ms=duration_ms,
            error_summary=error,
        )

    def get_execution_stats(self, model_id: str, limit: int = 20) -> Dict[str, Any]:
        """Aggregate success rate and latency for a given model from STDB."""
        query = (
            f"SELECT outcome, duration_ms FROM execution_memory "
            f"WHERE model_used = '{self.stdb._escape_sql_literal(model_id)}' "
            f"LIMIT {int(limit)}"
        )
        rows = self.stdb.query(query)
        rows.sort(key=lambda r: r.get("created_at", ""), reverse=True)
        if not rows:
            return {"success_rate": 1.0, "avg_latency_ms": 0, "p95_latency_ms": 0, "count": 0}

        passed = sum(1 for r in rows if r["outcome"] == "pass")
        latencies = sorted([int(r["duration_ms"]) for r in rows])
        
        count = len(rows)
        avg_latency = sum(latencies) // count
        p95_index = min(count - 1, int(count * 0.95))
        p95_latency = latencies[p95_index]

        return {
            "success_rate": round(passed / count, 2),
            "avg_latency_ms": avg_latency,
            "p95_latency_ms": p95_latency,
            "count": count
        }

    def _chunk_text(self, text: str, chunk_size: int = 1000, overlap: int = 100) -> List[str]:
        """Simple sliding window chunking."""
        if len(text) <= chunk_size:
            return [text]
        
        chunks = []
        start = 0
        while start < len(text):
            end = start + chunk_size
            chunks.append(text[start:end])
            start += chunk_size - overlap
        return chunks

    def _cosine_similarity(self, v1: List[float], v2: List[float]) -> float:
        """Compute cosine similarity between two vectors."""
        import math
        if not v1 or not v2 or len(v1) != len(v2):
            return 0.0
        
        dot_product = sum(a * b for a, b in zip(v1, v2))
        magnitude1 = math.sqrt(sum(a * a for a in v1))
        magnitude2 = math.sqrt(sum(a * a for a in v2))
        
        if magnitude1 == 0 or magnitude2 == 0:
            return 0.0
        return dot_product / (magnitude1 * magnitude2)
