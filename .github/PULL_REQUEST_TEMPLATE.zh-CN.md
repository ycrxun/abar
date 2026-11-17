## 变更

[English](PULL_REQUEST_TEMPLATE.md) | 简体中文

说明问题和变更内容。

## 兼容性

- [ ] 已记录 `abar-api` 影响。
- [ ] 已记录 Agent Revision、Session、Message 和 Runtime Event 影响。
- [ ] CLI 行为保持不变，或已明确版本化。

## 验证

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`

## ABAR 边界

- [ ] 未向 `abar-core` 引入垂直业务逻辑。
- [ ] 没有把可选集成变成必需运行时依赖。
- [ ] 默认本地和测试形态仍不依赖外部服务。

## 模块规则

- [ ] 入口文件仍是组合根，不承载领域处理逻辑。
- [ ] 新增或实质修改的 Rust 文件低于 500 行。
- [ ] 存量超限文件没有增长，被触碰的职责已拆分。
- [ ] `bash scripts/check.sh` 通过。

## 文档

- [ ] 中英文文档版本已同步更新。
