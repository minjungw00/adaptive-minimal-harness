//! A single-file, replay-checked JSON snapshot with serialized change history.
//!
//! File replacement uses a same-directory temporary file and an advisory lock.
//! It protects cooperating AMH processes against lost updates; it does NOT
//! authenticate changes or protect against privileged/uncooperative writers.

use crate::{Change, ItemError, ItemSnapshot};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FORMAT: &str = "amh.item-snapshot";
const FORMAT_REVISION: u32 = 1;

#[derive(Debug)]
pub enum StoreError {
    #[cfg(feature = "signed-approvals")]
    Authorization(crate::ApprovalError),
    Io(io::Error),
    Json(serde_json::Error),
    InvalidFormat,
    Integrity(ItemError),
    InconsistentSnapshot,
    Conflict {
        expected: u64,
        actual: u64,
    },
    SymlinkNotAllowed,
    TemporaryNameExhausted,
}

impl From<io::Error> for StoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSnapshot {
    format: String,
    format_revision: u32,
    snapshot: ItemSnapshot,
}

/// Local persistence only. The caller is responsible for authenticated approval.
#[derive(Clone, Debug)]
pub struct SnapshotStore {
    path: PathBuf,
}

impl SnapshotStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Absence means an empty new project. Corruption is never treated as empty.
    pub fn load(&self) -> Result<ItemSnapshot, StoreError> {
        match fs::symlink_metadata(&self.path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(ItemSnapshot::default());
            }
            Err(error) => return Err(error.into()),
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreError::SymlinkNotAllowed);
            }
            Ok(_) => {}
        }

        let bytes = fs::read(&self.path)?;
        let saved: StoredSnapshot = serde_json::from_slice(&bytes)?;
        if saved.format != FORMAT || saved.format_revision != FORMAT_REVISION {
            return Err(StoreError::InvalidFormat);
        }
        let rebuilt =
            ItemSnapshot::replay(saved.snapshot.history()).map_err(StoreError::Integrity)?;
        if rebuilt != saved.snapshot {
            return Err(StoreError::InconsistentSnapshot);
        }
        Ok(saved.snapshot)
    }

    /// Locks, loads, applies, and commits as one on-disk document.
    ///
    /// A failed precondition or validation leaves the previous document intact.
    pub(crate) fn apply_change(
        &self,
        expected_revision: u64,
        change: Change,
    ) -> Result<ItemSnapshot, StoreError> {
        let parent = self
            .path
            .parent()
            .filter(|part| !part.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let lock_path = self.path.with_extension("lock");
        if let Ok(metadata) = fs::symlink_metadata(&lock_path) {
            if metadata.file_type().is_symlink() {
                return Err(StoreError::SymlinkNotAllowed);
            }
        }
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;
        lock.lock()?;

        // Keep lock alive until the write is complete.
        let mut snapshot = self.load()?;
        if snapshot.revision() != expected_revision {
            return Err(StoreError::Conflict {
                expected: expected_revision,
                actual: snapshot.revision(),
            });
        }
        snapshot
            .apply_change(expected_revision, change)
            .map_err(StoreError::Integrity)?;

        let bytes = serde_json::to_vec_pretty(&StoredSnapshot {
            format: FORMAT.to_owned(),
            format_revision: FORMAT_REVISION,
            snapshot: snapshot.clone(),
        })?;
        self.atomic_replace(parent, &bytes)?;
        drop(lock);
        Ok(snapshot)
    }

    /// Apply an exactly signed change under a caller-supplied trusted project key.
    /// The trusted key and project binding MUST originate outside the agent's
    /// writable workspace; otherwise this is not a security boundary.
    #[cfg(feature = "signed-approvals")]
    pub fn apply_signed(
        &self,
        verifier: &crate::ApprovalVerifier,
        approval: &crate::SignedApproval,
    ) -> Result<ItemSnapshot, StoreError> {
        let approved = verifier
            .verify(approval)
            .map_err(StoreError::Authorization)?;
        self.apply_change(approved.expected_revision, approved.change)
    }

    fn atomic_replace(&self, parent: &Path, bytes: &[u8]) -> Result<(), StoreError> {
        let name = self.path.file_name().ok_or_else(|| {
            StoreError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "state path must name a file",
            ))
        })?;

        for attempt in 0..16_u8 {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|err| StoreError::Io(io::Error::other(err)))?
                .as_nanos();
            let temp = parent.join(format!(
                ".{}.{}.{}.{}.tmp",
                name.to_string_lossy(),
                std::process::id(),
                nonce,
                attempt
            ));

            let mut file = match OpenOptions::new().write(true).create_new(true).open(&temp) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            };

            let result = (|| -> io::Result<()> {
                file.write_all(bytes)?;
                file.sync_all()?;
                drop(file);
                // Same-directory replacement: there is no intermediate empty snapshot.
                fs::rename(&temp, &self.path)
            })();

            if result.is_err() {
                let _ = fs::remove_file(&temp);
            }
            return result.map_err(StoreError::Io);
        }
        Err(StoreError::TemporaryNameExhausted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ItemKind, ItemScope, Lifecycle, ManagedItem};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

    struct TemporaryStore {
        directory: PathBuf,
        store: SnapshotStore,
    }

    impl TemporaryStore {
        fn new() -> Self {
            let id = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
            let directory =
                std::env::temp_dir().join(format!("amh-store-{}-{}", std::process::id(), id));
            fs::create_dir_all(&directory).unwrap();
            let store = SnapshotStore::new(directory.join("state.json"));
            Self { directory, store }
        }
    }

    impl Drop for TemporaryStore {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    fn item(id: &str) -> ManagedItem {
        ManagedItem::new(
            id,
            ItemKind::Principle,
            ItemScope::Project,
            "do the right thing",
        )
        .unwrap()
    }

    #[test]
    fn new_store_loads_as_empty_without_creating_snapshot() {
        let fixture = TemporaryStore::new();
        assert_eq!(fixture.store.load().unwrap(), ItemSnapshot::default());
        assert!(!fixture.store.path().exists());
    }

    #[test]
    fn round_trip_preserves_item_and_history() {
        let fixture = TemporaryStore::new();
        fixture
            .store
            .apply_change(
                0,
                Change::Add {
                    item: item("P-ONE"),
                },
            )
            .unwrap();
        fixture
            .store
            .apply_change(
                1,
                Change::Activate {
                    id: "P-ONE".into(),
                    item_revision: 1,
                },
            )
            .unwrap();
        let loaded = fixture.store.load().unwrap();
        assert_eq!(loaded.revision(), 2);
        assert_eq!(loaded.history().len(), 2);
        assert_eq!(loaded.get("P-ONE").unwrap().lifecycle, Lifecycle::Active);
        assert_eq!(ItemSnapshot::replay(loaded.history()), Ok(loaded));
    }

    #[test]
    fn stale_revision_does_not_overwrite_document() {
        let fixture = TemporaryStore::new();
        fixture
            .store
            .apply_change(
                0,
                Change::Add {
                    item: item("P-ONE"),
                },
            )
            .unwrap();
        let before = fs::read(fixture.store.path()).unwrap();
        let error = fixture.store.apply_change(
            0,
            Change::Add {
                item: item("P-TWO"),
            },
        );
        assert!(matches!(
            error,
            Err(StoreError::Conflict {
                expected: 0,
                actual: 1
            })
        ));
        assert_eq!(fs::read(fixture.store.path()).unwrap(), before);
    }

    #[test]
    fn failed_transition_does_not_change_file() {
        let fixture = TemporaryStore::new();
        fixture
            .store
            .apply_change(
                0,
                Change::Add {
                    item: item("P-ONE"),
                },
            )
            .unwrap();
        let before = fs::read(fixture.store.path()).unwrap();
        let error = fixture.store.apply_change(
            1,
            Change::Pause {
                id: "P-ONE".into(),
                item_revision: 1,
            },
        );
        assert!(matches!(
            error,
            Err(StoreError::Integrity(ItemError::InvalidLifecycle { .. }))
        ));
        assert_eq!(fs::read(fixture.store.path()).unwrap(), before);
    }

    #[test]
    fn malformed_file_is_not_treated_as_empty() {
        let fixture = TemporaryStore::new();
        fs::write(fixture.store.path(), b"{corrupt").unwrap();
        assert!(matches!(fixture.store.load(), Err(StoreError::Json(_))));
        assert!(fixture
            .store
            .apply_change(
                0,
                Change::Add {
                    item: item("P-ONE")
                }
            )
            .is_err());
        assert_eq!(fs::read(fixture.store.path()).unwrap(), b"{corrupt");
    }

    #[test]
    fn tampered_snapshot_is_rejected_even_if_json_is_valid() {
        let fixture = TemporaryStore::new();
        fixture
            .store
            .apply_change(
                0,
                Change::Add {
                    item: item("P-ONE"),
                },
            )
            .unwrap();
        let bytes = fs::read(fixture.store.path()).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["snapshot"]["revision"] = serde_json::json!(999);
        fs::write(fixture.store.path(), serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(
            fixture.store.load(),
            Err(StoreError::InconsistentSnapshot)
        ));
    }

    #[test]
    fn unknown_format_is_rejected() {
        let fixture = TemporaryStore::new();
        fixture
            .store
            .apply_change(
                0,
                Change::Add {
                    item: item("P-ONE"),
                },
            )
            .unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.store.path()).unwrap()).unwrap();
        value["format_revision"] = serde_json::json!(42);
        fs::write(fixture.store.path(), serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(
            fixture.store.load(),
            Err(StoreError::InvalidFormat)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_snapshot() {
        use std::os::unix::fs::symlink;
        let fixture = TemporaryStore::new();
        let target = fixture.directory.join("outside.json");
        fs::write(&target, b"untouched").unwrap();
        symlink(&target, fixture.store.path()).unwrap();
        assert!(matches!(
            fixture.store.load(),
            Err(StoreError::SymlinkNotAllowed)
        ));
        assert!(fixture
            .store
            .apply_change(
                0,
                Change::Add {
                    item: item("P-ONE")
                }
            )
            .is_err());
        assert_eq!(fs::read(target).unwrap(), b"untouched");
    }
}
