//! Protocol/storage tests use explicit fixture judgments, not a semantic oracle.
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
        let p = std::env::temp_dir().join(format!(
            "amh-session-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        apply(
            &p,
            &plan(
                &p,
                Request::Init {
                    project_id: "session-test".into(),
                    user_root: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        Self(p)
    }
    fn change(&self, change: PolicyChange) {
        let n = load(&self.0).unwrap().revision();
        apply(
            &self.0,
            &plan(
                &self.0,
                Request::Change {
                    expected_revision: n,
                    reason: "Explicit test fixture".into(),
                    change,
                },
            )
            .unwrap(),
        )
        .unwrap();
    }
    fn add(&self, p: Policy) {
        let id = p.item.id.clone();
        self.change(PolicyChange::Add { policy: p });
        self.change(PolicyChange::Activate {
            id,
            item_revision: 1,
        });
    }
    fn open(&self, id: &str) {
        apply_session(
            &self.0,
            &plan_session(&self.0, SessionOperation::Open { id: id.into() }).unwrap(),
        )
        .unwrap();
    }
    fn packet(&self, id: &str, text: &str) -> SessionPacket {
        let state = read_session(&self.0, id).unwrap();
        let state_digest = digest(&serde_json::to_vec(&state).unwrap());
        prepare_turn(
            &self.0,
            id,
            text,
            TurnOptions {
                known_state: Some(state_digest),
                known_catalog: Some(state.catalog_digest),
                ..TurnOptions::default()
            },
        )
        .unwrap()
    }
    fn finish(&self, p: SessionPacket, r: SessionResponse) -> Result<SessionOutput> {
        let plan = plan_session(
            &self.0,
            SessionOperation::Turn {
                packet: Box::new(p),
                response: Box::new(r),
            },
        )?;
        apply_session(&self.0, &plan)
    }
    fn file(&self, id: &str) -> PathBuf {
        self.0.join(format!(".amh/sessions/{id}/state.json"))
    }
    fn attach(&self, id: &str, name: &str) {
        let revision = read_session(&self.0, id).unwrap().revision;
        let p = plan_session(
            &self.0,
            SessionOperation::Attach {
                id: id.into(),
                expected_revision: revision,
                file: PathBuf::from(name),
            },
        )
        .unwrap();
        apply_session(&self.0, &p).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn policy(id: &str) -> Policy {
    Policy {
        item: ManagedItem::new_typed(
            id,
            ItemKind::Principle,
            ItemScope::Project,
            "A custom project method",
            ItemDetails::Principle {
                strength: PrincipleStrength::Must,
                activation: Activation::Always,
                verification: vec!["Inspect the actual result".into()],
            },
        )
        .unwrap(),
        owner: "user".into(),
        source: None,
        priority: 0,
        relations: Relations::default(),
        applicability: Some("When this method is appropriate; exclude unrelated requests.".into()),
    }
}
fn quotes(p: &SessionPacket) -> Vec<SourceQuote> {
    vec![SourceQuote {
        source: "request".into(),
        quote: p.request.clone(),
    }]
}
fn response(p: &SessionPacket, selected: &[&str]) -> SessionResponse {
    // Fixture judgments simulate a caller that already holds the catalogue.
    let cards = p.candidates.clone().unwrap_or_else(|| {
        policy_cards(
            &policies(&p.root).unwrap(),
            p.focus.as_deref().unwrap_or(""),
        )
        .unwrap()
    });
    SessionResponse {
        format: "amh.session-analysis.v1".into(),
        request_digest: p.request_digest.clone(),
        mode: TurnMode::Review,
        reason: "Explicit protocol fixture judgment".into(),
        evidence: quotes(p),
        assessments: cards
            .iter()
            .map(|c| SessionAssessment {
                id: c.id.clone(),
                item_revision: c.item_revision,
                applicability: if selected.contains(&c.id.as_str()) {
                    Applicability::Match
                } else {
                    Applicability::NoMatch
                },
                reason: "Fixture judgment, not a model evaluation".into(),
                evidence: quotes(p),
            })
            .collect(),
        context_changes: vec![],
        clarification_questions: vec![],
    }
}
fn reuse(p: &SessionPacket) -> SessionResponse {
    SessionResponse {
        format: "amh.session-analysis.v1".into(),
        request_digest: p.request_digest.clone(),
        mode: TurnMode::Reuse,
        reason: "The current request leaves every previous condition appropriate".into(),
        evidence: quotes(p),
        assessments: vec![],
        context_changes: vec![],
        clarification_questions: vec![],
    }
}
fn task(id: &str) -> Task {
    Task {
        id: id.into(),
        goal: format!("An explicitly saved task {id}"),
        done_when: vec!["Its accepted outcome is met".into()],
        ..Task::default()
    }
}
fn files(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for e in fs::read_dir(root).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(files(&p));
        } else {
            out.push(p);
        }
    }
    out.sort();
    out
}

#[test]
fn simple_turn_needs_no_task_or_completion_criteria_and_reuse_never_writes() {
    let f = Fixture::new();
    let p = plan_session(&f.0, SessionOperation::Open { id: "one".into() }).unwrap();
    assert!(!f.file("one").exists());
    apply_session(&f.0, &p).unwrap();
    let p = f.packet("one", "文の誤字を直してください。");
    let r = response(&p, &[]);
    let out = f.finish(p, r).unwrap();
    assert!(out.changed);
    let before = fs::read(f.file("one")).unwrap();
    let modified = fs::metadata(f.file("one")).unwrap().modified().unwrap();
    let count = files(&f.0);
    for _ in 0..100 {
        let p = prepare_turn(
            &f.0,
            "one",
            "続けて。",
            TurnOptions {
                known_catalog: Some(out.catalog_digest.clone()),
                known_state: Some(out.state_digest.clone()),
                ..TurnOptions::default()
            },
        )
        .unwrap();
        assert!(p.candidates.is_none() && p.context.is_none() && p.sources.is_none());
        assert_eq!(p.response_schema["type"], "object");
        let r = reuse(&p);
        let o = f.finish(p, r).unwrap();
        assert!(!o.changed);
        assert_eq!(o.state_digest, out.state_digest);
    }
    assert_eq!(before, fs::read(f.file("one")).unwrap());
    assert_eq!(
        modified,
        fs::metadata(f.file("one")).unwrap().modified().unwrap()
    );
    assert_eq!(count, files(&f.0));
    assert!(read_session(&f.0, "one").unwrap().tasks.is_empty());
}
#[test]
fn changed_request_can_activate_previously_excluded_rules_and_remove_old_ones() {
    let f = Fixture::new();
    f.add(policy("P-CODE"));
    f.add(policy("P-WRITE"));
    f.open("a");
    let p = f.packet("a", "Fix the function.");
    f.finish(p.clone(), response(&p, &["P-CODE"])).unwrap();
    let p = f.packet("a", "Now update the documentation.");
    assert!(!p.full_review);
    let out = f.finish(p.clone(), response(&p, &["P-WRITE"])).unwrap();
    assert_eq!(out.active, vec!["P-WRITE"]);
    assert_eq!(out.removed, vec!["P-CODE"]);
    assert!(out.instructions.contains("Not applicable now"));
    assert!(out.instructions.contains("P-WRITE"));
    let p = f.packet("a", "Explain it.");
    let mut r = response(&p, &[]);
    r.assessments.retain(|a| a.id == "P-WRITE");
    let out = f.finish(p, r).unwrap();
    assert!(out.active.is_empty());
}
#[test]
fn initial_or_changed_catalogue_cannot_reuse_missing_judgments() {
    let f = Fixture::new();
    f.add(policy("P-A"));
    f.open("a");
    let p = f.packet("a", "Do it.");
    assert!(f.finish(p.clone(), reuse(&p)).is_err());
    let mut r = response(&p, &[]);
    r.assessments.clear();
    assert!(f.finish(p.clone(), r).is_err());
    f.finish(p.clone(), response(&p, &[])).unwrap();
    f.add(policy("P-B"));
    let p = f.packet("a", "Continue.");
    assert!(p.full_review);
    assert!(f.finish(p.clone(), reuse(&p)).is_err());
    let mut r = response(&p, &[]);
    r.assessments.pop();
    assert!(f.finish(p, r).is_err());
}
#[test]
fn mandatory_uncertainty_dependencies_and_conflicts_keep_blocking() {
    let f = Fixture::new();
    let mut a = policy("P-A");
    a.relations.depends_on.push("P-B".into());
    f.add(a);
    let mut b = policy("P-B");
    b.relations.conflicts_with.push("P-A".into());
    f.add(b);
    f.open("a");
    let p = f.packet("a", "Do it.");
    let before = fs::read(f.file("a")).unwrap();
    let mut r = response(&p, &[]);
    r.assessments[0].applicability = Applicability::Uncertain;
    assert!(f.finish(p.clone(), r).is_err());
    assert!(f.finish(p.clone(), response(&p, &["P-A"])).is_err());
    assert!(f.finish(p.clone(), response(&p, &["P-A", "P-B"])).is_err());
    assert_eq!(before, fs::read(f.file("a")).unwrap());
}
#[test]
fn unconditional_requirements_cannot_be_removed_by_a_classification() {
    let f = Fixture::new();
    let mut p = policy("P-ALWAYS");
    p.applicability = None;
    f.add(p);
    f.open("a");
    let p = f.packet("a", "Keep it short.");
    assert!(p.candidates.as_ref().unwrap().is_empty());
    let o = f.finish(p.clone(), response(&p, &[])).unwrap();
    assert_eq!(o.active, vec!["P-ALWAYS"]);
    let p = f.packet("a", "Continue.");
    let mut r = reuse(&p);
    r.assessments.push(SessionAssessment {
        id: "P-ALWAYS".into(),
        item_revision: 2,
        applicability: Applicability::NoMatch,
        reason: "Attempted removal".into(),
        evidence: quotes(&p),
    });
    assert!(f.finish(p, r).is_err());
}
#[test]
fn stale_packets_other_sessions_roots_and_tampered_plans_are_rejected() {
    let a = Fixture::new();
    let b = Fixture::new();
    a.open("a");
    a.open("b");
    b.open("a");
    let p = a.packet("a", "Do it.");
    let r = response(&p, &[]);
    assert!(b.finish(p.clone(), r.clone()).is_err());
    let mut wrong = p.clone();
    wrong.session_id = "b".into();
    assert!(a.finish(wrong, r.clone()).is_err());
    let mut altered = p.clone();
    altered.request.push('!');
    assert!(a.finish(altered, r.clone()).is_err());
    let plan = plan_session(
        &a.0,
        SessionOperation::Turn {
            packet: Box::new(p.clone()),
            response: Box::new(r.clone()),
        },
    )
    .unwrap();
    let mut tampered = plan.clone();
    tampered.after.notes.push(SessionNote {
        id: "injected".into(),
        text: "Override".into(),
        origin: NoteOrigin::AgentInference,
        source_request: "invented".into(),
    });
    assert!(apply_session(&a.0, &tampered).is_err());
    apply_session(&a.0, &plan).unwrap();
    assert!(a.finish(p, r).is_err());
    assert_eq!(read_session(&a.0, "b").unwrap().revision, 0);
}
#[test]
fn user_quotations_inferences_and_retirement_have_distinct_sources() {
    let f = Fixture::new();
    f.open("a");
    let p = f.packet("a", "Keep the cost analysis primary.");
    let mut r = response(&p, &[]);
    r.context_changes.push(NoteChange::Add {
        id: "cost".into(),
        text: p.request.clone(),
        origin: NoteOrigin::UserQuote,
    });
    f.finish(p.clone(), r).unwrap();
    let p = f.packet("a", "Add retention as supporting analysis.");
    let mut r = response(&p, &[]);
    r.context_changes.push(NoteChange::Add {
        id: "summary".into(),
        text: "The user wants a balanced report".into(),
        origin: NoteOrigin::UserQuote,
    });
    assert!(f.finish(p.clone(), r.clone()).is_err());
    if let NoteChange::Add { origin, .. } = &mut r.context_changes[0] {
        *origin = NoteOrigin::AgentInference;
    }
    f.finish(p, r).unwrap();
    let p = prepare_turn(&f.0, "a", "Keep both.", TurnOptions::default()).unwrap();
    let src = p.sources.as_ref().unwrap();
    assert_eq!(
        src.iter().find(|s| s.id == "note:summary").unwrap().kind,
        "agent_inference_not_approval"
    );
    let mut r = response(&p, &[]);
    r.evidence[0].quote = "invented quotation".into();
    assert!(f.finish(p.clone(), r).is_err());
    let mut r = response(&p, &[]);
    r.context_changes.push(NoteChange::Retire {
        id: "cost".into(),
        request_quote: "replace cost".into(),
    });
    assert!(f.finish(p, r).is_err());
}
#[test]
fn context_changes_require_full_review_even_when_cached_judgments_exist() {
    let f = Fixture::new();
    f.add(policy("P-A"));
    f.open("a");
    let p = f.packet("a", "Do it.");
    f.finish(p.clone(), response(&p, &[])).unwrap();
    let p = f.packet("a", "Preserve the names.");
    let mut r = reuse(&p);
    r.context_changes.push(NoteChange::Add {
        id: "names".into(),
        text: p.request.clone(),
        origin: NoteOrigin::UserQuote,
    });
    assert!(f.finish(p.clone(), r.clone()).is_err());
    r.mode = TurnMode::Review;
    assert!(f.finish(p.clone(), r.clone()).is_err());
    r.assessments = response(&p, &[]).assessments;
    f.finish(p, r).unwrap();
}
#[test]
fn clarification_is_a_valid_blocked_outcome_without_state_mutation() {
    let f = Fixture::new();
    f.open("a");
    let p = f.packet("a", "작업을 진행해.");
    let mut r = response(&p, &[]);
    r.clarification_questions
        .push("Which of the existing subjects do you mean?".into());
    let before = fs::read(f.file("a")).unwrap();
    let o = f.finish(p, r).unwrap();
    assert_eq!(o.status, "clarification_required");
    assert!(!o.changed);
    assert_eq!(before, fs::read(f.file("a")).unwrap());
}
#[test]
fn attached_tasks_are_information_and_multiple_tasks_do_not_imply_focus() {
    let f = Fixture::new();
    f.open("a");
    for id in ["T-ONE", "T-TWO"] {
        fs::write(
            f.0.join(format!("{id}.json")),
            serde_json::to_vec(&task(id)).unwrap(),
        )
        .unwrap();
        f.attach("a", &format!("{id}.json"));
    }
    let p = prepare_turn(&f.0, "a", "작업을 진행해.", TurnOptions::default()).unwrap();
    assert!(p.focus.is_none());
    assert_eq!(p.context.as_ref().unwrap().tasks.len(), 2);
    assert!(p
        .sources
        .as_ref()
        .unwrap()
        .iter()
        .any(|s| s.id == "task:T-TWO" && s.kind == "stored_task_not_current_intent"));
    let p = prepare_turn(
        &f.0,
        "a",
        "Finish the second task.",
        TurnOptions {
            focus: SessionFocus::Task("T-TWO".into()),
            ..TurnOptions::default()
        },
    )
    .unwrap();
    let o = f.finish(p.clone(), response(&p, &[])).unwrap();
    assert!(o.instructions.contains("Explicit task focus: T-TWO"));
    let p = prepare_turn(
        &f.0,
        "a",
        "Now something unrelated.",
        TurnOptions {
            focus: SessionFocus::Clear,
            ..TurnOptions::default()
        },
    )
    .unwrap();
    assert!(p.full_review);
    f.finish(p.clone(), response(&p, &[])).unwrap();
    assert!(read_session(&f.0, "a").unwrap().focus.is_none());
}
#[test]
fn task_scoped_rules_are_not_mistaken_for_session_rules() {
    let f = Fixture::new();
    f.open("T-ONE");
    let mut t = task("T-ONE");
    let mut p = policy("P-LOCAL");
    p.item.scope = ItemScope::Task;
    p.item.lifecycle = amh_core::Lifecycle::Active;
    p.applicability = None;
    t.items.push(p);
    fs::write(f.0.join("task.json"), serde_json::to_vec(&t).unwrap()).unwrap();
    f.attach("T-ONE", "task.json");
    let p = f.packet("T-ONE", "Do it.");
    assert!(!f
        .finish(p.clone(), response(&p, &[]))
        .unwrap()
        .active
        .contains(&"P-LOCAL".into()));
    let p = prepare_turn(
        &f.0,
        "T-ONE",
        "Use the explicit task.",
        TurnOptions {
            focus: SessionFocus::Task("T-ONE".into()),
            ..TurnOptions::default()
        },
    )
    .unwrap();
    assert!(f
        .finish(p.clone(), response(&p, &[]))
        .unwrap()
        .active
        .contains(&"P-LOCAL".into()));
}
#[test]
fn changed_task_sources_and_paused_policies_block_old_reuse() {
    let f = Fixture::new();
    f.add(policy("P-A"));
    f.open("a");
    let p = f.packet("a", "Do it.");
    f.finish(p.clone(), response(&p, &["P-A"])).unwrap();
    f.change(PolicyChange::Pause {
        id: "P-A".into(),
        item_revision: 2,
    });
    let p = f.packet("a", "Continue.");
    assert!(p.full_review);
    let o = f.finish(p.clone(), response(&p, &[])).unwrap();
    assert_eq!(o.removed, vec!["P-A"]);
    fs::write(
        f.0.join("task.json"),
        serde_json::to_vec(&task("T")).unwrap(),
    )
    .unwrap();
    f.attach("a", "task.json");
    fs::write(
        f.0.join("task.json"),
        serde_json::to_vec(&task("OTHER")).unwrap(),
    )
    .unwrap();
    assert!(prepare_turn(&f.0, "a", "Continue.", TurnOptions::default()).is_err());
}
#[test]
fn resume_refresh_requires_full_review_and_no_other_conversation_is_auto_selected() {
    let f = Fixture::new();
    f.add(policy("P-A"));
    f.open("a");
    f.open("b");
    let p = f.packet("a", "Do it.");
    let o = f.finish(p.clone(), response(&p, &[])).unwrap();
    let p = prepare_turn(
        &f.0,
        "a",
        "Continue.",
        TurnOptions {
            known_catalog: Some(o.catalog_digest),
            known_state: Some(o.state_digest),
            refresh: true,
            ..TurnOptions::default()
        },
    )
    .unwrap();
    assert!(p.full_review && p.candidates.is_some() && p.context.is_some());
    assert!(f.finish(p.clone(), reuse(&p)).is_err());
    assert!(read_session(&f.0, "unknown").is_err());
    assert_eq!(read_session(&f.0, "b").unwrap().revision, 0);
}
#[test]
fn closing_session_is_not_task_completion_and_reopening_never_overwrites() {
    let f = Fixture::new();
    f.open("a");
    let close = plan_session(
        &f.0,
        SessionOperation::Close {
            id: "a".into(),
            expected_revision: 0,
        },
    )
    .unwrap();
    apply_session(&f.0, &close).unwrap();
    assert!(f.packet_result("a").is_err());
    assert!(plan_session(&f.0, SessionOperation::Open { id: "a".into() }).is_err());
    assert!(read_session(&f.0, "a").unwrap().closed);
}
impl Fixture {
    fn packet_result(&self, id: &str) -> Result<SessionPacket> {
        prepare_turn(&self.0, id, "Continue.", TurnOptions::default())
    }
}
#[test]
fn session_checks_are_explicit_bound_and_cannot_be_injected_by_analysis() {
    let f = Fixture::new();
    f.open("a");
    let mut t = task("T-CHECK");
    t.checks.push(Check {
        id: "list".into(),
        argv: vec![
            std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            "--list".into(),
        ],
        timeout_seconds: 10,
        expected_exit_code: 0,
    });
    fs::write(f.0.join("task.json"), serde_json::to_vec(&t).unwrap()).unwrap();
    f.attach("a", "task.json");
    let p = prepare_turn(
        &f.0,
        "a",
        "Check this task.",
        TurnOptions {
            focus: SessionFocus::Task(t.id.clone()),
            ..TurnOptions::default()
        },
    )
    .unwrap();
    let mut r = serde_json::to_value(response(&p, &[])).unwrap();
    r["checks"] = serde_json::json!([]);
    assert!(serde_json::from_value::<SessionResponse>(r).is_err());
    f.finish(p.clone(), response(&p, &[])).unwrap();
    let preview = verify_session(&f.0, "a", false).unwrap();
    assert_eq!(preview.checks[0].status, "not_run");
    assert!(!f.0.join(".amh/evidence").exists());
    let e = verify_session(&f.0, "a", true).unwrap();
    assert_eq!(e.all_checks_passed, Some(true));
    assert_eq!(e.policy_compliance, "unknown");
    assert!(e.task_id.contains("session:a/task:T-CHECK"));
}
#[test]
fn both_old_bridges_upgrade_and_uninstall_preserves_session_state() {
    for bridge in [LEGACY_AGENT_BRIDGE, TASK_AGENT_BRIDGE] {
        let f = Fixture::new();
        fs::write(f.0.join("AGENTS.md"), format!("User text{bridge}")).unwrap();
        assert!(doctor(&f.0).unwrap().adapter_update_available);
        apply(
            &f.0,
            &plan(
                &f.0,
                Request::Init {
                    project_id: "session-test".into(),
                    user_root: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
        let text = fs::read_to_string(f.0.join("AGENTS.md")).unwrap();
        assert!(text.contains("amh session prepare"));
        assert!(text.starts_with("User text"));
        f.open("a");
        let before = fs::read(f.file("a")).unwrap();
        apply(&f.0, &plan(&f.0, Request::Uninstall).unwrap()).unwrap();
        assert_eq!(
            fs::read_to_string(f.0.join("AGENTS.md")).unwrap(),
            "User text"
        );
        assert_eq!(before, fs::read(f.file("a")).unwrap());
    }
}
#[cfg(unix)]
#[test]
fn unsafe_ids_and_symlinks_never_write_outside_session_storage() {
    let f = Fixture::new();
    for id in ["../escape", "a/b", ".", ""] {
        assert!(plan_session(&f.0, SessionOperation::Open { id: id.into() }).is_err());
    }
    let outside = Fixture::new();
    std::os::unix::fs::symlink(&outside.0, f.0.join(".amh/sessions")).unwrap();
    assert!(plan_session(&f.0, SessionOperation::Open { id: "a".into() }).is_err());
    assert!(!outside.0.join("a").exists());
}

#[test]
fn same_session_writers_use_revision_guards_and_distinct_sessions_do_not_overwrite() {
    use std::sync::{Arc, Barrier};
    let f = Fixture::new();
    f.open("a");
    f.open("b");
    let make = |id: &str, text: &str| {
        let p = f.packet(id, text);
        let r = response(&p, &[]);
        plan_session(
            &f.0,
            SessionOperation::Turn {
                packet: Box::new(p),
                response: Box::new(r),
            },
        )
        .unwrap()
    };
    let plans = [
        make("a", "First request."),
        make("a", "Concurrent request."),
    ];
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = plans
        .into_iter()
        .map(|p| {
            let root = f.0.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                apply_session(&root, &p).is_ok()
            })
        })
        .collect();
    let succeeded = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|ok| *ok)
        .count();
    assert_eq!(succeeded, 1);
    let a_before = fs::read(f.file("a")).unwrap();
    let p = make("b", "The other conversation.");
    apply_session(&f.0, &p).unwrap();
    assert_eq!(a_before, fs::read(f.file("a")).unwrap());
    let bridge = fs::read_to_string(f.0.join("AGENTS.md")).unwrap();
    assert!(!bridge.contains("First request."));
}
#[test]
fn broken_cached_decisions_and_selection_are_not_silently_reused() {
    let f = Fixture::new();
    f.add(policy("P-A"));
    f.open("a");
    let p = f.packet("a", "Do it.");
    f.finish(p.clone(), response(&p, &["P-A"])).unwrap();
    let original = fs::read(f.file("a")).unwrap();
    let mut state: SessionState = serde_json::from_slice(&original).unwrap();
    state.assessments.clear();
    fs::write(f.file("a"), serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(f.packet_result("a").is_err());
    let mut state: SessionState = serde_json::from_slice(&original).unwrap();
    state.selected.clear();
    fs::write(f.file("a"), serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(f.packet_result("a").is_err());
}
#[test]
fn oversized_context_fails_instead_of_truncating_accepted_conditions() {
    let f = Fixture::new();
    f.open("a");
    let text = "x".repeat(20_000);
    let p = f.packet("a", &text);
    let mut r = response(&p, &[]);
    r.context_changes.push(NoteChange::Add {
        id: "large".into(),
        text,
        origin: NoteOrigin::UserQuote,
    });
    let before = fs::read(f.file("a")).unwrap();
    assert!(f.finish(p, r).is_err());
    assert_eq!(before, fs::read(f.file("a")).unwrap());
}

#[test]
fn unacknowledged_conversation_context_requires_full_review_and_full_delivery() {
    let f = Fixture::new();
    f.add(policy("P-A"));
    let mut always = policy("P-ALWAYS");
    always.applicability = None;
    f.add(always);
    f.open("a");
    let p = f.packet("a", "Do it.");
    f.finish(p.clone(), response(&p, &["P-A"])).unwrap();
    let p = prepare_turn(&f.0, "a", "Explain this.", TurnOptions::default()).unwrap();
    assert!(p.full_review);
    assert!(p.context.is_some() && p.candidates.is_some());
    assert!(f.finish(p.clone(), reuse(&p)).is_err());
    let mut incomplete = response(&p, &["P-A"]);
    incomplete.assessments.clear();
    assert!(f.finish(p.clone(), incomplete).is_err());
    let out = f.finish(p.clone(), response(&p, &["P-A"])).unwrap();
    assert!(out.instructions.contains("## P-A"));
    assert!(out.instructions.contains("## P-ALWAYS"));
}
