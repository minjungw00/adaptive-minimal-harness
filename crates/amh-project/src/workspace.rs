//! Policy storage and repository selection in the user's AMH home.
use crate::{digest, policy_lookup::eligible, Policy, PolicyChange, Registry, Result};
use crate::{Lifecycle, Scope};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const CONFIG: &str = "config.toml";
const USER_POLICIES: &str = "user/policies.toml";
const CONFIG_FORMAT: &str = "amh.config.v1";
const PLAN_FORMAT: &str = "amh.home-plan.v2";

pub const AGENT_GUIDANCE: &str = r#"For substantive work, follow the repository's shared instructions. Identify the work from the request, relevant conversation and project evidence. Ask the user directly when ambiguity matters.
If AMH is available, run `amh policy discover` from the workspace before new work or a change in scope. If status is not_registered, proceed with the ordinary project instructions. If status is resolved, use the returned instructions.
If status is ready, judge the conditions using the current work. Use `amh policy read ID --catalog-digest DIGEST` when a summary is insufficient. Include a method only when its applicability condition is satisfied.
Send one JSON object to `amh policy read --selection` on stdin, for example {"catalog_digest":"DIGEST","decisions":[{"id":"ID","applicability":"match"}]}. Report match, no_match or uncertain for every must candidate. Include optional candidates you select as match. Use the returned instructions. Resolve mandatory uncertainty and declared conflicts before relying on an AMH selection.
Reuse relevant policy text during the same work. Refresh when policy, scope or available context changes.
"#;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub root: PathBuf,
    pub project: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub format: String,
    pub repositories: Vec<Project>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            format: CONFIG_FORMAT.into(),
            repositories: vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    User,
    Project { binding: Project },
}

impl Target {
    fn registry_path(&self) -> String {
        match self {
            Self::User => USER_POLICIES.into(),
            Self::Project { binding } => project_registry(&binding.project),
        }
    }

    fn registry_id(&self) -> &str {
        match self {
            Self::User => "user",
            Self::Project { binding } => &binding.project,
        }
    }

    fn accepts(&self, scope: Scope) -> bool {
        matches!(
            (self, scope),
            (Self::User, Scope::User) | (Self::Project { .. }, Scope::Project)
        )
    }

    fn validate(&self, registry: &Registry) -> Result<()> {
        if registry.project_id != self.registry_id() {
            return Err("policy registry identity does not match its scope".into());
        }
        registry.policies()?;
        // Check introductions even if a later event removed the policy.
        for event in &registry.events {
            let policies: &[Policy] = match &event.change {
                PolicyChange::Add { policy }
                | PolicyChange::Replace {
                    replacement: policy,
                    ..
                } => std::slice::from_ref(policy),
                PolicyChange::InstallPack { pack } => &pack.policies,
                _ => &[],
            };
            if policies.iter().any(|p| !self.accepts(p.scope)) {
                return Err("policy scope does not match the selected registry".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Register {
        binding: Project,
        #[serde(default)]
        new_project: bool,
    },
    Unregister {
        binding: Project,
    },
    Change {
        target: Target,
        expected_revision: u64,
        reason: String,
        change: Box<PolicyChange>,
    },
    Import {
        target: Target,
        registry: Registry,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub format: String,
    pub home: PathBuf,
    pub request: Request,
    pub source_digest: String,
    pub files: Vec<String>,
    pub policies: Vec<PolicyEffect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyEffect {
    pub id: String,
    pub before: Option<Lifecycle>,
    pub after: Option<Lifecycle>,
}

#[derive(Debug)]
pub struct Home {
    path: PathBuf,
}

fn project_registry(id: &str) -> String {
    format!("projects/{id}/policies.toml")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-_".contains(&c))
        && !matches!(
            id,
            "con"
                | "prn"
                | "aux"
                | "nul"
                | "com1"
                | "com2"
                | "com3"
                | "com4"
                | "com5"
                | "com6"
                | "com7"
                | "com8"
                | "com9"
                | "lpt1"
                | "lpt2"
                | "lpt3"
                | "lpt4"
                | "lpt5"
                | "lpt6"
                | "lpt7"
                | "lpt8"
                | "lpt9"
        )
}

fn check_id(id: &str) -> Result<()> {
    if valid_id(id) {
        Ok(())
    } else {
        Err("project ID must use 1–64 lowercase ASCII letters, digits, hyphens or underscores and be a portable directory name".into())
    }
}

fn existing_prefix(path: &Path) -> Result<&Path> {
    for ancestor in path.ancestors() {
        if ancestor.try_exists().map_err(|e| e.to_string())? {
            return Ok(ancestor);
        }
    }
    Err("AMH home has no existing parent".into())
}

fn canonical_dir(path: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !path.is_dir() {
        return Err(format!("not a directory: {}", path.display()));
    }
    Ok(path)
}

fn git_location(directory: &Path, option: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .current_dir(directory)
        .args(["rev-parse", option])
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_IMPLICIT_WORK_TREE")
        .env_remove("GIT_CEILING_DIRECTORIES")
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("could not run git to find the repository: {e}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        if error.contains("not a git repository") || error.contains("must be run in a work tree") {
            return Ok(None);
        }
        return Err(format!(
            "could not identify the Git work tree: {}",
            error.trim()
        ));
    }
    let location = String::from_utf8(output.stdout).map_err(|_| "Git path must be UTF-8")?;
    let location = location.strip_suffix('\n').unwrap_or(&location);
    #[cfg(windows)]
    let location = location.strip_suffix('\r').unwrap_or(location);
    Ok(Some(location.into()))
}

pub fn git_root(directory: &Path) -> Result<Option<PathBuf>> {
    let directory = canonical_dir(directory)?;
    let Some(root) = git_location(&directory, "--show-toplevel")? else {
        return Ok(None);
    };
    let root = canonical_dir(Path::new(&root))?;
    if !directory.starts_with(&root) {
        return Err("Git work tree does not contain the requested directory".into());
    }
    Ok(Some(root))
}

impl Home {
    pub fn new(path: &Path) -> Result<Self> {
        if !path.is_absolute() || path.components().any(|p| matches!(p, Component::ParentDir)) {
            return Err("AMH home must be an absolute path without parent components".into());
        }
        let prefix = existing_prefix(path)?;
        let tail = path.strip_prefix(prefix).map_err(|e| e.to_string())?;
        let path = canonical_dir(prefix)?.join(tail);
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn outside(&self, root: &Path) -> Result<()> {
        if self.path.starts_with(root) || root.starts_with(&self.path) {
            return Err("AMH home and registered work trees must be separate".into());
        }
        Ok(())
    }

    fn require_private_location(path: &Path) -> Result<()> {
        if git_location(existing_prefix(path)?, "--absolute-git-dir")?.is_some() {
            return Err("AMH home must be outside Git work trees".into());
        }
        Ok(())
    }

    pub fn config(&self) -> Result<Config> {
        self.decode_config(self.read(CONFIG)?.as_deref())
    }

    fn decode_config(&self, content: Option<&str>) -> Result<Config> {
        let Some(content) = content else {
            return Ok(Config::default());
        };
        let config: Config =
            toml::from_str(content).map_err(|e| format!("invalid AMH configuration: {e}"))?;
        let mut roots = BTreeSet::new();
        if config.format != CONFIG_FORMAT {
            return Err("unsupported AMH configuration format".into());
        }
        for binding in &config.repositories {
            check_id(&binding.project)?;
            if !binding.root.is_absolute()
                || binding
                    .root
                    .components()
                    .any(|p| matches!(p, Component::ParentDir))
                || !roots.insert(&binding.root)
            {
                return Err("AMH configuration has an invalid or duplicate work tree path".into());
            }
            self.outside(&binding.root)?;
        }
        Ok(config)
    }

    pub fn project(&self, directory: &Path) -> Result<Option<Project>> {
        let Some(root) = git_root(directory)? else {
            return Ok(None);
        };
        Ok(self
            .config()?
            .repositories
            .into_iter()
            .find(|p| p.root == root))
    }

    pub fn require_project(&self, directory: &Path) -> Result<Project> {
        self.project(directory)?
            .ok_or_else(|| "repository is not registered; run amh project add".into())
    }

    fn validate_target(&self, target: &Target) -> Result<()> {
        if let Target::Project { binding } = target {
            self.outside(&binding.root)?;
            if self.project(&binding.root)?.as_ref() != Some(binding) {
                return Err(
                    "repository registration changed; resolve the current project again".into(),
                );
            }
        }
        Ok(())
    }

    pub fn registry(&self, target: &Target) -> Result<Registry> {
        self.validate_target(target)?;
        self.read_registry(target, false)
    }

    fn read_registry(&self, target: &Target, allow_missing_project: bool) -> Result<Registry> {
        let path = target.registry_path();
        Self::decode_registry(target, self.read(&path)?.as_deref(), allow_missing_project)
    }

    fn decode_registry(
        target: &Target,
        content: Option<&str>,
        allow_missing_project: bool,
    ) -> Result<Registry> {
        check_id(target.registry_id())?;
        let path = target.registry_path();
        let registry = match content {
            Some(content) => toml::from_str(content)
                .map_err(|e| format!("invalid policy registry at {path}: {e}"))?,
            None if matches!(target, Target::User) || allow_missing_project => {
                Registry::new(target.registry_id().into())?
            }
            None => {
                return Err(format!(
                    "registered project is missing its policy registry: {path}"
                ))
            }
        };
        target.validate(&registry)?;
        Ok(registry)
    }

    pub fn policies(&self, binding: &Project) -> Result<Vec<Policy>> {
        let local = self.registry(&Target::Project {
            binding: binding.clone(),
        })?;
        let mut entries = self.registry(&Target::User)?.policies()?;
        entries.extend(local.policies()?);
        Ok(entries)
    }

    fn read(&self, relative: &str) -> Result<Option<String>> {
        match fs::read_to_string(self.storage_path(relative)?) {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("cannot read {relative}: {e}")),
        }
    }

    pub(crate) fn storage_path(&self, relative: &str) -> Result<PathBuf> {
        let mut current = self.path.clone();
        for part in Path::new(relative).components() {
            let Component::Normal(part) = part else {
                return Err("managed path must stay within AMH home".into());
            };
            current.push(part);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(format!("symlink in AMH storage: {}", current.display()));
                }
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
                _ => {}
            }
        }
        Ok(current)
    }

    pub fn project_id_for(root: &Path) -> Result<String> {
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("cannot infer a project ID; provide --id")?;
        let id: String = name
            .chars()
            .map(|c| match c {
                'a'..='z' | '0'..='9' | '-' | '_' => c,
                'A'..='Z' => c.to_ascii_lowercase(),
                _ => '-',
            })
            .collect();
        if !valid_id(&id) || !id.bytes().any(|c| c.is_ascii_alphanumeric()) {
            return Err("cannot infer a valid project ID; provide --id".into());
        }
        Ok(id)
    }

    // Compare before and after so older conflicting installations can be repaired.
    fn check_new_collisions(
        &self,
        config: &Config,
        target: &Target,
        old: &[Policy],
        new: &[Policy],
    ) -> Result<()> {
        let before: BTreeSet<_> = old.iter().filter(|p| eligible(p)).map(|p| &p.id).collect();
        let introduced: BTreeSet<_> = new
            .iter()
            .filter(|p| eligible(p))
            .map(|p| &p.id)
            .filter(|id| !before.contains(id))
            .collect();
        if introduced.is_empty() {
            return Ok(());
        }
        let check = |project: &str, other: &[Policy]| -> Result<()> {
            if let Some(policy) = other
                .iter()
                .filter(|p| eligible(p))
                .find(|p| introduced.contains(&p.id))
            {
                return Err(format!(
                    "active policy {} would exist in both user and project {project} scopes",
                    policy.id
                ));
            }
            Ok(())
        };
        match target {
            Target::User => {
                let mut checked = BTreeSet::new();
                for binding in &config.repositories {
                    if checked.insert(&binding.project) {
                        let local = self.read_registry(
                            &Target::Project {
                                binding: binding.clone(),
                            },
                            false,
                        )?;
                        check(&binding.project, &local.policies()?)?;
                    }
                }
            }
            Target::Project { binding } => {
                check(
                    &binding.project,
                    &self.read_registry(&Target::User, false)?.policies()?,
                )?;
            }
        }
        Ok(())
    }

    pub fn plan(&self, request: Request) -> Result<Plan> {
        self.prepare(request).map(|(plan, _)| plan)
    }

    fn prepare(&self, request: Request) -> Result<(Plan, Vec<(String, String)>)> {
        Self::require_private_location(&self.path)?;
        let mut config = self.config()?;
        let mut inputs = vec![];
        let mut writes = vec![];
        let mut policies = vec![];
        match &request {
            Request::Register {
                binding,
                new_project,
            } => {
                check_id(&binding.project)?;
                self.outside(&binding.root)?;
                if git_root(&binding.root)?.as_ref() != Some(&binding.root) {
                    return Err("registration requires an existing Git work tree root".into());
                }
                let target = Target::Project {
                    binding: binding.clone(),
                };
                let path = target.registry_path();
                let before = self.read(&path)?;
                let existing = config.repositories.iter().find(|p| p.root == binding.root);
                if let Some(existing) = existing {
                    if existing != binding {
                        return Err("work tree already registered to a different project; remove that registration first".into());
                    }
                }
                let already_known = config
                    .repositories
                    .iter()
                    .any(|p| p.project == binding.project);
                if *new_project && (already_known || before.is_some()) {
                    return Err(format!(
                        "project ID {} already exists; use --id explicitly to share it or choose another ID",
                        binding.project
                    ));
                }
                let registry = Self::decode_registry(&target, before.as_deref(), !already_known)?;
                self.check_new_collisions(&config, &target, &[], &registry.policies()?)?;
                inputs.push(existing.map(to_toml).transpose()?);
                if before.is_none() {
                    writes.push((path, to_toml(&registry)?));
                }
                inputs.push(before);
                if existing.is_none() {
                    config.repositories.push(binding.clone());
                    config.repositories.sort_by(|a, b| a.root.cmp(&b.root));
                    writes.push((CONFIG.into(), to_toml(&config)?));
                }
            }
            Request::Unregister { binding } => {
                // Exact stored binding permits removing a moved or deleted clone.
                let previous_count = config.repositories.len();
                config.repositories.retain(|p| p != binding);
                if config.repositories.len() == previous_count {
                    return Err("repository registration no longer exists".into());
                }
                inputs.push(Some(to_toml(binding)?));
                writes.push((CONFIG.into(), to_toml(&config)?));
            }
            Request::Change {
                target,
                expected_revision,
                reason,
                change,
            } => {
                let path = target.registry_path();
                self.validate_target(target)?;
                let before = self.read(&path)?;
                let current = Self::decode_registry(target, before.as_deref(), false)?;
                let next =
                    current.changed(*expected_revision, reason.clone(), change.as_ref().clone())?;
                target.validate(&next)?;
                let old = current.policies()?;
                let new = next.policies()?;
                self.check_new_collisions(&config, target, &old, &new)?;
                policies = policy_effects(&old, &new);
                inputs.push(before);
                writes.push((path, to_toml(&next)?));
            }
            Request::Import { target, registry } => {
                let path = target.registry_path();
                self.validate_target(target)?;
                let before = self.read(&path)?;
                let current = Self::decode_registry(target, before.as_deref(), false)?;
                target.validate(registry)?;
                inputs.push(before);
                if &current != registry {
                    if current.revision() != 0 {
                        return Err(
                            "destination already has policy history; import into an empty registry"
                                .into(),
                        );
                    }
                    let new = registry.policies()?;
                    self.check_new_collisions(&config, target, &[], &new)?;
                    policies = policy_effects(&[], &new);
                    writes.push((path, to_toml(registry)?));
                }
            }
        }
        let plan = Plan {
            format: PLAN_FORMAT.into(),
            home: self.path.clone(),
            request,
            source_digest: digest(&serde_json::to_vec(&inputs).map_err(|e| e.to_string())?),
            files: writes.iter().map(|(path, _)| path.clone()).collect(),
            policies,
        };
        Ok((plan, writes))
    }

    pub fn apply(&self, reviewed: &Plan) -> Result<()> {
        if reviewed.format != PLAN_FORMAT || reviewed.home != self.path {
            return Err("plan belongs to a different AMH home or format".into());
        }
        Self::require_private_location(&self.path)?;
        create_private_dir(&self.path)?;
        let path = self.storage_path(".lock")?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(path).map_err(|e| e.to_string())?;
        lock.lock().map_err(|e| e.to_string())?;
        let (current, writes) = self.prepare(reviewed.request.clone())?;
        if &current != reviewed {
            return Err("plan changed or is stale; review a newly generated plan".into());
        }
        // A new project registry precedes its config binding. A retry can reuse
        // an existing empty registry if only the config write was interrupted.
        for (path, content) in writes {
            self.write(&path, &content)?;
        }
        Ok(())
    }

    fn write(&self, relative: &str, content: &str) -> Result<()> {
        let path = self.storage_path(relative)?;
        let parent = path.parent().ok_or("managed path has no parent")?;
        Self::require_private_location(parent)?;
        create_private_dir(parent)?;
        self.storage_path(relative)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let temp = parent.join(format!(".amh-{}-{nonce}.tmp", std::process::id()));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp).map_err(|e| e.to_string())?;
            file.write_all(content.as_bytes())
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            fs::rename(&temp, &path).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            File::open(parent)
                .and_then(|d| d.sync_all())
                .map_err(|e| e.to_string())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

fn policy_effects(old: &[Policy], new: &[Policy]) -> Vec<PolicyEffect> {
    let old: BTreeMap<_, _> = old.iter().map(|p| (&p.id, p)).collect();
    let new: BTreeMap<_, _> = new.iter().map(|p| (&p.id, p)).collect();
    let ids: BTreeSet<_> = old.keys().chain(new.keys()).copied().collect();
    ids.into_iter()
        .filter_map(|id| {
            let before = old.get(id);
            let after = new.get(id);
            (before != after).then(|| PolicyEffect {
                id: id.clone(),
                before: before.map(|p| p.lifecycle),
                after: after.map(|p| p.lifecycle),
            })
        })
        .collect()
}

fn create_private_dir(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|e| e.to_string())
}

fn to_toml<T: Serialize>(value: &T) -> Result<String> {
    toml::to_string_pretty(value).map_err(|e| e.to_string())
}

pub fn import_registry(file: &Path, target: &Target) -> Result<Registry> {
    let mut registry: Registry = read_document(file)?;
    registry.policies()?;
    registry.project_id = target.registry_id().into();
    target.validate(&registry)?;
    Ok(registry)
}

pub fn read_document<T: serde::de::DeserializeOwned>(file: &Path) -> Result<T> {
    let content = fs::read_to_string(file).map_err(|e| e.to_string())?;
    if file
        .extension()
        .is_some_and(|extension| extension == "json")
    {
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
