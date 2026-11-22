# Adaptive Minimal Harness (AMH)

AMH manages user-defined policies locally and gives existing AI agents the
policies relevant to their current work. Policies can apply across all
registered work trees or only to one project. Built-in packs are optional.

**Status: pre-alpha.**

## Get started

From a Git work tree, with `amh` and Git on PATH:

```sh
amh project add --apply
amh pack install development --apply
amh agent instructions
```

Pack installation is optional. Add the printed guidance to your agent's
user-level settings, such as `~/.codex/AGENTS.md`. Give the agent your normal
request. It uses the request and its available context to judge applicability.
AMH combines and delivers the selected policies, checking the catalogue and
declared policy relations.

AMH stores registration, policy definitions and change history under `~/.amh`.
Set `AMH_HOME` to an absolute directory outside Git work trees to use another
location. Registration leaves the repository and its shared instructions alone.
Each clone or work tree needs its own registration. Explicitly use
`amh project add --id ID --apply` to share a project policy set across paths.

Changes preview by default. `--apply` writes to the local AMH home under the
caller's filesystem permissions. See the [walkthrough](docs/p0.md),
[CLI reference](docs/cli.md), and [agent integration](docs/agent-requests.md).

## Installation and development

Native `amh` executables are built in CI for Linux, macOS, and Windows. Download
the artifact for your OS and architecture from the corresponding successful run,
unpack it, and put the executable on PATH. Version tags prepare a draft release.
Artifact downloads may require GitHub sign-in.

To build and check locally:

```sh
cargo build -p amh-cli --release --locked
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

See [architecture](docs/architecture.md), [data contracts](docs/contracts.md),
[development](docs/development.md), and [roadmap](docs/roadmap.md).

## License

A license has yet to be selected. Check the repository for licensing terms before reuse.
