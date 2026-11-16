# Local snapshot persistence

**Status:** Implemented as a Rust library API; the consumer CLI is not yet wired to it.

## Scope and file format

`amh_core::SnapshotStore` stores the **current item snapshot and the complete change history in one JSON document**. This avoids an inconsistent pair of separately written snapshot and event files.

The document contains `format: "amh.item-snapshot"`, `format_revision: 1`, and `snapshot`. The numeric format revision is a **storage compatibility discriminator, not a planning or product release version**. The JSON format is an implementation detail and is **not** the full planned common item JSON contract.

The underlying Rust types are `ManagedItem`, `Change`, `RecordedChange`, and `ItemSnapshot`. Public operations are `SnapshotStore::new`, `load`, and `apply_change`. `ItemSnapshot::replay` reconstructs the in-memory state from the recorded event sequence.

## State transitions

Available: ADD, content MODIFY, explicit compatible REPLACE, ACTIVATE, PAUSE, REACTIVATE, RETIRE.

Not available: field-level arbitrary patching, REMOVE, REPRIORITIZE, typed goal/requirement acceptance criteria, policy-resolution semantics, trusted authorization.

Each successful operation increments the **snapshot revision**; changing an existing item also increments its own item revision. Failed changes leave in-memory state and on-disk state unchanged if failure occurs before replacement.

## Write contract

1. Create the state parent directory if necessary.
2. Open and acquire an exclusive lock on a dedicated sibling `.lock` file.
3. Reload current data and verify that its stored state matches replay of its history.
4. Reject stale expected snapshot revisions, stale item revisions, or invalid transitions.
5. Serialize the resulting state and history together.
6. Write a unique temporary file in the same directory, flush and synchronize its data, then rename it over the prior snapshot.
7. Release the advisory lock.

The implementation rejects a symlink at the direct state file path and at the lock file path when inspected. It does **not** establish a fully hardened sandbox against arbitrary concurrent untrusted filesystem access.

## Failure and durability limitations

- **No auth:** the caller is responsible for authenticating the user and authorizing the requested change. Any process with filesystem permission can edit the file.
- **No cryptographic provenance:** event replay detects inconsistency between stored state and event history, not a malicious actor who rewrites both consistently.
- **No universal power-loss guarantee:** temporary data is synchronized, but parent directory metadata is not yet explicitly synchronized. Crash/power-loss durability is filesystem dependent.
- **Advisory locks only:** cooperating AMH writers are serialized; unrelated writers can ignore locks.
- **Atomicity scope:** one state file, not an arbitrary set of files, integrations, or distributed agents.
- **Schema evolution:** unsupported storage format revisions are rejected rather than silently converted.

## Verification

CI runs `cargo test --workspace --locked`. Store tests cover round-trip loading, replay consistency, stale writes, invalid transitions, malformed snapshots, unknown format revisions, and symlink rejection on Unix.

These tests verify **local storage contracts**, not actual agent compliance or semantic correctness of policies.
