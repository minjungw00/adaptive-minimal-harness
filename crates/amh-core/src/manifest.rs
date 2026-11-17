//! Metadata-only projection for deliberate, operator-controlled read access.
//!
//! The projection is intentionally non-authoritative and is never accepted as
//! a source of policy decisions, signed approvals, or state mutation.

use crate::{ItemKind, ItemScope, ItemSnapshot, Lifecycle};
use serde::{Deserialize, Serialize};

const MANIFEST_FORMAT: &str = "amh.state-manifest";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestItem {
    pub id: String,
    pub kind: ItemKind,
    pub scope: ItemScope,
    pub lifecycle: Lifecycle,
    pub revision: u64,
    pub has_typed_details: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateManifest {
    pub format: String,
    pub project_id: String,
    pub snapshot_revision: u64,
    pub items: Vec<ManifestItem>,
    pub authority: String,
}

impl StateManifest {
    /// Create a metadata-only view, omitting item contents and event history.
    ///
    /// This is a data-minimization measure, NOT a confidentiality guarantee:
    /// item IDs and lifecycle metadata can themselves be sensitive.
    pub fn from_snapshot(project_id: impl Into<String>, snapshot: &ItemSnapshot) -> Self {
        let items = snapshot
            .items()
            .map(|item| ManifestItem {
                id: item.id.clone(),
                kind: item.kind,
                scope: item.scope,
                lifecycle: item.lifecycle,
                revision: item.revision,
                has_typed_details: item.details.is_some(),
            })
            .collect();
        Self {
            format: MANIFEST_FORMAT.to_owned(),
            project_id: project_id.into(),
            snapshot_revision: snapshot.revision(),
            items,
            authority: "informational_only".to_owned(),
        }
    }

    /// Structural validation only. A valid manifest is NOT an authenticated
    /// state snapshot, signature, or permission to mutate protected state.
    pub fn validate_shape(&self) -> Result<(), String> {
        if self.format != MANIFEST_FORMAT || self.authority != "informational_only" {
            return Err("unsupported or falsely authoritative state manifest".into());
        }
        if self.project_id.is_empty()
            || self.project_id.len() > 128
            || !self.project_id.bytes().all(|value| {
                value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_' | b'.' | b':')
            })
        {
            return Err("invalid manifest project identifier".into());
        }

        let mut previous = None;
        for item in &self.items {
            if item.id.trim().is_empty()
                || item.revision == 0
                || item.revision > self.snapshot_revision
                || previous.is_some_and(|id: &str| id >= item.id.as_str())
            {
                return Err("invalid, duplicated, or unordered manifest item".into());
            }
            previous = Some(item.id.as_str());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Change, ItemKind, ItemScope, ManagedItem};

    #[test]
    fn projection_excludes_private_item_content_and_history() {
        let mut state = ItemSnapshot::default();
        let private_text = "DO-NOT-EXPORT-THIS-PRIVATE-POLICY";
        let item = ManagedItem::new(
            "P-PRIVATE",
            ItemKind::Principle,
            ItemScope::Project,
            private_text,
        )
        .unwrap();
        state.insert(0, item).unwrap();
        state
            .apply_change(
                1,
                Change::Activate {
                    id: "P-PRIVATE".into(),
                    item_revision: 1,
                },
            )
            .unwrap();

        let manifest = StateManifest::from_snapshot("project-alpha", &state);
        assert_eq!(manifest.snapshot_revision, 2);
        assert_eq!(manifest.items.len(), 1);
        assert_eq!(manifest.items[0].id, "P-PRIVATE");
        assert_eq!(manifest.items[0].lifecycle, Lifecycle::Active);

        let json = serde_json::to_string(&manifest).unwrap();
        assert!(!json.contains(private_text));
        assert!(!json.contains("history"));
        assert!(!json.contains("change"));
        assert!(json.contains("informational_only"));
    }

    #[test]
    fn malformed_or_duplicate_projection_is_rejected() {
        let mut manifest = StateManifest::from_snapshot("project-alpha", &ItemSnapshot::default());
        assert!(manifest.validate_shape().is_ok());
        manifest.authority = "permission_to_write".into();
        assert!(manifest.validate_shape().is_err());
        manifest.authority = "informational_only".into();
        manifest.items.push(ManifestItem {
            id: "R-A".into(),
            kind: ItemKind::Requirement,
            scope: ItemScope::Project,
            lifecycle: Lifecycle::Draft,
            revision: 1,
            has_typed_details: false,
        });
        manifest.snapshot_revision = 1;
        assert!(manifest.validate_shape().is_ok());
        manifest.items.push(manifest.items[0].clone());
        assert!(manifest.validate_shape().is_err());
    }

    #[test]
    fn projection_is_deterministic_and_parseable() {
        let state = ItemSnapshot::default();
        let manifest = StateManifest::from_snapshot("alpha", &state);
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let restored: StateManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored, manifest);
        assert_eq!(restored.format, MANIFEST_FORMAT);
    }
}
