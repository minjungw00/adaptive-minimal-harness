use crate::Policy;
use std::fmt::Write;

pub(crate) fn render_policies(chosen: &[&Policy]) -> String {
    let mut content = String::new();
    for p in chosen {
        let _ = writeln!(
            content,
            "## {} · revision {}\n\n{}\n",
            p.id, p.revision, p.content
        );
        let _ = writeln!(content, "Scope: {:?} · Strength: {:?}", p.scope, p.strength);
        if let Some(source) = &p.source {
            let _ = writeln!(content, "Source: {source}");
        }
        for (label, ids) in [
            ("Depends on", &p.relations.depends_on),
            ("Conflicts with", &p.relations.conflicts_with),
        ] {
            if !ids.is_empty() {
                let _ = writeln!(content, "{label}: {}", ids.join(", "));
            }
        }
        content.push('\n');
    }
    content
}
