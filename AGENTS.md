# Repository Development Rules

English | [简体中文](AGENTS.zh-CN.md)

These rules apply to every Rust module under `abar-api/`, `abar-core/`, `abar-cli/`,
`adapters/`, and `apps/`.

## Documentation source of truth

- Read [Architecture](docs/en/concepts/ARCHITECTURE.md),
  [Contracts](docs/en/reference/CONTRACTS.md), and
  [Implementation](docs/en/guides/IMPLEMENTATION.md) before changing runtime behavior.
- Every contributor-visible document has a paired English and Chinese version.
  Update both versions in the same change.
- English technical docs live in `docs/en/`; Chinese docs live in
  `docs/zh-CN/`. Root README, AGENTS, contribution, governance, and security
  files use `name.md` and `name.zh-CN.md`.
- Markdown is the human-facing source of truth. Rust types, JSON Schema, and
  OpenAPI files remain the machine-readable contract.

## Change flow

- Update paired ABAR documentation before changing a public contract.
- Update contracts, tests, and implementation after the documentation matches.
- Do not infer contracts from examples or incidental implementation details.
- Pure defect, security, and internal-refactor work may proceed directly. If it
  changes behavior or a public contract, update both documentation languages in
  the same change.

## Module ownership

- An entrypoint (`main.rs` or `lib.rs`) is a composition root. It may declare
  modules, route commands, and re-export a deliberate public API. It must not
  accumulate command handlers, provider construction, or storage logic.
- One module owns one cohesive responsibility. CLI command families,
  transport clients, process supervision, provider adapters, persistence, and
  domain rules belong in separate modules.
- A shared helper moves to a dedicated module only when it has a stable,
  nameable responsibility. Do not create generic `utils` dumping grounds.
- Dependencies flow inward: `abar-cli -> abar-api`, `abar-core -> abar-api`, and
  `adapters -> abar-api`. Modules must not reach through another module's private
  implementation.
- Internal workspace crates use the `abar-` prefix. The contracts crate is
  `abar-api`, the implementation crate is `abar-core`, and the CLI crate is
  `abar-cli`. Adapters also use the prefix. Domain terms such as `agent.yaml`,
  `agent_name`, and Agent Revision remain valid.
- The public project name is `Agent Build And Runtime`; its abbreviation is
  `ABAR`. Use `abar` for the CLI binary, `ABAR_` for project environment
  variables, and `~/.abar` for project state.

## File size discipline

- New or substantially modified Rust source files should stay below 400 lines.
- 500 lines is the automated hard limit. A source file that reaches 500 lines
  must be split by responsibility before adding unrelated behavior.
- The limit does not permit coarse mechanical splits. Tests, DTOs, command
  parsing, I/O, and domain rules should be separated according to ownership,
  not shuffled into arbitrary files.
- No Rust module may exceed the limit; split responsibilities instead of
  adding a baseline exception.
- Generated files, lock files, fixtures, and non-Rust assets do not count
  toward the Rust module limit.

Run the automated check before submitting changes:

```text
bash scripts/check.sh
```

For behavior changes, also run:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Change discipline

- Preserve behavior when performing structural refactors unless the change
  explicitly includes a behavior or contract update.
- Add or update tests at the boundary affected by the change.
- Update architecture documentation when module ownership or dependency
  direction changes.
- Do not hide cross-domain coupling behind one oversized file.
- Contributor-visible documentation describes implementation, contracts,
  operations, and tests.
