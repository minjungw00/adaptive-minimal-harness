# CLI reference

Commands default to the current directory when no `--root PATH` is supplied.
To edit policies shared across all registered work trees, use `--scope user`
without `--root`. Policy mutations default to project scope.

## User setup and policy management

| Command | Behavior |
| --- | --- |
| `project add [--id ID] [--root PATH]` | Register the work tree. The directory name supplies a new project ID by default. Use `--id` to choose a name or explicitly reuse an existing project. |
| `project list` | List registered paths and project IDs. |
| `project remove [--root PATH]` | Remove the binding and retain its policies. If the work tree moved, supply its previously registered path. |
| `agent instructions` | Print guidance for the user to copy into the host agent's user-level settings. |
| `policy list [--scope user\|project] [--full]` | Show current policies and registry revisions. The default combines user and project summaries in registered work trees, and shows user summaries elsewhere. `--full` shows complete definitions and pack fingerprints. |
| `policy add --id ID --text TEXT [--when CONDITION] [--strength must\|should\|may]` | Add active reusable guidance. Strength defaults to `must`. Omit the condition for an always applicable principle. Use `--draft` for an inactive draft. |
| `policy add --file POLICY.toml` | Add a flat policy record from TOML or JSON. The target registry must match its scope. |
| `policy change ID --text TEXT` | Edit a policy's text or change its force with `--strength`. Use `--when CONDITION` or `--clear-condition` to edit its condition. Use `--activate`, `--pause`, or `--reactivate` in place of `--text` to change its status. Optional `--item-revision N` checks an earlier observation of the item. |
| `policy change --file CHANGE.json` | Apply a structured change, such as replacing a policy or editing its declared relations and provenance. |
| `pack list` / `pack show NAME` | List or inspect built-in packs before installation. |
| `pack install NAME` or `pack install --file PACK.toml` | Install an optional pack. A custom pack's policies must match the target scope. |
| `history [--full]` | Show the selected registry's events. `--full` exports its complete history for import. |
| `rollback --revision N` | Restore the selected registry's policy set at revision N as a new revision. |
| `policy import --file REGISTRY` | Copy a current-format AMH policy registry into an empty destination. Re-importing identical history is a no-op. |
| `apply --plan FILE\|-` | Apply a saved JSON plan from a file or stdin. AMH recomputes the change against the same home. |

Write commands show a summary of the proposed change by default. Use `--apply`
to make that change or `--plan` to export the complete plan. Save plan and policy
exports outside shared work trees when they contain personal policy. Policy
changes capture the current registry revision automatically. To require an
earlier revision, pass `--expected-revision N`. Use `--reason TEXT` to add
context to the history.

## Agent commands

| Command | Result |
| --- | --- |
| `policy discover [--root PATH]` | Returns `not_registered`, `ready` with candidate summaries and a catalogue identifier, or `resolved` with the unconditional policies when no candidate needs review. |
| `policy read ID --catalog-digest DIGEST [--root PATH]` | Reads one active policy for assessment. |
| `policy read --selection [--root PATH]` | Accepts catalogue-bound decisions as JSON on stdin and returns validated policy bodies. |

See the [agent integration guide](agent-requests.md) for the selection payload.
Run `amh <command> --help` for options or `amh --version` for the installed version.
