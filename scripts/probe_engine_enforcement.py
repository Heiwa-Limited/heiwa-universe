#!/usr/bin/env python3
"""Offline macOS containment experiments; never an engine admission certificate.

Uses synthetic files and a disposable loopback listener only. No inference,
provider credentials, installed Heiwa state, or Apple application is accessed.
"""

from __future__ import annotations

import argparse
import ctypes
from datetime import datetime, timezone
import errno
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import socket
import subprocess
import sys
import tempfile

# Deliberately an OFFLINE canary profile, not a production allowlist. Broader
# host reads remain permitted. Network is entirely denied, including inference.
PROFILE = '''(version 1)
(allow default)
(deny file-read* (subpath (param "PROTECTED")))
(deny file-write*)
(allow file-write* (subpath (param "WORKSPACE")) (literal "/dev/null"))
(deny network*)
(deny appleevent-send)
'''
SANDBOX = Path("/usr/bin/sandbox-exec")
PERMISSION_ERRNOS = {errno.EPERM, errno.EACCES}
GAPS = [
    "actual provider process, built-in tools, hooks, plugins, and MCP children",
    "provider authentication, inference egress, proxy bypass, and nested sandboxing",
    "real Apple Event delivery, Keychain, Mach services, and other host IPC",
    "runtime principal, grant validation, revocation, and approval-store isolation",
    "production allowlist and binding the tested launch policy to engine admission",
]


def _attempt(operation: str, target: str) -> int:
    try:
        if operation == "read":
            if Path(target).read_bytes() != b"synthetic-canary\n":
                raise ValueError("canary content mismatch")
        elif operation == "write":
            Path(target).write_bytes(b"synthetic-probe-write\n")
        elif operation == "tcp":
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as client:
                client.settimeout(1)
                client.connect(("127.0.0.1", int(target)))
        elif operation == "appleevent-policy":
            # Query only. Never send an Apple Event, open an app, or trigger TCC.
            check = ctypes.CDLL("/usr/lib/libsandbox.dylib").sandbox_check
            check.restype = ctypes.c_int
            check.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int]
            result = check(os.getpid(), b"appleevent-send", 0)
            if result not in (0, 1):
                raise ValueError("unexpected sandbox_check result")
            print(json.dumps({"status": "policy_denied" if result else "allowed"}))
            return 77 if result else 0
        else:
            raise ValueError("unknown operation")
        print(json.dumps({"status": "allowed"}))
        return 0
    except OSError as error:
        denied = error.errno in PERMISSION_ERRNOS
        print(json.dumps({"status": "denied" if denied else "error", "errno": error.errno}))
        return 77 if denied else 78
    except Exception as error:
        print(json.dumps({"status": "error", "error_type": type(error).__name__}))
        return 78


def payload(operation: str, target: str) -> int:
    if not operation.startswith("detached-"):
        return _attempt(operation, target)
    child = os.fork()
    if child == 0:
        try:
            os.setsid()
            result = _attempt(operation.removeprefix("detached-"), target)
            sys.stdout.flush()
        except Exception:
            result = 78
        os._exit(result)

    def cancel_child(signum, frame):
        try:
            os.kill(child, signal.SIGKILL)
        except ProcessLookupError:
            pass
        os.waitpid(child, 0)
        raise SystemExit(78)

    previous = signal.signal(signal.SIGTERM, cancel_child)
    try:
        _, status = os.waitpid(child, 0)
        return os.waitstatus_to_exitcode(status)
    finally:
        signal.signal(signal.SIGTERM, previous)


def run_process(argv: list[str], cwd: Path, timeout: float = 10) -> dict:
    """Bound output, discard caller credentials, and reap our subprocess group."""
    from check_ci_local import stop_process

    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        process = None
        try:
            process = subprocess.Popen(
                argv, cwd=cwd, env={"PATH": os.defpath},
                stdin=subprocess.DEVNULL, stdout=output, stderr=errors,
                start_new_session=True,
            )
            process.wait(timeout=timeout)
            output.seek(0)
            stdout = output.read(4097)
            errors.seek(0)
            stderr = errors.read(1024).decode("utf-8", errors="replace")
            return {"exit_code": process.returncode, "stdout": stdout.decode("utf-8", errors="replace"),
                    "stderr": stderr, "truncated": len(stdout) > 4096}
        except subprocess.TimeoutExpired:
            return {"error": "timeout"}
        except OSError as error:
            return {"error": "launch_failed", "errno": error.errno}
        finally:
            if process is not None:
                stop_process(process)


def observation(result: dict, *, policy_query: bool = False) -> str:
    if result.get("error") or result.get("truncated"):
        return "inconclusive"
    try:
        value = json.loads(result.get("stdout", ""))
    except (ValueError, TypeError):
        return "inconclusive"
    if not isinstance(value, dict):
        return "inconclusive"
    if result.get("exit_code") == 0 and value.get("status") == "allowed":
        return "allowed"
    if result.get("exit_code") == 77:
        number = value.get("errno")
        if value.get("status") == "denied" and type(number) is int and number in PERMISSION_ERRNOS:
            return "denied"
        if policy_query and value.get("status") == "policy_denied":
            return "policy_denied"
    return "inconclusive"


def verdict(control: str, observed: str, expected: str) -> str:
    if control != "allowed" or observed == "inconclusive":
        return "inconclusive"
    return "matched" if observed == expected else "failed"


def input_digests() -> dict:
    scripts = Path(__file__).resolve().parent
    return {name: hashlib.sha256((scripts / name).read_bytes()).hexdigest() for name in (
        "probe_engine_enforcement.py", "check_ci_local.py", "tests/test_engine_enforcement_probe.py",
    )}


def experiment(profile: str = PROFILE) -> dict:
    result = {"engine_admission": "not_established", "remaining_proof": GAPS.copy(), "cases": []}
    if sys.platform != "darwin" or not SANDBOX.is_file():
        return dict(result, status="unsupported")
    with tempfile.TemporaryDirectory(prefix="heiwa-enforcement-") as temporary:
        root = Path(temporary).resolve()
        workspace = root / "workspace"
        protected = root / "protected"
        workspace.mkdir()
        protected.mkdir()
        (workspace / "read.txt").write_bytes(b"synthetic-canary\n")
        (protected / "credential.txt").write_bytes(b"synthetic-canary\n")
        (workspace / "read-link").symlink_to(protected / "credential.txt")
        (workspace / "write-link").symlink_to(protected / "symlink-target.txt")
        profile_path = root / "candidate.sb"
        profile_path.write_text(profile)
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen(8)
            cases = [
                ("workspace_read", "read", workspace / "read.txt", "allowed"),
                ("workspace_write", "write", workspace / "write.txt", "allowed"),
                ("credential_read", "read", protected / "credential.txt", "denied"),
                ("approval_write", "write", protected / "approval.json", "denied"),
                ("outside_write", "write", root / "outside.txt", "denied"),
                ("symlink_read", "read", workspace / "read-link", "denied"),
                ("symlink_write", "write", workspace / "write-link", "denied"),
                ("runtime_loopback_tcp", "tcp", listener.getsockname()[1], "denied"),
                ("detached_read", "detached-read", protected / "credential.txt", "denied"),
                ("detached_write", "detached-write", root / "detached.txt", "denied"),
                ("appleevent_policy_only", "appleevent-policy", "unused", "policy_denied"),
            ]
            for name, operation, target, expected in cases:
                argv = [sys.executable, "-I", "-S", str(Path(__file__).resolve()), "--payload", operation, str(target)]
                control = run_process(argv, workspace)
                # Change the fixture after the positive write control. The host
                # observes the negative attempt independently of child output.
                write_target = Path(target) if operation.endswith("write") else None
                if write_target is not None:
                    write_target.write_bytes(b"unchanged-after-control\n")
                command = [str(SANDBOX), "-D", f"WORKSPACE={workspace}", "-D", f"PROTECTED={protected}",
                           "-f", str(profile_path), *argv]
                restricted = run_process(command, workspace)
                policy_query = operation == "appleevent-policy"
                base = observation(control, policy_query=policy_query)
                observed = observation(restricted, policy_query=policy_query)
                status = verdict(base, observed, expected)
                if write_target is not None and expected == "denied":
                    if write_target.read_bytes() != b"unchanged-after-control\n":
                        status = "failed"
                result["cases"].append({"name": name, "expected": expected, "control": base,
                                        "observed": observed, "status": status,
                                        "scope": "policy_query" if policy_query else "synthetic_operation",
                                        "control_process": control, "restricted_process": restricted})
    statuses = {row["status"] for row in result["cases"]}
    result["status"] = "failed" if "failed" in statuses else "inconclusive" if "inconclusive" in statuses else "matched"
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--payload", nargs=2, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.payload:
        return payload(*args.payload)
    from check_ci_local import source_state, write_receipt

    root = Path(__file__).resolve().parents[1]
    parent = root / "private" / "verification"
    parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    run_dir = Path(tempfile.mkdtemp(prefix="enforcement-", dir=parent))
    receipt = {"schema_version": 1, "kind": "offline-enforcement-spike", "status": "running",
               "started_at": datetime.now(timezone.utc).isoformat(), "source_start": source_state(root),
               "host": {"system": platform.system(), "release": platform.release(), "machine": platform.machine()},
               "profile_sha256": hashlib.sha256(PROFILE.encode()).hexdigest(),
               "input_digests_start": input_digests(),
               "engine_admission": "not_established"}
    path = run_dir / "receipt.json"
    write_receipt(path, receipt)
    try:
        receipt.update(experiment())
    except Exception as error:
        receipt.update(status="inconclusive", error_type=type(error).__name__)
    receipt["source_end"] = source_state(root)
    receipt["input_digests_end"] = input_digests()
    receipt["finished_at"] = datetime.now(timezone.utc).isoformat()
    if receipt["source_start"] != receipt["source_end"] or receipt["input_digests_start"] != receipt["input_digests_end"]:
        receipt.update(status="inconclusive", error_type="source_changed")
    write_receipt(path, receipt)
    print(f"Offline containment: {receipt['status']}")
    for row in receipt.get("cases", []):
        print(f"  {row['name']}: {row['status']} ({row['observed']})")
    print("Engine admission: NOT ESTABLISHED (see remaining proof)")
    print(f"Receipt: {path}")
    return 0 if receipt["status"] == "matched" else 1


if __name__ == "__main__":
    # Payloads run with -I -S and use only stdlib; host receipt helpers are lazy.
    raise SystemExit(main())
