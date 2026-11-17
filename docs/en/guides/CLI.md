# CLI

English | [简体中文](../../zh-CN/guides/CLI.md)

The official binary is `abar`.

## Commands

```text
abar version
abar publish [--json] [source]
abar revision list <agent>
abar revision current <agent>
abar revision show <agent:revision>
abar revision approve <agent:revision> --reviewer <name> [--comment <text>]
abar revision reject <agent:revision> --reviewer <name> [--comment <text>]
abar server start [--daemon]
abar server status
abar server stop
abar state backup <archive.tar.gz>
abar state restore <archive.tar.gz> [--replace]
abar state reindex-approvals
abar runtime status
abar session create --agent <agent>
abar session list
abar session get <session-id>
abar message create <session-id> --input <text> [--skill <name>] [--no-wait]
abar message get <message-id>
abar message events <message-id> [--after <sequence>]
abar message approve <message-id> [--session <id>] [--approval-id <id>] [--scope <scope>] [--comment <text>] [--no-wait]
abar message reject <message-id> [--session <id>] [--approval-id <id>] [--scope <scope>] [--comment <text>] [--no-wait]
abar message cancel <message-id>
abar message reconcile <message-id> --action query|cancel
```

## Publish and Revision Review

```bash
abar publish examples/merchandise-operations
abar revision list merchandise-operations-assistant
abar revision current merchandise-operations-assistant
```

Behavior-only changes become current automatically. Scope expansion waits for:

```bash
abar revision approve merchandise-operations-assistant:2 \
  --reviewer platform-reviewer \
  --comment "approved new Tool scope"
```

Revision state uses `ABAR_REVISION_STORE` or `<state-dir>/revisions`.

## Server

`abar server start` stays in the foreground for service managers. Add
`--daemon` for local detached execution.

Connection metadata is `<state-dir>/server.json`. Daemon mode logs to
`<state-dir>/server.log`.

## Backup and restore

State operations require a stopped Server:

```bash
abar server stop
abar state backup /backups/abar-2026-11-17.tar.gz
abar state restore /backups/abar-2026-11-17.tar.gz --replace
abar server start --daemon
```

Backup writes a SHA-256 sidecar. Restore verifies the checksum when present,
rejects unsafe archive entries, and preserves the previous state directory when
`--replace` is used.

`abar state reindex-approvals` rebuilds the pending approval read model from
Message source records while the Server is stopped.

## Session and Message

```bash
abar session create --agent merchandise-operations-assistant
abar message create <session-id> --input "看看华东库存异常门店，判断应该补货、配货还是调拨"
```

`message create` and approval waits until the Message leaves `queued` or
`running`. Use `--no-wait` for asynchronous clients.

Approval identity comes from the Server `AuthProvider`. A different authorized
approver passes the Session, Message, and approval evidence explicitly; ordinary
Session and Message reads remain private to the requesting user.

```bash
abar message approve <message-id> --scope 华东 --comment approved
ABAR_TOKEN="$APPROVER_TOKEN" abar message approve <message-id> \
  --session <requester-session-id> \
  --approval-id <approval-id> \
  --scope 华东 \
  --no-wait
abar message cancel <message-id>
abar message reconcile <message-id> --action query
```

Use `--no-wait` when an approver differs from the requesting user. The approver
cannot poll the requester-private Message; completion is observed through the
requester's Session or the external approval integration.

## Diagnostics

```bash
abar runtime status
```

Returns caller-scoped worker, Session, Message, provider configuration, and
storage diagnostics.

`healthz` only proves that the process is alive. `readyz` checks Message
Provider configuration and storage writability. `/metrics` exposes internal
Prometheus counters and Session/Message state gauges; because the Server binds
to loopback, these endpoints are intended for local operations.

## Environment

- `ABAR_CONFIG`: model runtime configuration.
- `ABAR_MESSAGE_CONFIG`: Model, MCP, Knowledge, and DataPolicy config.
- `ABAR_STATE_DIR`: Server state root.
- `ABAR_REVISION_STORE`: Revision store override.
- `ABAR_ENDPOINT` / `ABAR_TOKEN`: remote Server connection.
- `ABAR_OIDC_USERINFO_ENDPOINT`: enables OIDC UserInfo authentication.
- `ABAR_OIDC_SUBJECT_CLAIM`, `ABAR_OIDC_TENANT_CLAIM`, and
  `ABAR_OIDC_ROLES_CLAIM`: OIDC claim names.
- `ABAR_OIDC_APPROVER_ROLE`: required approval role.
- `ABAR_OIDC_SCOPE_FIELD`: optional pending-Tool argument field used for
  regional approval scopes, for example `requiredScopes`.
- `ABAR_OIDC_ALLOW_HTTP`: local test-only HTTP permission.
- `ABAR_OIDC_ISSUER`, `ABAR_OIDC_AUDIENCE`,
  `ABAR_OIDC_INTROSPECTION_ENDPOINT`, `ABAR_OIDC_CLIENT_ID`, and
  `ABAR_OIDC_CLIENT_SECRET`: strict access-token introspection. Configure all
  five together.
- `ABAR_OIDC_CACHE_SECONDS`: verified identity cache TTL, default 30, maximum
  300.
- `ABAR_CONTROL_PLANE_ROLE`: role allowed to use Platform Console APIs,
  default `platform-admin`.
- `ABAR_WEBUI_AGENT` / `ABAR_WEBUI_VIEW`: WebUI Agent name and View Pack.

## Exit codes

- `0`: success.
- `1`: validation or runtime failure.
- `2`: invalid arguments.
- `3`: Server stopped or unavailable.
