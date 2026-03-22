#!/usr/bin/env bash
# Direct Claude Code wrapper for Heiwa OAuth-backed strategy/review routes.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${HEIWA_WORKSPACE_ROOT:-$(cd "$SCRIPT_DIR/../../../../.." && pwd)}"
LOG_DIR="$ROOT/runtime/logs/claude"
mkdir -p "$LOG_DIR"

RUN_ID="$(date +%Y%m%d_%H%M%S)_$$"
LOG_FILE="$LOG_DIR/$RUN_ID.log"
PAYLOAD_FILE="$LOG_DIR/$RUN_ID.payload.txt"

if [[ $# -gt 0 ]]; then
    PAYLOAD="$1"
else
    PAYLOAD="$(cat)"
fi
echo "$PAYLOAD" > "$PAYLOAD_FILE"

{
    echo "=== CLAUDE EXEC START ==="
    echo "run_id: $RUN_ID"
    echo "timestamp: $(date -Iseconds)"
    echo "cwd: $ROOT"
    echo "payload_bytes: ${#PAYLOAD}"
    echo "---"
} >> "$LOG_FILE"

if ! command -v claude &>/dev/null; then
    echo "[ERR] claude not found in PATH" | tee -a "$LOG_FILE"
    exit 2
fi

MODEL="${HEIWA_ACTIVE_MODEL:-${CLAUDE_MODEL:-}}"
if [[ -n "$MODEL" && "$MODEL" == */* ]]; then
    MODEL="${MODEL#*/}"
fi
MODEL="${MODEL//./-}"
TIMEOUT_SEC="${CLAUDE_TIMEOUT:-900}"
PERMISSION_MODE="${HEIWA_CLAUDE_PERMISSION_MODE:-${CLAUDE_PERMISSION_MODE:-bypassPermissions}}"

case "$PERMISSION_MODE" in
    default|acceptEdits|bypassPermissions|dontAsk|plan|auto) ;;
    *)
        echo "[WARN] invalid Claude permission mode '$PERMISSION_MODE'; defaulting to bypassPermissions" | tee -a "$LOG_FILE"
        PERMISSION_MODE="bypassPermissions"
        ;;
esac

CMD=(claude -p "$PAYLOAD" --output-format text --permission-mode "$PERMISSION_MODE" --add-dir "$ROOT")
if [[ -n "$MODEL" ]]; then
    CMD+=(--model "$MODEL")
fi

# Claude Code refuses bypassPermissions as root — drop to a non-root user.
RUN_CMD=("${CMD[@]}")
if [[ "$(id -u)" == "0" ]]; then
    # Create heiwa user if it doesn't exist
    if ! id -u heiwa &>/dev/null; then
        useradd -m -s /bin/bash heiwa 2>/dev/null || true
    fi
    # Ensure heiwa user owns the workspace and config dirs
    chown -R heiwa:heiwa "$ROOT" 2>/dev/null || true
    chown -R heiwa:heiwa /root/.claude 2>/dev/null || true
    mkdir -p /home/heiwa/.claude
    cp -rn /root/.claude/* /home/heiwa/.claude/ 2>/dev/null || true
    chown -R heiwa:heiwa /home/heiwa/.claude 2>/dev/null || true
    # Pass through env vars needed by claude
    SUDO_ENV=()
    for var in CLAUDE_OAUTH_ACCESS_TOKEN CLAUDE_OAUTH_REFRESH_TOKEN CLAUDE_MODEL ANTHROPIC_API_KEY HOME PATH NODE_OPTIONS; do
        if [[ -n "${!var:-}" ]]; then
            SUDO_ENV+=("$var=${!var}")
        fi
    done
    SUDO_ENV+=("HOME=/home/heiwa")
    RUN_CMD=(env "${SUDO_ENV[@]}" su -s /bin/bash heiwa -c "cd \"$ROOT\" && $(printf '%q ' "${CMD[@]}")")
fi

set +e
if command -v timeout &>/dev/null; then
    (
        cd "$ROOT"
        timeout "$TIMEOUT_SEC" "${RUN_CMD[@]}"
    ) 2>&1 | tee -a "$LOG_FILE"
    EXIT_CODE=${PIPESTATUS[0]}
elif command -v gtimeout &>/dev/null; then
    (
        cd "$ROOT"
        gtimeout "$TIMEOUT_SEC" "${RUN_CMD[@]}"
    ) 2>&1 | tee -a "$LOG_FILE"
    EXIT_CODE=${PIPESTATUS[0]}
else
    echo "[WARN] timeout command not found; running without timeout" | tee -a "$LOG_FILE"
    (
        cd "$ROOT"
        "${RUN_CMD[@]}"
    ) 2>&1 | tee -a "$LOG_FILE"
    EXIT_CODE=${PIPESTATUS[0]}
fi
set -e

{
    echo "---"
    echo "model: ${MODEL:-default}"
    echo "permission_mode: $PERMISSION_MODE"
    echo "exit_code: $EXIT_CODE"
    echo "=== CLAUDE EXEC END ==="
} >> "$LOG_FILE"

exit "$EXIT_CODE"
