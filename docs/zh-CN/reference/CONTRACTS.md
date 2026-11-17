# 契约与接口

[English](../../en/reference/CONTRACTS.md) | 简体中文

Rust 类型、JSON Schema 和 OpenAPI 是机器可读契约。

## Schema 身份

```text
urn:abar:schema:agent-revision:v1
urn:abar:schema:session:v1
urn:abar:schema:message:v1
urn:abar:schema:scope-review:v1
urn:abar:schema:runtime-event:v1
```

## Agent Revision 项目

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

发布的 Revision 包含不可变范围、Behavior 引用、Knowledge Source 绑定、Tool
绑定、Runtime 要求和内容摘要。

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

Session 创建只接受 `{"agent":"merchandise-operations-assistant"}`。Tenant 和用户身份来自
`AuthProvider`。

Session 列表行可包含 `latestMessageId`、`latestMessageState` 和
`messageCount`，WebUI 因此不需要为每行额外请求 Message。

## Task

`POST /v1/tasks` 原子创建 Session 和首个 Message：

```json
{
  "requestId": "webui-request-id",
  "agent": "merchandise-operations-assistant",
  "input": "生成华东区域补货计划",
  "requestedSkills": []
}
```

同一认证用户和 `requestId` 幂等；使用相同 ID 但不同内容会被拒绝。首次成功
创建返回 `202`，幂等重放返回 `200`。

## Message

创建请求：

```json
{
  "input": "生成本周调拨方案",
  "requestedSkills": []
}
```

接口返回 `202` 和 `queued` Message。状态迁移：

```text
queued -> running
running -> waiting_approval | waiting_external | completed | failed | cancelled
waiting_approval -> running | failed | cancelled
waiting_external -> running | failed | cancelled
```

Message 证据包括 Agent Revision、实际 Model/Tool/Skill/Knowledge/Schema 组合、
Tool 结果、审批、待外部调用、checkpoint、输出和时间。

`pendingExternal` 只保存精确调用证据：

```json
{
  "toolId": "inventory.query_store_inventory",
  "argumentsJson": "{}",
  "idempotencyKey": "sha256:..."
}
```

## 审批

```json
{
  "approvalId": "sha256:...",
  "decision": "approved",
  "scopes": ["华东"],
  "comment": "approved"
}
```

审批人来自 `AuthProvider`。审批绑定精确 Tool、参数、effect、幂等键、Session
和 Message。

`GET /v1/approvals` 只返回当前认证审批人可以决定的任务：

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

投影不包含申请人的 Message 历史、输出、Composition 或 Tool 结果。审批可见性
不会扩大为普通 Session 访问权。

## 外部对账

`POST /v1/messages/{id}/reconcile` 接受 `action=query|cancel`。

Query 使用完整持久 Tool 调用执行 `ToolRegistry.check`。缺失写结果保持阻塞；
读操作可以按同一意图重试。

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

Event append-only，并通过单调游标读取。

## Provider

| Provider | 契约 |
|---|---|
| `ModelProvider` | 模型身份、文本输出、Tool 意图 |
| `ToolRegistry` | Revision 解析、调用、精确幂等查询 |
| `KnowledgeProvider` | 有界 Source 检索和引用 |
| `DataPolicy` | 敏感数据分类、投影和输出防护 |
| `ContentProvider` | 物化 Revision Behavior |
| `AuthProvider` | 认证 Tenant 和用户身份 |
| `ApprovalAuthorizer` | 审批可见范围、精确审批角色和 Scope 授权 |

所有 Adapter 必须通过 `abar_api::conformance` 契约校验器。

`ModelRequest` 同时携带组装后的模型输入和已投影 Tool 结果。OpenAI-compatible
Adapter 必须把 Tool 结果作为结构化 Tool Message 发送，而不是仅拼接到普通
Prompt 文本。模型不匹配、Tool 参数非法、未知 Tool 和超时均失败关闭。

模型配置可通过 `parameters` 传递 Provider 私有参数。Adapter 会合并到请求体，
但不会覆盖 `model`、`messages`、`tools`、`tool_choice` 和 `stream` 等保留字段。

`ModelResponse.usage` 报告 Prompt、Completion 和 Total Tokens。Message 执行会
跨迭代累计该用量，并持久化到 Checkpoint 和最终 Message。

`ToolRegistry` 接收与 Session 关联的 `RequestContext`。内置 MCP Adapter 将其
传递为 `x-abar-session-id` 和 `x-abar-tenant-id`，使业务 Tool 能保持稳定数据
快照，而核心无需加入业务字段。

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
