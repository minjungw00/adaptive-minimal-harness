//! Optional Linux-only, on-demand signed-change host.
//!
//! This entrypoint has a FIXED root-owned trust configuration. It never accepts
//! a public key, project identifier, or state path from the untrusted caller.
//! An external human-authenticated signer and OS-level file isolation are
//! necessary for this to form a real authorization boundary.

#[cfg(target_os = "linux")]
mod linux {
    use amh_core::{ApprovalVerifier, SignedApproval, SnapshotStore};
    use serde::Deserialize;
    use std::collections::BTreeSet;
    use std::fs;
    use std::io::{self, Read};
    use std::os::unix::fs::MetadataExt;
    use std::path::{Component, Path, PathBuf};
    use std::process::ExitCode;

    const TRUST_FILE: &str = "/etc/amh/trust.json";
    const STATE_ROOT: &str = "/var/lib/amh/projects";
    const MAX_RECEIPT_BYTES: u64 = 256 * 1024;

    const HELP: &str = "AMH trusted host (Linux-only, optional)

USAGE:
    amh-host --help
    amh-host doctor
    amh-host apply < signed-approval.json

TRUST:
    Root-owned configuration: /etc/amh/trust.json
    Root-owned state files: /var/lib/amh/projects/<project>/state.json

No command-line or environment override for trusted keys and state paths.
The host does NOT sign approvals; an external authenticated signer is needed.
";

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct HostConfig {
        projects: Vec<TrustedProject>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TrustedProject {
        project_id: String,
        state_file: PathBuf,
        public_key_hex: String,
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
            parse_key(&project.public_key_hex)?;
            if !ids.insert(id) || !paths.insert(&project.state_file) {
                return Err("duplicate trusted project or state path".into());
            }
        }
        Ok(())
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

        validate_state_security(&project.state_file)?;
        let verifier = ApprovalVerifier::new(
            project.project_id.clone(),
            parse_key(&project.public_key_hex)?,
        )
        .map_err(|e| format!("invalid project verifying key: {e:?}"))?;
        let state = SnapshotStore::new(&project.state_file)
            .apply_signed(&verifier, &signed)
            .map_err(|e| format!("receipt not applied: {e:?}"))?;

        serde_json::to_string_pretty(&serde_json::json!({
            "status": "applied",
            "project_id": project.project_id,
            "snapshot_revision": state.revision()
        }))
        .map_err(|e| format!("could not encode result: {e}"))
    }

    fn run(args: &[String]) -> Result<String, String> {
        match args {
            [] => Ok(HELP.to_owned()),
            [arg] if arg == "--help" || arg == "-h" => Ok(HELP.to_owned()),
            [arg] if arg == "doctor" => {
                let config = load_config()?;
                for project in &config.projects {
                    validate_state_security(&project.state_file)?;
                }
                Ok(format!(
                    "Protected configuration valid for {} project(s). No state changed.",
                    config.projects.len()
                ))
            }
            [arg] if arg == "apply" => apply(&load_config()?),
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
                public_key_hex: "11".repeat(32),
            };
            assert!(validate_config(&HostConfig {
                projects: vec![make(), make()]
            })
            .is_err());
        }

        #[test]
        fn untrusted_callers_cannot_set_keys_or_state_paths() {
            assert!(run(&["apply".into(), "--public-key".into(), "my-key".into()]).is_err());
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
