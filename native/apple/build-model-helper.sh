#!/bin/sh
# Builds heiwa-model for Apple silicon, macOS 12 or later, with FoundationModels weak-linked so
# older systems launch it and get an "unavailable" reply instead of a dyld failure. arm64 only:
# Apple Intelligence needs Apple silicon, and the server reports Intel hosts without launching.
# The only build of this helper: apps/server/scripts/cli.ts, scripts/dev-runner.ts and CI call it.
set -eu
[ $# -eq 1 ] || { echo "usage: $0 <output>" >&2; exit 2; }
weak=""
if [ -d "$(xcrun --show-sdk-path)/System/Library/Frameworks/FoundationModels.framework" ]; then
  weak="-Xlinker -weak_framework -Xlinker FoundationModels"
fi
# shellcheck disable=SC2086
exec xcrun swiftc -O -parse-as-library -target arm64-apple-macos12.0 $weak \
  "$(cd "$(dirname "$0")" && pwd)/HeiwaModel.swift" -o "$1"
