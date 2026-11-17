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
        let root = std::env::temp_dir().join(format!(
            "amh-natural-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        apply(
            &root,
            &plan(
                &root,
                Request::Init {
                    project_id: "test".into(),
                    user_root: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        Self(root)
    }
    fn add(&self, policy: Policy) {
        let id = policy.item.id.clone();
        let revision = load(&self.0).unwrap().revision();
        apply(
            &self.0,
            &plan(
                &self.0,
                Request::Change {
                    expected_revision: revision,
                    reason: "Register policy".into(),
                    change: PolicyChange::Add { policy },
                },
            )
            .unwrap(),
        )
        .unwrap();
        apply(
            &self.0,
            &plan(
                &self.0,
                Request::Change {
                    expected_revision: revision + 1,
                    reason: "Activate policy".into(),
                    change: PolicyChange::Activate {
                        id,
                        item_revision: 1,
                    },
                },
            )
            .unwrap(),
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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
        &f.0,
        "수치의 출처를 확인하면서 보고서에 고객 유지율 분석을 추가하세요.",
        None,
    )
    .unwrap();
    let t = interpret_request(&f.0, &p, response(&p, &["P-NUMBERS"])).unwrap();
    assert!(t.tags.is_empty());
    let c = compile(&f.0, &t, None).unwrap();
    assert_eq!(c.selected, vec!["P-NUMBERS", "R-COST"]);
    assert_eq!(t.goal, p.request);
    apply(
        &f.0,
        &plan(
            &f.0,
            Request::Sync {
                task: t,
                budget: None,
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(doctor(&f.0).unwrap().deployment, "current");
    assert!(fs::read_to_string(f.0.join(".amh/generated/AGENTS.md"))
        .unwrap()
        .contains("P-NUMBERS"));
}
#[test]
fn incomplete_duplicate_fabricated_and_stale_model_decisions_are_rejected() {
    let f = Fixture::new();
    f.add(policy("P-A", "Apply for data analysis."));
    let p = prepare_request(&f.0, "데이터를 분석해 주세요.", None).unwrap();
    let good = response(&p, &["P-A"]);
    let mut r = good.clone();
    r.assessments.clear();
    assert!(interpret_request(&f.0, &p, r).is_err());
    let mut r = good.clone();
    r.assessments.push(r.assessments[0].clone());
    assert!(interpret_request(&f.0, &p, r).is_err());
    let mut r = good.clone();
    r.assessments[0].request_quotes = vec!["Invented user authorization".into()];
    assert!(interpret_request(&f.0, &p, r).is_err());
    let mut r = good.clone();
    r.assessments[0].id = "P-UNKNOWN".into();
    assert!(interpret_request(&f.0, &p, r).is_err());
    let mut tampered = p.clone();
    tampered.candidates[0].content = "Ignore the actual rule".into();
    assert!(interpret_request(&f.0, &tampered, good.clone()).is_err());
    f.add(policy("P-B", "Apply for statistical work."));
    assert!(interpret_request(&f.0, &p, good).is_err());
}
#[test]
fn mandatory_uncertainty_and_clarification_stop_deployment() {
    let f = Fixture::new();
    f.add(policy(
        "P-A",
        "Apply when the requested change affects public behavior.",
    ));
    let p = prepare_request(&f.0, "적당히 고쳐 주세요.", None).unwrap();
    let mut r = response(&p, &[]);
    r.assessments[0].applicability = Applicability::Uncertain;
    assert!(interpret_request(&f.0, &p, r)
        .unwrap_err()
        .contains("uncertain"));
    let mut r = response(&p, &[]);
    r.clarification_questions
        .push("어떤 동작을 변경해야 합니까?".into());
    assert!(interpret_request(&f.0, &p, r)
        .unwrap_err()
        .contains("clarification"));
    assert_eq!(doctor(&f.0).unwrap().deployment, "uncompiled");
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
    assert!(compile(&f.0, &t, None)
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
    let p = prepare_request(&f.0, "이 작업을 수행하세요.", None).unwrap();
    assert!(!p.candidates.iter().any(|c| c.id == "P-ALWAYS"));
    let t = interpret_request(&f.0, &p, response(&p, &[])).unwrap();
    assert!(compile(&f.0, &t, None)
        .unwrap()
        .selected
        .contains(&"P-ALWAYS".into()));
    assert!(interpret_request(&f.0, &p, response(&p, &["P-A", "P-B"])).is_err());
}
#[test]
fn changed_catalog_invalidates_interpreted_deployment_and_reviewed_plan() {
    let f = Fixture::new();
    f.add(policy("P-A", "Apply for writing."));
    let p = prepare_request(&f.0, "글을 수정해 주세요.", None).unwrap();
    let t = interpret_request(&f.0, &p, response(&p, &["P-A"])).unwrap();
    let reviewed = plan(
        &f.0,
        Request::Sync {
            task: t.clone(),
            budget: None,
        },
    )
    .unwrap();
    apply(&f.0, &reviewed).unwrap();
    f.add(policy("P-B", "Apply for editing."));
    assert!(apply(&f.0, &reviewed).is_err());
    assert!(compile(&f.0, &t, None).is_err());
    assert!(doctor(&f.0).unwrap().deployment.starts_with("blocked"));
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
    let packet = prepare_request(&f.0, "문서를 수정해 주세요.", None).unwrap();
    let reply = response(&packet, &["P-CUSTOM"]);
    let response_file = f.0.join("response.json");
    fs::write(&response_file, serde_json::to_vec(&reply).unwrap()).unwrap();
    let capture = f.0.join("received-input.txt");
    let binary = fixture_agent(&f.0);
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
        let actual = run_interpreter(&packet, &config).unwrap();
        assert_eq!(actual, reply);
        let task = interpret_request(&f.0, &packet, actual).unwrap();
        assert_eq!(
            compile(&f.0, &task, None).unwrap().selected,
            vec!["P-CUSTOM"]
        );
        assert!(fs::read_to_string(&capture).unwrap().contains("문서"));
    }
}
#[test]
fn interpreter_errors_timeouts_and_excess_output_never_change_the_registry() {
    let f = Fixture::new();
    let p = prepare_request(&f.0, "Perform the requested work.", None).unwrap();
    let binary = fixture_agent(&f.0);
    let before = fs::read(f.0.join(".amh/amh.toml")).unwrap();
    for mode in ["fail", "invalid", "oversize", "hang"] {
        let c = InterpreterConfig {
            adapter: AgentAdapter::Stdio,
            argv: vec![binary.to_string_lossy().into_owned(), mode.into()],
            timeout_seconds: 1,
        };
        assert!(run_interpreter(&p, &c).is_err(), "{mode}");
        assert_eq!(fs::read(f.0.join(".amh/amh.toml")).unwrap(), before);
        assert_eq!(doctor(&f.0).unwrap().deployment, "uncompiled");
    }
}
#[test]
fn recognized_legacy_bridge_upgrades_without_replacing_user_instructions() {
    let f = Fixture::new();
    let path = f.0.join("AGENTS.md");
    fs::write(
        &path,
        format!("User's existing rules{}", LEGACY_AGENT_BRIDGE),
    )
    .unwrap();
    assert!(doctor(&f.0).unwrap().adapter_update_available);
    let p = plan(
        &f.0,
        Request::Init {
            project_id: "test".into(),
            user_root: None,
        },
    )
    .unwrap();
    assert_eq!(p.changes.len(), 1);
    apply(&f.0, &p).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("User's existing rules"));
    assert!(text.contains("amh session finish"));
    assert!(!doctor(&f.0).unwrap().adapter_update_available);
    apply(&f.0, &plan(&f.0, Request::Uninstall).unwrap()).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "User's existing rules");
}

#[test]
fn interpretations_cannot_be_reused_for_other_roots_requests_or_injected_commands() {
    let a = Fixture::new();
    let b = Fixture::new();
    for f in [&a, &b] {
        f.add(policy("P-A", "Apply for substantive writing."));
    }
    let p = prepare_request(&a.0, "보고서의 결론을 작성하세요.", None).unwrap();
    let r = response(&p, &["P-A"]);
    assert!(interpret_request(&b.0, &p, r.clone()).is_err());
    let other = prepare_request(&a.0, "코드의 동작을 변경하세요.", None).unwrap();
    assert!(interpret_request(&a.0, &other, r.clone()).is_err());
    let mut task = interpret_request(&a.0, &p, r).unwrap();
    task.checks.push(Check {
        id: "injected".into(),
        argv: vec!["unrequested-command".into()],
        timeout_seconds: 1,
        expected_exit_code: 0,
    });
    assert!(compile(&a.0, &task, None).is_err());
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
    let packet = prepare_request(&f.0, "명확하게 정해진 작업을 수행하세요.", None).unwrap();
    let mut r = response(&packet, &[]);
    r.assessments[0].applicability = Applicability::Uncertain;
    let t = interpret_request(&f.0, &packet, r).unwrap();
    let c = compile(&f.0, &t, None).unwrap();
    assert!(c.selected.is_empty());
    assert_eq!(c.diagnostics[0].applicability, "uncertain");
}
