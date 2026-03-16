"""OpenAI Codex CLI Adapter."""
from pathlib import Path
from .cli_adapter import CLIAdapter

class CodexAdapter(CLIAdapter):
    def __init__(self, root_dir: Path):
        super().__init__(root_dir, "heiwa_code")
