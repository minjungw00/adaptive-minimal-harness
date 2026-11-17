# Protected audit chain and state rollback detection

**Status:** Implemented in the optional Linux `amh-host`; this is an experimental local safeguard, not a security-audited audit service. No planning version number is assigned.

## Threat model and purpose

The protected state file is updated only after the host checks an externally signed, project-bound Ed25519 receipt. A state file that is later replaced with an older copy might otherwise accept the same still-valid receipt again.

The host now maintains an independent audit witness in `/var/lib/amh/audit/<project>.jsonl`. It is root-owned, private, and protected by a separate lock file. Every successful approved write appends a hash-chained record containing:

- project ID and monotonically increasing snapshot revision;
- SHA-256 digest of the preceding and resulting canonical snapshots;
- SHA-256 digest of the signed receipt (without storing the receipt contents);
- active signing key ID and record time;
- previous audit-entry hash and the current entry hash.

The audit chain **does not contain the policy text, typed details, or original change history**. Item contents may still be inferable from external information or hashes; treat the audit journal as sensitive administrative data.

## Operator setup

The trusted operator must prepare the independent protected audit location before applying a receipt. It must be inaccessible for modification to the agent or ordinary project user.

```sh
sudo install -d -o root -g root -m 0700 /var/lib/amh/audit
```

As before, the protected state lives in `/var/lib/amh/projects/<project>/state.json`, and project trust keys are configured in `/etc/amh/trust.json`. The host creates audit and lock files with `0600` permissions on their first use.

**Do not store the audit journal in the agent-writable repository.**

## Operational checks

`amh-host apply`:

1. Validates root-owned trust, protected state and audit paths.
2. Locks the project's independent audit journal.
3. Loads the current state (if present) and verifies it matches the latest audit entry.
4. Verifies the exact signed receipt and writes the new state under the existing state lock.
5. Appends and synchronizes the new audit record.
6. Re-checks the state against the new audit head before reporting success.

`amh-host doctor` and `amh-host export-manifest` also check that the protected state matches the retained audit record before reporting a valid status or exporting metadata.

The first approved change can initialize a fresh project with **no state file and no audit journal**. If an existing state file has no audit history, AMH refuses to invent one; explicitly reviewed operator migration is required.

## Fail-closed failure modes

| Condition | Result |
|---|---|
| Current state matches audit head | Allowed to proceed |
| State rolled back but audit retained | Refuse writes, diagnose rollback |
| Audit removed while state remains | Refuse writes or export |
| State missing while audit remains | Refuse writes or export |
| Audit entry corrupted or truncated | Refuse writes or export |
| State advanced but audit append failed | Refuse subsequent operations |
| Existing unaudited state | Operator migration required |

**No silent repair or automatic audit reset is implemented.** Operators must preserve evidence and reconcile from independently trusted backups. Manual restoration requires explicit administrative review of the state, receipt history, and audit witness.

## Crash consistency limitations

A snapshot replacement and append to a **different file** cannot be made atomic with a normal rename. The host writes state, then the audit journal. A crash after the first write can leave state ahead of the journal. The host detects this on the next operation and stops; it cannot safely infer whether the earlier approval was completely processed.

The audit uses a chained SHA-256 digest. This detects accidental corruption and modifications by a principal unable to rewrite the protected journal. **It does not prove authenticity against a privileged attacker who can replace both the state and the audit history.** Complete rollback protection against a compromised host requires a separately controlled append-only witness, signed remote ledger, TPM-backed monotonic counter, or equivalent external trust anchor.

Audit records are constrained to a finite maximum journal size; production rotation, external retention, snapshot schema migration, independently signed attestations, and privileged-account compromise recovery remain future work.

## Testing

The Rust tests exercise audit genesis, consecutive revisions, replayed and reverted states, deleted audit history, corruption and truncation, partial-commit mismatch, and omission of policy text.

Passing tests does not establish secure user authentication, tamper-proof external audit storage, or full OS-hardening. See [trusted host](trusted-host.md) and [external signer](external-signer.md).

## Incremental external evidence and local diagnosis

The protected host now exposes `amh-host export-audit PROJECT_ID AFTER_SEQUENCE`, producing at most 256 content-free, hashed audit records after the specified revision. An independently administered `amh-witness` command can validate a contiguous segment and store the resulting monotonic checkpoint on another machine. The observer can reject a reverted or forked chain **when submitted evidence is checked**; it does not authenticate the source of arbitrary forward records.

For local failures, `amh-host recovery-diagnose PROJECT_ID` classifies the state/audit relationship without changing files. See [independent witness](external-witness.md) and [recovery guidance](recovery.md).

The original protected audit journal remains the host's local source. External checkpoints must be held under an independent security principal and checked during incident response. They are not a replacement for proper backup retention or authenticated host attestations.
