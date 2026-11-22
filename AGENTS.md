# Agent working agreement

AMH is a local-first, model-agnostic layer for reusable user-defined principles and methods.

## Workflow
- Use the current task's goal, scope, applicable principles, and acceptance criteria. A later request does not silently replace unrelated valid conditions.
- Load only the project guidance relevant to the task. Do not apply TDD, refactoring, or Socratic dialogue unconditionally; they are configurable examples.
- Favor the smallest verifiable implementation. Reuse existing agent platforms and tooling before adding a new runner.
- Report what was actually checked. Do not claim an unexecuted command passed, or that a passive instruction file enforces permissions.
- Respect user-owned files. Stop on unresolved policy conflicts or unauthorized destructive changes.

## Commits
- **General working principle:** separate changes into atomic commits by real responsibility.
- **This repository's convention:** Conventional Commit subjects in the form `type: description`, e.g. `build: bootstrap Rust workspace` or `test: add CLI smoke tests`.
- Keep implementation, unrelated cleanup, and infrastructure changes separate when they serve different purposes.

## References
- [Architecture](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Development](docs/development.md)

## Verification (once Rust tooling is available)
```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```
