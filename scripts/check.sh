#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo fmt --all -- --check

while IFS= read -r -d '' file; do
  jq empty "$file"
done < <(find abar-api/schemas abar-api/openapi views/schema -type f -name '*.json' -print0)

manifest="$(mktemp)"
trap 'rm -f "$manifest"' EXIT
while IFS= read -r -d '' file; do
  relative="${file#"$root/"}"
  id="$(jq -r '."$id" // empty' "$file")"
  version="$(basename "$(dirname "$file")")"
  case "$relative" in
    abar-api/schemas/*/"$version"/*.schema.json)
      resource="$(basename "$(dirname "$(dirname "$file")")")"
      ;;
    views/schema/"$version"/*.schema.json)
      resource="$(basename "$file" .schema.json)"
      ;;
    *)
      echo "schema: unexpected schema path: $relative" >&2
      exit 1
      ;;
  esac
  expected="urn:abar:schema:${resource}:${version}"
  test "$id" = "$expected"
  printf '%s\t%s\n' "$id" "$relative" >>"$manifest"
done < <(find abar-api/schemas views/schema -type f -name '*.schema.json' -print0)
test -z "$(cut -f1 "$manifest" | sort | uniq -d)"
test "$(jq -r '.info.title' abar-api/openapi/server-v1.json)" = "ABAR Runtime API v1"

if rg -n -S \
  "商品运营|调拨|配货|补货|门店|库存|华东|成本|毛利|merchandise|transfer|allocation|replenishment|inventory|unitCost|gross-margin|supplierPrice" \
  abar-api abar-core abar-cli apps \
  adapters/abar-model-fake adapters/abar-model-openai adapters/abar-auth-oidc \
  adapters/abar-tool-fake adapters/abar-tool-mcp-http \
  --glob '*.rs' --glob '*.json' --glob '*.yaml' --glob '*.yml' \
  --glob '*.html' --glob '*.js'; then
  echo "check: example-specific business language leaked into generic code" >&2
  exit 1
fi

en_names="$(find docs/en -type f -name '*.md' | sed "s#^docs/en/##" | sort)"
zh_names="$(find docs/zh-CN -type f -name '*.md' | sed "s#^docs/zh-CN/##" | sort)"
test "$en_names" = "$zh_names"
while IFS= read -r relative; do
  grep -Fq "$(basename "$relative")" "docs/en/$relative"
  grep -Fq "$(basename "$relative")" "docs/zh-CN/$relative"
done <<<"$en_names"

while IFS= read -r -d '' file; do
  directory="$(dirname "$file")"
  while IFS= read -r raw; do
    link="${raw#"]("}"
    link="${link%)}"
    link="${link%%#*}"
    case "$link" in
      "" | \#* | http://* | https://* | mailto:*) continue ;;
    esac
    test -e "$directory/$link"
  done < <(grep -oE '\]\([^)]+' "$file" || true)
done < <(find . \
  \( -path ./.git -o -path ./target -o -path ./.local \) -prune -o \
  -type f -name '*.md' -print0)

while IFS= read -r path; do
  lines="$(wc -l <"$path" | tr -d ' ')"
  test "$lines" -le "${ABAR_MAX_MODULE_LINES:-500}"
done < <(find abar-api abar-core abar-cli adapters apps examples \
  -type f -name '*.rs' -print | sort)

echo "check: rust format, business boundaries, JSON Schemas, bilingual docs, links and module size passed"
