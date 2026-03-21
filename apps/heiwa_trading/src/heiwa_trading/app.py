"""Standalone ASGI app for the dedicated Heiwa Trading service."""

from __future__ import annotations

import os
import time

from fastapi import FastAPI
from fastapi.responses import FileResponse

from heiwa_trading.routes import WEB_DIR, router


app = FastAPI(title="Heiwa Trading")
app.include_router(router)


@app.get("/")
@app.head("/")
async def root():
    return FileResponse(WEB_DIR / "cockpit.html", media_type="text/html")


@app.get("/index.html")
async def index_page():
    return FileResponse(WEB_DIR / "cockpit.html", media_type="text/html")


@app.get("/health")
@app.head("/health")
async def health():
    return {
        "status": "alive",
        "service": "heiwa-trading",
        "routes_base": "/trading",
        "timestamp": time.time(),
    }


def start_trading_server():
    import uvicorn

    port = int(os.getenv("PORT", "8080"))
    uvicorn.run(app, host="0.0.0.0", port=port, log_level="info")


if __name__ == "__main__":
    start_trading_server()
