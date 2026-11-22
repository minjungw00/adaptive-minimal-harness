//! AMH CLI — local project workflows and read-only protected-state inspection.
//!
//! An untrusted agent cannot turn a preview into a persistent write.
//! Mutating protected state requires a separate trusted host; this executable does not
//! accept a public key or an "approved" flag to bypass that boundary.

use amh_core::{Change, ItemSnapshot, ManagedItem, SnapshotStore, StateManifest};
mod project;
#[cfg(feature = "approval-request")]
mod request;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const HELP: &str = "AMH — Adaptive Minimal Harness

USAGE:
    amh --help
    amh --version
    amh state inspect --file PATH
    amh manifest inspect --file PATH
    amh item get --file PATH --id ITEM_ID
    amh change preview --file PATH --proposal JSON_FILE

The legacy state and change commands are READ-ONLY. Preview is not authorization.
OPTIONAL FEATURE (approval-request):
    amh change request --file PATH --proposal JSON_FILE --project ID --ttl SECONDS
Authenticated protected-state writes remain an optional host capability.
";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangeProposal {
    expected_revision: u64,
    change: Change,
    reason: String,
}

#[derive(Debug, Serialize)]
struct ItemDiff {
    id: String,
    before: Option<ManagedItem>,
    after: Option<ManagedItem>,
}

fn write_json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|error| format!("could not encode output: {error}"))
}

fn parse_flags<'a>(
    args: &'a [String],
    expected: &[&str],
) -> Result<BTreeMap<&'a str, &'a str>, String> {
    if args.len() != expected.len() * 2 {
        return Err(format!("expected flags: {}", expected.join(", ")));
    }
    let mut flags = BTreeMap::new();
    for pair in args.chunks(2) {
        let flag = pair[0].as_str();
        let value = pair[1].as_str();
        if !expected.contains(&flag) || value.is_empty() || flags.insert(flag, value).is_some() {
            return Err(format!("unknown or duplicate/empty flag: {flag}"));
        }
    }
    Ok(flags)
}

fn load_state(path: &str) -> Result<ItemSnapshot, String> {
    SnapshotStore::new(path)
        .load()
        .map_err(|error| format!("state read/verification failed: {error:?}"))
}

fn inspect(path: &str) -> Result<String, String> {
    let snapshot = load_state(path)?;
    let legacy: Vec<_> = snapshot
        .items()
        .filter(|item| item.details.is_none())
        .map(|item| item.id.clone())
        .collect();

    write_json(&serde_json::json!({
        "status": "verified",
        "file_exists": Path::new(path).exists(),
        "snapshot_revision": snapshot.revision(),
        "item_count": snapshot.len(),
        "recorded_events": snapshot.history().len(),
        "typed_count": snapshot.len() - legacy.len(),
        "legacy_count": legacy.len(),
        "legacy_item_ids": legacy,
        "verification_scope": "structure_and_history_only",
        "note": "Legacy details require explicit review; no inferred migration was performed."
    }))
}

fn inspect_manifest(path: &str) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read metadata-only manifest: {error}"))?;
    let manifest: StateManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid metadata-only manifest: {error}"))?;
    manifest
        .validate_shape()
        .map_err(|error| format!("invalid metadata-only manifest: {error}"))?;
    write_json(&serde_json::json!({
        "status": "structurally_valid_untrusted",
        "project_id": manifest.project_id,
        "snapshot_revision": manifest.snapshot_revision,
        "item_count": manifest.items.len(),
        "authority": "informational_only",
        "warning": "Manifest structure is valid, but authenticity is not established. Do not use it for approval, policy enforcement, or protected-state writes."
    }))
}

fn get_item(path: &str, id: &str) -> Result<String, String> {
    let snapshot = load_state(path)?;
    let item = snapshot
        .get(id)
        .ok_or_else(|| format!("item not found: {id}"))?;
    write_json(item)
}

fn read_proposal(path: &str) -> Result<ChangeProposal, String> {
    let data = fs::read(path).map_err(|error| {
        format!(
            "cannot read proposal {}: {error}",
            Path::new(path).display()
        )
    })?;
    let proposal: ChangeProposal =
        serde_json::from_slice(&data).map_err(|error| format!("invalid proposal JSON: {error}"))?;
    if proposal.reason.trim().is_empty() {
        return Err("proposal reason must not be empty".into());
    }
    Ok(proposal)
}

fn item_map(snapshot: &ItemSnapshot) -> BTreeMap<String, ManagedItem> {
    snapshot
        .items()
        .map(|item| (item.id.clone(), item.clone()))
        .collect()
}

fn preview(state: &str, proposal_file: &str) -> Result<String, String> {
    let original = load_state(state)?;
    let proposal = read_proposal(proposal_file)?;
    let mut proposed = original.clone();
    proposed
        .apply_change(proposal.expected_revision, proposal.change.clone())
        .map_err(|error| format!("proposal rejected: {error:?}"))?;

    let before = item_map(&original);
    let after = item_map(&proposed);
    let ids: BTreeSet<_> = before.keys().chain(after.keys()).cloned().collect();
    let diffs: Vec<_> = ids
        .into_iter()
        .filter_map(|id| {
            let old = before.get(&id);
            let new = after.get(&id);
            (old != new).then(|| ItemDiff {
                id,
                before: old.cloned(),
                after: new.cloned(),
            })
        })
        .collect();

    write_json(&serde_json::json!({
        "status": "preview_only",
        "authorization": "not_granted",
        "state_file": PathBuf::from(state).display().to_string(),
        "expected_revision": proposal.expected_revision,
        "result_revision": proposed.revision(),
        "reason": proposal.reason,
        "change": proposal.change,
        "affected_items": diffs,
        "warning": "This is a simulation. No state file was changed or approved."
    }))
}

fn run(args: &[String]) -> Result<String, String> {
    if let Some(result) = project::run(args) {
        return result;
    }
    match args {
        [] => Ok(format!("{HELP}\n{}", project::HELP)),
        [arg] if arg == "-h" || arg == "--help" => Ok(format!("{HELP}\n{}", project::HELP)),
        [arg] if arg == "-V" || arg == "--version" => {
            Ok(format!("amh {}", env!("CARGO_PKG_VERSION")))
        }
        [a, b, rest @ ..] if a == "state" && b == "inspect" => {
            let flags = parse_flags(rest, &["--file"])?;
            inspect(flags["--file"])
        }
        [a, b, rest @ ..] if a == "manifest" && b == "inspect" => {
            let flags = parse_flags(rest, &["--file"])?;
            inspect_manifest(flags["--file"])
        }
        [a, b, rest @ ..] if a == "item" && b == "get" => {
            let flags = parse_flags(rest, &["--file", "--id"])?;
            get_item(flags["--file"], flags["--id"])
        }
        [a, b, rest @ ..] if a == "change" && b == "preview" => {
            let flags = parse_flags(rest, &["--file", "--proposal"])?;
            preview(flags["--file"], flags["--proposal"])
        }
        #[cfg(feature = "approval-request")]
        [a, b, rest @ ..] if a == "change" && b == "request" => {
            let flags = parse_flags(rest, &["--file", "--proposal", "--project", "--ttl"])?;
            request::prepare(
                flags["--file"],
                flags["--proposal"],
                flags["--project"],
                flags["--ttl"],
            )
        }
        _ => Err("unsupported command; run 'amh --help'".into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amh_core::{ItemKind, ItemScope};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        path: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!("amh-cli-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn state(&self, snapshot: &ItemSnapshot) -> PathBuf {
            let path = self.path.join("state.json");
            let bytes = serde_json::to_vec(&serde_json::json!({
                "format": "amh.item-snapshot",
                "format_revision": 1,
                "snapshot": snapshot
            }))
            .unwrap();
            fs::write(&path, bytes).unwrap();
            path
        }

        fn proposal(&self, value: &impl Serialize) -> PathBuf {
            let path = self.path.join("proposal.json");
            fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn sample_state() -> ItemSnapshot {
        let mut snapshot = ItemSnapshot::default();
        let item = ManagedItem::new(
            "R-A",
            ItemKind::Requirement,
            ItemScope::Project,
            "Cost analysis",
        )
        .unwrap();
        snapshot.insert(0, item).unwrap();
        snapshot
    }

    #[test]
    fn help_only_describes_available_commands() {
        let output = run(&["--help".into()]).unwrap();
        assert!(output.contains("change preview"));
        assert!(output.contains("READ-ONLY"));
        assert!(output.contains("amh project add"));
        assert!(!output.contains("amh sync"));
        assert!(output.contains("AMH home"));
    }

    #[test]
    fn unsupported_write_commands_fail() {
        assert!(run(&["init".into()]).is_err());
        assert!(run(&["item".into(), "apply".into()]).is_err());
        assert!(run(&["change".into(), "approve".into()]).is_err());
    }

    #[test]
    fn inspect_reports_legacy_items_without_auto_migration() {
        let fixture = Fixture::new();
        let path = fixture.state(&sample_state());
        let output = inspect(path.to_str().unwrap()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["snapshot_revision"], 1);
        assert_eq!(value["legacy_count"], 1);
        assert_eq!(value["legacy_item_ids"][0], "R-A");
        assert_eq!(value["verification_scope"], "structure_and_history_only");
    }

    #[test]
    fn get_item_returns_exact_stored_item() {
        let fixture = Fixture::new();
        let snapshot = sample_state();
        let path = fixture.state(&snapshot);
        let output = get_item(path.to_str().unwrap(), "R-A").unwrap();
        let item: ManagedItem = serde_json::from_str(&output).unwrap();
        assert_eq!(Some(&item), snapshot.get("R-A"));
        assert!(get_item(path.to_str().unwrap(), "missing").is_err());
    }

    #[test]
    fn preview_adds_item_without_mutating_state() {
        let fixture = Fixture::new();
        let path = fixture.state(&sample_state());
        let proposal = fixture.proposal(&serde_json::json!({
            "expected_revision": 1,
            "reason": "Include retention analysis",
            "change": {
                "operation": "ADD",
                "item": ManagedItem::new(
                    "R-B", ItemKind::Requirement, ItemScope::Project, "Customer retention"
                ).unwrap()
            }
        }));
        let original = fs::read(&path).unwrap();
        let output = preview(path.to_str().unwrap(), proposal.to_str().unwrap()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["status"], "preview_only");
        assert_eq!(value["authorization"], "not_granted");
        assert_eq!(value["affected_items"][0]["id"], "R-B");
        assert_eq!(value["result_revision"], 2);
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn stale_and_invalid_proposals_fail_without_write() {
        let fixture = Fixture::new();
        let path = fixture.state(&sample_state());
        let original = fs::read(&path).unwrap();
        let stale = fixture.proposal(&serde_json::json!({
            "expected_revision": 0,
            "reason": "Attempt from old state",
            "change": { "operation": "RETIRE", "id": "R-A", "item_revision": 1 }
        }));
        assert!(preview(path.to_str().unwrap(), stale.to_str().unwrap()).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::write(&stale, b"{invalid json").unwrap();
        assert!(preview(path.to_str().unwrap(), stale.to_str().unwrap()).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn structurally_valid_manifest_is_still_untrusted() {
        let fixture = Fixture::new();
        let manifest = StateManifest::from_snapshot("alpha", &sample_state());
        let path = fixture.path.join("manifest.json");
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let output = inspect_manifest(path.to_str().unwrap()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["status"], "structurally_valid_untrusted");
        assert_eq!(value["item_count"], 1);
        assert_eq!(value["authority"], "informational_only");
    }

    #[test]
    fn forged_manifest_authority_is_rejected() {
        let fixture = Fixture::new();
        let mut manifest = StateManifest::from_snapshot("alpha", &sample_state());
        manifest.authority = "trusted".into();
        let path = fixture.path.join("manifest.json");
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(inspect_manifest(path.to_str().unwrap()).is_err());
    }

    #[test]
    fn malformed_state_is_not_reported_valid() {
        let fixture = Fixture::new();
        let path = fixture.path.join("state.json");
        fs::write(&path, b"{").unwrap();
        assert!(inspect(path.to_str().unwrap()).is_err());
    }

    #[test]
    fn rejects_duplicate_or_unknown_cli_options() {
        assert!(run(&[
            "state".into(),
            "inspect".into(),
            "--file".into(),
            "a".into(),
            "--file".into(),
            "b".into()
        ])
        .is_err());
        assert!(run(&[
            "item".into(),
            "get".into(),
            "--file".into(),
            "a".into(),
            "--who".into(),
            "b".into()
        ])
        .is_err());
    }
}
