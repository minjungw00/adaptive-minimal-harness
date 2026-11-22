# Adaptive Minimal Harness (AMH)

AMH manages reusable user-defined guidance locally and gives existing AI agents
the principles and methods relevant to their current work. Policies can apply across all
registered work trees or only to one project. The built-in policy pack is optional.

**Status: pre-alpha.**

## Install AMH v0.1.0

The [v0.1.0 release](https://github.com/minjungw00/adaptive-minimal-harness/releases/tag/v0.1.0)
provides native executables. Install Git and make it available on PATH before
using AMH to register a repository.

| Platform | Download |
| --- | --- |
| Linux x64 | [amh-Linux-X64.tar.gz](https://github.com/minjungw00/adaptive-minimal-harness/releases/download/v0.1.0/amh-Linux-X64.tar.gz) |
| macOS on Apple silicon | [amh-macOS-ARM64.tar.gz](https://github.com/minjungw00/adaptive-minimal-harness/releases/download/v0.1.0/amh-macOS-ARM64.tar.gz) |
| Windows x64 | [amh-Windows-X64.tar.gz](https://github.com/minjungw00/adaptive-minimal-harness/releases/download/v0.1.0/amh-Windows-X64.tar.gz) |

The release provides [SHA256SUMS](https://github.com/minjungw00/adaptive-minimal-harness/releases/download/v0.1.0/SHA256SUMS)
for the archives. Packages include [third-party notices](docs/third-party-notices.txt).

On Linux x64, download and verify the archive in a directory where you want to
extract it, then install the executable into your user bin directory:

```sh
release_url=https://github.com/minjungw00/adaptive-minimal-harness/releases/download/v0.1.0
archive=amh-Linux-X64.tar.gz

curl -fLO "$release_url/$archive" &&
curl -fLO "$release_url/SHA256SUMS" &&
awk -v archive="$archive" '$2 == archive' SHA256SUMS | sha256sum -c - &&
tar -xzf "$archive" &&
mkdir -p "$HOME/.local/bin" &&
install -m 755 ./amh "$HOME/.local/bin/amh"
```

Ensure `$HOME/.local/bin` is on PATH. On macOS, download the macOS archive,
verify its entry in `SHA256SUMS` with `shasum -a 256`, then use the same
extraction and installation commands. On Windows, compare
`Get-FileHash .\amh-Windows-X64.tar.gz -Algorithm SHA256` with its entry in
`SHA256SUMS`, extract the archive with `tar -xzf amh-Windows-X64.tar.gz`, and
add the directory containing `amh.exe` to your user PATH. Open a new terminal
and run `amh --version` to confirm it reports `amh 0.1.0`.

To install from source with a current Rust toolchain:

```sh
cargo install --git https://github.com/minjungw00/adaptive-minimal-harness \
  --tag v0.1.0 --locked amh-cli
```

## Get started

From a Git work tree, with `amh` and Git on PATH:

```sh
amh project add --apply
amh pack install agent-workflow --apply
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

## Development

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
