# ABAR 核心模型

[English](../../en/concepts/CORE-MODEL.md) | 简体中文

ABAR 是一个薄 Agent Build/Runtime 核心。

```text
Agent
├── Agent Revision
├── Session
│   └── Message
└── Tool Service
```

## 1. Agent Revision

Build 解析、校验并封存一个不可变 Agent Revision：

```text
Agent Revision
├── resourceScope
├── behavior
├── knowledgeSources
└── toolBindings
```

`resourceScope` 固定 Tool ID 与最大 effect、Skill ID、Knowledge Source ID、
数据类别、模型类别和最大步骤数。

`behavior` 固定带摘要的 Prompt、Skill 和业务规则引用。普通知识文档允许演进；
每个 Message 记录实际使用的文档、版本、摘要和检索时间。

资源扩大时生成候选 Revision，并要求 Scope Review。仅行为变化可以直接成为当前
Revision。

## 2. Session

Session 是归属于一个已认证 Tenant 和用户的长期上下文容器，不是进程、容器或
单次任务。

```text
active <-> idle
idle -> closed -> archived
```

一个 Session 同时只能有一个活动 Message。

## 3. Message

Message 是一个 Agent Revision 的一次执行。

```text
queued -> running
running -> waiting_approval | waiting_external | completed | failed | cancelled
waiting_approval -> running | failed | cancelled
waiting_external -> running | failed | cancelled
```

每个 Message 记录输入、请求的 Skill、Agent Revision、实际 Model、Tool、Skill、
Knowledge 和 Schema、Tool 结果、审批证据、checkpoint、输出和 Runtime Event。

当前 Agent Revision 更新后，历史 Message 不变。

## 4. 执行

```text
Message
  -> Context Assembly
  -> Model
  -> Tool Intent
  -> Scope and DataPolicy
  -> Tool Execution
  -> Evidence / Approval / External Wait
  -> Result or Recovery
```

Message 创建先持久化 `queued`，再派发后台 Worker。取消为协作式，在下一个模型或
Tool 边界提交；已确认 Tool 证据不会因取消丢失。

`waiting_external` 保存精确 Tool ID、参数和幂等键。写结果缺失时保持等待，绝不
静默重发；读操作可以按同一持久意图安全重试。

启动恢复会继续 `queued`、`running` 和 `waiting_external` Message。
`waiting_approval` 保持挂起，等待审批决定。

## 5. 安全不变量

- 模型只产生意图，不获得执行权限；
- Tool 调用必须通过 Revision 范围和 effect 校验；
- 写操作必须精确审批和幂等；
- 敏感数据进入模型上下文前必须分类和投影；
- 禁止类别失败关闭；
- 日志和诊断不暴露 Token、Secret 或宿主路径。

## 6. 扩展边界

稳定扩展边界为 `ModelProvider`、`ToolRegistry`、`KnowledgeProvider`、
`DataPolicy`、`ContentProvider` 和 `AuthProvider`。Adapter 只依赖
`abar-api` 及其契约校验器。

审批权限是独立的 `ApprovalAuthorizer` 边界。身份认证不代表有权批准业务写操作。
审批收件箱只在同一 Tenant 和审批角色范围内暴露最小待调用证据，不授予申请人的
Session 历史或输出访问权。
