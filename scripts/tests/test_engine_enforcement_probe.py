"""Negative controls for evidence that must not become an admission claim."""

import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from probe_engine_enforcement import SANDBOX, experiment, observation, run_process, verdict


class EvidenceTests(unittest.TestCase):
    def test_only_permission_errors_count_as_denial(self):
        for number, expected in ((1, "denied"), (13, "denied"), (2, "inconclusive"), (61, "inconclusive")):
            with self.subTest(errno=number):
                row = {"exit_code": 77, "stdout": json.dumps({"status": "denied", "errno": number})}
                self.assertEqual(observation(row), expected)

    def test_crashes_timeouts_and_invalid_output_cannot_pass(self):
        for row in (
            {"error": "timeout"}, {"error": "launch_failed"},
            {"exit_code": -6, "stdout": '{"status":"denied","errno":1}'},
            {"exit_code": 0, "stdout": "not JSON"},
            {"exit_code": 0, "stdout": "[]"},
            {"exit_code": 77, "stdout": '{"status":"denied","errno":[]}'},
            {"exit_code": 77, "stdout": '{"status":"denied","errno":true}'},
            {"exit_code": 77, "stdout": '{"status":"denied","errno":1}', "truncated": True},
        ):
            with self.subTest(row=row):
                self.assertEqual(observation(row), "inconclusive")

    def test_policy_query_is_not_an_actual_apple_event(self):
        row = {"exit_code": 77, "stdout": '{"status":"policy_denied"}'}
        self.assertEqual(observation(row), "inconclusive")
        self.assertEqual(observation(row, policy_query=True), "policy_denied")

    def test_failed_positive_control_invalidates_negative_result(self):
        self.assertEqual(verdict("inconclusive", "denied", "denied"), "inconclusive")
        self.assertEqual(verdict("denied", "denied", "denied"), "inconclusive")
        self.assertEqual(verdict("allowed", "allowed", "denied"), "failed")

    def test_owned_timeout_is_reported_instead_of_denied(self):
        with tempfile.TemporaryDirectory() as directory:
            row = run_process([sys.executable, "-I", "-S", "-c", "import time; time.sleep(10)"],
                              Path(directory), timeout=0.1)
        self.assertEqual(row, {"error": "timeout"})


@unittest.skipUnless(sys.platform == "darwin" and SANDBOX.is_file(), "macOS Seatbelt required")
class MacOSBoundaryTests(unittest.TestCase):
    def test_candidate_matches_canaries_but_never_admits_an_engine(self):
        result = experiment()
        self.assertEqual(result["status"], "matched", result)
        self.assertEqual(result["engine_admission"], "not_established")
        self.assertTrue(result["remaining_proof"])
        rows = {row["name"]: row for row in result["cases"]}
        self.assertEqual(rows["detached_read"]["observed"], "denied")
        self.assertEqual(rows["runtime_loopback_tcp"]["control"], "allowed")
        self.assertEqual(rows["appleevent_policy_only"]["scope"], "policy_query")

    def test_removing_restrictions_is_detected(self):
        result = experiment('(version 1)\n(allow default)\n')
        self.assertEqual(result["status"], "failed", result)
        rows = {row["name"]: row for row in result["cases"]}
        for name in ("credential_read", "approval_write", "runtime_loopback_tcp", "detached_write", "symlink_read"):
            self.assertEqual(rows[name]["status"], "failed")
        self.assertEqual(result["engine_admission"], "not_established")

    def test_broken_profile_cannot_be_mistaken_for_containment(self):
        result = experiment('(version 1)\n(not-a-seatbelt-operation)\n')
        self.assertEqual(result["status"], "inconclusive", result)
        self.assertTrue(all(row["control"] == "allowed" for row in result["cases"]))


if __name__ == "__main__":
    unittest.main()
