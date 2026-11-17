//! Protected audit chain for an optional Linux-only approval host.
//!
//! This journal is a separate monotonic witness to the protected state file.
//! It detects a reverted state file if the audit chain is retained intact.
//! It is NOT proof against an attacker able to replace both protected files,
//! nor is the two-file update crash-atomic. Any mismatch is fail-closed.

use amh_core::{ItemSnapshot, SignedApproval};
use amh_audit::{
    body_hash, digest, validate_complete, AuditBody, AuditEntry, AuditSegment,
    CHAIN_FORMAT as FORMAT, CHAIN_FORMAT_REVISION as FORMAT_REVISION, SEGMENT_FORMAT,
};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn state_digest(state: &ItemSnapshot) -> Result<String, String> {
    let bytes = serde_json::to_vec(state)
        .map_err(|error| format!("cannot encode canonical state for audit: {error}"))?;
    Ok(digest(&bytes))
}

#[derive(Clone, Debug)]
pub(crate) struct AuditHead {
    pub(crate) project_id: String,
    pub(crate) revision: u64,
    pub(crate) state_digest: String,
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
        let file =
            File::open(&self.path).map_err(|error| format!("cannot read audit chain: {error}"))?;
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
        for line in bytes[..bytes.len() - 1].split(|byte| *byte == b'\n') {
            if line.is_empty() {
                return Err("audit chain contains an empty record".into());
            }
            let entry: AuditEntry = serde_json::from_slice(line)
                .map_err(|error| format!("corrupt audit entry: {error}"))?;
            entries.push(entry);
        }
        validate_complete(&entries)?;
        Ok(entries)
    }

    /// Return an integrity-checked journal head for diagnostic reporting.
    /// A missing journal is different from a damaged journal.
    pub(crate) fn checked_head(&self) -> Result<Option<AuditHead>, String> {
        let entries = self.entries()?;
        Ok(entries.last().map(|entry| AuditHead {
            project_id: entry.body.project_id.clone(),
            revision: entry.body.after_revision,
            state_digest: entry.body.after_state_digest.clone(),
        }))
    }

    /// Content-free, incremental evidence for an independently retained witness.
    pub(crate) fn export_segment(
        &self,
        project_id: &str,
        from_sequence: u64,
    ) -> Result<AuditSegment, String> {
        let entries = self.entries()?;
        let head = entries.last().ok_or("audit chain has no committed entries")?;
        if head.body.project_id != project_id || from_sequence > head.body.sequence {
            return Err("project mismatch or sequence beyond the audit head".into());
        }
        Ok(AuditSegment {
            format: SEGMENT_FORMAT.into(),
            project_id: project_id.into(),
            from_sequence,
            entries: entries
                .into_iter()
                .filter(|entry| entry.body.sequence > from_sequence)
                .collect(),
        })
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
                "existing state lacks an audit anchor; explicit operator migration required".into(),
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
                "P-PRIVATE",
                ItemKind::Principle,
                ItemScope::Project,
                "secret content",
            )
            .unwrap(),
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
        guard
            .append_approved("alpha", None, &state, &receipt, "primary")
            .unwrap();
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
        guard
            .append_approved("alpha", None, &state, &receipt, "primary")
            .unwrap();
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
        guard
            .append_approved("alpha", None, &state, &receipt, "primary")
            .unwrap();
        let path = fixture.dir.join("audit.jsonl");
        fs::write(&path, b"corrupted audit entry\n").unwrap();
        assert!(guard.verify_state("alpha", Some(&state)).is_err());
    }

    #[test]
    fn refuses_duplicate_or_skipped_revision() {
        let fixture = Fixture::new();
        let guard = fixture.guard();
        let (state, receipt) = event();
        guard
            .append_approved("alpha", None, &state, &receipt, "primary")
            .unwrap();
        assert!(guard
            .append_approved("alpha", None, &state, &receipt, "primary")
            .is_err());
        assert!(guard
            .append_approved("alpha", Some(&state), &state, &receipt, "primary")
            .is_err());
    }

    #[test]
    fn detects_ahead_state_from_interrupted_two_file_commit() {
        let fixture = Fixture::new();
        let guard = fixture.guard();
        let (state, receipt) = event();
        guard
            .append_approved("alpha", None, &state, &receipt, "primary")
            .unwrap();
        let mut newer = state.clone();
        newer
            .apply_change(
                1,
                Change::Retire {
                    id: "P-PRIVATE".into(),
                    item_revision: 1,
                },
            )
            .unwrap();
        assert!(guard.verify_state("alpha", Some(&newer)).is_err());
    }

    #[test]
    fn real_signed_state_write_is_witnessed_and_rollback_is_rejected() {
        use amh_core::{ApprovalVerifier, SnapshotStore};
        use ed25519_dalek::{Signer, SigningKey};

        let fixture = Fixture::new();
        let journal = fixture.guard();
        let store = SnapshotStore::new(fixture.dir.join("state.json"));
        let signer = SigningKey::from_bytes(&[31; 32]);
        let verifier = ApprovalVerifier::new("alpha", signer.verifying_key().to_bytes()).unwrap();
        let change = Change::Add {
            item: ManagedItem::new(
                "P-REAL",
                ItemKind::Principle,
                ItemScope::Project,
                "Verify before completion",
            )
            .unwrap(),
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let request = ApprovalRequest {
            project_id: "alpha".into(),
            expected_revision: 0,
            change,
            reason: "User approved the exact principle".into(),
            issued_at: now - 1,
            expires_at: now + 60,
        };
        let signed = SignedApproval {
            signature: signer
                .sign(&request.signing_message().unwrap())
                .to_bytes()
                .to_vec(),
            request,
        };

        assert_eq!(journal.verify_state("alpha", None).unwrap(), 0);
        let committed = store.apply_signed(&verifier, &signed).unwrap();
        journal
            .append_approved("alpha", None, &committed, &signed, "primary")
            .unwrap();
        let recovered = store.load().unwrap();
        assert_eq!(journal.verify_state("alpha", Some(&recovered)).unwrap(), 1);

        // A valid unexpired receipt cannot justify an older state when its
        // separate audit witness is still present.
        fs::remove_file(store.path()).unwrap();
        assert!(journal.verify_state("alpha", None).is_err());
    }

    #[test]
    fn two_approved_events_form_a_continuous_audit_chain() {
        let fixture = Fixture::new();
        let guard = fixture.guard();
        let (first, initial_receipt) = event();
        guard
            .append_approved("alpha", None, &first, &initial_receipt, "old")
            .unwrap();

        let mut second = first.clone();
        let change = Change::Retire {
            id: "P-PRIVATE".into(),
            item_revision: 1,
        };
        second.apply_change(1, change.clone()).unwrap();
        let second_receipt = SignedApproval {
            request: ApprovalRequest {
                project_id: "alpha".into(),
                expected_revision: 1,
                change,
                reason: "owner retired the old rule".into(),
                issued_at: 200,
                expires_at: 260,
            },
            signature: vec![29; 64],
        };
        guard
            .append_approved("alpha", Some(&first), &second, &second_receipt, "new")
            .unwrap();
        assert_eq!(guard.verify_state("alpha", Some(&second)).unwrap(), 2);
        assert!(guard.verify_state("alpha", Some(&first)).is_err());
        assert_eq!(guard.entries().unwrap().len(), 2);
    }

    #[test]
    fn deleting_audit_journal_never_turns_existing_state_into_genesis() {
        let fixture = Fixture::new();
        let guard = fixture.guard();
        let (state, receipt) = event();
        guard
            .append_approved("alpha", None, &state, &receipt, "primary")
            .unwrap();
        fs::remove_file(fixture.dir.join("audit.jsonl")).unwrap();
        assert!(guard.verify_state("alpha", Some(&state)).is_err());
    }

    #[test]
    fn refuses_truncated_journal() {
        let fixture = Fixture::new();
        let guard = fixture.guard();
        let (state, receipt) = event();
        guard
            .append_approved("alpha", None, &state, &receipt, "primary")
            .unwrap();
        let path = fixture.dir.join("audit.jsonl");
        let mut contents = fs::read(&path).unwrap();
        contents.pop();
        fs::write(&path, contents).unwrap();
        assert!(guard.verify_state("alpha", Some(&state)).is_err());
    }
}
