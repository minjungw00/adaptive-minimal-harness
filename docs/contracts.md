# Data contracts

## Storage

| Path relative to AMH home | Contents |
| --- | --- |
| `config.toml` | `amh.config.v1` with `repositories` containing canonical Git work-tree `root` and policy `project` ID. |
| `user/policies.toml` | `amh.policy-registry.v1` for the user's common policies. |
| `projects/<id>/policies.toml` | `amh.policy-registry.v1` for policies of one registered project ID. |
| `.lock` | Serializes changes within the AMH home. |

A policy registry records a project ID and an ordered list of events. An event
holds its registry revision, reason and typed change. Current policies are
calculated from the event sequence. Rollback adds a restoration event and
issues new item revisions for restored records.

## Policy records

Each policy has an item, owner, optional source and natural-language
applicability condition, priority, and declared relations. An item has an ID,
kind, scope, revision, lifecycle, content and typed details. Principles carry
a strength and activation, with optional verification guidance. An installed pack records its versioned definitions
in the history. See the [principle example](../examples/p0/natural-policy.toml),
[requirement example](../examples/p0/requirement.toml), and [packs](../packs).

Changes include `ADD`, `MODIFY`, `SET_DETAILS`, `SET_METADATA`,
`SET_APPLICABILITY`, `REPLACE`, `REMOVE`, `REPRIORITIZE`, `ACTIVATE`, `PAUSE`,
`REACTIVATE`, and `RETIRE`. Pack installations and restores use
`INSTALL_PACK` and `RESTORE`. Existing item mutations check the item revision.
Changes also check the registry revision.

New active IDs must be unique across the common and project policies delivered
in a work tree. Inactive policies and previously stored task-specific policies
remain available in management views and do not affect the delivery catalogue.

## Delivery and plans

`policy discover` and `policy read --selection` exchange catalogue-bound policy
summaries, decisions and text using JSON on stdin/stdout. See the
[agent integration guide](agent-requests.md) for the fields. The catalogue
identifier covers the work-tree binding and eligible policy definitions.

A saved plan has format `amh.home-plan.v2`. It contains the home, exact requested
change, digest of applicable inputs, affected files and changed policy IDs with
their lifecycle effects. Apply compares a recomputed plan under the home lock.
The plan permits review and concurrency checks under local filesystem
permissions.
