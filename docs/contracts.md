# Initial data contracts

**Status:** Draft — examples are design fixtures, **not yet parsed or enforced by the CLI**. This document does not use a planning version identifier.

## Common item envelope

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

## Explicit changes

Planned operations: `ADD`, `MODIFY`, `REPLACE`, `REMOVE`, `REPRIORITIZE`, `PAUSE`, `REACTIVATE`, `RETIRE`. Updates carry an expected snapshot revision; changing an important policy or a requirement role requires trusted approval. Later appearance in a conversation is not implicit priority.

**Implemented in the Rust library:** `ADD`, `MODIFY` (content), `REPLACE` (explicitly links compatible items), `ACTIVATE`, `PAUSE`, `REACTIVATE`, and `RETIRE`. Both snapshot and item revision guards are checked. Successful changes produce replayable events.

**Not yet implemented:** `REMOVE`, `REPRIORITIZE`, external authorization, source/provenance fields, type-specific payload validation, natural-language policy conflict analysis, the example input schema, and CLI mutation commands. `ItemSnapshot` currently uses a minimal Rust data model, not the complete planned JSON contract. See [persistence details](persistence.md).

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

## Example files

- [TDD principle](../examples/policy-tdd.toml)
- [Writing task](../examples/task-writing.json)
- [Requirement change](../examples/change-add-requirement.json)

The examples are draft contracts and must not be mistaken for currently supported CLI inputs.
