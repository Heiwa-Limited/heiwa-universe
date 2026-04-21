"""Heiwa Python sidecar.

Subprocess spoken to over stdio with JSONL frames. The Rust runtime spawns
this module and exchanges typed Request/Response messages. Handlers dispatch
to ML-ecosystem packages (LangGraph, LlamaIndex, Ragas).
"""

__version__ = "0.1.0"
