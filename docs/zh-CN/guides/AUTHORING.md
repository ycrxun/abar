# 业务定义 Agent

[English](../../en/guides/AUTHORING.md) | 简体中文

这份指南面向业务负责人和业务 Agent 设计者。读完后，你应当能够把一个岗位的
工作目标转化为可发布、可验证、可治理的 Agent。

平台负责身份、执行、审批、恢复、计量和调度；业务负责定义 Agent 为什么存在、
知道什么、能做什么，以及什么情况下必须停下来。

## 1. 先判断是否应该做一个 Agent

一个 Agent 应代表一个相对稳定的业务职责。以下信号说明适合拆分：

- 服务不同岗位或不同用户群；
- 需要不同的 Tool 和写权限；
- 访问不同数据范围；
- 有不同的审批人或审批范围；
- 有独立负责人和独立发布节奏。

不要因为页面按钮不同就拆分 Agent。例如商品运营中的配货、补货和调拨可以由同一
Agent 识别和路由，因为它们共享业务对象、数据和用户群。

## 2. 业务定义与平台职责

| 业务方定义 | 平台提供 |
|---|---|
| Agent 目标和边界 | 身份、Tenant 和权限执行 |
| Prompt、Skill、规则和知识 | Build、不可变 Revision 和 Scope Review |
| Tool 白名单与读写范围 | Tool 调用治理、审批、幂等和恢复 |
| 数据允许与禁止类别 | DataPolicy 投影和模型上下文隔离 |
| 模型能力和规模 | 具体 Model Provider 和运行实现 |
| 业务快捷入口和结果展示 | Agent WebUI、会话和实时同步 |
| 写操作的业务范围和审批所需 Scope | 审批人身份、角色、可见性和最终授权 |

业务不定义容器、进程、检查点格式、重试次数或某个厂商 SDK。

## 3. Agent 项目的文件结构

```text
my-agent/
├── agent.yaml
├── scope.yaml
├── behavior.yaml
├── knowledge.yaml
├── tools.yaml
├── prompt/
│   └── system.md
├── skills/
│   └── <skill-id>/
│       └── SKILL.md
└── rules/
    └── <rule-id>.md
```

推荐从商品运营示例开始：

```bash
cp -R examples/merchandise-operations ../agents/my-business-agent
```

## 4. 定义 Agent 身份和执行上限

`agent.yaml`：

```yaml
apiVersion: abar/v1
kind: Agent
metadata:
  name: merchandise-operations-assistant
spec:
  modelClass: general-tool-use
  maxSteps: 40
  scope: scope.yaml
  behavior: behavior.yaml
  knowledge: knowledge.yaml
  tools: tools.yaml
```

`metadata.name` 是稳定身份。Agent WebUI、Session 和部署路由都用它识别业务
Agent。

`modelClass` 表达能力等级，不绑定厂商模型；`maxSteps` 限制一次 Message 的最大
推理和 Tool 循环次数。

## 5. 定义权限范围

`scope.yaml` 回答“这个 Agent 最多能接触什么”：

```yaml
tools:
  read:
    - inventory.query_*
  write:
    - inventory.create-transfer-order
skills:
  - transfer-decision
knowledgeSources:
  - transfer-policy
data:
  allow:
    - inventory
    - sales
  deny:
    - cost
    - gross-margin
```

定义原则：

- 先给完成职责所需的最小范围；
- `write` Tool 必须进入写审批链路；
- 数据 `deny` 优先于“模型可能不需要”；
- 扩大 Tool、Skill、Knowledge 或数据范围会触发 Scope Review；
- 仅修改 Prompt、规则或知识内容不会自动扩大资源边界。

审批人和角色不写在 `agent.yaml` 中，由平台 IAM 和
`ApprovalAuthorizer` 配置与执行。

## 6. 定义 Agent 如何思考

`behavior.yaml` 绑定 Prompt、Skill 和规则：

```yaml
prompt: prompt/system.md
skills:
  - id: merchandise-intent-routing
    version: "1.0.0"
    path: skills/merchandise-intent-routing
rules:
  - id: merchandise-routing-rules
    version: "1"
    path: rules/routing-rules.md
```

Prompt 应写职责、边界和停止条件，不要把 Tool 参数格式或平台恢复机制写进 Prompt。

Skill 适合可复用的工作方法和 Tool 使用规范。规则适合业务口径、判断条件和处置
边界。知识适合可能随业务演进的制度、政策和说明文档。

## 7. 绑定 Tool 与知识

`tools.yaml` 声明 Agent 实际使用的 Tool：

```yaml
tools:
  - id: inventory.query-inventory
    versionRange: ">=4.7 <5.0"
    effect: read
  - id: inventory.create-transfer-order
    versionRange: ">=6.2 <7.0"
    effect: write
```

`knowledge.yaml` 声明知识 Source：

```yaml
sources:
  - id: transfer-policy
    required: true
    retrieval:
      topK: 4
```

Tool ID、版本和 MCP 服务由平台或集成团队提供。业务方负责选择允许使用哪些 Tool，
并确认读写 effect。知识 Source 的实际文件位置和检索实现属于运行时配置。

## 8. 处理敏感数据

业务先在 `scope.data` 声明允许和禁止的数据类别。平台在 Tool 结果进入模型上下文
前执行 DataPolicy。

例如客户明确禁止成本、毛利和供应商价格进入模型：

```yaml
data:
  allow: [inventory, sales]
  deny: [cost, gross-margin, supplier-price]
```

集成配置还必须把真实字段映射到这些类别：

```yaml
dataPolicy:
  fields:
    - path: unitCost
      dataClass: cost
    - path: grossMargin
      dataClass: gross-margin
```

仅写 `deny` 而不配置字段映射，不能形成可验证的数据保护。

## 9. 定义 WebUI 业务视图

View 可选。未提供 View 时，Agent WebUI 使用默认 Codex 式界面。

View 只能增加：

- 快捷提问字段和 Prompt 模板；
- 结构化业务确认文案；
- 业务指标、表格和展示格式；
- 审批和结果展示文案。
- 是否显示右侧上下文证据栏。
- 无写操作的 Agent 是否显示审批导航。

View 不能改变 Tool Scope、数据策略、审批权限或 Message 状态。

## 10. 验证 Agent

先启动示例专用的确定性模型和 Mock MCP，再验证业务契约：

```bash
export ABAR_STATE_DIR=/tmp/my-agent
export ABAR_CONFIG="$PWD/examples/merchandise-operations/config.fake.yaml"
export ABAR_MESSAGE_CONFIG="$PWD/examples/merchandise-operations/message.providers.yaml"

cargo run -p abar-merchandise-demo-model
cargo run -p abar-mock-mcp-server

abar publish ../agents/my-business-agent --json
```

至少验证：

- 典型请求能选择正确 Tool；
- 信息不足时先澄清；
- 越权 Tool 和敏感字段被拒绝；
- 写 Tool 在审批前不执行；
- 重复写请求使用同一个幂等键；
- 不存在的业务数据不会被模型编造。

业务验收应覆盖正常、歧义、拒绝、敏感数据和外部失败五类场景。

## 11. 发布与 Scope Review

行为内容变化会直接产生新的 Current Revision。资源范围扩大时，Revision 保持候选
状态，等待 Scope Review。

CLI：

```bash
abar revision approve <agent>:<revision> \
  --reviewer <reviewer> \
  --comment "approved by business and security"
```

也可以在平台 Console 的 Agent 目录中查看 Scope Diff 并批准或拒绝。

## 12. 上线 Agent WebUI

```bash
abar-webui \
  --agent <agent-name> \
  --view views/<business>/webui-view.yaml \
  --addr 127.0.0.1:19093
```

每个 Agent 使用独立 WebUI。业务用户从企业门户、钉钉或其他渠道进入对应 Agent，
不需要在页面内选择 Agent。

## 13. 发布前检查表

- Agent 只有一个明确的业务职责和负责人；
- Prompt、Skill、规则和知识的职责没有重复冲突；
- Tool 白名单达到最小可用范围；
- 所有写 Tool 都有精确审批和幂等设计；
- 禁止进入模型的数据字段已经在运行配置中映射；
- 典型、歧义、越权和外部失败场景已经验证；
- View 只做展示，不承载权限或业务状态；
- Scope 扩大的 Revision 已由正确角色评审；
- Agent WebUI 的上线地址和登录角色已经确认。
