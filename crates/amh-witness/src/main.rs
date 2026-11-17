//! Offline, optional audit checkpoint witness intended for an independently
//! administered machine. Hash continuity is not host authentication.
//!
//! The source audit segment MUST arrive through an authenticated operator
//! workflow and this checkpoint file MUST be outside the agent's control.

use amh_audit::{validate_checkpoint, validate_segment, AuditCheckpoint, AuditSegment};
use serde::Serialize;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_SEGMENT_BYTES: u64 = 16 * 1024 * 1024;
const HELP: &str = "AMH offline audit witness (experimental)

USAGE:
    amh-witness --help
    amh-witness check --file CHECKPOINT --project PROJECT_ID < audit-segment.json
    amh-witness record --file CHECKPOINT --project PROJECT_ID < audit-segment.json

The witness protects its own monotonic checkpoint from rollback/forks.
Host authenticity depends on the external operator's trusted transport.
Run on a separately administered machine; never in an agent-writable tree.
";

fn parse_args(args: &[String]) -> Result<(bool, &Path, &str), String> {
    if args.len() != 5
        || args[1] != "--file"
        || args[3] != "--project"
        || !matches!(args[0].as_str(), "check" | "record")
    {
        return Err("usage: amh-witness <check|record> --file PATH --project PROJECT_ID".into());
    }
    let id = args[4].as_str();
    if id.is_empty()
        || id.len() > 128
        || !id.bytes().all(|value| {
            value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_' | b'.' | b':')
        })
    {
        return Err("invalid trusted project ID".into());
    }
    Ok((args[0] == "record", Path::new(&args[2]), id))
}

fn reject_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("symlink is not permitted: {}", path.display()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect {}: {error}", path.display())),
    }
}

fn read_checkpoint(path: &Path, project: &str) -> Result<Option<AuditCheckpoint>, String> {
    reject_symlink(path)?;
    let contents = match fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read checkpoint: {error}")),
    };
    if contents.len() > 16 * 1024 {
        return Err("checkpoint exceeds the size limit".into());
    }
    let checkpoint: AuditCheckpoint = serde_json::from_slice(&contents)
        .map_err(|error| format!("corrupt retained checkpoint: {error}"))?;
    validate_checkpoint(&checkpoint)?;
    if checkpoint.project_id != project {
        return Err("retained checkpoint belongs to another project".into());
    }
    Ok(Some(checkpoint))
}

fn read_segment() -> Result<AuditSegment, String> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(MAX_SEGMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read audit segment: {error}"))?;
    if bytes.len() as u64 > MAX_SEGMENT_BYTES {
        return Err("audit segment is too large; request a smaller increment".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid audit segment: {error}"))
}

fn verification_result(
    previous: Option<&AuditCheckpoint>,
    segment: &AuditSegment,
    expected_project: &str,
) -> Result<Option<AuditCheckpoint>, String> {
    if segment.project_id != expected_project {
        return Err("incoming segment does not match the trusted project".into());
    }
    validate_segment(previous, segment)
}

fn encode<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn write_checkpoint(path: &Path, checkpoint: &AuditCheckpoint) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err("independent witness directory must be provisioned first".into());
    }
    reject_symlink(path)?;
    let basename = path
        .file_name()
        .ok_or("checkpoint path must name a file")?
        .to_string_lossy();
    let contents = serde_json::to_vec_pretty(checkpoint)
        .map_err(|error| format!("cannot encode checkpoint: {error}"))?;

    for attempt in 0..16u8 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let temp = parent.join(format!(
            ".{basename}.{}.{}.{}.tmp",
            std::process::id(),
            now,
            attempt
        ));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("cannot create temporary checkpoint: {error}")),
        };
        let write_result = (|| -> io::Result<()> {
            file.write_all(&contents)?;
            file.sync_all()?;
            fs::rename(&temp, path)?;
            File::open(parent)?.sync_all()
        })();
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temp);
            return Err(format!("checkpoint persistence failed: {error}"));
        }
        return Ok(());
    }
    Err("unable to create a unique checkpoint temporary file".into())
}

#[cfg(unix)]
fn process(
    checkpoint_file: &Path,
    project: &str,
    segment: &AuditSegment,
    record: bool,
) -> Result<String, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let parent = checkpoint_file
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err("independent witness directory does not exist".into());
    }
    let lock_path = checkpoint_file.with_extension("lock");
    reject_symlink(&lock_path)?;
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(lock_path)
        .map_err(|error| format!("cannot open witness lock: {error}"))?;
    lock.lock()
        .map_err(|error| format!("cannot lock witness: {error}"))?;

    let previous = read_checkpoint(checkpoint_file, project)?;
    let updated = verification_result(previous.as_ref(), segment, project)?;
    let checkpoint = updated.as_ref().or(previous.as_ref());
    if record {
        if let Some(ref next) = updated {
            write_checkpoint(checkpoint_file, next)?;
        }
    }
    encode(&serde_json::json!({
        "status": if record { "recorded" } else { "checked_without_write" },
        "project_id": project,
        "sequence": checkpoint.map(|value| value.sequence),
        "advanced": updated.is_some(),
        "warning": "Continuity is verified against the locally retained checkpoint, not authenticated host identity."
    }))
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || matches!(args.as_slice(), [arg] if arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }

    #[cfg(not(unix))]
    {
        eprintln!("amh-witness presently supports Unix only; no checkpoint was changed");
        ExitCode::from(2)
    }
    #[cfg(unix)]
    {
        let result = (|| -> Result<String, String> {
            let (record, path, project) = parse_args(&args)?;
            let segment = read_segment()?;
            process(path, project, &segment, record)
        })();
        match result {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use amh_audit::{
        body_hash, digest, AuditBody, AuditEntry, CHAIN_FORMAT, CHAIN_FORMAT_REVISION,
        SEGMENT_FORMAT,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        directory: PathBuf,
        checkpoint: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let directory = env::temp_dir().join(format!(
                "amh-witness-test-{}-{}",
                std::process::id(),
                TEST_ID.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir_all(&directory).unwrap();
            let checkpoint = directory.join("checkpoint.json");
            Self {
                directory,
                checkpoint,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    fn entry(sequence: u64, previous: Option<&AuditEntry>) -> AuditEntry {
        let body = AuditBody {
            format: CHAIN_FORMAT.into(),
            format_revision: CHAIN_FORMAT_REVISION,
            project_id: "alpha".into(),
            sequence,
            previous_hash: previous.map(|value| value.hash.clone()),
            before_revision: sequence - 1,
            after_revision: sequence,
            before_state_digest: previous.map(|value| value.body.after_state_digest.clone()),
            after_state_digest: digest(format!("state-{sequence}").as_bytes()),
            receipt_digest: digest(format!("receipt-{sequence}").as_bytes()),
            signing_key_id: "primary".into(),
            recorded_at: sequence,
        };
        AuditEntry {
            hash: body_hash(&body).unwrap(),
            body,
        }
    }

    fn segment(from: u64, entries: Vec<AuditEntry>) -> AuditSegment {
        AuditSegment {
            format: SEGMENT_FORMAT.into(),
            project_id: "alpha".into(),
            from_sequence: from,
            entries,
        }
    }

    #[cfg(unix)]
    #[test]
    fn genesis_and_next_checkpoint_are_stored_monotonically() {
        let fixture = Fixture::new();
        let first = entry(1, None);
        let initial = segment(0, vec![first.clone()]);
        assert!(!fixture.checkpoint.exists());
        let report = process(&fixture.checkpoint, "alpha", &initial, true).unwrap();
        assert!(report.contains("\"advanced\": true"));
        let checkpoint = read_checkpoint(&fixture.checkpoint, "alpha")
            .unwrap()
            .unwrap();
        assert_eq!(checkpoint.sequence, 1);

        let updated = segment(1, vec![entry(2, Some(&first))]);
        process(&fixture.checkpoint, "alpha", &updated, true).unwrap();
        assert_eq!(
            read_checkpoint(&fixture.checkpoint, "alpha")
                .unwrap()
                .unwrap()
                .sequence,
            2
        );
    }

    #[cfg(unix)]
    #[test]
    fn stale_or_forked_segment_keeps_retained_checkpoint_unchanged() {
        let fixture = Fixture::new();
        let first = entry(1, None);
        process(
            &fixture.checkpoint,
            "alpha",
            &segment(0, vec![first.clone()]),
            true,
        )
        .unwrap();
        let original = fs::read(&fixture.checkpoint).unwrap();

        assert!(process(
            &fixture.checkpoint,
            "alpha",
            &segment(0, vec![first.clone()]),
            true
        )
        .is_err());

        let mut fork = entry(2, Some(&first));
        fork.body.previous_hash = Some(digest(b"wrong prefix"));
        fork.hash = body_hash(&fork.body).unwrap();
        assert!(process(&fixture.checkpoint, "alpha", &segment(1, vec![fork]), true).is_err());
        assert_eq!(fs::read(&fixture.checkpoint).unwrap(), original);
    }

    #[cfg(unix)]
    #[test]
    fn read_only_check_does_not_create_checkpoint() {
        let fixture = Fixture::new();
        process(
            &fixture.checkpoint,
            "alpha",
            &segment(0, vec![entry(1, None)]),
            false,
        )
        .unwrap();
        assert!(!fixture.checkpoint.exists());
    }

    #[cfg(unix)]
    #[test]
    fn corrupt_existing_checkpoint_is_not_ignored() {
        let fixture = Fixture::new();
        fs::write(&fixture.checkpoint, b"{broken").unwrap();
        assert!(process(
            &fixture.checkpoint,
            "alpha",
            &segment(0, vec![entry(1, None)]),
            true
        )
        .is_err());
        assert_eq!(fs::read(&fixture.checkpoint).unwrap(), b"{broken");
    }

    #[test]
    fn wrong_project_is_not_accepted_as_genesis() {
        assert!(verification_result(None, &segment(0, vec![entry(1, None)]), "other").is_err());
    }

    #[test]
    fn accepts_exact_five_argument_operator_command() {
        let args = vec![
            "record".into(),
            "--file".into(),
            "checkpoint.json".into(),
            "--project".into(),
            "alpha".into(),
        ];
        let (record, path, project) = parse_args(&args).unwrap();
        assert!(record);
        assert_eq!(path, Path::new("checkpoint.json"));
        assert_eq!(project, "alpha");
    }

    #[test]
    fn cannot_use_untrusted_path_or_key_override_as_cli_options() {
        let args = vec![
            "record".into(),
            "--file".into(),
            "checkpoint.json".into(),
            "--project".into(),
            "alpha".into(),
            "--key".into(),
            "untrusted".into(),
        ];
        assert!(parse_args(&args).is_err());
    }
}
