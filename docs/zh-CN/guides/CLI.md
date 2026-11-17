# CLI

[English](../../en/guides/CLI.md) | 简体中文

官方二进制为 `abar`。

## 命令

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

## 发布与 Revision 审批

```bash
abar publish examples/merchandise-operations
abar revision list merchandise-operations-assistant
abar revision current merchandise-operations-assistant
```

仅行为变化自动成为当前 Revision。资源扩大等待审批：

```bash
abar revision approve merchandise-operations-assistant:2 \
  --reviewer platform-reviewer \
  --comment "approved new Tool scope"
```

Revision 状态使用 `ABAR_REVISION_STORE` 或 `<state-dir>/revisions`。

## Server

`abar server start` 默认前台运行，适合服务管理器。本地后台模式使用
`--daemon`。

连接信息位于 `<state-dir>/server.json`。Daemon 日志位于
`<state-dir>/server.log`。

## 备份与恢复

状态操作要求 Server 已停止：

```bash
abar server stop
abar state backup /backups/abar-2026-11-17.tar.gz
abar state restore /backups/abar-2026-11-17.tar.gz --replace
abar server start --daemon
```

备份生成 SHA-256 校验文件。恢复时校验存在的 Checksum，拒绝不安全的归档路径；
使用 `--replace` 时保留原状态目录，避免不可恢复覆盖。

`abar state reindex-approvals` 在 Server 停止时，从 Message 源记录重建待审批
读模型。

## Session 与 Message

```bash
abar session create --agent merchandise-operations-assistant
abar message create <session-id> --input "看看华东库存异常门店，判断应该补货、配货还是调拨"
```

`message create` 和审批默认等待 Message 离开 `queued` 或 `running`。异步客户端
使用 `--no-wait`。

审批身份来自 Server `AuthProvider`。其他授权审批人显式提供 Session、Message
和审批证据；普通 Session 与 Message 查询仍只对发起用户可见。

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

审批人不同于申请用户时使用 `--no-wait`。审批人不能轮询申请人私有的 Message；
完成状态由申请人的 Session 或外部审批集成观察。

## 诊断

```bash
abar runtime status
```

返回调用者可见的 Worker、Session、Message、Provider 配置和存储诊断。

`healthz` 只证明进程存活；`readyz` 检查 Message Provider 配置和存储可写性。
`/metrics` 暴露内部 Prometheus Counter 及 Session/Message 状态 Gauge。由于
Server 只绑定回环地址，这些接口用于本地运维。

## 环境变量

- `ABAR_CONFIG`：模型 Runtime 配置。
- `ABAR_MESSAGE_CONFIG`：Model、MCP、Knowledge 和 DataPolicy 配置。
- `ABAR_STATE_DIR`：Server 状态根目录。
- `ABAR_REVISION_STORE`：Revision Store 覆盖。
- `ABAR_ENDPOINT` / `ABAR_TOKEN`：远程 Server 连接。
- `ABAR_OIDC_USERINFO_ENDPOINT`：启用 OIDC UserInfo 认证。
- `ABAR_OIDC_SUBJECT_CLAIM`、`ABAR_OIDC_TENANT_CLAIM` 和
  `ABAR_OIDC_ROLES_CLAIM`：OIDC Claim 名称。
- `ABAR_OIDC_APPROVER_ROLE`：必需的审批角色。
- `ABAR_OIDC_SCOPE_FIELD`：区域审批 Scope 对应的待审批 Tool 参数，例如
  `requiredScopes`。
- `ABAR_OIDC_ALLOW_HTTP`：仅限本地测试的 HTTP 许可。
- `ABAR_OIDC_ISSUER`、`ABAR_OIDC_AUDIENCE`、
  `ABAR_OIDC_INTROSPECTION_ENDPOINT`、`ABAR_OIDC_CLIENT_ID` 和
  `ABAR_OIDC_CLIENT_SECRET`：严格 Access Token Introspection，五项必须同时
  配置。
- `ABAR_OIDC_CACHE_SECONDS`：已验证身份缓存时间，默认 30 秒，最大 300 秒。
- `ABAR_CONTROL_PLANE_ROLE`：允许使用平台 Console API 的角色，默认
  `platform-admin`。
- `ABAR_WEBUI_AGENT` / `ABAR_WEBUI_VIEW`：WebUI Agent 名称和 View Pack。

## 退出码

- `0`：成功。
- `1`：校验或运行失败。
- `2`：参数错误。
- `3`：Server 停止或不可用。
