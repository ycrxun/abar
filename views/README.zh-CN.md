# WebUI View Packs

[English](README.md) | 简体中文

View Pack 是 WebUI 的声明式展示配置，不进入 Agent Runtime。

没有独立 View Pack 时，内置默认 View 保留 Codex 式 Shell。View Pack 只负责：

- 可选的场景建议字段和 Prompt 模板。
- 方案生成后的确认按钮与 Message 模板。
- 业务指标、表格和列格式。
- 展示标题与业务文案。
- 是否展示右侧上下文栏。
- 是否展示审批收件箱和审批界面。

View Pack 不支持脚本执行，也不能扩大 Agent 工具权限或绕过审批。

所有 View Pack 复用同一套 Codex 式 Shell、左侧会话栏、中央对话、单一 Composer
和右侧上下文栏。View Pack 只向公共 Shell 增加业务快捷指令、指标、表格和文案，
不能替换交互骨架。

右侧上下文栏默认显示。View 可通过以下配置隐藏仅用于展示的运行证据：

```yaml
context:
  enabled: false
```

执行证据仍会持久化并可供审计，只是不对当前 View 展示。

审批界面默认显示。没有写操作能力的 Agent 可以在 View 中隐藏：

```yaml
approval:
  enabled: false
```

该配置只移除展示；只要 Agent Revision 中存在写 Tool，Server 仍会强制执行审批。

WebUI 以对话为默认入口，自由输入始终可用。View Pack 中的场景按钮只是快捷
建议，不限制 Session 内的提问顺序、追问和方案调整。

每个 WebUI 是单 Agent 部署；多个 Agent 使用多个独立 WebUI。每个 View Pack
必须声明并绑定一个 Agent；未提供 View Pack 时使用默认 View。参见
[Agent View 配置](../docs/zh-CN/guides/AGENT-VIEW.md)。

```yaml
binding:
  agent: merchandise-operations-assistant
```

当前示例：

- `merchandise-operations/webui-view.yaml`：商品运营助手。
