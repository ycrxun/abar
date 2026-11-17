# Agent WebUI

English | [简体中文](README.zh-CN.md)

`abar-webui` is an independent Agent WebUI. It does not depend on ABAR
internal implementation details and uses the `/v1` API.

It presents the conversations, business views, approvals, and runtime evidence
of one bound Agent.

The shared Agent shell:

- binds one Agent name;
- creates long-lived Sessions and asynchronous Messages;
- switches among the history of Messages in one long-lived Session;
- renders Message Composition and Runtime Events;
- displays checkpoint/final Tool results and Message output;
- approves or rejects `waiting_approval` Messages with scopes;
- cancels queued/running Messages;
- queries or cancels `waiting_external` recovery;
- loads declarative WebUI View Packs.

The WebUI always binds one Agent. A View Pack is optional: when `--view` is
omitted, the built-in Codex-style View is bound to that Agent at startup.

The browser never receives the Agent Server bearer token. The WebUI backend
proxies API requests.

The BFF adds three safety optimizations:

- initial Tasks use `POST /v1/tasks` and a client `requestId` for idempotent
  Session + Message creation;
- confirmation sends an opaque business confirmation object plus
  `agentRevision`, and verifies that exact object against Message Tool evidence;
- Session details request Events only after `eventCursor`.
- Internal `[structured confirmation]` Messages are presented as business text;
  business-specific confirmation fields stay in persisted Message evidence.

CSS and JavaScript are separate static assets, so the CSP does not allow inline
scripts or styles.

## Start

Start the Mock MCP Server and demo model:

```bash
export ABAR_STATE_DIR=/tmp/abar-merchandise-webui
export ABAR_CONFIG="$PWD/examples/merchandise-operations/config.fake.yaml"
export ABAR_MESSAGE_CONFIG="$PWD/examples/merchandise-operations/message.providers.yaml"
cargo run -p abar-mock-mcp-server
cargo run -p abar-merchandise-demo-model
```

In another terminal:

```bash
abar publish examples/merchandise-operations
abar server start --daemon
```

Start with the merchandise View Pack:

```bash
cargo run -p abar-webui -- \
  --agent merchandise-operations-assistant \
  --view views/merchandise-operations/webui-view.yaml
```

Open <http://127.0.0.1:19090>.

Start a second Agent with the default View:

```bash
cargo run -p abar-webui -- \
  --agent another-assistant \
  --addr 127.0.0.1:19091
```

The View Pack must declare `binding.agent`, and it must match `--agent`.

The WebUI reads local connection metadata from
`ABAR_STATE_DIR/server.json`. `ABAR_ENDPOINT` and `ABAR_TOKEN` select a remote
Server. `ABAR_WEBUI_AGENT` selects the Agent. `ABAR_WEBUI_ADDR` or `--addr`
changes the listen address.

The default listener is `127.0.0.1` and has no login system. Add organization
identity, roles, and approver checks before production use.

## Verification

```bash
bash scripts/test-merchandise-operations.sh
```

See [View Packs](../../views/README.md).
