"""Phase 2 integration test: memory indexing → enriched dispatch."""
import json
import pytest
from unittest.mock import MagicMock, patch, AsyncMock
from heiwa_protocol.routing import BrokerRouteRequest
from heiwa_cognition.enrichment import BrokerEnrichmentService


class TestPhase2Integration:
    """Verify Phase 2: Memory indexing and enriched dispatch."""

    @pytest.mark.asyncio
    @patch("heiwa_sdk.memory.aiohttp.ClientSession.post")
    async def test_full_memory_pipeline(self, mock_post):
        # 1. Mock Ollama embedding response
        mock_resp = MagicMock()
        mock_resp.status = 200
        mock_resp.json = AsyncMock(return_value={"embedding": [0.1] * 768})
        mock_resp.__aenter__.return_value = mock_resp
        mock_post.return_value = mock_resp

        # 2. Setup mock STDB and enrichment service
        mock_stdb = MagicMock()
        # Mock search to return our "indexed" file
        mock_stdb.search_knowledge_embeddings.return_value = [
            {
                "source_id": "packages/heiwa_sdk/heiwa_sdk/memory.py#chunk0",
                "embedding_json": json.dumps([0.1] * 768)
            }
        ]
        
        with patch("heiwa_cognition.enrichment.Database") as mock_db_cls:
            mock_db = MagicMock()
            mock_db.stdb = mock_stdb
            mock_db_cls.return_value = mock_db
            
            service = BrokerEnrichmentService()
            # Ensure the service uses our mock STDB
            service.memory.stdb = mock_stdb
            service.db = mock_db

            # 3. Create a request that should trigger memory enrichment
            request = BrokerRouteRequest(
                request_id="test-req",
                task_id="test-task",
                raw_text="How does the memory service work?",
                auth_validated=True
            )

            # 4. Enrich
            result = await service.enrich(request)

            # 5. Verify result includes context files
            assert "context_files_json" in result.to_dict()
            context_files = json.loads(result.context_files_json)
            assert len(context_files) > 0
            assert "packages/heiwa_sdk/heiwa_sdk/memory.py" in context_files

    @pytest.mark.asyncio
    async def test_executor_records_memory(self):
        from heiwa_hub.agents.heiwaclaw import HeiwaClawAgent as ExecutorAgent
        from heiwa_protocol.protocol import Subject

        mock_stdb = MagicMock()
        with patch("heiwa_hub.agents.heiwaclaw.Database") as mock_db_cls:
            mock_db = MagicMock()
            mock_db.stdb = mock_stdb
            mock_db_cls.return_value = mock_db
            
            agent = ExecutorAgent()
            # Mock memory service
            agent.memory = MagicMock()
            
            # Simulate task execution
            payload = {
                "task_id": "test-task-123",
                "instruction": "test instruction",
                "target_model": "test-model",
                "target_runtime": agent.executor_runtime,
            }
            
            # We mock the internal _handle_exec to just check memory recording
            # or we can mock gateway.execute
            with patch.object(agent.gateway, "execute", AsyncMock(return_value=(0, "Success"))):
                with patch.object(agent, "speak", AsyncMock()):
                    await agent._handle_exec({"data": payload})
            
            # Verify record_execution was called
            agent.memory.record_execution.assert_called_once()
            call_args = agent.memory.record_execution.call_args[1]
            assert call_args["task_id"] == "test-task-123"
            assert call_args["outcome"] == "pass"
