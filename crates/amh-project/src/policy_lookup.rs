//! Read-only policy catalogue and validated delivery for the current agent.
use crate::{
    digest, render::render_policies, Home, Lifecycle, Policy, Project, Result, Scope, Strength,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Serialize)]
pub struct Candidate {
    pub id: String,
    scope: Scope,
    condition: String,
    strength: Strength,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Discovery {
    NotRegistered,
    Ready {
        catalog_digest: String,
        candidates: Vec<Candidate>,
    },
    Resolved {
        #[serde(flatten)]
        policies: PolicyDelivery,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub catalog_digest: String,
    #[serde(default)]
    pub decisions: Vec<Decision>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub id: String,
    pub applicability: Applicability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Applicability {
    Match,
    NoMatch,
    Uncertain,
}

#[derive(Debug, Serialize)]
pub struct PolicyDelivery {
    pub selected: Vec<String>,
    pub instructions: String,
}

pub(crate) fn eligible(policy: &Policy) -> bool {
    policy.lifecycle == Lifecycle::Active
}

fn snapshot(home: &Home, binding: &Project) -> Result<(Vec<Policy>, String)> {
    let mut entries = home.policies(binding)?;
    entries.retain(eligible);
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    for pair in entries.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(format!(
                "active policy {} exists in both user and project {} scopes",
                pair[0].id, binding.project
            ));
        }
    }
    let fingerprint = digest(&serde_json::to_vec(&(binding, &entries)).map_err(|e| e.to_string())?);
    Ok((entries, fingerprint))
}

pub fn discover_policies(home: &Home, directory: &Path) -> Result<Discovery> {
    let Some(binding) = home.project(directory)? else {
        return Ok(Discovery::NotRegistered);
    };
    let (entries, catalog_digest) = snapshot(home, &binding)?;
    let candidates = entries
        .iter()
        .filter_map(|p| {
            Some(Candidate {
                id: p.id.clone(),
                scope: p.scope,
                condition: p.applicability.clone()?,
                strength: p.strength,
            })
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(Discovery::Resolved {
            policies: deliver(&entries, &[])?,
        });
    }
    Ok(Discovery::Ready {
        catalog_digest,
        candidates,
    })
}

fn current_catalog(home: &Home, directory: &Path, expected: &str) -> Result<Vec<Policy>> {
    let (entries, current) = snapshot(home, &home.require_project(directory)?)?;
    if current != expected {
        return Err("stale policy catalogue; run policy discover again".into());
    }
    Ok(entries)
}

pub fn read_policy(
    home: &Home,
    directory: &Path,
    id: &str,
    catalog_digest: &str,
) -> Result<Policy> {
    current_catalog(home, directory, catalog_digest)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("unknown, inactive or out-of-scope policy: {id}"))
}

pub fn read_selection(
    home: &Home,
    directory: &Path,
    selection: Selection,
) -> Result<PolicyDelivery> {
    let entries = current_catalog(home, directory, &selection.catalog_digest)?;
    deliver(&entries, &selection.decisions)
}

fn deliver(entries: &[Policy], decisions: &[Decision]) -> Result<PolicyDelivery> {
    let mut remaining = BTreeMap::new();
    for decision in decisions {
        if remaining
            .insert(decision.id.as_str(), decision.applicability)
            .is_some()
        {
            return Err(format!("duplicate policy decision: {}", decision.id));
        }
    }
    let mut chosen = Vec::new();
    for policy in entries {
        let id = policy.id.as_str();
        if policy.applicability.is_none() {
            chosen.push(policy);
            continue;
        }
        let decision = remaining.remove(id);
        if policy.strength == Strength::Must {
            match decision {
                None => return Err(format!("missing decision for mandatory policy: {id}")),
                Some(Applicability::Uncertain) => {
                    return Err(format!("uncertain applicability of mandatory policy: {id}"))
                }
                _ => {}
            }
        }
        if decision == Some(Applicability::Match) {
            chosen.push(policy);
        }
    }
    if let Some(id) = remaining.keys().next() {
        return Err(format!(
            "unknown, inactive or unconditional policy decision: {id}"
        ));
    }
    let ids: BTreeSet<_> = chosen.iter().map(|p| p.id.as_str()).collect();
    for p in &chosen {
        for id in &p.relations.depends_on {
            if !ids.contains(id.as_str()) {
                return Err(format!(
                    "unresolved dependency: {} requires active applicable {id}",
                    p.id
                ));
            }
        }
        for id in &p.relations.conflicts_with {
            if ids.contains(id.as_str()) {
                return Err(format!("unresolved conflict: {} versus {id}", p.id));
            }
        }
    }
    chosen.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));
    let mut instructions = String::new();
    render_policies(&mut instructions, &chosen);
    Ok(PolicyDelivery {
        selected: chosen.into_iter().map(|p| p.id.clone()).collect(),
        instructions: instructions.trim_start().to_owned(),
    })
}
