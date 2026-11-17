use crate::{
    digest,
    workspace::{path, require_current},
    Result, Task,
};
use serde::Serialize;
use std::{
    fs::{self, File},
    path::Path,
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
    pub task_digest: String,
    pub source_digest: String,
    pub started_at: u64,
    pub completed_at: u64,
    pub checks: Vec<CheckResult>,
    pub all_checks_passed: Option<bool>,
    pub completion_criteria: String,
    pub policy_compliance: String,
    pub authority: String,
    pub deployment_unchanged: bool,
    pub file: Option<String>,
}

/// Invoke only explicitly selected existing verification tools. No model runner,
/// scheduler, retries, or inferred semantic acceptance is provided.
pub fn verify(root: &Path, task: &Task, execute: bool) -> Result<Evidence> {
    let compiled = require_current(root, task)?;
    let now = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())
    };
    let started = now()?;
    let mut evidence = Evidence {
        task_id: task.id.clone(),
        task_digest: digest(&serde_json::to_vec(task).map_err(|e| e.to_string())?),
        source_digest: compiled.source_digest,
        started_at: started.as_secs(),
        completed_at: started.as_secs(),
        checks: vec![],
        all_checks_passed: None,
        completion_criteria: "unknown".into(),
        policy_compliance: "unknown".into(),
        authority: "local_execution_record_not_authenticated".into(),
        deployment_unchanged: true,
        file: None,
    };
    let dir = format!(
        ".amh/evidence/{}-{}",
        started.as_nanos(),
        std::process::id()
    );
    if execute && !task.checks.is_empty() {
        let parent = path(root, ".amh/evidence")?;
        fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        fs::create_dir(path(root, &dir)?).map_err(|e| e.to_string())?;
    }
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
        if execute {
            let stdout = format!("{dir}/{index}.stdout");
            let stderr = format!("{dir}/{index}.stderr");
            let mut command = Command::new(&check.argv[0]);
            command
                .args(&check.argv[1..])
                .current_dir(root)
                .stdin(Stdio::null())
                .stdout(File::create_new(path(root, &stdout)?).map_err(|e| e.to_string())?)
                .stderr(File::create_new(path(root, &stderr)?).map_err(|e| e.to_string())?);
            result.stdout = Some(stdout);
            result.stderr = Some(stderr);
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
    evidence.deployment_unchanged = require_current(root, task).is_ok();
    if execute && !task.checks.is_empty() {
        evidence.all_checks_passed = Some(
            evidence.deployment_unchanged && evidence.checks.iter().all(|r| r.status == "passed"),
        );
        let file = format!("{dir}/result.json");
        evidence.file = Some(file.clone());
        let output = serde_json::to_vec_pretty(&evidence).map_err(|e| e.to_string())?;
        use std::io::Write;
        let mut record = File::create_new(path(root, &file)?).map_err(|e| e.to_string())?;
        record.write_all(&output).map_err(|e| e.to_string())?;
        record.sync_all().map_err(|e| e.to_string())?;
    }
    Ok(evidence)
}
