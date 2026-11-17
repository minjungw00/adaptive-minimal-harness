use crate::{digest, kind_name, Policy, Result, Task};
use amh_core::{Activation, ItemDetails, Lifecycle, PrincipleStrength};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub id: String,
    pub applicability: String,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compilation {
    pub task_id: String,
    pub selected: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub source_digest: String,
    pub content: String,
    pub observed: String,
}

/// Deterministic, explicit matching. Tags and task references are supplied by
/// the caller; no semantic classification or model compliance is inferred.
pub fn resolve(policies: &[Policy], task: &Task, budget: Option<usize>) -> Result<Compilation> {
    task.validate()?;
    let mut all = BTreeMap::new();
    for policy in policies.iter().chain(task.items.iter()) {
        policy.validate()?;
        if all.insert(policy.item.id.clone(), policy).is_some() {
            return Err(format!(
                "ambiguous policy ID across scopes: {}",
                policy.item.id
            ));
        }
    }
    for id in &task.explicit_policies {
        let p = all
            .get(id)
            .ok_or_else(|| format!("uncertain applicability: explicit policy {id} is missing"))?;
        if p.item.lifecycle != Lifecycle::Active {
            return Err(format!("explicit policy {id} is not active"));
        }
    }
    let mut chosen = Vec::new();
    let mut diagnostics = Vec::new();
    for p in all.values() {
        let active = p.item.lifecycle == Lifecycle::Active;
        let scope_match =
            p.relations.applies_to.is_empty() || p.relations.applies_to.contains(&task.id);
        let activation = match &p.item.details {
            Some(ItemDetails::Principle { activation, .. }) => match activation {
                Activation::Always => true,
                Activation::ExplicitOnly => task.explicit_policies.contains(&p.item.id),
                Activation::OnTags { any } => any.iter().any(|tag| task.tags.contains(tag)),
            },
            _ => true,
        };
        let selected = active && scope_match && activation;
        let uncertain = active
            && scope_match
            && !activation
            && !task.classification_complete
            && matches!(
                &p.item.details,
                Some(ItemDetails::Principle {
                    activation: Activation::OnTags { .. },
                    ..
                })
            );
        if uncertain
            && matches!(
                &p.item.details,
                Some(ItemDetails::Principle {
                    strength: PrincipleStrength::Must,
                    ..
                })
            )
        {
            return Err(format!("uncertain applicability for mandatory policy {}; classify the task explicitly before compiling", p.item.id));
        }
        if task.explicit_policies.contains(&p.item.id) && !selected {
            return Err(format!(
                "explicit policy {} conflicts with its applicability conditions",
                p.item.id
            ));
        }
        diagnostics.push(Diagnostic {
            id: p.item.id.clone(),
            applicability: if uncertain {
                "uncertain"
            } else if selected {
                "match"
            } else {
                "no_match"
            }
            .into(),
            reason: if uncertain {
                "task classification is incomplete"
            } else if !active {
                "inactive lifecycle"
            } else if !scope_match {
                "different task scope"
            } else if !activation {
                "explicit activation conditions do not match"
            } else {
                "active and applicable"
            }
            .into(),
        });
        if selected {
            chosen.push(*p);
        }
    }
    let ids: BTreeSet<_> = chosen.iter().map(|p| p.item.id.as_str()).collect();
    for p in &chosen {
        for id in &p.relations.depends_on {
            if !ids.contains(id.as_str()) {
                return Err(format!(
                    "unresolved dependency: {} requires active applicable {id}",
                    p.item.id
                ));
            }
        }
        for id in &p.relations.conflicts_with {
            if ids.contains(id.as_str()) {
                return Err(format!("unresolved conflict: {} versus {id}; priority does not silently override a condition", p.item.id));
            }
        }
    }
    chosen.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.item.id.cmp(&b.item.id)));
    let selected = chosen.iter().map(|p| p.item.id.clone()).collect();
    let mut content = format!("# AMH task: {}\n\nGoal: {}\n", task.id, task.goal);
    render_list(&mut content, "Done when", &task.done_when);
    render_list(
        &mut content,
        "Allowed scope (requires external enforcement)",
        &task.allowed_paths,
    );
    render_list(&mut content, "Stop when", &task.stop_when);
    render_list(&mut content, "Outputs", &task.outputs);
    for p in chosen {
        content.push_str(&format!(
            "\n## {} · {} · revision {}\n\n{}\n\n",
            p.item.id,
            kind_name(p.item.kind),
            p.item.revision,
            p.item.content
        ));
        content.push_str(&format!(
            "Source: owner={}; scope={:?}",
            p.owner, p.item.scope
        ));
        if let Some(source) = &p.source {
            content.push_str(&format!("; provenance={source}"));
        }
        content.push('\n');
        render_list(&mut content, "Depends on", &p.relations.depends_on);
        render_list(&mut content, "Supports", &p.relations.supports);
        render_list(&mut content, "Conflicts with", &p.relations.conflicts_with);
        render_list(&mut content, "Verified by", &p.relations.verified_by);
        match p.item.details.as_ref().unwrap() {
            ItemDetails::Goal { success_criteria } => {
                render_list(&mut content, "Success criteria", success_criteria)
            }
            ItemDetails::Principle {
                strength,
                verification,
                ..
            } => {
                content.push_str(&format!("Strength: {strength:?}\n"));
                render_list(&mut content, "Verification", verification);
            }
            ItemDetails::Requirement {
                role,
                acceptance_criteria,
            } => {
                content.push_str(&format!("Requirement role: {role:?}\n"));
                render_list(&mut content, "Acceptance criteria", acceptance_criteria);
            }
            ItemDetails::Constraint {
                enforcement,
                checks,
            } => {
                content.push_str(&format!(
                    "Enforcement: {enforcement:?} (declaration only)\n"
                ));
                render_list(&mut content, "Checks", checks);
            }
            ItemDetails::Decision { rationale } => {
                content.push_str(&format!("Rationale: {rationale}\n"))
            }
        }
    }
    if !task.checks.is_empty() {
        content.push_str("\n## Verification commands\n\n");
        for check in &task.checks {
            content.push_str(&format!("- {}: {:?}\n", check.id, check.argv));
        }
    }
    content.push_str(
        "\nReport only checks actually executed. Policy delivery is not evidence of compliance.\n",
    );
    if budget.is_some_and(|b| content.chars().count() > b) {
        return Err(
            "context exceeds character budget; no applicable condition was silently discarded"
                .into(),
        );
    }
    let source_digest = digest(&serde_json::to_vec(&(policies, task)).map_err(|e| e.to_string())?);
    Ok(Compilation {
        task_id: task.id.clone(),
        selected,
        diagnostics,
        source_digest,
        content,
        observed: "unknown".into(),
    })
}

fn render_list(output: &mut String, title: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    output.push_str(&format!("\n{title}:\n"));
    for value in values {
        output.push_str(&format!("- {value}\n"));
    }
}
