use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "amh-cli-p0-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_amh"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let o = self.run(args);
        assert!(
            o.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn example(file: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/p0")
        .join(file)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn reviewed_cli_workflow_preserves_requirements_and_removes_paused_instructions() {
    let f = Fixture::new();
    fs::write(f.0.join("AGENTS.md"), "User instructions without newline").unwrap();
    let p = f.ok(&["init", "--id", "report"]);
    assert!(!f.0.join(".amh").exists());
    fs::write(f.0.join("plan.json"), serde_json::to_vec(&p).unwrap()).unwrap();
    f.ok(&["apply", "--plan", "plan.json"]);
    f.ok(&[
        "pack",
        "install",
        "--name",
        "inquiry",
        "--expected-revision",
        "0",
        "--reason",
        "Use writing principles",
        "--apply",
    ]);
    f.ok(&[
        "policy",
        "add",
        "--file",
        &example("requirement.toml"),
        "--expected-revision",
        "1",
        "--reason",
        "Keep cost analysis",
        "--apply",
    ]);
    f.ok(&[
        "policy",
        "change",
        "--file",
        &example("activate-requirement.json"),
        "--expected-revision",
        "2",
        "--reason",
        "Activate accepted requirement",
        "--apply",
    ]);
    let c = f.ok(&["compile", "--task", &example("writing.toml")]);
    assert_eq!(c["selected"], json!(["P-WRITING", "R-COST"]));
    f.ok(&["sync", "--task", &example("writing.toml"), "--apply"]);
    assert_eq!(f.ok(&["doctor"])["deployment"], "current");
    f.ok(&[
        "policy",
        "change",
        "--file",
        &example("pause-writing.json"),
        "--expected-revision",
        "3",
        "--reason",
        "Pause the writing method",
        "--apply",
    ]);
    assert!(!f.run(&["doctor"]).status.success());
    f.ok(&["sync", "--task", &example("writing.toml"), "--apply"]);
    let deployed = fs::read_to_string(f.0.join(".amh/generated/AGENTS.md")).unwrap();
    assert!(!deployed.contains("P-WRITING"));
    assert!(deployed.contains("R-COST"));
    assert_eq!(f.ok(&["history"])["events"].as_array().unwrap().len(), 4);
    f.ok(&["uninstall", "--apply"]);
    assert_eq!(
        fs::read_to_string(f.0.join("AGENTS.md")).unwrap(),
        "User instructions without newline"
    );
    assert!(f.0.join(".amh/amh.toml").exists());
}

#[test]
fn verification_requires_explicit_execution_and_returns_failure_exit_status() {
    let f = Fixture::new();
    f.ok(&["init", "--id", "checks", "--apply"]);
    let mut task = json!({"id":"T-CHECK", "goal":"Check the installed CLI", "classification_complete":true, "done_when":["CLI responds"], "checks":[{"id":"version", "argv":[env!("CARGO_BIN_EXE_amh"), "--version"], "timeout_seconds":10}]});
    fs::write(f.0.join("task.json"), serde_json::to_vec(&task).unwrap()).unwrap();
    f.ok(&["sync", "--task", "task.json", "--apply"]);
    assert_eq!(
        f.ok(&["verify", "--task", "task.json"])["checks"][0]["status"],
        "not_run"
    );
    assert!(!f.0.join(".amh/evidence").exists());
    let evidence = f.ok(&["verify", "--task", "task.json", "--run"]);
    assert_eq!(evidence["all_checks_passed"], true);
    assert_eq!(evidence["policy_compliance"], "unknown");
    task["checks"][0]["argv"][1] = "--invalid-command".into();
    fs::write(f.0.join("task.json"), serde_json::to_vec(&task).unwrap()).unwrap();
    f.ok(&["sync", "--task", "task.json", "--apply"]);
    let failed = f.run(&["verify", "--task", "task.json", "--run"]);
    assert!(!failed.status.success());
    let evidence: Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(evidence["all_checks_passed"], false);
    assert!(f.0.join(evidence["file"].as_str().unwrap()).exists());
}

#[test]
fn unknown_options_and_incomplete_contracts_fail_without_mutation() {
    let f = Fixture::new();
    assert!(!f
        .run(&["init", "--id", "one", "--id", "two", "--apply"])
        .status
        .success());
    assert!(!f.0.join(".amh").exists());
    f.ok(&["init", "--id", "one", "--apply"]);
    fs::write(f.0.join("task.json"), r#"{"id":"T","goal":"Vague"}"#).unwrap();
    assert!(!f
        .run(&["sync", "--task", "task.json", "--apply"])
        .status
        .success());
    assert_eq!(f.ok(&["doctor"])["deployment"], "uncompiled");
}

fn natural_analysis(packet: &Value, selected: &[&str]) -> Value {
    json!({"format":"amh.request-analysis.v1","request_digest":packet["request_digest"],"catalog_digest":packet["catalog_digest"],"done_when":["The user's requested document change is complete"],"assessments":packet["candidates"].as_array().unwrap().iter().map(|p|json!({"id":p["id"],"item_revision":p["item_revision"],"applicability":if selected.contains(&p["id"].as_str().unwrap()){"match"}else{"no_match"},"reason":"This request does or does not satisfy the stated natural-language condition","request_quotes":[packet["request"]]})).collect::<Vec<_>>(),"clarification_questions":[]})
}
#[test]
fn current_agent_handoff_interprets_natural_text_without_manual_tags() {
    let f = Fixture::new();
    f.ok(&["init", "--id", "natural", "--apply"]);
    f.ok(&[
        "pack",
        "install",
        "--name",
        "inquiry",
        "--expected-revision",
        "0",
        "--reason",
        "Use inquiry methods",
        "--apply",
    ]);
    let request = "기존 보고서의 논지를 유지하면서 결론 문단을 더 명확하게 다듬어 주세요.";
    fs::write(f.0.join("request.txt"), request).unwrap();
    let packet = f.ok(&["request", "--request-file", "request.txt"]);
    assert_eq!(packet["request"], request);
    assert!(!f
        .run(&["request", "--text", request, "--apply"])
        .status
        .success());
    let analysis = natural_analysis(&packet, &["P-WRITING"]);
    fs::write(
        f.0.join("packet.json"),
        serde_json::to_vec(&packet).unwrap(),
    )
    .unwrap();
    fs::write(
        f.0.join("analysis.json"),
        serde_json::to_vec(&analysis).unwrap(),
    )
    .unwrap();
    let preview = f.ok(&[
        "request",
        "finish",
        "--packet",
        "packet.json",
        "--analysis",
        "analysis.json",
    ]);
    assert_eq!(preview["request"]["task"]["tags"], json!([]));
    f.ok(&[
        "request",
        "finish",
        "--packet",
        "packet.json",
        "--analysis",
        "analysis.json",
        "--apply",
    ]);
    let instructions = fs::read_to_string(f.0.join(".amh/generated/AGENTS.md")).unwrap();
    assert!(instructions.contains("P-WRITING"));
    assert!(!instructions.contains("P-SOCRATIC"));
    assert_eq!(f.ok(&["doctor"])["deployment"], "current");
    let agents = fs::read_to_string(f.0.join("AGENTS.md")).unwrap();
    assert!(agents.contains("amh policy discover"));
}
#[test]
fn explicitly_selected_interpreter_completes_request_in_one_cli_command() {
    let f = Fixture::new();
    f.ok(&["init", "--id", "one-shot", "--apply"]);
    f.ok(&[
        "pack",
        "install",
        "--name",
        "inquiry",
        "--expected-revision",
        "0",
        "--reason",
        "Use writing policy",
        "--apply",
    ]);
    let text = "보고서의 결론을 간결하고 명확하게 수정하세요.";
    let packet = f.ok(&["request", "--text", text]);
    fs::write(
        f.0.join("reply.json"),
        serde_json::to_vec(&natural_analysis(&packet, &["P-WRITING"])).unwrap(),
    )
    .unwrap();
    let binary = f.0.join(if cfg!(windows) { "agent.exe" } else { "agent" });
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../amh-project/tests/fixtures/interpreter.rs");
    assert!(Command::new("rustc")
        .arg(source)
        .args(["--edition", "2021"])
        .arg("-o")
        .arg(&binary)
        .status()
        .unwrap()
        .success());
    let config = json!({"adapter":"stdio","argv":[binary,"reply",f.0.join("reply.json"),f.0.join("captured.json")],"timeout_seconds":10});
    fs::write(f.0.join("agent.json"), serde_json::to_vec(&config).unwrap()).unwrap();
    let result = f.ok(&[
        "request",
        "--text",
        text,
        "--agent-config",
        "agent.json",
        "--apply",
    ]);
    assert_eq!(result["status"], "applied_local_plan");
    assert_eq!(result["plan"]["request"]["task"]["goal"], text);
    assert_eq!(f.ok(&["doctor"])["deployment"], "current");
    assert_eq!(f.ok(&["policy", "list"])["revision"], 1); // Interpreting work cannot mutate policy authority.
}

impl Fixture {
    fn stdin(&self, args: &[&str], value: &Value) -> Output {
        use std::io::Write;
        use std::process::Stdio;
        let mut child = Command::new(env!("CARGO_BIN_EXE_amh"))
            .current_dir(&self.0)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(value).unwrap())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

fn policy_analysis(packet: &Value, selected: &[&str]) -> Value {
    let assessments: Vec<_> = packet["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|policy| {
            let id = policy["id"].as_str().unwrap();
            let applicability = if selected.contains(&id) {
                "match"
            } else {
                "no_match"
            };
            json!({
                "id": id, "item_revision": policy["item_revision"], "applicability": applicability,
                "reason": "Compare the requested work to the applicability condition",
                "request_quotes": [packet["request"]]
            })
        })
        .collect();
    json!({
        "format": "amh.policy-analysis.v1", "request": packet["request"],
        "request_digest": packet["request_digest"], "catalog_digest": packet["catalog_digest"],
        "assessments": assessments, "clarification_questions": []
    })
}

#[test]
fn stateless_cli_reads_selected_bodies_and_leaves_no_request_artifacts() {
    let f = Fixture::new();
    f.ok(&["init", "--id", "project", "--apply"]);
    f.ok(&[
        "pack",
        "install",
        "--name",
        "development",
        "--expected-revision",
        "0",
        "--reason",
        "Use development policies",
        "--apply",
    ]);
    let registry = fs::read(f.0.join(".amh/amh.toml")).unwrap();
    let task_contract = fs::read(f.0.join(".amh/generated/AGENTS.md")).unwrap();
    let inventory = fs::read_dir(f.0.join(".amh")).unwrap().count();
    assert!(!f.run(&["session", "open"]).status.success());

    let first = f.ok(&["policy", "discover", "--text", "이 오류를 고쳐줘."]);
    let candidates = first["candidates"].as_array().unwrap();
    assert!(!candidates.is_empty());
    assert!(candidates
        .iter()
        .all(|candidate| candidate.get("content").is_none()));
    let catalogue = first["catalog_digest"].as_str().unwrap();
    let body = f.ok(&[
        "policy",
        "read",
        "--id",
        "P-DEBUG",
        "--catalog-digest",
        catalogue,
    ]);
    assert!(body["item"]["content"]
        .as_str()
        .unwrap()
        .contains("Reproduce the failure"));
    let outcome = f.stdin(
        &["policy", "resolve", "--input", "-"],
        &policy_analysis(&first, &["P-DEBUG", "P-REGRESSION"]),
    );
    assert!(
        outcome.status.success(),
        "{}",
        String::from_utf8_lossy(&outcome.stderr)
    );
    let first_result: Value = serde_json::from_slice(&outcome.stdout).unwrap();
    assert_eq!(first_result["selected"], json!(["P-DEBUG", "P-REGRESSION"]));
    let instructions = first_result["instructions"].as_str().unwrap();
    assert!(instructions.contains("Reproduce the failure"));
    assert!(instructions.contains("Add a regression check"));
    assert!(!instructions.contains("P-MUTATION"));

    let second = f.ok(&["policy", "discover", "--text", "수정 이유를 설명해줘."]);
    let outcome = f.stdin(
        &["policy", "resolve", "--input", "-"],
        &policy_analysis(&second, &[]),
    );
    assert!(
        outcome.status.success(),
        "{}",
        String::from_utf8_lossy(&outcome.stderr)
    );
    let second_result: Value = serde_json::from_slice(&outcome.stdout).unwrap();
    assert_eq!(second_result["selected"], json!([]));
    assert_eq!(second_result["instructions"], "");
    assert_eq!(fs::read(f.0.join(".amh/amh.toml")).unwrap(), registry);
    assert_eq!(
        fs::read(f.0.join(".amh/generated/AGENTS.md")).unwrap(),
        task_contract
    );
    assert_eq!(fs::read_dir(f.0.join(".amh")).unwrap().count(), inventory);
    assert!(!f.0.join(".amh/sessions").exists());
}

#[test]
fn stateless_cli_reports_clarification_without_writing() {
    let f = Fixture::new();
    f.ok(&["init", "--id", "project", "--apply"]);
    let old_state = f.0.join(".amh/sessions/old/state.json");
    fs::create_dir_all(old_state.parent().unwrap()).unwrap();
    fs::write(&old_state, "older user data").unwrap();
    let packet = f.ok(&["policy", "discover", "--text", "작업을 진행해"]);
    let mut analysis = policy_analysis(&packet, &[]);
    analysis["clarification_questions"] = json!(["어떤 작업을 진행할까요?"]);
    let outcome = f.stdin(&["policy", "resolve", "--input", "-"], &analysis);
    assert!(!outcome.status.success());
    let block: Value = serde_json::from_slice(&outcome.stderr).unwrap();
    assert_eq!(block["status"], "clarification_required");
    assert_eq!(
        block["clarification_questions"],
        analysis["clarification_questions"]
    );
    assert_eq!(block["selected"], json!([]));
    assert_eq!(fs::read_to_string(old_state).unwrap(), "older user data");
}
