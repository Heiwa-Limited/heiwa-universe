from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


def main() -> int:
    required_files = [
        ROOT / "HEIWA.md",
        ROOT / "SOUL.md",
        ROOT / "IDENTITY.md",
        ROOT / "ops" / "context" / "HEIWA.md",
        ROOT / "ops" / "context" / "SOUL.md",
        ROOT / "ops" / "context" / "IDENTITY.md",
        ROOT / "ops" / "rooms" / "control-plane.md",
        ROOT / "ops" / "rooms" / "execution.md",
        ROOT / "ops" / "rooms" / "orchestration.md",
        ROOT / "ops" / "rooms" / "infra.md",
        ROOT / "ops" / "rooms" / "sdk.md",
    ]
    failures: list[str] = []

    for path in required_files:
        if not path.exists():
            failures.append(f"missing required context file: {path}")

    if not failures:
        heiwa_text = (ROOT / "HEIWA.md").read_text(encoding="utf-8")
        if "compatibility shim" not in heiwa_text.lower():
            failures.append("HEIWA.md should explain that it is a compatibility shim")
        if "ops/context/HEIWA.md" not in heiwa_text:
            failures.append("HEIWA.md should redirect to ops/context/HEIWA.md")

        soul_text = (ROOT / "SOUL.md").read_text(encoding="utf-8")
        if "compatibility shim" not in soul_text.lower():
            failures.append("SOUL.md should explain that it is a compatibility shim")
        if "ops/context/SOUL.md" not in soul_text:
            failures.append("SOUL.md should redirect to ops/context/SOUL.md")

        identity_text = (ROOT / "IDENTITY.md").read_text(encoding="utf-8")
        if "compatibility shim" not in identity_text.lower():
            failures.append("IDENTITY.md should explain that it is a compatibility shim")
        if "ops/context/IDENTITY.md" not in identity_text:
            failures.append("IDENTITY.md should redirect to ops/context/IDENTITY.md")

        canonical_heiwa_text = (ROOT / "ops" / "context" / "HEIWA.md").read_text(encoding="utf-8")
        if "Task Routing Table" not in canonical_heiwa_text:
            failures.append("ops/context/HEIWA.md is missing the task routing table")
        if "ops/rooms/control-plane.md" not in canonical_heiwa_text:
            failures.append("ops/context/HEIWA.md is missing ops/rooms index references")

    if failures:
        print("Agent context map test FAILED")
        for failure in failures:
            print(f" - {failure}")
        return 1

    print("Agent context map test PASSED")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
