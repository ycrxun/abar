# 系统架构

[English](../../en/concepts/ARCHITECTURE.md) | 简体中文

```text
abar CLI / WebUI
        |
        v
    Agent Server
        |
        +-- Agent Revision Store
        +-- Session / Message Store
        +-- Runtime Events
        +-- Message Worker
                |
                +-- ModelProvider
                +-- ToolRegistry / MCP
                +-- KnowledgeProvider
                +-- DataPolicy
```

## 职责

| 组件 | 职责 |
|---|---|
| `abar-api` | 稳定契约、Schema 和校验器 |
| `abar-core` | Revision Build、Session/Message 状态、Worker 和 Provider |
| `abar-cli` | 发布、Revision 审批、Server、Session、Message 和诊断 |
| `abar-webui` | OIDC BFF、Agent 对话、业务视图和审批界面 |
| `abar-platform-console` | Agent 目录、Revision 治理、Scope Review 和 Runtime 聚合 |
| `adapters` | 可替换 Model 和 Tool Provider |
| `examples` | 可执行的商品运营助手集成示例 |

## Build

`abar publish` 读取 Revision 项目，计算 Behavior 和 Scope 摘要，校验并不可变
存储 Revision。

- 仅行为变化自动成为当前 Revision。
- 资源范围扩大进入 `waiting_review`。
- Scope Review 证据绑定上一 Revision 和候选 Revision。

## Runtime

1. 客户端创建长期 Session。
2. 客户端创建 Message。
3. Server 解析当前 Revision 和 Provider Bundle。
4. Server 持久化 `queued` Message 证据。
5. Worker 执行模型和 Tool 循环。
6. 写操作挂起并等待精确审批。
7. 未知写结果挂起并等待精确幂等对账。
8. Worker 记录 Tool 结果、checkpoint、输出和 Runtime Event。

## 恢复

恢复状态保存在 Worker 之外：

- Session 和 Message JSON 记录；
- Execution Composition；
- 审批和待外部调用证据；
- checkpoint 和 Tool 结果；
- append-only Runtime Event。

启动时恢复 `queued`、`running` 和 `waiting_external` Message。审批等待需要
新的审批决定。

## 边界

- Worker 为进程内实现；跨进程分布式租约不在薄核心内。
- 诊断接口只读且按调用者隔离。
- WebUI 通过 OIDC BFF 边界隔离浏览器会话和 IdP Token。
- 组织 IAM 仍属于外部 `AuthProvider`；审批可见范围和决定权限由
  `ApprovalAuthorizer` 强制。
- 内置 Tool 传输为 MCP Streamable HTTP。
