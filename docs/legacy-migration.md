# Legacy item migration policy

**Status:** Explicit, reviewable typed-details upgrades are supported in the Rust reducer. No automatic migration is performed.

Older snapshots can contain valid items with no `details` field. Such items remain readable and replayable. AMH does not infer whether a requirement is core, what its acceptance criteria are, or what strength an old principle should have.

## Workflow

1. Read the state with `amh state inspect --file state.json`.
2. Inspect individual legacy items with `amh item get --file state.json --id ITEM_ID`.
3. Decide the missing type-specific data with the actual project owner.
4. Prepare a `SET_DETAILS` proposal using the item's current `item_revision` and snapshot `expected_revision`.
5. Run `amh change preview --file state.json --proposal proposal.json` and review the exact change.
6. If authorized, have a **trusted external signer** sign an `ApprovalRequest` representing that exact change; only the separate protected host can apply the resulting `SignedApproval` via its secure write path.

Example review proposal for an existing `R-COST` requirement at revision 2 and snapshot revision 5:

```json
{
  "expected_revision": 5,
  "reason": "Owner specified acceptance criteria for a legacy requirement",
  "change": {
    "operation": "SET_DETAILS",
    "id": "R-COST",
    "item_revision": 2,
    "details": {
      "type": "requirement",
      "role": "core",
      "acceptance_criteria": ["Identify the significant cost drivers"]
    }
  }
}
```

The reducer checks that the `details.type` matches the existing item kind, rejects invalid/duplicate details, increments the existing item's revision, and preserves the unrelated items and change history. Superseded and retired items cannot be silently rewritten.

## Safeguards

- Legacy state is **not** automatically a corrupted state.
- New typed items should use `ManagedItem::new_typed`.
- Do not silently assign semantics to older content.
- No rewrite of historical events; upgrades append explicit events.
- The present snapshot format's compatibility discriminator is a storage format marker, **not** a planning or product release version.
- Actual approval requires protected signing and file permissions, not a free-form flag or chat confirmation.
