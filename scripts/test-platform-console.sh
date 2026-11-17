#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/abar-platform-console.XXXXXX")"
state_dir="$temp_dir/state"
platform_port="${ABAR_PLATFORM_TEST_PORT:-19097}"
mock_port="${ABAR_PLATFORM_TEST_MOCK_PORT:-18193}"
abar="$root/target/debug/abar"
platform="$root/target/debug/abar-platform-console"
mock_pid=""
platform_pid=""

export ABAR_STATE_DIR="$state_dir"
export ABAR_CONFIG="$root/examples/merchandise-operations/config.fake.yaml"
export ABAR_MESSAGE_CONFIG="$root/examples/merchandise-operations/message.providers.yaml"
unset ABAR_ENDPOINT ABAR_TOKEN ABAR_PLATFORM_ADDR

cleanup() {
  rc=$?
  ABAR_STATE_DIR="$state_dir" "$abar" server stop >/dev/null 2>&1 || true
  if [[ -n "$platform_pid" ]]; then
    kill "$platform_pid" >/dev/null 2>&1 || true
    wait "$platform_pid" 2>/dev/null || true
  fi
  if [[ -n "$mock_pid" ]]; then
    kill "$mock_pid" >/dev/null 2>&1 || true
    wait "$mock_pid" 2>/dev/null || true
  fi
  if [[ "$rc" -ne 0 ]]; then
    tail -n 80 "$temp_dir/platform.log" 2>/dev/null || true
    tail -n 80 "$state_dir/server.log" 2>/dev/null || true
  fi
  rm -rf "$temp_dir"
  exit "$rc"
}
trap cleanup EXIT

sed \
  -e "s|18080/mcp|${mock_port}/mcp|" \
  -e "s|path: knowledge/allocation-policy|path: ${root}/examples/merchandise-operations/knowledge/allocation-policy|" \
  -e "s|path: knowledge/replenishment-policy|path: ${root}/examples/merchandise-operations/knowledge/replenishment-policy|" \
  -e "s|path: knowledge/transfer-policy|path: ${root}/examples/merchandise-operations/knowledge/transfer-policy|" \
  "$root/examples/merchandise-operations/message.providers.yaml" \
  >"$temp_dir/message.providers.yaml"
export ABAR_MESSAGE_CONFIG="$temp_dir/message.providers.yaml"

cargo build -q --manifest-path "$root/Cargo.toml" \
  -p abar-cli -p abar-platform-console -p abar-mock-mcp-server

ABAR_MOCK_MCP_ADDR="127.0.0.1:${mock_port}" \
  "$root/target/debug/mock-mcp-server" >"$temp_dir/mock.log" 2>&1 &
mock_pid=$!
for _ in $(seq 1 50); do
  if curl -fsS "http://127.0.0.1:${mock_port}/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

"$abar" publish "$root/examples/merchandise-operations" >/dev/null
"$abar" server start --daemon >/dev/null

candidate="$temp_dir/candidate"
cp -R "$root/examples/merchandise-operations" "$candidate"
perl -0pi -e 's/(tools:\n  read:\n)/$1    - inventory.review_*\n/' \
  "$candidate/scope.yaml"
"$abar" publish "$candidate" >/dev/null

ABAR_PLATFORM_ADDR="127.0.0.1:${platform_port}" \
  "$platform" >"$temp_dir/platform.log" 2>&1 &
platform_pid=$!
for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:${platform_port}/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

overview="$(curl -fsS "http://127.0.0.1:${platform_port}/api/overview")"
printf '%s' "$overview" | jq -e '
  .tenant == "local"
  and (.agents | length) == 1
  and .agents[0].agent == "merchandise-operations-assistant"
  and .agents[0].currentRevision == 1
  and .agents[0].revisionCount == 2
  and (.agents[0].revisions | map(select(.status == "waiting_review")) | length) == 1
  and .runtime.status == "ready"
' >/dev/null

curl -fsS \
  "http://127.0.0.1:${platform_port}/api/agents/merchandise-operations-assistant/revisions/2/review" \
  -H 'Content-Type: application/json' \
  -d '{"decision":"approved","comment":"workspace review"}' \
  | jq -e '.decision == "approved" and .candidateRevision == 2' >/dev/null

overview="$(curl -fsS "http://127.0.0.1:${platform_port}/api/overview")"
printf '%s' "$overview" | jq -e '
  .agents[0].currentRevision == 2
  and (.agents[0].revisions[] | select(.revision == 2) | .status) == "current"
' >/dev/null

page="$(curl -fsS "http://127.0.0.1:${platform_port}/")"
grep -q '/assets/platform.css' <<<"$page"
grep -q '/assets/platform.js' <<<"$page"
grep -q 'ABAR Platform Console' <<<"$page"

echo "platform-console: catalog, Scope Review and Runtime overview passed"
