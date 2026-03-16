"""Tests for Heiwa Memory Service."""
import pytest
import json
from unittest.mock import MagicMock, patch, AsyncMock
from heiwa_sdk.memory import MemoryService


class TestMemoryService:
    """Test memory indexing and search."""

    def setup_method(self):
        self.mock_stdb = MagicMock()
        self.service = MemoryService(stdb=self.mock_stdb)

    @pytest.mark.asyncio
    @patch("aiohttp.ClientSession.post")
    async def test_generate_embedding_calls_ollama(self, mock_post):
        # Mock aiohttp response
        mock_resp = MagicMock()
        mock_resp.status = 200
        mock_resp.json = AsyncMock(return_value={"embedding": [0.1, 0.2, 0.3]})
        mock_resp.__aenter__.return_value = mock_resp
        mock_post.return_value = mock_resp

        emb = await self.service.generate_embedding("test text")
        assert emb == [0.1, 0.2, 0.3]
        mock_post.assert_called_once()

    @pytest.mark.asyncio
    @patch.object(MemoryService, "generate_embedding", new_callable=AsyncMock)
    async def test_index_file_calls_stdb(self, mock_gen_emb):
        mock_gen_emb.return_value = [0.1, 0.2]
        content = "This is a test file content."
        
        success = await self.service.index_file("test.py", content)
        
        assert success is True
        self.mock_stdb.insert_knowledge_embedding.assert_called()

    def test_cosine_similarity(self):
        v1 = [1.0, 0.0]
        v2 = [1.0, 0.0]
        assert self.service._cosine_similarity(v1, v2) == pytest.approx(1.0)
        
        v3 = [0.0, 1.0]
        assert self.service._cosine_similarity(v1, v3) == pytest.approx(0.0)

    @pytest.mark.asyncio
    @patch.object(MemoryService, "generate_embedding", new_callable=AsyncMock)
    async def test_query_knowledge_ranks_results(self, mock_gen_emb):
        mock_gen_emb.return_value = [1.0, 0.0]
        self.mock_stdb.search_knowledge_embeddings.return_value = [
            {"source_id": "far", "embedding_json": json.dumps([0.0, 1.0])},
            {"source_id": "near", "embedding_json": json.dumps([0.9, 0.1])},
        ]
        
        results = await self.service.query_knowledge("find me something")
        assert len(results) == 2
        assert results[0]["source_id"] == "near"
        assert results[0]["score"] > results[1]["score"]
