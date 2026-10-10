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
# Usage: scripts/ship_release.sh <pr-number> <version> <verified-head-sha> [developer-id|ad-hoc]
# Local gates, meaningful value and applicable device proof must already be
# reviewed for that exact head. This script never renews proof by updating a PR.
set -euo pipefail

pr="${1:?pull request number into dev}"
version="${2:?stable version, e.g. 0.3.1}"
verified_head="${3:?full head SHA covered by the reviewed verification receipts}"
signing="${4:-ad-hoc}"
[[ "$pr" =~ ^[1-9][0-9]*$ && "$verified_head" =~ ^[0-9a-f]{40}$ ]] || { echo "PR number and full verified head SHA required" >&2; exit 2; }
[[ "$signing" == ad-hoc || "$signing" == developer-id ]] || { echo "unsupported signing lane" >&2; exit 2; }
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version must look like 0.3.1" >&2; exit 2; }
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 2
source scripts/lib/github_delivery.sh
repo=Heiwa-Limited/heiwa-universe
command -v jq >/dev/null || { echo "jq is required" >&2; exit 2; }
[[ "$(git rev-parse HEAD)" == "$verified_head" && -z "$(git status --porcelain)" ]] || {
  echo "start from the clean checkout at the verified PR head" >&2; exit 2;
}

log() { echo "[$(date -u +%H:%M:%SZ)] $*"; }
die() { log "FAIL: $*"; exit 1; }

verify_dev() { # Fresh clean dev proof, without touching the operator checkout.
  local expected="$1" proof_root verification receipt
  proof_root="$(mktemp -d "${TMPDIR:-/tmp}/heiwa-ship-dev.XXXXXX")" || return 1
  verification="$proof_root/repo"
  # The subshell owns this disposable clone. Durable receipt/logs are copied
  # before cleanup, including failures; unrelated worktrees are never changed.
  (
    trap 'rm -rf "$proof_root"' EXIT
    git clone --quiet --shared --no-checkout "$PWD" "$verification" || exit 1
    git -C "$verification" remote set-url origin "$(git remote get-url origin)" || exit 1
    git -C "$verification" fetch --quiet origin dev main || exit 1
    [[ "$(git -C "$verification" rev-parse origin/dev)" == "$expected" ]] || exit 1
    git -C "$verification" checkout --quiet -B dev "$expected" || exit 1
    cd "$verification" || exit 1
    [[ "$(git rev-parse HEAD)" == "$expected" && -z "$(git status --porcelain)" ]] || exit 1
    status=0
    HEIWA_BRANCH_MODE=integration bash scripts/check_ci_local.sh || status=$?
    receipt="${HEIWA_SHIP_RECEIPT_ROOT}/dev-$expected"
    mkdir -p "$receipt" || exit 1
    if [[ -d private/verification ]]; then
      cp -R private/verification/. "$receipt/" || exit 1
    fi
    printf 'Integration proof retained: %s\n' "$receipt"
    exit "$status"
  )
}
export HEIWA_SHIP_RECEIPT_ROOT="$PWD/private/verification/ship-release"

wait_checks() {
  local summary
  sleep 30
  for _ in $(seq 1 120); do
    summary="$(gh pr checks "$1" --repo "$repo" --json bucket --jq \
      '[.[]|.bucket]|{p:(map(select(.=="pending"))|length),f:(map(select(.=="fail" or .=="cancel"))|length),n:length}' 2>/dev/null)"
    if grep -q '"n":0' <<<"$summary"; then sleep 20; continue; fi
    if grep -q '"p":0' <<<"$summary"; then grep -q '"f":0' <<<"$summary"; return; fi
    sleep 30
  done
  return 1
}

merge() { # PR verified-head target-branch
  [[ "$1" =~ ^[1-9][0-9]*$ && "$2" =~ ^[0-9a-f]{40}$ ]] || die "invalid PR/head; no merge attempted"
  wait_checks "$1" || die "checks failed or unavailable on #$1"
  github_merge_pr "$repo" "$1" "$2" "$3" >/dev/null || die "merge #$1"
  git fetch -q origin dev main || die "fetch after #$1"
  log "merged #$1 at verified head $2"
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

merge "$pr" "$verified_head" dev
dev_sha="$(gh pr view "$pr" --repo "$repo" --json mergeCommit --jq .mergeCommit.oid)"
[[ "$(git rev-parse origin/dev)" == "$dev_sha" ]] || die "dev advanced beyond the verified integration; reverify it"
verify_dev "$dev_sha" || die "clean exact-dev local gate failed"
git fetch -q origin dev main
[[ "$(git rev-parse origin/dev)" == "$dev_sha" ]] || die "dev advanced during its gate; reverify it"

body_file="$(mktemp "${TMPDIR:-/tmp}/heiwa-ship-pr.XXXXXX")"
trap 'rm -f "$body_file"' EXIT
printf 'Promote verified dev %s for v%s. The clean exact-dev local gate passed.\n\nSource checkpoint #%s:\n\n' \
  "$dev_sha" "$version" "$pr" >"$body_file"
gh pr view "$pr" --repo "$repo" --json title,body --jq '"\(.title)\n\n\(.body)"' >>"$body_file"
promotion="$(gh pr create --repo "$repo" --base main --head dev --title "Promote v$version to main" \
  --body-file "$body_file" | tail -n 1)"
promotion="${promotion##*/}"
log "promotion #$promotion"
merge "$promotion" "$dev_sha" main
main_sha="$(gh pr view "$promotion" --repo "$repo" --json mergeCommit --jq .mergeCommit.oid)"
[[ "$(git rev-parse origin/main)" == "$main_sha" ]] || die "main advanced beyond this promotion; reverify it"
log "main $main_sha"

printf 'Synchronize promotion %s back into dev (trees identical).\n' "$main_sha" >"$body_file"
sync="$(gh pr create --repo "$repo" --base dev --head main --title "Synchronize dev with v$version promotion" \
  --body-file "$body_file" | tail -n 1)"
sync="${sync##*/}"
log "sync #$sync"
sleep 25
watch_run certification.yml "$main_sha"
watch_run ci.yml "$main_sha"
merge "$sync" "$main_sha" dev

deploy="$(github_dispatch_run "$repo" deploy.yml "$main_sha")" || die "deploy dispatch"
gh run watch "$deploy" --exit-status --interval 20 >/dev/null 2>&1 || die "deploy failed ($deploy)"
bash scripts/check_installer_version_pin.sh --served "$version" >/dev/null || die "served installer pin is not $version"
log "edge serves $version"

git tag -a "v$version" "$main_sha" -m "Heiwa v$version" || die "tag v$version"
git push -q origin "v$version" || die "push v$version"
log "tagged v$version"
release="$(github_dispatch_run "$repo" release.yml "$main_sha" -f tag="v$version" -f macos_signing="$signing")" || die "release dispatch"
log "release run $release"
watch_status=0
gh run watch "$release" --exit-status --interval 60 >/dev/null 2>&1 || watch_status=$?
conclusion="$(gh run view "$release" --json conclusion --jq .conclusion)"
gh run view "$release" --json jobs --jq '.jobs[]|"  \(.name): \(.conclusion)"'
[[ "$watch_status" == 0 && "$conclusion" == success ]] || die "release $conclusion ($release)"
log "RELEASE v$version SUCCESS"

pages="$(github_dispatch_run "$repo" pages.yml "$main_sha")" || die "pages dispatch"
log "pages dispatched ($pages)"
