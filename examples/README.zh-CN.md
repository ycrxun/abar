# 示例

[English](README.md) | 简体中文

本目录是全部示例的统一入口，包括最小示例和端到端业务示例。

规则：

- 最小示例避免垂直业务逻辑。
- 业务工作流放在独立子目录，不进入 `abar-core`。
- 每个示例必须能从仓库直接运行，不依赖隐藏配置。
- 业务示例必须说明数据假设、审批和安全边界。

定义新的业务 Agent 时，先阅读[业务定义 Agent 指南](../docs/zh-CN/guides/AUTHORING.md)。

## 可用示例

| 示例 | 范围 |
|---|---|
| [商品运营助手](merchandise-operations/README.zh-CN.md) | 对话识别配货、补货、调拨，以及 ERP/WMS 接入和钉钉 TopN 告警 |
| [知识问答助手](knowledge-qa/README.zh-CN.md) | 基于版本化业务知识的只读问答，以及独立 Agent WebUI |

商品运营示例同时包含确定性的 OpenAI-compatible 模型与 Mock MCP Server，
使业务词汇和行为完全留在示例内，不进入通用 Runtime 与 Adapter。
