# Signed change approval: trust and limitations

**Status:** Optional library feature implemented; secure host integration not yet implemented. Planning documents have no release version.

## Purpose

An untrusted agent cannot establish user approval by writing `approved: true` or claiming approval in natural language. AMH supports externally issued, Ed25519-signed receipts that bind the exact requested change to a specific project and snapshot revision.

Cryptographic verification protects the **receipt**. Actual authorization also requires an externally pinned trust anchor and filesystem controls. AMH alone is not a sandbox or operating-system security boundary.

## Optional feature

Build the core with `--features signed-approvals`. The base feature set does not pull in the Ed25519 implementation.

The library provides:

- `ApprovalRequest` — project ID, expected revision, exact `Change`, reason, issuance time, expiration.
- `ApprovalRequest::signing_message()` — domain-separated bytes for an **external** Ed25519 signing service.
- `SignedApproval` — typed request and 64-byte signature.
- `ApprovalVerifier::new(project_id, public_key)` — verify against the host-supplied trusted public key.
- `SnapshotStore::apply_signed` — verify, check state revision, and persist the approved change.

Only a private internal store method performs a write without a signature. External clients cannot call that method through the library API, but direct filesystem writes remain possible if operating-system permissions allow them.

## Signed request contract

The signature covers the exact serialized typed `ApprovalRequest`, prefixed with the protocol domain `AMH:change-approval:v1\0`.

The verifier checks:
1. A valid Ed25519 signature under the supplied trusted public key.
2. Exact match between signed project ID and the host's configured project ID.
3. A nonempty reason.
4. A positive validity window of at most one hour.
5. The current system time lies within the signed validity window.
6. The snapshot still has the signed expected revision; item-level revision and transition checks are performed by the reducer.

There is no generic `approved` boolean, and a signature authorizing one item or revision does not authorize another.

The protocol domain's numeral is a signing-format identifier, **not** a product release or planning version.

## Trust boundary requirements

For an actual deployment, the trusted host MUST:

- Obtain informed approval using a user-authenticated interface outside the agent's control.
- Sign the exact, human-reviewed request with a private key not accessible to the agent.
- Pin the corresponding public key and project identifier in configuration the agent cannot edit.
- Protect the writable state file or run writes through a separate privileged service/account.
- Prevent the agent from substituting its own public key, approval source, project ID or state path.
- Record an authorization audit trail and handle key rotation and revocation.

If the same actor can replace the public key, alter the state file directly, and call the library, **the signature check does not enforce approval**.

A trusted host/OS identity boundary, signer UI, key rotation, and revocation are **future work**. Do not interpret existing unit tests as evidence that those features are already delivered.

## Security tests

With `signed-approvals`, tests check correct signatures, wrong keys, project mismatch, tampered changes and revisions, expired and not-yet-valid approvals, unreasonable validity windows, and attempts to replay a signed change after state advancement. Integration tests additionally check that signed changes persist, replay is rejected by revision, and forged approvals do not create state.

## Design follow-up

Before enabling consumer-facing mutation commands, implement:

1. Trusted trust-anchor location and project binding outside the untrusted repository.
2. Authenticated human approval and exact change preview.
3. Securely stored signing keys and key lifecycle.
4. OS-level filesystem principal separation or a restricted write service.
5. CLI error reporting and recovery paths without leaking signing keys.

The signed-check mechanism is a reusable building block, not yet an end-to-end authorization product.
