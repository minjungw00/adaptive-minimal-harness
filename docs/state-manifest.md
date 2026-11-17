# Metadata-only state manifests

**Status:** Experimental read-only projection. Not a trusted or signed copy of protected state.

## Purpose

The privileged host writes policy changes only to its protected store. An operator can separately export a **metadata-only** view for a consumer repository to inspect, without exposing the full policy content or event history.

```sh
# Run the approved, independently installed host binary under operator control:
sudo /usr/local/bin/amh-host export-manifest my-project > .amh/state-manifest.json

# Validate structure as an unprivileged user:
amh manifest inspect --file .amh/state-manifest.json
```

The command outputs `StateManifest` JSON containing project ID, snapshot revision, and for each item its ID, kind, scope, lifecycle, revision, and whether typed details exist. It omits item `content`, typed details themselves, and change history.

A protected state must already exist; exporting a nonexistent or invalid protected state fails instead of emitting an empty success result. The caller can **select a configured project by ID**, but cannot supply an arbitrary state path or public key.

## Non-authoritative by design

- The exported manifest includes `authority: "informational_only"`.
- The unprivileged CLI returns `status: "structurally_valid_untrusted"` after structural validation.
- Its contents may be modified by any principal with write permission to the consumer project's copied file.
- It is **not signed** and does not establish that the projected revision is current.
- Never use the manifest alone for authorization, policy enforcement, trusted approval, or state mutation.
- Item IDs and status metadata may themselves be sensitive; export only to intentionally selected consumers.

This is a deliberate minimum disclosure bridge, **not** the full policy delivery mechanism. Distributing selected actionable principles will require a separate integrity-verified, explicitly authorized channel or a trusted read service.
