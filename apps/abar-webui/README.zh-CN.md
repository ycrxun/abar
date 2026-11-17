# 通用 Agent WebUI

[English](README.md) | 简体中文

`abar-webui` 是独立的 Agent WebUI，不直接依赖 ABAR 内部实现，使用
`/v1` API。

它承载绑定 Agent 的对话、业务视图、审批和运行证据。

Agent WebUI 的共享 Shell 负责：

- 绑定一个 Agent 名称。
- 创建长期 Session 和异步 Message。
- 在一个长期 Session 中切换并查看历史 Message。
- 展示 Message Composition 和 Runtime Event。
- 展示 checkpoint/最终 Tool 结果和 Message 输出。
- 对 `waiting_approval` 执行批准、拒绝和范围提交。
- 取消排队或运行中的 Message。
- 查询或终止 `waiting_external` 外部恢复。
- 读取声明式 WebUI View Pack。

WebUI 始终绑定一个 Agent。View Pack 是可选配置；未提供 `--view` 时，内置
Codex 式 View 会在启动时绑定到当前 Agent。

浏览器不接触 Agent Server Bearer Token；WebUI 后端代理 API 请求。

BFF 增加了三项安全优化：

- 初始任务使用 `POST /v1/tasks` 和客户端 `requestId`，幂等创建 Session 与 Message；
- 确认请求提交不透明的业务确认对象和 `agentRevision`，并与 Message Tool
  证据做精确一致校验，不再通过自然语言让模型解释；
- Session 详情只请求 `eventCursor` 之后的新 Event。
- 内部控制消息 `[structured confirmation]` 只向用户展示业务文案；业务确认字段
  保留在持久化 Message 证据中。

CSS 和 JavaScript 使用独立静态资源，CSP 不允许内联脚本和样式。

## 启动

先启动 Mock MCP Server 和演示模型：

```bash
export ABAR_STATE_DIR=/tmp/abar-merchandise-webui
export ABAR_CONFIG="$PWD/examples/merchandise-operations/config.fake.yaml"
export ABAR_MESSAGE_CONFIG="$PWD/examples/merchandise-operations/message.providers.yaml"
cargo run -p abar-mock-mcp-server
cargo run -p abar-merchandise-demo-model
```

在另一个终端：

```bash
abar publish examples/merchandise-operations
abar server start --daemon
```

使用商品运营 View Pack 启动：

```bash
cargo run -p abar-webui -- \
  --agent merchandise-operations-assistant \
  --view views/merchandise-operations/webui-view.yaml
```

打开 <http://127.0.0.1:19090>。

使用默认 View 启动第二个 Agent：

```bash
cargo run -p abar-webui -- \
  --agent another-assistant \
  --addr 127.0.0.1:19091
```

View Pack 必须声明 `binding.agent`，且必须与 `--agent` 一致。

WebUI 读取 `ABAR_STATE_DIR/server.json` 取得本地 Server 连接信息，也可以用
`ABAR_ENDPOINT` 和 `ABAR_TOKEN` 指向远程 Server。`ABAR_WEBUI_AGENT` 选择
Agent，监听地址可通过 `ABAR_WEBUI_ADDR` 或 `--addr` 修改。

当前 WebUI 默认只监听 `127.0.0.1`，没有自己的登录系统，仅用于本地业务演示。
生产使用前必须在 WebUI 外层接入组织身份、角色和审批人校验。

## 验证

```bash
bash scripts/test-merchandise-operations.sh
```

View Pack 说明见 [views/README.zh-CN.md](../../views/README.zh-CN.md)。
