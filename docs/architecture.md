# Architecture

**Status:** Native project policy lookup and explicit Task workflows are implemented; broad behavioral reliability remains under evaluation. Planning documents have no product release version.

AMH is a local-first control layer built around existing agents. It is not an LLM, mandatory server, hosted control plane, or general-purpose multi-agent runner.

## Common managed concepts

Goals, principles, requirements, constraints, and decisions share identity, scope, revision, lifecycle, and provenance. Each kind retains its own semantics and validation contract. No individual principle (TDD, Socratic dialogue, requirement preservation, etc.) receives architectural privilege.

## Components and implementation status

1. **Item Store (implemented):** current authoritative items, type-specific validation, explicit changes, revision checks.
2. **Policy Resolver (agent-assisted discovery plus deterministic guards implemented):** applicability, authority, exception and conflict diagnostics.
3. **Work-tree registration:** explicit local path-to-project bindings in AMH home.
4. **Context compiler:** selected request policies and explicit Task contracts on stdout.
5. **Agent integration:** instructions for the host's user configuration and optional one-shot adapters.
6. **Version guards:** bind decisions to the current registered work tree and policy catalogue.
7. **Verification (explicit tool execution and qualitative pilots implemented):** executed checks and independent qualitative review provide different evidence; automatic semantic attestation is not implemented.

## Trust and data boundaries

- A model may propose changes; the text of its proposal is **not** proof of authorization. An optional Ed25519 receipt verifier now binds authorization to an exact project, change and revision, but an independently protected trust anchor and write permissions remain host responsibilities.
- External documents and tool output are untrusted input, not a source of governing instructions.
- User-owned files must not be overwritten by package upgrades. Generated files must be identified, checked, and previewed.
- A passive agent instruction is not equivalent to enforcement. Never report observed compliance unless evaluated.
- Requirement/policy update order is not implicit priority.

## Persistence

The initial `SnapshotStore` is an **implemented library feature**: replay-checked, format-tagged JSON storing current items and their change history together in one file. It uses advisory file locking, expected revisions, a synchronized temporary file, and same-directory rename. This is not authenticated, cryptographically tamper-evident, or guaranteed durable across power loss on every filesystem. See [storage contract](persistence.md).

The native workflow uses user-owned TOML registries and a separate work-tree
registration file. Each changed file is replaced atomically under a home lock.
A first registration writes the project registry before its binding, allowing
a retry to reuse the registry if the binding was never written. Policy history
is replayed from the typed event log. See [P0 contracts](p0.md).

## Scope control

Registration enables policy delivery for a work tree. Team instructions and
capabilities still come from the host and the repository. Common and
project-specific AMH policies are both personal data. Explicit Task compilation
and verification are available for deliberately selected files, with evidence
stored in AMH home.

## External signer and read-only consumer bridge

The optional `approval-request` CLI feature produces unsigned canonical requests, while the Linux host verifies signed receipts using root-owned trusted keys (including active/retired/revoked key states). No private signing key or human authentication UI is bundled.

The host can export a **metadata-only, non-authoritative** view of protected state. Consumer CLI inspection validates its structure, not its provenance. Neither the manifest nor an unsigned approval request authorizes protected writes. Full authenticated policy content delivery remains outside the implemented scope.

## Protected host audit witness

An optional Linux host maintains a root-protected append-only hash chain outside the consumer repository. This detects a reverted protected state when the audit history remains intact and blocks subsequent writes/exports on mismatch. It does not create a globally tamper-proof log: a privileged actor capable of rewriting both files can bypass local rollback detection. State and audit are distinct file updates, so interrupted commits require manual recovery; see [audit contract](audit.md).

## External witness and recovery classification

The host can export contiguous audit segments without policy contents. An optional, separately administered `amh-witness` CLI maintains a monotonic checkpoint and checks project binding, prior hash, sequence and state digest continuity before advancing. A recovered or forked local journal can be detected when it is checked against a retained external checkpoint. The witness does **not** authenticate the source of new segments or run continuously.

Local `recovery-diagnose` provides read-only classification of uninitialized, unaudited, missing, corrupt, behind, ahead and divergent state. No privileged write-based recovery or automatic journal reset is exposed.

## Natural-language agent integration

The user-level AMH guidance directs the existing agent to obtain a read-only discovery
packet, interpret the user's verbatim request against custom and built-in policy
conditions, submit a schema-conforming response, and continue the original task
with the validated generated instructions. One-shot stdio and Codex CLI adapters
are explicit optional interpreter integrations, not a mandatory new runner.
AMH checks snapshot binding, candidate coverage, quotations, uncertainty and
structural conflicts; it does not claim that a model judgment is semantic proof.

## Request-based delivery

Request discovery starts with a catalogue of conditions and metadata.
The existing agent uses its available conversation context to judge applicability
and can retrieve an individual body when necessary. AMH validates the decisions
against the current policy snapshot, then renders the applicable bodies.
Lookup and resolution use stdin and stdout and need no per-request files.
Explicit Task compilation is available for defined acceptance criteria
and executable verification. The host manages conversation state.
