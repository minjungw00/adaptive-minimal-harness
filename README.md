# Adaptive Minimal Harness (AMH)

A **local-first, model-agnostic control layer** for user-defined goals, principles,
requirements, constraints, and decisions. AMH works with existing agents; it does
not provide an LLM, agent runner, or mandatory server.

**Status: pre-alpha.** AMH keeps its repository registrations, policies and
verification records in the user's home. The existing agent finds applicable
policies for the current request and reads only the bodies it needs.

## Getting started

With `amh` and Git on PATH, run these commands from a Git work tree:

```sh
amh project add --id my-project --apply
amh pack install --name development --expected-revision 0 --reason 'Use development policies' --apply
amh agent instructions
```

Pack installation is optional. Add the printed guidance to your agent's
user-level instruction file, such as `~/.codex/AGENTS.md`. Then use the agent to
request work normally. Work trees without a local registration receive
`not_registered` from policy discovery and follow their ordinary workflow.

AMH writes under `~/.amh` by default. `AMH_HOME` can select a different absolute
directory outside Git work trees. Project registration, policy changes and
verification records leave shared repository instructions untouched. Personal
common policies can be managed from any directory with `--scope user`.

- **User ownership.** TOML registries preserve policy IDs, revisions, lifecycle
  and change history. Each registered work tree uses the common registry and
  the policies for its selected project ID.
- **Adaptive selection.** Discovery yields conditions. An agent can fetch
  individual bodies and obtain the selected instructions after AMH validates
  applicability coverage, policy versions, dependencies and conflicts.
- **Explicit Tasks.** `compile --task FILE` renders a contract to stdout.
  `verify --task FILE` previews checks and `--run` executes them and records
  evidence locally. A check result alone does not establish policy compliance.

See [agent integration](docs/agent-requests.md), the [CLI reference](docs/cli.md)
and the [walkthrough and migration](docs/p0.md). Use [zero-install guidance](docs/zero-install.md)
for chat and notes without a CLI.

## Trust levels

The project workflow edits **ordinary files in the user’s AMH home under OS permissions**.
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
workflow run, unpack the inner `.tar.gz`, and place `amh` (`amh.exe` on Windows)
on PATH. The package includes examples and documentation; end users do not need Rust.
Version tags (`v*`) prepare a draft GitHub Release with native packages and SHA-256
checksums. A maintainer must review release gates and publish it before public,
account-free release downloads are available. Artifact downloads may require GitHub
sign-in. Package signatures and a published release are not yet available.
For a source build, developers need stable Rust with rustfmt and clippy:

```sh
cargo build -p amh-cli --release --locked
cargo test --workspace --locked
cargo test --workspace --all-features --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p amh-core --no-default-features --locked
```

The executable is `target/release/amh` (`amh.exe` on Windows). The ordinary workflow uses `amh` and Git. Protected-host and witness binaries are optional.

See [architecture](docs/architecture.md), [data contracts](docs/contracts.md), and
[roadmap](docs/roadmap.md). Commits follow `type: description` and are separated by
actual responsibility.

## License

**Not yet selected.** Do not assume a reuse license until one is added.
