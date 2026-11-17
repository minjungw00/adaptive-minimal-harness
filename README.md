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

The native `amh` CLI now supports **read-only state inspection, item retrieval, and change preview**. Project installation, policy compilation, and consumer-facing authenticated approval remain future work. An **optional experimental Linux-only `amh-host`** accepts externally signed receipts and writes only to independently protected state paths, with a separate local audit witness for state rollback detection. It does not sign approvals or provide a user-authentication UI.

```sh
amh state inspect --file ./state.json
amh item get --file ./state.json --id R-COST
amh change preview --file ./state.json --proposal ./proposal.json
amh manifest inspect --file ./state-manifest.json
```

See the [CLI reference](docs/cli.md), [external signer contract](docs/external-signer.md), [explicit legacy migration](docs/legacy-migration.md), [metadata-only state sharing](docs/state-manifest.md), [protected audit contract](docs/audit.md), [independent witness](docs/external-witness.md), [manual recovery](docs/recovery.md), and [trusted host limitations](docs/trusted-host.md).

The Rust core now supports **typed item details with structural validation**, explicit validated transitions, replayable history, and a local JSON snapshot store. An optional `signed-approvals` feature checks Ed25519 signatures for exactly scoped changes before writing through the public store mutation API. **End-to-end authorization is not yet integrated**: the host must supply and protect the trusted public key and control filesystem permissions. These are library capabilities only; the consumer installer, policy resolver, task compiler, and authenticated approval UI are not implemented.

See [architecture](docs/architecture.md), [persistence and limits](docs/persistence.md), [approval trust boundary](docs/approvals.md), [contracts](docs/contracts.md), and [roadmap](docs/roadmap.md).

## Development

```sh
cargo test --workspace --locked
cargo test --workspace --all-features --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo run -p amh-cli --locked -- --help
cargo run -p amh-cli --features approval-request --locked -- --help
cargo run -p amh-host --locked -- --help
cargo run -p amh-witness --locked -- --help
```

Commits are separated by **one actual responsibility** and use the repository's **Conventional Commit subject format** `type: description`. Responsibility-level atomic commits are a general working principle; the message format is a repository-level convention.

## License

**Not yet selected.** Do not assume a reuse license until one is added.
