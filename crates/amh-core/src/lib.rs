//! Small, dependency-free building blocks for AMH.
//!
//! This module is an initial foundation, not a complete policy engine.
//! Types describe governed items; no model inference or authorization is implied.

use std::collections::BTreeMap;

#[cfg(feature = "signed-approvals")]
mod approval;
mod details;
mod storage;
mod transition;
#[cfg(feature = "signed-approvals")]
pub use approval::{ApprovalError, ApprovalRequest, ApprovalVerifier, SignedApproval};
pub use details::{
    Activation, ConstraintEnforcement, ItemDetails, PrincipleStrength, RequirementRole,
};
pub use storage::{SnapshotStore, StoreError};
pub use transition::{Change, RecordedChange};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Goal,
    Principle,
    Requirement,
    Constraint,
    Decision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemScope {
    User,
    Project,
    Task,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Draft,
    Active,
    Paused,
    Superseded,
    Retired,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedItem {
    pub id: String,
    pub kind: ItemKind,
    pub scope: ItemScope,
    pub revision: u64,
    pub lifecycle: Lifecycle,
    pub content: String,
    /// Optional for snapshots written before typed details were introduced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<ItemDetails>,
    pub replaces: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ItemError {
    EmptyId,
    EmptyContent,
    ZeroRevision,
    DuplicateId(String),
    RevisionMismatch {
        expected: u64,
        actual: u64,
    },
    ItemRevisionMismatch {
        id: String,
        expected: u64,
        actual: u64,
    },
    MissingItem(String),
    InvalidLifecycle {
        from: Lifecycle,
        to: Lifecycle,
    },
    InvalidNewItem,
    InvalidReplacement,
    IncompatibleReplacement,
    RevisionExhausted,
    DetailsKindMismatch,
    InvalidDetails,
    InvalidEventRevision {
        expected: u64,
        actual: u64,
    },
}

impl ManagedItem {
    pub fn new(
        id: impl Into<String>,
        kind: ItemKind,
        scope: ItemScope,
        content: impl Into<String>,
    ) -> Result<Self, ItemError> {
        let item = Self {
            id: id.into(),
            kind,
            scope,
            revision: 1,
            lifecycle: Lifecycle::Draft,
            content: content.into(),
            details: None,
            replaces: None,
        };
        item.validate()?;
        Ok(item)
    }

    /// Construct a typed item. Legacy items without details remain readable.
    pub fn new_typed(
        id: impl Into<String>,
        kind: ItemKind,
        scope: ItemScope,
        content: impl Into<String>,
        details: ItemDetails,
    ) -> Result<Self, ItemError> {
        let mut item = Self::new(id, kind, scope, content)?;
        item.details = Some(details);
        item.validate()?;
        Ok(item)
    }

    pub fn validate(&self) -> Result<(), ItemError> {
        if self.id.trim().is_empty() {
            return Err(ItemError::EmptyId);
        }
        if self.content.trim().is_empty() {
            return Err(ItemError::EmptyContent);
        }
        if self.revision == 0 {
            return Err(ItemError::ZeroRevision);
        }
        if let Some(details) = &self.details {
            details.validate_for(self.kind)?;
        }
        if self.replaces.as_deref() == Some(self.id.as_str()) {
            return Err(ItemError::InvalidReplacement);
        }
        Ok(())
    }
}

/// A minimal in-memory snapshot to exercise identity and optimistic revisions.
///
/// No authorization or persistence is implied by this state model.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSnapshot {
    revision: u64,
    items: BTreeMap<String, ManagedItem>,
    history: Vec<RecordedChange>,
}

impl ItemSnapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn get(&self, id: &str) -> Option<&ManagedItem> {
        self.items.get(id)
    }

    pub fn insert(&mut self, expected_revision: u64, item: ManagedItem) -> Result<u64, ItemError> {
        self.apply_change(expected_revision, Change::Add { item })
    }

    pub fn items(&self) -> impl Iterator<Item = &ManagedItem> {
        self.items.values()
    }

    pub fn history(&self) -> &[RecordedChange] {
        &self.history
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> ManagedItem {
        ManagedItem::new(
            id,
            ItemKind::Principle,
            ItemScope::Project,
            "TDD when applicable",
        )
        .expect("valid fixture")
    }

    #[test]
    fn validates_empty_identifiers_and_content() {
        assert_eq!(
            ManagedItem::new(" ", ItemKind::Goal, ItemScope::User, "goal"),
            Err(ItemError::EmptyId)
        );
        assert_eq!(
            ManagedItem::new("g", ItemKind::Goal, ItemScope::User, " "),
            Err(ItemError::EmptyContent)
        );
    }

    #[test]
    fn starts_as_draft_with_a_first_revision() {
        let item = sample("P-TDD");
        assert_eq!(item.revision, 1);
        assert_eq!(item.lifecycle, Lifecycle::Draft);
    }

    #[test]
    fn rejects_duplicate_identity_without_changing_snapshot() {
        let mut snapshot = ItemSnapshot::default();
        assert_eq!(snapshot.insert(0, sample("P-TDD")), Ok(1));
        assert_eq!(
            snapshot.insert(1, sample("P-TDD")),
            Err(ItemError::DuplicateId("P-TDD".to_owned()))
        );
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot.revision(), 1);
    }

    #[test]
    fn rejects_stale_write_without_changing_snapshot() {
        let mut snapshot = ItemSnapshot::default();
        assert_eq!(snapshot.insert(0, sample("P-TDD")), Ok(1));
        assert_eq!(
            snapshot.insert(0, sample("P-REFACTOR")),
            Err(ItemError::RevisionMismatch {
                expected: 0,
                actual: 1
            })
        );
        assert!(snapshot.get("P-REFACTOR").is_none());
    }

    #[test]
    fn insertion_does_not_modify_existing_item_fields() {
        let mut snapshot = ItemSnapshot::default();
        let first = sample("P-TDD");
        snapshot.insert(0, first.clone()).expect("first insert");
        snapshot
            .insert(1, sample("P-WRITING"))
            .expect("second insert");
        assert_eq!(snapshot.get("P-TDD"), Some(&first));
    }

    #[test]
    fn stores_independent_item_kinds() {
        let mut snapshot = ItemSnapshot::default();
        let requirement = ManagedItem::new(
            "R-A",
            ItemKind::Requirement,
            ItemScope::Task,
            "Analyze cost structure",
        )
        .expect("valid requirement");
        snapshot.insert(0, requirement).expect("insert");
        assert_eq!(
            snapshot.get("R-A").map(|item| item.kind),
            Some(ItemKind::Requirement)
        );
    }
}
