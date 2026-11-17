# 平台 Console

[English](README.md) | 简体中文

平台 Console 是 ABAR 控制面的管理界面。它与 Agent WebUI 分离：

- Agent WebUI：业务用户与单个 Agent 对话并使用业务视图；
- 平台 Console：查看和管理 Agent 目录、Revision、Scope Review 与 Runtime。

平台 Console 不展示业务 Session 对话、Prompt、完整 Tool 参数或最终业务输出。

## 能力

平台 Console 提供：

- 当前 Tenant 的 Agent 目录；
- 每个 Agent 的 Current Revision 与历史 Revision；
- Model Class、最大步骤数、Tool、Skill 和 Knowledge Scope；
- Scope Review 证据以及批准/拒绝动作；
- 活跃 Worker、Session、Message、Token、Provider 和 Storage 聚合；
- OIDC BFF 登录和 CSRF 保护。

界面展示 Server 已持久化的事实。

## 权限

Server 通过 `ABAR_CONTROL_PLANE_ROLE` 指定平台管理角色，默认
`platform-admin`。本地 Server 默认同时授予 `approver` 和
`platform-admin`。

平台的 Scope Review 与业务写审批是不同权限：

- Scope Review 决定候选 Revision 是否成为 Current；
- 业务写审批决定一次精确 Tool 调用是否执行。

本地工作台通过身份测试夹具分配审批角色。生产环境应使用独立的 IdP 角色控制
平台 Console 操作。

## API

```text
GET  /v1/control-plane/overview
POST /v1/control-plane/agents/{agent}/revisions/{revision}/review
```

`overview` 返回当前 Tenant 的聚合数据，不返回逐 Session 明细。Review 请求只接受
`approved` 或 `rejected`，评审人来自已认证 Subject。

## 启动

```bash
ABAR_PLATFORM_OIDC_ISSUER=https://id.example.com/realms/acme \
ABAR_PLATFORM_OIDC_CLIENT_ID=abar-platform \
ABAR_PLATFORM_OIDC_REDIRECT_URI=https://platform.example.com/auth/callback \
ABAR_PLATFORM_OIDC_SCOPES="openid profile email" \
abar-platform-console --addr 127.0.0.1:19092
```

本地工作台同时启动 Agent WebUI（`19093`）和平台 Console（`19094`）。

## 验收

```bash
bash scripts/test-platform-console.sh
```

测试覆盖首次发布、Scope 扩张候选、平台评审批准、Current Revision 切换，以及
Runtime 和 Token 聚合。
