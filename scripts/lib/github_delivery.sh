#!/usr/bin/env bash
# Shared protected-delivery checks. No admin bypass, auto-merge or latest-run
# fallback: missing/ambiguous evidence stops the caller.

github_delivery_error() { printf 'Delivery stopped: %s\n' "$*" >&2; return 1; }

github_assert_pr() { # repository number expected-head expected-base
  local snapshot
  snapshot="$(gh pr view "$2" --repo "$1" --json \
    headRefOid,baseRefName,state,isDraft,mergeStateStatus,mergeable,reviewDecision)" || return 1
  jq -e --arg head "$3" --arg base "$4" '
    .headRefOid == $head and .baseRefName == $base and .state == "OPEN" and
    .isDraft == false and .mergeStateStatus == "CLEAN" and .mergeable == "MERGEABLE" and
    (.reviewDecision != "CHANGES_REQUESTED" and .reviewDecision != "REVIEW_REQUIRED")
  ' <<<"$snapshot" >/dev/null || github_delivery_error "#$2 head/base or merge/review state changed; reverify before retrying"
}

github_merge_pr() { # repository number expected-head expected-base
  local repo="$1" pr="$2" head="$3" base="$4" checks threads result
  [[ "$pr" =~ ^[1-9][0-9]*$ && "$head" =~ ^[0-9a-f]{40}$ ]] || {
    github_delivery_error 'merge requires a PR number and full verified head SHA'; return 1;
  }
  github_assert_pr "$repo" "$pr" "$head" "$base" || return 1
  checks="$(gh pr checks "$pr" --repo "$repo" --required --json name,bucket)" || return 1
  jq -e 'type == "array" and length > 0 and all(.[];
    .bucket == "pass" or .bucket == "skipping")' <<<"$checks" >/dev/null || {
    github_delivery_error "#$pr required checks are absent or not passing"; return 1;
  }
  # GraphQL variables must reach gh literally, rather than expand in the shell.
  # shellcheck disable=SC2016
  threads="$(gh api graphql --paginate --slurp \
    -F owner="${repo%/*}" -F name="${repo#*/}" -F number="$pr" -f query='
    query($owner:String!, $name:String!, $number:Int!, $endCursor:String) {
      repository(owner:$owner, name:$name) {
        pullRequest(number:$number) {
          reviewThreads(first:100, after:$endCursor) {
            nodes { id isResolved }
            pageInfo { hasNextPage endCursor }
          }
        }
      }
    }')" || return 1
  jq -e 'type == "array" and length > 0 and
    all(.[]; .errors == null and
      (.data.repository.pullRequest.reviewThreads.nodes | type == "array") and
      all(.data.repository.pullRequest.reviewThreads.nodes[]; .isResolved == true)) and
    .[-1].data.repository.pullRequest.reviewThreads.pageInfo.hasNextPage == false
  ' <<<"$threads" >/dev/null || {
    github_delivery_error "#$pr has unresolved threads or incomplete review evidence"; return 1;
  }
  # Head may have moved while checks or paginated reviews were being read.
  github_assert_pr "$repo" "$pr" "$head" "$base" || return 1
  gh pr merge "$pr" --repo "$repo" --merge --match-head-commit "$head" || return 1
  result="$(gh pr view "$pr" --repo "$repo" --json state,headRefOid)" || return 1
  jq -e --arg head "$head" '.state == "MERGED" and .headRefOid == $head' \
    <<<"$result" >/dev/null || github_delivery_error "#$pr merged result could not be verified"
}

github_dispatch_run() { # repository workflow expected-main-sha [workflow inputs]
  local repo="$1" workflow="$2" head="$3" main_head workflow_id output run_id run pattern
  shift 3
  main_head="$(gh api "repos/$repo/commits/main" --jq .sha)" || return 1
  [[ "$main_head" == "$head" ]] || {
    github_delivery_error 'main advanced; renew release/deploy evidence before dispatch'; return 1;
  }
  workflow_id="$(gh api "repos/$repo/actions/workflows/$workflow" --jq .id)" || return 1
  [[ "$workflow_id" =~ ^[1-9][0-9]*$ ]] || return 1
  output="$(gh workflow run "$workflow" --repo "$repo" --ref main "$@")" || return 1
  pattern="https://github\\.com/${repo}/actions/runs/([0-9]+)"
  [[ "$output" =~ $pattern ]] || {
    github_delivery_error 'dispatch returned no run URL; inspect the dispatch manually, never select latest'; return 1;
  }
  run_id="${BASH_REMATCH[1]}"
  run="$(gh run view "$run_id" --repo "$repo" --json headSha,event,workflowDatabaseId,headBranch)" || return 1
  jq -e --arg head "$head" --argjson workflow "$workflow_id" '
    .headSha == $head and .event == "workflow_dispatch" and
    .workflowDatabaseId == $workflow and .headBranch == "main"
  ' <<<"$run" >/dev/null || {
    github_delivery_error "dispatch run $run_id does not match the certified workflow/revision"; return 1;
  }
  printf '%s\n' "$run_id"
}
