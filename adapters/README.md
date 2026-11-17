# Adapters

English | [简体中文](README.zh-CN.md)

Reference adapters depend only on `abar-api`.

- `abar-model-fake`: deterministic test ModelProvider.
- `abar-model-openai`: OpenAI-compatible ModelProvider.
- `abar-auth-oidc`: strict token introspection, OIDC UserInfo identity, and
  role-based approval authorization.
- [`abar-channel-dingtalk`](abar-channel-dingtalk/README.md): signed DingTalk
  robot action cards with Workbench deep links.
- `abar-tool-fake`: deterministic ToolRegistry.
- `abar-tool-mcp-http`: MCP Streamable HTTP ToolRegistry with exact idempotency
  lookup.

Business-specific example providers and servers live under `examples/`.
