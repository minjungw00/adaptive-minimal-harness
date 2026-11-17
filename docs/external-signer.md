# External signer interoperability

**Status:** Unsigned request generation and Ed25519 receipt verification implemented. **Human authentication and signing-key custody are external responsibilities.** This document deliberately has no planning version number.

## Unprivileged request preparation

The optional `approval-request` CLI feature adds:

```sh
cargo run -p amh-cli --features approval-request --locked -- \
  change request --file state.json --proposal proposal.json \
  --project my-project --ttl 300
```

This validates the proposal against the current local state, then prints:

- `status: "untrusted_unsigned_request"`
- `authorization: "not_granted"`
- `request`: typed `ApprovalRequest`
- `signing_message_hex`: domain-separated exact bytes to sign
- a warning that the request does not constitute approval.

The request binds project ID, expected snapshot revision, the exact change, reason, issue time, and expiration. TTL is limited to 1–3600 seconds. No keys are generated or used. **An agent can freely construct a request; it cannot thereby authorize anything.**

## What an independent signer MUST do

1. Authenticate the human operator independently of the agent and its repository.
2. Independently establish the target project ID and that the operator has authority to change it. Never accept `--project` as proof of authority.
3. Display the actual typed change, its effects on the approved state, reason, and expiry to the human. Require informed approval.
4. Recompute the signing message from the **typed `ApprovalRequest`** using `amh_core::ApprovalRequest::signing_message()`, rather than trusting caller-supplied `signing_message_hex`.
5. Use a private Ed25519 signing key not accessible to the agent or untrusted project.
6. Produce the existing `SignedApproval` JSON shape:
   ```json
   {
     "request": { "project_id": "...", "expected_revision": 5, "change": {}, "reason": "...", "issued_at": 0, "expires_at": 1 },
     "signature": [0, 0]
   }
   ```
   The displayed `request` is schematic and `signature` must actually be **64 Ed25519 signature bytes**, expressed as a JSON array of byte values.
7. Deliver the completed receipt to a separately protected `amh-host apply` process.

The signed byte sequence is `b"AMH:change-approval:v1\0" + serde_json::to_vec(typed_request)` (the domain literal denotes a **NUL byte**, not two printable characters). This is a signing protocol discriminator, **not** a planning or product release version.

**Do not sign the enclosing untrusted JSON document** or a hand-constructed JSON representation with different field order. Do not treat a preview or a generated receipt as verified human consent.

## Canonical byte regression vector

For the following typed request:

```json
{"project_id":"p","expected_revision":7,"change":{"operation":"PAUSE","id":"P-A","item_revision":2},"reason":"approved","issued_at":100,"expires_at":200}
```

The signed message is the UTF-8 bytes for `AMH:change-approval:v1`, **one binary NUL byte**, then the compact JSON above without additional whitespace. The Rust test `canonical_signing_message_matches_external_signer_vector` pins this exact byte sequence. An external signing implementation should compare its output against this vector before producing real receipts.

## Operational limits

- The signer must verify the protected state revision and intended human-readable change, not solely the untrusted agent's local copy.
- The host checks time limits and the signed expected revision again under its write lock. Stale and modified receipts fail.
- If a protected state is rolled back externally to an older revision, short-lived receipts may become replayable during their validity period. Durable anti-rollback or receipt IDs are not yet implemented.
- The signer service, UI, key custody, revocation workflow, recovery process, and identity integration are **not included** in this repository.
- Key rotation and retirement in the host's protected trust configuration are documented in [trusted host](trusted-host.md).
