from __future__ import annotations

import datetime
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "packages/heiwa_sdk"))
sys.path.insert(0, str(ROOT / "packages/heiwa_protocol"))
sys.path.insert(0, str(ROOT / "packages/heiwa_identity"))
sys.path.insert(0, str(ROOT / "apps"))


class FakeSpacetimeDB:
    """Minimal fake that satisfies the MCP server surface test."""

    def __init__(self, *args, **kwargs):
        self.runs: list[dict] = []

    def call(self, *args, **kwargs):
        return True

    def query(self, sql):
        if "FROM runs" in sql:
            return list(self.runs)
        return []

    def record_run(self, run_data):
        self.runs.append(run_data)
        return True

    def get_runs(self, proposal_id=None, limit=50):
        rows = list(self.runs)
        if proposal_id:
            rows = [r for r in rows if r.get("proposal_id") == proposal_id]
        return rows[:limit]

    def get_model_usage_summary(self, minutes=60):
        return [{"model_id": "ollama/llama-4-scout:q4_k_m", "request_count": 1, "total_tokens": 11, "total_cost": 0.0}]

    def upsert_liveness_state(self, *args, **kwargs):
        return True

    def get_liveness_state(self, key):
        return {"key": key, "last_state": "ONLINE", "last_changed_at": datetime.datetime.now(datetime.timezone.utc).isoformat()}

    def list_nodes(self, status=None):
        return [{"node_id": "test-node", "status": "ONLINE"}]


def main() -> int:
    original_backend = os.environ.get("HEIWA_STATE_BACKEND")
    original_identity = os.environ.get("STDB_IDENTITY")
    original_auth = os.environ.get("HEIWA_AUTH_TOKEN")
    os.environ["HEIWA_STATE_BACKEND"] = "spacetimedb"
    os.environ["STDB_IDENTITY"] = "heiwa_test_module"
    os.environ["HEIWA_AUTH_TOKEN"] = "test-mcp-token"

    try:
        import heiwa_sdk.db as db_module

        original_stdb_class = db_module.SpacetimeDB
        db_module.SpacetimeDB = FakeSpacetimeDB
        try:
            db = db_module.Database()
            now = datetime.datetime.now(datetime.timezone.utc)
            db.record_run({
                "run_id": "run-mcp",
                "proposal_id": "proposal-mcp",
                "started_at": (now - datetime.timedelta(minutes=1)).isoformat(),
                "ended_at": now.isoformat(),
                "status": "PASS",
                "chain_result": "ok",
                "model_id": "ollama/llama-4-scout:q4_k_m",
                "tokens_total": 11,
                "cost": 0.0,
            })

            from apps.heiwa_hub import mcp_server

            mcp_server.db = db
            # Re-set after import because load_swarm_env() may override from .env/vault
            os.environ["HEIWA_AUTH_TOKEN"] = "test-mcp-token"

            from fastapi.testclient import TestClient

            client = TestClient(mcp_server.app)
            auth = {"Authorization": "Bearer test-mcp-token"}

            failures: list[str] = []
            if client.get("/health").status_code != 200:
                failures.append("/health should return 200")
            status_payload = client.get("/status").json()
            if "state_backend" not in status_payload:
                failures.append(f"/status missing state_backend: {status_payload}")
            tools_payload = client.get("/tools", headers=auth).json()
            native_names = {tool["name"] for tool in tools_payload.get("native", [])}
            if "heiwa_resolve_route" not in native_names:
                failures.append("/tools is missing heiwa_resolve_route")
            if "heiwa_run_bench" not in native_names:
                failures.append("/tools is missing heiwa_run_bench")
            if "heiwa_get_cells_catalog" not in native_names:
                failures.append("/tools is missing heiwa_get_cells_catalog")
            bench_payload = client.post("/call/heiwa_run_bench", json={}, headers=auth).json()
            if '"ok": true' not in bench_payload["content"][0]["text"]:
                failures.append("/call/heiwa_run_bench did not report success")
            cells_payload = client.post("/call/heiwa_get_cells_catalog", json={"prompt": "implement code refactor"}, headers=auth).json()
            if "codex-builder" not in cells_payload["content"][0]["text"]:
                failures.append("/call/heiwa_get_cells_catalog did not recommend codex-builder")

            if failures:
                print("MCP server surface test FAILED")
                for failure in failures:
                    print(f" - {failure}")
                return 1

            print("MCP server surface test PASSED")
            return 0
        finally:
            db_module.SpacetimeDB = original_stdb_class
    finally:
        if original_backend is not None:
            os.environ["HEIWA_STATE_BACKEND"] = original_backend
        else:
            os.environ.pop("HEIWA_STATE_BACKEND", None)
        if original_identity is not None:
            os.environ["STDB_IDENTITY"] = original_identity
        else:
            os.environ.pop("STDB_IDENTITY", None)
        if original_auth is not None:
            os.environ["HEIWA_AUTH_TOKEN"] = original_auth
        else:
            os.environ.pop("HEIWA_AUTH_TOKEN", None)


if __name__ == "__main__":
    raise SystemExit(main())
