//! Optional Linux-only, on-demand signed-change host.
//!
//! This entrypoint has a FIXED root-owned trust configuration. It never accepts
//! a public key, project identifier, or state path from the untrusted caller.
//! An external human-authenticated signer and OS-level file isolation are
//! necessary for this to form a real authorization boundary.

#[cfg(target_os = "linux")]
mod audit;

#[cfg(target_os = "linux")]
mod linux {
    use crate::audit::{AuditGuard, AuditHead};
    use amh_core::{ApprovalVerifier, SignedApproval, SnapshotStore, StateManifest};
    use serde::Deserialize;
    use std::collections::BTreeSet;
    use std::fs;
    use std::io::{self, Read};
    use std::os::unix::fs::MetadataExt;
    use std::path::{Component, Path, PathBuf};
    use std::process::ExitCode;

    const TRUST_FILE: &str = "/etc/amh/trust.json";
    const STATE_ROOT: &str = "/var/lib/amh/projects";
    const AUDIT_ROOT: &str = "/var/lib/amh/audit";
    const MAX_RECEIPT_BYTES: u64 = 256 * 1024;

    const HELP: &str = "AMH trusted host (Linux-only, optional)

USAGE:
    amh-host --help
    amh-host doctor
    amh-host apply < signed-approval.json
    amh-host export-manifest PROJECT_ID
    amh-host export-audit PROJECT_ID AFTER_SEQUENCE
    amh-host recovery-diagnose PROJECT_ID

TRUST:
    Root-owned configuration: /etc/amh/trust.json
    Root-owned state files: /var/lib/amh/projects/<project>/state.json
    Protected audit chain: /var/lib/amh/audit/<project>.jsonl

No command-line or environment override for trusted keys and state paths.
The host does NOT sign approvals; an external authenticated signer is needed.
";

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct HostConfig {
        projects: Vec<TrustedProject>,
    }

    #[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
    #[serde(rename_all = "snake_case")]
    enum KeyStatus {
        Active,
        Retired,
        Revoked,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TrustedKey {
        id: String,
        public_key_hex: String,
        status: KeyStatus,
        #[serde(default)]
        not_before: Option<u64>,
        #[serde(default)]
        accept_until: Option<u64>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TrustedProject {
        project_id: String,
        state_file: PathBuf,
        /// Backward-compatible single verifier. Cannot coexist with `keys`.
        #[serde(default)]
        public_key_hex: Option<String>,
        #[serde(default)]
        keys: Vec<TrustedKey>,
    }

    fn parse_key(hex: &str) -> Result<[u8; 32], String> {
        if hex.len() != 64 || !hex.is_ascii() {
            return Err("trusted key must contain 64 hexadecimal characters".into());
        }
        let mut result = [0; 32];
        for (index, byte) in result.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
                .map_err(|_| "trusted key is not hexadecimal")?;
        }
        Ok(result)
    }

    fn validate_state_path(path: &Path) -> Result<(), String> {
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
            || !path.starts_with(STATE_ROOT)
            || path.parent() == Some(Path::new(STATE_ROOT))
            || path.file_name().and_then(|name| name.to_str()) != Some("state.json")
        {
            return Err(
                "untrusted state path (must be /var/lib/amh/projects/<project>/state.json)".into(),
            );
        }
        Ok(())
    }

    /// All ancestor directories must be root-owned, non-symlink and not
    /// group/other-writable. An absent final state/lock file is permitted.
    fn check_protected_path(path: &Path, absent_final_ok: bool) -> Result<(), String> {
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
        {
            return Err("protected path must be absolute without dot components".into());
        }

        for component in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
            let is_leaf = component == path;
            let meta = match fs::symlink_metadata(component) {
                Ok(meta) => meta,
                Err(error)
                    if is_leaf && absent_final_ok && error.kind() == io::ErrorKind::NotFound =>
                {
                    return Ok(());
                }
                Err(error) => {
                    return Err(format!(
                        "protected path inaccessible: {} ({error})",
                        component.display()
                    ))
                }
            };
            if meta.file_type().is_symlink() || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
                return Err(format!(
                    "unsafe ownership/permissions/symlink: {}",
                    component.display()
                ));
            }
            if (is_leaf && !meta.is_file()) || (!is_leaf && !meta.is_dir()) {
                return Err(format!(
                    "incorrect protected path type: {}",
                    component.display()
                ));
            }
        }
        Ok(())
    }

    fn validate_state_security(path: &Path) -> Result<(), String> {
        validate_state_path(path)?;
        for protected_file in [path.to_path_buf(), path.with_extension("lock")] {
            check_protected_path(&protected_file, true)?;
            if let Ok(metadata) = fs::symlink_metadata(&protected_file) {
                // Project state can contain sensitive text. An existing file
                // must not be readable or writable by group or other users.
                if metadata.mode() & 0o077 != 0 {
                    return Err(format!(
                        "state or lock file is not private: {}",
                        protected_file.display()
                    ));
                }
            }
        }
        Ok(())
    }

    fn audit_path(project_id: &str) -> PathBuf {
        Path::new(AUDIT_ROOT).join(format!("{project_id}.jsonl"))
    }

    fn validate_audit_security(project_id: &str) -> Result<(), String> {
        let path = audit_path(project_id);
        for item in [path.clone(), path.with_extension("lock")] {
            check_protected_path(&item, true)?;
            if let Ok(metadata) = fs::symlink_metadata(&item) {
                if metadata.mode() & 0o077 != 0 {
                    return Err(format!(
                        "audit file or lock is not private: {}",
                        item.display()
                    ));
                }
            }
        }
        Ok(())
    }

    /// The journal lock protects verification, write, and audit append as
    /// one cooperating-host operation. Crash recovery remains fail-closed.
    fn audited_state(
        project: &TrustedProject,
    ) -> Result<(AuditGuard, Option<amh_core::ItemSnapshot>), String> {
        validate_state_security(&project.state_file)?;
        validate_audit_security(&project.project_id)?;
        let guard = AuditGuard::acquire(&audit_path(&project.project_id))?;
        let state = if project.state_file.exists() {
            Some(
                SnapshotStore::new(&project.state_file)
                    .load()
                    .map_err(|e| format!("invalid protected state: {e:?}"))?,
            )
        } else {
            None
        };
        guard.verify_state(&project.project_id, state.as_ref())?;
        Ok((guard, state))
    }

    fn validate_config(config: &HostConfig) -> Result<(), String> {
        if config.projects.is_empty() {
            return Err("no trusted projects configured".into());
        }
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for project in &config.projects {
            let id = &project.project_id;
            if id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':'))
            {
                return Err("invalid trusted project ID".into());
            }
            validate_state_path(&project.state_file)?;
            let has_legacy = project.public_key_hex.is_some();
            if (has_legacy && !project.keys.is_empty())
                || (!has_legacy && project.keys.is_empty())
                || project.keys.len() > 16
            {
                return Err("exactly one verifier configuration is required (legacy key or 1-16 named keys)".into());
            }
            if let Some(ref key) = project.public_key_hex {
                parse_key(key)?;
            }
            let mut key_ids = BTreeSet::new();
            let mut key_bytes = BTreeSet::new();
            for key in &project.keys {
                if key.id.is_empty()
                    || key.id.len() > 128
                    || !key.id.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':')
                    })
                    || !key_ids.insert(&key.id)
                    || !key_bytes.insert(parse_key(&key.public_key_hex)?)
                    || matches!((key.not_before, key.accept_until), (Some(start), Some(end)) if start >= end)
                {
                    return Err("invalid or duplicated signer key configuration".into());
                }
            }
            if !ids.insert(id) || !paths.insert(&project.state_file) {
                return Err("duplicate trusted project or state path".into());
            }
        }
        Ok(())
    }

    /// Select the unique currently active verifier that accepts the receipt.
    /// The receipt format is unchanged: the Ed25519 signature identifies its key.
    fn select_verifier(
        project: &TrustedProject,
        signed: &SignedApproval,
    ) -> Result<(String, ApprovalVerifier), String> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock is invalid")?
            .as_secs();
        let mut matching = None;

        if let Some(ref legacy_key) = project.public_key_hex {
            let verifier =
                ApprovalVerifier::new(project.project_id.clone(), parse_key(legacy_key)?)
                    .map_err(|_| "invalid legacy trusted verifier")?;
            if verifier.check(signed).is_ok() {
                matching = Some(("legacy".to_owned(), verifier));
            }
        } else {
            for key in &project.keys {
                if key.status != KeyStatus::Active
                    || key
                        .not_before
                        .is_some_and(|start| now < start || signed.request.issued_at < start)
                    || key
                        .accept_until
                        .is_some_and(|end| now >= end || signed.request.expires_at > end)
                {
                    continue;
                }
                let verifier = ApprovalVerifier::new(
                    project.project_id.clone(),
                    parse_key(&key.public_key_hex)?,
                )
                .map_err(|_| "invalid trusted verifier")?;
                if verifier.check(signed).is_ok() {
                    if matching.is_some() {
                        return Err("ambiguous trusted signing key".into());
                    }
                    matching = Some((key.id.clone(), verifier));
                }
            }
        }
        matching.ok_or_else(|| "no active trusted key accepted the signed approval".into())
    }

    fn load_config() -> Result<HostConfig, String> {
        check_protected_path(Path::new(TRUST_FILE), false)?;
        let meta = fs::metadata(TRUST_FILE).map_err(|e| format!("cannot stat trust file: {e}"))?;
        if meta.len() > 1024 * 1024 {
            return Err("trusted configuration too large".into());
        }
        let config: HostConfig = serde_json::from_slice(
            &fs::read(TRUST_FILE).map_err(|e| format!("cannot read trust file: {e}"))?,
        )
        .map_err(|e| format!("invalid trusted configuration: {e}"))?;
        validate_config(&config)?;
        Ok(config)
    }

    fn apply(config: &HostConfig) -> Result<String, String> {
        let mut input = Vec::new();
        io::stdin()
            .take(MAX_RECEIPT_BYTES + 1)
            .read_to_end(&mut input)
            .map_err(|e| format!("cannot read signed receipt: {e}"))?;
        if input.len() as u64 > MAX_RECEIPT_BYTES {
            return Err("signed receipt exceeds size limit".into());
        }
        let signed: SignedApproval =
            serde_json::from_slice(&input).map_err(|e| format!("invalid signed receipt: {e}"))?;

        let project = config
            .projects
            .iter()
            .find(|p| p.project_id == signed.request.project_id)
            .ok_or("no trusted project matches signed receipt")?;

        let (audit, previous) = audited_state(project)?;
        let (key_id, verifier) = select_verifier(project, &signed)?;
        let state = SnapshotStore::new(&project.state_file)
            .apply_signed(&verifier, &signed)
            .map_err(|e| format!("receipt not applied: {e:?}"))?;
        // This second durable file cannot be made atomic with the snapshot
        // using rename alone. On failure, subsequent operations must stop.
        audit.append_approved(
            &project.project_id,
            previous.as_ref(),
            &state,
            &signed,
            &key_id,
        )?;
        audit.verify_state(&project.project_id, Some(&state))?;

        serde_json::to_string_pretty(&serde_json::json!({
            "status": "applied",
            "project_id": project.project_id,
            "signing_key_id": key_id,
            "snapshot_revision": state.revision()
        }))
        .map_err(|e| format!("could not encode result: {e}"))
    }

    /// Deliberately export only a non-authoritative metadata view. The caller
    /// may select a configured project but cannot choose a key or state path.
    /// No item content or change history leaves the protected store.
    fn export_manifest(config: &HostConfig, project_id: &str) -> Result<String, String> {
        let project = config
            .projects
            .iter()
            .find(|item| item.project_id == project_id)
            .ok_or("unknown trusted project")?;
        let (_audit, state) = audited_state(project)?;
        let state = state.ok_or("protected state is not initialized; no manifest was exported")?;
        let manifest = StateManifest::from_snapshot(project_id, &state);
        serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("cannot encode state manifest: {error}"))
    }

    /// Export validated audit records after a known retained checkpoint.
    /// The independent witness MUST authenticate the transport/operator.
    /// This output contains no policy text or signed approval bodies.
    fn export_audit(config: &HostConfig, project_id: &str, after: &str) -> Result<String, String> {
        let from_sequence = after
            .parse::<u64>()
            .map_err(|_| "AFTER_SEQUENCE must be a nonnegative integer")?;
        let project = config
            .projects
            .iter()
            .find(|item| item.project_id == project_id)
            .ok_or("unknown trusted project")?;
        let (audit, state) = audited_state(project)?;
        if state.is_none() {
            return Err("cannot export an audit segment for an uninitialized project".into());
        }
        let segment = audit.export_segment(project_id, from_sequence)?;
        serde_json::to_string(&segment)
            .map_err(|error| format!("cannot serialize audit segment: {error}"))
    }

    fn classify_recovery(
        project_id: &str,
        state: Option<&amh_core::ItemSnapshot>,
        head: Option<&AuditHead>,
    ) -> Result<&'static str, String> {
        match (state, head) {
            (None, None) => Ok("uninitialized"),
            (Some(_), None) => Ok("unaudited_state"),
            (None, Some(_)) => Ok("missing_state"),
            (Some(state), Some(head)) => {
                if project_id != head.project_id {
                    return Ok("audit_project_mismatch");
                }
                if state.revision() < head.revision {
                    return Ok("state_behind_audit");
                }
                if state.revision() > head.revision {
                    return Ok("state_ahead_of_audit");
                }
                let bytes = serde_json::to_vec(state)
                    .map_err(|error| format!("cannot encode state for diagnosis: {error}"))?;
                if amh_audit::digest(&bytes) != head.state_digest {
                    return Ok("state_digest_mismatch");
                }
                Ok("healthy")
            }
        }
    }

    /// Read-only classification of a protected mismatch. Never restores or
    /// re-anchors files; no approval is inferred from a diagnostic report.
    fn recovery_diagnose(config: &HostConfig, project_id: &str) -> Result<String, String> {
        let project = config
            .projects
            .iter()
            .find(|item| item.project_id == project_id)
            .ok_or("unknown trusted project")?;
        validate_state_security(&project.state_file)?;
        validate_audit_security(&project.project_id)?;
        let guard = AuditGuard::acquire(&audit_path(project_id))?;

        let (state, state_error) = if project.state_file.exists() {
            match SnapshotStore::new(&project.state_file).load() {
                Ok(state) => (Some(state), None),
                Err(_) => (None, Some("corrupt_state")),
            }
        } else {
            (None, None)
        };
        let (head, audit_error) = match guard.checked_head() {
            Ok(head) => (head, None),
            Err(_) => (None, Some("corrupt_audit")),
        };
        let classification = if let Some(error) = audit_error {
            error
        } else if let Some(error) = state_error {
            error
        } else {
            classify_recovery(project_id, state.as_ref(), head.as_ref())?
        };
        let result = serde_json::json!({
            "project_id": project_id,
            "classification": classification,
            "state_revision": state.as_ref().map(amh_core::ItemSnapshot::revision),
            "audit_revision": head.as_ref().map(|entry| entry.revision),
            "safe_to_proceed": matches!(classification, "healthy" | "uninitialized"),
            "recovery": "no_mutation_performed",
            "next_step": if matches!(classification, "healthy" | "uninitialized") {
                "No recovery action indicated by local state and audit checks"
            } else {
                "Freeze writes; preserve state, audit, receipts and logs; compare independent witness and trusted backup; require manual operator review"
            }
        });
        serde_json::to_string_pretty(&result)
            .map_err(|error| format!("cannot encode recovery diagnosis: {error}"))
    }

    fn run(args: &[String]) -> Result<String, String> {
        match args {
            [] => Ok(HELP.to_owned()),
            [arg] if arg == "--help" || arg == "-h" => Ok(HELP.to_owned()),
            [arg] if arg == "doctor" => {
                let config = load_config()?;
                for project in &config.projects {
                    audited_state(project)?;
                }
                Ok(format!(
                    "Protected state and audit chain checked for {} project(s). No state changed.",
                    config.projects.len()
                ))
            }
            [arg] if arg == "apply" => apply(&load_config()?),
            [arg, project_id] if arg == "export-manifest" => {
                export_manifest(&load_config()?, project_id)
            }
            [arg, project_id, sequence] if arg == "export-audit" => {
                export_audit(&load_config()?, project_id, sequence)
            }
            [arg, project_id] if arg == "recovery-diagnose" => {
                recovery_diagnose(&load_config()?, project_id)
            }
            _ => Err("unsupported host command; run 'amh-host --help'".into()),
        }
    }

    pub(super) fn main() -> ExitCode {
        let args: Vec<String> = std::env::args().skip(1).collect();
        match run(&args) {
            Ok(output) => {
                println!("{output}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(2)
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_only_strict_ed25519_public_keys() {
            assert_eq!(parse_key(&"ab".repeat(32)).unwrap(), [0xab; 32]);
            assert!(parse_key("1234").is_err());
            assert!(parse_key(&"zz".repeat(32)).is_err());
        }

        #[test]
        fn state_path_must_be_within_protected_host_tree() {
            assert!(validate_state_path(Path::new("/var/lib/amh/projects/one/state.json")).is_ok());
            assert!(validate_state_path(Path::new("/tmp/agent/state.json")).is_err());
            assert!(
                validate_state_path(Path::new("/var/lib/amh/projects/../escape/state.json"))
                    .is_err()
            );
            assert!(validate_state_path(Path::new("relative/state.json")).is_err());
        }

        #[test]
        fn duplicate_project_or_state_path_is_rejected() {
            let make = || TrustedProject {
                project_id: "alpha".into(),
                state_file: PathBuf::from("/var/lib/amh/projects/alpha/state.json"),
                public_key_hex: Some("11".repeat(32)),
                keys: vec![],
            };
            assert!(validate_config(&HostConfig {
                projects: vec![make(), make()]
            })
            .is_err());
        }

        #[test]
        fn audit_export_rejects_arbitrary_state_path_or_sequence() {
            assert!(run(&[
                "export-audit".into(),
                "alpha".into(),
                "../untrusted".into(),
            ]).is_err());
            assert!(run(&[
                "export-audit".into(),
                "alpha".into(),
                "0".into(),
                "/tmp/other-state".into(),
            ]).is_err());
        }

        #[test]
        fn untrusted_callers_cannot_set_keys_or_state_paths() {
            assert!(run(&["apply".into(), "--public-key".into(), "my-key".into()]).is_err());
        }

        #[test]
        fn rotates_active_keys_and_rejects_retired_or_revoked_signers() {
            use amh_core::{ApprovalRequest, Change, ItemKind, ItemScope, ManagedItem};
            use ed25519_dalek::{Signer, SigningKey};
            use std::time::{SystemTime, UNIX_EPOCH};

            let old = SigningKey::from_bytes(&[17_u8; 32]);
            let next = SigningKey::from_bytes(&[19_u8; 32]);
            let public = |key: &SigningKey| {
                key.verifying_key()
                    .to_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            };
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let request = ApprovalRequest {
                project_id: "alpha".into(),
                expected_revision: 0,
                change: Change::Add {
                    item: ManagedItem::new("P-1", ItemKind::Principle, ItemScope::Project, "TDD")
                        .unwrap(),
                },
                reason: "Owner approved policy".into(),
                issued_at: now - 1,
                expires_at: now + 120,
            };
            let sign = |key: &SigningKey| SignedApproval {
                signature: key
                    .sign(&request.signing_message().unwrap())
                    .to_bytes()
                    .to_vec(),
                request: request.clone(),
            };
            let key = |id: &str, signer: &SigningKey, status| TrustedKey {
                id: id.into(),
                public_key_hex: public(signer),
                status,
                not_before: None,
                accept_until: None,
            };
            let mut project = TrustedProject {
                project_id: "alpha".into(),
                state_file: PathBuf::from("/var/lib/amh/projects/alpha/state.json"),
                public_key_hex: None,
                keys: vec![
                    key("old", &old, KeyStatus::Active),
                    key("new", &next, KeyStatus::Active),
                ],
            };
            assert!(validate_config(&HostConfig {
                projects: vec![TrustedProject {
                    project_id: project.project_id.clone(),
                    state_file: project.state_file.clone(),
                    public_key_hex: None,
                    keys: vec![
                        key("old", &old, KeyStatus::Active),
                        key("new", &next, KeyStatus::Active)
                    ],
                }]
            })
            .is_ok());
            assert_eq!(select_verifier(&project, &sign(&old)).unwrap().0, "old");
            assert_eq!(select_verifier(&project, &sign(&next)).unwrap().0, "new");
            project.keys[0].status = KeyStatus::Retired;
            assert!(select_verifier(&project, &sign(&old)).is_err());
            assert_eq!(select_verifier(&project, &sign(&next)).unwrap().0, "new");
            project.keys[0].status = KeyStatus::Revoked;
            assert!(select_verifier(&project, &sign(&old)).is_err());
            project.keys[1].not_before = Some(now + 300);
            assert!(select_verifier(&project, &sign(&next)).is_err());
        }

        #[test]
        fn prevents_ambiguous_key_configuration() {
            let config = serde_json::json!({
                "projects": [{
                    "project_id": "alpha",
                    "state_file": "/var/lib/amh/projects/alpha/state.json",
                    "public_key_hex": "11".repeat(32),
                    "keys": [{
                        "id": "primary",
                        "public_key_hex": "22".repeat(32),
                        "status": "active"
                    }]
                }]
            });
            let parsed: HostConfig = serde_json::from_value(config).unwrap();
            assert!(validate_config(&parsed).is_err());

            let config = serde_json::json!({
                "projects": [{
                    "project_id": "alpha",
                    "state_file": "/var/lib/amh/projects/alpha/state.json",
                    "keys": [
                        {"id": "key", "public_key_hex": "11".repeat(32), "status": "active"},
                        {"id": "key", "public_key_hex": "22".repeat(32), "status": "revoked"}
                    ]
                }]
            });
            let parsed: HostConfig = serde_json::from_value(config).unwrap();
            assert!(validate_config(&parsed).is_err());
        }

        #[test]
        fn exporting_unknown_project_fails_without_side_effects() {
            let config = HostConfig { projects: vec![] };
            assert!(export_manifest(&config, "unknown").is_err());
            assert!(run(&["export-manifest".into(), "/tmp/state.json".into()]).is_err());
        }

        #[test]
        fn metadata_view_never_contains_item_content() {
            use amh_core::{Change, ItemKind, ItemScope, ItemSnapshot, ManagedItem};

            let mut snapshot = ItemSnapshot::default();
            snapshot
                .apply_change(
                    0,
                    Change::Add {
                        item: ManagedItem::new(
                            "P-01",
                            ItemKind::Principle,
                            ItemScope::Project,
                            "private policy contents that must not be exported",
                        )
                        .unwrap(),
                    },
                )
                .unwrap();
            let manifest = StateManifest::from_snapshot("alpha", &snapshot);
            let serialized = serde_json::to_string(&manifest).unwrap();
            assert!(!serialized.contains("private policy contents"));
            assert!(!serialized.contains("history"));
            assert!(serialized.contains("informational_only"));
        }

        #[test]
        fn recovery_classes_distinguish_rollbacks_and_interrupted_writes() {
            use amh_core::{Change, ItemKind, ItemScope, ItemSnapshot, ManagedItem};

            let mut committed = ItemSnapshot::default();
            committed
                .apply_change(
                    0,
                    Change::Add {
                        item: ManagedItem::new(
                            "P-ONE",
                            ItemKind::Principle,
                            ItemScope::Project,
                            "Check quality",
                        )
                        .unwrap(),
                    },
                )
                .unwrap();
            let state_digest = amh_audit::digest(&serde_json::to_vec(&committed).unwrap());
            let head = AuditHead {
                project_id: "alpha".into(),
                revision: 1,
                state_digest,
            };
            assert_eq!(classify_recovery("alpha", Some(&committed), Some(&head)).unwrap(), "healthy");
            assert_eq!(classify_recovery("alpha", None, Some(&head)).unwrap(), "missing_state");
            assert_eq!(classify_recovery("alpha", Some(&committed), None).unwrap(), "unaudited_state");
            assert_eq!(classify_recovery("alpha", None, None).unwrap(), "uninitialized");
            assert_eq!(
                classify_recovery("alpha", Some(&ItemSnapshot::default()), Some(&head)).unwrap(),
                "state_behind_audit"
            );
            let mut ahead = committed.clone();
            ahead
                .apply_change(1, Change::Retire { id: "P-ONE".into(), item_revision: 1 })
                .unwrap();
            assert_eq!(classify_recovery("alpha", Some(&ahead), Some(&head)).unwrap(), "state_ahead_of_audit");
            let bad = AuditHead {
                state_digest: amh_audit::digest(b"mismatched"),
                ..head
            };
            assert_eq!(
                classify_recovery("alpha", Some(&committed), Some(&bad)).unwrap(),
                "state_digest_mismatch"
            );
        }

        #[test]
        fn world_writable_paths_fail_closed() {
            assert!(
                check_protected_path(Path::new("/tmp/amh-test-protected-state"), true).is_err()
            );
        }
    }
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    linux::main()
}

#[cfg(not(target_os = "linux"))]
fn main() -> std::process::ExitCode {
    eprintln!("amh-host currently supports Linux only; no state was modified");
    std::process::ExitCode::from(2)
}
