"""OpenClaw: Spawning/dispatch mechanism for HeiwaClaw."""
from heiwa_sdk.heiwaclaw.gateway import (
    OpenClaw,
    OpenClawDispatch,
    HeiwaClawGateway,    # backward compat
    HeiwaClawDispatch,   # backward compat
)

__all__ = ["OpenClaw", "OpenClawDispatch", "HeiwaClawGateway", "HeiwaClawDispatch"]
