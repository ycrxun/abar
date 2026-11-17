# Contributing

English | [简体中文](CONTRIBUTING.zh-CN.md)

## Toolchain

Use the toolchain pinned in `rust-toolchain.toml`.

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/check.sh
```

## DCO

Every commit must include a Developer Certificate of Origin sign-off:

```text
Signed-off-by: Name <email>
```

Use `git commit -s`.

## ABAR change requirements

Every ABAR change must include:

- Tests.
- `abar-api` and schema impact.
- Default-form runtime evidence.
- Portability answer: would this capability remain useful for unrelated agents
  without domain-specific assumptions?
- Dependency-removal answer: can ABAR still run in its default local form
  without the new dependency?

Do not add vertical business Skills to `abar-core`.

Changes that alter public contracts must update both documentation languages
before changing contracts, tests, or code. Defect and security fixes may proceed
directly when they preserve those contracts.

## Module boundaries

Repository-wide module rules are defined in `AGENTS.md`. In particular:

- Entrypoints remain composition roots.
- One module owns one cohesive responsibility.
- Rust source files have a 500-line automated hard limit.
- Legacy oversized files are baseline exceptions, must not grow, and must be
  split as they are materially changed.

No module-size exceptions are allowed.

## Contributor documentation

Contributor-visible documentation describes the implementation, contracts,
operations, and tests. Every documentation change must update both its English
and Chinese version.
