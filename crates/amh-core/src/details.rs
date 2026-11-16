//! Structural, type-specific validation of explicitly typed managed items.
//!
//! The optional details field preserves historical snapshots. A new consumer
//! API should call ManagedItem::new_typed rather than relying on legacy fields.

use crate::{ItemError, ItemKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipleStrength {
    Must,
    Should,
    May,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Activation {
    Always,
    OnTags { any: Vec<String> },
    ExplicitOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementRole {
    Core,
    Support,
    Unspecified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintEnforcement {
    Advisory,
    ExternalGate,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ItemDetails {
    Goal {
        success_criteria: Vec<String>,
    },
    Principle {
        strength: PrincipleStrength,
        activation: Activation,
        verification: Vec<String>,
    },
    Requirement {
        role: RequirementRole,
        acceptance_criteria: Vec<String>,
    },
    Constraint {
        enforcement: ConstraintEnforcement,
        checks: Vec<String>,
    },
    Decision {
        rationale: String,
    },
}

fn complete(values: &[String], required: bool) -> bool {
    (!required || !values.is_empty()) && values.iter().all(|value| !value.trim().is_empty())
}

impl ItemDetails {
    pub fn kind(&self) -> ItemKind {
        match self {
            Self::Goal { .. } => ItemKind::Goal,
            Self::Principle { .. } => ItemKind::Principle,
            Self::Requirement { .. } => ItemKind::Requirement,
            Self::Constraint { .. } => ItemKind::Constraint,
            Self::Decision { .. } => ItemKind::Decision,
        }
    }

    pub fn validate_for(&self, kind: ItemKind) -> Result<(), ItemError> {
        if self.kind() != kind {
            return Err(ItemError::DetailsKindMismatch);
        }
        let valid = match self {
            Self::Goal { success_criteria } => complete(success_criteria, true),
            Self::Principle {
                strength,
                activation,
                verification,
            } => {
                let activation_valid = match activation {
                    Activation::Always | Activation::ExplicitOnly => true,
                    Activation::OnTags { any } => complete(any, true),
                };
                activation_valid && complete(verification, *strength == PrincipleStrength::Must)
            }
            Self::Requirement {
                acceptance_criteria,
                ..
            } => complete(acceptance_criteria, true),
            Self::Constraint {
                enforcement,
                checks,
            } => complete(checks, *enforcement == ConstraintEnforcement::ExternalGate),
            Self::Decision { rationale } => !rationale.trim().is_empty(),
        };
        if !valid {
            return Err(ItemError::InvalidDetails);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ItemScope, ManagedItem};

    #[test]
    fn typed_goal_requires_a_success_criterion() {
        let goal = ItemDetails::Goal {
            success_criteria: vec![],
        };
        assert_eq!(
            ManagedItem::new_typed("G-1", ItemKind::Goal, ItemScope::Project, "Build AMH", goal),
            Err(ItemError::InvalidDetails)
        );
    }

    #[test]
    fn typed_principle_checks_activation_and_verification() {
        let principle = ItemDetails::Principle {
            strength: PrincipleStrength::Must,
            activation: Activation::OnTags {
                any: vec!["code.behavior_change".into()],
            },
            verification: vec!["RED evidence exists".into()],
        };
        let item = ManagedItem::new_typed(
            "P-TDD",
            ItemKind::Principle,
            ItemScope::Project,
            "Apply TDD to behavior changes",
            principle,
        )
        .unwrap();
        assert_eq!(item.details.as_ref().unwrap().kind(), ItemKind::Principle);
    }

    #[test]
    fn mismatched_kind_is_rejected() {
        assert_eq!(
            ManagedItem::new_typed(
                "R-X",
                ItemKind::Requirement,
                ItemScope::Task,
                "Report",
                ItemDetails::Decision {
                    rationale: "approved".into()
                },
            ),
            Err(ItemError::DetailsKindMismatch)
        );
    }

    #[test]
    fn required_check_must_not_be_empty() {
        let details = ItemDetails::Constraint {
            enforcement: ConstraintEnforcement::ExternalGate,
            checks: vec![],
        };
        assert_eq!(
            ManagedItem::new_typed(
                "C-1",
                ItemKind::Constraint,
                ItemScope::Project,
                "Do not deploy without approval",
                details,
            ),
            Err(ItemError::InvalidDetails)
        );
    }

    #[test]
    fn structural_details_round_trip() {
        let item = ManagedItem::new_typed(
            "R-A",
            ItemKind::Requirement,
            ItemScope::Project,
            "Cost analysis",
            ItemDetails::Requirement {
                role: RequirementRole::Core,
                acceptance_criteria: vec!["List main cost drivers".into()],
            },
        )
        .unwrap();
        let text = serde_json::to_string(&item).unwrap();
        let restored: ManagedItem = serde_json::from_str(&text).unwrap();
        assert_eq!(restored, item);
    }

    #[test]
    fn historical_items_without_details_are_still_readable() {
        let item = ManagedItem::new(
            "P-OLD",
            ItemKind::Principle,
            ItemScope::Project,
            "Old policy",
        )
        .unwrap();
        let text = serde_json::to_string(&item).unwrap();
        assert!(!text.contains("details"));
        let restored: ManagedItem = serde_json::from_str(&text).unwrap();
        assert_eq!(restored, item);
    }

    #[test]
    fn typed_activation_rejects_empty_tags() {
        let result = ManagedItem::new_typed(
            "P-1",
            ItemKind::Principle,
            ItemScope::User,
            "Activate on task tags",
            ItemDetails::Principle {
                strength: PrincipleStrength::Should,
                activation: Activation::OnTags { any: Vec::new() },
                verification: Vec::new(),
            },
        );
        assert_eq!(result, Err(ItemError::InvalidDetails));
    }
}
