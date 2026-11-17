//! Untrusted request preparation for an EXTERNAL, human-authenticated signer.
//!
//! This module never issues a signature, authenticates a person, or writes to
//! the protected state. The signer must independently validate the project,
//! change, rationale, and intended authorization before signing.

use super::{load_state, read_proposal, write_json, ChangeProposal};
use amh_core::{ApprovalRequest, ItemSnapshot};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_SECONDS: u64 = 3_600;

fn valid_project_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

fn build_request(
    snapshot: &ItemSnapshot,
    proposal: ChangeProposal,
    project_id: &str,
    issued_at: u64,
    ttl_seconds: u64,
) -> Result<ApprovalRequest, String> {
    if !valid_project_id(project_id) {
        return Err("invalid project ID".into());
    }
    if ttl_seconds == 0 || ttl_seconds > MAX_SECONDS {
        return Err("TTL must be between 1 and 3600 seconds".into());
    }
    if proposal.reason.trim().is_empty() {
        return Err("change reason must not be empty".into());
    }
    let expires_at = issued_at
        .checked_add(ttl_seconds)
        .ok_or("request timestamp overflow")?;
    let mut candidate = snapshot.clone();
    candidate
        .apply_change(proposal.expected_revision, proposal.change.clone())
        .map_err(|error| format!("change rejected by state contract: {error:?}"))?;

    Ok(ApprovalRequest {
        project_id: project_id.to_owned(),
        expected_revision: proposal.expected_revision,
        change: proposal.change,
        reason: proposal.reason,
        issued_at,
        expires_at,
    })
}

fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for value in bytes {
        write!(&mut encoded, "{value:02x}").expect("formatting into String is infallible");
    }
    encoded
}

/// Prepare an exact request and its canonical signing message, without
/// creating an approval. A trusted signer must re-derive the signing bytes.
pub(super) fn prepare(
    state_file: &str,
    proposal_file: &str,
    project_id: &str,
    ttl: &str,
) -> Result<String, String> {
    let ttl = ttl
        .parse::<u64>()
        .map_err(|_| "TTL must be an unsigned integer in seconds")?;
    let issued_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock precedes Unix epoch")?
        .as_secs();
    let snapshot = load_state(state_file)?;
    let proposal = read_proposal(proposal_file)?;
    let request = build_request(&snapshot, proposal, project_id, issued_at, ttl)?;
    let signing_message = request
        .signing_message()
        .map_err(|error| format!("cannot encode signing message: {error}"))?;

    write_json(&serde_json::json!({
        "status": "untrusted_unsigned_request",
        "authorization": "not_granted",
        "request": request,
        "signing_message_hex": encode_hex(&signing_message),
        "warning": "External signer MUST re-derive the exact signing message, authenticate the approver, and review the change before signing. No state was modified."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use amh_core::{Change, ItemKind, ItemScope, ManagedItem};

    fn proposal(revision: u64) -> ChangeProposal {
        ChangeProposal {
            expected_revision: revision,
            change: Change::Add {
                item: ManagedItem::new(
                    "R-A",
                    ItemKind::Requirement,
                    ItemScope::Project,
                    "Analyze cost drivers",
                )
                .unwrap(),
            },
            reason: "Explicitly requested by the project owner".into(),
        }
    }

    #[test]
    fn approved_request_contract_is_not_an_approval() {
        let state = ItemSnapshot::default();
        let prepared = build_request(&state, proposal(0), "project-one", 100, 300).unwrap();
        assert_eq!(prepared.project_id, "project-one");
        assert_eq!(prepared.expires_at, 400);
        assert_eq!(prepared.expected_revision, 0);
        assert!(prepared
            .signing_message()
            .unwrap()
            .starts_with(b"AMH:change-approval:v1\0"));
    }

    #[test]
    fn stale_proposal_cannot_be_prepared() {
        let mut state = ItemSnapshot::default();
        state
            .insert(
                0,
                ManagedItem::new("P-X", ItemKind::Principle, ItemScope::Project, "X").unwrap(),
            )
            .unwrap();
        assert!(build_request(&state, proposal(0), "project-one", 100, 300).is_err());
    }

    #[test]
    fn rejects_invalid_project_and_expiration() {
        let snapshot = ItemSnapshot::default();
        assert!(build_request(&snapshot, proposal(0), "../unsafe", 100, 300).is_err());
        assert!(build_request(&snapshot, proposal(0), "project-one", 100, 0).is_err());
        assert!(build_request(&snapshot, proposal(0), "project-one", 100, 3601).is_err());
        assert!(build_request(&snapshot, proposal(0), "project-one", u64::MAX, 1).is_err());
    }

    #[test]
    fn encoding_is_exact_lowercase_hex() {
        assert_eq!(encode_hex(&[0, 1, 0xab, 0xff]), "0001abff");
    }
}
