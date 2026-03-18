# Acceptance Checklist

- [ ] Diagram shows a distinct local execution substrate under the broader Heiwa architecture.
- [ ] Diagram shows `heiwa` launcher and `CLIContext` exporting the canonical monorepo root.
- [ ] Diagram shows wrapper fallback roots resolving to the Heiwa monorepo root, not `apps/heiwa_cli` or `apps`.
- [ ] Gemini lane is labeled with `yolo` approval mode.
- [ ] Claude lane is labeled with `bypassPermissions`.
- [ ] Codex lane is labeled with `danger-full-access`.
- [ ] OpenClaw is shown as local-first with an Ollama auth stub in the shell/runtime layer.
- [ ] PicoClaw is shown as local-first on the Heiwa repo root with the current `config/identities/profiles.json` identity map.
- [ ] The local model stack annotation uses currently configured local models, not stale `deepseek-coder-v2:16b` or absent 7B variants.
- [ ] No element implies the wrapper layer is optional or that the runtime contract can drift independently from `ai_router.json`.
