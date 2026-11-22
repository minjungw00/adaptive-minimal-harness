use crate::{kind_name, Policy};
use amh_core::ItemDetails;

fn render_list(output: &mut String, title: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    output.push_str(&format!("\n{title}:\n"));
    for value in values {
        output.push_str(&format!("- {value}\n"));
    }
}

pub(crate) fn render_policies(content: &mut String, chosen: &[&Policy]) {
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
        render_list(content, "Depends on", &p.relations.depends_on);
        render_list(content, "Supports", &p.relations.supports);
        render_list(content, "Conflicts with", &p.relations.conflicts_with);
        render_list(content, "Verified by", &p.relations.verified_by);
        match p.item.details.as_ref().unwrap() {
            ItemDetails::Goal { success_criteria } => {
                render_list(content, "Success criteria", success_criteria)
            }
            ItemDetails::Principle {
                strength,
                verification,
                ..
            } => {
                content.push_str(&format!("Strength: {strength:?}\n"));
                render_list(content, "Verification", verification);
            }
            ItemDetails::Requirement {
                role,
                acceptance_criteria,
            } => {
                content.push_str(&format!("Requirement role: {role:?}\n"));
                render_list(content, "Acceptance criteria", acceptance_criteria);
            }
            ItemDetails::Constraint {
                enforcement,
                checks,
            } => {
                content.push_str(&format!(
                    "Enforcement: {enforcement:?} (declaration only)\n"
                ));
                render_list(content, "Checks", checks);
            }
            ItemDetails::Decision { rationale } => {
                content.push_str(&format!("Rationale: {rationale}\n"))
            }
        }
    }
}
