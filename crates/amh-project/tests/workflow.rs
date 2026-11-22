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
    fn compile(&self, task: &Task, budget: Option<usize>) -> Result<Compilation> {
        compile(&self.home, &self.binding, task, budget)
    }
    fn verify(&self, task: &Task, execute: bool) -> Evidence {
        let source = self.dir.join("task.json");
        fs::write(&source, serde_json::to_vec(task).unwrap()).unwrap();
        verify(&self.home, &self.binding, &source, execute).unwrap()
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
fn task(tag: &str) -> Task {
    Task {
        id: "T-1".into(),
        goal: "Change behavior".into(),
        tags: vec![tag.into()],
        classification_complete: true,
        done_when: vec!["Requested behavior works".into()],
        ..Task::default()
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
fn policy_pause_changes_the_read_only_compilation() {
    let f = Fixture::new();
    f.change(PolicyChange::Add {
        policy: principle("P-TDD", "code"),
    });
    f.change(PolicyChange::Activate {
        id: "P-TDD".into(),
        item_revision: 1,
    });
    assert!(f
        .compile(&task("writing"), None)
        .unwrap()
        .selected
        .is_empty());
    let compiled = f.compile(&task("code"), None).unwrap();
    assert_eq!(compiled.selected, ["P-TDD"]);
    assert!(f.compile(&task("code"), Some(1)).is_err());
    f.change(PolicyChange::Pause {
        id: "P-TDD".into(),
        item_revision: 2,
    });
    let paused = f.compile(&task("code"), None).unwrap();
    assert_ne!(paused.source_digest, compiled.source_digest);
    assert!(paused.selected.is_empty());
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
fn explicit_conflicts_and_dependencies_do_not_silently_drop_must_rules() {
    let mut a = principle("P-A", "code");
    a.item.lifecycle = amh_core::Lifecycle::Active;
    let mut b = principle("P-B", "code");
    b.item.lifecycle = amh_core::Lifecycle::Active;
    a.relations.conflicts_with.push("P-B".into());
    assert!(resolve(&[a.clone(), b], &task("code"), None).is_err());
    a.relations.conflicts_with.clear();
    a.relations.depends_on.push("P-MISSING".into());
    assert!(resolve(&[a], &task("code"), None).is_err());
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
fn unclassified_mandatory_rules_are_uncertain_instead_of_silently_excluded() {
    let mut p = principle("P-TDD", "code");
    p.item.lifecycle = amh_core::Lifecycle::Active;
    let mut t = task("writing");
    t.classification_complete = false;
    assert!(resolve(&[p.clone()], &t, None)
        .unwrap_err()
        .contains("uncertain"));
    t.classification_complete = true;
    assert!(resolve(&[p], &t, None).unwrap().selected.is_empty());
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
    let writing = f.compile(&task("document.revision"), None).unwrap();
    assert_eq!(writing.selected, vec!["P-WRITING"]);
    assert_eq!(
        f.compile(&task("inquiry.unclear"), None).unwrap().selected,
        vec!["P-SOCRATIC"]
    );
    assert_eq!(
        f.compile(&task("code.behavior_change"), None)
            .unwrap()
            .selected,
        vec!["P-TDD"]
    );
    assert_eq!(
        f.compile(&task("code.refactor"), None).unwrap().selected,
        vec!["P-REFACTOR"]
    );
    assert!(f
        .compile(&task("simple"), None)
        .unwrap()
        .selected
        .is_empty());
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
    let c = f.compile(&task("document.revision"), None).unwrap();
    assert!(
        c.content.contains("Requirement role: Core")
            && c.content.contains("Requirement role: Support")
    );
    f.change(PolicyChange::Replace {
        id: "R-COST".into(),
        item_revision: 2,
        replacement: requirement("R-COST-NEW", RequirementRole::Core),
    });
    let c = f.compile(&task("document.revision"), None).unwrap();
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
        f.compile(&task("code"), None).unwrap().selected,
        vec!["P-A", "P-Z"]
    );
    f.change(PolicyChange::Reprioritize {
        id: "P-Z".into(),
        item_revision: 2,
        priority: 5,
    });
    assert_eq!(
        f.compile(&task("code"), None).unwrap().selected,
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
    });
    let second = a.home.require_project(&b.root).unwrap();
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
    assert_eq!(a.compile(&task("code"), None).unwrap().selected, ["P-USER"]);
    assert_eq!(
        compile(&a.home, &second, &task("code"), None)
            .unwrap()
            .selected,
        ["P-USER"]
    );
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
fn inline_task_scope_is_preserved_and_scope_collisions_are_diagnostics() {
    let mut t = task("simple");
    let mut inline = principle("P-TASK", "simple");
    inline.item.scope = ItemScope::Task;
    inline.item.lifecycle = amh_core::Lifecycle::Active;
    t.items.push(inline.clone());
    assert_eq!(resolve(&[], &t, None).unwrap().selected, vec!["P-TASK"]);
    assert!(resolve(&[inline], &t, None)
        .unwrap_err()
        .contains("ambiguous"));
}

#[test]
fn verification_records_actual_results_under_home() {
    let f = Fixture::new();
    let mut t = task("simple");
    t.checks = vec![Check {
        id: "tool-version".into(),
        argv: vec!["rustc".into(), "--version".into()],
        timeout_seconds: 10,
        expected_exit_code: 0,
    }];
    let preview = f.verify(&t, false);
    assert_eq!(preview.all_checks_passed, None);
    assert_eq!(preview.checks[0].status, "not_run");
    let directory = f.home.path().join("projects/test/evidence");
    assert!(!directory.exists());
    let actual = f.verify(&t, true);
    assert_eq!(actual.all_checks_passed, Some(true));
    assert_eq!(actual.policy_compliance, "unknown");
    assert_eq!(actual.completion_criteria, "unknown");
    assert!(
        fs::read_to_string(actual.checks[0].stdout.as_ref().unwrap())
            .unwrap()
            .contains("rustc")
    );
    assert!(PathBuf::from(actual.file.unwrap()).starts_with(&directory));
    assert!(!f.root.join(".amh").exists());
}

#[test]
fn verification_missing_tools_are_failures_with_evidence_not_unexecuted_success() {
    let f = Fixture::new();
    let mut t = task("simple");
    t.checks.push(Check {
        id: "missing".into(),
        argv: vec!["amh-nonexistent-tool-for-regression".into()],
        timeout_seconds: 1,
        expected_exit_code: 0,
    });
    let evidence = f.verify(&t, true);
    assert_eq!(evidence.all_checks_passed, Some(false));
    assert!(evidence.checks[0].status.starts_with("could_not_start"));
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
    assert!(f.compile(&task("code"), None).unwrap().selected.is_empty());
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

#[test]
fn verification_expected_failure_and_timeout_are_observed_not_assumed() {
    let f = Fixture::new();
    let mut t = task("simple");
    t.checks.push(Check {
        id: "expected-red".into(),
        argv: vec!["rustc".into(), "--amh-invalid-option".into()],
        timeout_seconds: 10,
        expected_exit_code: 1,
    });
    let result = f.verify(&t, true);
    assert_eq!(result.checks[0].exit_code, Some(1));
    assert_eq!(result.all_checks_passed, Some(true));
    t.checks = vec![Check {
        id: "timeout".into(),
        argv: vec![
            std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            "--ignored".into(),
            "--exact".into(),
            "slow_helper".into(),
        ],
        timeout_seconds: 1,
        expected_exit_code: 0,
    }];
    let result = f.verify(&t, true);
    assert_eq!(result.checks[0].status, "timed_out");
    assert_eq!(result.all_checks_passed, Some(false));
}

#[test]
#[ignore = "invoked only by the timeout regression in a subprocess"]
fn slow_helper() {
    std::thread::sleep(std::time::Duration::from_secs(5));
}
