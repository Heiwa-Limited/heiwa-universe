# Update pyproject.toml Dependencies Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `pyproject.toml` the authoritative source for Python dependencies in the monorepo.

**Architecture:** Move dependencies from `requirements.txt` and `docs/requirements.txt` into `pyproject.toml` using standard PEP 621 syntax.

**Tech Stack:** Python (uv, pyproject.toml).

---

### Task 1: Update pyproject.toml

**Files:**

- Modify: `pyproject.toml`

- [ ] **Step 1: Update `pyproject.toml` with dependencies**

```toml
[project]
name = "heiwa-universe"
version = "0.1.0"
requires-python = ">=3.14"
dependencies = [
    "python-dotenv>=1.0.0",
    "pyyaml>=6.0",
    "psutil>=5.9.0",
    "tenacity>=8.2.0",
    "cryptography>=42.0.0",
    "fastapi>=0.104.0",
    "uvicorn>=0.24.0",
    "requests>=2.32.0",
    "httpx>=0.27.0",
    "aiohttp>=3.13.5",
    "websockets>=12.0",
    "psycopg2-binary>=2.9.9",
    "discord.py>=2.3.0",
    "rich>=13.0.0",
    "prompt_toolkit>=3.0.43",
    "textual>=0.53.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.0.0",
    "pytest-asyncio>=1.3.0",
    "pytest-cov>=6.0.0",
]
docs = [
    "mkdocs>=1.6,<2.0",
    "mkdocs-material>=9.6,<10.0",
    "pymdown-extensions>=10.14,<11.0",
]

[tool.pytest.ini_options]
testpaths = ["apps/heiwa_hub/tests", "apps/heiwa_trading/tests", "scripts/tests"]
pythonpath = ["apps/heiwa_trading/src", "scripts"]
python_files = "test_*.py"
python_functions = "test_*"
addopts = "--tb=short -q"
```

- [ ] **Step 2: Commit change**
      `git add pyproject.toml && git commit -m "chore: make pyproject.toml dependency authority"`

### Task 2: Verify and Sync

- [ ] **Step 1: Run `uv sync`**
      Note: This might fail in sandbox if network is needed for resolution, but we can try to resolve from cache if possible.

- [ ] **Step 2: Run a smoke test**
      `uv run pytest scripts/tests/test_workspace_roots.py`
