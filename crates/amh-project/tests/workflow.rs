use amh_core::{Activation, ItemDetails, ItemKind, ItemScope, ManagedItem, PrincipleStrength};
use amh_project::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "amh-p0-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn init(&self) {
        let plan = plan(
            &self.0,
            Request::Init {
                project_id: "test".into(),
                user_root: None,
            },
        )
        .unwrap();
        apply(&self.0, &plan).unwrap();
    }
    fn change(&self, change: PolicyChange) {
        let revision = load(&self.0).unwrap().revision();
        let p = plan(
            &self.0,
            Request::Change {
                expected_revision: revision,
                reason: "User-selected fixture change".into(),
                change,
            },
        )
        .unwrap();
        apply(&self.0, &p).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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
fn init_preview_and_reinstall_preserve_user_files() {
    let f = Fixture::new();
    fs::write(f.0.join("AGENTS.md"), "# My rules\nKeep this verbatim.\n").unwrap();
    let p = plan(
        &f.0,
        Request::Init {
            project_id: "test".into(),
            user_root: None,
        },
    )
    .unwrap();
    assert!(!f.0.join(".amh").exists());
    apply(&f.0, &p).unwrap();
    assert!(fs::read_to_string(f.0.join("AGENTS.md"))
        .unwrap()
        .starts_with("# My rules\nKeep this verbatim.\n"));
    assert!(plan(
        &f.0,
        Request::Init {
            project_id: "test".into(),
            user_root: None
        }
    )
    .unwrap()
    .changes
    .is_empty());
    assert!(f.0.join(".amh/amh.toml").exists());
}

#[test]
fn selected_policy_is_compiled_and_pause_leaves_detectable_drift() {
    let f = Fixture::new();
    f.init();
    f.change(PolicyChange::Add {
        policy: principle("P-TDD", "code"),
    });
    f.change(PolicyChange::Activate {
        id: "P-TDD".into(),
        item_revision: 1,
    });
    let c = compile(&f.0, &task("writing"), None).unwrap();
    assert!(c.selected.is_empty());
    let c = compile(&f.0, &task("code"), None).unwrap();
    assert_eq!(c.selected, vec!["P-TDD"]);
    assert!(compile(&f.0, &task("code"), Some(1)).is_err());
    let p = plan(
        &f.0,
        Request::Sync {
            task: task("code"),
            budget: None,
        },
    )
    .unwrap();
    apply(&f.0, &p).unwrap();
    assert_eq!(doctor(&f.0).unwrap().deployment, "current");
    f.change(PolicyChange::Pause {
        id: "P-TDD".into(),
        item_revision: 2,
    });
    assert_eq!(doctor(&f.0).unwrap().deployment, "stale");
    assert_eq!(doctor(&f.0).unwrap().observed, "unknown");
    let p = plan(
        &f.0,
        Request::Sync {
            task: task("code"),
            budget: None,
        },
    )
    .unwrap();
    apply(&f.0, &p).unwrap();
    assert!(!fs::read_to_string(f.0.join(".amh/generated/AGENTS.md"))
        .unwrap()
        .contains("P-TDD"));
}

#[test]
fn stale_or_edited_plans_and_managed_file_edits_are_rejected() {
    let f = Fixture::new();
    f.init();
    let p = plan(
        &f.0,
        Request::Change {
            expected_revision: 0,
            reason: "Add".into(),
            change: PolicyChange::Add {
                policy: principle("P-1", "code"),
            },
        },
    )
    .unwrap();
    f.change(PolicyChange::Add {
        policy: principle("P-2", "code"),
    });
    assert!(apply(&f.0, &p).is_err());
    let mut p = plan(
        &f.0,
        Request::Sync {
            task: task("code"),
            budget: None,
        },
    )
    .unwrap();
    p.changes[0].after = Some("injected content".into());
    assert!(apply(&f.0, &p).is_err());
    fs::write(f.0.join(".amh/generated/AGENTS.md"), "user edit").unwrap();
    assert!(plan(
        &f.0,
        Request::Sync {
            task: task("code"),
            budget: None
        }
    )
    .is_err());
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
fn rollback_creates_a_new_revision_and_uninstall_keeps_user_configuration() {
    let f = Fixture::new();
    f.init();
    f.change(PolicyChange::Add {
        policy: principle("P-1", "code"),
    });
    let p = plan(
        &f.0,
        Request::Restore {
            expected_revision: 1,
            revision: 0,
            reason: "Undo addition".into(),
        },
    )
    .unwrap();
    apply(&f.0, &p).unwrap();
    assert_eq!(load(&f.0).unwrap().revision(), 2);
    assert!(load(&f.0).unwrap().policies().unwrap().is_empty());
    let p = plan(&f.0, Request::Uninstall).unwrap();
    apply(&f.0, &p).unwrap();
    assert!(f.0.join(".amh/amh.toml").exists());
    assert!(!f.0.join(".amh/generated/AGENTS.md").exists());
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
    f.init();
    f.change(PolicyChange::InstallPack {
        pack: builtin_pack("development").unwrap(),
    });
    f.change(PolicyChange::InstallPack {
        pack: builtin_pack("inquiry").unwrap(),
    });
    assert_eq!(pack_locks(&load(&f.0).unwrap()).unwrap().len(), 2);
    let writing = compile(&f.0, &task("document.revision"), None).unwrap();
    assert_eq!(writing.selected, vec!["P-WRITING"]);
    assert_eq!(
        compile(&f.0, &task("inquiry.unclear"), None)
            .unwrap()
            .selected,
        vec!["P-SOCRATIC"]
    );
    assert_eq!(
        compile(&f.0, &task("code.behavior_change"), None)
            .unwrap()
            .selected,
        vec!["P-TDD"]
    );
    assert_eq!(
        compile(&f.0, &task("code.refactor"), None)
            .unwrap()
            .selected,
        vec!["P-REFACTOR"]
    );
    assert!(compile(&f.0, &task("simple"), None)
        .unwrap()
        .selected
        .is_empty());
    let p = plan(
        &f.0,
        Request::Change {
            expected_revision: 2,
            reason: "Reinstall".into(),
            change: PolicyChange::InstallPack {
                pack: builtin_pack("development").unwrap(),
            },
        },
    );
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
    f.init();
    f.change(PolicyChange::Add {
        policy: requirement("R-COST", RequirementRole::Core),
    });
    f.change(PolicyChange::Activate {
        id: "R-COST".into(),
        item_revision: 1,
    });
    let original = load(&f.0).unwrap().policies().unwrap()[0].clone();
    f.change(PolicyChange::Add {
        policy: requirement("R-RETENTION", RequirementRole::Support),
    });
    f.change(PolicyChange::Activate {
        id: "R-RETENTION".into(),
        item_revision: 1,
    });
    assert_eq!(load(&f.0).unwrap().policies().unwrap()[0], original);
    let c = compile(&f.0, &task("document.revision"), None).unwrap();
    assert!(
        c.content.contains("Requirement role: Core")
            && c.content.contains("Requirement role: Support")
    );
    f.change(PolicyChange::Replace {
        id: "R-COST".into(),
        item_revision: 2,
        replacement: requirement("R-COST-NEW", RequirementRole::Core),
    });
    let c = compile(&f.0, &task("document.revision"), None).unwrap();
    assert_eq!(c.selected, vec!["R-COST-NEW", "R-RETENTION"]);
}

#[test]
fn explicit_priority_is_not_recency_and_removal_does_not_reuse_identity() {
    let f = Fixture::new();
    f.init();
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
        compile(&f.0, &task("code"), None).unwrap().selected,
        vec!["P-A", "P-Z"]
    );
    f.change(PolicyChange::Reprioritize {
        id: "P-Z".into(),
        item_revision: 2,
        priority: 5,
    });
    assert_eq!(
        compile(&f.0, &task("code"), None).unwrap().selected,
        vec!["P-Z", "P-A"]
    );
    f.change(PolicyChange::Remove {
        id: "P-A".into(),
        item_revision: 2,
    });
    assert!(plan(
        &f.0,
        Request::Change {
            expected_revision: 6,
            reason: "Reuse".into(),
            change: PolicyChange::Add {
                policy: principle("P-A", "code")
            }
        }
    )
    .is_err());
}

#[test]
fn user_registry_is_reused_without_cross_project_writes_and_import_changes_cause_drift() {
    let user = Fixture::new();
    user.init();
    let mut p = principle("P-USER", "code");
    p.item.scope = ItemScope::User;
    user.change(PolicyChange::Add { policy: p });
    user.change(PolicyChange::Activate {
        id: "P-USER".into(),
        item_revision: 1,
    });
    let a = Fixture::new();
    let b = Fixture::new();
    for project in [&a, &b] {
        apply(
            &project.0,
            &plan(
                &project.0,
                Request::Init {
                    project_id: "consumer".into(),
                    user_root: Some(user.0.clone()),
                },
            )
            .unwrap(),
        )
        .unwrap();
        apply(
            &project.0,
            &plan(
                &project.0,
                Request::Sync {
                    task: task("code"),
                    budget: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
    }
    let before = fs::read(b.0.join(".amh/amh.toml")).unwrap();
    a.change(PolicyChange::Add {
        policy: principle("P-LOCAL", "code"),
    });
    assert_eq!(fs::read(b.0.join(".amh/amh.toml")).unwrap(), before);
    user.change(PolicyChange::Pause {
        id: "P-USER".into(),
        item_revision: 2,
    });
    assert_eq!(doctor(&b.0).unwrap().deployment, "stale");
    assert!(compile(&b.0, &task("code"), None)
        .unwrap()
        .selected
        .is_empty());
}

#[test]
fn recovery_restores_partial_updates_and_refuses_unrelated_edits() {
    let f = Fixture::new();
    fs::write(f.0.join("AGENTS.md"), "Original user instructions").unwrap();
    let p = plan(
        &f.0,
        Request::Init {
            project_id: "test".into(),
            user_root: None,
        },
    )
    .unwrap();
    fs::create_dir_all(f.0.join(".amh")).unwrap();
    fs::write(
        f.0.join(".amh/transaction.json"),
        serde_json::to_vec(&p).unwrap(),
    )
    .unwrap();
    fs::write(
        f.0.join(&p.changes[0].path),
        p.changes[0].after.as_ref().unwrap(),
    )
    .unwrap();
    assert!(load(&f.0).is_err());
    assert!(recover(&f.0, false).is_ok());
    fs::write(f.0.join("AGENTS.md"), "Unrelated user edit").unwrap();
    assert!(recover(&f.0, true).is_err());
    fs::write(f.0.join("AGENTS.md"), "Original user instructions").unwrap();
    recover(&f.0, true).unwrap();
    assert!(!f.0.join(".amh/amh.toml").exists());
    assert_eq!(
        fs::read_to_string(f.0.join("AGENTS.md")).unwrap(),
        "Original user instructions"
    );
}

#[test]
fn uninstall_and_reinstall_preserve_exact_user_text_and_custom_policy_changes() {
    let f = Fixture::new();
    fs::write(f.0.join("AGENTS.md"), "No trailing newline").unwrap();
    f.init();
    f.change(PolicyChange::Add {
        policy: principle("P-1", "code"),
    });
    f.change(PolicyChange::Modify {
        id: "P-1".into(),
        item_revision: 1,
        content: "User customization".into(),
    });
    apply(&f.0, &plan(&f.0, Request::Uninstall).unwrap()).unwrap();
    assert_eq!(
        fs::read_to_string(f.0.join("AGENTS.md")).unwrap(),
        "No trailing newline"
    );
    apply(
        &f.0,
        &plan(
            &f.0,
            Request::Sync {
                task: task("code"),
                budget: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        load(&f.0).unwrap().policies().unwrap()[0].item.content,
        "User customization"
    );
}

#[cfg(unix)]
#[test]
fn symlink_targets_are_refused_and_shared_permissions_are_preserved() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let f = Fixture::new();
    let outside = Fixture::new();
    fs::write(outside.0.join("user.txt"), "untouched").unwrap();
    symlink(outside.0.join("user.txt"), f.0.join("AGENTS.md")).unwrap();
    assert!(plan(
        &f.0,
        Request::Init {
            project_id: "test".into(),
            user_root: None
        }
    )
    .is_err());
    fs::remove_file(f.0.join("AGENTS.md")).unwrap();
    fs::write(f.0.join("AGENTS.md"), "user instructions").unwrap();
    fs::set_permissions(f.0.join("AGENTS.md"), fs::Permissions::from_mode(0o640)).unwrap();
    f.init();
    assert_eq!(
        fs::metadata(f.0.join("AGENTS.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(
        fs::read_to_string(outside.0.join("user.txt")).unwrap(),
        "untouched"
    );
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
fn verification_records_real_commands_and_never_promotes_check_success_to_policy_compliance() {
    let f = Fixture::new();
    f.init();
    let mut t = task("simple");
    t.checks = vec![Check {
        id: "tool-version".into(),
        argv: vec!["rustc".into(), "--version".into()],
        timeout_seconds: 10,
        expected_exit_code: 0,
    }];
    apply(
        &f.0,
        &plan(
            &f.0,
            Request::Sync {
                task: t.clone(),
                budget: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let preview = verify(&f.0, &t, false).unwrap();
    assert_eq!(preview.all_checks_passed, None);
    assert_eq!(preview.checks[0].status, "not_run");
    assert!(!f.0.join(".amh/evidence").exists());
    let actual = verify(&f.0, &t, true).unwrap();
    assert_eq!(actual.all_checks_passed, Some(true));
    assert_eq!(actual.policy_compliance, "unknown");
    assert_eq!(actual.completion_criteria, "unknown");
    assert!(
        fs::read_to_string(f.0.join(actual.checks[0].stdout.as_ref().unwrap()))
            .unwrap()
            .contains("rustc")
    );
    assert!(f.0.join(actual.file.unwrap()).exists());
    t.checks[0].argv.push("--amh-invalid-option".into());
    assert!(verify(&f.0, &t, true).is_err()); // A different task must be deployed explicitly.
    apply(
        &f.0,
        &plan(
            &f.0,
            Request::Sync {
                task: t.clone(),
                budget: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        verify(&f.0, &t, true).unwrap().all_checks_passed,
        Some(false)
    );
}

#[test]
fn verification_missing_tools_are_failures_with_evidence_not_unexecuted_success() {
    let f = Fixture::new();
    f.init();
    let mut t = task("simple");
    t.checks.push(Check {
        id: "missing".into(),
        argv: vec!["amh-nonexistent-tool-for-regression".into()],
        timeout_seconds: 1,
        expected_exit_code: 0,
    });
    apply(
        &f.0,
        &plan(
            &f.0,
            Request::Sync {
                task: t.clone(),
                budget: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let evidence = verify(&f.0, &t, true).unwrap();
    assert_eq!(evidence.all_checks_passed, Some(false));
    assert!(evidence.checks[0].status.starts_with("could_not_start"));
}

#[test]
fn task_file_changes_invalidate_reviewed_plans_and_existing_deployment() {
    let f = Fixture::new();
    f.init();
    let file = f.0.join("task.json");
    fs::write(&file, serde_json::to_vec(&task("code")).unwrap()).unwrap();
    let request = Request::SyncFile {
        task_file: file.clone(),
        budget: None,
    };
    let old_plan = plan(&f.0, request.clone()).unwrap();
    let mut next = task("code");
    next.goal = "Changed accepted goal".into();
    fs::write(&file, serde_json::to_vec(&next).unwrap()).unwrap();
    assert!(apply(&f.0, &old_plan).is_err());
    apply(&f.0, &plan(&f.0, request).unwrap()).unwrap();
    assert_eq!(doctor(&f.0).unwrap().deployment, "current");
    next.goal = "Another accepted goal".into();
    fs::write(&file, serde_json::to_vec(&next).unwrap()).unwrap();
    assert_eq!(doctor(&f.0).unwrap().deployment, "stale");
}

#[test]
fn metadata_and_lifecycle_changes_are_versioned_and_invalid_updates_leave_source_intact() {
    let f = Fixture::new();
    f.init();
    f.change(PolicyChange::Add {
        policy: principle("P-1", "code"),
    });
    let before = fs::read(f.0.join(".amh/amh.toml")).unwrap();
    assert!(plan(
        &f.0,
        Request::Change {
            expected_revision: 1,
            reason: "Invalid update".into(),
            change: PolicyChange::Modify {
                id: "P-1".into(),
                item_revision: 1,
                content: " ".into()
            }
        }
    )
    .is_err());
    assert_eq!(fs::read(f.0.join(".amh/amh.toml")).unwrap(), before);
    f.change(PolicyChange::SetMetadata {
        id: "P-1".into(),
        item_revision: 1,
        owner: "project-user".into(),
        source: Some("Reviewed source".into()),
        relations: Relations::default(),
    });
    assert_eq!(
        load(&f.0).unwrap().policies().unwrap()[0].owner,
        "project-user"
    );
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
    assert!(compile(&f.0, &task("code"), None)
        .unwrap()
        .selected
        .is_empty());
    assert!(plan(
        &f.0,
        Request::Change {
            expected_revision: 6,
            reason: "Revive terminal policy".into(),
            change: PolicyChange::Reactivate {
                id: "P-1".into(),
                item_revision: 6
            }
        }
    )
    .is_err());
}

#[test]
fn verification_expected_failure_and_timeout_are_observed_not_assumed() {
    let f = Fixture::new();
    f.init();
    let mut t = task("simple");
    t.checks.push(Check {
        id: "expected-red".into(),
        argv: vec!["rustc".into(), "--amh-invalid-option".into()],
        timeout_seconds: 10,
        expected_exit_code: 1,
    });
    apply(
        &f.0,
        &plan(
            &f.0,
            Request::Sync {
                task: t.clone(),
                budget: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let result = verify(&f.0, &t, true).unwrap();
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
    apply(
        &f.0,
        &plan(
            &f.0,
            Request::Sync {
                task: t.clone(),
                budget: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let result = verify(&f.0, &t, true).unwrap();
    assert_eq!(result.checks[0].status, "timed_out");
    assert_eq!(result.all_checks_passed, Some(false));
}

#[test]
#[ignore = "invoked only by the timeout regression in a subprocess"]
fn slow_helper() {
    std::thread::sleep(std::time::Duration::from_secs(5));
}
