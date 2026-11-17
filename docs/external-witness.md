# Independent audit checkpoint witness

**Status:** Experimental offline witness. Planning documents intentionally have no release version.

AMH can now export **incremental, content-free audit segments** from a protected Linux host and check their continuity against a checkpoint retained on a **separately administered system**.

This is a conditional rollback/fork detector, not a tamper-proof log or an authenticated remote attestation service.

## Operator workflow

A trusted operator first provisions a directory on a different machine or separate security domain, outside agent-writable source trees. For example:

```sh
install -d -m 0700 "$HOME/amh-witness"
```

For initial registration, on the **protected host**:

```sh
amh-host export-audit my-project 0 > initial-audit-segment.json
```

Transfer the segment to the **independent witness** over an authenticated operator-controlled channel, such as a correctly verified SSH connection. On the witness:

```sh
amh-witness check --file "$HOME/amh-witness/my-project.json" \
  --project my-project < initial-audit-segment.json
amh-witness record --file "$HOME/amh-witness/my-project.json" \
  --project my-project < initial-audit-segment.json
```

The first segment must contain the original genesis record (sequence 1). An uninitialized host cannot export an audit segment.

For subsequent updates, inspect the retained checkpoint's `sequence`, for example `N`, then export from that sequence:

```sh
amh-host export-audit my-project N > new-audit-segment.json
```

Transfer it via the trusted channel and call `amh-witness record` again. The tool rejects a segment whose starting sequence, previous hash, state digest, or project does not match the retained checkpoint. It also rejects a segment that restarts at an earlier sequence.

Each host export contains at most **256 records**. Repeat the operation using the updated checkpoint sequence to advance through more records. The witness limits incoming JSON to **16 MiB**.

## Data contract

`AuditSegment` contains:

- `format: "amh.audit-segment"`
- `project_id`
- `from_sequence`: sequence of the retained checkpoint, or zero for genesis
- `entries`: contiguous `AuditEntry` records with their hashes

`amh-witness record` writes an `AuditCheckpoint` with:

- `format: "amh.audit-checkpoint"`
- `project_id`
- `sequence`
- `head_hash`
- `state_digest`

The witness uses a sibling lock and a same-directory temporary file, synchronizes the file and directory where supported, and updates the checkpoint only after validating the entire received segment. Failure or a fork leaves the prior checkpoint intact. A `check` operation never updates the checkpoint, although it may create a lock file.

Audit segments do not export policy text, typed details, or signed approval bodies. **The hashes, project ID, timestamps, and key IDs are still potentially sensitive operational metadata.**

## Security boundaries and limitations

1. **SHA-256 continuity is not proof of who generated the segment.** A malicious host could fabricate a *future* hash chain that extends an old checkpoint. Authenticated transport, operator identity, approval receipts, and external attestation are separate requirements.
2. The independent witness must be on a separate protected system or under an independently controlled security principal. A checkpoint kept alongside the original journal can be rolled back with it.
3. The witness CLI accepts a checkpoint path from the invoking operator, so the operator—not the segment—must select a trusted location. An untrusted agent controlling the checkpoint file or the command arguments can undermine the protection.
4. If the external checkpoint is lost or rewritten, the witness cannot detect that loss solely from hashes. Secure backups, append-only external retention, or signed external attestations are separate improvements.
5. The host does **not** contact the independent witness before every change. The external witness detects inconsistency **when new evidence is submitted and compared**, not continuously or synchronously.
6. The witness does not authenticate human approvals or reverify Ed25519 receipts from hash-only audit entries. It verifies continuity against the last independently retained checkpoint.

The witness is not required for the normal lightweight `amh` CLI. Its runtime and persistence are optional.

See [protected audit chain](audit.md), [recovery guidance](recovery.md), and [trusted-host boundary](trusted-host.md).
