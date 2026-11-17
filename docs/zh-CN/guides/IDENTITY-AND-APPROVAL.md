# 身份与审批

[English](../../en/guides/IDENTITY-AND-APPROVAL.md) | 简体中文

身份认证和审批权限是两件独立的事。

- `AuthProvider` 证明调用者身份和 Tenant。
- `ApprovalAuthorizer` 决定该身份是否可以为一次精确的待审批 Tool 调用作出决定。

## OIDC UserInfo 模式

内置 `abar-auth-oidc` Adapter 通过 OIDC UserInfo Endpoint 验证 Access Token，
并把 Claim 映射到 `AuthContext`。

```bash
export ABAR_OIDC_USERINFO_ENDPOINT="https://id.example.com/oidc/userinfo"
export ABAR_OIDC_SUBJECT_CLAIM="sub"
export ABAR_OIDC_TENANT_CLAIM="tenant"
export ABAR_OIDC_ROLES_CLAIM="roles"
export ABAR_OIDC_APPROVER_ROLE="transfer-approver"
export ABAR_OIDC_SCOPE_FIELD="requiredScopes"
export ABAR_OIDC_ISSUER="https://id.example.com/realms/acme"
export ABAR_OIDC_AUDIENCE="abar-api"
export ABAR_OIDC_INTROSPECTION_ENDPOINT="https://id.example.com/realms/acme/protocol/openid-connect/token/introspect"
export ABAR_OIDC_CLIENT_ID="abar-server"
export ABAR_OIDC_CLIENT_SECRET="<服务端 Secret>"
export ABAR_OIDC_CACHE_SECONDS="30"
export ABAR_TOKEN="<OIDC access token>"
```

默认要求 HTTPS。`ABAR_OIDC_ALLOW_HTTP=true` 仅用于本地集成测试。

配置 Introspection 后，Server 在解析 UserInfo Claim 前要求 Token
`active=true`、Issuer 精确匹配、Audience 包含预期值且未过期。Issuer、
Audience、Endpoint、Client ID 和 Client Secret 必须完整配置。

认证结果以 Token 的 SHA-256 哈希缓存，缓存时间由
`ABAR_OIDC_CACHE_SECONDS` 控制（默认 30 秒，最大 300 秒），且不会超过
Introspection 的过期时间。并发缓存未命中会合并。该策略用有界的撤销延迟，
换取 API 请求不会重复触发两次 IdP 调用。

配置 `ABAR_OIDC_SCOPE_FIELD` 后，精确基础角色 `transfer-approver` 仍代表全局
审批。区域角色可使用 `transfer-approver:华东` 或
`transfer-approver:华东,华南`。任务可见与审批要求待审批参数中的
`requiredScopes` 都被角色后缀覆盖，批准 Scope 也不得超出该后缀。

OIDC 模式下：

- Server 不把 OIDC Access Token 写入 `server.json`；
- CLI 调用方通过 `ABAR_TOKEN` 提供 Token；
- WebUI 使用 Authorization Code + PKCE，Token 只保存在服务端；
- 审批人来自 UserInfo Subject；
- 审批角色来自配置的 Roles Claim；
- 同一 Tenant 内，审批人可以不同于申请用户；
- 批准时必须至少包含一个 Scope。

## WebUI 浏览器登录

WebUI 的 OIDC 模式不需要预先提供 `ABAR_TOKEN`：

```bash
export ABAR_WEBUI_OIDC_ISSUER="https://id.example.com/realms/acme"
export ABAR_WEBUI_OIDC_CLIENT_ID="abar-webui"
export ABAR_WEBUI_OIDC_REDIRECT_URI="https://abar.example.com/auth/callback"
export ABAR_WEBUI_OIDC_SCOPES="openid profile email"
```

WebUI 作为 Backend for Frontend：

- 使用 Authorization Code + PKCE 建立浏览器会话；
- 浏览器只持有不透明的 `HttpOnly`、`SameSite=Lax` Cookie；
- Access Token 和 Refresh Token 只保存在 WebUI 进程内存；
- WebUI 会话为进程内状态，进程重启后用户需要重新认证；
- 所有写请求必须携带绑定当前会话的 CSRF Token；
- 注销时清理 WebUI 会话并跳转 IdP End Session Endpoint；
- 审批收件箱只返回当前 Tenant 内该角色可处理的审批任务，不暴露申请人的
  Message 历史和执行证据。

机密客户端可设置 `ABAR_WEBUI_OIDC_CLIENT_SECRET`。除显式启用的本地测试
`ABAR_WEBUI_OIDC_ALLOW_HTTP=true` 外，必须使用 HTTPS。

## 本地模式

未配置 OIDC 时，Server 生成本地 Bearer Token，并使用 `approver` 角色。该模式
只用于本地开发。

## 安全不变量

审批必须同时满足：

1. Message 正在等待审批；
2. Approval ID 与待审批 Tool 调用精确匹配；
3. 已认证主体拥有配置的审批角色；
4. Session 属于同一 Tenant；
5. 批准决定至少包含一个 Scope。

审批结果通过 Runtime Event 记录实际 Actor 和 Roles。
