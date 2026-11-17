# 贡献指南

[English](CONTRIBUTING.md) | 简体中文

## 工具链

使用 `rust-toolchain.toml` 固定的工具链。

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/check.sh
```

## DCO

每个提交都必须包含 Developer Certificate of Origin 签核：

```text
Signed-off-by: Name <email>
```

使用 `git commit -s`。

## ABAR 变更要求

每次 ABAR 变更都必须包含：

- 测试。
- `abar-api` 和 Schema 影响说明。
- 默认运行形态的证据。
- 可移植性回答：该能力是否对没有特定业务假设的无关 Agent 仍然成立？
- 依赖移除回答：移除新增依赖后，ABAR 是否仍能以默认本地形态运行？

不得把垂直业务 Skill 加入 `abar-core`。

修改公共契约前，先同步更新中英文文档，再修改契约、测试和代码。保持契约不变的
缺陷和安全修复可以直接处理。

## 模块边界

仓库级模块规则见 [AGENTS.zh-CN.md](AGENTS.zh-CN.md)，重点包括：

- 入口文件只作为组合根。
- 一个模块承担一个完整职责。
- Rust 源文件有 500 行自动检查上限。
- 存量超限文件不得增长，实质修改时必须拆分。

不允许模块大小例外。

## 贡献者文档

贡献者文档描述实现、契约、运行和测试。

每个文档变更都必须同步更新对应的中英文版本。
