#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const TRUST_POLICY_SCHEMA_VERSION: &str = "canon.trust.policy.v1";
pub const TRUST_ATTESTATION_SCHEMA_VERSION: &str = "canon.trust.attestation.v1";
pub const TRUST_VERIFICATION_RECEIPT_SCHEMA_VERSION: &str = "canon.trust.verification.receipt.v1";
pub const LOCAL_TEST_SIGNATURE_ALGORITHM: &str = "canon.local-test.blake3-keyed.v1";

pub type TrustResult<T> = Result<T, TrustError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustPolicy {
    pub schema_version: String,
    pub policy_id: String,
    pub policy_version: String,
    pub roots: Vec<TrustRoot>,
    pub trusted_signers: Vec<TrustedSigner>,
    pub thresholds: Vec<TrustThreshold>,
    pub revocations: Vec<TrustRevocation>,
    pub unsigned_local_workflows: UnsignedLocalWorkflowPolicy,
    pub transparency: TransparencyPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustRoot {
    pub root_id: String,
    pub issuer: String,
    pub key_id: String,
    pub key_hex: String,
    pub offline: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedSigner {
    pub signer_id: String,
    pub issuer: String,
    pub key_id: String,
    pub allowed_attestation_kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustThreshold {
    pub attestation_kind: String,
    pub required_signatures: usize,
    pub accepted_signer_ids: Vec<String>,
    pub require_distinct_issuers: bool,
    pub required_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustRevocation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    pub revoked_at: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsignedLocalWorkflowPolicy {
    pub allow: bool,
    pub allowed_package_schemas: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransparencyPolicy {
    pub mode: TransparencyMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_log_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransparencyMode {
    OfflineRootsOnly,
    RequireLogDigest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustWorkflow {
    Deployment,
    Promotion,
    LocalUnsigned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustVerificationRequest {
    pub policy: TrustPolicy,
    pub subject: TrustSubject,
    pub required_claims: TrustAttestationClaims,
    pub attestations: Vec<TrustAttestation>,
    pub verification_time: String,
    pub workflow: TrustWorkflow,
    pub semantic_package_verification: SemanticPackageVerification,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transparency_log_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticPackageVerification {
    pub verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustSubject {
    pub package_schema: String,
    pub package_id: String,
    pub package_version: String,
    pub package_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oci_manifest_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oci_subject_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustAttestation {
    pub schema_version: String,
    pub attestation_id: String,
    pub kind: String,
    pub subject: TrustSubject,
    pub claims: TrustAttestationClaims,
    pub signer_id: String,
    pub issuer: String,
    pub issued_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub signature: TrustSignature,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TrustAttestationClaims {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promotion_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustSignature {
    pub algorithm: String,
    pub key_id: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustVerificationReceipt {
    pub schema_version: String,
    pub verified: bool,
    pub workflow: TrustWorkflow,
    pub policy_id: String,
    pub policy_version: String,
    pub policy_digest: String,
    pub subject_digest: String,
    pub semantic_package_verified: bool,
    pub unsigned_local_allowed: bool,
    pub signatures_checked: usize,
    pub accepted_attestations: Vec<AcceptedAttestation>,
    pub failures: Vec<TrustFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedAttestation {
    pub attestation_id: String,
    pub kind: String,
    pub signer_id: String,
    pub issuer: String,
    pub key_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustFailure {
    pub code: TrustFailureCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustFailureCode {
    SemanticPackageNotVerified,
    MissingSemanticReceipt,
    UnsignedNotAllowed,
    MissingAttestation,
    UnsupportedAttestationSchema,
    SubjectMismatch,
    MissingClaim,
    ClaimMismatch,
    UntrustedSigner,
    UntrustedIssuer,
    SignerNotAllowedForKind,
    Expired,
    NotYetValid,
    Revoked,
    InvalidSignature,
    ThresholdNotMet,
    MissingTransparency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustErrorKind {
    UnsupportedPolicySchema,
    InvalidDigest,
    InvalidKey,
    DuplicateIdentity,
    InvalidThreshold,
    Parse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustError {
    pub kind: TrustErrorKind,
    pub message: String,
}

impl TrustError {
    fn new(kind: TrustErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for TrustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for TrustError {}

pub fn verify_trust(request: &TrustVerificationRequest) -> TrustResult<TrustVerificationReceipt> {
    let policy = canonical_policy(&request.policy)?;
    validate_policy(&policy)?;
    let subject = canonical_subject(&request.subject)?;
    validate_subject(&subject)?;
    validate_claims(&request.required_claims)?;

    let mut failures = Vec::new();
    let mut accepted_attestations = Vec::new();
    let policy_digest = trust_policy_digest(&policy)?;
    let subject_digest = digest_json(&subject)?;
    let unsigned_local_allowed = request.workflow == TrustWorkflow::LocalUnsigned
        && request.attestations.is_empty()
        && policy.unsigned_local_workflows.allow
        && policy
            .unsigned_local_workflows
            .allowed_package_schemas
            .iter()
            .any(|schema| schema == &subject.package_schema);

    if !request.semantic_package_verification.verified {
        failures.push(failure(
            TrustFailureCode::SemanticPackageNotVerified,
            "semantic package verification must pass before trust verification",
            None,
        ));
    }
    match &request.semantic_package_verification.receipt_digest {
        Some(digest) => validate_digest(digest)?,
        None => failures.push(failure(
            TrustFailureCode::MissingSemanticReceipt,
            "semantic package verification receipt digest is required",
            None,
        )),
    }

    match policy.transparency.mode {
        TransparencyMode::OfflineRootsOnly => {}
        TransparencyMode::RequireLogDigest => {
            let expected = policy.transparency.required_log_digest.as_deref();
            match (expected, request.transparency_log_digest.as_deref()) {
                (Some(expected), Some(actual)) if expected == actual => validate_digest(actual)?,
                (Some(_), Some(_)) => failures.push(failure(
                    TrustFailureCode::MissingTransparency,
                    "transparency log digest does not match policy",
                    None,
                )),
                _ => failures.push(failure(
                    TrustFailureCode::MissingTransparency,
                    "policy requires a transparency log digest",
                    None,
                )),
            }
        }
    }

    if request.attestations.is_empty() {
        if request.workflow == TrustWorkflow::LocalUnsigned {
            if !unsigned_local_allowed {
                failures.push(failure(
                    TrustFailureCode::UnsignedNotAllowed,
                    "unsigned local workflow is not explicitly allowed by policy",
                    None,
                ));
            }
        } else {
            failures.push(failure(
                TrustFailureCode::MissingAttestation,
                "signed attestations are required for this workflow",
                None,
            ));
        }
    }

    let root_by_key = roots_by_key(&policy)?;
    let signer_by_id = signers_by_id(&policy)?;
    let mut attestations = request.attestations.clone();
    attestations.sort_by(|left, right| left.attestation_id.cmp(&right.attestation_id));

    for attestation in &attestations {
        match verify_one_attestation(
            attestation,
            &subject,
            &request.required_claims,
            &request.verification_time,
            &policy,
            &root_by_key,
            &signer_by_id,
        ) {
            Ok(accepted) => accepted_attestations.push(accepted),
            Err(mut local_failures) => failures.append(&mut local_failures),
        }
    }

    if !(unsigned_local_allowed && request.attestations.is_empty()) {
        for threshold in &policy.thresholds {
            verify_threshold(threshold, &accepted_attestations, &mut failures);
        }
    }

    let mut receipt = TrustVerificationReceipt {
        schema_version: TRUST_VERIFICATION_RECEIPT_SCHEMA_VERSION.to_string(),
        verified: false,
        workflow: request.workflow,
        policy_id: policy.policy_id,
        policy_version: policy.policy_version,
        policy_digest,
        subject_digest,
        semantic_package_verified: request.semantic_package_verification.verified,
        unsigned_local_allowed,
        signatures_checked: attestations.len(),
        accepted_attestations,
        failures,
    };
    canonicalize_receipt(&mut receipt);
    receipt.verified = receipt.failures.is_empty();
    Ok(receipt)
}

pub fn trust_policy_digest(policy: &TrustPolicy) -> TrustResult<String> {
    digest_json(&canonical_policy(policy)?)
}

pub fn trust_subject_digest(subject: &TrustSubject) -> TrustResult<String> {
    digest_json(&canonical_subject(subject)?)
}

pub fn canonical_receipt_bytes(receipt: &TrustVerificationReceipt) -> TrustResult<Vec<u8>> {
    let mut canonical = receipt.clone();
    canonicalize_receipt(&mut canonical);
    serde_json::to_vec(&canonical).map_err(|error| {
        TrustError::new(
            TrustErrorKind::Parse,
            format!("failed to serialize trust receipt: {error}"),
        )
    })
}

pub fn sign_attestation_with_local_test_key(
    mut attestation: TrustAttestation,
    key_hex: &str,
) -> TrustResult<TrustAttestation> {
    let key = local_test_key(key_hex)?;
    attestation.signature.value.clear();
    let bytes = attestation_signing_bytes(&attestation)?;
    let signature = blake3::keyed_hash(&key, &bytes);
    attestation.signature.value = format!("blake3:{}", signature.to_hex());
    Ok(attestation)
}

fn verify_one_attestation(
    attestation: &TrustAttestation,
    subject: &TrustSubject,
    required_claims: &TrustAttestationClaims,
    verification_time: &str,
    policy: &TrustPolicy,
    root_by_key: &BTreeMap<String, TrustRoot>,
    signer_by_id: &BTreeMap<String, TrustedSigner>,
) -> Result<AcceptedAttestation, Vec<TrustFailure>> {
    let mut failures = Vec::new();
    let attestation_id = Some(attestation.attestation_id.clone());

    if attestation.schema_version != TRUST_ATTESTATION_SCHEMA_VERSION {
        failures.push(failure(
            TrustFailureCode::UnsupportedAttestationSchema,
            format!(
                "unsupported attestation schema_version {}",
                attestation.schema_version
            ),
            attestation_id.clone(),
        ));
    }
    if canonical_subject(&attestation.subject).as_ref() != Ok(subject) {
        failures.push(failure(
            TrustFailureCode::SubjectMismatch,
            "attestation subject does not match expected package or OCI subject",
            attestation_id.clone(),
        ));
    }
    if let Err(error) = validate_claims(&attestation.claims) {
        failures.push(failure(
            TrustFailureCode::ClaimMismatch,
            error.message,
            attestation_id.clone(),
        ));
    }
    verify_required_claims(
        &attestation.kind,
        &attestation.claims,
        required_claims,
        policy,
        attestation_id.clone(),
        &mut failures,
    );

    let signer = signer_by_id.get(&attestation.signer_id);
    let Some(signer) = signer else {
        failures.push(failure(
            TrustFailureCode::UntrustedSigner,
            format!("signer {} is not trusted by policy", attestation.signer_id),
            attestation_id.clone(),
        ));
        return Err(failures);
    };
    if signer.issuer != attestation.issuer {
        failures.push(failure(
            TrustFailureCode::UntrustedIssuer,
            format!(
                "attestation issuer {} does not match signer issuer {}",
                attestation.issuer, signer.issuer
            ),
            attestation_id.clone(),
        ));
    }
    if !signer
        .allowed_attestation_kinds
        .iter()
        .any(|kind| kind == &attestation.kind)
    {
        failures.push(failure(
            TrustFailureCode::SignerNotAllowedForKind,
            format!(
                "signer {} is not allowed to issue {} attestations",
                signer.signer_id, attestation.kind
            ),
            attestation_id.clone(),
        ));
    }
    verify_time_window(
        verification_time,
        signer.valid_from.as_deref(),
        signer.expires_at.as_deref(),
        "signer",
        attestation_id.clone(),
        &mut failures,
    );

    let root = root_by_key.get(&signer.key_id);
    let Some(root) = root else {
        failures.push(failure(
            TrustFailureCode::UntrustedIssuer,
            format!("signer key {} is not rooted in policy", signer.key_id),
            attestation_id.clone(),
        ));
        return Err(failures);
    };
    if root.issuer != signer.issuer {
        failures.push(failure(
            TrustFailureCode::UntrustedIssuer,
            format!(
                "root issuer {} does not match signer issuer {}",
                root.issuer, signer.issuer
            ),
            attestation_id.clone(),
        ));
    }
    verify_time_window(
        verification_time,
        root.valid_from.as_deref(),
        root.expires_at.as_deref(),
        "root",
        attestation_id.clone(),
        &mut failures,
    );
    verify_time_window(
        verification_time,
        Some(&attestation.issued_at),
        attestation.expires_at.as_deref(),
        "attestation",
        attestation_id.clone(),
        &mut failures,
    );
    verify_revocation(
        policy,
        &attestation.signer_id,
        &attestation.issuer,
        &attestation.signature.key_id,
        verification_time,
        attestation_id.clone(),
        &mut failures,
    );
    verify_signature(attestation, root, attestation_id.clone(), &mut failures);

    if failures.is_empty() {
        Ok(AcceptedAttestation {
            attestation_id: attestation.attestation_id.clone(),
            kind: attestation.kind.clone(),
            signer_id: attestation.signer_id.clone(),
            issuer: attestation.issuer.clone(),
            key_id: attestation.signature.key_id.clone(),
        })
    } else {
        Err(failures)
    }
}

fn verify_required_claims(
    kind: &str,
    claims: &TrustAttestationClaims,
    required_claims: &TrustAttestationClaims,
    policy: &TrustPolicy,
    attestation_id: Option<String>,
    failures: &mut Vec<TrustFailure>,
) {
    let required_names = policy
        .thresholds
        .iter()
        .filter(|threshold| threshold.attestation_kind == kind)
        .flat_map(|threshold| threshold.required_claims.iter())
        .collect::<BTreeSet<_>>();

    for name in required_names {
        let expected = claim_value(required_claims, name);
        let actual = claim_value(claims, name);
        match (expected, actual) {
            (Some(expected), Some(actual)) if expected == actual => {}
            (None, _) => failures.push(failure(
                TrustFailureCode::MissingClaim,
                format!("required claim {name} is absent from verification request"),
                attestation_id.clone(),
            )),
            (_, None) => failures.push(failure(
                TrustFailureCode::MissingClaim,
                format!("attestation is missing required claim {name}"),
                attestation_id.clone(),
            )),
            (Some(expected), Some(actual)) => failures.push(failure(
                TrustFailureCode::ClaimMismatch,
                format!("claim {name} expected {expected}, found {actual}"),
                attestation_id.clone(),
            )),
        }
    }
}

fn verify_threshold(
    threshold: &TrustThreshold,
    accepted_attestations: &[AcceptedAttestation],
    failures: &mut Vec<TrustFailure>,
) {
    let allowed = threshold
        .accepted_signer_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let matching = accepted_attestations
        .iter()
        .filter(|attestation| attestation.kind == threshold.attestation_kind)
        .filter(|attestation| {
            threshold.accepted_signer_ids.is_empty() || allowed.contains(&attestation.signer_id)
        })
        .collect::<Vec<_>>();
    let distinct_signers = matching
        .iter()
        .map(|attestation| attestation.signer_id.as_str())
        .collect::<BTreeSet<_>>();
    let distinct_issuers = matching
        .iter()
        .map(|attestation| attestation.issuer.as_str())
        .collect::<BTreeSet<_>>();

    if distinct_signers.len() < threshold.required_signatures
        || (threshold.require_distinct_issuers
            && distinct_issuers.len() < threshold.required_signatures)
    {
        failures.push(failure(
            TrustFailureCode::ThresholdNotMet,
            format!(
                "threshold for {} requires {} accepted signatures{}",
                threshold.attestation_kind,
                threshold.required_signatures,
                if threshold.require_distinct_issuers {
                    " from distinct issuers"
                } else {
                    ""
                }
            ),
            None,
        ));
    }
}

fn verify_time_window(
    verification_time: &str,
    valid_from: Option<&str>,
    expires_at: Option<&str>,
    label: &str,
    attestation_id: Option<String>,
    failures: &mut Vec<TrustFailure>,
) {
    if let Some(valid_from) = valid_from
        && verification_time < valid_from
    {
        failures.push(failure(
            TrustFailureCode::NotYetValid,
            format!("{label} is not valid until {valid_from}"),
            attestation_id.clone(),
        ));
    }
    if let Some(expires_at) = expires_at
        && verification_time > expires_at
    {
        failures.push(failure(
            TrustFailureCode::Expired,
            format!("{label} expired at {expires_at}"),
            attestation_id,
        ));
    }
}

fn verify_revocation(
    policy: &TrustPolicy,
    signer_id: &str,
    issuer: &str,
    key_id: &str,
    verification_time: &str,
    attestation_id: Option<String>,
    failures: &mut Vec<TrustFailure>,
) {
    for revocation in &policy.revocations {
        if revocation.revoked_at.as_str() > verification_time {
            continue;
        }
        let signer_matches = revocation
            .signer_id
            .as_deref()
            .is_some_and(|id| id == signer_id);
        let issuer_matches = revocation.issuer.as_deref().is_some_and(|id| id == issuer);
        let key_matches = revocation.key_id.as_deref().is_some_and(|id| id == key_id);
        if signer_matches || issuer_matches || key_matches {
            failures.push(failure(
                TrustFailureCode::Revoked,
                format!("trust material revoked: {}", revocation.reason),
                attestation_id.clone(),
            ));
        }
    }
}

fn verify_signature(
    attestation: &TrustAttestation,
    root: &TrustRoot,
    attestation_id: Option<String>,
    failures: &mut Vec<TrustFailure>,
) {
    if attestation.signature.algorithm != LOCAL_TEST_SIGNATURE_ALGORITHM {
        failures.push(failure(
            TrustFailureCode::InvalidSignature,
            format!(
                "unsupported signature algorithm {}",
                attestation.signature.algorithm
            ),
            attestation_id.clone(),
        ));
        return;
    }
    if attestation.signature.key_id != root.key_id {
        failures.push(failure(
            TrustFailureCode::InvalidSignature,
            format!(
                "signature key {} does not match trusted root key {}",
                attestation.signature.key_id, root.key_id
            ),
            attestation_id.clone(),
        ));
        return;
    }
    let Ok(key) = local_test_key(&root.key_hex) else {
        failures.push(failure(
            TrustFailureCode::InvalidSignature,
            "trusted root key is not a valid local test key",
            attestation_id,
        ));
        return;
    };
    let Ok(bytes) = attestation_signing_bytes(attestation) else {
        failures.push(failure(
            TrustFailureCode::InvalidSignature,
            "failed to serialize attestation signing payload",
            attestation_id,
        ));
        return;
    };
    let expected = format!("blake3:{}", blake3::keyed_hash(&key, &bytes).to_hex());
    if attestation.signature.value != expected {
        failures.push(failure(
            TrustFailureCode::InvalidSignature,
            "attestation signature does not match canonical payload",
            attestation_id,
        ));
    }
}

fn validate_policy(policy: &TrustPolicy) -> TrustResult<()> {
    if policy.schema_version != TRUST_POLICY_SCHEMA_VERSION {
        return Err(TrustError::new(
            TrustErrorKind::UnsupportedPolicySchema,
            format!(
                "unsupported trust policy schema_version {}",
                policy.schema_version
            ),
        ));
    }
    validate_non_empty(&policy.policy_id, "policy_id")?;
    validate_non_empty(&policy.policy_version, "policy_version")?;

    let mut root_keys = BTreeSet::new();
    for root in &policy.roots {
        validate_non_empty(&root.root_id, "root_id")?;
        validate_non_empty(&root.issuer, "root.issuer")?;
        validate_non_empty(&root.key_id, "root.key_id")?;
        local_test_key(&root.key_hex)?;
        if !root_keys.insert(root.key_id.clone()) {
            return Err(TrustError::new(
                TrustErrorKind::DuplicateIdentity,
                format!("duplicate trust root key_id {}", root.key_id),
            ));
        }
    }

    let mut signer_ids = BTreeSet::new();
    for signer in &policy.trusted_signers {
        validate_non_empty(&signer.signer_id, "signer_id")?;
        validate_non_empty(&signer.issuer, "signer.issuer")?;
        validate_non_empty(&signer.key_id, "signer.key_id")?;
        if signer.allowed_attestation_kinds.is_empty() {
            return Err(TrustError::new(
                TrustErrorKind::InvalidThreshold,
                format!(
                    "signer {} must allow at least one attestation kind",
                    signer.signer_id
                ),
            ));
        }
        if !root_keys.contains(&signer.key_id) {
            return Err(TrustError::new(
                TrustErrorKind::InvalidKey,
                format!(
                    "signer {} references unknown key {}",
                    signer.signer_id, signer.key_id
                ),
            ));
        }
        if !signer_ids.insert(signer.signer_id.clone()) {
            return Err(TrustError::new(
                TrustErrorKind::DuplicateIdentity,
                format!("duplicate signer_id {}", signer.signer_id),
            ));
        }
    }

    for threshold in &policy.thresholds {
        validate_non_empty(&threshold.attestation_kind, "threshold.attestation_kind")?;
        if threshold.required_signatures == 0 {
            return Err(TrustError::new(
                TrustErrorKind::InvalidThreshold,
                "threshold required_signatures must be greater than zero",
            ));
        }
        for signer_id in &threshold.accepted_signer_ids {
            if !signer_ids.contains(signer_id) {
                return Err(TrustError::new(
                    TrustErrorKind::InvalidThreshold,
                    format!("threshold references unknown signer {signer_id}"),
                ));
            }
        }
        for claim in &threshold.required_claims {
            validate_claim_name(claim)?;
        }
    }

    if policy.transparency.mode == TransparencyMode::RequireLogDigest {
        match policy.transparency.required_log_digest.as_deref() {
            Some(digest) => validate_digest(digest)?,
            None => {
                return Err(TrustError::new(
                    TrustErrorKind::InvalidDigest,
                    "required_log_digest is mandatory when transparency mode requires a log digest",
                ));
            }
        }
    }

    Ok(())
}

fn validate_subject(subject: &TrustSubject) -> TrustResult<()> {
    validate_non_empty(&subject.package_schema, "package_schema")?;
    validate_non_empty(&subject.package_id, "package_id")?;
    validate_non_empty(&subject.package_version, "package_version")?;
    validate_digest(&subject.package_digest)?;
    if let Some(digest) = &subject.oci_manifest_digest {
        validate_digest(digest)?;
    }
    if let Some(digest) = &subject.oci_subject_digest {
        validate_digest(digest)?;
    }
    Ok(())
}

fn validate_claims(claims: &TrustAttestationClaims) -> TrustResult<()> {
    for digest in [
        claims.audit_digest.as_deref(),
        claims.review_digest.as_deref(),
        claims.source_digest.as_deref(),
        claims.promotion_digest.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        validate_digest(digest)?;
    }
    Ok(())
}

fn validate_claim_name(name: &str) -> TrustResult<()> {
    if matches!(
        name,
        "audit_digest" | "review_digest" | "source_digest" | "promotion_digest"
    ) {
        Ok(())
    } else {
        Err(TrustError::new(
            TrustErrorKind::InvalidThreshold,
            format!("unknown required claim {name}"),
        ))
    }
}

fn claim_value<'a>(claims: &'a TrustAttestationClaims, name: &str) -> Option<&'a str> {
    match name {
        "audit_digest" => claims.audit_digest.as_deref(),
        "review_digest" => claims.review_digest.as_deref(),
        "source_digest" => claims.source_digest.as_deref(),
        "promotion_digest" => claims.promotion_digest.as_deref(),
        _ => None,
    }
}

fn roots_by_key(policy: &TrustPolicy) -> TrustResult<BTreeMap<String, TrustRoot>> {
    policy
        .roots
        .iter()
        .map(|root| Ok((root.key_id.clone(), root.clone())))
        .collect()
}

fn signers_by_id(policy: &TrustPolicy) -> TrustResult<BTreeMap<String, TrustedSigner>> {
    policy
        .trusted_signers
        .iter()
        .map(|signer| Ok((signer.signer_id.clone(), signer.clone())))
        .collect()
}

fn canonical_policy(policy: &TrustPolicy) -> TrustResult<TrustPolicy> {
    let mut canonical = policy.clone();
    canonical.roots.sort_by(|left, right| {
        left.issuer
            .cmp(&right.issuer)
            .then_with(|| left.key_id.cmp(&right.key_id))
            .then_with(|| left.root_id.cmp(&right.root_id))
    });
    canonical.trusted_signers.sort_by(|left, right| {
        left.signer_id
            .cmp(&right.signer_id)
            .then_with(|| left.issuer.cmp(&right.issuer))
    });
    for signer in &mut canonical.trusted_signers {
        signer.allowed_attestation_kinds.sort();
        signer.allowed_attestation_kinds.dedup();
    }
    canonical.thresholds.sort_by(|left, right| {
        left.attestation_kind
            .cmp(&right.attestation_kind)
            .then_with(|| left.required_signatures.cmp(&right.required_signatures))
    });
    for threshold in &mut canonical.thresholds {
        threshold.accepted_signer_ids.sort();
        threshold.accepted_signer_ids.dedup();
        threshold.required_claims.sort();
        threshold.required_claims.dedup();
    }
    canonical.revocations.sort_by(|left, right| {
        left.revoked_at
            .cmp(&right.revoked_at)
            .then_with(|| left.signer_id.cmp(&right.signer_id))
            .then_with(|| left.issuer.cmp(&right.issuer))
            .then_with(|| left.key_id.cmp(&right.key_id))
    });
    canonical
        .unsigned_local_workflows
        .allowed_package_schemas
        .sort();
    canonical
        .unsigned_local_workflows
        .allowed_package_schemas
        .dedup();
    Ok(canonical)
}

fn canonical_subject(subject: &TrustSubject) -> TrustResult<TrustSubject> {
    let canonical = subject.clone();
    validate_subject(&canonical)?;
    Ok(canonical)
}

fn canonicalize_receipt(receipt: &mut TrustVerificationReceipt) {
    receipt.accepted_attestations.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.signer_id.cmp(&right.signer_id))
            .then_with(|| left.attestation_id.cmp(&right.attestation_id))
    });
    receipt.failures.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| left.attestation_id.cmp(&right.attestation_id))
            .then_with(|| left.message.cmp(&right.message))
    });
}

fn attestation_signing_bytes(attestation: &TrustAttestation) -> TrustResult<Vec<u8>> {
    let mut canonical = attestation.clone();
    canonical.signature.value.clear();
    serde_json::to_vec(&canonical).map_err(|error| {
        TrustError::new(
            TrustErrorKind::Parse,
            format!("failed to serialize attestation signing payload: {error}"),
        )
    })
}

fn digest_json<T: Serialize>(value: &T) -> TrustResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        TrustError::new(
            TrustErrorKind::Parse,
            format!("failed to serialize trust digest payload: {error}"),
        )
    })?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn validate_non_empty(value: &str, field: &str) -> TrustResult<()> {
    if value.trim().is_empty() {
        Err(TrustError::new(
            TrustErrorKind::Parse,
            format!("{field} must not be empty"),
        ))
    } else {
        Ok(())
    }
}

fn validate_digest(digest: &str) -> TrustResult<()> {
    let Some(hex) = digest.strip_prefix("blake3:") else {
        return Err(TrustError::new(
            TrustErrorKind::InvalidDigest,
            format!("invalid BLAKE3 digest {digest}"),
        ));
    };
    if hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(TrustError::new(
            TrustErrorKind::InvalidDigest,
            format!("invalid BLAKE3 digest {digest}"),
        ))
    }
}

fn local_test_key(key_hex: &str) -> TrustResult<[u8; 32]> {
    let bytes = decode_hex(key_hex)?;
    bytes.try_into().map_err(|_| {
        TrustError::new(
            TrustErrorKind::InvalidKey,
            "local test key must decode to exactly 32 bytes",
        )
    })
}

fn decode_hex(hex: &str) -> TrustResult<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return Err(TrustError::new(
            TrustErrorKind::InvalidKey,
            "hex string has odd length",
        ));
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut offset = 0;
    while offset < hex.len() {
        let text = hex.get(offset..offset + 2).ok_or_else(|| {
            TrustError::new(TrustErrorKind::InvalidKey, "invalid hex string boundary")
        })?;
        bytes.push(u8::from_str_radix(text, 16).map_err(|error| {
            TrustError::new(
                TrustErrorKind::InvalidKey,
                format!("invalid hex string: {error}"),
            )
        })?);
        offset += 2;
    }
    Ok(bytes)
}

fn failure(
    code: TrustFailureCode,
    message: impl Into<String>,
    attestation_id: Option<String>,
) -> TrustFailure {
    TrustFailure {
        code,
        message: message.into(),
        attestation_id,
    }
}
