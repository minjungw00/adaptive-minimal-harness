//! Shared policy item types for AMH.
//!
//! Typed fields describe user-defined policies.

mod details;
pub use details::{
    Activation, ConstraintEnforcement, ItemDetails, PrincipleStrength, RequirementRole,
};

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
    InvalidReplacement,
    DetailsKindMismatch,
    InvalidDetails,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
