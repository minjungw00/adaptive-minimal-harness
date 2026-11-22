# Initial data contracts

**Status:** This document distinguishes the original legacy snapshot contract from the implemented native project contract. Examples under `examples/p0/` are supported; the three original examples remain illustrative legacy design fixtures.

## Native project contract

The user-owned `~/.amh/config.toml` contains `format = "amh.config.v1"` and
`repositories`, an array of `root` and `project` bindings. Roots are canonical
Git work-tree paths. Project IDs identify policy stores, so several work trees
can explicitly share one policy store.

```toml
format = "amh.config.v1"

[[repositories]]
root = "/home/alex/work/reports"
project = "reports"
```


`user/policies.toml` and `projects/<id>/policies.toml` contain
`format = "amh.policy-registry.v1"`, `project_id` and ordered `events`. The
personal common registry has identity `user` and holds user-scoped items. The
project registry holds project-scoped and explicitly Task-scoped items. Events
carry sequential revisions, reasons and typed changes. State is derived by
replay. Versioned packs are retained in the event log. Duplicate policy IDs
across scopes are rejected when selecting policies.

Task has `id`, `goal`, `done_when`, optional `tags`, `classification_complete`,
`allowed_paths`, `stop_when`, `outputs`, `explicit_policies`, task-scoped `items`,
and `checks`. A check has unique `id`, `argv`, `timeout_seconds` (1–300) and optional
`expected_exit_code` (default 0). Classification is explicit; unmatched mandatory
tag policies block compilation until classification is complete. Context budget
units are characters, not estimated model tokens. No applicable condition is
silently truncated. Allowed paths require external enforcement.

The compile response traces selected IDs, per-entry applicability diagnostics,
source digest and rendered content on stdout.
Semantic conflict analysis, authenticated human approval, arbitrary adapter
formats and live-session behavioral attestation remain outside P0.

## Read-only policy lookup

| Format | Fields and binding |
| --- | --- |
| `amh.policy-discovery.v1` | `status: ready`, `request`, `request_digest`, `catalog_digest`, candidate summaries, instructions and `response_schema`. A summary has `id`, `item_revision`, `scope`, `condition`, `activation` and `strength`. |
| `amh.policy-analysis.v1` | Original `request`, both digests, `assessments` and `clarification_questions`. Each assessment supplies ID and version, applicability, reason and one or more exact request quotes. |
| Resolution | `status`, `selected`, `instructions` and `clarification_questions`. A clarification result supplies no selected instructions. |

The catalogue digest binds the work-tree registration and current user/project
policy entries. The request digest binds the catalogue and the exact request.
Reading a single body and resolving the analysis both check the current catalogue.
The resolver independently rebuilds the possible candidates and verifies complete
coverage, lifecycle, mandatory uncertainty and declared policy relations.
Unconditional policies are selected by the resolver. Task-scoped policies are
handled by the explicit Task path. Lookup and resolution persist no conversation
state and render selected instructions directly in the result.

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
