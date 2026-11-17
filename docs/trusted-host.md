# Optional Linux trusted host

**Status:** Experimental, Linux-only integration of existing signed approval verification and persistence. **Not a security-audited production approval service.**

The ordinary `amh` CLI is read-only. A separate `amh-host` binary is an optional **on-demand** writer for environments that need independently protected state. It does not run a daemon and does not carry a private signing key.

## Fixed trust locations

The host never takes a project ID, state path, or verification key from command-line flags, environment variables, or the unsigned receipt.

- Trusted host configuration: `/etc/amh/trust.json`
- Protected state root: `/var/lib/amh/projects`
- Expected state paths: `/var/lib/amh/projects/<project>/state.json`

The host checks that trusted configuration and protected state paths have no symlink components and that existing ancestors/files are root-owned and not group-/world-writable. It refuses unresolved or untrusted paths.

Host configuration format:

```json
{
  "projects": [
    {
      "project_id": "my-project",
      "state_file": "/var/lib/amh/projects/my-project/state.json",
      "public_key_hex": "<64 lowercase or uppercase hex characters>"
    }
  ]
}
```

`public_key_hex` holds the **public** Ed25519 verification key only. It must correspond to a signing key managed by an external trusted signer. Never place private signing keys inside the project repository, CI logs or agent-visible configuration.

## Operator provisioning prerequisites

A trusted operator provisions root-owned directories and configuration *outside* an agent-writable project. For example, after reviewing the binary and the intended host configuration:

```sh
sudo install -d -o root -g root -m 0755 /etc/amh
sudo install -d -o root -g root -m 0755 /var/lib/amh/projects/my-project
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
```

The `apply` command reads a size-limited `SignedApproval` from standard input rather than following an attacker-controlled receipt path. It verifies project binding and Ed25519 signature, checks the current snapshot revision, and writes via the protected `SnapshotStore::apply_signed` path. A stale approval cannot authorize a different snapshot revision.

The existing Rust library provides `ApprovalRequest::signing_message()`; **creating a signature and obtaining authenticated human consent are not implemented in the AMH CLI**. An external signer must review and sign the exact request, including its short validity window and the expected revision. Do not send an unsigned preview JSON directly to the host and mistake it for an approval.

## Trust limitations

- The host does not authenticate users; it verifies a cryptographic authorization issued by a trusted external signer.
- A process with write access to the protected state or trust configuration can bypass this verification. Strong protection therefore depends on independently managed OS identities, restricted permissions, a trustworthy binary, and an external signer.
- The implementation currently has no key-rotation, revocation, audit signature, interactive approval UI, or multi-platform host.
- It is not a sandbox, a secure enclave, or a replacement for OS permissions.
- Filesystem metadata checks are not a general defense against a compromised root account. Production deployment needs an independent security review.
- The current host does not support placing its protected writable state directly inside an untrusted repository. Read-only exported or mirrored state is a later integration problem.

This is an **optional enforcement integration**. A lightweight CLI-only user does not need the host or root-owned configuration.
