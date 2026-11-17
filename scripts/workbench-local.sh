#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
state_dir="${ABAR_WORKBENCH_STATE_DIR:-$root/.local/workbench}"
mock_port="${ABAR_WORKBENCH_MOCK_PORT:-18080}"
model_port="${ABAR_WORKBENCH_MODEL_PORT:-18081}"
keycloak_port="${ABAR_KEYCLOAK_LOCAL_PORT:-18093}"
webui_port="${ABAR_WORKBENCH_WEBUI_PORT:-19093}"
webui_url="http://127.0.0.1:${webui_port}"
public_webui_url="${ABAR_WORKBENCH_PUBLIC_URL:-$webui_url}"
platform_port="${ABAR_WORKBENCH_PLATFORM_PORT:-19094}"
platform_url="http://127.0.0.1:${platform_port}"
public_platform_url="${ABAR_WORKBENCH_PLATFORM_URL:-$platform_url}"
keycloak_url="http://127.0.0.1:${keycloak_port}"
public_keycloak_issuer="${ABAR_KEYCLOAK_PUBLIC_ISSUER:-${keycloak_url}/realms/abar}"
runtime_config_file="$state_dir/runtime-config.txt"
abar="$root/target/debug/abar"
webui_pid_file="$state_dir/webui.pid"
platform_pid_file="$state_dir/platform.pid"
mock_pid_file="$state_dir/mock.pid"
model_pid_file="$state_dir/model.pid"
command="${1:-status}"

export ABAR_STATE_DIR="$state_dir"
export ABAR_CONFIG="$root/examples/merchandise-operations/config.fake.yaml"
export ABAR_MESSAGE_CONFIG="$root/examples/merchandise-operations/message.providers.yaml"
export ABAR_OIDC_APPROVER_ROLE="transfer-approver"
export ABAR_OIDC_SCOPE_FIELD="requiredScopes"
export ABAR_OIDC_ALLOW_HTTP=true
export ABAR_CONTROL_PLANE_ROLE="transfer-approver"
unset ABAR_ENDPOINT ABAR_TOKEN

webui_issuer=""
webui_client_id=""
platform_issuer=""
platform_client_id=""

configure_keycloak() {
  bash "$root/scripts/keycloak-local.sh" start >/dev/null
  webui_issuer="$public_keycloak_issuer"
  webui_client_id="abar-test"
  platform_issuer="$public_keycloak_issuer"
  platform_client_id="abar-test"
  export ABAR_OIDC_USERINFO_ENDPOINT="${keycloak_url}/realms/abar/protocol/openid-connect/userinfo"
  export ABAR_OIDC_ISSUER="${keycloak_url}/realms/abar"
  export ABAR_OIDC_AUDIENCE="abar-api"
  export ABAR_OIDC_INTROSPECTION_ENDPOINT="${keycloak_url}/realms/abar/protocol/openid-connect/token/introspect"
  export ABAR_OIDC_CLIENT_ID="abar-server"
  export ABAR_OIDC_CLIENT_SECRET="abar-server-secret"
}

wait_http() {
  local url="$1"
  for _ in $(seq 1 120); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
  return 1
}

daemonize() {
  local pid_file="$1"
  local log_file="$2"
  shift 2
  perl -MPOSIX -e '
    my ($pid_file, $log_file, @command) = @ARGV;
    exit 0 if fork();
    POSIX::setsid();
    exit 0 if fork();
    open STDIN, "<", "/dev/null" or die "stdin: $!";
    open STDOUT, ">>", $log_file or die "stdout: $!";
    open STDERR, ">&STDOUT" or die "stderr: $!";
    open my $pid, ">", $pid_file or die "pid: $!";
    print {$pid} "$$\n";
    close $pid;
    exec @command or die "exec $command[0]: $!";
  ' "$pid_file" "$log_file" "$@"
}

start_mock() {
  if curl -fsS "http://127.0.0.1:${mock_port}/healthz" >/dev/null 2>&1; then
    echo "mock: running"
    return
  fi
  export ABAR_MOCK_MCP_ADDR="127.0.0.1:${mock_port}"
  daemonize "$mock_pid_file" "$state_dir/mock.log" \
    "$root/target/debug/mock-mcp-server"
  if ! wait_http "http://127.0.0.1:${mock_port}/healthz"; then
    echo "mock MCP failed to start; inspect $state_dir/mock.log" >&2
    exit 1
  fi
}

start_model() {
  if curl -fsS "http://127.0.0.1:${model_port}/healthz" >/dev/null 2>&1; then
    echo "model: running"
    return
  fi
  export ABAR_DEMO_MODEL_ADDR="127.0.0.1:${model_port}"
  daemonize "$model_pid_file" "$state_dir/model.log" \
    "$root/target/debug/merchandise-demo-model"
  if ! wait_http "http://127.0.0.1:${model_port}/healthz"; then
    echo "demo model failed to start; inspect $state_dir/model.log" >&2
    exit 1
  fi
}

start_webui() {
  if curl -fsS "${webui_url}/healthz" >/dev/null 2>&1; then
    echo "webui: running"
    return
  fi
  export ABAR_WEBUI_ADDR="127.0.0.1:${webui_port}"
  export ABAR_WEBUI_OIDC_ISSUER="$webui_issuer"
  export ABAR_WEBUI_OIDC_CLIENT_ID="$webui_client_id"
  export ABAR_WEBUI_OIDC_REDIRECT_URI="${public_webui_url}/auth/callback"
  export ABAR_WEBUI_OIDC_SCOPES="openid profile email"
  export ABAR_WEBUI_OIDC_ALLOW_HTTP=true
  daemonize "$webui_pid_file" "$state_dir/webui.log" \
    "$root/target/debug/abar-webui" \
    --agent merchandise-operations-assistant \
    --view "$root/views/merchandise-operations/webui-view.yaml"
  if ! wait_http "${webui_url}/healthz"; then
    echo "WebUI failed to start; inspect $state_dir/webui.log" >&2
    exit 1
  fi
}

start_platform_console() {
  if curl -fsS "${platform_url}/healthz" >/dev/null 2>&1; then
    echo "platform console: running"
    return
  fi
  export ABAR_PLATFORM_ADDR="127.0.0.1:${platform_port}"
  export ABAR_PLATFORM_OIDC_ISSUER="$platform_issuer"
  export ABAR_PLATFORM_OIDC_CLIENT_ID="$platform_client_id"
  export ABAR_PLATFORM_OIDC_REDIRECT_URI="${public_platform_url}/auth/callback"
  export ABAR_PLATFORM_OIDC_SCOPES="openid profile email"
  export ABAR_PLATFORM_OIDC_ALLOW_HTTP=true
  daemonize "$platform_pid_file" "$state_dir/platform.log" \
    "$root/target/debug/abar-platform-console" \
    --addr "127.0.0.1:${platform_port}"
  if ! wait_http "${platform_url}/healthz"; then
    echo "Platform Console failed to start; inspect $state_dir/platform.log" >&2
    exit 1
  fi
}

start() {
  mkdir -p "$state_dir"
  local runtime_signature="${public_webui_url}|${public_platform_url}|${public_keycloak_issuer}"
  if [[ ! -f "$runtime_config_file" || "$(cat "$runtime_config_file")" != "$runtime_signature" ]]; then
    if [[ -f "$webui_pid_file" ]]; then
      kill "$(cat "$webui_pid_file")" >/dev/null 2>&1 || true
      rm -f "$webui_pid_file"
    fi
    if [[ -f "$platform_pid_file" ]]; then
      kill "$(cat "$platform_pid_file")" >/dev/null 2>&1 || true
      rm -f "$platform_pid_file"
    fi
    "$abar" server stop >/dev/null 2>&1 || true
  fi
  configure_keycloak
  cargo build -q --manifest-path "$root/Cargo.toml" \
    -p abar-cli -p abar-mock-mcp-server -p abar-merchandise-demo-model \
    -p abar-webui -p abar-platform-console
  start_mock
  start_model
  "$abar" publish "$root/examples/merchandise-operations" >/dev/null
  if [[ ! -f "$state_dir/server.json" ]]; then
    "$abar" server start --daemon >/dev/null
  elif ! "$abar" server status >/dev/null 2>&1; then
    "$abar" server start --daemon >/dev/null
  fi
  start_webui
  start_platform_console
  printf '%s\n' "$runtime_signature" >"$runtime_config_file"
  print_connection
}

status() {
  local ready=true
  bash "$root/scripts/keycloak-local.sh" status >/dev/null 2>&1 \
    || ready=false
  if ! curl -fsS "http://127.0.0.1:${mock_port}/healthz" >/dev/null 2>&1; then
    ready=false
  fi
  if ! curl -fsS "http://127.0.0.1:${model_port}/healthz" >/dev/null 2>&1; then
    ready=false
  fi
  if ! "$abar" server status >/dev/null 2>&1; then
    ready=false
  fi
  if ! curl -fsS "${webui_url}/healthz" >/dev/null 2>&1; then
    ready=false
  fi
  if ! curl -fsS "${platform_url}/healthz" >/dev/null 2>&1; then
    ready=false
  fi
  if [[ "$ready" == "true" ]]; then
    echo "state: running"
    print_connection
    return
  fi
  echo "state: stopped"
  exit 3
}

stop() {
  if [[ -f "$webui_pid_file" ]]; then
    kill "$(cat "$webui_pid_file")" >/dev/null 2>&1 || true
    rm -f "$webui_pid_file"
  fi
  if [[ -f "$platform_pid_file" ]]; then
    kill "$(cat "$platform_pid_file")" >/dev/null 2>&1 || true
    rm -f "$platform_pid_file"
  fi
  "$abar" server stop >/dev/null 2>&1 || true
  if [[ -f "$mock_pid_file" ]]; then
    kill "$(cat "$mock_pid_file")" >/dev/null 2>&1 || true
    rm -f "$mock_pid_file"
  fi
  if [[ -f "$model_pid_file" ]]; then
    kill "$(cat "$model_pid_file")" >/dev/null 2>&1 || true
    rm -f "$model_pid_file"
  fi
  echo "state: stopped"
  echo "keycloak: left running"
}

print_connection() {
  webui_issuer="$public_keycloak_issuer"
  cat <<EOF
webui: ${public_webui_url}/
assistant: ${public_webui_url}/?agent=merchandise-operations-assistant
platform console: ${public_platform_url}/
idp: keycloak
oidc issuer: ${webui_issuer}
login: alice / alice-password
state: ${state_dir}
EOF
}

case "$command" in
  start)
    start
    ;;
  status)
    status
    ;;
  stop)
    stop
    ;;
  *)
    echo "usage: $0 start|status|stop" >&2
    exit 2
    ;;
esac
