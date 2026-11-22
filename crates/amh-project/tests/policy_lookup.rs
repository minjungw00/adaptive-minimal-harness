use amh_project::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    binding: Project,
    home: Home,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "amh-natural-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let root = dir.join("repo");
        fs::create_dir_all(&root).unwrap();
        assert!(std::process::Command::new("git")
            .current_dir(&root)
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success());
        let root = git_root(&root).unwrap().unwrap();
        let home = Home::new(&dir.join(".amh")).unwrap();
        let binding = Project {
            root,
            project: "test".into(),
        };
        let fixture = Self { dir, binding, home };
        fixture.apply(Request::Register {
            binding: fixture.binding.clone(),
            new_project: false,
        });
        fixture
    }
    fn target(&self) -> Target {
        Target::Project {
            binding: self.binding.clone(),
        }
    }
    fn apply(&self, request: Request) {
        self.home.apply(&self.home.plan(request).unwrap()).unwrap();
    }
    fn add(&self, policy: Policy) {
        let target = if policy.scope == Scope::User {
            Target::User
        } else {
            self.target()
        };
        let id = policy.id.clone();
        let revision = self.home.registry(&target).unwrap().revision();
        for (expected_revision, change) in [
            (revision, PolicyChange::Add { policy }),
            (
                revision + 1,
                PolicyChange::Activate {
                    id,
                    item_revision: 1,
                },
            ),
        ] {
            self.apply(Request::Change {
                target: target.clone(),
                expected_revision,
                reason: "Fixture policy".into(),
                change: Box::new(change),
            });
        }
    }
    fn discovery(&self) -> Discovery {
        discover_policies(&self.home, &self.binding.root).unwrap()
    }

    fn read(&self, digest: &str, decisions: &[(&str, Applicability)]) -> Result<PolicyDelivery> {
        read_selection(
            &self.home,
            &self.binding.root,
            Selection {
                catalog_digest: digest.into(),
                decisions: decisions
                    .iter()
                    .map(|(id, applicability)| Decision {
                        id: (*id).into(),
                        applicability: *applicability,
                    })
                    .collect(),
            },
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn policy(id: &str, condition: &str) -> Policy {
    Policy {
        id: id.into(),
        scope: Scope::Project,
        content: "Use this method only when relevant".into(),
        strength: Strength::Must,
        revision: 1,
        lifecycle: Lifecycle::Draft,
        source: None,
        priority: 0,
        relations: Relations::default(),
        applicability: Some(condition.into()),
        replaces: None,
    }
}

#[test]
fn merges_scopes_and_unconditional_guidance_and_detects_declared_relations() {
    use Applicability::{Match, NoMatch};
    let fixture = Fixture::new();
    let mut common = policy("P-COMMON", "When editing a report.");
    common.scope = Scope::User;
    fixture.add(common);

    let mut local = policy("P-LOCAL", "When the report is intended for publication.");
    local.relations.depends_on.push("P-COMMON".into());
    fixture.add(local);

    let mut exclusive = policy("P-EXCLUSIVE", "When publishing an internal report.");
    exclusive.relations.conflicts_with.push("P-LOCAL".into());
    fixture.add(exclusive);

    let mut baseline = policy("P-EVIDENCE", "unused");
    baseline.applicability = None;
    baseline.content = "Distinguish checked facts from assumptions.".into();
    fixture.add(baseline);

    let Discovery::Ready {
        catalog_digest,
        candidates,
    } = fixture.discovery()
    else {
        panic!("expected candidates");
    };
    assert_eq!(candidates.len(), 3);
    let result = fixture
        .read(
            &catalog_digest,
            &[
                ("P-COMMON", Match),
                ("P-LOCAL", Match),
                ("P-EXCLUSIVE", NoMatch),
            ],
        )
        .unwrap();
    assert_eq!(result.selected, ["P-COMMON", "P-EVIDENCE", "P-LOCAL"]);
    assert!(fixture
        .read(
            &catalog_digest,
            &[
                ("P-COMMON", NoMatch),
                ("P-LOCAL", Match),
                ("P-EXCLUSIVE", NoMatch)
            ]
        )
        .unwrap_err()
        .contains("unresolved dependency"));
    assert!(fixture
        .read(
            &catalog_digest,
            &[
                ("P-COMMON", Match),
                ("P-LOCAL", Match),
                ("P-EXCLUSIVE", Match)
            ]
        )
        .unwrap_err()
        .contains("unresolved conflict"));
}

#[test]
fn expects_mandatory_decisions_but_optional_candidates_can_be_omitted() {
    use Applicability::{Match, NoMatch, Uncertain};
    let fixture = Fixture::new();
    fixture.add(policy("P-MANDATORY", "When editing a report."));
    let mut optional = policy("P-OPTIONAL", "When starting a draft.");
    optional.strength = Strength::Should;
    fixture.add(optional);

    let Discovery::Ready { catalog_digest, .. } = fixture.discovery() else {
        panic!("expected candidates");
    };
    assert!(fixture
        .read(&catalog_digest, &[])
        .unwrap_err()
        .contains("missing decision"));
    assert!(fixture
        .read(&catalog_digest, &[("P-MANDATORY", Uncertain)])
        .unwrap_err()
        .contains("uncertain applicability"));
    assert!(fixture
        .read(
            &catalog_digest,
            &[("P-MANDATORY", Match), ("P-MANDATORY", NoMatch)]
        )
        .unwrap_err()
        .contains("duplicate"));
    assert!(fixture
        .read(
            &catalog_digest,
            &[("P-MANDATORY", NoMatch), ("P-UNKNOWN", Match)]
        )
        .unwrap_err()
        .contains("unknown"));
    assert!(fixture
        .read(&catalog_digest, &[("P-MANDATORY", NoMatch)])
        .unwrap()
        .selected
        .is_empty());
    assert_eq!(
        fixture
            .read(
                &catalog_digest,
                &[("P-MANDATORY", NoMatch), ("P-OPTIONAL", Match)]
            )
            .unwrap()
            .selected,
        ["P-OPTIONAL"]
    );
    let other = Fixture::new();
    other.add(policy("P-MANDATORY", "When editing a report."));
    assert!(other
        .read(&catalog_digest, &[("P-MANDATORY", Match)])
        .unwrap_err()
        .contains("stale"));
}
