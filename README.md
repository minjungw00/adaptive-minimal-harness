# Adaptive Minimal Harness (AMH)

A **local-first, model-agnostic control layer** for user-defined goals, principles, requirements, constraints, and decisions.

AMH is designed to select only the conditions relevant to a task and make their application and verification inspectable. It does **not** replace an LLM, coding agent, workflow runner, or test framework.

> **Status: early repository bootstrap (pre-alpha).** The planning contracts are being translated into an implementation. The repository is **not yet an installable or policy-enforcing product**.

## Product boundaries

- **Unified management:** different item kinds share identity, scope, version, provenance, and lifecycle metadata without losing their distinct semantics.
- **Adaptive execution:** task-relevant policies are selected; unrelated skills or procedures are not loaded by default.
- **Verifiable operation:** distinguish declared policy, generated instructions, deployed configuration, and behavior observed in a particular evaluation.
- **Portable by design:** local files and standard agent instruction/skill formats first; external runners, MCP, and cloud services remain optional.
- **User ownership:** project-specific policies and state are not overwritten by package updates.

TDD, refactoring, Socratic questioning, writing practices, and requirement preservation are **examples of configurable principles**, not privileged hard-coded modes.

## Planned use

A native `amh` CLI will eventually initialize a consumer repository, validate managed items, compile task-specific instructions, compare deployment changes, and inspect state. **Those commands must not be assumed to exist until implemented and tested.**

The first deliverable is a Rust workspace, versioned data contracts, executable tests, and a CI baseline. See [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md) as they are introduced.

## Development

Once the Rust workspace is available:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p amh-cli -- --help
```

Commits are separated by **one actual responsibility** and use the repository's **Conventional Commit subject format** `type: description`. Responsibility-level atomic commits are a general working principle; the message format is a repository-level convention.

## License

**Not yet selected.** Do not assume a reuse license until one is added.
