use amh_project::{
    builtin_pack, Home, Lifecycle, Policy, PolicyChange, Project, Registry, Request, Scope,
    Strength, Target,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
};

const USAGE: &[(&str, &str, &str)] = &[
    ("project add", "[--id ID] [--root PATH] [--apply | --plan]", "Register a Git work tree. Explicitly supply an existing ID to share its policies."),
    ("project list", "", "List registered work trees and their project IDs."),
    ("project remove", "[--root PATH] [--apply | --plan]", "Unregister a work tree and keep its policies."),
    ("agent instructions", "", "Print guidance to add to the agent's user-level settings."),
    ("policy list", "[--scope user|project] [--root PATH] [--full]", "Show policy summaries for this project and the user. An explicit scope selects one registry."),
    ("policy discover", "[--root PATH]", "List conditional candidates or deliver unconditional policies when there are no candidates."),
    ("policy read", "ID --catalog-digest DIGEST [--root PATH]\n    amh policy read --selection [--root PATH]", "Read one policy for assessment or deliver a validated selection from JSON on stdin."),
    ("policy add", "(--id ID --text TEXT [--when CONDITION] [--strength must|should|may] | --file FILE) [--draft] [change options]", "Add an active policy. Use --draft to add it without activating it."),
    ("policy change", "(ID [--item-revision N] (--text TEXT | --strength must|should|may | --when CONDITION | --clear-condition | --activate | --pause | --reactivate) | --file FILE) [change options]", "Edit policy text, condition, strength, or lifecycle. Use a change file for detailed operations."),
    ("policy import", "--file REGISTRY [--scope user|project] [--root PATH] [--apply | --plan]", "Copy history from an AMH registry into an empty registry."),
    ("pack list", "", "List built-in policy packs."),
    ("pack show", "NAME", "Show the policies in a built-in pack before installation."),
    ("pack install", "(NAME | --file FILE) [change options]", "Install a built-in or custom policy pack."),
    ("history", "[--scope user|project] [--root PATH] [--full]", "Show policy change history. Use --full for a portable registry export."),
    ("rollback", "--revision N [change options]", "Restore an earlier policy set as a new revision."),
    ("apply", "--plan FILE|-", "Apply a saved plan to the same AMH home after checking relevant state."),
];

#[derive(Default)]
struct Flags<'a> {
    values: BTreeMap<&'a str, &'a str>,
    switches: BTreeSet<&'a str>,
}

impl<'a> Flags<'a> {
    fn parse(args: &'a [String], values: &[&str], switches: &[&str]) -> Result<Self, String> {
        let mut result = Self::default();
        let mut args = args.iter().map(String::as_str);
        while let Some(flag) = args.next() {
            if switches.contains(&flag) {
                if !result.switches.insert(flag) {
                    return Err(format!("duplicate flag: {flag}"));
                }
            } else if let Some((name, value)) = flag.split_once('=') {
                if !values.contains(&name) {
                    return Err(format!("unknown option: {name}"));
                }
                result.insert(name, value)?;
            } else if values.contains(&flag) {
                let value = args.next()
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| format!("missing value for {flag}; use {flag}=VALUE for a value starting with --"))?;
                result.insert(flag, value)?;
            } else {
                return Err(format!("unknown option: {flag}"));
            }
        }
        Ok(result)
    }

    fn insert(&mut self, name: &'a str, value: &'a str) -> Result<(), String> {
        if value.is_empty() || self.values.insert(name, value).is_some() {
            return Err(format!("duplicate or empty option: {name}"));
        }
        Ok(())
    }

    fn required(&self, key: &str) -> Result<&str, String> {
        self.values
            .get(key)
            .copied()
            .ok_or_else(|| format!("required option: {key}"))
    }

    fn number(&self, key: &str) -> Result<Option<u64>, String> {
        self.values
            .get(key)
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| format!("{key} must be a nonnegative integer"))
            })
            .transpose()
    }

    fn root(&self) -> &Path {
        Path::new(self.values.get("--root").copied().unwrap_or("."))
    }
}

fn position(args: &[String]) -> (Option<&str>, &[String]) {
    match args.split_first() {
        Some((value, rest)) if !value.starts_with('-') => (Some(value), rest),
        _ => (None, args),
    }
}

fn json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

fn home() -> Result<Home, String> {
    let path = match std::env::var_os("AMH_HOME") {
        Some(path) => PathBuf::from(path),
        None => std::env::home_dir()
            .ok_or("cannot determine the user home directory; set AMH_HOME")?
            .join(".amh"),
    };
    Home::new(&path)
}

fn target(home: &Home, f: &Flags<'_>) -> Result<Target, String> {
    match f.values.get("--scope").copied().unwrap_or("project") {
        "user" if f.values.contains_key("--root") => {
            Err("--root is only used for project scope".into())
        }
        "user" => Ok(Target::User),
        "project" => Ok(Target::Project {
            binding: home.require_project(f.root())?,
        }),
        _ => Err("scope must be user or project".into()),
    }
}

fn mutation(home: &Home, f: &Flags<'_>, request: Request) -> Result<String, String> {
    let apply = f.switches.contains("--apply");
    let export = f.switches.contains("--plan");
    if apply && export {
        return Err("select --apply or --plan".into());
    }
    let plan = home.plan(request)?;
    if export {
        return json(&plan);
    }
    if apply {
        home.apply(&plan)?;
    }
    mutation_summary(&plan, apply)
}

fn mutation_summary(plan: &amh_project::Plan, apply: bool) -> Result<String, String> {
    let (operation, target, revision) = match &plan.request {
        Request::Register { binding, .. } => ("register", json!(binding), None),
        Request::Unregister { binding } => ("unregister", json!(binding), None),
        Request::Import { target, registry } => {
            ("import", json!(target), Some(registry.revision()))
        }
        Request::Change {
            target,
            expected_revision,
            change,
            ..
        } => (
            change.operation(),
            json!(target),
            expected_revision.checked_add(1),
        ),
    };
    let mut output = json!({
        "status": if apply { "applied" } else { "preview" },
        "operation": operation,
        "target": target,
        "policies": plan.policies,
    });
    if let Some(revision) = revision {
        output["revision"] = json!(revision);
    }
    if let Request::Change { change, .. } = &plan.request {
        if let PolicyChange::Restore { revision } = change.as_ref() {
            output["restored_revision"] = json!(revision);
        }
    }
    if plan.files.is_empty() {
        output["status"] = json!("unchanged");
    }
    json(&output)
}

const CHANGE_VALUES: &[&str] = &["--root", "--scope", "--expected-revision", "--reason"];

fn change_flags<'a>(
    args: &'a [String],
    values: &[&str],
    switches: &[&str],
) -> Result<Flags<'a>, String> {
    Flags::parse(
        args,
        &[CHANGE_VALUES, values].concat(),
        &[&["--apply", "--plan"][..], switches].concat(),
    )
}

fn change(
    home: &Home,
    f: &Flags<'_>,
    target: Target,
    snapshot_revision: u64,
    change: PolicyChange,
) -> Result<String, String> {
    let expected_revision = f
        .number("--expected-revision")?
        .unwrap_or(snapshot_revision);
    let reason = f.values.get("--reason").map_or_else(
        || change.operation().replace('_', " "),
        |reason| (*reason).to_owned(),
    );
    mutation(
        home,
        f,
        Request::Change {
            target,
            expected_revision,
            reason,
            change: Box::new(change),
        },
    )
}

fn usage(path: &str) -> String {
    let mut output = String::new();
    if path.is_empty() {
        output.push_str("AMH — Adaptive Minimal Harness\n\nUsage: amh <command>\n\n");
    }
    for (command, arguments, description) in USAGE {
        if path.is_empty()
            || *command == path
            || command
                .strip_prefix(path)
                .is_some_and(|suffix| suffix.starts_with(' '))
        {
            output.push_str(&format!(
                "    amh {command} {arguments}\n        {description}\n"
            ));
        }
    }
    if path.is_empty()
        || matches!(
            path,
            "policy" | "policy add" | "policy change" | "pack" | "pack install" | "rollback"
        )
    {
        output.push_str("\nChange options: [--scope user|project] [--root PATH] [--reason TEXT]\n    [--expected-revision N] [--apply | --plan]\n");
    }
    if path.is_empty() {
        output.push_str("\n    amh --help | --version\n\nAMH_HOME defaults to ~/.amh. Changes preview by default.\n");
    }
    output.trim_end().to_owned()
}

pub fn run(args: &[String]) -> Result<String, String> {
    let is_help = |arg: &str| matches!(arg, "-h" | "--help");
    let Some((command, rest)) = args.split_first() else {
        return Ok(usage(""));
    };
    if args.len() == 1 {
        match command.as_str() {
            "-h" | "--help" => return Ok(usage("")),
            "-V" | "--version" => return Ok(format!("amh {}", env!("CARGO_PKG_VERSION"))),
            _ => {}
        }
    }
    let (path, rest) = if matches!(command.as_str(), "agent" | "project" | "policy" | "pack") {
        match rest.split_first() {
            Some((sub, tail)) if !is_help(sub) => (format!("{command} {sub}"), tail),
            Some((sub, [])) if is_help(sub) => return Ok(usage(command)),
            _ => {
                return Err(format!(
                    "choose a {command} subcommand; run amh {command} --help"
                ))
            }
        }
    } else {
        (command.clone(), rest)
    };
    if !USAGE.iter().any(|(command, _, _)| *command == path) {
        return Err("unsupported command; run amh --help".into());
    }
    if rest.len() == 1 && is_help(&rest[0]) {
        return Ok(usage(&path));
    }
    match path.as_str() {
        "agent instructions" => {
            Flags::parse(rest, &[], &[])?;
            return Ok(amh_project::AGENT_GUIDANCE.into());
        }
        "pack list" => {
            Flags::parse(rest, &[], &[])?;
            return json(&[amh_project::BUILTIN_PACK_ID]);
        }
        "pack show" => {
            let (name, args) = position(rest);
            Flags::parse(args, &[], &[])?;
            return json(&builtin_pack(name.ok_or("provide a pack name")?)?);
        }
        _ => {}
    }

    let home = home()?;
    match path.as_str() {
        "project add" => {
            let f = Flags::parse(rest, &["--root", "--id"], &["--apply", "--plan"])?;
            let root =
                amh_project::git_root(f.root())?.ok_or("project add requires a Git work tree")?;
            let (project, new_project) = match f.values.get("--id") {
                Some(id) => (id.to_string(), false),
                None => match home.project(&root)? {
                    Some(binding) => (binding.project, false),
                    None => (Home::project_id_for(&root)?, true),
                },
            };
            mutation(
                &home,
                &f,
                Request::Register {
                    binding: Project { root, project },
                    new_project,
                },
            )
        }
        "project list" => {
            Flags::parse(rest, &[], &[])?;
            json(&json!({"repositories": home.config()?.repositories}))
        }
        "project remove" => {
            let f = Flags::parse(rest, &["--root"], &["--apply", "--plan"])?;
            let exists = f.root().try_exists().map_err(|e| e.to_string())?;
            let path = if exists {
                std::fs::canonicalize(f.root()).map_err(|e| e.to_string())?
            } else {
                std::path::absolute(f.root()).map_err(|e| e.to_string())?
            };
            let binding = match home
                .config()?
                .repositories
                .into_iter()
                .find(|entry| entry.root == path)
            {
                Some(binding) => binding,
                None if exists => home.require_project(f.root())?,
                None => {
                    return Err(
                        "provide the exact stored path of a moved or deleted work tree".into(),
                    )
                }
            };
            mutation(&home, &f, Request::Unregister { binding })
        }
        "policy list" => policy_list(&home, rest),
        "policy discover" => {
            let f = Flags::parse(rest, &["--root"], &[])?;
            json(&amh_project::discover_policies(&home, f.root())?)
        }
        "policy read" => {
            let (id, flags) = position(rest);
            let f = Flags::parse(flags, &["--root", "--catalog-digest"], &["--selection"])?;
            match (id, f.switches.contains("--selection")) {
                (Some(id), false) => {
                    let policy = amh_project::read_policy(&home, f.root(), id, f.required("--catalog-digest")?)?;
                    json(&json!({
                        "id": policy.id,
                        "scope": policy.scope,
                        "content": policy.content,
                        "condition": policy.applicability,
                        "strength": policy.strength,
                        "depends_on": policy.relations.depends_on,
                        "conflicts_with": policy.relations.conflicts_with,
                    }))
                }
                (None, true) if !f.values.contains_key("--catalog-digest") => {
                    let selection = read_stdin()?;
                    json(&amh_project::read_selection(&home, f.root(), selection)?)
                }
                _ => Err("select a policy ID with --catalog-digest, or use --selection with JSON on stdin".into()),
            }
        }
        "policy add" => policy_add(&home, rest),
        "policy change" => policy_change(&home, rest),
        "policy import" => {
            let f = Flags::parse(
                rest,
                &["--root", "--scope", "--file"],
                &["--apply", "--plan"],
            )?;
            let target = target(&home, &f)?;
            let registry = amh_project::import_registry(Path::new(f.required("--file")?), &target)?;
            mutation(&home, &f, Request::Import { target, registry })
        }
        "pack install" => {
            let (name, flags) = position(rest);
            let f = change_flags(flags, &["--file"], &[])?;
            let target = target(&home, &f)?;
            let scope = scope_of(&target);
            let pack = match (name, f.values.get("--file")) {
                (Some(name), None) => {
                    let mut pack = builtin_pack(name)?;
                    for policy in &mut pack.policies {
                        policy.scope = scope;
                    }
                    pack
                }
                (None, Some(file)) => amh_project::read_document(Path::new(file))?,
                _ => return Err("provide a built-in pack name or --file".into()),
            };
            for policy in &pack.policies {
                if policy.scope != scope {
                    return Err(format!(
                        "policy {} does not match the target scope",
                        policy.id
                    ));
                }
            }
            let revision = home.registry(&target)?.revision();
            change(
                &home,
                &f,
                target,
                revision,
                PolicyChange::InstallPack { pack },
            )
        }
        "history" => {
            let f = Flags::parse(rest, &["--root", "--scope"], &["--full"])?;
            let registry = home.registry(&target(&home, &f)?)?;
            if f.switches.contains("--full") {
                return json(&registry);
            }
            let events: Vec<_> = registry
                .events
                .iter()
                .map(|event| {
                    let change = &event.change;
                    let mut summary = json!({
                        "revision": event.revision,
                        "reason": event.reason,
                        "operation": change.operation(),
                        "policies": change.policy_ids(),
                    });
                    if let PolicyChange::Restore { revision } = change {
                        summary["restored_revision"] = json!(revision);
                    }
                    summary
                })
                .collect();
            json(
                &json!({"project_id": registry.project_id, "revision": registry.revision(), "events": events}),
            )
        }
        "rollback" => {
            let f = change_flags(rest, &["--revision"], &[])?;
            let target = target(&home, &f)?;
            let revision = f
                .number("--revision")?
                .ok_or("required option: --revision")?;
            let current = home.registry(&target)?.revision();
            change(
                &home,
                &f,
                target,
                current,
                PolicyChange::Restore { revision },
            )
        }
        "apply" => {
            let f = Flags::parse(rest, &["--plan"], &[])?;
            let source = f.required("--plan")?;
            let plan = if source == "-" {
                read_stdin()?
            } else {
                amh_project::read_document(Path::new(source))?
            };
            home.apply(&plan)?;
            mutation_summary(&plan, true)
        }
        _ => unreachable!("command was checked against the usage list"),
    }
}

fn read_stdin<T: DeserializeOwned>() -> Result<T, String> {
    const LIMIT: u64 = 2 * 1024 * 1024;
    let mut input = Vec::new();
    std::io::stdin()
        .take(LIMIT + 1)
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())?;
    if input.len() as u64 > LIMIT {
        return Err("JSON input exceeds 2 MiB".into());
    }
    serde_json::from_slice(&input).map_err(|e| e.to_string())
}

fn scope_of(target: &Target) -> Scope {
    match target {
        Target::User => Scope::User,
        Target::Project { .. } => Scope::Project,
    }
}

fn policy_list(home: &Home, args: &[String]) -> Result<String, String> {
    let f = Flags::parse(args, &["--root", "--scope"], &["--full"])?;
    let targets = if f.values.contains_key("--scope") {
        vec![target(home, &f)?]
    } else if let Some(binding) = home.project(f.root())? {
        vec![Target::User, Target::Project { binding }]
    } else if f.values.contains_key("--root") {
        return Err("repository is not registered; run amh project add".into());
    } else {
        vec![Target::User]
    };

    let full = f.switches.contains("--full");
    let mut registries = Vec::new();
    let mut policies = Vec::new();
    for target in targets {
        let registry = home.registry(&target)?;
        let (entries, packs) = registry.policies_and_pack_locks()?;
        let mut summary = json!({
            "scope": scope_of(&target),
            "project_id": registry.project_id,
            "revision": registry.revision(),
        });
        let packs = if full {
            json!(packs)
        } else {
            let versions: BTreeMap<_, _> = packs
                .iter()
                .filter_map(|(name, lock)| {
                    lock.rsplit_once(':').map(|(version, _)| (name, version))
                })
                .collect();
            json!(versions)
        };
        summary["packs"] = packs;
        registries.push(summary);
        for policy in entries {
            policies.push(if full {
                serde_json::to_value(&policy).map_err(|e| e.to_string())?
            } else {
                json!({
                    "id": policy.id,
                    "scope": policy.scope,
                    "lifecycle": policy.lifecycle,
                    "revision": policy.revision,
                    "source": policy.source,
                })
            });
        }
    }
    json(&json!({"registries": registries, "policies": policies}))
}

fn policy_add(home: &Home, args: &[String]) -> Result<String, String> {
    let f = change_flags(
        args,
        &["--file", "--id", "--text", "--when", "--strength"],
        &["--draft"],
    )?;
    let target = target(home, &f)?;
    let scope = scope_of(&target);
    let mut policy = if let Some(file) = f.values.get("--file") {
        if ["--id", "--text", "--when", "--strength"]
            .iter()
            .any(|key| f.values.contains_key(key))
        {
            return Err("use --file or direct policy fields".into());
        }
        amh_project::read_document::<Policy>(Path::new(file))?
    } else {
        Policy {
            id: f.required("--id")?.into(),
            scope,
            content: f.required("--text")?.into(),
            strength: f
                .values
                .get("--strength")
                .map_or(Ok(Strength::Must), |value| parse_strength(value))?,
            revision: 1,
            lifecycle: Lifecycle::Draft,
            replaces: None,
            source: None,
            priority: 0,
            relations: Default::default(),
            applicability: f.values.get("--when").map(|value| value.to_string()),
        }
    };
    if policy.scope != scope {
        return Err("new policy scope does not match the target registry".into());
    }
    if !matches!(policy.lifecycle, Lifecycle::Draft | Lifecycle::Active) {
        return Err("new policy must be draft or active".into());
    }
    policy.lifecycle = if f.switches.contains("--draft") {
        Lifecycle::Draft
    } else {
        Lifecycle::Active
    };
    let revision = home.registry(&target)?.revision();
    change(home, &f, target, revision, PolicyChange::Add { policy })
}

fn policy_change(home: &Home, args: &[String]) -> Result<String, String> {
    let (id, flags) = position(args);
    let f = change_flags(
        flags,
        &[
            "--file",
            "--text",
            "--when",
            "--item-revision",
            "--strength",
        ],
        &["--activate", "--pause", "--reactivate", "--clear-condition"],
    )?;
    let operations = [
        "--text",
        "--strength",
        "--when",
        "--clear-condition",
        "--activate",
        "--pause",
        "--reactivate",
    ]
    .into_iter()
    .filter(|key| f.switches.contains(key) || f.values.contains_key(key))
    .collect::<Vec<_>>();
    let target = target(home, &f)?;
    let registry: Registry = home.registry(&target)?;
    let operation = if let Some(file) = f.values.get("--file") {
        if id.is_some() || !operations.is_empty() || f.values.contains_key("--item-revision") {
            return Err("use --file or a direct policy change".into());
        }
        let change: PolicyChange = amh_project::read_document(Path::new(file))?;
        match change {
            PolicyChange::Add { .. } => return Err("use policy add for ADD".into()),
            PolicyChange::InstallPack { .. } => {
                return Err("use pack install for INSTALL_PACK".into())
            }
            PolicyChange::Restore { .. } => return Err("use rollback for RESTORE".into()),
            _ => change,
        }
    } else {
        let id = id.ok_or("provide a policy ID or --file")?.to_owned();
        let policy = registry
            .policies()?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("unknown policy in the selected scope: {id}"))?;
        let item_revision = f.number("--item-revision")?.unwrap_or(policy.revision);
        match operations.as_slice() {
            ["--text"] => PolicyChange::Modify {
                id,
                item_revision,
                content: f.required("--text")?.into(),
            },
            ["--strength"] => PolicyChange::SetStrength {
                id,
                item_revision,
                strength: parse_strength(f.required("--strength")?)?,
            },
            ["--when"] => PolicyChange::SetApplicability {
                id,
                item_revision,
                applicability: Some(f.required("--when")?.into()),
            },
            ["--clear-condition"] => PolicyChange::SetApplicability {
                id,
                item_revision,
                applicability: None,
            },
            ["--activate"] => PolicyChange::Activate { id, item_revision },
            ["--pause"] => PolicyChange::Pause { id, item_revision },
            ["--reactivate"] => PolicyChange::Reactivate { id, item_revision },
            _ => {
                return Err("supply exactly one policy change; run amh policy change --help".into())
            }
        }
    };
    change(home, &f, target, registry.revision(), operation)
}

fn parse_strength(value: &str) -> Result<Strength, String> {
    match value {
        "must" => Ok(Strength::Must),
        "should" => Ok(Strength::Should),
        "may" => Ok(Strength::May),
        _ => Err("strength must be must, should or may".into()),
    }
}
