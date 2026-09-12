#!/usr/bin/env bash
# Import a Developer ID certificate into an ephemeral GitHub runner keychain.
set -euo pipefail
[[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_OS:-}" == macOS ]] || {
  echo 'Signing keychain setup requires an ephemeral macOS GitHub runner.' >&2
  exit 1
}
: "${APPLE_CERTIFICATE:?Apple Developer ID certificate is required}"
: "${APPLE_CERTIFICATE_PASSWORD:?Certificate password is required}"
: "${APPLE_SIGNING_IDENTITY:?Developer ID signing identity is required}"
[[ "$APPLE_SIGNING_IDENTITY" == 'Developer ID Application:'* ]] || {
  echo 'Public downloads require a Developer ID Application identity.' >&2
  exit 1
}
umask 077
certificate_path="$RUNNER_TEMP/heiwa-developer-id.p12"
keychain_path="$RUNNER_TEMP/heiwa-signing.keychain-db"
trap 'rm -f "$certificate_path"' EXIT
python3 - "$certificate_path" <<'PY'
import base64, os, sys
with open(sys.argv[1], 'wb') as handle:
    handle.write(base64.b64decode(os.environ['APPLE_CERTIFICATE'], validate=True))
PY
signing_password="$(openssl rand -hex 32)"
echo "::add-mask::$signing_password"
security create-keychain -p "$signing_password" "$keychain_path"
security set-keychain-settings -lut 7200 "$keychain_path"
security unlock-keychain -p "$signing_password" "$keychain_path"
security import "$certificate_path" -k "$keychain_path" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$signing_password" "$keychain_path" >/dev/null
security list-keychains -d user -s "$keychain_path"
security default-keychain -d user -s "$keychain_path"
