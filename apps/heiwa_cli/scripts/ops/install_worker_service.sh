#!/usr/bin/env bash
# Install the Heiwa Worker launchd service on macOS
set -euo pipefail

PLIST_NAME="ltd.heiwa.worker.plist"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
SOURCE_PLIST="$REPO_ROOT/infra/nodes/macos/$PLIST_NAME"
TARGET_DIR="$HOME/Library/LaunchAgents"
TARGET_PLIST="$TARGET_DIR/$PLIST_NAME"

echo "🚀 Installing Heiwa Worker as a persistent service..."

mkdir -p "$TARGET_DIR"

if [[ -f "$TARGET_PLIST" ]]; then
    echo "⚠️  Existing service found. Unloading..."
    launchctl unload "$TARGET_PLIST" || true
fi

cp "$SOURCE_PLIST" "$TARGET_PLIST"
chmod 644 "$TARGET_PLIST"

echo "✅ Loading service: $PLIST_NAME"
launchctl load "$TARGET_PLIST"

echo "✨ Heiwa Worker is now persistent and always-on."
echo "   Logs: tail -f ~/heiwa-universe/runtime/logs/worker_manager.log"
