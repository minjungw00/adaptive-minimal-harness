# Development

**Planning documents have statuses, not version numbers.** Only actual software releases should receive release versions.

## Prerequisites
- Rust stable with Cargo and rustfmt/clippy components for development only.
- End users are intended to receive native executables without installing Rust.

## Checks
```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run -p amh-cli -- --help
```

## Commit policy
Separate commits by responsibility. Use `type: description` subjects in this repository. Typical types include `docs`, `build`, `feat`, `fix`, `test`, `ci`, `chore`, and `refactor`. Never insert a planning version into a commit subject as though it were a product release.

## Contract-driven work
Write a test for a verifiable behavior before implementing it when TDD is applicable. For refactors, confirm observable behavior is preserved. Do not claim a test succeeded unless executed.

## Design documents
Treat design drafts as proposals. Confirm that implemented behavior matches the code, not simply the README.
