//! Explicit, replayable transitions with optimistic revision guards.
//! This is an in-memory reducer, NOT a trusted authorization boundary.

use crate::{ItemDetails, ItemError, ItemSnapshot, Lifecycle, ManagedItem};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum Change {
    Add {
        item: ManagedItem,
    },
    Modify {
        id: String,
        item_revision: u64,
        content: String,
    },
    Replace {
        id: String,
        item_revision: u64,
        replacement: ManagedItem,
    },
    /// Explicit, version-checked migration or update of kind-specific data.
    SetDetails {
        id: String,
        item_revision: u64,
        details: ItemDetails,
    },
    Activate {
        id: String,
        item_revision: u64,
    },
    Pause {
        id: String,
        item_revision: u64,
    },
    Reactivate {
        id: String,
        item_revision: u64,
    },
    Retire {
        id: String,
        item_revision: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedChange {
    pub revision: u64,
    pub change: Change,
}

impl ItemSnapshot {
    /// A failed transition changes neither the items nor the event history.
    pub fn apply_change(
        &mut self,
        expected_revision: u64,
        change: Change,
    ) -> Result<u64, ItemError> {
        if self.revision != expected_revision {
            return Err(ItemError::RevisionMismatch {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let next = self
            .revision
            .checked_add(1)
            .ok_or(ItemError::RevisionExhausted)?;
        let mut updated = self.items.clone();
        apply_to_items(&mut updated, &change)?;
        self.items = updated;
        self.history.push(RecordedChange {
            revision: next,
            change,
        });
        self.revision = next;
        Ok(next)
    }

    /// Reconstruct state by applying the exact change sequence.
    pub fn replay(history: &[RecordedChange]) -> Result<Self, ItemError> {
        let mut snapshot = Self::default();
        for record in history {
            let next = snapshot
                .revision
                .checked_add(1)
                .ok_or(ItemError::RevisionExhausted)?;
            if record.revision != next {
                return Err(ItemError::InvalidEventRevision {
                    expected: next,
                    actual: record.revision,
                });
            }
            snapshot.apply_change(snapshot.revision, record.change.clone())?;
        }
        Ok(snapshot)
    }
}

fn fresh(item: &ManagedItem) -> Result<(), ItemError> {
    item.validate()?;
    if item.revision != 1 || item.lifecycle != Lifecycle::Draft || item.replaces.is_some() {
        return Err(ItemError::InvalidNewItem);
    }
    Ok(())
}

fn require<'a>(
    items: &'a mut BTreeMap<String, ManagedItem>,
    id: &str,
    expected: u64,
) -> Result<&'a mut ManagedItem, ItemError> {
    let item = items
        .get_mut(id)
        .ok_or_else(|| ItemError::MissingItem(id.to_owned()))?;
    if item.revision != expected {
        return Err(ItemError::ItemRevisionMismatch {
            id: id.to_owned(),
            expected,
            actual: item.revision,
        });
    }
    Ok(item)
}

fn bump(item: &mut ManagedItem) -> Result<(), ItemError> {
    item.revision = item
        .revision
        .checked_add(1)
        .ok_or(ItemError::RevisionExhausted)?;
    Ok(())
}

fn transition(
    items: &mut BTreeMap<String, ManagedItem>,
    id: &str,
    expected: u64,
    from: Lifecycle,
    to: Lifecycle,
) -> Result<(), ItemError> {
    let item = require(items, id, expected)?;
    if item.lifecycle != from {
        return Err(ItemError::InvalidLifecycle {
            from: item.lifecycle,
            to,
        });
    }
    bump(item)?;
    item.lifecycle = to;
    Ok(())
}

fn apply_to_items(
    items: &mut BTreeMap<String, ManagedItem>,
    change: &Change,
) -> Result<(), ItemError> {
    match change {
        Change::Add { item } => {
            fresh(item)?;
            if items.contains_key(&item.id) {
                return Err(ItemError::DuplicateId(item.id.clone()));
            }
            items.insert(item.id.clone(), item.clone());
        }
        Change::Modify {
            id,
            item_revision,
            content,
        } => {
            if content.trim().is_empty() {
                return Err(ItemError::EmptyContent);
            }
            let item = require(items, id, *item_revision)?;
            if matches!(item.lifecycle, Lifecycle::Superseded | Lifecycle::Retired) {
                return Err(ItemError::InvalidLifecycle {
                    from: item.lifecycle,
                    to: item.lifecycle,
                });
            }
            bump(item)?;
            item.content = content.clone();
        }
        Change::SetDetails {
            id,
            item_revision,
            details,
        } => {
            let item = require(items, id, *item_revision)?;
            if matches!(item.lifecycle, Lifecycle::Superseded | Lifecycle::Retired) {
                return Err(ItemError::InvalidLifecycle {
                    from: item.lifecycle,
                    to: item.lifecycle,
                });
            }
            details.validate_for(item.kind)?;
            if item.details.as_ref() == Some(details) {
                return Err(ItemError::UnchangedDetails);
            }
            bump(item)?;
            item.details = Some(details.clone());
        }
        Change::Replace {
            id,
            item_revision,
            replacement,
        } => {
            fresh(replacement)?;
            if replacement.id == *id {
                return Err(ItemError::InvalidReplacement);
            }
            if items.contains_key(&replacement.id) {
                return Err(ItemError::DuplicateId(replacement.id.clone()));
            }
            let old = require(items, id, *item_revision)?;
            if old.lifecycle != Lifecycle::Active {
                return Err(ItemError::InvalidLifecycle {
                    from: old.lifecycle,
                    to: Lifecycle::Superseded,
                });
            }
            if old.kind != replacement.kind || old.scope != replacement.scope {
                return Err(ItemError::IncompatibleReplacement);
            }
            bump(old)?;
            old.lifecycle = Lifecycle::Superseded;
            let mut new_item = replacement.clone();
            new_item.lifecycle = Lifecycle::Active;
            new_item.replaces = Some(id.clone());
            items.insert(new_item.id.clone(), new_item);
        }
        Change::Activate { id, item_revision } => transition(
            items,
            id,
            *item_revision,
            Lifecycle::Draft,
            Lifecycle::Active,
        )?,
        Change::Pause { id, item_revision } => transition(
            items,
            id,
            *item_revision,
            Lifecycle::Active,
            Lifecycle::Paused,
        )?,
        Change::Reactivate { id, item_revision } => transition(
            items,
            id,
            *item_revision,
            Lifecycle::Paused,
            Lifecycle::Active,
        )?,
        Change::Retire { id, item_revision } => {
            let item = require(items, id, *item_revision)?;
            if matches!(item.lifecycle, Lifecycle::Superseded | Lifecycle::Retired) {
                return Err(ItemError::InvalidLifecycle {
                    from: item.lifecycle,
                    to: Lifecycle::Retired,
                });
            }
            bump(item)?;
            item.lifecycle = Lifecycle::Retired;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ItemKind, ItemScope};

    fn sample(id: &str) -> ManagedItem {
        ManagedItem::new(
            id,
            ItemKind::Principle,
            ItemScope::Project,
            "work as specified",
        )
        .unwrap()
    }

    fn active() -> ItemSnapshot {
        let mut s = ItemSnapshot::default();
        s.insert(0, sample("P-A")).unwrap();
        s.apply_change(
            1,
            Change::Activate {
                id: "P-A".into(),
                item_revision: 1,
            },
        )
        .unwrap();
        s
    }

    #[test]
    fn activate_pause_reactivate_retire_are_explicit() {
        let mut s = active();
        s.apply_change(
            2,
            Change::Pause {
                id: "P-A".into(),
                item_revision: 2,
            },
        )
        .unwrap();
        assert_eq!(s.get("P-A").unwrap().lifecycle, Lifecycle::Paused);
        s.apply_change(
            3,
            Change::Reactivate {
                id: "P-A".into(),
                item_revision: 3,
            },
        )
        .unwrap();
        s.apply_change(
            4,
            Change::Retire {
                id: "P-A".into(),
                item_revision: 4,
            },
        )
        .unwrap();
        assert_eq!(s.get("P-A").unwrap().lifecycle, Lifecycle::Retired);
    }

    #[test]
    fn cannot_pause_draft_and_failed_change_preserves_snapshot() {
        let mut s = ItemSnapshot::default();
        s.insert(0, sample("P-A")).unwrap();
        let before = s.clone();
        assert_eq!(
            s.apply_change(
                1,
                Change::Pause {
                    id: "P-A".into(),
                    item_revision: 1
                }
            ),
            Err(ItemError::InvalidLifecycle {
                from: Lifecycle::Draft,
                to: Lifecycle::Paused
            })
        );
        assert_eq!(s, before);
    }

    #[test]
    fn modify_does_not_change_unrelated_item_or_semantics() {
        let mut s = active();
        let other = sample("P-B");
        s.insert(2, other.clone()).unwrap();
        s.apply_change(
            3,
            Change::Modify {
                id: "P-A".into(),
                item_revision: 2,
                content: "revised".into(),
            },
        )
        .unwrap();
        assert_eq!(s.get("P-A").unwrap().revision, 3);
        assert_eq!(s.get("P-A").unwrap().content, "revised");
        assert_eq!(s.get("P-B"), Some(&other));
    }

    #[test]
    fn stale_item_revision_is_rejected_without_changes() {
        let mut s = active();
        let before = s.clone();
        assert!(matches!(
            s.apply_change(
                2,
                Change::Modify {
                    id: "P-A".into(),
                    item_revision: 1,
                    content: "revised".into(),
                }
            ),
            Err(ItemError::ItemRevisionMismatch { .. })
        ));
        assert_eq!(s, before);
    }

    #[test]
    fn replacement_is_explicit_and_records_link() {
        let mut s = active();
        s.apply_change(
            2,
            Change::Replace {
                id: "P-A".into(),
                item_revision: 2,
                replacement: sample("P-B"),
            },
        )
        .unwrap();
        assert_eq!(s.get("P-A").unwrap().lifecycle, Lifecycle::Superseded);
        assert_eq!(s.get("P-B").unwrap().lifecycle, Lifecycle::Active);
        assert_eq!(s.get("P-B").unwrap().replaces.as_deref(), Some("P-A"));
    }

    #[test]
    fn failed_replacement_does_not_change_original_item() {
        let mut s = active();
        let replacement =
            ManagedItem::new("R-X", ItemKind::Requirement, ItemScope::Project, "r").unwrap();
        let before = s.clone();
        assert_eq!(
            s.apply_change(
                2,
                Change::Replace {
                    id: "P-A".into(),
                    item_revision: 2,
                    replacement,
                }
            ),
            Err(ItemError::IncompatibleReplacement)
        );
        assert_eq!(s, before);
    }

    #[test]
    fn retired_item_cannot_be_reactivated() {
        let mut s = active();
        s.apply_change(
            2,
            Change::Retire {
                id: "P-A".into(),
                item_revision: 2,
            },
        )
        .unwrap();
        let before = s.clone();
        assert!(matches!(
            s.apply_change(
                3,
                Change::Reactivate {
                    id: "P-A".into(),
                    item_revision: 3
                }
            ),
            Err(ItemError::InvalidLifecycle { .. })
        ));
        assert_eq!(s, before);
    }

    #[test]
    fn typed_details_can_be_added_to_legacy_item_without_changing_identity() {
        use crate::{Activation, PrincipleStrength};
        let mut snapshot = active();
        let prior = snapshot.get("P-A").unwrap().clone();
        let details = ItemDetails::Principle {
            strength: PrincipleStrength::Must,
            activation: Activation::ExplicitOnly,
            verification: vec!["Run the relevant test".into()],
        };
        assert_eq!(
            snapshot.apply_change(
                2,
                Change::SetDetails {
                    id: "P-A".into(),
                    item_revision: 2,
                    details: details.clone(),
                }
            ),
            Ok(3)
        );
        let changed = snapshot.get("P-A").unwrap();
        assert_eq!(changed.details.as_ref(), Some(&details));
        assert_eq!(changed.content, prior.content);
        assert_eq!(changed.kind, prior.kind);
        assert_eq!(changed.lifecycle, prior.lifecycle);
        assert_eq!(changed.revision, 3);
        assert_eq!(ItemSnapshot::replay(snapshot.history()), Ok(snapshot));
    }

    #[test]
    fn invalid_or_duplicate_details_do_not_mutate_snapshot() {
        use crate::{Activation, PrincipleStrength};
        let mut snapshot = active();
        let before = snapshot.clone();
        let invalid = ItemDetails::Decision {
            rationale: "not a principle".into(),
        };
        assert_eq!(
            snapshot.apply_change(
                2,
                Change::SetDetails {
                    id: "P-A".into(),
                    item_revision: 2,
                    details: invalid,
                }
            ),
            Err(ItemError::DetailsKindMismatch)
        );
        assert_eq!(snapshot, before);
        let details = ItemDetails::Principle {
            strength: PrincipleStrength::Should,
            activation: Activation::Always,
            verification: vec![],
        };
        snapshot
            .apply_change(
                2,
                Change::SetDetails {
                    id: "P-A".into(),
                    item_revision: 2,
                    details: details.clone(),
                },
            )
            .unwrap();
        let again = snapshot.clone();
        assert_eq!(
            snapshot.apply_change(
                3,
                Change::SetDetails {
                    id: "P-A".into(),
                    item_revision: 3,
                    details,
                }
            ),
            Err(ItemError::UnchangedDetails)
        );
        assert_eq!(snapshot, again);
    }

    #[test]
    fn history_replays_to_identical_snapshot() {
        let mut s = active();
        s.apply_change(
            2,
            Change::Modify {
                id: "P-A".into(),
                item_revision: 2,
                content: "new".into(),
            },
        )
        .unwrap();
        assert_eq!(ItemSnapshot::replay(s.history()), Ok(s));
    }

    #[test]
    fn invalid_event_revision_is_rejected() {
        let s = active();
        let mut records = s.history().to_vec();
        records[1].revision = 100;
        assert_eq!(
            ItemSnapshot::replay(&records),
            Err(ItemError::InvalidEventRevision {
                expected: 2,
                actual: 100
            })
        );
    }
}
