#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
audit_script="$repo_root/scripts/audit_operator_machine.sh"
fake_bin="$(mktemp -d /tmp/heiwa-audit-test.XXXXXX)"
trap 'find "$fake_bin" -depth -delete' EXIT

write_version_stub() {
  local name="$1"
  local version="$2"
  local path="$fake_bin/$name"

  printf '#!/usr/bin/env bash\nprintf '\''%%s\\n'\'' %q\n' "$version" >"$path"
  chmod +x "$path"
}

write_version_stub rustc "rustc 1.95.0 (test)"
write_version_stub cargo "cargo 1.95.0 (test)"
write_version_stub node "v22.22.3"
write_version_stub npm "10.9.8"
write_version_stub python3 "Python 3.14.5"
write_version_stub uv "uv 0.11.3"
write_version_stub brew "Homebrew 6.0.13"
write_version_stub gh "gh version 2.96.0"
write_version_stub wrangler "4.94.0"
write_version_stub pnpm "10.33.0"
write_version_stub ollama "ollama version 0.24.0"
write_version_stub tailscale "1.96.4"

if output="$(PATH="$fake_bin:/usr/bin:/bin" bash "$audit_script" 2>&1)"; then
  echo "Expected audit to reject Node 22 when repo requires Node 26." >&2
  exit 1
fi

if [[ "$output" != *"Node runtime mismatch: expected 26.x, found v22.22.3"* ]]; then
  echo "Audit failed without the expected Node mismatch evidence." >&2
  printf '%s\n' "$output" >&2
  exit 1
fi

echo "Operator audit rejects a Node runtime outside the repo major."
