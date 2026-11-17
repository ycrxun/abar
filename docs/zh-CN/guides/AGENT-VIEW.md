# Agent View 配置

[English](../../en/guides/AGENT-VIEW.md) | 简体中文

Agent View 是单个 Agent WebUI 的声明式展示配置，不进入 Agent Runtime。

一个 WebUI 只绑定一个 Agent，一个 View 也只能绑定这个 Agent。未提供 View
时，WebUI 使用内置的默认 View。View 只改变信息如何展示，不改变身份、权限、
Tool Scope、DataPolicy、审批或 Message 状态。

## 1. 最小配置

创建 `agent-view.yaml`：

```yaml
apiVersion: webui/v1
kind: WebUIView
binding:
  agent: example-assistant
metadata:
  name: example-view
  title: Example Assistant
task:
  title: 开始对话
  chatPlaceholder: 描述目标
  fields: []
  actions: []
```

启动 WebUI 时必须绑定同一个 Agent：

```bash
abar-webui \
  --agent example-assistant \
  --view ./agent-view.yaml
```

如果 `binding.agent` 与 `--agent` 不一致，WebUI 会拒绝启动。

## 2. 完整配置示例

以下配置展示当前 `webui/v1` 支持的主要展示项：

```yaml
apiVersion: webui/v1
kind: WebUIView
binding:
  agent: example-assistant
metadata:
  name: example-view
  title: Example Assistant
  subtitle: 对话、结构化结果与执行证据
  badges:
    - Read-only
    - Example

task:
  title: 今天要处理什么任务？
  chatPlaceholder: 描述目标，或选择一个快捷操作
  fields:
    - name: scope
      label: 范围
      type: select
      defaultValue: all
      options: [all, group-a, group-b]
    - name: note
      label: 补充要求
      type: textarea
      placeholder: 可选
  actions:
    - id: summarize
      label: 生成摘要
      primary: true
      inputTemplate: "汇总{{scope}}范围内的任务。{{note}}"

detail:
  titleTemplate: "{{title}}"
  titleToolId: example.read_summary
  titlePath: title

confirmation:
  label: 确认执行
  inputTemplate: 确认执行当前会话中已展示的业务方案。
  requestedSkills:
    - execution-guidance

metrics:
  - label: 已处理
    toolId: example.read_summary
    path: summary.processed
  - label: 总量
    toolId: example.read_summary
    path: summary.total

panels:
  - type: table
    title: 结果明细
    toolId: example.read_summary
    rowsPath: items
    meta:
      - label: 数据时间
        path: dataAsOf
    columns:
      - label: 名称
        path: name
      - label: 状态
        path: status
      - label: 数量
        path: quantity

context:
  enabled: true

approval:
  enabled: true
  title: 等待人工审批
  approveLabel: 批准
  rejectLabel: 拒绝
```

## 3. 配置字段

### `binding`

```yaml
binding:
  agent: example-assistant
```

`agent` 必须是 Agent Revision 中的稳定名称，并且与 WebUI 启动参数
`--agent` 完全一致。

### `metadata`

```yaml
metadata:
  name: example-view
  title: Example Assistant
  subtitle: 对话、结构化结果与执行证据
  badges:
    - Read-only
```

| 字段 | 必填 | 用途 |
|---|---|---|
| `name` | 是 | View 的稳定名称 |
| `title` | 是 | 页面标题、Agent 标题和浮动消息框标题 |
| `subtitle` | 否 | Agent 说明 |
| `badges` | 否 | 用户区域旁的轻量状态标签 |

### `task`

`task` 定义新对话的空态和快捷输入。自由输入始终可用，快捷操作不是强制流程。

```yaml
task:
  title: 今天要处理什么任务？
  chatPlaceholder: 描述目标
  fields:
    - name: scope
      label: 范围
      type: select
      defaultValue: all
      options: [all, group-a, group-b]
  actions:
    - id: summarize
      label: 生成摘要
      primary: true
      inputTemplate: "汇总{{scope}}范围内的任务。"
```

字段约束：

- `fields[].name` 必须以字母开头，只能包含字母、数字、`_` 和 `-`；
- `type` 支持 `text`、`textarea` 和 `select`；
- `select` 的 `defaultValue` 应属于 `options`；
- `actions[].id` 使用小写字母开头，只能包含小写字母、数字、`_` 和 `-`；
- `inputTemplate` 使用 `{{fieldName}}` 引用字段值。

### `detail`

`detail` 控制对话详情标题，可以从指定 Tool 结果中读取一个值：

```yaml
detail:
  titleTemplate: "{{title}}"
  titleToolId: example.read_summary
  titlePath: title
```

缺少 `titleToolId` 或 `titlePath` 时使用“任务详情”作为默认标题。

### `confirmation`

`confirmation` 定义业务确认按钮和确认 Message 的展示文案：

```yaml
confirmation:
  label: 确认执行
  inputTemplate: 确认执行当前会话中已展示的业务方案。
  requestedSkills:
    - execution-guidance
```

- `label` 和 `inputTemplate` 必填；
- `requestedSkills` 是确认 Message 可使用的 Skill；
- 业务确认字段由 Tool 结果携带，平台只校验其与 Message 证据是否完全一致；
- 内部 `[structured confirmation]` 前缀和业务确认字段不会展示给用户。

### `metrics`

`metrics` 从 Tool 结果中提取轻量指标：

```yaml
metrics:
  - label: 已处理
    toolId: example.read_summary
    path: summary.processed
```

`path` 使用点分隔路径读取 JSON。`toolId` 使用当前 Message 中最近一次匹配的
Tool 结果。

### `panels`

当前 Panel 类型为 `table`：

```yaml
panels:
  - type: table
    title: 结果明细
    toolId: example.read_summary
    rowsPath: items
    meta:
      - label: 数据时间
        path: dataAsOf
    columns:
      - label: 名称
        path: name
      - label: 来源与目标
        paths: [source, target]
        format: "{{source}} → {{target}}"
      - label: 原因
        path: reasons
        join: ", "
```

列可以配置：

| 字段 | 用途 |
|---|---|
| `path` | 读取单值 |
| `paths` | 同时读取多个值 |
| `format` | 使用 `{{path}}` 组合展示文本 |
| `join` | 数组值的连接符 |
| `className` | 附加 CSS 类 |

### `context`

上下文栏默认显示。只读或结果简单的 Agent 可以隐藏它：

```yaml
context:
  enabled: false
```

该配置只影响展示，已持久化的 Composition、Tool 结果和 Runtime Event 仍然保留。

### `approval`

审批界面默认显示。没有写操作的 Agent 可以隐藏审批导航：

```yaml
approval:
  enabled: false
```

这项配置只控制展示。只要 Agent Revision 中存在写 Tool，Server 仍会强制执行审批。

还可以配置审批区文案：

```yaml
approval:
  enabled: true
  title: 等待人工审批
  approveLabel: 批准
  rejectLabel: 拒绝
```

## 4. 默认值合并

View 是默认 View 的增量覆盖：

- 缺失的对象字段从默认 View 继承；
- 数组和标量使用 View 中的值替换默认值；
- 未提供 `--view` 时直接使用内置默认 View；
- 默认 View 绑定 WebUI 启动时传入的 Agent 名称。

因此业务 View 不需要重复定义 Shell、Session 列表、Composer、审批卡或上下文栏。

## 5. 安全边界

View 不能：

- 修改认证、Tenant、用户身份或 Agent 绑定；
- 扩大 Tool Scope 或 DataPolicy；
- 跳过审批；
- 修改 Message 状态迁移；
- 执行脚本或模板代码；
- 读取另一个 Agent 的 Session 或 Tool 结果。

View 中出现未知字段或不符合 Schema 的结构时，应视为无效配置。

## 6. 启动与验证

使用自定义 View：

```bash
abar-webui \
  --agent example-assistant \
  --view ./agent-view.yaml \
  --addr 127.0.0.1:19090
```

使用默认 View：

```bash
abar-webui \
  --agent example-assistant \
  --addr 127.0.0.1:19090
```

启动后检查：

```bash
curl -fsS http://127.0.0.1:19090/api/view | jq
```

确认：

1. `binding.agent` 与 `--agent` 一致；
2. `metadata.title` 和 Shell 文案符合预期；
3. 快捷操作能填入 Composer，但不限制自由输入；
4. 指标、表格和上下文项从正确的 Tool 结果读取；
5. 隐藏 `context` 或 `approval` 后只改变展示，不改变 Server 行为。

机器约束以
[`webui-view.schema.json`](../../../views/schema/v1/webui-view.schema.json) 为准，
可直接运行示例见 [`views/README.zh-CN.md`](../../../views/README.zh-CN.md)。
