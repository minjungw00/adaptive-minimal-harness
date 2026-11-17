# Adaptive Minimal Harness (AMH)

A **local-first, model-agnostic control layer** for user-defined goals, principles,
requirements, constraints, and decisions. AMH works with existing agents; it does
not provide an LLM, agent runner, or mandatory server.

**Status: pre-alpha.** The native project workflow now connects initialization,
policy management, deterministic selection, task contracts, AGENTS.md deployment,
and explicit verification. Semantic policy interpretation and measured agent
compliance remain future work. See the [P0 acceptance boundary](docs/p0.md).

## Native project workflow

Run these from an existing project. Writes default to a JSON change plan; use
`--apply` for an explicitly selected local change or save the plan and review it
before `amh apply --plan plan.json`.

```sh
amh init --id my-project > init-plan.json
amh apply --plan init-plan.json
amh pack install --name development --expected-revision 0 --reason 'Use development policies' --apply
amh compile --task /path/to/task.toml
amh sync --task /path/to/task.toml --apply
amh doctor
amh verify --task /path/to/task.toml       # does not execute commands
amh verify --task /path/to/task.toml --run # executes the selected verification tools
```

Use [supported examples](examples/p0/), the [CLI reference](docs/cli.md), and the
[complete walkthrough](docs/p0.md). For chat and notes, use the optional
[zero-install contract](docs/zero-install.md).

- **User ownership:** one project-owned TOML event log derives current policy
  state. Optional user policies are imported from an explicitly selected registry.
- **Adaptive selection:** lifecycle, explicit task classification, scope references,
  activation conditions and declared conflicts determine selection. Uncertain
  mandatory conditions and unresolved dependencies/conflicts block compilation.
- **Minimal context:** only selected policies and the current task contract are
  rendered. A character budget cannot silently discard an applicable condition.
- **Safe updates:** reviewed plans are recomputed before application; generated
  edits and symlinks are refused. Only an identified block in an existing
  `AGENTS.md` is managed. Interrupted updates have an explicit recovery path.
- **Honest evidence:** delivered instructions, executed checks and actual policy
  compliance are distinct. Successful checks do not certify semantic correctness,
  TDD process adherence, or live-agent compliance.

TDD, refactoring, Socratic dialogue, research and writing are editable policy pack
entries, not privileged core modes. Packs are opt-in; no model, API key, Docker,
DB server or background process is required.

## Trust levels

The project workflow edits **ordinary user-owned files under OS permissions**.
It does not authenticate a human approver or sandbox an agent with the same write
permissions. `--apply` is an explicit local write, not a cryptographic approval.

The separate experimental Linux `amh-host` verifies externally signed receipts
and writes only to independently protected state. Its state format, trust anchor,
audit chain and authorization boundary are unchanged by the local project flow.
It does not automatically ingest or trust local registries. See [trusted host](docs/trusted-host.md),
[approvals](docs/approvals.md), and [audit](docs/audit.md).

The existing `amh state inspect`, `amh item get`, `amh change preview` and
`amh manifest inspect` commands remain read-only for the legacy snapshot format.

## Installation and development

Native `amh` executables are built and tested for Linux, macOS and Windows by CI.
Download the artifact for your OS/architecture from the corresponding successful
workflow run and place the executable on PATH; end users do not need Rust.
Signed release packaging and a public release channel are not yet available.
For a source build, developers need stable Rust with rustfmt and clippy:

```sh
cargo build -p amh-cli --release --locked
cargo test --workspace --locked
cargo test --workspace --all-features --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p amh-core --no-default-features --locked
```

The executable is `target/release/amh` (`amh.exe` on Windows). Only `amh` is needed
for the ordinary project workflow; protected-host and witness binaries are optional.

See [architecture](docs/architecture.md), [data contracts](docs/contracts.md), and
[roadmap](docs/roadmap.md). Commits follow `type: description` and are separated by
actual responsibility.

## License

**Not yet selected.** Do not assume a reuse license until one is added.
