use amh_core::{Activation, ItemDetails, ItemKind, ItemScope, ManagedItem, PrincipleStrength};
use amh_project::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    root: PathBuf,
    home: Home,
    binding: Project,
}
impl Fixture {
    fn empty() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "amh-home-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let root = dir.join("repo");
        fs::create_dir_all(&root).unwrap();
        assert!(std::process::Command::new("git")
            .current_dir(&root)
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success());
        let root = git_root(&root).unwrap().unwrap();
        let home = Home::new(&dir.join(".amh")).unwrap();
        let binding = Project {
            root: root.clone(),
            project: "test".into(),
        };
        Self {
            dir,
            root,
            home,
            binding,
        }
    }
    fn new() -> Self {
        let f = Self::empty();
        f.apply(Request::Register {
            binding: f.binding.clone(),
            new_project: false,
        });
        f
    }
    fn apply(&self, request: Request) {
        self.home.apply(&self.home.plan(request).unwrap()).unwrap();
    }
    fn target(&self) -> Target {
        Target::Project {
            binding: self.binding.clone(),
        }
    }
    fn registry(&self) -> Registry {
        self.home.registry(&self.target()).unwrap()
    }
    fn propose(&self, expected_revision: u64, change: PolicyChange) -> Request {
        Request::Change {
            target: self.target(),
            expected_revision,
            reason: "Fixture change".into(),
            change: Box::new(change),
        }
    }
    fn change(&self, change: PolicyChange) {
        self.apply(self.propose(self.registry().revision(), change));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn principle(id: &str, tag: &str) -> Policy {
    Policy {
        item: ManagedItem::new_typed(
            id,
            ItemKind::Principle,
            ItemScope::Project,
            "Use this method only when relevant",
            ItemDetails::Principle {
                strength: PrincipleStrength::Must,
                activation: Activation::OnTags {
                    any: vec![tag.into()],
                },
                verification: vec!["Provide execution evidence".into()],
            },
        )
        .unwrap(),
        owner: "user".into(),
        source: None,
        priority: 0,
        relations: Relations::default(),
        applicability: None,
    }
}
fn read_matches(home: &Home, root: &std::path::Path, matches: &[&str]) -> PolicyDelivery {
    match discover_policies(home, root).unwrap() {
        Discovery::Resolved { policies } => policies,
        Discovery::Ready {
            catalog_digest,
            candidates,
        } => {
            let decisions = candidates
                .into_iter()
                .map(|candidate| Decision {
                    applicability: if matches.contains(&candidate.id.as_str()) {
                        Applicability::Match
                    } else {
                        Applicability::NoMatch
                    },
                    id: candidate.id,
                })
                .collect();
            read_selection(
                home,
                root,
                Selection {
                    catalog_digest,
                    decisions,
                },
            )
            .unwrap()
        }
        Discovery::NotRegistered => panic!("expected registered work tree"),
    }
}

#[test]
fn registration_preview_and_removal_keep_the_repository_and_policies_intact() {
    let f = Fixture::empty();
    let agents = f.root.join("AGENTS.md");
    let original = b"# Team policy\r\nKeep this verbatim.\r\n";
    fs::write(&agents, original).unwrap();
    let register = Request::Register {
        binding: f.binding.clone(),
        new_project: false,
    };
    let plan = f.home.plan(register.clone()).unwrap();
    assert!(!f.home.path().exists());
    f.home.apply(&plan).unwrap();
    assert!(f.home.plan(register.clone()).unwrap().files.is_empty());
    f.change(PolicyChange::Add {
        policy: principle("P-KEEP", "code"),
    });
    f.apply(Request::Unregister {
        binding: f.binding.clone(),
    });
    assert!(f.home.project(&f.root).unwrap().is_none());
    assert!(f.home.path().join("projects/test/policies.toml").is_file());
    f.apply(register);
    assert_eq!(f.registry().revision(), 1);
    assert_eq!(fs::read(&agents).unwrap(), original);
    assert!(!f.root.join(".amh").exists());
}

#[test]
fn stale_or_edited_plans_are_rejected() {
    let f = Fixture::new();
    let add = |id| PolicyChange::Add {
        policy: principle(id, "code"),
    };
    let pending = f.home.plan(f.propose(0, add("P-1"))).unwrap();
    f.change(add("P-2"));
    assert!(f.home.apply(&pending).is_err());
    let mut tampered = f.home.plan(f.propose(1, add("P-3"))).unwrap();
    tampered
        .files
        .push(f.root.join("AGENTS.md").display().to_string());
    assert!(f.home.apply(&tampered).is_err());
    assert!(!f.root.join("AGENTS.md").exists());
    assert_eq!(f.registry().revision(), 1);
}

#[test]
fn rollback_creates_a_new_revision() {
    let f = Fixture::new();
    f.change(PolicyChange::Add {
        policy: principle("P-1", "code"),
    });
    f.change(PolicyChange::Restore { revision: 0 });
    assert_eq!(f.registry().revision(), 2);
    assert!(f.registry().policies().unwrap().is_empty());
}

#[test]
fn packs_are_pinned_and_socratic_tdd_refactor_are_conditionally_selected() {
    let f = Fixture::new();
    f.change(PolicyChange::InstallPack {
        pack: builtin_pack("development").unwrap(),
    });
    f.change(PolicyChange::InstallPack {
        pack: builtin_pack("inquiry").unwrap(),
    });
    assert_eq!(pack_locks(&f.registry()).unwrap().len(), 2);
    for id in ["P-WRITING", "P-SOCRATIC", "P-TDD", "P-REFACTOR"] {
        assert_eq!(read_matches(&f.home, &f.root, &[id]).selected, [id]);
    }
    assert!(read_matches(&f.home, &f.root, &[]).selected.is_empty());
    let p = f.home.plan(Request::Change {
        target: f.target(),
        expected_revision: 2,
        reason: "Reinstall".into(),
        change: Box::new(PolicyChange::InstallPack {
            pack: builtin_pack("development").unwrap(),
        }),
    });
    assert!(p.is_err()); // Reinstall cannot overwrite user-customized pack policies.
}

#[test]
fn adding_and_replacing_requirements_preserves_the_other_accepted_roles() {
    use amh_core::RequirementRole;
    fn requirement(id: &str, role: RequirementRole) -> Policy {
        let mut p = principle(id, "unused");
        p.item.kind = ItemKind::Requirement;
        p.item.content = format!("Accepted requirement {id}");
        p.item.details = Some(ItemDetails::Requirement {
            role,
            acceptance_criteria: vec![format!("Cover {id}")],
        });
        p
    }
    let f = Fixture::new();
    f.change(PolicyChange::Add {
        policy: requirement("R-COST", RequirementRole::Core),
    });
    f.change(PolicyChange::Activate {
        id: "R-COST".into(),
        item_revision: 1,
    });
    let original = f.registry().policies().unwrap()[0].clone();
    f.change(PolicyChange::Add {
        policy: requirement("R-RETENTION", RequirementRole::Support),
    });
    f.change(PolicyChange::Activate {
        id: "R-RETENTION".into(),
        item_revision: 1,
    });
    assert_eq!(f.registry().policies().unwrap()[0], original);
    let c = read_matches(&f.home, &f.root, &[]);
    assert!(
        c.instructions.contains("Requirement role: Core")
            && c.instructions.contains("Requirement role: Support")
    );
    f.change(PolicyChange::Replace {
        id: "R-COST".into(),
        item_revision: 2,
        replacement: requirement("R-COST-NEW", RequirementRole::Core),
    });
    let c = read_matches(&f.home, &f.root, &[]);
    assert_eq!(c.selected, vec!["R-COST-NEW", "R-RETENTION"]);
}

#[test]
fn explicit_priority_is_not_recency_and_removal_does_not_reuse_identity() {
    let f = Fixture::new();
    for id in ["P-A", "P-Z"] {
        f.change(PolicyChange::Add {
            policy: principle(id, "code"),
        });
        f.change(PolicyChange::Activate {
            id: id.into(),
            item_revision: 1,
        });
    }
    assert_eq!(
        read_matches(&f.home, &f.root, &["P-A", "P-Z"]).selected,
        vec!["P-A", "P-Z"]
    );
    f.change(PolicyChange::Reprioritize {
        id: "P-Z".into(),
        item_revision: 2,
        priority: 5,
    });
    assert_eq!(
        read_matches(&f.home, &f.root, &["P-A", "P-Z"]).selected,
        vec!["P-Z", "P-A"]
    );
    f.change(PolicyChange::Remove {
        id: "P-A".into(),
        item_revision: 2,
    });
    assert!(f
        .home
        .plan(Request::Change {
            target: f.target(),
            expected_revision: 6,
            reason: "Reuse".into(),
            change: Box::new(PolicyChange::Add {
                policy: principle("P-A", "code")
            }),
        })
        .is_err());
}

#[test]
fn user_policies_are_reused_without_cross_project_writes() {
    let a = Fixture::new();
    let b = Fixture::empty();
    a.apply(Request::Register {
        binding: Project {
            root: b.root.clone(),
            project: "other".into(),
        },
        new_project: false,
    });
    let before = fs::read(a.home.path().join("projects/other/policies.toml")).unwrap();
    let mut rule = principle("P-USER", "code");
    rule.item.scope = ItemScope::User;
    for (expected_revision, change) in [
        (0, PolicyChange::Add { policy: rule }),
        (
            1,
            PolicyChange::Activate {
                id: "P-USER".into(),
                item_revision: 1,
            },
        ),
    ] {
        a.apply(Request::Change {
            target: Target::User,
            expected_revision,
            reason: "Common policy".into(),
            change: Box::new(change),
        });
    }
    for root in [&a.root, &b.root] {
        assert_eq!(
            read_matches(&a.home, root, &["P-USER"]).selected,
            ["P-USER"]
        );
    }
    a.change(PolicyChange::Add {
        policy: principle("P-LOCAL", "code"),
    });
    assert_eq!(
        fs::read(a.home.path().join("projects/other/policies.toml")).unwrap(),
        before
    );
}

#[cfg(unix)]
#[test]
fn symlinked_registry_paths_and_homes_inside_git_are_refused() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let escape = f.root.join("redirected");
    fs::create_dir(&escape).unwrap();
    symlink(&escape, f.home.path().join("user")).unwrap();
    assert!(f
        .home
        .registry(&Target::User)
        .unwrap_err()
        .contains("symlink"));
    for path in [f.root.join(".amh"), f.root.join(".git/private")] {
        let nested = Home::new(&path).unwrap();
        assert!(nested
            .plan(f.propose(
                0,
                PolicyChange::Add {
                    policy: principle("P-1", "code")
                }
            ))
            .is_err());
        assert!(!path.exists());
    }
    assert_eq!(fs::read_dir(escape).unwrap().count(), 0);
}

#[test]
fn metadata_and_lifecycle_changes_are_versioned_and_invalid_updates_leave_source_intact() {
    let f = Fixture::new();
    f.change(PolicyChange::Add {
        policy: principle("P-1", "code"),
    });
    let before = fs::read(f.home.path().join("projects/test/policies.toml")).unwrap();
    assert!(f
        .home
        .plan(Request::Change {
            target: f.target(),
            expected_revision: 1,
            reason: "Invalid update".into(),
            change: Box::new(PolicyChange::Modify {
                id: "P-1".into(),
                item_revision: 1,
                content: " ".into()
            }),
        })
        .is_err());
    assert_eq!(
        fs::read(f.home.path().join("projects/test/policies.toml")).unwrap(),
        before
    );
    f.change(PolicyChange::SetMetadata {
        id: "P-1".into(),
        item_revision: 1,
        owner: "project-user".into(),
        source: Some("Reviewed source".into()),
        relations: Relations::default(),
    });
    assert_eq!(f.registry().policies().unwrap()[0].owner, "project-user");
    f.change(PolicyChange::Activate {
        id: "P-1".into(),
        item_revision: 2,
    });
    f.change(PolicyChange::Pause {
        id: "P-1".into(),
        item_revision: 3,
    });
    f.change(PolicyChange::Reactivate {
        id: "P-1".into(),
        item_revision: 4,
    });
    f.change(PolicyChange::Retire {
        id: "P-1".into(),
        item_revision: 5,
    });
    assert!(read_matches(&f.home, &f.root, &[]).selected.is_empty());
    assert!(f
        .home
        .plan(Request::Change {
            target: f.target(),
            expected_revision: 6,
            reason: "Revive terminal policy".into(),
            change: Box::new(PolicyChange::Reactivate {
                id: "P-1".into(),
                item_revision: 6
            }),
        })
        .is_err());
}
