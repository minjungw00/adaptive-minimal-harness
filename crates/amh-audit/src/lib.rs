//! Portable audit-segment validation shared by the protected host and an
//! independently operated witness.
//!
//! Hash continuity detects rollbacks/forks **only if an earlier checkpoint
//! survives independently**. SHA-256 alone does not authenticate the host.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CHAIN_FORMAT: &str = "amh.audit-chain";
pub const CHAIN_FORMAT_REVISION: u32 = 1;
pub const SEGMENT_FORMAT: &str = "amh.audit-segment";
pub const CHECKPOINT_FORMAT: &str = "amh.audit-checkpoint";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditBody {
    pub format: String,
    pub format_revision: u32,
    pub project_id: String,
    pub sequence: u64,
    pub previous_hash: Option<String>,
    pub before_revision: u64,
    pub after_revision: u64,
    pub before_state_digest: Option<String>,
    pub after_state_digest: String,
    pub receipt_digest: String,
    pub signing_key_id: String,
    pub recorded_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditEntry {
    pub body: AuditBody,
    pub hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditSegment {
    pub format: String,
    pub project_id: String,
    pub from_sequence: u64,
    pub entries: Vec<AuditEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditCheckpoint {
    pub format: String,
    pub project_id: String,
    pub sequence: u64,
    pub head_hash: String,
    pub state_digest: String,
}

pub fn digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let hash = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for value in hash {
        write!(&mut output, "{value:02x}").expect("formatting a string cannot fail");
    }
    output
}

pub fn body_hash(body: &AuditBody) -> Result<String, String> {
    let bytes = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    let mut framed = CHAIN_FORMAT.as_bytes().to_vec();
    framed.push(0);
    framed.extend(bytes);
    Ok(digest(&framed))
}

fn is_lower_hex_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub fn validate_checkpoint(checkpoint: &AuditCheckpoint) -> Result<(), String> {
    if checkpoint.format != CHECKPOINT_FORMAT
        || checkpoint.project_id.trim().is_empty()
        || checkpoint.sequence == 0
        || !is_lower_hex_hash(&checkpoint.head_hash)
        || !is_lower_hex_hash(&checkpoint.state_digest)
    {
        return Err("invalid independent audit checkpoint".into());
    }
    Ok(())
}

/// Validate a contiguous segment against an independently retained checkpoint.
///
/// Returns a new checkpoint for a nonempty segment. An empty segment with a
/// known checkpoint is an idempotent no-op. A fresh observer must receive
/// genesis (sequence 1), not a claimed head without history.
pub fn validate_segment(
    existing: Option<&AuditCheckpoint>,
    segment: &AuditSegment,
) -> Result<Option<AuditCheckpoint>, String> {
    if segment.format != SEGMENT_FORMAT || segment.project_id.trim().is_empty() {
        return Err("invalid audit segment header".into());
    }
    if let Some(anchor) = existing {
        validate_checkpoint(anchor)?;
        if segment.project_id != anchor.project_id || segment.from_sequence != anchor.sequence {
            return Err("audit segment conflicts with retained project or sequence".into());
        }
    } else if segment.from_sequence != 0 {
        return Err("unanchored audit segment must start at genesis".into());
    }
    if segment.entries.is_empty() {
        if existing.is_some() {
            return Ok(None);
        }
        return Err("an initial audit witness requires a genesis entry".into());
    }
    let mut prev_sequence = segment.from_sequence;
    let mut prev_hash = existing.map(|anchor| anchor.head_hash.clone());
    let mut prev_state_digest = existing.map(|anchor| anchor.state_digest.clone());

    for entry in &segment.entries {
        let expected = prev_sequence
            .checked_add(1)
            .ok_or("audit sequence overflow")?;
        let body = &entry.body;
        if body.format != CHAIN_FORMAT
            || body.format_revision != CHAIN_FORMAT_REVISION
            || body.project_id != segment.project_id
            || body.sequence != expected
            || body.before_revision != prev_sequence
            || body.after_revision != expected
            || body.previous_hash != prev_hash
            || body.before_state_digest != prev_state_digest
            || body.signing_key_id.trim().is_empty()
            || !is_lower_hex_hash(&body.receipt_digest)
            || !is_lower_hex_hash(&body.after_state_digest)
            || !is_lower_hex_hash(&entry.hash)
            || body_hash(body)? != entry.hash
        {
            return Err(format!(
                "invalid or forked audit record at sequence {expected}"
            ));
        }
        prev_sequence = expected;
        prev_hash = Some(entry.hash.clone());
        prev_state_digest = Some(body.after_state_digest.clone());
    }
    Ok(Some(AuditCheckpoint {
        format: CHECKPOINT_FORMAT.into(),
        project_id: segment.project_id.clone(),
        sequence: prev_sequence,
        head_hash: prev_hash.expect("nonempty segment has a hash"),
        state_digest: prev_state_digest.expect("nonempty segment has a state digest"),
    }))
}

/// Validate an entire journal from genesis, including an explicit project ID.
///
/// An absent audit journal is represented by an empty slice, not a checkpoint.
pub fn validate_complete(entries: &[AuditEntry]) -> Result<Option<AuditCheckpoint>, String> {
    let Some(first) = entries.first() else {
        return Ok(None);
    };
    validate_segment(
        None,
        &AuditSegment {
            format: SEGMENT_FORMAT.into(),
            project_id: first.body.project_id.clone(),
            from_sequence: 0,
            entries: entries.to_vec(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(project: &str, sequence: u64, previous: Option<&AuditEntry>) -> AuditEntry {
        let body = AuditBody {
            format: CHAIN_FORMAT.into(),
            format_revision: CHAIN_FORMAT_REVISION,
            project_id: project.into(),
            sequence,
            previous_hash: previous.map(|record| record.hash.clone()),
            before_revision: sequence - 1,
            after_revision: sequence,
            before_state_digest: previous.map(|record| record.body.after_state_digest.clone()),
            after_state_digest: digest(format!("state-{sequence}").as_bytes()),
            receipt_digest: digest(format!("receipt-{sequence}").as_bytes()),
            signing_key_id: "primary".into(),
            recorded_at: 100 + sequence,
        };
        AuditEntry {
            hash: body_hash(&body).unwrap(),
            body,
        }
    }

    #[test]
    fn genesis_and_incremental_segments_produce_same_checkpoint() {
        let first = entry("project", 1, None);
        let second = entry("project", 2, Some(&first));
        let initial = AuditSegment {
            format: SEGMENT_FORMAT.into(),
            project_id: "project".into(),
            from_sequence: 0,
            entries: vec![first.clone()],
        };
        let checkpoint = validate_segment(None, &initial).unwrap().unwrap();
        assert_eq!(checkpoint.sequence, 1);
        let update = AuditSegment {
            from_sequence: 1,
            entries: vec![second.clone()],
            ..initial.clone()
        };
        let updated = validate_segment(Some(&checkpoint), &update)
            .unwrap()
            .unwrap();
        let full = validate_complete(&[first, second]).unwrap().unwrap();
        assert_eq!(updated, full);
    }

    #[test]
    fn retained_checkpoint_rejects_rollbacks_and_forks() {
        let first = entry("project", 1, None);
        let checkpoint = validate_complete(std::slice::from_ref(&first)).unwrap().unwrap();
        let rollback = AuditSegment {
            format: SEGMENT_FORMAT.into(),
            project_id: "project".into(),
            from_sequence: 0,
            entries: vec![first.clone()],
        };
        assert!(validate_segment(Some(&checkpoint), &rollback).is_err());
        let mut forged = entry("project", 2, Some(&first));
        forged.body.previous_hash = Some(digest(b"different-prefix"));
        forged.hash = body_hash(&forged.body).unwrap();
        let fork = AuditSegment {
            from_sequence: 1,
            entries: vec![forged],
            ..rollback
        };
        assert!(validate_segment(Some(&checkpoint), &fork).is_err());
    }

    #[test]
    fn refuses_empty_initial_proof_and_unknown_project() {
        let segment = AuditSegment {
            format: SEGMENT_FORMAT.into(),
            project_id: "project".into(),
            from_sequence: 0,
            entries: vec![],
        };
        assert!(validate_segment(None, &segment).is_err());
        let first = entry("project", 1, None);
        let checkpoint = validate_complete(&[first]).unwrap().unwrap();
        assert!(validate_segment(Some(&checkpoint), &segment).is_err());
    }

    #[test]
    fn rejects_valid_hash_with_invalid_state_continuity() {
        let first = entry("project", 1, None);
        let mut second = entry("project", 2, Some(&first));
        second.body.before_state_digest = Some(digest(b"wrong-state"));
        second.hash = body_hash(&second.body).unwrap();
        assert!(validate_complete(&[first, second]).is_err());
    }
}
