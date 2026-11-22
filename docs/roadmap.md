# Roadmap

**Planning status only.** No planning version number; product release numbers will be assigned independently.

## Foundation
- [x] Rust CLI/library workspace, unit tests, and CI.
- [x] Minimal typed items, revision-checked transitions, replayable history.
- [x] Local JSON snapshot persistence as a library API.
- [x] Draft examples and explicit CLI capability boundaries.
- [x] Optional, structurally validated type-specific details (legacy items remain supported).
- [x] Optional cryptographic approval verification for externally signed changes.
- [ ] Complete common item schema and migrate legacy items to required typed details.
- [x] Experimental Linux protected host consuming externally signed receipts.
- [x] Read-only CLI inspection and change preview.
- [x] Explicit typed-details migrations without inferred semantics.
- [ ] Implement trusted key provisioning and authenticated human approval UX.
- [x] Host configuration supports active/retired/revoked verifier keys and staged rotation.
- [x] Optional unsigned signing-request preparation for external signer interoperability.
- [x] Operator-controlled metadata-only export with untrusted consumer inspection.
- [x] Protected local audit journal and state rollback detection (fail-closed).
- [x] Incremental audit-segment exports and an optional independently retained monotonic checkpoint.
- [x] Read-only recovery diagnosis with explicit failure classifications.
- [ ] Authenticated, independently witnessed audit service, tested crash recovery, secure host review, platform portability, and key issuance/custody.
- [x] User-owned project initialization and explicit local write-plan integration (not authenticated protected writes).

## P0 native project flow
- [x] Work-tree registration under AMH home with read-only previews.
- [x] Separate personal and project TOML registries with task-scoped items.
- [x] Version-checked item/metadata operations, REMOVE, REPRIORITIZE and history-preserving rollback.
- [x] Optional content-pinned development/inquiry packs and custom pack inputs.
- [x] Agent-assisted natural-language applicability with complete grounded assessments and deterministic dependency/conflict guards.
- [x] User-level agent handoff and explicit one-shot stdio/Codex interpreter adapters.
- [x] Minimal task compilation with provenance and no silent budget truncation.
- [x] Repository-free integration, catalogue freshness checks and explicit policy-history import.
- [x] Locked home updates with atomic file replacement.
- [x] Explicit verification-tool execution and local evidence; unobserved compliance remains unknown.
- [x] Library/CLI regression cases and supported walkthrough inputs.
- [x] Linux/macOS/Windows native packages with examples, checksums and tag-triggered release drafts.
- [ ] Publish an approved public release for account-free binary downloads after release gates.
- [x] Initial independent qualitative pilot: 8 fresh actors, 16 two-stage reviews, findings and evidence retained. See [report](../evals/results/2026-11-17-pilot/report.md).
- [x] Read-only natural-language policy lookup and selected-body delivery through stdio.
- [x] Regression coverage for policy catalogue changes, applicable bodies and unchanged request-time files.
- [ ] Repeat multi-turn and cross-model qualitative evaluation on the current protocol. Earlier observations remain available as historical evidence.
- [ ] Broader downstream effectiveness measurement, repeated/cross-model semantic evaluation and rule-removal experiments (P1).

The local write path relies on user filesystem permissions. It is not the
optional signed host and does not claim authenticated policy enforcement.

## Policy and task contracts
- Extend semantic analysis and authority/exception handling beyond the implemented explicit native metadata.
- Evaluate authenticated consumer approval integration; native local REMOVE/REPRIORITIZE are implemented.
- Measure natural-language selection quality and evaluate semantic conflicts beyond declared relations.
- Evaluate compiler effectiveness and support additional native agent formats.

## Safe installation
- Expand existing instruction/tool detection beyond the initial instruction inventory.
- Extend the implemented managed/user/shared ownership model to additional adapters.
- Extend preview/recovery to format migrations and additional integrations.
- Measure effective host context separately from policy delivery.

## Evaluation and adapters
- Agent-specific adapter support; prefer existing Agent Skills and interoperability tools.
- Deterministic contract and regression tests plus measured semantic evaluations.
- Compare bare/minimal/adaptive configurations on real tasks, including false-positive skill triggers.
- Do not build a new agent runner, multi-agent orchestrator, background daemon, or cloud service by default.

## Release gates
- Tests actually pass on supported operating systems.
- License selected and security policy reviewed.
- Installation and removal preserve user files.
- Commands, supported features, and evaluation limitations documented accurately.
