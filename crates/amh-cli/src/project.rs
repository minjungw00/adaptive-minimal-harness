use amh_project::{builtin_pack, Policy, PolicyChange, Request, Task};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub const HELP: &str = "PROJECT WORKFLOW (user-owned local files; no protected-host authorization):
    amh init --id ID [--root PATH] [--user-root PATH] [--apply]
    amh policy list [--root PATH]
    amh policy add --file POLICY.toml --expected-revision N --reason TEXT [--root PATH] [--apply]
    amh policy change --file CHANGE.json --expected-revision N --reason TEXT [--root PATH] [--apply]
    amh pack list
    amh pack show --name development|inquiry
    amh pack install --name NAME [--scope user|project] --expected-revision N --reason TEXT [--root PATH] [--apply]
    amh pack install --file PACK.toml [--scope user|project] --expected-revision N --reason TEXT [--root PATH] [--apply]
    amh request --text TEXT | --request-file FILE [--root PATH] [--task-id ID]
    amh request --text TEXT --agent codex [--root PATH] [--apply] [--budget CHARS]
    amh request --request-file FILE --agent-config CONFIG.json [--root PATH] [--apply]
    amh request finish --packet PACKET.json --analysis RESPONSE.json [--root PATH] [--apply]
    amh compile --task TASK.toml [--root PATH] [--budget CHARS]
    amh sync --task TASK.toml [--root PATH] [--budget CHARS] [--apply]
    amh apply --plan PLAN.json [--root PATH]
    amh doctor [--root PATH]
    amh history [--root PATH]
    amh rollback --revision N --expected-revision N --reason TEXT [--root PATH] [--apply]
    amh uninstall [--root PATH] [--apply]
    amh recover [--root PATH] [--apply]
    amh verify --task TASK.toml [--root PATH] [--run]

Writes default to JSON plans. --apply performs the explicitly selected local change.
Save a plan and use 'amh apply --plan' to apply exactly reviewed inputs/outputs.
Verification commands run only with --run. Allowed paths are advisory, not a sandbox.
";

struct Flags<'a> {
    values: BTreeMap<&'a str, &'a str>,
    switches: BTreeSet<&'a str>,
}
impl<'a> Flags<'a> {
    fn parse(args: &'a [String], values: &[&str], switches: &[&str]) -> Result<Self, String> {
        let mut result = Self {
            values: BTreeMap::new(),
            switches: BTreeSet::new(),
        };
        let mut i = 0;
        while i < args.len() {
            let flag = args[i].as_str();
            if switches.contains(&flag) {
                if !result.switches.insert(flag) {
                    return Err(format!("duplicate flag: {flag}"));
                }
                i += 1;
            } else if values.contains(&flag) {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| format!("missing value for {flag}"))?;
                if value.is_empty()
                    || value.starts_with("--")
                    || result.values.insert(flag, value).is_some()
                {
                    return Err(format!("duplicate or empty option: {flag}"));
                }
                i += 2;
            } else {
                return Err(format!("unknown option: {flag}"));
            }
        }
        Ok(result)
    }
    fn required(&self, key: &str) -> Result<&str, String> {
        self.values
            .get(key)
            .copied()
            .ok_or_else(|| format!("required option: {key}"))
    }
    fn number(&self, key: &str) -> Result<u64, String> {
        self.required(key)?
            .parse()
            .map_err(|_| format!("{key} must be a nonnegative integer"))
    }
    fn root(&self) -> &Path {
        Path::new(self.values.get("--root").copied().unwrap_or("."))
    }
    fn budget(&self) -> Result<Option<usize>, String> {
        self.values
            .get("--budget")
            .map(|s| {
                s.parse()
                    .map_err(|_| "budget must be a nonnegative character count".into())
            })
            .transpose()
    }
}
fn json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}
fn mutation(f: &Flags<'_>, request: Request) -> Result<String, String> {
    let p = amh_project::plan(f.root(), request)?;
    if f.switches.contains("--apply") {
        amh_project::apply(f.root(), &p)?;
        json(
            &serde_json::json!({"status":"applied_local_plan", "authorization":"local_filesystem_permissions_only", "plan":p}),
        )
    } else {
        json(&p)
    }
}

pub fn run(args: &[String]) -> Option<Result<String, String>> {
    let first = args.first()?.as_str();
    if ![
        "init",
        "request",
        "policy",
        "pack",
        "compile",
        "sync",
        "apply",
        "doctor",
        "history",
        "rollback",
        "uninstall",
        "recover",
        "verify",
    ]
    .contains(&first)
    {
        return None;
    }
    Some(execute(args))
}
fn execute(args: &[String]) -> Result<String, String> {
    let command = args[0].as_str();
    let (sub, rest) = if ["policy", "pack"].contains(&command) {
        (
            args.get(1).ok_or("missing subcommand")?.as_str(),
            &args[2..],
        )
    } else {
        ("", &args[1..])
    };
    if command == "request" {
        return execute_request(rest);
    }
    match (command, sub) {
        ("init", _) => {
            let f = Flags::parse(rest, &["--root", "--id", "--user-root"], &["--apply"])?;
            mutation(
                &f,
                Request::Init {
                    project_id: f.required("--id")?.into(),
                    user_root: f.values.get("--user-root").map(PathBuf::from),
                },
            )
        }
        ("policy", "list") => {
            let f = Flags::parse(rest, &["--root"], &[])?;
            let registry = amh_project::load(f.root())?;
            json(
                &serde_json::json!({"revision":registry.revision(), "policies":amh_project::policies(f.root())?, "pack_locks":amh_project::pack_locks(&registry)?}),
            )
        }
        ("policy", "add" | "change") => {
            let f = Flags::parse(
                rest,
                &["--root", "--file", "--expected-revision", "--reason"],
                &["--apply"],
            )?;
            let file = Path::new(f.required("--file")?);
            let change = if sub == "add" {
                PolicyChange::Add {
                    policy: amh_project::read_document::<Policy>(file)?,
                }
            } else {
                amh_project::read_document(file)?
            };
            mutation(
                &f,
                Request::Change {
                    expected_revision: f.number("--expected-revision")?,
                    reason: f.required("--reason")?.into(),
                    change,
                },
            )
        }
        ("pack", "list") => {
            Flags::parse(rest, &[], &[])?;
            json(&["development", "inquiry"])
        }
        ("pack", "show") => {
            let f = Flags::parse(rest, &["--name"], &[])?;
            json(&builtin_pack(f.required("--name")?)?)
        }
        ("pack", "install") => {
            let f = Flags::parse(
                rest,
                &[
                    "--root",
                    "--name",
                    "--file",
                    "--scope",
                    "--expected-revision",
                    "--reason",
                ],
                &["--apply"],
            )?;
            let mut pack = match (f.values.get("--name"), f.values.get("--file")) {
                (Some(name), None) => builtin_pack(name)?,
                (None, Some(file)) => amh_project::read_document(Path::new(file))?,
                _ => return Err("select exactly one of --name or --file".into()),
            };
            if let Some(scope) = f.values.get("--scope") {
                let scope = match *scope {
                    "user" => amh_core::ItemScope::User,
                    "project" => amh_core::ItemScope::Project,
                    _ => return Err("pack scope must be user or project".into()),
                };
                for policy in &mut pack.policies {
                    policy.item.scope = scope;
                }
            }
            mutation(
                &f,
                Request::Change {
                    expected_revision: f.number("--expected-revision")?,
                    reason: f.required("--reason")?.into(),
                    change: PolicyChange::InstallPack { pack },
                },
            )
        }
        ("compile" | "sync", _) => {
            let f = Flags::parse(
                rest,
                &["--root", "--task", "--budget"],
                if command == "sync" { &["--apply"] } else { &[] },
            )?;
            let task: Task = amh_project::read_document(Path::new(f.required("--task")?))?;
            if command == "compile" {
                json(&amh_project::compile(f.root(), &task, f.budget()?)?)
            } else {
                mutation(
                    &f,
                    Request::SyncFile {
                        task_file: std::fs::canonicalize(f.required("--task")?)
                            .map_err(|e| e.to_string())?,
                        budget: f.budget()?,
                    },
                )
            }
        }
        ("apply", _) => {
            let f = Flags::parse(rest, &["--root", "--plan"], &[])?;
            let plan = amh_project::read_document(Path::new(f.required("--plan")?))?;
            amh_project::apply(f.root(), &plan)?;
            json(
                &serde_json::json!({"status":"applied_local_plan", "authorization":"local_filesystem_permissions_only"}),
            )
        }
        ("doctor", _) => {
            let f = Flags::parse(rest, &["--root"], &[])?;
            let d = amh_project::doctor(f.root())?;
            let output = json(&d)?;
            if d.deployment == "stale" || d.deployment.starts_with("blocked") {
                Err(output)
            } else {
                Ok(output)
            }
        }
        ("history", _) => {
            let f = Flags::parse(rest, &["--root"], &[])?;
            json(&amh_project::load(f.root())?)
        }
        ("rollback", _) => {
            let f = Flags::parse(
                rest,
                &["--root", "--revision", "--expected-revision", "--reason"],
                &["--apply"],
            )?;
            mutation(
                &f,
                Request::Restore {
                    expected_revision: f.number("--expected-revision")?,
                    revision: f.number("--revision")?,
                    reason: f.required("--reason")?.into(),
                },
            )
        }
        ("uninstall", _) => {
            let f = Flags::parse(rest, &["--root"], &["--apply"])?;
            mutation(&f, Request::Uninstall)
        }
        ("recover", _) => {
            let f = Flags::parse(rest, &["--root"], &["--apply"])?;
            json(&amh_project::recover(
                f.root(),
                f.switches.contains("--apply"),
            )?)
        }
        ("verify", _) => {
            let f = Flags::parse(rest, &["--root", "--task"], &["--run"])?;
            let task = amh_project::read_document(Path::new(f.required("--task")?))?;
            let evidence = amh_project::verify(f.root(), &task, f.switches.contains("--run"))?;
            let output = json(&evidence)?;
            if evidence.all_checks_passed == Some(false) {
                Err(output)
            } else {
                Ok(output)
            }
        }
        _ => Err("unsupported project command; run amh --help".into()),
    }
}

fn execute_request(args: &[String]) -> Result<String, String> {
    if args.first().is_some_and(|s| s == "finish") {
        let f = Flags::parse(
            &args[1..],
            &["--root", "--packet", "--analysis", "--budget"],
            &["--apply"],
        )?;
        let packet = amh_project::read_document(Path::new(f.required("--packet")?))?;
        let response = amh_project::read_document(Path::new(f.required("--analysis")?))?;
        let task = amh_project::interpret_request(f.root(), &packet, response)?;
        return mutation(
            &f,
            Request::Sync {
                task,
                budget: f.budget()?,
            },
        );
    }
    let f = Flags::parse(
        args,
        &[
            "--root",
            "--text",
            "--request-file",
            "--task-id",
            "--agent",
            "--agent-config",
            "--budget",
        ],
        &["--apply"],
    )?;
    let text = match (f.values.get("--text"), f.values.get("--request-file")) {
        (Some(text), None) => (*text).to_string(),
        (None, Some(file)) => std::fs::read_to_string(file).map_err(|e| e.to_string())?,
        _ => return Err("select exactly one of --text or --request-file".into()),
    };
    let packet = amh_project::prepare_request(f.root(), &text, f.values.get("--task-id").copied())?;
    let config = match (f.values.get("--agent"), f.values.get("--agent-config")) {
        (None, None) => None,
        (Some(&"codex"), None) => Some(amh_project::InterpreterConfig::codex()),
        (None, Some(file)) => Some(amh_project::read_document(Path::new(file))?),
        _ => return Err("select --agent codex or one explicit --agent-config file".into()),
    };
    if let Some(config) = config {
        let response = amh_project::run_interpreter(&packet, &config)?;
        let task = amh_project::interpret_request(f.root(), &packet, response)?;
        mutation(
            &f,
            Request::Sync {
                task,
                budget: f.budget()?,
            },
        )
    } else {
        if f.values.contains_key("--budget") {
            return Err(
                "pass a compilation budget to request finish or an explicitly selected interpreter"
                    .into(),
            );
        }
        if f.switches.contains("--apply") {
            return Err("a discovery packet is not a completed interpretation; use the current agent's request finish step or explicitly select an interpreter".into());
        }
        json(&packet)
    }
}
