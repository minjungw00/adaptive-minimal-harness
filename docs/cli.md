# CLI: inspection and change previews

**Status:** Implemented for read-only operations. The core engine, consumer installer, and authenticated writes are distinct capabilities.

## Supported

```sh
amh --help
amh --version
amh state inspect --file ./state.json
amh manifest inspect --file ./state-manifest.json
amh item get --file ./state.json --id R-COST
amh change preview --file ./state.json --proposal ./proposal.json
```

The CLI emits JSON for state inspection, item retrieval and proposal previews. Errors produce a nonzero exit code. It does **not** modify the state file for any of the above commands.

`state inspect` verifies that the snapshot matches replay of its stored history. It reports legacy entries without typed details, but does not invent or migrate those details. An absent state file is reported as an empty new state with `file_exists: false`. This is not an assertion that an existing project's state was found.

The preview's `authorization` is always `not_granted`. A preview is **not** a signed receipt and is not sufficient to authorize a write. It checks expected snapshot revision and structural transition rules.

Example `proposal.json`:

```json
{
  "expected_revision": 1,
  "reason": "Add customer retention as an additional requirement",
  "change": {
    "operation": "ADD",
    "item": {
      "id": "R-RETENTION",
      "kind": "requirement",
      "scope": "project",
      "revision": 1,
      "lifecycle": "draft",
      "content": "Analyze customer retention as a supporting perspective",
      "replaces": null
    }
  }
}
```

A review must be grounded in the output's `affected_items` differences and the current accepted requirements. A missing `details` field is a legacy-compatible but less expressive item; see [legacy migration](legacy-migration.md).

## Not supported

- `amh init`, `amh apply`, `amh compile`, online installation, agent integration.
- Human authentication or issuing signatures.
- An `approved: true` escape hatch.

For optional Linux-only receipt application through a separate OS-protected host, see [trusted host boundary](trusted-host.md).

## Optional external signing request

With `--features approval-request`, the read-only CLI additionally supports `amh change request --file PATH --proposal FILE --project ID --ttl SECONDS`. It prints an unsigned typed request and a canonical signing message for an **independent** signer. It does not authenticate users, issue signatures, or write state. See [signer contract](external-signer.md).

## Read-only metadata import

`amh manifest inspect --file PATH` validates the shape of an informational-only manifest exported by a trusted operator. It always reports `structurally_valid_untrusted`; validating a JSON structure is **not** a signature check. See [state manifest](state-manifest.md).
