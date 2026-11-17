# 仓库开发规则

[English](AGENTS.md) | 简体中文

这些规则适用于 `abar-api/`、`abar-core/`、`abar-cli/`、`adapters/`
和 `apps/` 中的全部 Rust 模块。

## 文档事实源

- 修改运行行为前，先阅读[系统架构](docs/zh-CN/concepts/ARCHITECTURE.md)、
  [契约](docs/zh-CN/reference/CONTRACTS.md)和
  [实现设计](docs/zh-CN/guides/IMPLEMENTATION.md)。
- 每个贡献者可见文档都必须有中英文两个版本，并在同一次变更中同步更新。
- 英文技术文档位于 `docs/en/`，中文技术文档位于 `docs/zh-CN/`。根目录
  README、AGENTS、贡献、治理和安全文档使用 `name.md` 与
  `name.zh-CN.md` 配对。
- Markdown 是人类开发者的事实源；Rust 类型、JSON Schema 和 OpenAPI 是机器
  可读契约。

## 变更流程

- 修改公共契约前，先更新 ABAR 中英文文档。
- 文档与目标一致后，再更新契约、测试和实现。
- 不得从示例或偶发的实现细节反推契约。
- 纯缺陷、安全和内部重构可以直接处理；如果改变行为或公共契约，必须在同一
  变更中同步更新中英文文档。

## 模块职责

- `main.rs` 或 `lib.rs` 只作为组合根，负责模块声明、命令路由和明确的公开
  导出，不承载命令处理、Provider 构造或存储逻辑。
- 一个模块只承担一个完整职责。CLI 命令、传输客户端、进程管理、Provider
  Adapter、持久化和领域规则必须分文件管理。
- 共享工具只有形成稳定职责后才能进入独立模块，禁止创建泛化的 `utils`
  文件。
- 依赖方向固定为：`abar-cli -> abar-api`、`abar-core -> abar-api`、
  `adapters -> abar-api`。模块不得穿透其他模块的私有实现。
- 所有内部 workspace crate 使用 `abar-` 前缀。契约 crate 为 `abar-api`，
  实现 crate 为 `abar-core`，CLI crate 为 `abar-cli`。领域术语使用
  `agent.yaml`、`agent_name` 和 Agent Revision。
- 项目全称为 `Agent Build And Runtime`，缩写为 `ABAR`。CLI 二进制为
  `abar`，环境变量使用 `ABAR_`，状态目录为 `~/.abar`。

## 文件规模

- 新增或实质修改的 Rust 源文件以 400 行为目标。
- 500 行是自动检查上限。达到 500 行后，必须先按职责拆分，才能加入无关行为。
- 禁止为了行数进行无语义拆分。
- 任何 Rust 模块都不得超过上限；应拆分职责，而不是增加例外基线。
- 生成文件、锁文件、fixture 和非 Rust 资产不计入 Rust 模块行数。

提交前运行：

```text
bash scripts/check.sh
```

行为变更还必须运行：

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## 变更约束

- 结构重构必须保持行为不变，除非变更明确包含行为或契约更新。
- 在受影响边界增加或更新测试。
- 模块职责或依赖方向变化时，同步更新架构文档。
- 不得用一个超大文件隐藏跨领域耦合。
- 贡献者文档描述实现、契约、运行和测试。
