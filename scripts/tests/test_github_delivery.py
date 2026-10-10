#!/usr/bin/env python3
"""Delivery regressions use fake gh/git; they never contact or mutate GitHub."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SHA = "a" * 40


class DeliveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="heiwa-delivery-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.calls = self.root / "calls.jsonl"
        self.gate_marker = self.root / "gate-called"
        self.state = self.root / "state.json"
        self.fixture = {
            "head": SHA, "base": "dev", "state": "OPEN", "draft": False,
            "mergeState": "CLEAN", "mergeable": "MERGEABLE", "decision": "",
            "checks": [{"name": "Rust Source Policy", "bucket": "pass"}],
        "pages": [{"data": {"repository": {"pullRequest": {"reviewThreads": {
                "nodes": [], "pageInfo": {"hasNextPage": False, "endCursor": None}
            }}}}}], "runHead": SHA, "runEvent": "workflow_dispatch",
            "runWorkflow": 42, "runBranch": "main", "dispatchUrl": True,
        }
        bindir = self.root / "bin"
        bindir.mkdir()
        fake = bindir / "gh"
        fake.write_text('''#!/usr/bin/env python3
import json, os, subprocess, sys
from pathlib import Path
args=sys.argv[1:]; s=json.loads(Path(os.environ['FIXTURE_STATE']).read_text())
calls=Path(os.environ['FIXTURE_CALLS'])
prior=[json.loads(x) for x in calls.read_text().splitlines()] if calls.exists() else []
with calls.open('a') as f: f.write(json.dumps(args)+'\\n')
def emit(x):
    if '--jq' in args:
        result=subprocess.run(['jq','-cr',args[args.index('--jq')+1]],input=json.dumps(x),text=True,capture_output=True)
        sys.stdout.write(result.stdout); sys.stderr.write(result.stderr)
        if result.returncode: sys.exit(result.returncode)
    else: print(json.dumps(x))
if args[:2]==['pr','checks']:
    emit(s['checks']); sys.exit(s.get('checksExit',0))
if args[:2]==['pr','view']:
    count=sum(x[:2]==['pr','view'] for x in prior)
    head=s.get('laterHead',s['head']) if count else s['head']
    merged=any(x[:2]==['pr','merge'] for x in prior)
    emit(dict(headRefOid=head,baseRefName=s['base'],headRefName='codex/fixture',mergeCommit={'oid':s['head']},
        state='MERGED' if merged else s['state'],isDraft=s['draft'],
        mergeStateStatus=s['mergeState'],mergeable=s['mergeable'],reviewDecision=s['decision']))
elif args[:2]==['pr','merge']: sys.exit(0)
elif args[:2]==['api','graphql']:
    if s.get('reviewError'): sys.exit(1)
    emit(s['pages'])
elif args[0]=='api':
    print('42' if '/actions/workflows/' in args[1] else s.get('mainHead',s['head']))
elif args[:2]==['workflow','run']:
    if s.get('dispatchError'): sys.exit(1)
    if s['dispatchUrl']: print('https://github.com/Heiwa-Limited/heiwa-universe/actions/runs/123')
elif args[:2]==['run','view']:
    emit(dict(headSha=s['runHead'],event=s['runEvent'],workflowDatabaseId=s['runWorkflow'],
        headBranch=s['runBranch'],url='https://github.com/Heiwa-Limited/heiwa-universe/actions/runs/123'))
elif args[:2]==['pr','update-branch']: sys.exit(0)
else: sys.exit(9)
''')
        fake.chmod(0o755)
        (bindir / "sleep").write_text("#!/bin/sh\nexit 0\n")
        (bindir / "sleep").chmod(0o755)
        (bindir / "git").write_text('''#!/usr/bin/env python3
import os, sys
from pathlib import Path
args=sys.argv[1:]
if 'clone' in args:
    root=Path(args[-1]); (root/'scripts').mkdir(parents=True)
    (root/'scripts/check_ci_local.sh').write_text('touch "$FIXTURE_GATE_MARKER"\\nexit "$FIXTURE_GATE_STATUS"\\n')
elif 'rev-parse' in args: print(os.environ['FIXTURE_SHA'])
elif 'get-url' in args: print('https://github.com/Heiwa-Limited/heiwa-universe')
elif 'status' in args and os.environ.get('FIXTURE_DIRTY'): print(' M fixture')
''')
        (bindir / "git").chmod(0o755)
        self.env = dict(os.environ, PATH=f"{bindir}:{os.environ['PATH']}",
                        FIXTURE_STATE=str(self.state), FIXTURE_CALLS=str(self.calls), FIXTURE_SHA=SHA,
                        FIXTURE_GATE_MARKER=str(self.gate_marker), FIXTURE_GATE_STATUS="1")

    def run_guard(self, function, **changes):
        self.fixture.update(changes)
        self.state.write_text(json.dumps(self.fixture))
        command = f'source "$1"; {function}'
        return subprocess.run(["bash", "-euo", "pipefail", "-c", command, "fixture",
                               str(ROOT / "scripts/lib/github_delivery.sh")],
                              env=self.env, text=True, capture_output=True, timeout=10)

    def calls_for(self, prefix):
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()] if self.calls.exists() else []
        return [call for call in calls if call[:len(prefix)] == prefix]

    def merge(self, **changes):
        return self.run_guard(f'github_merge_pr Heiwa-Limited/heiwa-universe 17 {SHA} dev', **changes)

    def test_ship_script_refuses_changed_head_before_any_merge(self):
        # Execute the real entry point, including its old wait_checks/merge path.
        self.fixture["head"] = "b" * 40
        self.state.write_text(json.dumps(self.fixture))
        result = subprocess.run(["bash", str(ROOT / "scripts/ship_release.sh"), "17", "0.3.5", SHA],
                                env=self.env, capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls_for(["pr", "merge"]), [], result.stdout + result.stderr)

    def test_successful_merge_is_bound_to_exact_head(self):
        result = self.merge()
        self.assertEqual(result.returncode, 0, result.stderr)
        call, = self.calls_for(["pr", "merge"])
        self.assertEqual(call[call.index("--match-head-commit") + 1], SHA)
        self.assertNotIn("--admin", call)
        self.assertNotIn("--auto", call)

    def test_integration_gate_failure_stops_before_promotion(self):
        self.state.write_text(json.dumps(self.fixture))
        result = subprocess.run(["bash", str(ROOT / "scripts/ship_release.sh"), "17", "0.3.5", SHA],
                                env=self.env, capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(self.gate_marker.exists(), result.stdout + result.stderr)
        self.assertEqual(len(self.calls_for(["pr", "merge"])), 1)
        self.assertEqual(self.calls_for(["pr", "create"]), [])

    def test_dirty_checkout_stops_before_remote_mutation(self):
        self.env["FIXTURE_DIRTY"] = "1"
        self.state.write_text(json.dumps(self.fixture))
        result = subprocess.run(["bash", str(ROOT / "scripts/ship_release.sh"), "17", "0.3.5", SHA],
                                env=self.env, capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls_for(["pr", "merge"]), [])
        self.assertEqual(self.calls_for(["workflow", "run"]), [])

    def test_merge_denials_never_invoke_merge(self):
        for changes in [dict(head="b" * 40), dict(laterHead="b" * 40), dict(base="main"),
                        dict(state="CLOSED"), dict(draft=True), dict(mergeState="BLOCKED"),
                        dict(mergeable="UNKNOWN"), dict(decision="CHANGES_REQUESTED"),
                        dict(checks=[]), dict(checks=[{"name": "required", "bucket": "pending"}]),
                        dict(checks=[{"name": "required", "bucket": "fail"}]), dict(reviewError=True)]:
            with self.subTest(changes=changes):
                self.setUp()
                result = self.merge(**changes)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.calls_for(["pr", "merge"]), [])

    def test_unresolved_outdated_thread_on_second_page_blocks_merge(self):
        pages = json.loads(json.dumps(self.fixture["pages"]))
        pages[0]["data"]["repository"]["pullRequest"]["reviewThreads"]["pageInfo"] = {
            "hasNextPage": True, "endCursor": "next"}
        second = json.loads(json.dumps(self.fixture["pages"][0]))
        second["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"] = [
            {"id": "thread", "isResolved": False, "isOutdated": True}]
        pages.append(second)
        self.assertNotEqual(self.merge(pages=pages).returncode, 0)
        self.assertEqual(self.calls_for(["pr", "merge"]), [])
        call, = self.calls_for(["api", "graphql"])
        self.assertIn("--paginate", call)

    def test_incomplete_or_malformed_review_evidence_blocks_merge(self):
        for pages in [[], [{"errors": [{"message": "unavailable"}]}],
                      [{"data": {"repository": {"pullRequest": {"reviewThreads": {
                          "nodes": [], "pageInfo": {"hasNextPage": True}}}}}}]]:
            with self.subTest(pages=pages):
                self.setUp()
                self.assertNotEqual(self.merge(pages=pages).returncode, 0)
                self.assertEqual(self.calls_for(["pr", "merge"]), [])

    def dispatch(self, **changes):
        return self.run_guard(f'github_dispatch_run Heiwa-Limited/heiwa-universe release.yml {SHA} -f tag=v0.3.5', **changes)

    def test_dispatch_uses_returned_run_id(self):
        result = self.dispatch()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "123")
        self.assertEqual(self.calls_for(["run", "list"]), [])

    def test_dispatch_failures_never_fall_back_to_latest_run(self):
        for changes in [dict(dispatchError=True), dict(dispatchUrl=False), dict(mainHead="b" * 40),
                        dict(runHead="b" * 40), dict(runWorkflow=99), dict(runEvent="push"),
                        dict(runBranch="dev")]:
            with self.subTest(changes=changes):
                self.setUp()
                result = self.dispatch(**changes)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.calls_for(["run", "list"]), [])
                self.assertEqual(self.calls_for(["run", "watch"]), [])


if __name__ == "__main__":
    unittest.main()
