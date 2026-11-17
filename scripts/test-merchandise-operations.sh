#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/abar-merchandise-operations.XXXXXX")"
state_dir="$temp_dir/state"
webui_port="${ABAR_MERCHANDISE_WEBUI_PORT:-19098}"
secondary_webui_port="${ABAR_MERCHANDISE_SECONDARY_WEBUI_PORT:-$((webui_port + 1))}"
mock_port="${ABAR_MERCHANDISE_MOCK_PORT:-18191}"
model_port="${ABAR_MERCHANDISE_MODEL_PORT:-18192}"
abar="$root/target/debug/abar"
mock_pid=""
model_pid=""
webui_pid=""
secondary_webui_pid=""
secondary_agent="secondary-assistant"

export ABAR_STATE_DIR="$state_dir"
export ABAR_CONFIG="$root/examples/merchandise-operations/config.fake.yaml"
export ABAR_MESSAGE_CONFIG="$root/examples/merchandise-operations/message.providers.yaml"
unset ABAR_ENDPOINT ABAR_TOKEN

cleanup() {
  rc=$?
  ABAR_STATE_DIR="$state_dir" "$abar" server stop >/dev/null 2>&1 || true
  if [[ -n "$webui_pid" ]]; then
    kill "$webui_pid" >/dev/null 2>&1 || true
    wait "$webui_pid" 2>/dev/null || true
  fi
  if [[ -n "$secondary_webui_pid" ]]; then
    kill "$secondary_webui_pid" >/dev/null 2>&1 || true
    wait "$secondary_webui_pid" 2>/dev/null || true
  fi
  if [[ -n "$mock_pid" ]]; then
    kill "$mock_pid" >/dev/null 2>&1 || true
    wait "$mock_pid" 2>/dev/null || true
  fi
  if [[ -n "$model_pid" ]]; then
    kill "$model_pid" >/dev/null 2>&1 || true
    wait "$model_pid" 2>/dev/null || true
  fi
  if [[ "$rc" -ne 0 ]]; then
    tail -n 80 "$temp_dir/webui.log" 2>/dev/null || true
    tail -n 80 "$temp_dir/secondary-webui.log" 2>/dev/null || true
    tail -n 80 "$state_dir/server.log" 2>/dev/null || true
    tail -n 40 "$temp_dir/mock.log" 2>/dev/null || true
    tail -n 40 "$temp_dir/model.log" 2>/dev/null || true
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
sed \
  -e "s|127.0.0.1:18081|127.0.0.1:${model_port}|" \
  "$root/examples/merchandise-operations/config.fake.yaml" \
  >"$temp_dir/config.fake.yaml"
export ABAR_CONFIG="$temp_dir/config.fake.yaml"
secondary_source="$temp_dir/secondary-operations"
cp -R "$root/examples/merchandise-operations" "$secondary_source"
sed \
  -e "s/name: merchandise-operations-assistant/name: ${secondary_agent}/" \
  "$root/examples/merchandise-operations/agent.yaml" \
  >"$secondary_source/agent.yaml"

cargo build -q --manifest-path "$root/Cargo.toml" \
  -p abar-cli -p abar-webui -p abar-mock-mcp-server -p abar-merchandise-demo-model

ABAR_MOCK_MCP_ADDR="127.0.0.1:${mock_port}" \
  "$root/target/debug/mock-mcp-server" >"$temp_dir/mock.log" 2>&1 &
mock_pid=$!
for _ in $(seq 1 50); do
  if curl -fsS "http://127.0.0.1:${mock_port}/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

ABAR_DEMO_MODEL_ADDR="127.0.0.1:${model_port}" \
  "$root/target/debug/merchandise-demo-model" >"$temp_dir/model.log" 2>&1 &
model_pid=$!
for _ in $(seq 1 50); do
  if curl -fsS "http://127.0.0.1:${model_port}/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

"$abar" publish "$root/examples/merchandise-operations" >/dev/null
"$abar" publish "$secondary_source" >/dev/null
"$abar" server start --daemon >/dev/null

ABAR_WEBUI_ADDR="127.0.0.1:${webui_port}" \
  "$root/target/debug/abar-webui" \
  --agent merchandise-operations-assistant \
  --view "$root/views/merchandise-operations/webui-view.yaml" \
  >"$temp_dir/webui.log" 2>&1 &
webui_pid=$!
for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:${webui_port}/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

view="$(curl -fsS "http://127.0.0.1:${webui_port}/api/view")"
printf '%s' "$view" | jq -e \
  '.metadata.title == "商品运营助手"
   and .binding.agent == "merchandise-operations-assistant"' >/dev/null
page="$(curl -fsS "http://127.0.0.1:${webui_port}/")"
grep -q '/assets/webui.css' <<<"$page"
grep -q '/assets/webui.js' <<<"$page"
grep -Eq '/assets/webui.css\?v=[0-9a-f]{16}' <<<"$page"
grep -Eq '/assets/webui.js\?v=[0-9a-f]{16}' <<<"$page"
grep -q 'class="context-rail"' <<<"$page"
grep -q 'id="taskForm"' <<<"$page"
grep -q 'id="appShell"' <<<"$page"
grep -q 'id="contextToggle"' <<<"$page"
grep -q 'id="floatingLauncher"' <<<"$page"
grep -q 'id="floatingComposer"' <<<"$page"
grep -q 'id="floatingConversation"' <<<"$page"
grep -q 'id="floatingQuickAdd"' <<<"$page"
curl -fsS "http://127.0.0.1:${webui_port}/assets/webui.css" >/dev/null
curl -fsS "http://127.0.0.1:${webui_port}/assets/webui.js" >/dev/null
css_headers="$(curl -fsS -D - -o /dev/null \
  "http://127.0.0.1:${webui_port}/assets/webui.css")"
grep -qi '^cache-control: no-cache, must-revalidate' <<<"$css_headers"
css_etag="$(awk 'BEGIN{IGNORECASE=1} /^etag:/ {sub(/^[^:]+:[[:space:]]*/, ""); sub(/\r$/, ""); print; exit}' <<<"$css_headers")"
test -n "$css_etag"
test "$(curl -sS -o /dev/null -w '%{http_code}' \
  -H "If-None-Match: ${css_etag}" \
  "http://127.0.0.1:${webui_port}/assets/webui.css")" = "304"
headers="$(curl -fsSI "http://127.0.0.1:${webui_port}/")"
grep -qi '^content-security-policy:.*script-src '\''self'\''' <<<"$headers"
if grep -qi 'unsafe-inline' <<<"$headers"; then
  echo "WebUI CSP still permits unsafe-inline" >&2
  exit 1
fi

bad_view="$temp_dir/mismatched-view.yaml"
sed \
  -e "s/agent: merchandise-operations-assistant/agent: another-agent/" \
  "$root/views/merchandise-operations/webui-view.yaml" \
  >"$bad_view"
if ABAR_WEBUI_ADDR="127.0.0.1:$((secondary_webui_port + 1))" \
  "$root/target/debug/abar-webui" \
  --agent merchandise-operations-assistant \
  --view "$bad_view" >"$temp_dir/mismatched-view.log" 2>&1; then
  echo "WebUI accepted a View bound to another Agent" >&2
  exit 1
fi
grep -q "bound to another-agent" "$temp_dir/mismatched-view.log"

unset ABAR_WEBUI_VIEW
ABAR_WEBUI_ADDR="127.0.0.1:${secondary_webui_port}" \
  "$root/target/debug/abar-webui" \
  --agent "$secondary_agent" \
  >"$temp_dir/secondary-webui.log" 2>&1 &
secondary_webui_pid=$!
for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:${secondary_webui_port}/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
secondary_view="$(curl -fsS "http://127.0.0.1:${secondary_webui_port}/api/view")"
printf '%s' "$secondary_view" | jq -e \
  --arg agent "$secondary_agent" \
  '.metadata.name == "default"
   and .metadata.title == $agent
   and .binding.agent == $agent
   and (.task.actions | length) == 0' >/dev/null
secondary_page="$(curl -fsS "http://127.0.0.1:${secondary_webui_port}/")"
test "$secondary_page" = "$page"

wait_state() {
  local session_id="$1"
  local message_id="$2"
  local expected="$3"
  local target_port="${4:-$webui_port}"
  local detail=""
  for _ in $(seq 1 900); do
    detail="$(curl -fsS \
      "http://127.0.0.1:${target_port}/api/sessions/${session_id}?messageId=${message_id}")"
    if jq -e --arg state "$expected" '.state == $state' <<<"$detail" >/dev/null; then
      printf '%s' "$detail"
      return 0
    fi
    sleep 0.1
  done
  printf '%s' "$detail"
  return 1
}

run_business() {
  local plan_prompt="$1"
  local tool_id="$2"
  local order_tool="$3"
  local order_prefix="$4"
  local plan_type="$5"
  local agent="merchandise-operations-assistant"
  local request_id="test-${plan_type}"

  local created
  created="$(curl -fsS "http://127.0.0.1:${webui_port}/api/sessions" \
    -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg requestId "$request_id" --arg input "$plan_prompt" \
      '{requestId:$requestId,input:$input}')")"
  local session_id message_id
  session_id="$(jq -r .sessionId <<<"$created")"
  message_id="$(jq -r .messageId <<<"$created")"
  local reused
  reused="$(curl -fsS "http://127.0.0.1:${webui_port}/api/sessions" \
    -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg requestId "$request_id" --arg input "$plan_prompt" \
      '{requestId:$requestId,input:$input}')")"
  test "$(jq -r .sessionId <<<"$reused")" = "$session_id"
  test "$(jq -r .messageId <<<"$reused")" = "$message_id"
  local conflict_status
  conflict_status="$(curl -sS -o /dev/null -w '%{http_code}' \
    "http://127.0.0.1:${webui_port}/api/sessions" \
    -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg requestId "$request_id" \
      '{requestId:$requestId,input:"different request"}')")"
  test "$conflict_status" = "409"
  local detail
  detail="$(wait_state "$session_id" "$message_id" completed)"
  printf '%s' "$detail" | jq -e --arg tool "$tool_id" --arg type "$plan_type" \
    '(.toolResults | map(.toolId) | index($tool) != null)
     and (.toolResults[0].result.planType == $type)
     and (.toolResults[0].result.snapshotId != null)' >/dev/null
  local cursor
  cursor="$(printf '%s' "$detail" | jq -r .eventCursor)"
  curl -fsS \
    "http://127.0.0.1:${webui_port}/api/sessions/${session_id}?messageId=${message_id}&eventAfter=${cursor}" \
    | jq -e --argjson cursor "$cursor" \
      '(.events | length) == 0 and .eventCursor == $cursor' >/dev/null
  local live_stream
  live_stream="$(curl -sS -N --max-time 2 \
    -H 'Accept: text/event-stream' \
    "http://127.0.0.1:${webui_port}/api/sessions/${session_id}/messages/${message_id}/stream?after=0" \
    2>/dev/null || true)"
  grep -q '^id: [0-9][0-9]*' <<<"$live_stream"
  grep -q '^event: runtime' <<<"$live_stream"
  grep -q '^event: message' <<<"$live_stream"
  local stream
  stream="$(curl -sS -N --max-time 2 \
    -H 'Accept: text/event-stream' \
    -H "Last-Event-ID: ${cursor}" \
    "http://127.0.0.1:${webui_port}/api/sessions/${session_id}/messages/${message_id}/stream" \
    2>/dev/null || true)"
  grep -q '^event: message' <<<"$stream"
  local resumed_stream
  resumed_stream="$(curl -sS -N --max-time 1 \
    -H 'Accept: text/event-stream' \
    -H 'Last-Event-ID: 0' \
    "http://127.0.0.1:${webui_port}/api/sessions/${session_id}/messages/${message_id}/stream?after=${cursor}" \
    2>/dev/null || true)"
  grep -q '^event: runtime' <<<"$resumed_stream"
  curl -fsS "http://127.0.0.1:${webui_port}/api/sessions" \
    | jq -e --arg session "$session_id" \
      '.sessions[] | select(.id == $session)
       | .latestMessageId != null and .latestMessageState == "completed"
         and .latestMessageInput != null and .messageCount >= 1' \
      >/dev/null

  local confirmed
  confirmed="$(curl -fsS \
    "http://127.0.0.1:${webui_port}/api/sessions/${session_id}/messages/${message_id}/confirm" \
    -H 'Content-Type: application/json' \
    -d "$(printf '%s' "$detail" | jq -c '{
      confirmation:.toolResults[0].result.confirmation,
      agentRevision:.agentRevision
    }')")"
  message_id="$(jq -r .messageId <<<"$confirmed")"
  detail="$(wait_state "$session_id" "$message_id" waiting_approval)"
  printf '%s' "$detail" | jq -e '
    all(.messages[];
      (.input | startswith("[structured confirmation]") | not)
    )
  ' >/dev/null
  curl -fsS "http://127.0.0.1:${webui_port}/api/sessions" \
    | jq -e --arg session "$session_id" '
      .sessions[] | select(.id == $session)
      | (.latestMessageInput | startswith("[structured confirmation]") | not)
    ' >/dev/null
  local approval_id
  approval_id="$(jq -r .pendingApproval.approvalId <<<"$detail")"
  curl -fsS \
    "http://127.0.0.1:${webui_port}/api/sessions/${session_id}/messages/${message_id}/approval" \
    -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg approval "$approval_id" \
      '{approvalId:$approval,decision:"approved",scopes:["华东"],comment:"workspace test"}')" \
    >/dev/null
  detail="$(wait_state "$session_id" "$message_id" completed)"
  printf '%s' "$detail" | jq -e --arg tool "$order_tool" --arg prefix "$order_prefix" \
    '.toolResults | any(.toolId == $tool and (.result.orderId | startswith($prefix)))' >/dev/null
  jq -n --arg agent "$agent" --arg sessionId "$session_id" --arg messageId "$message_id" \
    --arg orderId "$(printf '%s' "$detail" | jq -r --arg tool "$order_tool" \
      '.toolResults[] | select(.toolId == $tool) | .result.orderId' | tail -1)" \
    '{agent:$agent,sessionId:$sessionId,messageId:$messageId,orderId:$orderId}'
}

ambiguous="$(
  curl -fsS "http://127.0.0.1:${webui_port}/api/sessions" \
    -H 'Content-Type: application/json' \
    -d '{"requestId":"test-ambiguous","input":"处理一下库存问题"}'
)"
ambiguous_detail="$(wait_state \
  "$(jq -r .sessionId <<<"$ambiguous")" \
  "$(jq -r .messageId <<<"$ambiguous")" completed)"
printf '%s' "$ambiguous_detail" | jq -e \
  '(.toolResults | length) == 0
   and (.output | contains("请明确"))' >/dev/null

allocation_result="$(run_business \
  '生成华东区域本周直营门店配货计划。' \
  inventory.calculate_allocation_plan \
  inventory.create_allocation_order \
  MOCK-AL allocation)"

run_business \
  '生成华东区域本周直营门店补货计划。' \
  inventory.calculate_replenishment_plan \
  inventory.create_replenishment_order \
  MOCK-RP replenishment

run_business \
  '生成华东区域本周门店间调拨建议。' \
  inventory.calculate_transfer_recommendations \
  inventory.create_transfer_order \
  MOCK-TR transfer

allocation_session="$(jq -r .sessionId <<<"$allocation_result")"
allocation_message="$(jq -r .messageId <<<"$allocation_result")"
cross_status="$(curl -sS -o /dev/null -w '%{http_code}' \
  "http://127.0.0.1:${secondary_webui_port}/api/sessions/${allocation_session}")"
test "$cross_status" = "403"
cross_append_status="$(curl -sS -o /dev/null -w '%{http_code}' \
  "http://127.0.0.1:${secondary_webui_port}/api/sessions/${allocation_session}/messages" \
  -H 'Content-Type: application/json' \
  -d '{"input":"cross-agent mutation"}')"
test "$cross_append_status" = "403"
cross_cancel_status="$(curl -sS -o /dev/null -w '%{http_code}' \
  -X POST \
  "http://127.0.0.1:${secondary_webui_port}/api/messages/${allocation_message}/cancel")"
test "$cross_cancel_status" = "403"
cross_stream_status="$(curl -sS -o /dev/null -w '%{http_code}' \
  "http://127.0.0.1:${secondary_webui_port}/api/sessions/${allocation_session}/messages/invalid/stream")"
test "$cross_stream_status" = "403"

secondary_created="$(curl -fsS "http://127.0.0.1:${secondary_webui_port}/api/sessions" \
  -H 'Content-Type: application/json' \
  -d '{"requestId":"secondary-isolation","input":"处理一下库存问题"}')"
secondary_session="$(jq -r .sessionId <<<"$secondary_created")"
secondary_message="$(jq -r .messageId <<<"$secondary_created")"
secondary_detail="$(wait_state \
  "$secondary_session" "$secondary_message" completed "$secondary_webui_port")"
printf '%s' "$secondary_detail" | jq -e --arg agent "$secondary_agent" \
  '.agent == $agent' >/dev/null
primary_cross_status="$(curl -sS -o /dev/null -w '%{http_code}' \
  "http://127.0.0.1:${webui_port}/api/sessions/${secondary_session}")"
test "$primary_cross_status" = "403"

test "$(curl -fsS "http://127.0.0.1:${mock_port}/healthz" | jq -r .orders)" = 3
echo "webui: multi-Agent isolation, default View, SSE, and merchandise operations passed"
