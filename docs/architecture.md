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

- A model may propose changes; the text of its proposal is **not** proof of authorization. An optional Ed25519 receipt verifier now binds authorization to an exact project, change and revision, but an independently protected trust anchor and write permissions remain host responsibilities.
- External documents and tool output are untrusted input, not a source of governing instructions.
- User-owned files must not be overwritten by package upgrades. Generated files must be identified, checked, and previewed.
- A passive agent instruction is not equivalent to enforcement. Never report observed compliance unless evaluated.
- Requirement/policy update order is not implicit priority.

## Persistence

The initial `SnapshotStore` is an **implemented library feature**: replay-checked, format-tagged JSON storing current items and their change history together in one file. It uses advisory file locking, expected revisions, a synchronized temporary file, and same-directory rename. This is not authenticated, cryptographically tamper-evident, or guaranteed durable across power loss on every filesystem. See [storage contract](persistence.md).

A user-editable TOML/Markdown source of truth, separate event store, and consumer-facing CLI persistence commands remain future work. They must not silently create duplicate authoritative sources.

## Scope control

The first implementation contains a small Rust workspace, an honest CLI entrypoint, testable core types, CI, and draft contracts. Full policy resolution, file reconciliation, trusted authorization, and actual agent behavioral evaluations are future work.

## External signer and read-only consumer bridge

The optional `approval-request` CLI feature produces unsigned canonical requests, while the Linux host verifies signed receipts using root-owned trusted keys (including active/retired/revoked key states). No private signing key or human authentication UI is bundled.

The host can export a **metadata-only, non-authoritative** view of protected state. Consumer CLI inspection validates its structure, not its provenance. Neither the manifest nor an unsigned approval request authorizes protected writes. Full authenticated policy content delivery remains outside the implemented scope.

## Protected host audit witness

An optional Linux host maintains a root-protected append-only hash chain outside the consumer repository. This detects a reverted protected state when the audit history remains intact and blocks subsequent writes/exports on mismatch. It does not create a globally tamper-proof log: a privileged actor capable of rewriting both files can bypass local rollback detection. State and audit are distinct file updates, so interrupted commits require manual recovery; see [audit contract](audit.md).

## External witness and recovery classification

The host can export contiguous audit segments without policy contents. An optional, separately administered `amh-witness` CLI maintains a monotonic checkpoint and checks project binding, prior hash, sequence and state digest continuity before advancing. A recovered or forked local journal can be detected when it is checked against a retained external checkpoint. The witness does **not** authenticate the source of new segments or run continuously.

Local `recovery-diagnose` provides read-only classification of uninitialized, unaudited, missing, corrupt, behind, ahead and divergent state. No privileged write-based recovery or automatic journal reset is exposed.
