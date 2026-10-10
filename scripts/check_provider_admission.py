#!/usr/bin/env python3
"""Reject production adapter construction outside registry admission.

This gate complements Rust visibility and behavioral tests; it scans every
production Rust module, including modules not compiled on the current host.
"""
from __future__ import annotations

from pathlib import Path
import re
import sys

ADAPTERS = {
    "anthropic_api": "AnthropicApiAdapter", "openai_api": "OpenAiApiAdapter",
    "gemini_api": "GeminiApiAdapter", "openrouter": "OpenRouterAdapter",
    "claude_code": "ClaudeCodeCliAdapter", "codex_cli": "CodexCliAdapter",
    "gemini_cli": "GeminiCliAdapter", "ollama": "OllamaCliAdapter",
}


def violations(root: Path) -> list[str]:
    errors: list[str] = []
    providers = root / "crates/heiwa_provider/src/providers"
    factory = root / "crates/heiwa_provider/src/routing.rs"
    for module, adapter in ADAPTERS.items():
        path = providers / f"{module}.rs"
        source = path.read_text()
        if re.search(rf"pub\s+struct\s+{adapter}\s*(?:;|\()", source):
            errors.append(f"{path.relative_to(root)}: forgeable adapter shape")
        fields = re.search(rf"pub\s+struct\s+{adapter}\s*\{{([^}}]+)\}}", source)
        if not fields or re.search(r"\bpub\b", fields.group(1)):
            errors.append(f"{path.relative_to(root)}: adapter fields must be private")
        if re.search(rf"impl\s+Default\s+for\s+{adapter}\b", source):
            errors.append(f"{path.relative_to(root)}: Default bypasses admission")
        if not re.search(r"pub\(crate\)\s+fn\s+from_admitted_lane\s*\(\s*lane:\s*&crate::admission::RegistryAdmittedLane\b", source):
            errors.append(f"{path.relative_to(root)}: constructor must require a private-field witness")
        impl_start = source.index("{", source.index(f"impl {adapter}"))
        depth, impl_end = 1, impl_start + 1
        while depth and impl_end < len(source):
            depth += (source[impl_end] == "{") - (source[impl_end] == "}")
            impl_end += 1
        impl = source[impl_start:impl_end]
        if re.search(r"\bpub\s+(?:async\s+)?fn\b", impl):
            errors.append(f"{path.relative_to(root)}: public constructor or builder bypass")
        public_methods = re.findall(r"\bpub(?:\([^)]*\))?\s+(?:async\s+)?fn\s+(\w+)", impl)
        if public_methods != ["from_admitted_lane"]:
            errors.append(f"{path.relative_to(root)}: admission is the only callable constructor")
        if re.search(r"\bfn\s+(?:new|from_registry|with_model)\b", impl):
            errors.append(f"{path.relative_to(root)}: legacy constructor bypass")
    admission = root / "crates/heiwa_provider/src/admission.rs"
    source = admission.read_text()
    fields = re.search(r"pub\s+struct\s+RegistryAdmittedLane\s*\{([^}]+)\}", source)
    if not fields or re.search(r"\bpub\b", fields.group(1)):
        errors.append("admission: lane fields must be private")
    if re.search(r"derive\([^)]*(?:Deserialize|Default)[^)]*\)\s*pub\s+struct\s+RegistryAdmittedLane", source):
        errors.append("admission: lane cannot deserialize or default")
    if re.search(r"impl(?:<[^>]+>)?\s+(?:Deserialize|Default)(?:<[^>]+>)?\s+for\s+RegistryAdmittedLane", source):
        errors.append("admission: lane cannot implement deserialization/default")
    for base in [root / "apps", root / "crates"]:
        for path in base.rglob("*.rs"):
            if "tests" in path.parts or "target" in path.parts:
                continue
            source = path.read_text()
            if path.parent != providers and path != factory:
                for adapter in ADAPTERS.values():
                    if re.search(rf"\b{adapter}\s*(?:::|\{{)", source):
                        errors.append(f"{path.relative_to(root)}: {adapter} construction outside factory")
            if path != admission and re.search(r"RegistryAdmittedLane\s*\{", source):
                errors.append(f"{path.relative_to(root)}: lane construction outside admission")
    main = (root / "apps/heiwa_shell/src/main.rs").read_text()
    start = main.index("fn default_model_call_runtime()")
    end = main.index("fn default_loop_model_caller()", start)
    runtime = main[start:end]
    if "ModelCallExecutor::new_registry(" not in runtime or "ModelCallExecutor::new(" in runtime:
        errors.append("shell production executor must use per-execution registry admission")
    return errors


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    errors = violations(root)
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        return 1
    print("Provider admission structure passed: all transports require the selected witness")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
