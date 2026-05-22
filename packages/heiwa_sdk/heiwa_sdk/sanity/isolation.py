
import asyncio
import os
from pathlib import Path


def _is_monorepo_root(path: Path) -> bool:
    return (path / "apps").exists() and (path / "packages").exists()


def _candidate_monorepo_roots() -> list[Path]:
    candidates: list[Path] = []
    seen: set[Path] = set()
    for key in ("HEIWA_ROOT_DIR", "HEIWA_WORKSPACE_ROOT", "HEIWA_ROOT"):
        raw = os.getenv(key)
        if not raw:
            continue
        path = Path(raw).expanduser().resolve()
        if path in seen:
            continue
        candidates.append(path)
        seen.add(path)
    for path in (Path.home() / "heiwa-universe", Path.home() / "heiwa"):
        resolved = path.expanduser().resolve()
        if resolved in seen:
            continue
        candidates.append(resolved)
        seen.add(resolved)
    return candidates


def resolve_root_dir(start_path: Path | None = None) -> Path:
    for candidate in _candidate_monorepo_roots():
        if _is_monorepo_root(candidate):
            return candidate
    current = (start_path or Path(__file__).resolve()).resolve()
    for _ in range(6):
        probe = current if current.is_dir() else current.parent
        if _is_monorepo_root(probe):
            return probe
        if probe.parent == probe:
            break
        current = probe.parent
    for candidate in _candidate_monorepo_roots():
        if candidate.exists():
            return candidate
    return current if current.is_dir() else current.parent


# Paths
ROOT_DIR = str(resolve_root_dir())
RUNTIME_SCRIPT = os.path.join(ROOT_DIR, "apps/heiwa_hub/agent_runtime.py")
INTERNAL_CONFIG = os.getenv("HEIWA_INTERNAL_AGENT_CONFIG", "")
CLIENT_CONFIG = os.getenv("HEIWA_CLIENT_AGENT_CONFIG", "")
FIELD_OP_CONFIG = os.getenv("HEIWA_FIELD_OP_CONFIG", "")


def _config_exists(path):
    return bool(path) and os.path.exists(path)

async def run_agent_test(name, config_path, test_prompt, forbidden_response_fragment=None, expected_response_fragment=None):
    print(f"\n--- Testing Agent: {name} ---")
    
    # Start the agent process
    cmd = ["python3", RUNTIME_SCRIPT, "--agent", config_path]
    process = await asyncio.create_subprocess_exec(
        *cmd,
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE
    )
    
    try:
        # Wait for startup
        await asyncio.sleep(2)
        
        # Check startup logs
        # We need to read stdout without blocking. 
        # For simplicity in this test, we interact linearly.
        
        # Send Prompt
        print(f"Sending input: '{test_prompt}'")
        if process.stdin:
            process.stdin.write(f"{test_prompt}\n".encode())
            await process.stdin.drain()
        
        # Read response
        # We'll read a few lines
        output_buffer = ""
        for _ in range(10):
            try:
                line = await asyncio.wait_for(process.stdout.readline(), timeout=1.0)
                decoded_line = line.decode().strip()
                print(f"Agent Output: {decoded_line}")
                output_buffer += decoded_line + "\n"
            except asyncio.TimeoutError:
                break
        
        # Verification Logic
        success = True
        if forbidden_response_fragment and forbidden_response_fragment in output_buffer:
            print(f"FAILURE: Forbidden content '{forbidden_response_fragment}' found in output.")
            success = False
        
        if expected_response_fragment and expected_response_fragment not in output_buffer:
            print(f"FAILURE: Expected content '{expected_response_fragment}' NOT found in output.")
            success = False
            
        if success:
            print(f"SUCCESS: Agent {name} passed isolation test.")
        else:
            print(f"FAILED: Agent {name} failed isolation test.")

    finally:
        try:
            process.terminate()
            await process.wait()
        except:
            pass

async def main():
    print("Starting Runtime Segregation Verification...")

    checks = [
        (
            "Client Agent",
            CLIENT_CONFIG,
            "Check local runtime status",
            None,
            "I do not have direct host-admin tools.",
        ),
        (
            "Heiwa Owner Runtime",
            INTERNAL_CONFIG,
            "Check local runtime status",
            None,
            "Executing local runtime tool...",
        ),
        (
            "Heiwa Field Op",
            FIELD_OP_CONFIG,
            "Check filesystem",
            None,
            "Executing Filesystem tool...",
        ),
    ]

    ran_any = False
    for name, config_path, prompt, forbidden, expected in checks:
        if not _config_exists(config_path):
            print(f"SKIP: {name} config is not set or missing. Provide it via HEIWA_*_AGENT_CONFIG.")
            continue
        ran_any = True
        await run_agent_test(
            name=name,
            config_path=config_path,
            test_prompt=prompt,
            forbidden_response_fragment=forbidden,
            expected_response_fragment=expected,
        )

    if not ran_any:
        print("No canonical isolation configs configured. Set HEIWA_*_AGENT_CONFIG to run this helper.")

if __name__ == "__main__":
    asyncio.run(main())
