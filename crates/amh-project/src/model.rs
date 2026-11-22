use crate::{digest, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    User,
    Project,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    #[default]
    Draft,
    Active,
    Paused,
    Superseded,
    Retired,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    #[default]
    Must,
    Should,
    May,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Relations {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts_with: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub id: String,
    pub scope: Scope,
    pub content: String,
    #[serde(default)]
    pub strength: Strength,
    #[serde(default = "first_revision")]
    pub revision: u64,
    #[serde(default)]
    pub lifecycle: Lifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub relations: Relations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applicability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
}

fn first_revision() -> u64 {
    1
}

impl Policy {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() || self.content.trim().is_empty() || self.revision == 0 {
            return Err("policy requires an ID, nonempty content, and a positive revision".into());
        }
        if self.replaces.as_deref() == Some(self.id.as_str()) {
            return Err("a policy cannot replace itself".into());
        }
        if self
            .applicability
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err("policy applicability must be nonempty when provided".into());
        }
        if self
            .relations
            .depends_on
            .iter()
            .chain(&self.relations.conflicts_with)
            .any(|id| id.trim().is_empty())
        {
            return Err("empty policy relation reference".into());
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
    SetStrength {
        id: String,
        item_revision: u64,
        strength: Strength,
    },
    SetMetadata {
        id: String,
        item_revision: u64,
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
            Self::SetStrength { .. } => "set_strength",
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
            Self::Add { policy } => vec![&policy.id],
            Self::Replace {
                id, replacement, ..
            } => vec![id, &replacement.id],
            Self::Modify { id, .. }
            | Self::SetStrength { id, .. }
            | Self::SetMetadata { id, .. }
            | Self::SetApplicability { id, .. }
            | Self::Remove { id, .. }
            | Self::Reprioritize { id, .. }
            | Self::Activate { id, .. }
            | Self::Pause { id, .. }
            | Self::Reactivate { id, .. }
            | Self::Retire { id, .. } => vec![id],
            Self::InstallPack { pack } => pack.policies.iter().map(|p| p.id.as_str()).collect(),
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

const REGISTRY_FORMAT: &str = "amh.policy-registry.v1";

#[derive(Clone, Default)]
struct ReplayState<'a> {
    policies: BTreeMap<String, Policy>,
    packs: BTreeMap<&'a str, &'a Pack>,
}

impl ReplayState<'_> {
    fn pack_locks(&self) -> Result<BTreeMap<String, String>> {
        self.packs
            .iter()
            .map(|(&id, pack)| {
                let bytes = serde_json::to_vec(pack).map_err(|e| e.to_string())?;
                Ok((
                    id.to_owned(),
                    format!("{}:{}", pack.version, digest(&bytes)),
                ))
            })
            .collect()
    }
}

impl Registry {
    pub fn new(project_id: String) -> Result<Self> {
        if project_id.trim().is_empty() {
            return Err("project ID is required".into());
        }
        Ok(Self {
            format: REGISTRY_FORMAT.into(),
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
        Ok(self.replay(revision)?.policies.into_values().collect())
    }

    pub fn policies_and_pack_locks(&self) -> Result<(Vec<Policy>, BTreeMap<String, String>)> {
        let state = self.replay(self.revision())?;
        let locks = state.pack_locks()?;
        Ok((state.policies.into_values().collect(), locks))
    }

    fn replay(&self, revision: u64) -> Result<ReplayState<'_>> {
        if self.format != REGISTRY_FORMAT
            || self.project_id.trim().is_empty()
            || revision > self.revision()
        {
            return Err("unsupported or invalid project registry".into());
        }
        let events = &self.events[..revision as usize];
        // Only revisions referenced by a restore need a snapshot. Keep item
        // revision counters outside snapshots so removals and restores never rewind them.
        let restore_points: BTreeSet<_> = events
            .iter()
            .filter_map(|event| match event.change {
                PolicyChange::Restore { revision } => Some(revision),
                _ => None,
            })
            .collect();
        let mut state = ReplayState::default();
        let mut snapshots = BTreeMap::new();
        if restore_points.contains(&0) {
            snapshots.insert(0, state.clone());
        }
        let mut seen = BTreeSet::new();
        let mut highest = BTreeMap::<String, u64>::new();
        for (index, event) in events.iter().enumerate() {
            if event.revision != index as u64 + 1 || event.reason.trim().is_empty() {
                return Err("invalid event sequence or missing reason".into());
            }
            if let PolicyChange::Restore { revision } = &event.change {
                state = snapshots
                    .get(revision)
                    .ok_or("restore must refer to an earlier revision")?
                    .clone();
                for (id, policy) in &mut state.policies {
                    let revision = highest.get_mut(id).ok_or("unknown restored item")?;
                    *revision = revision.checked_add(1).ok_or("item revision exhausted")?;
                    policy.revision = *revision;
                }
            } else {
                reduce(&mut state.policies, &mut seen, &event.change)?;
                for id in event.change.policy_ids() {
                    if let Some(policy) = state.policies.get(id) {
                        policy.validate()?;
                        highest.insert(policy.id.clone(), policy.revision);
                    }
                }
                if let PolicyChange::InstallPack { pack } = &event.change {
                    state.packs.insert(&pack.id, pack);
                }
            }
            if restore_points.contains(&event.revision) {
                snapshots.insert(event.revision, state.clone());
            }
        }
        Ok(state)
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

pub fn pack_locks(registry: &Registry) -> Result<BTreeMap<String, String>> {
    registry.replay(registry.revision())?.pack_locks()
}

fn add(
    items: &mut BTreeMap<String, Policy>,
    seen: &mut BTreeSet<String>,
    policy: &Policy,
) -> Result<()> {
    policy.validate()?;
    if policy.revision != 1
        || !matches!(policy.lifecycle, Lifecycle::Draft | Lifecycle::Active)
        || policy.replaces.is_some()
        || !seen.insert(policy.id.clone())
    {
        return Err(
            "ADD requires a fresh, unique draft or active identity at item revision 1".into(),
        );
    }
    items.insert(policy.id.clone(), policy.clone());
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
    if p.revision != revision {
        return Err(format!("stale item revision for {id}"));
    }
    if matches!(p.lifecycle, Lifecycle::Retired | Lifecycle::Superseded) {
        return Err("terminal item cannot be modified".into());
    }
    p.revision = p.revision.checked_add(1).ok_or("item revision exhausted")?;
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
    if from.is_some_and(|v| v != p.lifecycle) {
        return Err("invalid lifecycle transition".into());
    }
    p.lifecycle = to;
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
        } => existing(items, id, *item_revision)?.content = content.clone(),
        PolicyChange::SetStrength {
            id,
            item_revision,
            strength,
        } => existing(items, id, *item_revision)?.strength = *strength,
        PolicyChange::SetMetadata {
            id,
            item_revision,
            source,
            relations,
        } => {
            let p = existing(items, id, *item_revision)?;
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
            if p.revision != *item_revision {
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
            if old.lifecycle != Lifecycle::Active
                || old.scope != replacement.scope
                || replacement.lifecycle != Lifecycle::Draft
            {
                return Err(
                    "REPLACE requires an active original and a new draft in the same scope".into(),
                );
            }
            old.lifecycle = Lifecycle::Superseded;
            add(items, seen, replacement)?;
            let new = items.get_mut(&replacement.id).unwrap();
            new.lifecycle = Lifecycle::Active;
            new.replaces = Some(id.clone());
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
                if entry.lifecycle != Lifecycle::Draft {
                    return Err("pack policies must start as drafts".into());
                }
                entry
                    .source
                    .get_or_insert_with(|| format!("pack:{}@{}", pack.id, pack.version));
                add(items, seen, &entry)?;
                status(
                    items,
                    &entry.id,
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

pub const BUILTIN_PACK_ID: &str = "agent-workflow";

pub fn builtin_pack(id: &str) -> Result<Pack> {
    if id != BUILTIN_PACK_ID {
        return Err(format!("unknown pack; available: {BUILTIN_PACK_ID}"));
    }
    toml::from_str(include_str!("../../../packs/agent-workflow.toml")).map_err(|e| e.to_string())
}
