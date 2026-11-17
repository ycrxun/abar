# Agent Build And Runtime (ABAR)

[English](README.md) | 简体中文

ABAR 是一个使用 Rust 实现的薄层业务中立 Agent Build/Runtime 核心。

```text
Agent Revision -> Session -> Message -> Runtime Event
```

## 按角色开始

- 业务负责人：[定义业务 Agent](docs/zh-CN/guides/AUTHORING.md)
- 业务用户：[Agent WebUI](apps/abar-webui/README.zh-CN.md)
- 平台管理员：[平台 Console](apps/abar-platform-console/README.zh-CN.md)
- 平台开发者：[实现设计](docs/zh-CN/guides/IMPLEMENTATION.md)和
  [契约与接口](docs/zh-CN/reference/CONTRACTS.md)

## 已实现

- 不可变 Agent Revision Build 和 Scope Review；
- 长期 Session 和异步 Message Runtime；
- 后台执行、取消和重启恢复；
- 精确写审批和外部幂等对账；
- 已投影 Context Assembly 和敏感字段 DataPolicy；
- MCP Streamable HTTP Tool 和 OpenAI-compatible Model；
- OIDC 严格 Introspection、UserInfo 身份和基于角色的审批授权；
- WebUI Authorization Code + PKCE 登录和安全 BFF 会话；
- 按 Tenant/角色隔离的审批收件箱；
- Readiness、Prometheus 指标和结构化运行事件；
- 有界 OIDC 身份缓存和持久化审批收件箱索引；
- Offline 状态备份/恢复和 300 门店负载测试；
- 多 Agent WebUI 和对话优先的商品运营统一 Agent，自动识别配货、补货、调拨；
- 独立平台 Console，用于 Agent 目录、Scope Review 和 Runtime 聚合；
- 钉钉经理告警和按 Agent 路由的工作台深链；
- 按调用者隔离的 Runtime 诊断；
- Provider 契约校验器；
- 商品运营助手端到端测试；

## 快速开始

```bash
export ABAR_STATE_DIR=/tmp/abar-demo
export ABAR_CONFIG="$PWD/examples/merchandise-operations/config.fake.yaml"
export ABAR_MESSAGE_CONFIG="$PWD/examples/merchandise-operations/message.providers.yaml"

cargo run -p abar-mock-mcp-server
```

在另一个终端启动示例专用的确定性 OpenAI-compatible 模型：

```bash
cargo run -p abar-merchandise-demo-model
```

在第三个终端：

```bash
abar publish examples/merchandise-operations
abar server start --daemon
abar session create --agent merchandise-operations-assistant
abar message create <session-id> \
  --input "看看华东库存异常门店，判断应该补货、配货还是门店间调拨"
abar runtime status
```

商品运营统一 Agent 见[商品运营助手](examples/merchandise-operations/README.zh-CN.md)。

## Workspace

```text
abar-api/       契约、Schema、OpenAPI、一致性校验器
abar-core/      Revision Build、Session/Message Runtime、Worker、Provider
abar-cli/       abar
adapters/       可复用的 Model、Tool 和渠道 Adapter
apps/abar-webui/
apps/abar-platform-console/
examples/
  merchandise-operations/  Agent、Mock MCP 和演示模型
views/
```

## 文档

- [业务定义 Agent](docs/zh-CN/guides/AUTHORING.md)
- [核心模型](docs/zh-CN/concepts/CORE-MODEL.md)
- [系统架构](docs/zh-CN/concepts/ARCHITECTURE.md)
- [契约与接口](docs/zh-CN/reference/CONTRACTS.md)
- [实现设计](docs/zh-CN/guides/IMPLEMENTATION.md)
- [CLI](docs/zh-CN/guides/CLI.md)
- [身份与审批](docs/zh-CN/guides/IDENTITY-AND-APPROVAL.md)
- [钉钉工作台入口](adapters/abar-channel-dingtalk/README.zh-CN.md)
- [平台 Console](apps/abar-platform-console/README.zh-CN.md)
- [OpenAPI v1](abar-api/openapi/server-v1.json)

## 验证

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
bash scripts/check.sh
bash scripts/test-merchandise-operations.sh
bash scripts/test-platform-console.sh
```

Shell 测试保留一个 Agent 业务场景（商品运营助手），并新增独立的平台 Console
控制面场景。
