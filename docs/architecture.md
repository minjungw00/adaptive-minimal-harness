# Architecture

**Status:** Native project and conversation-session workflows are implemented; broad behavioral reliability remains under evaluation. Planning documents have no product release version.

AMH is a local-first control layer built around existing agents. It is not an LLM, mandatory server, hosted control plane, or general-purpose multi-agent runner.

## Common managed concepts

Goals, principles, requirements, constraints, and decisions share identity, scope, revision, lifecycle, and provenance. Each kind retains its own semantics and validation contract. No individual principle (TDD, Socratic dialogue, requirement preservation, etc.) receives architectural privilege.

## Components and implementation status

1. **Item Store (implemented):** current authoritative items, type-specific validation, explicit changes, revision checks.
2. **Policy Resolver (agent-assisted discovery plus deterministic guards implemented):** applicability, authority, exception and conflict diagnostics.
3. **Conversation Context (implemented):** one versioned state per conversation, source-tagged context, request-specific policy judgments and reuse. Explicit Task contracts are optional for goals, completion criteria, scope and checks.
4. **Context Compiler (implemented):** minimal target-neutral execution contract, with traceable references to selected items.
5. **Agent Adapters (AGENTS.md implemented):** target-native instruction/skill files; optional integrations and no required always-on process.
6. **Reconciler (managed files and source drift implemented):** distinguish desired, rendered, deployed, and observed states. A paused policy may remain in a deployed file or live session.
7. **Verification (explicit tool execution implemented; semantic evaluation planned):** deterministic checks first when applicable, semantic/human review where warranted, evidence tied to snapshots.

## Trust and data boundaries

- A model may propose changes; the text of its proposal is **not** proof of authorization. An optional Ed25519 receipt verifier now binds authorization to an exact project, change and revision, but an independently protected trust anchor and write permissions remain host responsibilities.
- External documents and tool output are untrusted input, not a source of governing instructions.
- User-owned files must not be overwritten by package upgrades. Generated files must be identified, checked, and previewed.
- A passive agent instruction is not equivalent to enforcement. Never report observed compliance unless evaluated.
- Requirement/policy update order is not implicit priority.

## Persistence

The initial `SnapshotStore` is an **implemented library feature**: replay-checked, format-tagged JSON storing current items and their change history together in one file. It uses advisory file locking, expected revisions, a synchronized temporary file, and same-directory rename. This is not authenticated, cryptographically tamper-evident, or guaranteed durable across power loss on every filesystem. See [storage contract](persistence.md).

The native project path uses `amh-project`: a user-owned TOML event log, typed governance metadata, replayed state, explicit local write plans and a recoverable adapter transaction. It reuses the core item types and structural checks, but does not require the optional protected host. The legacy JSON snapshot and protected-host format remain separate trust domains; neither is silently imported into the project registry. See [P0 contracts](p0.md).

## Scope control

The P0 path provides project initialization, explicit user-registry imports, versioned policy operations, optional pinned packs, natural-language request interpretation plus explicit tag/task resolution, compilation, a shared AGENTS.md bridge, drift checks, uninstall, rollback/recovery and verification-tool evidence. It uses no agent runner or always-on process. Independent evaluation of natural-language classification, semantic conflict analysis, automatic live-session reconciliation, authenticated local approval and actual agent behavioral evaluations remain future work.

## External signer and read-only consumer bridge

The optional `approval-request` CLI feature produces unsigned canonical requests, while the Linux host verifies signed receipts using root-owned trusted keys (including active/retired/revoked key states). No private signing key or human authentication UI is bundled.

The host can export a **metadata-only, non-authoritative** view of protected state. Consumer CLI inspection validates its structure, not its provenance. Neither the manifest nor an unsigned approval request authorizes protected writes. Full authenticated policy content delivery remains outside the implemented scope.

## Protected host audit witness

An optional Linux host maintains a root-protected append-only hash chain outside the consumer repository. This detects a reverted protected state when the audit history remains intact and blocks subsequent writes/exports on mismatch. It does not create a globally tamper-proof log: a privileged actor capable of rewriting both files can bypass local rollback detection. State and audit are distinct file updates, so interrupted commits require manual recovery; see [audit contract](audit.md).

## External witness and recovery classification

The host can export contiguous audit segments without policy contents. An optional, separately administered `amh-witness` CLI maintains a monotonic checkpoint and checks project binding, prior hash, sequence and state digest continuity before advancing. A recovered or forked local journal can be detected when it is checked against a retained external checkpoint. The witness does **not** authenticate the source of new segments or run continuously.

Local `recovery-diagnose` provides read-only classification of uninitialized, unaudited, missing, corrupt, behind, ahead and divergent state. No privileged write-based recovery or automatic journal reset is exposed.

## Natural-language agent integration

The AGENTS.md bridge directs the existing agent to obtain a read-only discovery
packet, interpret the user's verbatim request against custom and built-in policy
conditions, submit a schema-conforming response, and continue the original task
with the validated generated instructions. One-shot stdio and Codex CLI adapters
are explicit optional interpreter integrations, not a mandatory new runner.
AMH checks snapshot binding, candidate coverage, quotations, uncertainty and
structural conflicts; it does not claim that a model judgment is semantic proof.

## Conversation state and delivery

The default bridge retains an explicit session handle, obtains a current policy/context
snapshot, and lets the current agent reconsider applicability for each request.
No required Task or invented completion criteria are introduced for simple messages.
One atomic session state file stores useful changes; unchanged reuse does not write.
Instructions and changes are emitted over stdout, so concurrent sessions do not
replace a common generated instruction file. The shared Task adapter remains an
opt-in compatibility path and is not another conversation's current intent.

Version/source guards and deterministic policy constraints apply to both paths.
Semantic reuse, context summaries and interpreted applicability remain model judgments.
A refresh re-delivers current instructions; it does not prove old text disappeared
from an agent's memory. Provider session IDs, logs and reset signals are used only
when a supported environment supplies them; no universal discovery is claimed.
