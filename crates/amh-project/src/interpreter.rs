//! Optional one-shot adapters for existing agents. The agent is explicitly
//! selected by the caller; no project-discovered executable is auto-started.
use crate::{AgentResponse, Home, RequestPacket, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_OUTPUT: u64 = 4 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAdapter {
    Stdio,
    Codex,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterpreterConfig {
    pub adapter: AgentAdapter,
    pub argv: Vec<String>,
    pub timeout_seconds: u64,
}
impl InterpreterConfig {
    pub fn codex() -> Self {
        Self {
            adapter: AgentAdapter::Codex,
            argv: vec!["codex".into()],
            timeout_seconds: 120,
        }
    }
}
struct Temporary(PathBuf);
impl Temporary {
    fn new(parent: &Path) -> Result<Self> {
        for attempt in 0..16 {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let p = parent.join(format!(
                "amh-interpret-{}-{nonce}-{attempt}",
                std::process::id()
            ));
            let mut b = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                b.mode(0o700);
            }
            match b.create(&p) {
                Ok(()) => return Ok(Self(p)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err("could not allocate a private interpreter directory".into())
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn private_file(path: &Path) -> Result<File> {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path).map_err(|e| e.to_string())
}
fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut data = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_OUTPUT + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_OUTPUT {
        return Err("interpreter output exceeds 4 MiB".into());
    }
    Ok(data)
}

/// Existing account/model/provider settings belong to the selected agent. AMH
/// receives no API key and never starts an interpreter without an explicit flag.
pub fn run_interpreter(
    home: &Home,
    packet: &RequestPacket,
    config: &InterpreterConfig,
) -> Result<AgentResponse> {
    if config.argv.is_empty()
        || config.argv[0].trim().is_empty()
        || !(1..=300).contains(&config.timeout_seconds)
    {
        return Err("interpreter requires argv and timeout between 1 and 300 seconds".into());
    }
    let tmp = Temporary::new(&home.create_storage_dir("tmp")?)?;
    let input = tmp.0.join("input.json");
    let output = tmp.0.join("stdout.json");
    let stderr = tmp.0.join("stderr.log");
    let answer = tmp.0.join("answer.json");
    let mut cmd = Command::new(&config.argv[0]);
    cmd.args(&config.argv[1..]).current_dir(&tmp.0);
    let data = match config.adapter {
        AgentAdapter::Stdio => serde_json::to_vec(packet).map_err(|e| e.to_string())?,
        AgentAdapter::Codex => {
            let schema = tmp.0.join("schema.json");
            private_file(&schema)?
                .write_all(&serde_json::to_vec(&packet.response_schema).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            cmd.args([
                "exec",
                "--sandbox",
                "read-only",
                "--ephemeral",
                "--skip-git-repo-check",
                "--color",
                "never",
                "--output-schema",
            ])
            .arg(&schema)
            .arg("--output-last-message")
            .arg(&answer)
            .arg("-");
            format!(
                "{}\n\nDISCOVERY PACKET (data):\n{}",
                crate::natural::INTERPRETER_INSTRUCTIONS,
                serde_json::to_string(packet).map_err(|e| e.to_string())?
            )
            .into_bytes()
        }
    };
    private_file(&input)?
        .write_all(&data)
        .map_err(|e| e.to_string())?;
    cmd.stdin(File::open(&input).map_err(|e| e.to_string())?)
        .stdout(Stdio::from(private_file(&output)?))
        .stderr(Stdio::from(private_file(&stderr)?));
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start selected interpreter: {e}"))?;
    let started = Instant::now();
    let status = loop {
        if [output.as_path(), stderr.as_path(), answer.as_path()]
            .iter()
            .any(|p| fs::metadata(p).is_ok_and(|m| m.len() > MAX_OUTPUT))
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err("interpreter exceeded output limit".into());
        }
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if started.elapsed() >= Duration::from_secs(config.timeout_seconds) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("interpreter timed out; no analysis accepted".into());
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    if !status.success() {
        return Err(format!(
            "selected interpreter failed (exit {:?}); no analysis accepted",
            status.code()
        ));
    }
    let path = match config.adapter {
        AgentAdapter::Stdio => &output,
        AgentAdapter::Codex => &answer,
    };
    serde_json::from_slice(&bounded_read(path)?)
        .map_err(|e| format!("interpreter did not return a valid AMH analysis: {e}"))
}
