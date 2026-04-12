# Heiwa Concise Mode

Provider-agnostic concise-output mode for Heiwa surfaces.

This package is informed by [JuliusBrussee/caveman](https://github.com/JuliusBrussee/caveman) but is intentionally translated into Heiwa's extension model:

- Codex and Claude use native skill installs.
- Gemini uses a native extension wrapper.
- Antigravity inherits the Gemini install.
- Heiwa gets a runtime-local mode artifact.
- Ollama is not a separate install target; Heiwa applies the mode when routing to Ollama-backed work.

## Install

From repo root:

```bash
python3 scripts/install_heiwa_concise_mode.py
```

Use `--copy` if you want copied files instead of symlinks.
