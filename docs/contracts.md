# Initial data contracts

**Status:** This document distinguishes the original legacy snapshot contract from the implemented native project contract. Examples under `examples/p0/` are supported; the three original examples remain illustrative legacy design fixtures.

## Native project contract

The user-owned `.amh/amh.toml` contains `format: "amh.project-registry.v1"`,
`project_id`, an optional explicitly selected `user_root`, and sequential `events`.
An event carries its registry revision, reason and typed change. Replay derives
current state; no duplicate authoritative current-state file is stored.

`Policy` wraps a core `ManagedItem` with `owner`, optional `source`, explicit
`priority`, optional natural-language `applicability`, and `relations` (`depends_on`, `supports`, `conflicts_with`, `applies_to`,
`verified_by`). Core typed details are required for new project entries. Owner
and provenance are declarations, not authentication. Event reason records the
change rationale. User/project/task scopes compose explicitly; duplicate IDs
are diagnosed rather than silently overridden. Task-scoped inline items belong
to their enclosing contract; persisted task-scoped policies require `applies_to`.

Native changes support ADD, MODIFY, SET_DETAILS, SET_METADATA, REPLACE, REMOVE,
REPRIORITIZE, SET_APPLICABILITY, ACTIVATE, PAUSE, REACTIVATE, RETIRE, INSTALL_PACK and RESTORE. Existing
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

## Conversation sessions

`amh.session.v1` is a project/root-bound state with an ID, revision, closed flag,
policy catalogue fingerprint, conditional judgments and their request bindings,
currently selected IDs, provenance-tagged notes, optional attached Tasks/focus, and
a delivery fingerprint. Closing a conversation is not completion of its Tasks.
One state file is authoritative; emitted instructions are derived, not another store.

`amh.session-packet.v1` binds the current message, session snapshot, policy snapshot,
focus and refresh/known-context options. Optional omitted catalogue/context bodies
are references to material already held by the current agent, not omitted rules.
`amh.session-analysis.v1` supplies review/reuse, reasoning and exact source quotations,
conditional assessment changes, optional context changes and clarification questions.
No goal, Task ID, done criteria or executable check fields are required.

The source catalogue distinguishes current user messages, saved user quotes, agent
inferences, unresolved agent questions, stored Tasks and registered policy content. Quotation binding establishes
traceability, not semantic truth, user approval or authentic provenance. Inferred
summaries cannot be declared verbatim user quotations. Notes are bounded to 64 and
32 KiB including their source messages; state and packets are bounded to 1 MiB.
Explicit retirement cites the current user message and never removes project policy.
Clarification output is retained as agent_question context with its original request,
without making it a user requirement or proof of actual message delivery. It remains
unresolved until explicitly retired; unrelated successful work does not complete it.

Full conditional coverage is required initially and after catalogue, focus or context
changes, refresh, or without both matching prior state/catalogue digests. Other turns can inherit unchanged judgments, including previous
no_match decisions, while the agent reconsiders their relevance. AMH rechecks the
complete selected set's lifecycle, scope, mandatory uncertainty, dependencies and
conflicts. Reuse does not certify semantic correctness or process adherence.

Mutation plans bind state, source inputs and project; application recomputes the
plan under the project lock. Session IDs cannot escape managed paths and symlink
components are rejected. Unchanged state is not rewritten. Explicit Tasks and their
scope IDs keep their original contracts; a matching session ID does not activate a
Task-scoped policy. Session analyses cannot add executable checks.

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

## Agent-interpreted requests

`amh.request-packet.v1` binds the verbatim request, project/root, task ID and current
policy catalogue. `AgentResponse` (`amh.request-analysis.v1`) returns bound digests,
`done_when`, per-candidate assessments (`id`, `item_revision`, `applicability`,
`reason`, `request_quotes`) and `clarification_questions`. Every active conditional
principle requires exactly one assessment. Unconditional accepted conditions remain
selected independently. Natural requests produce no manual tags or executable checks.

Task's optional `interpretation` retains the request and response. Compilation and
plan application revalidate source identity, coverage and grounding. Registry changes
require new interpretation. The model's natural-language judgments are inspectable
but are not independent verification or authenticated approval. See [agent requests](agent-requests.md).
