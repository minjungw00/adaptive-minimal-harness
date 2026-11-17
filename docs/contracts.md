# Initial data contracts

**Status:** This document distinguishes the original legacy snapshot contract from the implemented native project contract. Examples under `examples/p0/` are supported; the three original examples remain illustrative legacy design fixtures.

## Native project contract

The user-owned `.amh/amh.toml` contains `format: "amh.project-registry.v1"`,
`project_id`, an optional explicitly selected `user_root`, and sequential `events`.
An event carries its registry revision, reason and typed change. Replay derives
current state; no duplicate authoritative current-state file is stored.

`Policy` wraps a core `ManagedItem` with `owner`, optional `source`, explicit
`priority`, and `relations` (`depends_on`, `supports`, `conflicts_with`, `applies_to`,
`verified_by`). Core typed details are required for new project entries. Owner
and provenance are declarations, not authentication. Event reason records the
change rationale. User/project/task scopes compose explicitly; duplicate IDs
are diagnosed rather than silently overridden. Task-scoped inline items belong
to their enclosing contract; persisted task-scoped policies require `applies_to`.

Native changes support ADD, MODIFY, SET_DETAILS, SET_METADATA, REPLACE, REMOVE,
REPRIORITIZE, ACTIVATE, PAUSE, REACTIVATE, RETIRE, INSTALL_PACK and RESTORE. Existing
items require their item revision, and every mutation checks registry revision.
ADD requires a unique draft at revision 1; MODIFY preserves kind, scope and role.
REPLACE requires an active original and compatible kind/scope, retaining the
replacement relation. Removed IDs cannot be reused. RESTORE creates a new event
and advances restored item revisions rather than rewinding history.

Packs have an ID, version and array of policies. Installation explicitly
activates their entries; individual activation conditions still determine task
applicability. Installed definitions are retained in events and exposed as version
and content-digest locks. Existing user-customized identities are not overwritten.

Task has `id`, `goal`, `done_when`, optional `tags`, `classification_complete`,
`allowed_paths`, `stop_when`, `outputs`, `explicit_policies`, task-scoped `items`,
and `checks`. A check has unique `id`, `argv`, `timeout_seconds` (1–300) and optional
`expected_exit_code` (default 0). Classification is explicit; unmatched mandatory
tag policies block compilation until classification is complete. Context budget
units are characters, not estimated model tokens. No applicable condition is
silently truncated. Allowed paths require external enforcement.

The compile response traces selected IDs, per-entry applicability diagnostics,
source digest and rendered content. Sync only targets the AGENTS.md adapter.
Semantic conflict analysis, authenticated human approval, arbitrary adapter
formats and live-session behavioral attestation remain outside P0.

## Legacy common item envelope

A goal, principle, requirement, constraint, or decision may share the following identity and governance metadata:

| Field | Meaning |
| --- | --- |
| `id` | Stable identifier |
| `kind` | Semantic kind; type-specific validation is required |
| `revision` | Positive item revision |
| `scope` | User, project, or task |
| `owner` | Claimed owner (NOT proof of authorization) |
| `lifecycle` | Draft, active, paused, superseded, retired |
| `content` | Human-authored intent |
| `relations` | Explicit dependencies, conflicts, replacement links |

Goals and requirements express desired outcomes. Principles describe applicable behavior. Constraints identify boundaries. Decisions record authorized choices. These kinds may share a storage protocol, **not identical semantic validation**.

## Legacy snapshot changes

Planned operations: `ADD`, `MODIFY`, `REPLACE`, `REMOVE`, `REPRIORITIZE`, `PAUSE`, `REACTIVATE`, `RETIRE`. Updates carry an expected snapshot revision; changing an important policy or a requirement role requires trusted approval. Later appearance in a conversation is not implicit priority.

**Implemented in the Rust library:** `ADD`, `MODIFY` (content), `REPLACE` (explicitly links compatible items), `ACTIVATE`, `PAUSE`, `REACTIVATE`, and `RETIRE`. Both snapshot and item revision guards are checked. Successful changes produce replayable events.

**Typed details:** New code can call `ManagedItem::new_typed` to supply kind-specific information. Structural checks cover goal success criteria, principle activation/strength/verification, requirement role and acceptance criteria, constraint enforcement/checks, and decision rationale. `details` remains optional when loading older snapshots. This does **not** validate the semantic quality of user-written criteria.

**Optional signed approval verification:** The `signed-approvals` feature provides `ApprovalRequest`, `SignedApproval`, `ApprovalVerifier`, and `SnapshotStore::apply_signed`. Signatures bind an exact change, project ID, expected snapshot revision, justification, and short validity window. This is only secure when an external trusted host pins the project's verification key and protects the store; there is no authenticated approval UI yet. See [approval trust contract](approvals.md).

**Implemented since then:** `SET_DETAILS` supports explicit type-specific migration; the CLI can inspect state and simulate changes without writing. A separate, optional Linux trusted-host binary verifies externally signed receipts using a fixed root-owned trust configuration and protected state root.

**Not yet implemented:** `REMOVE`, `REPRIORITIZE`, an authenticated human approval or key provisioning UI, signing-key management, source/provenance fields, natural-language policy conflict analysis, the example input schema, and an unprivileged CLI write path for this legacy/protected snapshot format. `ItemSnapshot` is still a minimal Rust data model, not the complete planned JSON contract. See [persistence details](persistence.md).

## Policy resolution

Activation is scoped to the task. An applicable mandatory condition must not be dropped simply to fit an instruction budget. A resolver should distinguish match, no-match, and uncertain. Explicit unresolved mandatory conflicts require a diagnostic, not silent selection.

## Task contract

When useful, a task describes:

- goal and currently applicable requirements;
- done criteria;
- verification;
- allowed paths and actions;
- stop conditions and output locations.

A simple task should not be forced into a heavyweight workflow.

## Deployment and evaluation

These are intentionally separate:

1. **desired** — current authorized rule;
2. **rendered** — generated instruction representation;
3. **deployed** — inspected target file or configuration;
4. **observed** — behavior actually evaluated.

Unknown observation must remain unknown. A generated `SKILL.md` or `AGENTS.md` does not establish actual agent compliance.

## Original illustrative example files

- [TDD principle](../examples/policy-tdd.toml)
- [Writing task](../examples/task-writing.json)
- [Requirement change](../examples/change-add-requirement.json)

The examples are draft contracts and must not be mistaken for currently supported CLI inputs.

Supported native inputs and an end-to-end sequence are in [examples/p0](../examples/p0/) and [P0 workflow](p0.md).
