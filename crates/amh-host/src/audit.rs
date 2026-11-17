//! Protected audit chain for an optional Linux-only approval host.
//!
//! This journal is a separate monotonic witness to the protected state file.
//! It detects a reverted state file if the audit chain is retained intact.
//! It is NOT proof against an attacker able to replace both protected files,
//! nor is the two-file update crash-atomic. Any mismatch is fail-closed.

use amh_core::{ItemSnapshot, SignedApproval};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FORMAT: &str = "amh.audit-chain";
const FORMAT_REVISION: u32 = 1;
const MAX_JOURNAL_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct AuditBody {
    format: String,
    format_revision: u32,
    project_id: String,
    sequence: u64,
    previous_hash: Option<String>,
    before_revision: u64,
    after_revision: u64,
    before_state_digest: Option<String>,
    after_state_digest: String,
    receipt_digest: String,
    signing_key_id: String,
    recorded_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditEntry {
    body: AuditBody,
    hash: String,
}

fn digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let hash = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in hash {
        write!(&mut out, "{byte:02x}").expect("format into a String cannot fail");
    }
    out
}

fn state_digest(state: &ItemSnapshot) -> Result<String, String> {
    let bytes = serde_json::to_vec(state)
        .map_err(|error| format!("cannot encode canonical state for audit: {error}"))?;
    Ok(digest(&bytes))
}

fn body_hash(body: &AuditBody) -> Result<String, String> {
    let bytes = serde_json::to_vec(body)
        .map_err(|error| format!("cannot serialize audit entry: {error}"))?;
    let mut signed = FORMAT.as_bytes().to_vec();
    signed.push(0);
    signed.extend(bytes);
    Ok(digest(&signed))
}

/// Holds the audit lock across state verification, signed state write,
/// and audit append. The caller must verify filesystem ownership and isolation.
pub(crate) struct AuditGuard {
    path: PathBuf,
    _lock: File,
}

impl AuditGuard {
    pub(crate) fn acquire(path: &Path) -> Result<Self, String> {
        let lock_path = path.with_extension("lock");
        let mut opts = OpenOptions::new();
        opts.create(true).read(true).write(true).mode(0o600);
        let lock = opts
            .open(&lock_path)
            .map_err(|error| format!("cannot open audit lock: {error}"))?;
        lock.lock()
            .map_err(|error| format!("cannot lock audit journal: {error}"))?;
        Ok(Self {
            path: path.to_path_buf(),
            _lock: lock,
        })
    }

    fn entries(&self) -> Result<Vec<AuditEntry>, String> {
        let metadata = match fs::metadata(&self.path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Vec::new());
            }
            Err(error) => return Err(format!("cannot inspect audit chain: {error}")),
        };
        if metadata.len() > MAX_JOURNAL_BYTES {
            return Err("audit chain exceeds size limit; operator maintenance required".into());
        }
        let mut file = File::open(&self.path)
            .map_err(|error| format!("cannot read audit chain: {error}"))?;
        let mut bytes = Vec::new();
        file.take(MAX_JOURNAL_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("cannot read audit chain: {error}"))?;
        if bytes.len() as u64 > MAX_JOURNAL_BYTES {
            return Err("audit chain too large".into());
        }
        if bytes.is_empty() {
            return Err("existing audit chain is empty; manual recovery required".into());
        }
        if bytes.last() != Some(&b'\n') {
            return Err("audit chain ends with an incomplete record".into());
        }

        let mut entries = Vec::new();
        let mut previous_hash: Option<String> = None;
        for line in bytes.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
            let entry: AuditEntry = serde_json::from_slice(line)
                .map_err(|error| format!("corrupt audit entry: {error}"))?;
            let body = &entry.body;
            let expected = u64::try_from(entries.len())
                .map_err(|_| "audit index overflow")?
                .checked_add(1)
                .ok_or("audit index overflow")?;
            if body.format != FORMAT
                || body.format_revision != FORMAT_REVISION
                || body.sequence != expected
                || body.after_revision != body.before_revision + 1
                || body.after_revision != body.sequence
                || body.previous_hash != previous_hash
                || body.project_id.is_empty()
                || body.signing_key_id.is_empty()
                || body.receipt_digest.len() != 64
                || body.after_state_digest.len() != 64
                || entry.hash != body_hash(body)?
            {
                return Err(format!("audit chain integrity check failed at entry {expected}"));
            }
            if expected == 1 {
                if body.before_revision != 0 || body.before_state_digest.is_some() {
                    return Err("first audit entry does not represent a fresh state".into());
                }
            } else if let Some(previous) = entries.last() {
                let previous: &AuditEntry = previous;
                if body.before_revision != previous.body.after_revision
                    || body.before_state_digest.as_deref()
                        != Some(previous.body.after_state_digest.as_str())
                {
                    return Err("audit chain state continuity check failed".into());
                }
            }
            previous_hash = Some(entry.hash.clone());
            entries.push(entry);
        }
        Ok(entries)
    }

    /// Only a fresh state without an audit history or a snapshot matching
    /// the latest protected audit record is accepted.
    pub(crate) fn verify_state(
        &self,
        project_id: &str,
        state: Option<&ItemSnapshot>,
    ) -> Result<u64, String> {
        let entries = self.entries()?;
        match (entries.last(), state) {
            (None, None) => Ok(0),
            (None, Some(_)) => Err(
                "existing state lacks an audit anchor; explicit operator migration required"
                    .into(),
            ),
            (Some(_), None) => Err("protected state is missing but audit history exists".into()),
            (Some(head), Some(state)) => {
                if head.body.project_id != project_id
                    || head.body.after_revision != state.revision()
                    || head.body.after_state_digest != state_digest(state)?
                {
                    return Err(
                        "protected state differs from audit anchor (possible rollback or interrupted write); no automatic repair"
                            .into(),
                    );
                }
                Ok(head.body.after_revision)
            }
        }
    }

    /// Call this only AFTER the exact signed change has been verified and
    /// persisted. An interruption between state replace and audit sync is
    /// intentionally detected as a mismatch on the next operation.
    pub(crate) fn append_approved(
        &self,
        project_id: &str,
        before: Option<&ItemSnapshot>,
        after: &ItemSnapshot,
        receipt: &SignedApproval,
        signing_key_id: &str,
    ) -> Result<(), String> {
        let previous = self.verify_state(project_id, before)?;
        let next = previous.checked_add(1).ok_or("audit revision overflow")?;
        if after.revision() != next || receipt.request.expected_revision != previous {
            return Err("approved state does not match audit revision".into());
        }

        let last_hash = self.entries()?.last().map(|entry| entry.hash.clone());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock precedes Unix epoch")?
            .as_secs();
        let body = AuditBody {
            format: FORMAT.into(),
            format_revision: FORMAT_REVISION,
            project_id: project_id.into(),
            sequence: next,
            previous_hash: last_hash,
            before_revision: previous,
            after_revision: next,
            before_state_digest: before.map(state_digest).transpose()?,
            after_state_digest: state_digest(after)?,
            receipt_digest: digest(&serde_json::to_vec(receipt).map_err(|e| e.to_string())?),
            signing_key_id: signing_key_id.into(),
            recorded_at: now,
        };
        let record = AuditEntry {
            hash: body_hash(&body)?,
            body,
        };
        let mut encoded = serde_json::to_vec(&record)
            .map_err(|error| format!("cannot encode audit record: {error}"))?;
        encoded.push(b'\n');
        let mut options = OpenOptions::new();
        options.create(true).append(true).mode(0o600);
        let mut file = options
            .open(&self.path)
            .map_err(|error| format!("cannot open audit chain for append: {error}"))?;
        file.write_all(&encoded)
            .map_err(|error| format!("cannot append audit record: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("cannot sync audit record: {error}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amh_core::{ApprovalRequest, Change, ItemKind, ItemScope, ManagedItem};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        dir: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "amh-audit-fixture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }
        fn guard(&self) -> AuditGuard {
            AuditGuard::acquire(&self.dir.join("audit.jsonl")).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn event() -> (ItemSnapshot, SignedApproval) {
        let mut state = ItemSnapshot::default();
        let change = Change::Add {
            item: ManagedItem::new(
                "P-PRIVATE", ItemKind::Principle, ItemScope::Project, "secret content"
            ).unwrap(),
        };
        state.apply_change(0, change.clone()).unwrap();
        let receipt = SignedApproval {
            request: ApprovalRequest {
                project_id: "alpha".into(),
                expected_revision: 0,
                change,
                reason: "owner approved".into(),
                issued_at: 100,
                expires_at: 200,
            },
            signature: vec![19; 64],
        };
        (state, receipt)
    }

    #[test]
    fn first_commit_creates_monotonic_anchor_and_never_stores_content() {
        let fixture = Fixture::new();
        let guard = fixture.guard();
        let (state, receipt) = event();
        assert_eq!(guard.verify_state("alpha", None).unwrap(), 0);
        guard.append_approved("alpha", None, &state, &receipt, "primary").unwrap();
        assert_eq!(guard.verify_state("alpha", Some(&state)).unwrap(), 1);
        let raw = fs::read_to_string(fixture.dir.join("audit.jsonl")).unwrap();
        assert!(!raw.contains("secret content"));
        assert!(!raw.contains("owner approved"));
        assert!(raw.contains("primary"));
    }

    #[test]
    fn detects_rollback_of_state_when_audit_is_preserved() {
        let fixture = Fixture::new();
        let guard = fixture.guard();
        let (state, receipt) = event();
        guard.append_approved("alpha", None, &state, &receipt, "primary").unwrap();
        let rolled_back = ItemSnapshot::default();
        assert!(guard.verify_state("alpha", Some(&rolled_back)).is_err());
        assert!(guard.verify_state("alpha", None).is_err());
        assert!(guard.verify_state("wrong-project", Some(&state)).is_err());
    }

    #[test]
    fn rejects_unanchored_existing_state_and_corrupt_chain() {
        let fixture = Fixture::new();
        let guard = fixture.guard();
        let (state, receipt) = event();
        assert!(guard.verify_state("alpha", Some(&state)).is_err());
        guard.append_approved("alpha", None, &state, &receipt, "primary").unwrap();
        let path = fixture.dir.join("audit.jsonl");
        fs::write(&path, b"corrupted audit entry\n").unwrap();
        assert!(guard.verify_state("alpha", Some(&state)).is_err());
    }

    #[test]
    fn refuses_duplicate_or_skipped_revision() {
        let fixture = Fixture::new();
        let guard = fixture.guard();
        let (state, receipt) = event();
        guard.append_approved("alpha", None, &state, &receipt, "primary").unwrap();
        assert!(guard.append_approved("alpha", None, &state, &receipt, "primary").is_err());
        assert!(guard.append_approved("alpha", Some(&state), &state, &receipt, "primary").is_err());
    }

    #[test]
    fn detects_ahead_state_from_interrupted_two_file_commit() {
        let fixture = Fixture::new();
        let guard = fixture.guard();
        let (state, receipt) = event();
        guard.append_approved("alpha", None, &state, &receipt, "primary").unwrap();
        let mut newer = state.clone();
        newer.apply_change(1, Change::Retire { id: "P-PRIVATE".into(), item_revision: 1 }).unwrap();
        assert!(guard.verify_state("alpha", Some(&newer)).is_err());
    }

    #[test]
    fn refuses_truncated_journal() {
        let fixture = Fixture::new();
        let guard = fixture.guard();
        let (state, receipt) = event();
        guard.append_approved("alpha", None, &state, &receipt, "primary").unwrap();
        let path = fixture.dir.join("audit.jsonl");
        let mut contents = fs::read(&path).unwrap();
        contents.pop();
        fs::write(&path, contents).unwrap();
        assert!(guard.verify_state("alpha", Some(&state)).is_err());
    }
}
