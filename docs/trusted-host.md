# Optional Linux trusted host

**Status:** Experimental, Linux-only integration of existing signed approval verification and persistence. **Not a security-audited production approval service.**

The ordinary `amh` CLI is read-only. A separate `amh-host` binary is an optional **on-demand** writer for environments that need independently protected state. It does not run a daemon and does not carry a private signing key.

## Fixed trust locations

The host never takes a project ID, state path, or verification key from command-line flags, environment variables, or the unsigned receipt.

- Trusted host configuration: `/etc/amh/trust.json`
- Protected state root: `/var/lib/amh/projects`
- Independent audit witness root: `/var/lib/amh/audit`
- Expected state paths: `/var/lib/amh/projects/<project>/state.json`

The host checks that trusted configuration and protected state paths have no symlink components and that existing ancestors/files are root-owned and not group-/world-writable. It refuses unresolved or untrusted paths.

Host configuration format:

```json
{
  "projects": [
    {
      "project_id": "my-project",
      "state_file": "/var/lib/amh/projects/my-project/state.json",
      "keys": [
        {"id": "primary", "public_key_hex": "<64 hex characters>", "status": "active"},
        {"id": "old", "public_key_hex": "<64 hex characters>", "status": "retired"}
      ]
    }
  ]
}
```

Each `public_key_hex` holds the **public** Ed25519 verification key only. It must correspond to a signing key managed by an external trusted signer. Never place private signing keys inside the project repository, CI logs or agent-visible configuration.

## Operator provisioning prerequisites

A trusted operator provisions root-owned directories and configuration *outside* an agent-writable project. For example, after reviewing the binary and the intended host configuration:

```sh
sudo install -d -o root -g root -m 0755 /etc/amh
sudo install -d -o root -g root -m 0755 /var/lib/amh/projects/my-project
sudo install -d -o root -g root -m 0700 /var/lib/amh/audit
# A trusted administrator installs /etc/amh/trust.json owned by root.
sudo chown root:root /etc/amh/trust.json
sudo chmod 0644 /etc/amh/trust.json
```

**Do not run a binary compiled from an untrusted, agent-writable checkout as root.** Package approval, installation provenance, and OS service isolation remain the operator's responsibility.

## Use

```sh
amh-host --help
amh-host doctor
amh-host apply < signed-approval.json
amh-host export-manifest my-project
```

The `apply` command reads a size-limited `SignedApproval` from standard input rather than following an attacker-controlled receipt path. It checks the independent protected audit witness before changes, verifies project binding and Ed25519 signature, checks the current snapshot revision, writes via the protected `SnapshotStore::apply_signed` path, and appends a hashed audit record before reporting success. A stale approval cannot authorize a different snapshot revision.

The optional CLI `change request` generates an **unsigned** approval request and its canonical signing bytes; **creating a signature and obtaining authenticated human consent are not implemented**. An external signer must review and sign the exact request, including its short validity window and the expected revision. Do not send an unsigned preview JSON directly to the host and mistake it for an approval.

## Trust limitations

- The host does not authenticate users; it verifies a cryptographic authorization issued by a trusted external signer.
- A process with write access to the protected state or trust configuration can bypass this verification. Strong protection therefore depends on independently managed OS identities, restricted permissions, a trustworthy binary, and an external signer.
- The host supports multiple configured verification keys, staged rotation, and key status `active`/`retired`/`revoked`, but **does not** provide an administrative key-provisioning UI, private-key custody, audit signatures, or multi-platform support.
- It is not a sandbox, a secure enclave, or a replacement for OS permissions.
- Filesystem metadata checks are not a general defense against a compromised root account. Production deployment needs an independent security review.
- The host checks its protected state against the audit witness before exporting metadata to stdout upon explicit operator request, not the actual protected policy content. The resulting consumer copy is **not authenticated** and cannot be used for writes or enforcement.

This is an **optional enforcement integration**. A lightweight CLI-only user does not need the host or root-owned configuration.

## Key lifecycle in protected configuration

Use either the previous single-key `public_key_hex` field for compatibility, **or** a nonempty `keys` array. Supplying both is rejected. Each named key has a unique `id`, `public_key_hex`, `status`, and optional `not_before` / `accept_until` Unix epoch seconds. A project may have up to 16 keys.

- `active`: eligible to verify receipts during its time window.
- `retired`: no new receipts accepted; retain metadata for operator records.
- `revoked`: no new receipts accepted, including previously issued unexpired receipts, when a new host invocation loads the updated trust file.

For staged rotation, add a new active verifier while the old is active. Once the signer uses the new key, change the old key to `retired` or `revoked`. The receipt does not include a key ID; the host selects the unique active key whose signature verification succeeds and includes `signing_key_id` in its output. A revoked key is no longer eligible to authorize a receipt.

This is **configuration-enforced verification status**, not automatic key issuance, online revocation checks, or a complete audit trail. Only a trusted operator may modify `/etc/amh/trust.json`.

## Read-only consumer bridge

`amh-host export-manifest PROJECT_ID` reads the protected state, verifies its replayed history, and returns metadata only. It never accepts a caller-provided state path or public key. An explicitly selected project ID is a **selector among trusted configured projects**, not a trust-anchor override. See [metadata-only manifest](state-manifest.md).

External signing protocol: [signer contract](external-signer.md).

## Audit witness and interrupted-write handling

The host requires `/var/lib/amh/audit` to exist as an independently protected, root-owned directory. Audit and lock files are private. Existing state without an audit witness is refused until a trusted migration is designed and executed.

The two-file state/audit update is not crash-atomic. An interrupted write will cause later `apply`, `doctor` and `export-manifest` operations to fail closed rather than guessing a repair. A retained audit record detects rollback of the state file. It does **not** detect an attacker with authority to roll back both.

See [audit and recovery contract](audit.md) before deploying the host.

## Optional external checkpoint operations

```sh
amh-host export-audit my-project 0
amh-host recovery-diagnose my-project
```

`export-audit` returns up to 256 consecutive audit records after the specified revision. A separate `amh-witness` CLI can validate that segment and record its hash checkpoint on an independently administered host. The signed approval body and policy contents are not exported.

`recovery-diagnose` classifies corrupted, missing, ahead, behind, and mismatched protected state without performing any repair. See [external witness](external-witness.md) and [manual recovery](recovery.md). Neither feature provides a globally authenticated audit service.
