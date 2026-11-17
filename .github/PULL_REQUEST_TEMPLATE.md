## Change

English | [简体中文](PULL_REQUEST_TEMPLATE.zh-CN.md)

Describe the problem and the change.

## Compatibility

- [ ] `abar-api` impact documented.
- [ ] Agent Revision/Session/Message/Runtime Event impact documented.
- [ ] CLI behavior preserved or explicitly versioned.

## Verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`

## ABAR boundaries

- [ ] No vertical business logic entered `abar-core`.
- [ ] No optional integration became a required runtime dependency.
- [ ] Default local and test forms still run without external services.

## Module discipline

- [ ] Entrypoints remain composition roots without domain handlers.
- [ ] New or materially changed Rust files stay below the 500-line limit.
- [ ] Legacy oversized files did not grow, and touched responsibilities were extracted.
- [ ] `bash scripts/check.sh` passes.

## Documentation

- [ ] English and Chinese documentation versions were updated together.
