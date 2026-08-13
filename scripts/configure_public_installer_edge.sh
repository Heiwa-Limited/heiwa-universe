#!/usr/bin/env bash
set -euo pipefail

api_base="https://api.cloudflare.com/client/v4"
zone_name="${HEIWA_PUBLIC_ZONE:-heiwa.ltd}"
rule_ref="heiwa_public_installer_access_v1"
rule_description="Heiwa public installer non-browser access"

rule_json() {
  jq -n \
    --arg host "$zone_name" \
    --arg ref "$rule_ref" \
    --arg description "$rule_description" \
    '{
      action: "skip",
      action_parameters: {
        phase: "current",
        phases: [
          "http_ratelimit",
          "http_request_firewall_managed",
          "http_request_sbfm"
        ],
        products: ["bic", "hot", "rateLimit", "securityLevel", "uaBlock", "waf", "zoneLockdown"]
      },
      expression: ("(http.host eq \"" + $host + "\" and http.request.uri.path in {\"/install\" \"/install.sh\"})"),
      description: $description,
      enabled: true,
      logging: {enabled: true},
      ref: $ref,
      position: {before: ""}
    }'
}

if [[ "${1:-}" == "--print-rule" ]]; then
  rule_json
  exit 0
fi

: "${CF_API_TOKEN:?CF_API_TOKEN is required}"
: "${CF_ACCOUNT_ID:?CF_ACCOUNT_ID is required}"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/heiwa-installer-edge.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

api_request() {
  local method="$1"
  local path="$2"
  local payload="${3:-}"
  local response_file="$tmp_dir/response.json"
  local status
  local -a curl_args=(
    --silent
    --show-error
    --request "$method"
    --header "Authorization: Bearer $CF_API_TOKEN"
    --header "Content-Type: application/json"
    --output "$response_file"
    --write-out '%{http_code}'
  )

  if [[ -n "$payload" ]]; then
    curl_args+=(--data "$payload")
  fi

  status="$(curl "${curl_args[@]}" "$api_base$path")"
  if [[ "$status" -lt 200 || "$status" -ge 300 ]] || ! jq -e '.success == true' "$response_file" >/dev/null; then
    echo "Cloudflare API request failed: $method $path (HTTP $status)" >&2
    jq -c '{errors, messages}' "$response_file" >&2 2>/dev/null || sed -n '1,20p' "$response_file" >&2
    return 1
  fi

  cat "$response_file"
}

probe_installer() {
  local headers="$tmp_dir/headers"
  local body="$tmp_dir/install"
  local status

  status="$(curl \
    --silent \
    --show-error \
    --location \
    --output "$body" \
    --dump-header "$headers" \
    --write-out '%{http_code}' \
    --user-agent 'Heiwa-Release-Smoke/1.0' \
    --header 'Accept: text/x-shellscript,text/plain;q=0.9,*/*;q=0.1' \
    "https://$zone_name/install")"

  if [[ "$status" == "200" ]] \
    && ! grep -Eiq '^cf-mitigated:[[:space:]]*challenge' "$headers" \
    && head -n 1 "$body" | grep -qx '#!/bin/sh'; then
    return 0
  fi

  echo "Installer edge probe failed (HTTP $status)." >&2
  grep -Ei '^(cf-ray|cf-mitigated|content-type|server):' "$headers" >&2 || true
  return 1
}

zones="$(api_request GET "/zones?name=$zone_name&account.id=$CF_ACCOUNT_ID&status=active")"
zone_count="$(jq '.result | length' <<<"$zones")"
if [[ "$zone_count" != "1" ]]; then
  echo "Expected one active $zone_name zone in the configured account; found $zone_count." >&2
  exit 1
fi
zone_id="$(jq -r '.result[0].id' <<<"$zones")"

rulesets="$(api_request GET "/zones/$zone_id/rulesets")"
ruleset_id="$(jq -r '[.result[] | select(.kind == "zone" and .phase == "http_request_firewall_custom")][0].id // empty' <<<"$rulesets")"
rule="$(rule_json)"

if [[ -z "$ruleset_id" ]]; then
  create_ruleset="$(jq -n \
    --arg description "Heiwa-managed zone security exceptions" \
    --argjson rule "${rule%$'\n'}" \
    '{name: "Heiwa zone custom firewall", description: $description, kind: "zone", phase: "http_request_firewall_custom", rules: [$rule | del(.position)]}')"
  created="$(api_request POST "/zones/$zone_id/rulesets" "$create_ruleset")"
  ruleset_id="$(jq -r '.result.id' <<<"$created")"
  echo "Created installer edge rule in ruleset $ruleset_id."
else
  ruleset="$(api_request GET "/zones/$zone_id/rulesets/$ruleset_id")"
  rule_id="$(jq -r --arg ref "$rule_ref" '.result.rules[]? | select(.ref == $ref) | .id' <<<"$ruleset" | head -n 1)"
  if [[ -n "$rule_id" ]]; then
    api_request PATCH "/zones/$zone_id/rulesets/$ruleset_id/rules/$rule_id" "$rule" >/dev/null
    echo "Updated installer edge rule $rule_id."
  else
    created="$(api_request POST "/zones/$zone_id/rulesets/$ruleset_id/rules" "$rule")"
    rule_id="$(jq -r '.result.id' <<<"$created")"
    echo "Created installer edge rule $rule_id."
  fi
fi

for attempt in 1 2 3; do
  if probe_installer; then
    echo "Public installer is curl-compatible."
    exit 0
  fi
  if [[ "$attempt" -lt 3 ]]; then
    sleep 3
  fi
done

# Cloudflare cannot exempt a path from free-plan Bot Fight Mode. Disable it only
# when it is confirmed active and the exact-path skip rule was insufficient.
bot_config="$(api_request GET "/zones/$zone_id/bot_management")"
if [[ "$(jq -r '.result.fight_mode // false' <<<"$bot_config")" == "true" ]]; then
  echo "Bot Fight Mode is active and cannot honor path exceptions; testing with it disabled."
  api_request PUT "/zones/$zone_id/bot_management" '{"fight_mode":false}' >/dev/null
  for attempt in 1 2 3 4; do
    if probe_installer; then
      echo "Public installer is curl-compatible; exact-path WAF exception remains logged."
      exit 0
    fi
    if [[ "$attempt" -lt 4 ]]; then
      sleep 3
    fi
  done
  echo "Bot Fight Mode was not the sole blocker; restoring it." >&2
  api_request PUT "/zones/$zone_id/bot_management" '{"fight_mode":true}' >/dev/null
fi

echo "Public installer remains inaccessible to non-browser clients." >&2
exit 1
