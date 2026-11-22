# CLI reference

## Local policy workflow

Git must be on PATH. Commands that accept `--root PATH` use that directory or
the current directory to identify the registered Git work tree. `AMH_HOME`
defaults to `~/.amh` and must be outside Git work trees.

| Command | Behavior |
| --- | --- |
| `project add --id ID [--root PATH]` | Register the work tree under a policy project ID. Multiple explicit registrations can use the same ID. |
| `project list` | List registered roots and project IDs. |
| `project remove [--root PATH]` | Remove a root's registration and retain its project's policies and history. A moved or deleted tree can be removed using its stored path. |
| `agent instructions` | Print guidance to add to the current agent's user-level instructions. |
| `policy list [--scope user]` | Show the selected registry, revision and installed pack fingerprints. Project scope is the default. |
| `policy add --file FILE --expected-revision N --reason TEXT` | Add a typed draft policy to the selected registry. |
| `policy change --file FILE --expected-revision N --reason TEXT` | Apply an explicit policy operation. |
| `policy import --file FILE` | Copy policy history from a previous registry into an empty registry. |
| `pack list`, `pack show --name NAME`, `pack install` | Inspect or install built-in packs, or install a custom pack with `--file FILE`. Installation takes an expected revision and reason. |
| `history`, `rollback --revision N --expected-revision N --reason TEXT` | Inspect registry events or restore a policy set as a new revision. |
| `apply --plan FILE` | Apply a reviewed plan after recomputing it in the same AMH home. |
| `request --text TEXT`, `request finish --packet FILE --analysis FILE` | Prepare an interpretation packet and produce a validated Task on stdout. The optional `--agent codex` or `--agent-config FILE` runs a selected interpreter. |
| `compile --task FILE [--budget CHARS]` | Print rendered Task instructions and diagnostics. |
| `verify --task FILE [--run]` | Preview checks, or run them from the repository root and store evidence under the AMH home. |

Policy and history operations accept `--scope user` to select personal common
policies. Use `--help` for all supported arguments. Mutations return JSON plans
by default. Add `--apply` to apply a command immediately. Existing-item changes
require their item revision and registry changes check the expected registry
revision. See the [policy contracts](contracts.md).

## Legacy/protected snapshot inspection

These existing commands remain read-only and retain their formats:

```sh
amh state inspect --file state.json
amh item get --file state.json --id R-COST
amh change preview --file state.json --proposal proposal.json
amh manifest inspect --file state-manifest.json
```

`state inspect` verifies snapshot/history consistency, not authenticity or agent
behavior. Missing legacy snapshots report `file_exists: false`. `change preview`
always reports `authorization: not_granted`. Metadata manifests are always
`structurally_valid_untrusted`.

With `--features approval-request`, `change request --file PATH --proposal FILE
--project ID --ttl SECONDS` prepares an unsigned request for an independent signer.
The local project `apply` command does not write this store. Only the optional
[protected host](trusted-host.md) consumes signed receipts.

## Read-only policy lookup

| Command | Input and output |
| --- | --- |
| `policy discover --text TEXT` | Returns `amh.policy-discovery.v1` with a body-free candidate list, request and catalogue digests, and the JSON input schema for resolve. |
| `policy read --id ID --catalog-digest DIGEST` | Returns one active policy from the same user/project catalogue. |
| `policy resolve --input -\|FILE` | Accepts a single `amh.policy-analysis.v1` object. Use `-` for stdin. Returns selected IDs and instruction text on stdout, or structured clarification questions on stderr with a failing exit status. |

All three accept `--root PATH`. A catalogue change requires a fresh discovery.
There is no Task or conversation state in this path. Use `request`, `compile`,
`sync` or `verify` when the caller explicitly needs a Task contract.
