# Roadmap

**Planning status only.** No planning version number; product release numbers will be assigned independently.

## Foundation
- Rust native CLI and library, workspace tests and CI.
- Baseline schema and examples for managed items, task contracts, policy selection and deployment observation.
- Clear supported/unsupported CLI behavior.

## Policy and task contracts
- Validate and store typed items and revisions.
- Apply explicit ADD/MODIFY/REPLACE/REMOVE/REPRIORITIZE/PAUSE/REACTIVATE/RETIRE operations with authorization checks.
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
