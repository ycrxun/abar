# Contracts

English | [简体中文](../../zh-CN/reference/CONTRACTS.md)

Rust types, JSON Schema, and OpenAPI are the machine-readable contracts.

## Schema identities

```text
urn:abar:schema:agent-revision:v1
urn:abar:schema:session:v1
urn:abar:schema:message:v1
urn:abar:schema:scope-review:v1
urn:abar:schema:runtime-event:v1
```

## Agent Revision project

```yaml
apiVersion: abar/v1
kind: Agent
metadata:
  name: merchandise-operations-assistant
spec:
  modelClass: general-tool-use
  maxSteps: 8
  scope: scope.yaml
  behavior: behavior.yaml
  knowledge: knowledge.yaml
  tools: tools.yaml
```

A published Revision includes immutable scope, behavior references, Knowledge
Source bindings, Tool bindings, runtime requirements, and content digests.

## Session

```json
{
  "id": "ses_...",
  "tenant": "local",
  "agent": "merchandise-operations-assistant",
  "user": "local",
  "status": "active",
  "createdAt": "2026-11-17T08:30:00Z",
  "lastActiveAt": "2026-11-17T09:10:00Z"
}
```

Session creation accepts only `{"agent":"merchandise-operations-assistant"}`. Tenant and user
identity come from `AuthProvider`.

Session list rows may include `latestMessageId`, `latestMessageState`, and
`messageCount`, allowing WebUI clients to render state without an N+1
Message query.

## Task

`POST /v1/tasks` atomically creates a Session and its first Message:

```json
{
  "requestId": "webui-request-id",
  "agent": "merchandise-operations-assistant",
  "input": "生成华东区域补货计划",
  "requestedSkills": []
}
```

The same authenticated user and `requestId` is idempotent. Reusing it with
different content is rejected. A successfully queued Task returns `202`; an
idempotent replay returns `200`.

## Message

Creation accepts:

```json
{
  "input": "Generate this week's transfer plan",
  "requestedSkills": []
}
```

The endpoint returns `202` with a `queued` Message. States are:

```text
queued -> running
running -> waiting_approval | waiting_external | completed | failed | cancelled
waiting_approval -> running | failed | cancelled
waiting_external -> running | failed | cancelled
```

Message evidence includes:

- `agentRevision`;
- model, Tool, Skill, Knowledge and Schema composition;
- `toolResults`;
- `pendingApproval` and `approval`;
- `pendingExternal`;
- `checkpoint`;
- output and timestamps.

`pendingExternal` contains only exact call evidence:

```json
{
  "toolId": "inventory.query_store_inventory",
  "argumentsJson": "{}",
  "idempotencyKey": "sha256:..."
}
```

## Approval

```json
{
  "approvalId": "sha256:...",
  "decision": "approved",
  "scopes": ["华东"],
  "comment": "approved"
}
```

The actor comes from `AuthProvider`. Approval binds the exact Tool, arguments,
effect, idempotency key, Session, and Message.

`GET /v1/approvals` returns only tasks the authenticated reviewer may decide:

```json
{
  "approvals": [{
    "approvalId": "sha256:...",
    "sessionId": "ses_...",
    "messageId": "msg_...",
    "agent": "merchandise-operations-assistant",
    "agentRevision": 1,
    "requester": "user-subject",
    "toolId": "inventory.create_transfer_order",
    "argumentsJson": "{}",
    "pendingSince": "2026-11-17T09:01:00Z"
  }]
}
```

The projection excludes requester Message history, output, Composition, and Tool
results. Visibility still does not grant general Session access.

## External reconciliation

`POST /v1/messages/{id}/reconcile` accepts `action=query|cancel`.

Query uses `ToolRegistry.check` with the complete persisted Tool call. Missing
writes remain blocked. Reads may be retried from the same intent.

## Runtime Event

```json
{
  "eventId": "msg_...:3",
  "sessionId": "ses_...",
  "messageId": "msg_...",
  "sequence": 3,
  "eventType": "approval.requested",
  "at": "2026-11-17T09:01:00Z",
  "schemaVersion": "runtime-event/v1",
  "data": {}
}
```

Events are append-only and queried after a monotonic cursor.

## Providers

| Provider | Contract |
|---|---|
| `ModelProvider` | model identity, text output, Tool intents |
| `ToolRegistry` | Revision resolution, call, exact idempotency lookup |
| `KnowledgeProvider` | bounded Source retrieval with citations |
| `DataPolicy` | classify, project, and guard sensitive data |
| `ContentProvider` | materialized Revision behavior |
| `AuthProvider` | authenticated tenant and user identity |
| `ApprovalAuthorizer` | review visibility, exact approval role, and scope authorization |

Every adapter must pass the `abar_api::conformance` validators.

`ModelRequest` carries both the assembled model input and projected Tool
results. An OpenAI-compatible adapter must send those results as tool messages
rather than flattening them into plain Prompt text. Model mismatches, malformed
Tool arguments, unknown Tool names, and timeouts fail closed.

Model configuration may pass provider-specific `parameters`. The adapter merges
them into the request body except reserved fields such as `model`, `messages`,
`tools`, `tool_choice`, and `stream`.

`ModelResponse.usage` reports prompt, completion, and total Tokens. Message
execution accumulates this usage across iterations and persists it in
Checkpoints and the final Message.

`ToolRegistry` receives the Session-correlated `RequestContext`. The bundled MCP
adapter forwards it as `x-abar-session-id` and `x-abar-tenant-id`, allowing
business Tools to retain a stable snapshot without adding business fields to
the core.

## Server API

```text
GET  /healthz
GET  /readyz
GET  /metrics
GET  /v1/capabilities
GET  /v1/runtime/status
GET  /v1/agents/{agent}/revisions
GET  /v1/agents/{agent}/revisions/current
GET  /v1/agents/{agent}/revisions/{revision}
GET  /v1/approvals
POST /v1/sessions
GET  /v1/sessions
GET  /v1/sessions/{id}
POST /v1/sessions/{id}/close
POST /v1/sessions/{id}/archive
GET  /v1/sessions/{id}/messages
POST /v1/sessions/{id}/messages
GET  /v1/sessions/{id}/messages/{message_id}
POST /v1/sessions/{id}/messages/{message_id}/approval
GET  /v1/messages/{message_id}
GET  /v1/messages/{message_id}/events
POST /v1/messages/{message_id}/cancel
POST /v1/messages/{message_id}/reconcile
```
