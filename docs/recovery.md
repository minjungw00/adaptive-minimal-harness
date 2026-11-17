# Recovery diagnosis and manual recovery

**Status:** Non-mutating diagnosis implemented; actual restoration intentionally requires an operator-run process. No automatic repair or audit reset exists.

## Diagnosis

On a correctly provisioned Linux protected host:

```sh
amh-host recovery-diagnose my-project
```

The command reads the root-owned trusted project configuration, inspects the protected state and validates the audit chain under its lock. It returns JSON containing a classification, available state/audit revisions, whether the local files are safe to proceed, and an operator action message. **It does not change state, repair journal entries, or grant approval.**

| Classification | Meaning | Permitted response |
|---|---|---|
| `healthy` | State revision and digest match the audit head | Normal guarded operations may continue |
| `uninitialized` | Neither protected state nor audit journal exists | New project may be initialized by a newly approved signed change |
| `unaudited_state` | Existing state without a protected audit anchor | Freeze and perform independently reviewed migration |
| `missing_state` | Audit journal exists but state is absent | Freeze; compare independently retained backups |
| `state_behind_audit` | State revision is behind the audit head | Treat as suspected rollback |
| `state_ahead_of_audit` | State revision is beyond the audit head | Suspect interrupted state/audit update or unauthorized change |
| `state_digest_mismatch` | Same revision but different state contents | Treat as divergence or tampering |
| `audit_project_mismatch` | Journal belongs to a different project | Stop and investigate trust configuration |
| `corrupt_audit` | Audit chain is malformed, truncated, or inconsistent | Preserve evidence; do not reset the log |
| `corrupt_state` | State cannot be structurally loaded and replay-verified | Preserve evidence; do not overwrite the state |

All classifications except `healthy` and `uninitialized` report `safe_to_proceed: false`. **These are local diagnostics**; a compromised operator environment can still fabricate local files, so independently retained evidence matters.

## Mandatory human-operated recovery procedure

1. **Freeze:** stop receipt application and do not run commands that write the protected state or journal.
2. **Capture evidence:** preserve read-only copies of state, audit journal, signed receipts and host configuration, together with timestamps and file metadata. Protect copies from the agent and do not expose private signer keys.
3. **Diagnose:** run `recovery-diagnose` and save its output; record whether the state is ahead, behind, missing or malformed.
4. **Consult an independent witness:** compare against the separately held `AuditCheckpoint`. If the host can still safely export audit segments, check them against that checkpoint. If it cannot, use independently verified backups; do not force the host to export corrupted data.
5. **Verify candidate backups:** replay each state snapshot, validate its journal chain, compare project identity and revision continuity, and inspect approvals associated with the intended changes.
6. **Decide with trusted authority:** an authenticated operator must decide how to reconcile a partial commit or restore from a backup. Never treat an untrusted assistant's recommendation or `approved=true` flag as authority.
7. **Restore using a separately reviewed operational procedure:** the current AMH CLI has no recovery-write command. Review both files as a pair; document the selected state, lost work, and external witness implications.
8. **Revalidate before reopening:** ensure the state and journal agree, the external witness does not contradict the recovered history, trust keys are current, and new operations are explicitly authorized.

In particular, a state ahead of its audit could represent an **approved change followed by a failed journal append**. Automatically reverting the state might discard legitimate work; automatically appending a journal record could conceal an unauthorized change. The correct recovery action cannot be inferred from revision numbers alone.

## Limits

A local audit journal and an independently stored checkpoint still cannot authenticate fabricated **forward** records without externally trusted evidence. Fully automatic recovery, cryptographic host identity, a hardened signer, external append-only retention, and audited disaster-recovery tools remain future work.

These procedures protect evidence and prevent unsafe automatic writes. They do **not** guarantee recovery of all data after hardware failure or administrative compromise.
