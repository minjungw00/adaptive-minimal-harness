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
    assert_eq!(f.ok(&["policy", "discover"])["status"], "not_registered");
    assert!(!f.1.exists());
    f.ok(&[
        "pack",
        "install",
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
    let project = f.ok(&["policy", "discover", "--root", subdirectory]);
    assert_eq!(project["status"], "ready");
    assert!(project["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "P-DEBUG" && c["scope"] == "user"));
    assert_eq!(
        f.ok(&["policy", "discover", "--root", alias])["status"],
        "not_registered"
    );
    f.ok(&["project", "add", "--root", alias, "--id", "code", "--apply"]);
    let second = f.ok(&["policy", "discover", "--root", alias]);
    assert_eq!(second["candidates"], project["candidates"]);
    assert_ne!(second["catalog_digest"], project["catalog_digest"]);
    f.ok(&["project", "remove", "--root", alias, "--apply"]);
    assert!(!f
        .stdin(
            &["policy", "read", "--root", alias, "--selection"],
            &selection(&second, &[])
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
fn current_v1_import_preserves_event_history() {
    let f = Fixture::new();
    f.ok(&["project", "add", "--id", "source", "--apply"]);
    f.ok(&[
        "pack",
        "install",
        "inquiry",
        "--reason",
        "Original reason",
        "--apply",
    ]);
    let original = f.ok(&["history", "--full"]);
    assert_eq!(original["format"], "amh.policy-registry.v1");
    let exported = f.2.join("registry.json");
    let original_bytes = serde_json::to_vec(&original).unwrap();
    fs::write(&exported, &original_bytes).unwrap();
    f.ok(&["project", "remove", "--apply"]);
    f.ok(&["project", "add", "--id", "restored", "--apply"]);
    let source = exported.to_str().unwrap();
    f.ok(&["policy", "import", "--file", source, "--apply"]);
    let restored = f.ok(&["history", "--full"]);
    assert_eq!(restored["project_id"], "restored");
    assert_eq!(restored["events"], original["events"]);
    assert_eq!(
        f.ok(&["policy", "import", "--file", source, "--apply"])["status"],
        "unchanged"
    );
    assert_eq!(fs::read(&exported).unwrap(), original_bytes);
}

#[test]
fn previous_typed_v1_fails_closed_and_keeps_the_original_bytes() {
    let f = Fixture::new();
    f.ok(&["project", "add", "--apply"]);
    let registry = f.1.join("projects/repo/policies.toml");
    let old = r#"format = "amh.policy-registry.v1"
project_id = "repo"
[[events]]
revision = 1
reason = "Fixture written by previous binary"
[events.change]
operation = "ADD"
[events.change.policy]
owner = "user"
[events.change.policy.item]
id = "P-OLD"
kind = "principle"
scope = "project"
content = "A previously active policy"
lifecycle = "active"
revision = 1
[events.change.policy.item.details]
type = "principle"
strength = "must"
[events.change.policy.item.details.activation]
mode = "always"
"#;
    let source = f.2.join("old.toml");
    fs::write(&source, old).unwrap();
    assert!(!f
        .run(&[
            "policy",
            "import",
            "--file",
            source.to_str().unwrap(),
            "--apply"
        ])
        .status
        .success());
    fs::write(&registry, old).unwrap();
    for args in [
        &["policy", "discover"][..],
        &[
            "policy",
            "add",
            "--id",
            "P-NEW",
            "--text",
            "A new policy",
            "--apply",
        ],
    ] {
        let result = f.run(args);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("invalid policy registry"));
    }
    assert_eq!(fs::read(&registry).unwrap(), old.as_bytes());
}

fn selection(discovery: &Value, decisions: &[(&str, &str)]) -> Value {
    json!({
        "catalog_digest": discovery["catalog_digest"],
        "decisions": decisions.iter()
            .map(|(id, applicability)| json!({"id": id, "applicability": applicability}))
            .collect::<Vec<_>>()
    })
}

fn delivered(f: &Fixture, value: &Value) -> Value {
    let output = f.stdin(&["policy", "read", "--selection"], value);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn user_setup_and_delivery_use_no_task_or_request_files() {
    let f = Fixture::new();
    fs::write(f.0.join("AGENTS.md"), "User instructions without newline").unwrap();
    assert_eq!(f.ok(&["project", "add"])["status"], "preview");
    assert!(!f.1.exists());
    let plan = f.ok(&["project", "add", "--plan"]);
    assert_eq!(f.ok(&["project", "list"])["repositories"], json!([]));
    let output = f.stdin(&["apply", "--plan", "-"], &plan);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        f.ok(&["project", "list"])["repositories"][0]["project"],
        "repo"
    );

    f.ok(&[
        "policy",
        "add",
        "--file",
        &example("evidence-policy.toml"),
        "--apply",
    ]);
    let immediate = f.ok(&["policy", "discover"]);
    assert_eq!(immediate["status"], "resolved");
    assert_eq!(immediate["selected"], json!(["P-EVIDENCE"]));
    f.ok(&[
        "policy",
        "add",
        "--id",
        "P-WRITING",
        "--text",
        "Use direct writing.",
        "--when",
        "Editing reports",
        "--strength",
        "should",
        "--apply",
    ]);
    let discovery = f.ok(&["policy", "discover"]);
    assert_eq!(discovery["status"], "ready");
    assert!(discovery.get("request").is_none());
    assert!(discovery.get("response_schema").is_none());
    assert_eq!(discovery["candidates"][0]["condition"], "Editing reports");
    assert!(discovery["candidates"][0].get("content").is_none());
    let digest = discovery["catalog_digest"].as_str().unwrap();
    let detail = f.ok(&["policy", "read", "P-WRITING", "--catalog-digest", digest]);
    assert_eq!(detail["content"], "Use direct writing.");
    let selection = selection(&discovery, &[("P-WRITING", "match")]);
    assert_eq!(
        delivered(&f, &selection)["selected"],
        json!(["P-EVIDENCE", "P-WRITING"])
    );
    f.ok(&[
        "policy",
        "change",
        "P-WRITING",
        "--strength",
        "must",
        "--apply",
    ]);
    assert_eq!(
        f.ok(&["policy", "discover"])["candidates"][0]["strength"],
        "must"
    );
    f.ok(&["policy", "change", "P-WRITING", "--pause", "--apply"]);
    let stale = f.stdin(&["policy", "read", "--selection"], &selection);
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("stale policy catalogue"));
    assert_eq!(
        f.ok(&["policy", "discover"])["selected"],
        json!(["P-EVIDENCE"])
    );
    assert_eq!(f.ok(&["history"])["events"].as_array().unwrap().len(), 4);
    assert_eq!(
        fs::read_to_string(f.0.join("AGENTS.md")).unwrap(),
        "User instructions without newline"
    );
    assert!(!f.0.join(".amh").exists());
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 2);
}

#[test]
fn policy_setup_rejects_new_cross_scope_duplicates_and_leaves_active_view_fresh() {
    let f = Fixture::new();
    f.ok(&["project", "add", "--apply"]);
    let pending = f.ok(&["pack", "install", "development", "--plan"]);
    f.ok(&[
        "pack",
        "install",
        "development",
        "--scope",
        "user",
        "--apply",
    ]);
    let rejected = f.stdin(&["apply", "--plan", "-"], &pending);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("both user and project repo scopes"));
    let before = f.ok(&["policy", "discover"]);
    let fail = f.run(&["pack", "install", "development", "--apply"]);
    assert!(!fail.status.success());
    let message = String::from_utf8_lossy(&fail.stderr);
    assert!(message.contains("both user and project repo scopes"));
    assert_eq!(
        f.ok(&["policy", "list", "--scope", "project"])["registries"][0]["revision"],
        0
    );
    assert_eq!(f.ok(&["policy", "discover"]), before);
    f.ok(&[
        "policy",
        "add",
        "--id",
        "P-API",
        "--text",
        "Local API practice.",
        "--draft",
        "--apply",
    ]);
    assert_eq!(f.ok(&["policy", "discover"]), before);
    let failed = f.run(&["policy", "change", "P-API", "--activate", "--apply"]);
    assert!(!failed.status.success());
    let list = f.ok(&["policy", "list"]);
    assert_eq!(list["registries"].as_array().unwrap().len(), 2);
    assert!(list["policies"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| { p["id"] == "P-API" && p["scope"] == "project" && p["lifecycle"] == "draft" }));
}

#[test]
fn planned_changes_survive_unrelated_registration_and_same_names_require_opt_in() {
    let f = Fixture::new();
    f.ok(&["project", "add", "--apply"]);
    let plan = f.ok(&[
        "policy",
        "add",
        "--id",
        "P-NOTES",
        "--text=--example",
        "--plan",
    ]);
    let other = f.2.join("another/repo");
    fs::create_dir_all(&other).unwrap();
    assert!(Command::new("git")
        .current_dir(&other)
        .args(["init", "-q"])
        .status()
        .unwrap()
        .success());
    let path = other.to_str().unwrap();
    let duplicate = f.run(&["project", "add", "--root", path, "--apply"]);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already exists"));
    f.ok(&[
        "project",
        "add",
        "--root",
        path,
        "--id",
        "another-project",
        "--apply",
    ]);
    assert!(f.stdin(&["apply", "--plan", "-"], &plan).status.success());
    assert!(f.ok(&["policy", "discover"])["instructions"]
        .as_str()
        .unwrap()
        .contains("--example"));
}

#[test]
fn help_is_available_and_retired_commands_are_unavailable() {
    let f = Fixture::new();
    let help = f.run(&["policy", "read", "--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--selection"));
    for args in [
        &["policy", "resolve"][..],
        &["policy", "discover", "--text", "old"],
        &["compile"],
        &["request", "finish"],
        &["verify"],
        &["change", "preview"],
        &["state", "inspect"],
    ] {
        assert!(!f.run(args).status.success(), "{args:?}");
    }
}
