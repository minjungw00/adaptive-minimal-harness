use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf, PathBuf, PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "amh-cli-p0-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let repository = root.join("repo");
        fs::create_dir_all(&repository).unwrap();
        assert!(Command::new("git")
            .current_dir(&repository)
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success());
        Self(repository, root.join("home"), root)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_amh"))
            .current_dir(&self.0)
            .env("AMH_HOME", &self.1)
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
        let _ = fs::remove_dir_all(&self.2);
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
    let p = f.ok(&["project", "add", "--id", "report"]);
    assert!(!f.1.exists());
    let plan = f.2.join("plan.json");
    fs::write(&plan, serde_json::to_vec(&p).unwrap()).unwrap();
    f.ok(&["apply", "--plan", plan.to_str().unwrap()]);
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
    let paused = f.ok(&["compile", "--task", &example("writing.toml")]);
    let deployed = paused["content"].as_str().unwrap();
    assert!(!deployed.contains("P-WRITING"));
    assert!(deployed.contains("R-COST"));
    assert_eq!(f.ok(&["history"])["events"].as_array().unwrap().len(), 4);
    f.ok(&["project", "remove", "--apply"]);
    assert_eq!(
        fs::read_to_string(f.0.join("AGENTS.md")).unwrap(),
        "User instructions without newline"
    );
    assert!(f.1.join("projects/report/policies.toml").exists());
    assert!(!f.0.join(".amh").exists());
}

#[test]
fn verification_requires_explicit_execution_and_returns_failure_exit_status() {
    let f = Fixture::new();
    f.ok(&["project", "add", "--id", "checks", "--apply"]);
    let mut task = json!({"id":"T-CHECK", "goal":"Check the installed CLI", "classification_complete":true, "done_when":["CLI responds"], "checks":[{"id":"version", "argv":[env!("CARGO_BIN_EXE_amh"), "--version"], "timeout_seconds":10}]});
    fs::write(f.0.join("task.json"), serde_json::to_vec(&task).unwrap()).unwrap();
    assert_eq!(
        f.ok(&["verify", "--task", "task.json"])["checks"][0]["status"],
        "not_run"
    );
    assert!(!f.1.join("projects/checks/evidence").exists());
    let evidence = f.ok(&["verify", "--task", "task.json", "--run"]);
    assert_eq!(evidence["all_checks_passed"], true);
    assert_eq!(evidence["policy_compliance"], "unknown");
    task["checks"][0]["argv"][1] = "--invalid-command".into();
    fs::write(f.0.join("task.json"), serde_json::to_vec(&task).unwrap()).unwrap();
    let failed = f.run(&["verify", "--task", "task.json", "--run"]);
    assert!(!failed.status.success());
    let evidence: Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(evidence["all_checks_passed"], false);
    let path = PathBuf::from(evidence["file"].as_str().unwrap());
    assert!(path.starts_with(
        fs::canonicalize(&f.1)
            .unwrap()
            .join("projects/checks/evidence")
    ));
    assert!(path.exists());
    assert!(!f.0.join(".amh").exists());
}

#[test]
fn unknown_options_and_incomplete_contracts_fail_without_mutation() {
    let f = Fixture::new();
    assert!(!f
        .run(&["project", "add", "--id", "one", "--id", "two", "--apply"])
        .status
        .success());
    assert!(!f.0.join(".amh").exists());
    f.ok(&["project", "add", "--id", "one", "--apply"]);
    fs::write(f.0.join("task.json"), r#"{"id":"T","goal":"Vague"}"#).unwrap();
    assert!(!f.run(&["compile", "--task", "task.json"]).status.success());
    assert!(!f.0.join(".amh").exists());
}

fn natural_analysis(packet: &Value, selected: &[&str]) -> Value {
    json!({"format":"amh.request-analysis.v1","request_digest":packet["request_digest"],"catalog_digest":packet["catalog_digest"],"done_when":["The user's requested document change is complete"],"assessments":packet["candidates"].as_array().unwrap().iter().map(|p|json!({"id":p["id"],"item_revision":p["item_revision"],"applicability":if selected.contains(&p["id"].as_str().unwrap()){"match"}else{"no_match"},"reason":"This request does or does not satisfy the stated natural-language condition","request_quotes":[packet["request"]]})).collect::<Vec<_>>(),"clarification_questions":[]})
}
#[test]
fn current_agent_handoff_interprets_natural_text_without_manual_tags() {
    let f = Fixture::new();
    f.ok(&["project", "add", "--id", "natural", "--apply"]);
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
    assert_eq!(preview["tags"], json!([]));
    fs::write(f.0.join("task.json"), serde_json::to_vec(&preview).unwrap()).unwrap();
    let compiled = f.ok(&["compile", "--task", "task.json"]);
    assert_eq!(compiled["selected"], json!(["P-WRITING"]));
    assert!(!f.0.join("AGENTS.md").exists());
    assert!(!f.0.join(".amh").exists());
}
#[test]
fn explicitly_selected_interpreter_completes_request_in_one_cli_command() {
    let f = Fixture::new();
    f.ok(&["project", "add", "--id", "one-shot", "--apply"]);
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
    let result = f.ok(&["request", "--text", text, "--agent-config", "agent.json"]);
    assert_eq!(result["goal"], text);
    assert!(!f.0.join(".amh").exists());
    assert_eq!(f.ok(&["policy", "list"])["revision"], 1); // Interpreting work cannot mutate policy authority.
}

impl Fixture {
    fn stdin(&self, args: &[&str], value: &Value) -> Output {
        use std::io::Write;
        use std::process::Stdio;
        let mut child = Command::new(env!("CARGO_BIN_EXE_amh"))
            .current_dir(&self.0)
            .env("AMH_HOME", &self.1)
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
    f.ok(&["project", "add", "--id", "project", "--apply"]);
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
    let path = f.1.join("projects/project/policies.toml");
    let registry = fs::read(&path).unwrap();
    let inventory = fs::read_dir(&f.0).unwrap().count();
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
    assert_eq!(fs::read(path).unwrap(), registry);
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), inventory);
    assert!(!f.0.join(".amh").exists());
    assert!(!f.0.join("AGENTS.md").exists());
}

#[test]
fn stateless_cli_reports_clarification_without_writing() {
    let f = Fixture::new();
    f.ok(&["project", "add", "--id", "project", "--apply"]);
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

#[test]
fn allowlist_binds_exact_work_trees_and_supports_opt_in_aliases() {
    let f = Fixture::new();
    let git = |args: &[&str]| {
        let result = Command::new("git")
            .current_dir(&f.0)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        result.stdout
    };
    fs::write(f.0.join("AGENTS.md"), "# Shared team guidance\n").unwrap();
    git(&["add", "AGENTS.md"]);
    git(&[
        "-c",
        "user.name=fixture",
        "-c",
        "user.email=fixture@example.test",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-qm",
        "Initial fixture",
    ]);
    let alias = f.2.join("review worktree");
    git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        alias.to_str().unwrap(),
        "HEAD",
    ]);
    let alias = alias.to_str().unwrap();
    let subdirectory = f.0.join("src");
    fs::create_dir(&subdirectory).unwrap();
    let subdirectory = subdirectory.to_str().unwrap();
    let before = git(&[
        "--no-optional-locks",
        "status",
        "--porcelain",
        "--untracked-files=all",
    ]);
    assert_eq!(
        f.ok(&["policy", "discover", "--text", "Fix this"])["status"],
        "not_registered"
    );
    assert!(!f.1.exists());
    f.ok(&[
        "pack",
        "install",
        "--name",
        "development",
        "--scope",
        "user",
        "--expected-revision",
        "0",
        "--reason",
        "Shared personal policies",
        "--apply",
    ]);
    f.ok(&[
        "project",
        "add",
        "--root",
        subdirectory,
        "--id",
        "code",
        "--apply",
    ]);
    let project = f.ok(&[
        "policy",
        "discover",
        "--root",
        subdirectory,
        "--text",
        "Fix this",
    ]);
    assert_eq!(project["status"], "ready");
    assert!(project["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "P-DEBUG" && c["scope"] == "user"));
    assert_eq!(
        f.ok(&["policy", "discover", "--root", alias, "--text", "Fix this"])["status"],
        "not_registered"
    );
    f.ok(&["project", "add", "--root", alias, "--id", "code", "--apply"]);
    let second = f.ok(&["policy", "discover", "--root", alias, "--text", "Fix this"]);
    assert_eq!(second["candidates"], project["candidates"]);
    assert_ne!(second["catalog_digest"], project["catalog_digest"]);
    f.ok(&["project", "remove", "--root", alias, "--apply"]);
    assert!(!f
        .stdin(
            &["policy", "resolve", "--root", alias, "--input", "-"],
            &policy_analysis(&second, &[])
        )
        .status
        .success());
    assert!(f.1.join("projects/code/policies.toml").exists());
    assert!(!f.0.join(".amh").exists());
    assert!(!PathBuf::from(alias).join(".amh").exists());
    assert_eq!(
        git(&[
            "--no-optional-locks",
            "status",
            "--porcelain",
            "--untracked-files=all"
        ]),
        before
    );
}

#[test]
fn explicit_import_preserves_events_and_leaves_the_old_store_untouched() {
    let f = Fixture::new();
    f.ok(&["project", "add", "--id", "source", "--apply"]);
    f.ok(&[
        "pack",
        "install",
        "--name",
        "inquiry",
        "--expected-revision",
        "0",
        "--reason",
        "Original reason",
        "--apply",
    ]);
    let original = f.ok(&["history"]);
    let mut source = fs::read_to_string(f.1.join("projects/source/policies.toml")).unwrap();
    source = source.replace("amh.policy-registry.v1", "amh.project-registry.v1");
    source = source.replace(
        "project_id = \"source\"",
        "project_id = \"source\"\nuser_root = \"/unused-local-path\"",
    );
    let old_directory = f.0.join(".amh");
    fs::create_dir(&old_directory).unwrap();
    let old = old_directory.join("amh.toml");
    fs::write(&old, &source).unwrap();
    let agents = f.0.join("AGENTS.md");
    fs::write(&agents, "User content").unwrap();
    f.ok(&["project", "remove", "--apply"]);
    assert_eq!(
        f.ok(&["policy", "discover", "--text", "Check policy"])["status"],
        "not_registered"
    );
    f.ok(&["project", "add", "--id", "destination", "--apply"]);
    f.ok(&[
        "policy",
        "import",
        "--file",
        old.to_str().unwrap(),
        "--apply",
    ]);
    let imported = f.ok(&["history"]);
    assert_eq!(imported["events"], original["events"]);
    assert_eq!(imported["project_id"], "destination");
    fs::write(&old, source.replace("Original reason", "Changed source")).unwrap();
    let edited = fs::read(&old).unwrap();
    assert!(!f
        .run(&[
            "policy",
            "import",
            "--file",
            old.to_str().unwrap(),
            "--apply"
        ])
        .status
        .success());
    assert_eq!(f.ok(&["history"])["events"], original["events"]);
    assert_eq!(fs::read(&old).unwrap(), edited);
    assert_eq!(fs::read_to_string(agents).unwrap(), "User content");
}
