#!/usr/bin/env bash
set -euo pipefail

example_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$example_dir/../.." && pwd)"
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/abar-knowledge-qa.XXXXXX")"
state_dir="$temp_dir/state"
webui_port="${ABAR_QA_WEBUI_PORT:-19102}"
model_port="${ABAR_QA_MODEL_PORT:-18195}"
abar="$root/target/debug/abar"
webui_pid=""
model_pid=""

export ABAR_STATE_DIR="$state_dir"
export ABAR_CONFIG="$example_dir/config.test.yaml"
export ABAR_MESSAGE_CONFIG="$example_dir/message.providers.test.yaml"
unset ABAR_ENDPOINT ABAR_TOKEN ABAR_WEBUI_VIEW

cleanup() {
  rc=$?
  ABAR_STATE_DIR="$state_dir" "$abar" server stop >/dev/null 2>&1 || true
  if [[ -n "$webui_pid" ]]; then
    kill "$webui_pid" >/dev/null 2>&1 || true
    wait "$webui_pid" 2>/dev/null || true
  fi
  if [[ -n "$model_pid" ]]; then
    kill "$model_pid" >/dev/null 2>&1 || true
    wait "$model_pid" 2>/dev/null || true
  fi
  if [[ "$rc" -ne 0 ]]; then
    tail -n 80 "$temp_dir/webui.log" 2>/dev/null || true
    tail -n 80 "$state_dir/server.log" 2>/dev/null || true
    tail -n 40 "$temp_dir/model.log" 2>/dev/null || true
  fi
  rm -rf "$temp_dir"
  exit "$rc"
}
trap cleanup EXIT

cargo build -q --manifest-path "$root/Cargo.toml" -p abar-cli -p abar-webui

cat >"$temp_dir/model.mjs" <<'NODE'
import http from "node:http";

const port = Number(process.argv[2]);
http.createServer((request, response) => {
  let body = "";
  request.on("data", (chunk) => { body += chunk; });
  request.on("end", () => {
    const input = JSON.parse(body || "{}");
    const output = {
      model: "qa-model-v1",
      choices: [{
        message: {
          content: "标准门店营业时间为 10:00–22:00。商场统一营业时间不一致时，以商场要求为准。"
        }
      }],
      usage: { prompt_tokens: 20, completion_tokens: 18, total_tokens: 38 }
    };
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify(output));
  });
}).listen(port, "127.0.0.1");
NODE
node "$temp_dir/model.mjs" "$model_port" >"$temp_dir/model.log" 2>&1 &
model_pid=$!
for _ in $(seq 1 50); do
  if (exec 3<>/dev/tcp/127.0.0.1/"${model_port}") 2>/dev/null; then break; fi
  sleep 0.1
done

"$abar" publish "$example_dir" --json >"$temp_dir/publish.json"
jq -e '
  .agent == "knowledge-qa-assistant"
  and .status == "current"
  and .scopeReviewRequired == false
' "$temp_dir/publish.json" >/dev/null
"$abar" server start --daemon >/dev/null

session="$("$abar" session create --agent knowledge-qa-assistant)"
session_id="$(jq -r .id <<<"$session")"
message="$(
  "$abar" message create "$session_id" \
    --input "标准门店营业时间是什么？"
)"
printf '%s' "$message" | jq -e '
  .state == "completed"
  and (.output | contains("10:00–22:00"))
  and (.output | contains("store-service.md") | not)
  and (.output | contains("依据") | not)
  and (.modelUsage.totalTokens == 38)
  and (.toolResults | length) == 0
  and (.composition.knowledge | length) >= 1
  and (.composition.knowledge | map(.documentId) | index("store-service.md") != null)
' >/dev/null

ABAR_WEBUI_ADDR="127.0.0.1:${webui_port}" \
  "$root/target/debug/abar-webui" \
  --agent knowledge-qa-assistant \
  --view "$example_dir/webui-view.yaml" \
  >"$temp_dir/webui.log" 2>&1 &
webui_pid=$!
for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:${webui_port}/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
curl -fsS "http://127.0.0.1:${webui_port}/api/view" \
  | jq -e '
    .binding.agent == "knowledge-qa-assistant"
    and .metadata.title == "知识问答助手"
    and .context.enabled == false
    and .approval.enabled == false
  ' >/dev/null

echo "knowledge-qa: grounded answer, knowledge evidence, and WebUI passed"
