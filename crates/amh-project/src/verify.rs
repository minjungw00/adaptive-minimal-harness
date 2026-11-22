use crate::{compile, digest, read_document, Home, Project, Result, Task};
use serde::Serialize;
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub id: String,
    pub argv: Vec<String>,
    pub status: String,
    pub exit_code: Option<i32>,
    pub expected_exit_code: i32,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Evidence {
    pub task_id: String,
    pub project_id: String,
    pub work_tree: PathBuf,
    pub task_digest: String,
    pub source_digest: String,
    pub started_at: u64,
    pub completed_at: u64,
    pub checks: Vec<CheckResult>,
    pub all_checks_passed: Option<bool>,
    pub completion_criteria: String,
    pub policy_compliance: String,
    pub authority: String,
    pub context_unchanged: bool,
    pub file: Option<String>,
}

/// Invoke only explicitly selected existing verification tools. No model runner,
/// scheduler, retries, or inferred semantic acceptance is provided.
pub fn verify(home: &Home, binding: &Project, task_file: &Path, execute: bool) -> Result<Evidence> {
    let task: Task = read_document(task_file)?;
    let compiled = compile(home, binding, &task, None)?;
    let bound = || -> Result<String> {
        let current: Task = read_document(task_file)?;
        Ok(compile(home, binding, &current, None)?.source_digest)
    };
    let now = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())
    };
    let started = now()?;
    let mut evidence = Evidence {
        task_id: task.id.clone(),
        project_id: binding.project.clone(),
        work_tree: binding.root.clone(),
        task_digest: digest(&serde_json::to_vec(&task).map_err(|e| e.to_string())?),
        source_digest: compiled.source_digest,
        started_at: started.as_secs(),
        completed_at: started.as_secs(),
        checks: vec![],
        all_checks_passed: None,
        completion_criteria: "unknown".into(),
        policy_compliance: "unknown".into(),
        authority: "local_execution_record_not_authenticated".into(),
        context_unchanged: true,
        file: None,
    };
    let dir = if execute && !task.checks.is_empty() {
        let parent = home.create_storage_dir(&format!("projects/{}/evidence", binding.project))?;
        let dir = parent.join(format!("{}-{}", started.as_nanos(), std::process::id()));
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        Some(dir)
    } else {
        None
    };
    for (index, check) in task.checks.iter().enumerate() {
        let mut result = CheckResult {
            id: check.id.clone(),
            argv: check.argv.clone(),
            status: "not_run".into(),
            exit_code: None,
            expected_exit_code: check.expected_exit_code,
            stdout: None,
            stderr: None,
        };
        if let Some(dir) = &dir {
            let stdout = dir.join(format!("{index}.stdout"));
            let stderr = dir.join(format!("{index}.stderr"));
            let mut command = Command::new(&check.argv[0]);
            command
                .args(&check.argv[1..])
                .current_dir(&binding.root)
                .stdin(Stdio::null())
                .stdout(File::create_new(&stdout).map_err(|e| e.to_string())?)
                .stderr(File::create_new(&stderr).map_err(|e| e.to_string())?);
            result.stdout = Some(stdout.to_string_lossy().into_owned());
            result.stderr = Some(stderr.to_string_lossy().into_owned());
            match command.spawn() {
                Err(error) => result.status = format!("could_not_start: {error}"),
                Ok(mut child) => {
                    let start = Instant::now();
                    loop {
                        match child.try_wait().map_err(|e| e.to_string())? {
                            Some(status) => {
                                result.status =
                                    if status.code() == Some(check.expected_exit_code) {
                                        "passed"
                                    } else {
                                        "failed"
                                    }
                                    .into();
                                result.exit_code = status.code();
                                break;
                            }
                            None if start.elapsed()
                                >= Duration::from_secs(check.timeout_seconds) =>
                            {
                                let _ = child.kill();
                                let _ = child.wait();
                                result.status = "timed_out".into();
                                break;
                            }
                            None => std::thread::sleep(Duration::from_millis(20)),
                        }
                    }
                }
            }
        }
        evidence.checks.push(result);
    }
    evidence.completed_at = now()?.as_secs();
    evidence.context_unchanged = bound().is_ok_and(|current| current == evidence.source_digest);
    if let Some(dir) = dir {
        evidence.all_checks_passed = Some(
            evidence.context_unchanged && evidence.checks.iter().all(|r| r.status == "passed"),
        );
        let file = dir.join("result.json");
        evidence.file = Some(file.to_string_lossy().into_owned());
        let output = serde_json::to_vec_pretty(&evidence).map_err(|e| e.to_string())?;
        use std::io::Write;
        let mut record = File::create_new(file).map_err(|e| e.to_string())?;
        record.write_all(&output).map_err(|e| e.to_string())?;
        record.sync_all().map_err(|e| e.to_string())?;
    }
    Ok(evidence)
}
