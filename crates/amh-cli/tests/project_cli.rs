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
