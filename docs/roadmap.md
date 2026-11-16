# Roadmap

**Planning status only.** No planning version number; product release numbers will be assigned independently.

## Foundation
- [x] Rust CLI/library workspace, unit tests, and CI.
- [x] Minimal typed items, revision-checked transitions, replayable history.
- [x] Local JSON snapshot persistence as a library API.
- [x] Draft examples and explicit CLI capability boundaries.
- [ ] Complete type-specific data model and contract schemas.
- [ ] User-facing persistence CLI and policy-change authorization boundary.

## Policy and task contracts
- Extend item models with explicit scope references, owner, source, typed payloads, and governance relations.
- Complete REMOVE and REPRIORITIZE, and introduce a trusted authorization mechanism for policy changes.
- Resolve policy applicability with explicit conflict diagnostics and no recency-based priority.
- Compile a minimal context preserving all applicable mandatory conditions.

## Safe installation
- Detect existing instructions and tooling before install.
- Assign managed/user/shared ownership to each file.
- Preview and verify add/update/delete operations; support recovery and migration.
- Confirm actual deployment state separately from declared desired state.

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
