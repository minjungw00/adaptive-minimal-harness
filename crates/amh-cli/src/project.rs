use amh_project::{builtin_pack, Home, Policy, PolicyChange, Project, Request, Target};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub const HELP: &str = "LOCAL POLICY WORKFLOW (AMH_HOME defaults to ~/.amh):
    amh project add --id ID [--root PATH] [--apply]
    amh project list
    amh project remove [--root REGISTERED_PATH] [--apply]
    amh agent instructions
    amh policy list [--scope user|project] [--root PATH]
    amh policy discover --text TEXT [--root PATH]
    amh policy read --id ID --catalog-digest DIGEST [--root PATH]
    amh policy resolve --input -|FILE [--root PATH]
    amh policy add --file FILE --expected-revision N --reason TEXT [--scope user|project] [--root PATH] [--apply]
    amh policy change --file FILE --expected-revision N --reason TEXT [--scope user|project] [--root PATH] [--apply]
    amh policy import --file PREVIOUS_REGISTRY [--scope user|project] [--root PATH] [--apply]
    amh pack list
    amh pack show --name development|inquiry
    amh pack install (--name NAME | --file FILE) --expected-revision N --reason TEXT [--scope user|project] [--root PATH] [--apply]
    amh history [--scope user|project] [--root PATH]
    amh rollback --revision N --expected-revision N --reason TEXT [--scope user|project] [--root PATH] [--apply]
    amh apply --plan PLAN.json
    amh request (--text TEXT | --request-file FILE) [--task-id ID] [--agent codex | --agent-config FILE] [--root PATH]
    amh request finish --packet FILE --analysis FILE [--root PATH]
    amh compile --task FILE [--budget CHARS] [--root PATH]
    amh verify --task FILE [--run] [--root PATH]

Review and apply writes only in AMH home. Explicit Task checks run from the work tree with --run.
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
    let plan = home.plan(request)?;
    if f.switches.contains("--apply") {
        home.apply(&plan)?;
        json(&serde_json::json!({"status":"applied_local_plan", "plan":plan}))
    } else {
        json(&plan)
    }
}

fn change(
    home: &Home,
    f: &Flags<'_>,
    target: Target,
    change: PolicyChange,
) -> Result<String, String> {
    mutation(
        home,
        f,
        Request::Change {
            target,
            expected_revision: f.number("--expected-revision")?,
            reason: f.required("--reason")?.into(),
            change: Box::new(change),
        },
    )
}

pub fn run(args: &[String]) -> Option<Result<String, String>> {
    let first = args.first()?.as_str();
    [
        "project", "agent", "policy", "pack", "request", "compile", "verify", "apply", "history",
        "rollback",
    ]
    .contains(&first)
    .then(|| execute(args))
}

fn execute(args: &[String]) -> Result<String, String> {
    let command = args[0].as_str();
    let (sub, rest) = if ["agent", "project", "policy", "pack"].contains(&command) {
        (
            args.get(1).ok_or("missing subcommand")?.as_str(),
            &args[2..],
        )
    } else {
        ("", &args[1..])
    };
    match (command, sub) {
        ("agent", "instructions") => {
            Flags::parse(rest, &[], &[])?;
            return Ok(amh_project::AGENT_GUIDANCE.into());
        }
        ("pack", "list") => {
            Flags::parse(rest, &[], &[])?;
            return json(&["development", "inquiry"]);
        }
        ("pack", "show") => {
            let f = Flags::parse(rest, &["--name"], &[])?;
            return json(&builtin_pack(f.required("--name")?)?);
        }
        _ => {}
    }
    let home = home()?;
    match (command, sub) {
        ("project", "add") => {
            let f = Flags::parse(rest, &["--root", "--id"], &["--apply"])?;
            let root =
                amh_project::git_root(f.root())?.ok_or("project add requires a Git work tree")?;
            let binding = Project {
                root,
                project: f.required("--id")?.into(),
            };
            mutation(&home, &f, Request::Register { binding })
        }
        ("project", "list") => {
            Flags::parse(rest, &[], &[])?;
            json(&home.config()?)
        }
        ("project", "remove") => {
            let f = Flags::parse(rest, &["--root"], &["--apply"])?;
            let exists = f.root().try_exists().map_err(|e| e.to_string())?;
            let path = if exists {
                std::fs::canonicalize(f.root()).map_err(|e| e.to_string())?
            } else {
                std::path::absolute(f.root()).map_err(|e| e.to_string())?
            };
            let exact = home
                .config()?
                .repositories
                .into_iter()
                .find(|entry| entry.root == path);
            let binding = match exact {
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
        ("policy", "list") => {
            let f = Flags::parse(rest, &["--root", "--scope"], &[])?;
            let registry = home.registry(&target(&home, &f)?)?;
            json(
                &serde_json::json!({"revision":registry.revision(), "policies":registry.policies()?, "pack_locks":amh_project::pack_locks(&registry)?}),
            )
        }
        ("policy", "discover") => {
            let f = Flags::parse(rest, &["--root", "--text"], &[])?;
            match amh_project::discover_policies(&home, f.root(), f.required("--text")?)? {
                Some(packet) => json(&packet),
                None => json(&serde_json::json!({"status":"not_registered"})),
            }
        }
        ("policy", "read") => {
            let f = Flags::parse(rest, &["--root", "--id", "--catalog-digest"], &[])?;
            json(&amh_project::read_policy(
                &home,
                f.root(),
                f.required("--id")?,
                f.required("--catalog-digest")?,
            )?)
        }
        ("policy", "resolve") => {
            let f = Flags::parse(rest, &["--root", "--input"], &[])?;
            let analysis = input_json(f.required("--input")?)?;
            let result = amh_project::resolve_policies(&home, f.root(), analysis)?;
            let output = json(&result)?;
            if result.clarification_questions.is_empty() {
                Ok(output)
            } else {
                Err(output)
            }
        }
        ("policy", "add" | "change") => {
            let f = Flags::parse(
                rest,
                &[
                    "--root",
                    "--scope",
                    "--file",
                    "--expected-revision",
                    "--reason",
                ],
                &["--apply"],
            )?;
            let target = target(&home, &f)?;
            let source = Path::new(f.required("--file")?);
            let operation = if sub == "add" {
                PolicyChange::Add {
                    policy: amh_project::read_document::<Policy>(source)?,
                }
            } else {
                amh_project::read_document(source)?
            };
            change(&home, &f, target, operation)
        }
        ("policy", "import") => {
            let f = Flags::parse(rest, &["--root", "--scope", "--file"], &["--apply"])?;
            let target = target(&home, &f)?;
            let registry = amh_project::import_registry(Path::new(f.required("--file")?), &target)?;
            mutation(&home, &f, Request::Import { target, registry })
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
            let target = target(&home, &f)?;
            let mut pack = match (f.values.get("--name"), f.values.get("--file")) {
                (Some(name), None) => builtin_pack(name)?,
                (None, Some(file)) => amh_project::read_document(Path::new(file))?,
                _ => return Err("select exactly one of --name or --file".into()),
            };
            if f.values.contains_key("--scope") {
                let scope = if matches!(target, Target::User) {
                    amh_core::ItemScope::User
                } else {
                    amh_core::ItemScope::Project
                };
                for policy in &mut pack.policies {
                    policy.item.scope = scope;
                }
            }
            change(&home, &f, target, PolicyChange::InstallPack { pack })
        }
        ("history", _) => {
            let f = Flags::parse(rest, &["--root", "--scope"], &[])?;
            json(&home.registry(&target(&home, &f)?)?)
        }
        ("rollback", _) => {
            let f = Flags::parse(
                rest,
                &[
                    "--root",
                    "--scope",
                    "--revision",
                    "--expected-revision",
                    "--reason",
                ],
                &["--apply"],
            )?;
            change(
                &home,
                &f,
                target(&home, &f)?,
                PolicyChange::Restore {
                    revision: f.number("--revision")?,
                },
            )
        }
        ("apply", _) => {
            let f = Flags::parse(rest, &["--plan"], &[])?;
            let plan = amh_project::read_document(Path::new(f.required("--plan")?))?;
            home.apply(&plan)?;
            json(&serde_json::json!({"status":"applied_local_plan"}))
        }
        ("request", _) => execute_request(&home, rest),
        ("compile", _) => {
            let f = Flags::parse(rest, &["--root", "--task", "--budget"], &[])?;
            let binding = home.require_project(f.root())?;
            let task = amh_project::read_document(Path::new(f.required("--task")?))?;
            json(&amh_project::compile(&home, &binding, &task, f.budget()?)?)
        }
        ("verify", _) => {
            let f = Flags::parse(rest, &["--root", "--task"], &["--run"])?;
            let binding = home.require_project(f.root())?;
            let evidence = amh_project::verify(
                &home,
                &binding,
                Path::new(f.required("--task")?),
                f.switches.contains("--run"),
            )?;
            let output = json(&evidence)?;
            if evidence.all_checks_passed == Some(false) {
                Err(output)
            } else {
                Ok(output)
            }
        }
        _ => Err("unsupported policy workflow command; run amh --help".into()),
    }
}

fn execute_request(home: &Home, args: &[String]) -> Result<String, String> {
    if args.first().is_some_and(|s| s == "finish") {
        let f = Flags::parse(&args[1..], &["--root", "--packet", "--analysis"], &[])?;
        let packet = amh_project::read_document(Path::new(f.required("--packet")?))?;
        let response = amh_project::read_document(Path::new(f.required("--analysis")?))?;
        let binding = home.require_project(f.root())?;
        return json(&amh_project::interpret_request(
            home, &binding, &packet, response,
        )?);
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
        ],
        &[],
    )?;
    let text = match (f.values.get("--text"), f.values.get("--request-file")) {
        (Some(text), None) => (*text).to_string(),
        (None, Some(file)) => std::fs::read_to_string(file).map_err(|e| e.to_string())?,
        _ => return Err("select exactly one of --text or --request-file".into()),
    };
    let binding = home.require_project(f.root())?;
    let packet =
        amh_project::prepare_request(home, &binding, &text, f.values.get("--task-id").copied())?;
    let config = match (f.values.get("--agent"), f.values.get("--agent-config")) {
        (None, None) => None,
        (Some(&"codex"), None) => Some(amh_project::InterpreterConfig::codex()),
        (None, Some(file)) => Some(amh_project::read_document(Path::new(file))?),
        _ => return Err("select --agent codex or one explicit --agent-config file".into()),
    };
    if let Some(config) = config {
        let response = amh_project::run_interpreter(home, &packet, &config)?;
        json(&amh_project::interpret_request(
            home, &binding, &packet, response,
        )?)
    } else {
        json(&packet)
    }
}
fn input_json<T: serde::de::DeserializeOwned>(file: &str) -> Result<T, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    if file == "-" {
        std::io::stdin()
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
    } else {
        std::fs::File::open(file)
            .map_err(|e| e.to_string())?
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
    }
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("policy analysis input exceeds 2 MiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
