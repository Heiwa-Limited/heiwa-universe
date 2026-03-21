#!/bin/bash
set -euo pipefail

cd /app || exit 1

export PYTHONPATH="/app/src:${PYTHONPATH:-}"

exec python -m heiwa_trading.app
