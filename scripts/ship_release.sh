#!/usr/bin/env bash
# Ship a reviewed pull request into dev all the way to a published release,
# following docs/publishing.md "Release tagging and sequence". Each stage waits
# for its gate and the script stops at the first failure, so a partial run is
# safe to inspect and resume by hand.
#
#   1. wait for the PR's checks, merge it into dev
#   2. open and merge the dev -> main promotion
#   3. open the main -> dev synchronization
#   4. wait for main push CI and certification at the promoted commit
#   5. merge the synchronization
#   6. deploy the public installer and verify the served pin
#   7. tag v<version>, dispatch release.yml, wait for every job
#   8. dispatch pages.yml
#
# Usage: scripts/ship_release.sh <pr-number> <version> [developer-id|ad-hoc]
set -uo pipefail

pr="${1:?pull request number into dev}"
version="${2:?stable version, e.g. 0.3.1}"
signing="${3:-ad-hoc}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version must look like 0.3.1" >&2; exit 2; }
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 2

log() { echo "[$(date -u +%H:%M:%SZ)] $*"; }
die() { log "FAIL: $*"; exit 1; }
attribution=$'\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)'

wait_checks() {
  local summary
  sleep 30
  for _ in $(seq 1 120); do
    summary="$(gh pr checks "$1" --json bucket --jq \
      '[.[]|.bucket]|{p:(map(select(.=="pending"))|length),f:(map(select(.=="fail" or .=="cancel"))|length),n:length}' 2>/dev/null)"
    if grep -q '"n":0' <<<"$summary"; then sleep 20; continue; fi
    if grep -q '"p":0' <<<"$summary"; then grep -q '"f":0' <<<"$summary"; return; fi
    sleep 30
  done
  return 1
}

merge() {
  wait_checks "$1" || die "checks failed on #$1: $(gh pr checks "$1" --json name,bucket --jq '[.[]|select(.bucket=="fail")|.name]|join(",")')"
  gh pr merge "$1" --merge >/dev/null || die "merge #$1"
  git fetch -q origin
  log "merged #$1"
}

watch_run() { # workflow sha
  local id=""
  for _ in $(seq 1 20); do
    id="$(gh run list --workflow "$1" --branch main --event push --json databaseId,headSha \
      --jq ".[]|select(.headSha==\"$2\")|.databaseId" | head -n 1)"
    [[ -n "$id" ]] && break
    sleep 15
  done
  [[ -n "$id" ]] || die "no $1 run for $2"
  gh run watch "$id" --exit-status --interval 30 >/dev/null 2>&1 || die "$1 failed ($id)"
  log "$1 ok ($id)"
}

latest_run() { gh run list --workflow "$1" --limit 1 --json databaseId --jq '.[0].databaseId'; }

gh pr update-branch "$pr" >/dev/null 2>&1 || true
merge "$pr"

promotion="$(gh pr create --base main --head dev --title "Promote v$version to main" \
  --body "Promote dev $(git rev-parse --short origin/dev) for v$version.$attribution" | tail -n 1)"
promotion="${promotion##*/}"
log "promotion #$promotion"
merge "$promotion"
main_sha="$(git rev-parse origin/main)"
log "main $main_sha"

sync="$(gh pr create --base dev --head main --title "Synchronize dev with v$version promotion" \
  --body "Synchronize the promotion merge back into dev (trees identical).$attribution" | tail -n 1)"
sync="${sync##*/}"
log "sync #$sync"
sleep 25
watch_run certification.yml "$main_sha"
watch_run ci.yml "$main_sha"
merge "$sync"

gh workflow run deploy.yml --ref main >/dev/null && sleep 15
deploy="$(latest_run deploy.yml)"
gh run watch "$deploy" --exit-status --interval 20 >/dev/null 2>&1 || die "deploy failed ($deploy)"
bash scripts/check_installer_version_pin.sh --served "$version" >/dev/null || die "served installer pin is not $version"
log "edge serves $version"

git tag -a "v$version" "$main_sha" -m "Heiwa v$version" || die "tag v$version"
git push -q origin "v$version" || die "push v$version"
log "tagged v$version"
gh workflow run release.yml --ref main -f tag="v$version" -f macos_signing="$signing" >/dev/null && sleep 15
release="$(latest_run release.yml)"
log "release run $release"
gh run watch "$release" --exit-status --interval 60 >/dev/null 2>&1
conclusion="$(gh run view "$release" --json conclusion --jq .conclusion)"
gh run view "$release" --json jobs --jq '.jobs[]|"  \(.name): \(.conclusion)"'
[[ "$conclusion" == success ]] || die "release $conclusion ($release)"
log "RELEASE v$version SUCCESS"

gh workflow run pages.yml --ref main >/dev/null && log "pages dispatched"
