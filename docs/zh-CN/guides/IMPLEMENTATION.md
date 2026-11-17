# 实现设计

[English](../../en/guides/IMPLEMENTATION.md) | 简体中文

本文是实现指南。业务定义者应先阅读[业务定义 Agent](AUTHORING.md)。

## Workspace

```text
abar-api/        稳定契约、JSON Schema、OpenAPI、校验器
abar-core/       Revision Build、Session/Message Store、Worker、Provider
abar-cli/        abar 命令行客户端
adapters/        参考 Model 和 Tool Provider
apps/abar-webui/
apps/abar-platform-console/
examples/merchandise-operations/
views/           声明式 WebUI View Pack
```

## Agent Revision Build

`abar publish`：

1. 读取 Revision 项目 YAML；
2. 校验 Source 路径并拒绝符号链接；
3. 计算 Prompt、Skill、规则、Knowledge 和 Scope 摘要；
4. 计算保守 Scope Diff；
5. 存储不可变 Revision；
6. 仅行为变化时直接移动当前指针；
7. 资源扩大时保存为 `waiting_review`。

`abar revision approve` 保存 Review 证据，并且只有证据绑定当前基准 Revision
时才移动当前指针。

## Session 与 Message 存储

默认状态：

```text
<state-dir>/
├── revisions/
└── sessions/
    ├── approval-index.json
    └── <session-id>/
        ├── session.json
        └── messages/
            ├── <message-id>.json
            └── <message-id>.events.jsonl
```

写入原子化，Session 变更使用文件锁，Event 成功前执行 fsync。

## Message 执行

1. 解析当前 Agent Revision。
2. 解析 Provider Bundle 并持久化 Execution Composition。
3. 创建 `queued` Message。
4. 派发后台 Worker。
5. 通过 Prompt、Skill、Knowledge、历史、已投影 Tool 结果和当前输入组装上下文。
6. 执行模型和 Tool 循环。
7. 持久化审批、外部等待、输出、Tool 结果和 Event。

取消在模型和 Tool 调用前检查，并在模型响应后再次检查。

## 审批与恢复

待审批写证据保存 Tool ID、精确参数、幂等键和 Approval ID。审批不能跨 Message
或 Revision 复用。

重启行为：

| 状态 | 恢复 |
|---|---|
| `queued` | 派发 Worker |
| `running` | 从 checkpoint 或已批准写继续 |
| `waiting_approval` | 不自动执行 |
| `waiting_external` | 查询精确持久 Tool 调用 |

内置 MCP Adapter 调用配置的只读 `idempotencyCheckTool`。写结果缺失时保持未确认。

## 上下文与敏感数据

`ContextAssembler` 只使用已完成历史，不注入失败或取消输出。Tool 结果在进入模型
上下文前通过 `DataPolicy`；禁止字段被移除，敏感标量值会阻止其出现在模型输出。

## WebUI

`abar-webui --agent <name>` 只使用 `/v1` API，支持：

- Session 列表；
- 同一 Session 的 Message 历史；
- 在选中的 Session 内继续多轮追问；
- 通过明确的“确认执行”追加 Message，并产生写 Tool Intent；
- 按 Tenant 和角色过滤、仅含最小证据的审批收件箱；
- Composition、待审批、Tool 结果、输出和 Runtime Event；
- 批准和拒绝；
- 取消和外部对账。

WebUI 以对话为默认入口。用户可以使用任意 Message 创建 Session，并在同一
Session 内自由多轮追问和调整。View Pack 中的任务按钮只是可选 Prompt 快捷方式，
不是唯一路径。写操作仍然经过 Tool Scope、精确审批和外部幂等执行。

WebUI 启动时绑定一个 Agent 和一个 View Pack。`GET /api/view` 返回 View
Pack，Session 和审批查询都过滤为当前 Agent。Server 仍保留多 Agent 能力，
但 WebUI 不再维护未使用的 Agent 选择状态。

Session 列表由 Server 直接返回最新 Message 状态，避免逐行查询 Message。
`POST /v1/tasks` 以 `requestId` 幂等地创建 Session 和首个 Message；准备失败时
会归档占位 Session。WebUI 确认传递不透明的业务确认对象和 `agentRevision`，
并且只有所选 Message 的 Tool 证据包含完全相同的确认对象时才创建确认 Message。

BFF 在 Session 标题和对话中把结构化确认 Message 展示为业务文案。内部协议标记
和业务专用确认字段保留在持久化 Message 中，不向用户渲染。

Message Event 使用游标。轮询只请求 `eventCursor` 之后的 Event；对话渲染保留
用户滚动位置，过期的详情响应会被丢弃。浏览器 UI 拆分为 HTML、CSS 和 ES
Module，因此 CSP 使用 `script-src 'self'` 和 `style-src 'self'`，不需要
`unsafe-inline`。

OIDC 模式下，WebUI 使用 Authorization Code + PKCE，并作为 Backend for
Frontend。浏览器只持有不透明的 `HttpOnly`、`SameSite=Lax` 会话 Cookie；
Access Token 和 Refresh Token 只保存在 WebUI 内存。写请求需要会话绑定的
CSRF Token。浏览器不会接触 ABAR Server Bearer Token 或 IdP Access Token。

WebUI 在页面不可见时暂停轮询，恢复可见后执行一次串行刷新。

MCP Adapter 传递 `x-abar-session-id` 和 `x-abar-tenant-id`。业务 Tool 因此可以
跨多个 Message 固定同一数据快照。示例 Mock MCP 返回自己的确认字段，只有
`refreshSnapshot=true` 时才推进快照。

## 平台 Console

`abar-platform-console` 通过 OIDC BFF 使用 `/v1/control-plane/*`。Server 在返回
任何控制面数据前检查 `ABAR_CONTROL_PLANE_ROLE`。平台 Console 提供 Agent/Revision 目录、
Scope Review 和当前 Tenant 的 Runtime 聚合，不接收 Session 对话、Prompt、完整
Tool 参数或最终业务输出。

Scope Review 使用已认证 Subject ID 作为评审证据，批准后把候选 Revision 设为
Current。业务写审批仍是独立权限，由 Agent WebUI 处理。

## 身份与审批

本地模式使用生成的 Bearer Token、`approver` 角色和精确审批 Scope。OIDC 模式
可选执行严格 Token Introspection，再通过 UserInfo 解析 Subject、Tenant 和
Roles。Introspection 校验 Active、Issuer、Audience 和过期时间。
`server.json` 不保存 OIDC Access Token。

已验证身份按 Token 哈希缓存，TTL 由 `ABAR_OIDC_CACHE_SECONDS` 控制（默认
30 秒，最大 300 秒）。并发缓存未命中会合并，缓存时间不会超过 Introspection
返回的 Token 过期时间。

CLI 调用方仍通过 `ABAR_TOKEN` 提供 Token；WebUI 用户通过浏览器登录，Token
获取与刷新完全在服务端完成。

`GET /v1/approvals` 只返回同一 Tenant 内、当前认证角色有权处理的待审批任务。
投影包含审批坐标、Tool ID、精确参数、Agent Revision、申请人和等待时间；不返回
申请人的历史、最终输出、Composition 或 Tool 结果证据。

收件箱读取 Session Store 下持久化的 `approval-index.json`。审批请求、取消、
完成、失败和审批决定路径在写入状态时同步更新索引。Server 启动时从 Message
源记录重建；`abar state reindex-approvals` 提供显式离线重建。

审批接口在持久化决定前调用 `ApprovalAuthorizer`。缺少角色或批准 Scope 为空时
失败关闭并返回 HTTP 403；跨 Tenant 访问以 HTTP 404 隐藏；Approval ID 过期或
不匹配按非法输入拒绝，不执行写操作。

配置 `ABAR_OIDC_SCOPE_FIELD` 后，审批授权器可执行区域角色后缀，例如
`transfer-approver:华东` 或 `transfer-approver:华东,华南`。区域审批人只有在
角色覆盖 Tool Scope 数组中的全部值时才可见该待审批，也不能批准超过角色范围的
Scope。不带后缀的精确基础角色仍作为全局审批人。

## 诊断

`abar runtime status` 读取认证接口 `/v1/runtime/status`：

- 活跃 Worker 数；
- 调用者可见的 Session 和 Message 状态计数；
- Provider 配置校验；
- 状态存储写探针。

Server 还在回环监听地址上提供：

- `/healthz`：进程存活；
- `/readyz`：Provider 配置和存储就绪；
- `/metrics`：Prometheus Counter 和全局 Session/Message 状态 Gauge。

HTTP 错误、Worker 完成、Worker 失败和审批决定使用结构化 JSON 日志。Token、
Prompt、Tool 参数和输出不会写入日志。

模型 Prompt、Completion 和 Total Token Usage 会在同一 Message 的所有模型调用
中累计。审批或外部等待前的用量写入 Message Checkpoint，因此重启和恢复不会
丢失计量。`/metrics` 暴露
`abar_model_tokens_total{type="prompt|completion|total"}`。

## 备份与恢复

`abar state backup` 获取 Server 锁后生成 `tar.gz` 和 SHA-256 校验文件；Server
运行时拒绝执行。

`abar state restore` 在解压前验证归档路径和文件类型。使用 `--replace` 时保留
原状态目录，并在命令输出中返回其位置。

经理专用 TopN 提醒流程参见[钉钉告警](../../../examples/merchandise-operations/DINGTALK-ALERTS.zh-CN.md)。告警报告与模型
可调用的 MCP Tool 分离，Action Card 跳转到 OIDC 工作台。

## 验证

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
bash scripts/check.sh
bash scripts/test-merchandise-operations.sh
```

Shell 测试只保留一个业务场景：商品运营助手。核心契约、身份、Runtime 和存储
行为仍由 Rust 测试覆盖。
