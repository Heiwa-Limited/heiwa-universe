from __future__ import annotations

import os

os.environ.setdefault("HEIWA_STATE_BACKEND", "spacetimedb")
os.environ.setdefault("STDB_IDENTITY", "heiwa_test_module")

import heiwa_sdk.db as db_module


class FakeSpacetimeDB:
    def __init__(self, *args, **kwargs):
        self.calls: list[tuple[str, object]] = []

    def create_mission(self, mission_data):
        self.calls.append(("create_mission", mission_data))
        return True

    def append_mission_step(self, step_data):
        self.calls.append(("append_mission_step", step_data))
        return True

    def start_cell_run(self, run_data):
        self.calls.append(("start_cell_run", run_data))
        return True

    def finish_cell_run(self, run_data):
        self.calls.append(("finish_cell_run", run_data))
        return True

    def pause_mission(self, mission_id, summary=None):
        self.calls.append(("pause_mission", {"mission_id": mission_id, "summary": summary}))
        return True

    def resume_mission(self, mission_id, summary=None):
        self.calls.append(("resume_mission", {"mission_id": mission_id, "summary": summary}))
        return True

    def complete_mission(self, mission_id, summary=None):
        self.calls.append(("complete_mission", {"mission_id": mission_id, "summary": summary}))
        return True

    def fail_mission(self, mission_id, error=None):
        self.calls.append(("fail_mission", {"mission_id": mission_id, "error": error}))
        return True

    def register_artifact(self, artifact_data):
        self.calls.append(("register_artifact", artifact_data))
        return True

    def get_mission(self, mission_id):
        self.calls.append(("get_mission", mission_id))
        return {"mission_id": mission_id, "status": "running"}

    def get_missions(self, status=None, limit=50):
        self.calls.append(("get_missions", {"status": status, "limit": limit}))
        return []

    def get_mission_steps(self, mission_id, limit=100):
        self.calls.append(("get_mission_steps", {"mission_id": mission_id, "limit": limit}))
        return []

    def get_cell_runs(self, mission_id=None, status=None, limit=100):
        self.calls.append(("get_cell_runs", {"mission_id": mission_id, "status": status, "limit": limit}))
        return []

    def write_session_summary(self, summary_data):
        self.calls.append(("write_session_summary", summary_data))
        return True

    def list_session_summaries(self, node_id=None, session_id=None, limit=50):
        self.calls.append(
            ("list_session_summaries", {"node_id": node_id, "session_id": session_id, "limit": limit})
        )
        return []

    def list_artifacts(self, mission_id=None, limit=100):
        self.calls.append(("list_artifacts", {"mission_id": mission_id, "limit": limit}))
        return []


def test_mission_facade_delegates_to_stdb(monkeypatch):
    monkeypatch.setattr(db_module, "SpacetimeDB", FakeSpacetimeDB)

    db = db_module.Database()

    assert db.create_mission({"mission_id": "mission-1", "prompt": "deploy"})
    assert db.append_mission_step({"step_id": "step-1", "mission_id": "mission-1"})
    assert db.start_cell_run({"cell_run_id": "run-1", "mission_id": "mission-1"})
    assert db.finish_cell_run({"cell_run_id": "run-1"})
    assert db.pause_mission("mission-1", summary="waiting")
    assert db.resume_mission("mission-1", summary="resumed")
    assert db.complete_mission("mission-1", summary="done")
    assert db.fail_mission("mission-1", error="boom")
    assert db.register_artifact({"artifact_id": "artifact-1", "mission_id": "mission-1"})
    assert db.set_mission_status("mission-1", "waiting_approval", summary="waiting")
    assert db.set_mission_status("mission-1", "running", summary="resumed")
    assert db.write_session_summary({"summary_id": "sum-1", "session_id": "sess-1"})
    assert db.get_mission_steps("mission-1", limit=5) == []
    assert db.get_cell_runs(mission_id="mission-1", limit=5) == []
    assert db.list_session_summaries(node_id="node-1", limit=5) == []
    assert db.list_artifacts(mission_id="mission-1", limit=5) == []

    observed = {name for name, _ in db.stdb.calls}
    expected = {
        "create_mission",
        "append_mission_step",
        "start_cell_run",
        "finish_cell_run",
        "pause_mission",
        "resume_mission",
        "complete_mission",
        "fail_mission",
        "register_artifact",
        "write_session_summary",
        "get_mission_steps",
        "get_cell_runs",
        "list_session_summaries",
        "list_artifacts",
    }
    missing = sorted(expected - observed)
    assert not missing, f"missing STDB delegation calls: {', '.join(missing)}"
