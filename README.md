# Agent Build And Runtime (ABAR)

English | [简体中文](README.zh-CN.md)

ABAR is a thin, business-neutral Agent Build/Runtime core implemented in Rust.

```text
Agent Revision -> Session -> Message -> Runtime Event
```

## Start by role

- Business owner: [define a business Agent](docs/en/guides/AUTHORING.md)
- Business user: [Agent WebUI](apps/abar-webui/README.md)
- Platform operator: [Platform Console](apps/abar-platform-console/README.md)
- Platform developer: [Implementation](docs/en/guides/IMPLEMENTATION.md) and
  [Contracts](docs/en/reference/CONTRACTS.md)

## Implemented

- immutable Agent Revision Build and Scope Review;
- long-lived Session and asynchronous Message Runtime;
- background execution, cancellation, and restart recovery;
- exact write approval and external idempotency reconciliation;
- projected Context Assembly and sensitive-field DataPolicy;
- MCP Streamable HTTP Tools and OpenAI-compatible Models;
- strict OIDC introspection plus UserInfo identity;
- WebUI Authorization Code + PKCE login and secure BFF sessions;
- tenant/role-scoped approval inbox and role-based authorization;
- readiness, Prometheus metrics, and structured runtime events;
- bounded OIDC identity cache and durable approval inbox index;
- offline state backup/restore and a 300-store pilot load suite;
- a multi-Agent WebUI and a conversation-first merchandise Agent that routes
  allocation, replenishment, and transfer intent;
- a separate Platform Console for Agent catalog, Scope Review, and Runtime
  aggregates;
- DingTalk manager alerts with Agent-scoped Workbench deep links;
- caller-scoped runtime diagnostics;
- Provider conformance validators;
- one unified merchandise operations end-to-end suite;

## Quick start

```bash
export ABAR_STATE_DIR=/tmp/abar-demo
export ABAR_CONFIG="$PWD/examples/merchandise-operations/config.fake.yaml"
export ABAR_MESSAGE_CONFIG="$PWD/examples/merchandise-operations/message.providers.yaml"

cargo run -p abar-mock-mcp-server
```

Run the example's deterministic OpenAI-compatible model in another terminal:

```bash
cargo run -p abar-merchandise-demo-model
```

In a third terminal:

```bash
abar publish examples/merchandise-operations
abar server start --daemon
abar session create --agent merchandise-operations-assistant
abar message create <session-id> \
  --input "看看华东库存异常门店，判断应该补货、配货还是门店间调拨"
abar runtime status
```

See [Merchandise Operations Assistant](examples/merchandise-operations/README.md)
for the unified conversation-first Agent.

## Workspace

```text
abar-api/       contracts, schemas, OpenAPI, conformance validators
abar-core/      Revision Build, Session/Message Runtime, workers, providers
abar-cli/       abar
adapters/       reusable Model, Tool, and channel adapters
apps/abar-webui/
apps/abar-platform-console/
examples/
  merchandise-operations/  Agent, mock MCP, and demo model
views/
```

## Documentation

- [Define a Business Agent](docs/en/guides/AUTHORING.md)
- [Core Model](docs/en/concepts/CORE-MODEL.md)
- [Architecture](docs/en/concepts/ARCHITECTURE.md)
- [Contracts](docs/en/reference/CONTRACTS.md)
- [Implementation](docs/en/guides/IMPLEMENTATION.md)
- [CLI](docs/en/guides/CLI.md)
- [Identity and Approval](docs/en/guides/IDENTITY-AND-APPROVAL.md)
- [DingTalk Workbench Entry](adapters/abar-channel-dingtalk/README.md)
- [Platform Console](apps/abar-platform-console/README.md)
- [OpenAPI v1](abar-api/openapi/server-v1.json)

## Verification

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
bash scripts/check.sh
bash scripts/test-merchandise-operations.sh
bash scripts/test-platform-console.sh
```

The shell suites keep one Agent business scenario (商品运营助手) and add a
separate Platform Console control-plane scenario.
