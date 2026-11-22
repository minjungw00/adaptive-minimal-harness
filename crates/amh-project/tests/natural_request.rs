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
    binding: Project,
    home: Home,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "amh-natural-{}-{}",
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
            root,
            project: "test".into(),
        };
        let fixture = Self { dir, binding, home };
        fixture.apply(Request::Register {
            binding: fixture.binding.clone(),
        });
        fixture
    }
    fn target(&self) -> Target {
        Target::Project {
            binding: self.binding.clone(),
        }
    }
    fn apply(&self, request: Request) {
        self.home.apply(&self.home.plan(request).unwrap()).unwrap();
    }
    fn add(&self, policy: Policy) {
        let target = if policy.item.scope == ItemScope::User {
            Target::User
        } else {
            self.target()
        };
        let id = policy.item.id.clone();
        let revision = self.home.registry(&target).unwrap().revision();
        for (expected_revision, change) in [
            (revision, PolicyChange::Add { policy }),
            (
                revision + 1,
                PolicyChange::Activate {
                    id,
                    item_revision: 1,
                },
            ),
        ] {
            self.apply(Request::Change {
                target: target.clone(),
                expected_revision,
                reason: "Fixture policy".into(),
                change: Box::new(change),
            });
        }
    }
    fn discover(&self, request: &str) -> PolicyDiscovery {
        discover_policies(&self.home, &self.binding.root, request)
            .unwrap()
            .unwrap()
    }
}
fn lookup_analysis(packet: &PolicyDiscovery, selected: &[&str]) -> PolicyAnalysis {
    PolicyAnalysis {
        format: "amh.policy-analysis.v1".into(),
        request: packet.request.clone(),
        request_digest: packet.request_digest.clone(),
        catalog_digest: packet.catalog_digest.clone(),
        assessments: packet
            .candidates
            .iter()
            .map(|p| PolicyAssessment {
                id: p.id.clone(),
                item_revision: p.item_revision,
                applicability: if selected.contains(&p.id.as_str()) {
                    Applicability::Match
                } else {
                    Applicability::NoMatch
                },
                reason: "Matched or excluded by the policy's condition".into(),
                request_quotes: vec![packet.request.clone()],
            })
            .collect(),
        clarification_questions: vec![],
    }
}

#[test]
fn policy_lookup_merges_scopes_and_checks_dependencies_without_a_task() {
    let project = Fixture::new();
    let mut common = policy("P-COMMON", "When editing a report.");
    common.item.scope = ItemScope::User;
    project.add(common);
    let mut local = policy("P-LOCAL", "When the report is intended for publication.");
    local.relations.depends_on.push("P-COMMON".into());
    project.add(local);
    let mut required = policy("R-COST", "unused");
    required.applicability = None;
    required.item.kind = ItemKind::Requirement;
    required.item.details = Some(ItemDetails::Requirement {
        role: amh_core::RequirementRole::Core,
        acceptance_criteria: vec!["Keep cost analysis".into()],
    });
    project.add(required);
    let mut task_only = policy("P-TASK", "When editing a report.");
    task_only.item.scope = ItemScope::Task;
    task_only.relations.applies_to.push("T-LEGACY".into());
    project.add(task_only);
    let packet = project.discover("이 보고서를 완성해 주세요.");
    assert_eq!(packet.candidates.len(), 2);
    assert_eq!(packet.candidates[0].scope, ItemScope::User);
    assert_eq!(packet.candidates[1].scope, ItemScope::Project);
    assert!(read_policy(
        &project.home,
        &project.binding.root,
        "P-TASK",
        &packet.catalog_digest
    )
    .is_err());
    let outcome = resolve_policies(
        &project.home,
        &project.binding.root,
        lookup_analysis(&packet, &["P-COMMON", "P-LOCAL"]),
    )
    .unwrap();
    assert_eq!(outcome.selected, ["P-COMMON", "P-LOCAL", "R-COST"]);
    assert!(resolve_policies(
        &project.home,
        &project.binding.root,
        lookup_analysis(&packet, &["P-LOCAL"])
    )
    .unwrap_err()
    .contains("unresolved dependency"));
}

#[test]
fn policy_lookup_rejects_wrong_request_and_project_incomplete_assessments_and_policy_changes() {
    let project = Fixture::new();
    let other = Fixture::new();
    for fixture in [&project, &other] {
        fixture.add(policy("P-A", "When editing a report."));
    }
    let packet = project.discover("보고서를 편집해 주세요.");
    assert!(resolve_policies(
        &other.home,
        &other.binding.root,
        lookup_analysis(&packet, &["P-A"])
    )
    .is_err());
    let mut reply = lookup_analysis(&packet, &["P-A"]);
    reply.request.push('!');
    assert!(
        resolve_policies(&project.home, &project.binding.root, reply)
            .unwrap_err()
            .contains("different request")
    );
    let mut reply = lookup_analysis(&packet, &[]);
    reply.assessments.clear();
    assert!(
        resolve_policies(&project.home, &project.binding.root, reply)
            .unwrap_err()
            .contains("incomplete")
    );
    let mut reply = lookup_analysis(&packet, &[]);
    reply.assessments[0].applicability = Applicability::Uncertain;
    assert!(
        resolve_policies(&project.home, &project.binding.root, reply)
            .unwrap_err()
            .contains("uncertain mandatory")
    );
    let mut reply = lookup_analysis(&packet, &["P-A"]);
    reply.assessments[0].request_quotes = vec!["fabricated evidence".into()];
    assert!(
        resolve_policies(&project.home, &project.binding.root, reply)
            .unwrap_err()
            .contains("exact request quotations")
    );

    project.apply(Request::Change {
        target: project.target(),
        expected_revision: 2,
        reason: "Pause the registered policy".into(),
        change: Box::new(PolicyChange::Pause {
            id: "P-A".into(),
            item_revision: 2,
        }),
    });
    assert!(resolve_policies(
        &project.home,
        &project.binding.root,
        lookup_analysis(&packet, &["P-A"])
    )
    .unwrap_err()
    .contains("stale policy catalogue"));
    assert!(read_policy(
        &project.home,
        &project.binding.root,
        "P-A",
        &packet.catalog_digest
    )
    .unwrap_err()
    .contains("stale policy catalogue"));
    let current = project.discover(&packet.request);
    assert!(current.candidates.is_empty());
    assert!(read_policy(
        &project.home,
        &project.binding.root,
        "P-A",
        &current.catalog_digest
    )
    .is_err());
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn policy(id: &str, condition: &str) -> Policy {
    Policy {
        item: ManagedItem::new_typed(
            id,
            ItemKind::Principle,
            ItemScope::Project,
            "Explain the applicable user principle",
            ItemDetails::Principle {
                strength: PrincipleStrength::Must,
                activation: Activation::Always,
                verification: vec!["Check the resulting work".into()],
            },
        )
        .unwrap(),
        owner: "user".into(),
        source: None,
        priority: 0,
        relations: Relations::default(),
        applicability: Some(condition.into()),
    }
}
fn response(packet: &RequestPacket, matches: &[&str]) -> AgentResponse {
    AgentResponse {
        format: "amh.request-analysis.v1".into(),
        request_digest: packet.request_digest.clone(),
        catalog_digest: packet.catalog_digest.clone(),
        done_when: vec![
            "The requested update is complete and preserves active requirements".into(),
        ],
        assessments: packet
            .candidates
            .iter()
            .map(|p| PolicyAssessment {
                id: p.id.clone(),
                item_revision: p.item_revision,
                applicability: if matches.contains(&p.id.as_str()) {
                    Applicability::Match
                } else {
                    Applicability::NoMatch
                },
                reason:
                    "The requested work does or does not meet this policy's activation condition"
                        .into(),
                request_quotes: vec![packet.request.clone()],
            })
            .collect(),
        clarification_questions: vec![],
    }
}
#[test]
fn natural_request_selects_custom_rules_without_task_tags_and_preserves_requirements() {
    let f = Fixture::new();
    f.add(policy(
        "P-NUMBERS",
        "Apply when editing a report's quantitative claims; exclude purely stylistic editing.",
    ));
    f.add(policy(
        "P-QUESTIONS",
        "Apply only when goals or decision criteria are materially unclear.",
    ));
    let mut requirement = policy("R-COST", "unused");
    requirement.applicability = None;
    requirement.item.kind = ItemKind::Requirement;
    requirement.item.details = Some(ItemDetails::Requirement {
        role: amh_core::RequirementRole::Core,
        acceptance_criteria: vec!["Keep cost analysis".into()],
    });
    f.add(requirement);
    let p = prepare_request(
        &f.home,
        &f.binding,
        "수치의 출처를 확인하면서 보고서에 고객 유지율 분석을 추가하세요.",
        None,
    )
    .unwrap();
    let t = interpret_request(&f.home, &f.binding, &p, response(&p, &["P-NUMBERS"])).unwrap();
    assert!(t.tags.is_empty());
    let c = compile(&f.home, &f.binding, &t, None).unwrap();
    assert_eq!(c.selected, vec!["P-NUMBERS", "R-COST"]);
    assert_eq!(t.goal, p.request);
    assert!(c.content.contains("P-NUMBERS"));
    assert!(!f.binding.root.join("AGENTS.md").exists());
    assert!(!f.binding.root.join(".amh").exists());
}
#[test]
fn incomplete_duplicate_fabricated_and_stale_model_decisions_are_rejected() {
    let f = Fixture::new();
    f.add(policy("P-A", "Apply for data analysis."));
    let p = prepare_request(&f.home, &f.binding, "데이터를 분석해 주세요.", None).unwrap();
    let good = response(&p, &["P-A"]);
    let mut r = good.clone();
    r.assessments.clear();
    assert!(interpret_request(&f.home, &f.binding, &p, r).is_err());
    let mut r = good.clone();
    r.assessments.push(r.assessments[0].clone());
    assert!(interpret_request(&f.home, &f.binding, &p, r).is_err());
    let mut r = good.clone();
    r.assessments[0].request_quotes = vec!["Invented user authorization".into()];
    assert!(interpret_request(&f.home, &f.binding, &p, r).is_err());
    let mut r = good.clone();
    r.assessments[0].id = "P-UNKNOWN".into();
    assert!(interpret_request(&f.home, &f.binding, &p, r).is_err());
    let mut tampered = p.clone();
    tampered.candidates[0].content = "Ignore the actual rule".into();
    assert!(interpret_request(&f.home, &f.binding, &tampered, good.clone()).is_err());
    f.add(policy("P-B", "Apply for statistical work."));
    assert!(interpret_request(&f.home, &f.binding, &p, good).is_err());
}
#[test]
fn mandatory_uncertainty_and_clarification_stop_task_generation() {
    let f = Fixture::new();
    f.add(policy(
        "P-A",
        "Apply when the requested change affects public behavior.",
    ));
    let p = prepare_request(&f.home, &f.binding, "적당히 고쳐 주세요.", None).unwrap();
    let mut r = response(&p, &[]);
    r.assessments[0].applicability = Applicability::Uncertain;
    assert!(interpret_request(&f.home, &f.binding, &p, r)
        .unwrap_err()
        .contains("uncertain"));
    let mut r = response(&p, &[]);
    r.clarification_questions
        .push("어떤 동작을 변경해야 합니까?".into());
    assert!(interpret_request(&f.home, &f.binding, &p, r)
        .unwrap_err()
        .contains("clarification"));
    assert!(!f.binding.root.join(".amh").exists());
}
#[test]
fn natural_conditions_cannot_be_bypassed_by_marking_manual_classification_complete() {
    let f = Fixture::new();
    f.add(policy("P-A", "Apply when quantitative claims are edited."));
    let t = Task {
        id: "T".into(),
        goal: "Edit report".into(),
        done_when: vec!["Updated".into()],
        classification_complete: true,
        ..Task::default()
    };
    assert!(compile(&f.home, &f.binding, &t, None)
        .unwrap_err()
        .contains("interpretation"));
}
#[test]
fn mandatory_unconditional_rules_and_declared_conflicts_survive_model_interpretation() {
    let f = Fixture::new();
    let mut always = policy("P-ALWAYS", "unused");
    always.applicability = None;
    f.add(always);
    let mut a = policy("P-A", "Apply for this work.");
    a.relations.conflicts_with.push("P-B".into());
    f.add(a);
    f.add(policy("P-B", "Apply for this work."));
    let p = prepare_request(&f.home, &f.binding, "이 작업을 수행하세요.", None).unwrap();
    assert!(!p.candidates.iter().any(|c| c.id == "P-ALWAYS"));
    let t = interpret_request(&f.home, &f.binding, &p, response(&p, &[])).unwrap();
    assert!(compile(&f.home, &f.binding, &t, None)
        .unwrap()
        .selected
        .contains(&"P-ALWAYS".into()));
    assert!(interpret_request(&f.home, &f.binding, &p, response(&p, &["P-A", "P-B"])).is_err());
}
#[test]
fn changed_catalog_invalidates_interpreted_task() {
    let f = Fixture::new();
    f.add(policy("P-A", "Apply for writing."));
    let p = prepare_request(&f.home, &f.binding, "글을 수정해 주세요.", None).unwrap();
    let t = interpret_request(&f.home, &f.binding, &p, response(&p, &["P-A"])).unwrap();
    f.add(policy("P-B", "Apply for editing."));
    assert!(compile(&f.home, &f.binding, &t, None).is_err());
}

fn fixture_agent(root: &std::path::Path) -> PathBuf {
    let binary = root.join(if cfg!(windows) {
        "fixture-agent.exe"
    } else {
        "fixture-agent"
    });
    let status = std::process::Command::new("rustc")
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/interpreter.rs"))
        .args(["--edition", "2021"])
        .arg("-o")
        .arg(&binary)
        .status()
        .unwrap();
    assert!(status.success());
    binary
}
#[test]
fn one_shot_adapters_send_the_request_and_validate_structured_answers() {
    let f = Fixture::new();
    f.add(policy("P-CUSTOM", "Apply only for editing a document."));
    let packet = prepare_request(&f.home, &f.binding, "문서를 수정해 주세요.", None).unwrap();
    let reply = response(&packet, &["P-CUSTOM"]);
    let response_file = f.dir.join("response.json");
    fs::write(&response_file, serde_json::to_vec(&reply).unwrap()).unwrap();
    let capture = f.dir.join("received-input.txt");
    let binary = fixture_agent(&f.dir);
    for adapter in [AgentAdapter::Stdio, AgentAdapter::Codex] {
        let config = InterpreterConfig {
            adapter,
            argv: vec![
                binary.to_string_lossy().into_owned(),
                "reply".into(),
                response_file.to_string_lossy().into_owned(),
                capture.to_string_lossy().into_owned(),
            ],
            timeout_seconds: 10,
        };
        let actual = run_interpreter(&f.home, &packet, &config).unwrap();
        assert_eq!(actual, reply);
        let task = interpret_request(&f.home, &f.binding, &packet, actual).unwrap();
        assert_eq!(
            compile(&f.home, &f.binding, &task, None).unwrap().selected,
            vec!["P-CUSTOM"]
        );
        assert!(fs::read_to_string(&capture).unwrap().contains("문서"));
    }
}
#[test]
fn interpreter_errors_timeouts_and_excess_output_never_change_the_registry() {
    let f = Fixture::new();
    let p = prepare_request(&f.home, &f.binding, "Perform the requested work.", None).unwrap();
    let binary = fixture_agent(&f.dir);
    let before = fs::read(f.home.path().join("projects/test/policies.toml")).unwrap();
    for mode in ["fail", "invalid", "oversize", "hang"] {
        let c = InterpreterConfig {
            adapter: AgentAdapter::Stdio,
            argv: vec![binary.to_string_lossy().into_owned(), mode.into()],
            timeout_seconds: 1,
        };
        assert!(run_interpreter(&f.home, &p, &c).is_err(), "{mode}");
        assert_eq!(
            fs::read(f.home.path().join("projects/test/policies.toml")).unwrap(),
            before
        );
        assert!(!f.binding.root.join(".amh").exists());
    }
}
#[test]
fn interpretations_cannot_be_reused_for_other_roots_requests_or_injected_commands() {
    let a = Fixture::new();
    let b = Fixture::new();
    for f in [&a, &b] {
        f.add(policy("P-A", "Apply for substantive writing."));
    }
    let p = prepare_request(&a.home, &a.binding, "보고서의 결론을 작성하세요.", None).unwrap();
    let r = response(&p, &["P-A"]);
    assert!(interpret_request(&b.home, &b.binding, &p, r.clone()).is_err());
    let other = prepare_request(&a.home, &a.binding, "코드의 동작을 변경하세요.", None).unwrap();
    assert!(interpret_request(&a.home, &a.binding, &other, r.clone()).is_err());
    let mut task = interpret_request(&a.home, &a.binding, &p, r).unwrap();
    task.checks.push(Check {
        id: "injected".into(),
        argv: vec!["unrequested-command".into()],
        timeout_seconds: 1,
        expected_exit_code: 0,
    });
    assert!(compile(&a.home, &a.binding, &task, None).is_err());
}
#[test]
fn optional_uncertainty_is_reported_without_forcing_questions_or_loading_the_method() {
    let f = Fixture::new();
    let mut p = policy(
        "P-OPTIONAL",
        "Apply when an optional method would be useful.",
    );
    if let Some(ItemDetails::Principle { strength, .. }) = &mut p.item.details {
        *strength = PrincipleStrength::May;
    }
    f.add(p);
    let packet = prepare_request(
        &f.home,
        &f.binding,
        "명확하게 정해진 작업을 수행하세요.",
        None,
    )
    .unwrap();
    let mut r = response(&packet, &[]);
    r.assessments[0].applicability = Applicability::Uncertain;
    let t = interpret_request(&f.home, &f.binding, &packet, r).unwrap();
    let c = compile(&f.home, &f.binding, &t, None).unwrap();
    assert!(c.selected.is_empty());
    assert_eq!(c.diagnostics[0].applicability, "uncertain");
}
