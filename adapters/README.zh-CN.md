# Adapters

[English](README.md) | 简体中文

参考 Adapter 只依赖 `abar-api`。

- `abar-model-fake`：确定性测试 ModelProvider。
- `abar-model-openai`：OpenAI-compatible ModelProvider。
- `abar-auth-oidc`：严格 Token Introspection、OIDC UserInfo 身份和角色审批授权。
- [`abar-channel-dingtalk`](abar-channel-dingtalk/README.zh-CN.md)：签名钉钉机器人
  Action Card 和工作台深链。
- `abar-tool-fake`：确定性 ToolRegistry。
- `abar-tool-mcp-http`：支持精确幂等查询的 MCP Streamable HTTP ToolRegistry。

业务专用的示例 Provider 和 Server 位于 `examples/`，不进入通用 Adapter。
