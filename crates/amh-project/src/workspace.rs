use crate::{digest, resolve, Compilation, Policy, PolicyChange, Registry, Result, Task};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const REGISTRY: &str = ".amh/amh.toml";
const GENERATED: &str = ".amh/generated/AGENTS.md";
const MANIFEST: &str = ".amh/deployment.json";
const JOURNAL: &str = ".amh/transaction.json";
pub const LEGACY_AGENT_BRIDGE: &str = "\n\n<!-- AMH:BEGIN -->\nFor the current AMH task contract, read `.amh/generated/AGENTS.md`.\nIf no task is compiled or the deployment is stale, run `amh doctor` and compile the intended task before relying on it.\n<!-- AMH:END -->\n";
pub const AGENT_BRIDGE: &str = "\n\n<!-- AMH:BEGIN -->\nAMH agent workflow for substantive user requests (not AMH protocol calls):\n1. Save the user's verbatim request to a temporary UTF-8 file. Run `amh request --request-file PATH` and save its JSON packet to a temporary file. Do not ask the user for tags or policy IDs.\n2. As the current agent, interpret the request against EVERY candidate's activation condition and exclusions. Return the packet's response_schema with match/no_match/uncertain judgments, reasons and exact request quotations; keep accepted conditions. Do not start a second agent.\n3. Save your response JSON to a temporary file and run `amh request finish --packet PACKET.json --analysis RESPONSE.json --apply`. If AMH reports material uncertainty, conflicting mandatory conditions or clarification questions, resolve those before acting. Do not invent certainty.\n4. Read `.amh/generated/AGENTS.md`, then perform the original user task using the selected principles. Reinterpret materially changed scope instead of reusing a previous classification. Compilation is not completion of the user task.\nThese instructions manage local task context, not authentication or OS permissions.\n<!-- AMH:END -->\n";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Init {
        project_id: String,
        user_root: Option<PathBuf>,
    },
    Change {
        expected_revision: u64,
        reason: String,
        change: PolicyChange,
    },
    Restore {
        expected_revision: u64,
        revision: u64,
        reason: String,
    },
    Sync {
        task: Task,
        budget: Option<usize>,
    },
    SyncFile {
        task_file: PathBuf,
        budget: Option<usize>,
    },
    Uninstall,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileChange {
    pub path: String,
    pub ownership: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub format: String,
    pub root: PathBuf,
    pub request: Request,
    pub source_digest: String,
    pub changes: Vec<FileChange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Deployment {
    format: String,
    generated_digest: String,
    created_agents: bool,
    task: Option<Task>,
    #[serde(default)]
    task_file: Option<PathBuf>,
    budget: Option<usize>,
    source_digest: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Diagnosis {
    pub project_id: String,
    pub revision: u64,
    pub deployment: String,
    pub observed: String,
    pub adapter_update_available: bool,
    pub note: String,
    pub instruction_files: Vec<String>,
}

fn root_path(root: &Path) -> Result<PathBuf> {
    let root = fs::canonicalize(root).map_err(|e| format!("project root must exist: {e}"))?;
    if !root.is_dir() {
        return Err("project root must be a directory".into());
    }
    Ok(root)
}

fn root_path_file(root: &Path, file: &Path) -> Result<PathBuf> {
    fs::canonicalize(if file.is_absolute() {
        file.to_path_buf()
    } else {
        root.join(file)
    })
    .map_err(|e| format!("task source is unavailable: {e}"))
}

pub(crate) fn path(root: &Path, relative: &str) -> Result<PathBuf> {
    let rel = Path::new(relative);
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("managed path must stay within the project".into());
    }
    let mut current = root.to_path_buf();
    for part in rel.components() {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(format!("symlink in managed path: {}", current.display()))
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
    }
    Ok(current)
}

fn read(root: &Path, relative: &str) -> Result<Option<String>> {
    match fs::read_to_string(path(root, relative)?) {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {relative}: {e}")),
    }
}

pub fn load(root: &Path) -> Result<Registry> {
    let root = root_path(root)?;
    if read(&root, JOURNAL)?.is_some() {
        return Err("interrupted transaction; recover before reading the registry".into());
    }
    let content = read(&root, REGISTRY)?.ok_or("project is not initialized; run amh init")?;
    let registry: Registry =
        toml::from_str(&content).map_err(|e| format!("invalid registry: {e}"))?;
    registry.policies()?;
    Ok(registry)
}

fn input_policies(root: &Path) -> Result<Vec<Policy>> {
    let registry = load(root)?;
    let mut result = Vec::new();
    if let Some(user_root) = &registry.user_root {
        let user = load(user_root)?;
        if user.user_root.is_some() {
            return Err(
                "nested user registries are not supported; choose one explicit user registry"
                    .into(),
            );
        }
        for policy in user.policies()? {
            if policy.item.scope != amh_core::ItemScope::User {
                return Err("imported user registry contains a non-user-scoped policy".into());
            }
            result.push(policy);
        }
    }
    result.extend(registry.policies()?);
    Ok(result)
}

pub fn policies(root: &Path) -> Result<Vec<Policy>> {
    input_policies(root)
}

pub fn compile(root: &Path, task: &Task, budget: Option<usize>) -> Result<Compilation> {
    crate::natural::validate_interpreted_task(root, task)?;
    resolve(&input_policies(root)?, task, budget)
}

fn decode_deployment(root: &Path) -> Result<Option<Deployment>> {
    read(root, MANIFEST)?
        .map(|text| {
            let d: Deployment = serde_json::from_str(&text)
                .map_err(|e| format!("invalid deployment manifest: {e}"))?;
            if d.format != "amh.deployment.v1" {
                return Err("unknown deployment format".into());
            }
            Ok(d)
        })
        .transpose()
}

fn bridge_valid(agents: &str) -> bool {
    agents.matches("<!-- AMH:BEGIN -->").count() == 1
        && agents.matches("<!-- AMH:END -->").count() == 1
        && (agents.contains(AGENT_BRIDGE) || agents.contains(LEGACY_AGENT_BRIDGE))
}

fn check_owned(root: &Path, d: &Deployment) -> Result<()> {
    let generated = read(root, GENERATED)?.ok_or(
        "managed instructions are missing; preserve files and diagnose before replacing them",
    )?;
    if digest(generated.as_bytes()) != d.generated_digest {
        return Err(
            "managed instructions were edited; preserve or move those edits before syncing".into(),
        );
    }
    if !bridge_valid(&read(root, "AGENTS.md")?.ok_or("shared AGENTS.md is missing")?) {
        return Err("AMH shared instruction block is missing, duplicated, or edited".into());
    }
    Ok(())
}

fn change(
    root: &Path,
    changes: &mut Vec<FileChange>,
    relative: &str,
    after: Option<String>,
    ownership: &str,
) -> Result<()> {
    let before = read(root, relative)?;
    if before != after {
        changes.push(FileChange {
            path: relative.into(),
            ownership: ownership.into(),
            before,
            after,
        });
    }
    Ok(())
}

pub fn plan(root: &Path, request: Request) -> Result<Plan> {
    let root = root_path(root)?;
    if read(&root, JOURNAL)?.is_some() {
        return Err(
            "interrupted transaction exists; run amh recover before planning another update".into(),
        );
    }
    let mut changes = Vec::new();
    let mut inputs = vec![
        read(&root, REGISTRY)?,
        read(&root, MANIFEST)?,
        read(&root, GENERATED)?,
        read(&root, "AGENTS.md")?,
    ];
    match &request {
        Request::Init {
            project_id,
            user_root,
        } => {
            let user_root = user_root
                .as_ref()
                .map(|p| {
                    root_path(&if p.is_absolute() {
                        p.clone()
                    } else {
                        root.join(p)
                    })
                })
                .transpose()?;
            if let Some(user) = &user_root {
                if *user == root {
                    return Err("a project cannot import itself as its user registry".into());
                }
                let imported = load(user)?;
                if imported.user_root.is_some()
                    || imported
                        .policies()?
                        .iter()
                        .any(|p| p.item.scope != amh_core::ItemScope::User)
                {
                    return Err(
                        "user registry must contain only user-scoped entries and no nested import"
                            .into(),
                    );
                }
            }
            if inputs[0].is_some() {
                let r = load(&root)?;
                if &r.project_id != project_id || r.user_root != user_root {
                    return Err("existing project identity/import differs; initialization will not overwrite user configuration".into());
                }
                if let Some(d) = decode_deployment(&root)? {
                    check_owned(&root, &d)?;
                    let agents = read(&root, "AGENTS.md")?.unwrap();
                    change(
                        &root,
                        &mut changes,
                        "AGENTS.md",
                        Some(agents.replacen(LEGACY_AGENT_BRIDGE, AGENT_BRIDGE, 1)),
                        "shared",
                    )?;
                } else {
                    return Err(
                        "project is initialized but adapter is uninstalled; use sync to reinstall"
                            .into(),
                    );
                }
            } else {
                if inputs[1].is_some() || inputs[2].is_some() {
                    return Err(
                        "existing AMH-named files are unowned; refusing to overwrite".into(),
                    );
                }
                let agents = inputs[3].clone().unwrap_or_default();
                if agents.contains("<!-- AMH:") {
                    return Err("existing AMH markers require manual conflict resolution".into());
                }
                let registry = Registry::new(project_id.clone(), user_root)?;
                let initial = "# AMH\n\nNo task has been interpreted. Follow the natural-language agent workflow in the project AGENTS.md, or use `amh sync` for an explicit task.\n".to_string();
                let deployment = Deployment {
                    format: "amh.deployment.v1".into(),
                    generated_digest: digest(initial.as_bytes()),
                    created_agents: inputs[3].is_none(),
                    task: None,
                    task_file: None,
                    budget: None,
                    source_digest: None,
                };
                change(
                    &root,
                    &mut changes,
                    REGISTRY,
                    Some(toml::to_string_pretty(&registry).map_err(|e| e.to_string())?),
                    "user_owned",
                )?;
                change(&root, &mut changes, GENERATED, Some(initial), "amh_managed")?;
                change(
                    &root,
                    &mut changes,
                    "AGENTS.md",
                    Some(agents + AGENT_BRIDGE),
                    "shared",
                )?;
                change(
                    &root,
                    &mut changes,
                    MANIFEST,
                    Some(json(&deployment)?),
                    "amh_managed",
                )?;
            }
        }
        Request::Change {
            expected_revision,
            reason,
            change: operation,
        } => {
            let next =
                load(&root)?.changed(*expected_revision, reason.clone(), operation.clone())?;
            change(
                &root,
                &mut changes,
                REGISTRY,
                Some(toml::to_string_pretty(&next).map_err(|e| e.to_string())?),
                "user_owned",
            )?;
        }
        Request::Restore {
            expected_revision,
            revision,
            reason,
        } => {
            let next = load(&root)?.changed(
                *expected_revision,
                reason.clone(),
                PolicyChange::Restore {
                    revision: *revision,
                },
            )?;
            change(
                &root,
                &mut changes,
                REGISTRY,
                Some(toml::to_string_pretty(&next).map_err(|e| e.to_string())?),
                "user_owned",
            )?;
        }
        Request::Sync { .. } | Request::SyncFile { .. } => {
            let (task, budget, task_file) = match &request {
                Request::Sync { task, budget } => (task.clone(), *budget, None),
                Request::SyncFile { task_file, budget } => {
                    let file = root_path_file(&root, task_file)?;
                    inputs.push(Some(fs::read_to_string(&file).map_err(|e| e.to_string())?));
                    (read_document(&file)?, *budget, Some(file))
                }
                _ => unreachable!(),
            };
            let compiled = compile(&root, &task, budget)?;
            inputs.push(Some(compiled.source_digest.clone()));
            let old = decode_deployment(&root)?;
            let agents = read(&root, "AGENTS.md")?.unwrap_or_default();
            let (agents, created) = if let Some(d) = old {
                check_owned(&root, &d)?;
                (
                    agents.replacen(LEGACY_AGENT_BRIDGE, AGENT_BRIDGE, 1),
                    d.created_agents,
                )
            } else {
                if read(&root, GENERATED)?.is_some() || agents.contains("<!-- AMH:") {
                    return Err("unowned adapter files or markers; refusing to overwrite".into());
                }
                (agents + AGENT_BRIDGE, read(&root, "AGENTS.md")?.is_none())
            };
            let deployment = Deployment {
                format: "amh.deployment.v1".into(),
                generated_digest: digest(compiled.content.as_bytes()),
                created_agents: created,
                task: Some(task),
                task_file,
                budget,
                source_digest: Some(compiled.source_digest),
            };
            change(
                &root,
                &mut changes,
                GENERATED,
                Some(compiled.content),
                "amh_managed",
            )?;
            change(&root, &mut changes, "AGENTS.md", Some(agents), "shared")?;
            change(
                &root,
                &mut changes,
                MANIFEST,
                Some(json(&deployment)?),
                "amh_managed",
            )?;
        }
        Request::Uninstall => {
            load(&root)?;
            if let Some(d) = decode_deployment(&root)? {
                check_owned(&root, &d)?;
                let agents = read(&root, "AGENTS.md")?
                    .unwrap()
                    .replacen(AGENT_BRIDGE, "", 1)
                    .replacen(LEGACY_AGENT_BRIDGE, "", 1);
                let remaining = if agents.is_empty() && d.created_agents {
                    None
                } else {
                    Some(agents)
                };
                change(&root, &mut changes, "AGENTS.md", remaining, "shared")?;
                change(&root, &mut changes, GENERATED, None, "amh_managed")?;
                change(&root, &mut changes, MANIFEST, None, "amh_managed")?;
            }
        }
    }
    Ok(Plan {
        format: "amh.project-plan.v1".into(),
        root,
        source_digest: digest(&serde_json::to_vec(&inputs).map_err(|e| e.to_string())?),
        request,
        changes,
    })
}

fn json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

fn lock(root: &Path) -> Result<File> {
    let p = path(root, ".amh.lock")?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(p)
        .map_err(|e| e.to_string())?;
    file.lock().map_err(|e| e.to_string())?;
    Ok(file)
}

fn write(root: &Path, relative: &str, content: Option<&str>) -> Result<()> {
    let p = path(root, relative)?;
    if let Some(content) = content {
        let parent = p.parent().unwrap();
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        path(root, relative)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let tmp = parent.join(format!(".amh-{}-{nonce}.tmp", std::process::id()));
        let result = (|| {
            let mut opts = OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut f = opts.open(&tmp).map_err(|e| e.to_string())?;
            if let Ok(metadata) = fs::metadata(&p) {
                f.set_permissions(metadata.permissions())
                    .map_err(|e| e.to_string())?;
            }
            f.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            drop(f);
            fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
            sync_directory(parent)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    } else {
        match fs::remove_file(&p) {
            Ok(()) => sync_directory(p.parent().unwrap()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn sync_directory(directory: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(directory)
            .and_then(|d| d.sync_all())
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}

/// Recompute a plan from its typed request, then compare every input and output.
/// Plan JSON is never an arbitrary file-write capability.
pub fn apply(root: &Path, reviewed: &Plan) -> Result<()> {
    let root = root_path(root)?;
    let _lock = lock(&root)?;
    if reviewed.root != root || plan(&root, reviewed.request.clone())? != *reviewed {
        return Err("plan changed or is stale; review a newly generated plan".into());
    }
    if reviewed.changes.is_empty() {
        return Ok(());
    }
    write(&root, JOURNAL, Some(&json(reviewed)?))?;
    for c in &reviewed.changes {
        if read(&root, &c.path)? != c.before {
            return Err("file changed during apply; transaction retained for recovery".into());
        }
        write(&root, &c.path, c.after.as_deref())?;
    }
    write(&root, JOURNAL, None)
}

/// Restore only transaction-owned bytes. Concurrent user edits stop recovery.
pub fn recover(root: &Path, apply_recovery: bool) -> Result<serde_json::Value> {
    let root = root_path(root)?;
    let _lock = if apply_recovery {
        Some(lock(&root)?)
    } else {
        None
    };
    let text = read(&root, JOURNAL)?.ok_or("no interrupted transaction exists")?;
    let p: Plan = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if p.format != "amh.project-plan.v1" || p.root != root {
        return Err("invalid transaction binding".into());
    }
    for c in &p.changes {
        if ![REGISTRY, GENERATED, MANIFEST, "AGENTS.md"].contains(&c.path.as_str()) {
            return Err("invalid transaction path".into());
        }
        let current = read(&root, &c.path)?;
        if current != c.before && current != c.after {
            return Err(format!(
                "{} contains unrelated edits; manual reconciliation required",
                c.path
            ));
        }
    }
    if apply_recovery {
        for c in p.changes.iter().rev() {
            write(&root, &c.path, c.before.as_deref())?;
        }
        write(&root, JOURNAL, None)?;
    }
    Ok(
        serde_json::json!({"status": if apply_recovery { "restored_previous_files" } else { "recoverable_interrupted_update" }, "changes": p.changes.len()}),
    )
}

pub fn doctor(root: &Path) -> Result<Diagnosis> {
    let root = root_path(root)?;
    if read(&root, JOURNAL)?.is_some() {
        return Err("interrupted update; run amh recover".into());
    }
    let registry = load(&root)?;
    let deployment = match decode_deployment(&root)? {
        None => "uninstalled".into(),
        Some(d) => {
            check_owned(&root, &d)?;
            match &d.task {
                None => "uncompiled".into(),
                Some(task) => match compile(
                    &root,
                    &match &d.task_file {
                        Some(file) => read_document(file)?,
                        None => task.clone(),
                    },
                    d.budget,
                ) {
                    Err(e) => format!("blocked: {e}"),
                    Ok(c)
                        if Some(c.source_digest.as_str()) == d.source_digest.as_deref()
                            && digest(c.content.as_bytes()) == d.generated_digest =>
                    {
                        "current".into()
                    }
                    Ok(_) => "stale".into(),
                },
            }
        }
    };
    let instruction_files = [
        "AGENTS.md",
        "CLAUDE.md",
        ".cursor/rules",
        ".github/copilot-instructions.md",
    ]
    .iter()
    .filter(|p| root.join(p).exists())
    .map(|p| (*p).to_string())
    .collect();
    let adapter_update_available =
        read(&root, "AGENTS.md")?.is_some_and(|s| s.contains(LEGACY_AGENT_BRIDGE));
    Ok(Diagnosis { adapter_update_available, project_id: registry.project_id, revision: registry.events.len() as u64, deployment, observed: "unknown".into(), note: "Deployment inspection is not behavioral evaluation. Other instruction files are inventoried, not semantically reconciled. Live sessions must reload updated instructions.".into(), instruction_files })
}

pub(crate) fn require_current(root: &Path, task: &Task) -> Result<Compilation> {
    let compiled = compile(root, task, None)?;
    let d = decode_deployment(root)?.ok_or("sync the task before executing its checks")?;
    if d.task.as_ref() != Some(task) || doctor(root)?.deployment != "current" {
        return Err(
            "the requested task is not the current deployed contract; sync it first".into(),
        );
    }
    Ok(compiled)
}

pub fn read_document<T: serde::de::DeserializeOwned>(file: &Path) -> Result<T> {
    let content = fs::read_to_string(file).map_err(|e| e.to_string())?;
    if file.extension().is_some_and(|e| e == "json") {
        serde_json::from_str(&content).map_err(|e| e.to_string())
    } else {
        toml::from_str(&content).map_err(|e| e.to_string())
    }
}

pub fn pack_locks(registry: &Registry) -> Result<BTreeMap<String, String>> {
    registry.policies()?;
    let mut versions = vec![BTreeMap::new()];
    for event in &registry.events {
        let mut locks = versions.last().unwrap().clone();
        match &event.change {
            PolicyChange::InstallPack { pack } => {
                locks.insert(
                    pack.id.clone(),
                    format!(
                        "{}:{}",
                        pack.version,
                        digest(&serde_json::to_vec(pack).map_err(|e| e.to_string())?)
                    ),
                );
            }
            PolicyChange::Restore { revision } => locks = versions[*revision as usize].clone(),
            _ => {}
        }
        versions.push(locks);
    }
    Ok(versions.pop().unwrap())
}
