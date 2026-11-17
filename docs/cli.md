# CLI reference

## Native user-owned projects

`--root PATH` selects an existing project directory; the default is the current
directory. Input documents accept TOML, or JSON when the filename ends in `.json`.
`--help` lists all supported options. Unknown/duplicate options fail.

| Command | Behavior |
| --- | --- |
| `init --id ID [--user-root PATH]` | Plan initial registry, adapter, and shared instruction block |
| `policy list` | Current policies, registry revision and pinned pack content digests |
| `policy add --file FILE --expected-revision N --reason TEXT` | Add one explicitly typed draft policy |
| `policy change --file FILE --expected-revision N --reason TEXT` | Apply an explicit operation described below |
| `pack list` / `pack show --name NAME` | Inspect optional built-in packs |
| `pack install --name NAME --expected-revision N --reason TEXT` | Install and activate a version-pinned built-in pack; optional `--scope user|project` |
| `pack install --file FILE --expected-revision N --reason TEXT` | Install a custom pack using the same schema; optional `--scope user|project` |
| `request --text TEXT` or `request --request-file FILE` | Read-only discovery packet for the current agent; no manual tags required |
| `request finish --packet FILE --analysis FILE [--budget CHARS]` | Validate agent judgments and plan the interpreted task's deployment |
| `request --text TEXT --agent codex [--budget CHARS]` | Use an explicitly selected existing Codex CLI for interpretation |
| `request --request-file FILE --agent-config FILE [--budget CHARS]` | Use an explicit JSON stdin/stdout agent adapter |
| `compile --task FILE [--budget CHARS]` | Read-only resolution diagnostics and rendered task instructions |
| `sync --task FILE [--budget CHARS]` | Plan AGENTS.md adapter deployment |
| `apply --plan FILE` | Recompute and apply exactly the reviewed local plan |
| `doctor` | Inspect source/adapter drift and inventory other instruction files |
| `history` | Inspect explicit registry events and reasons |
| `rollback --revision N --expected-revision N --reason TEXT` | Restore an earlier policy set as a new revision |
| `uninstall` | Remove owned adapter content; retain registry, history and evidence |
| `recover` | Diagnose an interrupted local file transaction |
| `verify --task FILE` | Show selected verification commands without executing them |
| `verify --task FILE --run` | Run the task's explicit argv commands and save actual outcomes |

Mutating commands emit plans by default. `--apply` applies the current command's
plan immediately; it is not human authentication. `recover --apply` rolls back
an interrupted file transaction when all affected files still match either
reviewed version. Recovery refuses unrelated user edits. Registry rollback is
separate: it preserves event history and advances item revisions.

`policy change` supports `ADD`, `MODIFY`, `SET_DETAILS`, `SET_METADATA`, `REPLACE`,
`REMOVE`, `REPRIORITIZE`, `SET_APPLICABILITY`, `ACTIVATE`, `PAUSE`, `REACTIVATE`, `RETIRE`, `INSTALL_PACK`,
and `RESTORE`. See [contracts](contracts.md). Existing-item operations require
`item_revision`; all registry mutations also require the current registry revision.

`doctor` returns nonzero for stale or blocked deployment. Corrupt/missing owned
files, malformed data, uncertain mandatory rules and conflicts also return nonzero.
An uncompiled project is reported as `uncompiled`, never as observed compliance.

Verification only starts commands with `--run`, never invokes a shell implicitly,
and records stdout/stderr, expected/actual exit codes, timestamps and contract
fingerprints. The default expected exit code is zero; an explicit expected code
can represent a failing RED check. Each timeout is 1–300 seconds and kills the
direct child; process-tree isolation is not provided. There are no retries.
A failed/missing/timed-out check returns nonzero while retaining its evidence.
Checks do not infer completion criteria or policy compliance. Empty check sets
remain unverified. These are local execution records, not authenticated attestations.

Natural request interpretation and deployment are described in [agent requests](agent-requests.md).
Request commands accept `--root`; the initial request accepts an optional `--task-id`.
`--apply` applies only a finished interpretation or explicitly selected backend result;
a discovery packet alone cannot be applied. The current agent produces intermediate
JSON files as protocol data. Users do not have to select tags or policies.

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

## Conversation sessions

`session open [--id ID] [--apply]` creates a handle; normally the existing agent
manages it. `session prepare --id ID --text TEXT` reads current policies/context.
`--known-catalog DIGEST --known-state DIGEST` can omit already available bodies;
`--refresh` restores full context and requires renewed assessment. Do not connect
a conversation to a project's latest handle automatically.

Send `{packet, response}` over stdin to `session finish --input - [--apply]`.
The response uses `amh.session-analysis.v1`; refer to the packet's response_schema.
That schema is only the inner response. `analysis` is an accepted alias for the
outer `response` field; supplying both is rejected. `session finish --help`
explains the envelope.
No Task ID or completion criteria are required. A clarification_required JSON
response uses the error stream/nonzero exit to block action, not to label a
clarification semantically incorrect. With --apply, its agent_question context is
retained without activating new work instructions. Unchanged reuse does not write state.

`session inspect --id ID` exports state/freshness. `session attach --id ID --task
FILE --expected-revision N [--apply]` registers optional work information without
selecting it. Prepare `--task-id TASK_ID` selects a task scope when contextual
grounds exist; `--clear-focus` clears it. `session verify --id ID [--run]` uses
only checks in that explicitly attached/focused Task. `session close --id ID
--expected-revision N [--apply]` closes the session, not its work items.

A mutation without --apply returns a recomputable `amh.session-plan.v1`.
`session apply --input -|FILE` applies that exact reviewed plan. All commands
accept --root PATH. Per-session state lives at `.amh/sessions/ID/state.json`;
instructions use stdout, not shared conversation-specific files. The existing
`doctor`, `compile`, `sync` and `verify --task` describe the optional Task path.
