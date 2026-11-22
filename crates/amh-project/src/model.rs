use crate::Result;
use amh_core::{ItemDetails, ItemKind, Lifecycle, ManagedItem};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Relations {
    pub depends_on: Vec<String>,
    pub supports: Vec<String>,
    pub conflicts_with: Vec<String>,
    pub applies_to: Vec<String>,
    pub verified_by: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub item: ManagedItem,
    pub owner: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub relations: Relations,
    /// Human-authored activation condition for semantic agent discovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applicability: Option<String>,
}

impl Policy {
    pub fn validate(&self) -> Result<()> {
        self.item
            .validate()
            .map_err(|e| format!("invalid item: {e:?}"))?;
        if self.owner.trim().is_empty() || self.item.details.is_none() {
            return Err("project policies require an owner and explicit typed details".into());
        }
        if let Some(condition) = &self.applicability {
            if self.item.kind != ItemKind::Principle || condition.trim().is_empty() {
                return Err(
                    "natural applicability requires a principle and a nonempty condition".into(),
                );
            }
        }
        for relation in [
            &self.relations.depends_on,
            &self.relations.supports,
            &self.relations.conflicts_with,
            &self.relations.applies_to,
            &self.relations.verified_by,
        ] {
            if relation.iter().any(|v| v.trim().is_empty()) {
                return Err("empty relation reference".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub id: String,
    pub version: String,
    pub policies: Vec<Policy>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum PolicyChange {
    Add {
        policy: Policy,
    },
    Modify {
        id: String,
        item_revision: u64,
        content: String,
    },
    SetDetails {
        id: String,
        item_revision: u64,
        details: ItemDetails,
    },
    SetMetadata {
        id: String,
        item_revision: u64,
        owner: String,
        source: Option<String>,
        relations: Relations,
    },
    SetApplicability {
        id: String,
        item_revision: u64,
        applicability: Option<String>,
    },
    Replace {
        id: String,
        item_revision: u64,
        replacement: Policy,
    },
    Remove {
        id: String,
        item_revision: u64,
    },
    Reprioritize {
        id: String,
        item_revision: u64,
        priority: i32,
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
    InstallPack {
        pack: Pack,
    },
    Restore {
        revision: u64,
    },
}

impl PolicyChange {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Add { .. } => "add",
            Self::Modify { .. } => "modify",
            Self::SetDetails { .. } => "set_details",
            Self::SetMetadata { .. } => "set_metadata",
            Self::SetApplicability { .. } => "set_applicability",
            Self::Replace { .. } => "replace",
            Self::Remove { .. } => "remove",
            Self::Reprioritize { .. } => "reprioritize",
            Self::Activate { .. } => "activate",
            Self::Pause { .. } => "pause",
            Self::Reactivate { .. } => "reactivate",
            Self::Retire { .. } => "retire",
            Self::InstallPack { .. } => "install_pack",
            Self::Restore { .. } => "restore",
        }
    }

    pub fn policy_ids(&self) -> Vec<&str> {
        match self {
            Self::Add { policy } => vec![&policy.item.id],
            Self::Replace {
                id, replacement, ..
            } => vec![id, &replacement.item.id],
            Self::Modify { id, .. }
            | Self::SetDetails { id, .. }
            | Self::SetMetadata { id, .. }
            | Self::SetApplicability { id, .. }
            | Self::Remove { id, .. }
            | Self::Reprioritize { id, .. }
            | Self::Activate { id, .. }
            | Self::Pause { id, .. }
            | Self::Reactivate { id, .. }
            | Self::Retire { id, .. } => vec![id],
            Self::InstallPack { pack } => {
                pack.policies.iter().map(|p| p.item.id.as_str()).collect()
            }
            Self::Restore { .. } => vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub revision: u64,
    pub reason: String,
    pub change: PolicyChange,
}

/// One authoritative TOML event log. Current state is derived, never kept as a
/// second editable source. History is local and is not an authenticated audit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub format: String,
    pub project_id: String,
    #[serde(default)]
    pub events: Vec<Event>,
}

impl Registry {
    pub fn new(project_id: String) -> Result<Self> {
        if project_id.trim().is_empty() {
            return Err("project ID is required".into());
        }
        Ok(Self {
            format: "amh.policy-registry.v1".into(),
            project_id,
            events: vec![],
        })
    }
    pub fn revision(&self) -> u64 {
        self.events.len() as u64
    }
    pub fn policies(&self) -> Result<Vec<Policy>> {
        self.at(self.revision())
    }
    pub fn at(&self, revision: u64) -> Result<Vec<Policy>> {
        if self.format != "amh.policy-registry.v1"
            || self.project_id.trim().is_empty()
            || revision > self.revision()
        {
            return Err("unsupported or invalid project registry".into());
        }
        let mut versions: Vec<BTreeMap<String, Policy>> = vec![BTreeMap::new()];
        let mut seen = BTreeSet::new();
        for (index, event) in self.events.iter().take(revision as usize).enumerate() {
            if event.revision != index as u64 + 1 || event.reason.trim().is_empty() {
                return Err("invalid event sequence or missing reason".into());
            }
            let mut next = versions.last().unwrap().clone();
            if let PolicyChange::Restore { revision } = &event.change {
                let previous = versions
                    .get(*revision as usize)
                    .ok_or("restore must refer to an earlier revision")?;
                // A restored item retains its identity but gets a new item revision.
                next = previous.clone();
                for policy in next.values_mut() {
                    let highest = versions
                        .iter()
                        .filter_map(|v| v.get(&policy.item.id))
                        .map(|p| p.item.revision)
                        .max()
                        .unwrap_or(0);
                    policy.item.revision =
                        highest.checked_add(1).ok_or("item revision exhausted")?;
                }
            } else {
                reduce(&mut next, &mut seen, &event.change)?;
            }
            for policy in next.values() {
                policy.validate()?;
                if policy.item.scope == amh_core::ItemScope::Task
                    && policy.relations.applies_to.is_empty()
                {
                    return Err("persisted task policy lacks an applies_to reference".into());
                }
            }
            versions.push(next);
        }
        Ok(versions.pop().unwrap().into_values().collect())
    }
    pub fn changed(&self, expected: u64, reason: String, change: PolicyChange) -> Result<Self> {
        if expected != self.revision() {
            return Err(format!(
                "stale registry revision: expected {expected}, actual {}",
                self.revision()
            ));
        }
        if reason.trim().is_empty() {
            return Err("a change reason is required".into());
        }
        self.policies()?;
        let mut next = self.clone();
        next.events.push(Event {
            revision: expected.checked_add(1).ok_or("revision exhausted")?,
            reason,
            change,
        });
        next.policies()?;
        Ok(next)
    }
}

fn add(
    items: &mut BTreeMap<String, Policy>,
    seen: &mut BTreeSet<String>,
    policy: &Policy,
) -> Result<()> {
    policy.validate()?;
    if policy.item.scope == amh_core::ItemScope::Task && policy.relations.applies_to.is_empty() {
        return Err("persisted task policies require explicit applies_to task IDs".into());
    }
    if policy.item.revision != 1
        || !matches!(policy.item.lifecycle, Lifecycle::Draft | Lifecycle::Active)
        || policy.item.replaces.is_some()
        || !seen.insert(policy.item.id.clone())
    {
        return Err(
            "ADD requires a fresh, unique draft or active identity at item revision 1".into(),
        );
    }
    items.insert(policy.item.id.clone(), policy.clone());
    Ok(())
}
fn existing<'a>(
    items: &'a mut BTreeMap<String, Policy>,
    id: &str,
    revision: u64,
) -> Result<&'a mut Policy> {
    let p = items
        .get_mut(id)
        .ok_or_else(|| format!("unknown policy: {id}"))?;
    if p.item.revision != revision {
        return Err(format!("stale item revision for {id}"));
    }
    if matches!(p.item.lifecycle, Lifecycle::Retired | Lifecycle::Superseded) {
        return Err("terminal item cannot be modified".into());
    }
    p.item.revision = p
        .item
        .revision
        .checked_add(1)
        .ok_or("item revision exhausted")?;
    Ok(p)
}
fn status(
    items: &mut BTreeMap<String, Policy>,
    id: &str,
    revision: u64,
    from: Option<Lifecycle>,
    to: Lifecycle,
) -> Result<()> {
    let p = existing(items, id, revision)?;
    if from.is_some_and(|v| v != p.item.lifecycle) {
        return Err("invalid lifecycle transition".into());
    }
    p.item.lifecycle = to;
    Ok(())
}
fn reduce(
    items: &mut BTreeMap<String, Policy>,
    seen: &mut BTreeSet<String>,
    change: &PolicyChange,
) -> Result<()> {
    match change {
        PolicyChange::Add { policy } => add(items, seen, policy)?,
        PolicyChange::Modify {
            id,
            item_revision,
            content,
        } => existing(items, id, *item_revision)?.item.content = content.clone(),
        PolicyChange::SetDetails {
            id,
            item_revision,
            details,
        } => {
            let p = existing(items, id, *item_revision)?;
            if p.item.details.as_ref() == Some(details) {
                return Err("details are unchanged".into());
            }
            p.item.details = Some(details.clone());
        }
        PolicyChange::SetMetadata {
            id,
            item_revision,
            owner,
            source,
            relations,
        } => {
            let p = existing(items, id, *item_revision)?;
            p.owner = owner.clone();
            p.source = source.clone();
            p.relations = relations.clone();
        }
        PolicyChange::SetApplicability {
            id,
            item_revision,
            applicability,
        } => {
            existing(items, id, *item_revision)?.applicability = applicability.clone();
        }
        PolicyChange::Reprioritize {
            id,
            item_revision,
            priority,
        } => existing(items, id, *item_revision)?.priority = *priority,
        PolicyChange::Remove { id, item_revision } => {
            let p = items.get(id).ok_or("unknown item")?;
            if p.item.revision != *item_revision {
                return Err("stale item revision".into());
            }
            items.remove(id);
        }
        PolicyChange::Replace {
            id,
            item_revision,
            replacement,
        } => {
            let old = existing(items, id, *item_revision)?;
            if old.item.lifecycle != Lifecycle::Active
                || old.item.kind != replacement.item.kind
                || old.item.scope != replacement.item.scope
                || replacement.item.lifecycle != Lifecycle::Draft
            {
                return Err("REPLACE requires an active original and a new draft of compatible kind and scope".into());
            }
            old.item.lifecycle = Lifecycle::Superseded;
            add(items, seen, replacement)?;
            let new = items.get_mut(&replacement.item.id).unwrap();
            new.item.lifecycle = Lifecycle::Active;
            new.item.replaces = Some(id.clone());
        }
        PolicyChange::Activate { id, item_revision } => status(
            items,
            id,
            *item_revision,
            Some(Lifecycle::Draft),
            Lifecycle::Active,
        )?,
        PolicyChange::Pause { id, item_revision } => status(
            items,
            id,
            *item_revision,
            Some(Lifecycle::Active),
            Lifecycle::Paused,
        )?,
        PolicyChange::Reactivate { id, item_revision } => status(
            items,
            id,
            *item_revision,
            Some(Lifecycle::Paused),
            Lifecycle::Active,
        )?,
        PolicyChange::Retire { id, item_revision } => {
            status(items, id, *item_revision, None, Lifecycle::Retired)?
        }
        PolicyChange::InstallPack { pack } => {
            if pack.id.trim().is_empty()
                || pack.version.trim().is_empty()
                || pack.policies.is_empty()
            {
                return Err("pack requires ID, version and policies".into());
            }
            for entry in &pack.policies {
                let mut entry = entry.clone();
                if entry.item.lifecycle != Lifecycle::Draft {
                    return Err("pack policies must start as drafts".into());
                }
                entry.source = Some(format!("pack:{}@{}", pack.id, pack.version));
                add(items, seen, &entry)?;
                status(
                    items,
                    &entry.item.id,
                    1,
                    Some(Lifecycle::Draft),
                    Lifecycle::Active,
                )?;
            }
        }
        PolicyChange::Restore { .. } => unreachable!("handled by registry replay"),
    }
    Ok(())
}

pub fn builtin_pack(id: &str) -> Result<Pack> {
    let source = match id {
        "development" => include_str!("../../../packs/development.toml"),
        "inquiry" => include_str!("../../../packs/inquiry.toml"),
        _ => return Err("unknown pack; available: development, inquiry".into()),
    };
    toml::from_str(source).map_err(|e| e.to_string())
}

pub fn kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Goal => "goal",
        ItemKind::Principle => "principle",
        ItemKind::Requirement => "requirement",
        ItemKind::Constraint => "constraint",
        ItemKind::Decision => "decision",
    }
}
