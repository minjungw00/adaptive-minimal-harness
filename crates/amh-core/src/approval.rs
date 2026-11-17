//! Verification of externally issued, narrowly scoped signed changes.
//!
//! No private key is generated, stored, or loaded by AMH. The caller MUST
//! obtain the project-bound verifying key from a protected trust anchor,
//! not from untrusted repository content or an AI-generated argument.

use crate::Change;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const DOMAIN: &[u8] = b"AMH:change-approval:v1\0";
const MAX_LIFETIME_SECS: u64 = 60 * 60;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRequest {
    pub project_id: String,
    pub expected_revision: u64,
    pub change: Change,
    pub reason: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

impl ApprovalRequest {
    /// Bytes to sign using an external Ed25519 signing service.
    ///
    /// A domain separator and deterministically serialized typed request
    /// prevent approval of one project/change being reused for another.
    pub fn signing_message(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut message = DOMAIN.to_vec();
        message.extend(serde_json::to_vec(self)?);
        Ok(message)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedApproval {
    pub request: ApprovalRequest,
    /// Ed25519 signature bytes (64 bytes). The signing key stays outside AMH.
    pub signature: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ApprovalError {
    InvalidTrustedKey,
    InvalidSignature,
    WrongProject,
    InvalidReason,
    InvalidWindow,
    NotYetValid,
    Expired,
    InvalidClock,
    SerializationFailed,
}

/// A verifier constructed by the *trusted host*, not by an agent.
///
/// The public key and project identifier are the host's trusted inputs.
/// This type by itself does not protect against callers who can change the
/// trust anchor or write the state file directly.
pub struct ApprovalVerifier {
    project_id: String,
    verifying_key: VerifyingKey,
}

pub(crate) struct ApprovedChange {
    pub expected_revision: u64,
    pub change: Change,
}

impl ApprovalVerifier {
    pub fn new(project_id: impl Into<String>, public_key: [u8; 32]) -> Result<Self, ApprovalError> {
        let project_id = project_id.into();
        if project_id.trim().is_empty() {
            return Err(ApprovalError::WrongProject);
        }
        let verifying_key =
            VerifyingKey::from_bytes(&public_key).map_err(|_| ApprovalError::InvalidTrustedKey)?;
        Ok(Self {
            project_id,
            verifying_key,
        })
    }

    /// Check a receipt without granting write access. Trusted hosts can use
    /// this to select between concurrently active keys during rotation.
    pub fn check(&self, approval: &SignedApproval) -> Result<(), ApprovalError> {
        self.verify(approval).map(|_| ())
    }

    pub(crate) fn verify(
        &self,
        approval: &SignedApproval,
    ) -> Result<ApprovedChange, ApprovalError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ApprovalError::InvalidClock)?
            .as_secs();
        self.verify_at(approval, now)
    }

    fn verify_at(
        &self,
        approval: &SignedApproval,
        now: u64,
    ) -> Result<ApprovedChange, ApprovalError> {
        let signature = Signature::from_slice(&approval.signature)
            .map_err(|_| ApprovalError::InvalidSignature)?;
        let message = approval
            .request
            .signing_message()
            .map_err(|_| ApprovalError::SerializationFailed)?;
        self.verifying_key
            .verify_strict(&message, &signature)
            .map_err(|_| ApprovalError::InvalidSignature)?;

        let request = &approval.request;
        if request.project_id != self.project_id {
            return Err(ApprovalError::WrongProject);
        }
        if request.reason.trim().is_empty() {
            return Err(ApprovalError::InvalidReason);
        }
        let duration = request
            .expires_at
            .checked_sub(request.issued_at)
            .ok_or(ApprovalError::InvalidWindow)?;
        if duration == 0 || duration > MAX_LIFETIME_SECS {
            return Err(ApprovalError::InvalidWindow);
        }
        if now < request.issued_at {
            return Err(ApprovalError::NotYetValid);
        }
        if now >= request.expires_at {
            return Err(ApprovalError::Expired);
        }
        Ok(ApprovedChange {
            expected_revision: request.expected_revision,
            change: request.change.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ItemKind, ItemScope, ManagedItem};
    use ed25519_dalek::{Signer, SigningKey};

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[42_u8; 32])
    }

    fn request(now: u64, project: &str) -> ApprovalRequest {
        ApprovalRequest {
            project_id: project.into(),
            expected_revision: 0,
            change: Change::Add {
                item: ManagedItem::new(
                    "P-TDD",
                    ItemKind::Principle,
                    ItemScope::Project,
                    "TDD is required for behavior changes",
                )
                .unwrap(),
            },
            reason: "Enable a user-approved rule".into(),
            issued_at: now - 1,
            expires_at: now + 60,
        }
    }

    fn sign(request: ApprovalRequest, key: &SigningKey) -> SignedApproval {
        let message = request.signing_message().unwrap();
        SignedApproval {
            request,
            signature: key.sign(&message).to_bytes().to_vec(),
        }
    }

    #[test]
    fn valid_signature_authorizes_only_the_exact_change() {
        let key = signing_key();
        let verifier =
            ApprovalVerifier::new("project-one", key.verifying_key().to_bytes()).unwrap();
        let approval = sign(request(100, "project-one"), &key);
        let verified = verifier.verify_at(&approval, 101).unwrap();
        assert_eq!(verified.expected_revision, 0);
        assert_eq!(verified.change, approval.request.change);
    }

    #[test]
    fn tampering_with_change_or_revision_invalidates_signature() {
        let key = signing_key();
        let verifier =
            ApprovalVerifier::new("project-one", key.verifying_key().to_bytes()).unwrap();
        let original = sign(request(100, "project-one"), &key);
        let mut tampered = original.clone();
        tampered.request.expected_revision = 17;
        assert_eq!(
            verifier.verify_at(&tampered, 101).err(),
            Some(ApprovalError::InvalidSignature)
        );

        let mut tampered = original;
        tampered.request.reason = "Another reason".into();
        assert_eq!(
            verifier.verify_at(&tampered, 101).err(),
            Some(ApprovalError::InvalidSignature)
        );
    }

    #[test]
    fn rejects_wrong_project_and_wrong_trusted_key() {
        let key = signing_key();
        let other = SigningKey::from_bytes(&[43_u8; 32]);
        let approval = sign(request(100, "project-one"), &key);
        let wrong_project =
            ApprovalVerifier::new("project-two", key.verifying_key().to_bytes()).unwrap();
        assert_eq!(
            wrong_project.verify_at(&approval, 101).err(),
            Some(ApprovalError::WrongProject)
        );

        let wrong_key =
            ApprovalVerifier::new("project-one", other.verifying_key().to_bytes()).unwrap();
        assert_eq!(
            wrong_key.verify_at(&approval, 101).err(),
            Some(ApprovalError::InvalidSignature)
        );
    }

    #[test]
    fn rejects_outside_validity_window() {
        let key = signing_key();
        let verifier =
            ApprovalVerifier::new("project-one", key.verifying_key().to_bytes()).unwrap();
        let approval = sign(request(100, "project-one"), &key);
        assert_eq!(
            verifier.verify_at(&approval, 98).err(),
            Some(ApprovalError::NotYetValid)
        );
        assert_eq!(
            verifier.verify_at(&approval, 161).err(),
            Some(ApprovalError::Expired)
        );
    }

    #[test]
    fn rejects_invalid_lifetime_and_missing_reason_even_when_signed() {
        let key = signing_key();
        let verifier =
            ApprovalVerifier::new("project-one", key.verifying_key().to_bytes()).unwrap();
        let mut data = request(100, "project-one");
        data.expires_at = data.issued_at + MAX_LIFETIME_SECS + 1;
        assert_eq!(
            verifier.verify_at(&sign(data, &key), 101).err(),
            Some(ApprovalError::InvalidWindow)
        );

        let mut data = request(100, "project-one");
        data.reason = " ".into();
        assert_eq!(
            verifier.verify_at(&sign(data, &key), 101).err(),
            Some(ApprovalError::InvalidReason)
        );
    }

    #[test]
    fn rejects_missing_or_broken_signature() {
        let key = signing_key();
        let verifier =
            ApprovalVerifier::new("project-one", key.verifying_key().to_bytes()).unwrap();
        let mut approval = sign(request(100, "project-one"), &key);
        approval.signature.clear();
        assert_eq!(
            verifier.verify_at(&approval, 101).err(),
            Some(ApprovalError::InvalidSignature)
        );
    }
}
