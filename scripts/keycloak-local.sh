#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
container="${ABAR_KEYCLOAK_CONTAINER:-abar-keycloak-local}"
volume="${ABAR_KEYCLOAK_VOLUME:-abar-keycloak-local-data-26.6.1}"
port="${ABAR_KEYCLOAK_LOCAL_PORT:-18093}"
realm_file="$root/tests/keycloak/abar-realm.json"
provider_jar="${ABAR_KEYCLOAK_DINGTALK_PROVIDER_JAR:-$root/tests/keycloak/keycloak-dingtalk-provider.jar}"
provider_destination="/opt/keycloak/providers/keycloak-dingtalk-provider.jar"
admin_user="${ABAR_KEYCLOAK_ADMIN_USER:-admin}"
admin_password="${ABAR_KEYCLOAK_ADMIN_PASSWORD:-abar-local-admin}"
image="${ABAR_KEYCLOAK_IMAGE:-quay.io/keycloak/keycloak:26.6.1}"
command="${1:-status}"

realm_url="http://127.0.0.1:${port}/realms/abar"

wait_ready() {
  for _ in $(seq 1 120); do
    if curl -fsS "${realm_url}/.well-known/openid-configuration" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "Keycloak did not become ready; inspect with: $0 logs" >&2
  return 1
}

verify_dingtalk_provider() {
  local logs
  logs="$(docker logs "$container" 2>&1)"
  if ! grep -q 'com.ainuoyan.keycloak.dingtalk.DingTalkIdentityProviderFactory' \
    <<<"$logs"; then
    echo "Keycloak started but the DingTalk Identity Provider was not loaded" >&2
    return 1
  fi
}

ensure_client_endpoints() {
  local token client_id client_json redirects origins payload
  token="$(curl -fsS \
    -d "client_id=admin-cli" \
    -d "username=${admin_user}" \
    -d "password=${admin_password}" \
    -d "grant_type=password" \
    "http://127.0.0.1:${port}/realms/master/protocol/openid-connect/token" \
    | jq -r .access_token)"
  client_json="$(curl -fsS \
    -H "Authorization: Bearer ${token}" \
    "http://127.0.0.1:${port}/admin/realms/abar/clients?clientId=abar-test")"
  client_id="$(jq -r '.[0].id // empty' <<<"$client_json")"
  test -n "$client_id"
  redirects="$(jq -c '
    ((.[0].redirectUris // []) + [
      "http://127.0.0.1:19093/auth/callback",
      "http://127.0.0.1:19094/auth/callback"
    ]) | unique
  ' <<<"$client_json")"
  origins="$(jq -c '
    ((.[0].webOrigins // []) + [
      "http://127.0.0.1:19093",
      "http://127.0.0.1:19094"
    ]) | unique
  ' <<<"$client_json")"
  payload="$(jq -nc --argjson redirects "$redirects" --argjson origins "$origins" \
    '{redirectUris:$redirects,webOrigins:$origins}')"
  curl -fsS \
    -X PUT \
    -H "Authorization: Bearer ${token}" \
    -H 'Content-Type: application/json' \
    -d "$payload" \
    "http://127.0.0.1:${port}/admin/realms/abar/clients/${client_id}" \
    >/dev/null
}

start() {
  if ! docker image inspect "$image" >/dev/null 2>&1; then
    echo "missing Docker image: $image" >&2
    exit 2
  fi
  if [[ ! -f "$provider_jar" ]]; then
    echo "missing Keycloak provider JAR: $provider_jar" >&2
    exit 2
  fi
  if docker container inspect "$container" >/dev/null 2>&1; then
    mounted_provider="$(docker inspect -f \
      "{{range .Mounts}}{{if eq .Destination \"${provider_destination}\"}}{{.Source}}{{end}}{{end}}" \
      "$container")"
    current_image="$(docker inspect -f '{{.Config.Image}}' "$container")"
    mounted_volume="$(docker inspect -f \
      '{{range .Mounts}}{{if eq .Destination "/opt/keycloak/data"}}{{.Name}}{{end}}{{end}}' \
      "$container")"
    if [[ "$mounted_provider" != "$provider_jar" \
      || "$current_image" != "$image" \
      || "$mounted_volume" != "$volume" ]]; then
      docker rm -f "$container" >/dev/null
    fi
  fi
  if docker container inspect "$container" >/dev/null 2>&1; then
    if [[ "$(docker inspect -f '{{.State.Running}}' "$container")" != "true" ]]; then
      docker start "$container" >/dev/null
    fi
  else
    docker run -d \
      --name "$container" \
      --restart unless-stopped \
      -p "127.0.0.1:${port}:8080" \
      -e "KC_BOOTSTRAP_ADMIN_USERNAME=${admin_user}" \
      -e "KC_BOOTSTRAP_ADMIN_PASSWORD=${admin_password}" \
      -v "${volume}:/opt/keycloak/data" \
      -v "${realm_file}:/opt/keycloak/data/import/abar-realm.json:ro" \
      -v "${provider_jar}:${provider_destination}:ro" \
      "$image" start-dev --import-realm >/dev/null
  fi
  wait_ready
  verify_dingtalk_provider
  ensure_client_endpoints
  print_connection
}

status() {
  if ! docker container inspect "$container" >/dev/null 2>&1; then
    echo "state: not-created"
    exit 3
  fi
  if [[ "$(docker inspect -f '{{.State.Running}}' "$container")" == "true" ]] \
    && curl -fsS "${realm_url}/.well-known/openid-configuration" >/dev/null 2>&1; then
    echo "state: running"
    print_connection
    return
  fi
  echo "state: stopped"
  exit 3
}

print_connection() {
  cat <<EOF
admin: http://127.0.0.1:${port}/admin/
realm: abar
issuer: ${realm_url}
discovery: ${realm_url}/.well-known/openid-configuration
WebUI client: abar-test
WebUI redirect: http://127.0.0.1:19093/auth/callback
server client: abar-server
dingtalk provider: ${provider_jar}
provider mount: ${provider_destination}
image: ${image}
admin user: ${admin_user}
admin password: ${admin_password}
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
    docker stop --timeout 10 "$container" >/dev/null 2>&1 || true
    echo "state: stopped"
    ;;
  logs)
    docker logs --tail 100 -f "$container"
    ;;
  *)
    echo "usage: $0 start|status|stop|logs" >&2
    exit 2
    ;;
esac
