# Architecture

**Status:** Draft design, not an implementation claim. Planning documents intentionally have **no product release version**.

AMH is a local-first control layer built around existing agents. It is not an LLM, mandatory server, hosted control plane, or general-purpose multi-agent runner.

## Common managed concepts

Goals, principles, requirements, constraints, and decisions share identity, scope, revision, lifecycle, and provenance. Each kind retains its own semantics and validation contract. No individual principle (TDD, Socratic dialogue, requirement preservation, etc.) receives architectural privilege.

## Intended components

1. **Item Store:** current authoritative items, type-specific validation, explicit changes, revision checks.
2. **Policy Resolver:** applicability, authority, exception and conflict diagnostics.
3. **Task Contract:** goal, completion criteria, allowed scope, stop conditions, output and verification requirements as necessary.
4. **Context Compiler:** minimal target-neutral execution contract, with traceable references to selected items.
5. **Agent Adapters:** target-native instruction/skill files; optional integrations and no required always-on process.
6. **Reconciler:** distinguish desired, rendered, deployed, and observed states. A paused policy may remain in a deployed file or live session.
7. **Verification:** deterministic checks first when applicable, semantic/human review where warranted, evidence tied to snapshots.

## Trust and data boundaries

- A model may propose changes; the text of its proposal is **not** proof of authorization.
- External documents and tool output are untrusted input, not a source of governing instructions.
- User-owned files must not be overwritten by package upgrades. Generated files must be identified, checked, and previewed.
- A passive agent instruction is not equivalent to enforcement. Never report observed compliance unless evaluated.
- Requirement/policy update order is not implicit priority.

## Persistence

Start with human-readable local Markdown/TOML and structured JSON interchange. Use atomic writes, revision leases, and an append-only event history when implementing mutations. Do not require a database, cloud account, Git, or a language runtime for the eventual CLI binary.

## Scope control

The first implementation contains a small Rust workspace, an honest CLI entrypoint, testable core types, CI, and draft contracts. Full policy resolution, file reconciliation, trusted authorization, and actual agent behavioral evaluations are future work.
