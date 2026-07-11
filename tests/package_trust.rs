#![forbid(unsafe_code)]

#[path = "../src/distribution/trust.rs"]
mod trust;

use serde_json::Value;
use trust::{
    AcceptedAttestation, LOCAL_TEST_SIGNATURE_ALGORITHM, SemanticPackageVerification,
    TransparencyMode, TransparencyPolicy, TrustAttestation, TrustAttestationClaims,
    TrustFailureCode, TrustPolicy, TrustRevocation, TrustRoot, TrustSignature, TrustSubject,
    TrustThreshold, TrustVerificationRequest, TrustWorkflow, TrustedSigner,
    UnsignedLocalWorkflowPolicy, canonical_receipt_bytes, sign_attestation_with_local_test_key,
    trust_policy_digest, trust_subject_digest, verify_trust,
};

const TRUST_SCHEMA: &str = include_str!("../schemas/canon.trust.policy.v1.schema.json");
const KEY_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const KEY_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const NOW: &str = "2026-07-10T00:00:00Z";

#[test]
fn schema_declares_fail_closed_trust_policy_contract() {
    let schema: Value = serde_json::from_str(TRUST_SCHEMA).expect("schema parses");
    assert_eq!(schema["title"], "canon.trust.policy.v1");
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        "canon.trust.policy.v1"
    );
    for field in [
        "roots",
        "trusted_signers",
        "thresholds",
        "revocations",
        "unsigned_local_workflows",
        "transparency",
    ] {
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&Value::String(field.to_string())),
            "schema requires {field}"
        );
    }
    assert_eq!(
        schema["x-canon-contract"]["semantic_package_verification_required"],
        true
    );
    assert!(
        schema["x-canon-contract"]["unsigned_local_workflows"]
            .as_str()
            .unwrap()
            .contains("fail-closed")
    );
}

#[test]
fn two_person_promotion_attestations_verify_and_receipt_is_deterministic() {
    let policy = sample_policy(false);
    let request = TrustVerificationRequest {
        policy: shuffled_policy(policy.clone()),
        subject: subject(),
        required_claims: claims(),
        attestations: vec![
            signed_attestation("att-b", "signer-b", "issuer-b", "key-b", KEY_B),
            signed_attestation("att-a", "signer-a", "issuer-a", "key-a", KEY_A),
        ],
        verification_time: NOW.to_string(),
        workflow: TrustWorkflow::Promotion,
        semantic_package_verification: semantic_verified(),
        transparency_log_digest: None,
    };

    let receipt = verify_trust(&request).expect("trust verifies");
    assert!(receipt.verified, "{:?}", receipt.failures);
    assert_eq!(
        receipt.subject_digest,
        trust_subject_digest(&subject()).expect("subject digest")
    );
    assert_eq!(receipt.signatures_checked, 2);
    assert_eq!(
        receipt.accepted_attestations,
        vec![
            accepted("att-a", "signer-a", "issuer-a", "key-a"),
            accepted("att-b", "signer-b", "issuer-b", "key-b"),
        ]
    );

    let first = canonical_receipt_bytes(&receipt).expect("receipt serializes");
    let second = canonical_receipt_bytes(&verify_trust(&request).unwrap()).unwrap();
    assert_eq!(first, second);

    let mut changed_policy = policy;
    changed_policy.policy_version = "1.0.1".to_string();
    assert_ne!(
        receipt.policy_digest,
        trust_policy_digest(&changed_policy).expect("changed policy digest")
    );
}

#[test]
fn wrong_subject_stale_claim_and_missing_semantic_receipt_fail_closed() {
    let mut wrong_subject = signed_attestation("att-a", "signer-a", "issuer-a", "key-a", KEY_A);
    wrong_subject.subject.package_digest = digest('9');
    let receipt = verify_trust(&request_with_attestations(vec![
        wrong_subject,
        signed_attestation("att-b", "signer-b", "issuer-b", "key-b", KEY_B),
    ]))
    .unwrap();
    assert!(!receipt.verified);
    assert!(has_failure(
        &receipt.failures,
        TrustFailureCode::SubjectMismatch
    ));

    let mut stale_audit = signed_attestation("att-a", "signer-a", "issuer-a", "key-a", KEY_A);
    stale_audit.claims.audit_digest = Some(digest('8'));
    let receipt = verify_trust(&request_with_attestations(vec![
        stale_audit,
        signed_attestation("att-b", "signer-b", "issuer-b", "key-b", KEY_B),
    ]))
    .unwrap();
    assert!(!receipt.verified);
    assert!(has_failure(
        &receipt.failures,
        TrustFailureCode::ClaimMismatch
    ));

    let mut request = request_with_attestations(vec![
        signed_attestation("att-a", "signer-a", "issuer-a", "key-a", KEY_A),
        signed_attestation("att-b", "signer-b", "issuer-b", "key-b", KEY_B),
    ]);
    request.semantic_package_verification = SemanticPackageVerification {
        verified: false,
        receipt_digest: None,
    };
    let receipt = verify_trust(&request).unwrap();
    assert!(!receipt.verified);
    assert!(has_failure(
        &receipt.failures,
        TrustFailureCode::SemanticPackageNotVerified
    ));
    assert!(has_failure(
        &receipt.failures,
        TrustFailureCode::MissingSemanticReceipt
    ));
}

#[test]
fn expired_revoked_untrusted_and_bad_signature_material_fail_closed() {
    let mut expired = sample_policy(false);
    expired.trusted_signers[0].expires_at = Some("2026-01-01T00:00:00Z".to_string());
    let mut request = request_with_attestations(vec![
        signed_attestation("att-a", "signer-a", "issuer-a", "key-a", KEY_A),
        signed_attestation("att-b", "signer-b", "issuer-b", "key-b", KEY_B),
    ]);
    request.policy = expired;
    let receipt = verify_trust(&request).unwrap();
    assert!(!receipt.verified);
    assert!(has_failure(&receipt.failures, TrustFailureCode::Expired));

    let mut revoked = sample_policy(false);
    revoked.revocations.push(TrustRevocation {
        signer_id: Some("signer-a".to_string()),
        issuer: None,
        key_id: None,
        revoked_at: "2026-01-01T00:00:00Z".to_string(),
        reason: "operator rotated key".to_string(),
    });
    let mut request = request_with_attestations(vec![
        signed_attestation("att-a", "signer-a", "issuer-a", "key-a", KEY_A),
        signed_attestation("att-b", "signer-b", "issuer-b", "key-b", KEY_B),
    ]);
    request.policy = revoked;
    let receipt = verify_trust(&request).unwrap();
    assert!(!receipt.verified);
    assert!(has_failure(&receipt.failures, TrustFailureCode::Revoked));

    let mut untrusted = signed_attestation("att-a", "signer-a", "issuer-a", "key-a", KEY_A);
    untrusted.signer_id = "unknown-signer".to_string();
    let receipt = verify_trust(&request_with_attestations(vec![
        untrusted,
        signed_attestation("att-b", "signer-b", "issuer-b", "key-b", KEY_B),
    ]))
    .unwrap();
    assert!(!receipt.verified);
    assert!(has_failure(
        &receipt.failures,
        TrustFailureCode::UntrustedSigner
    ));

    let mut bad_signature = signed_attestation("att-a", "signer-a", "issuer-a", "key-a", KEY_A);
    bad_signature.signature.value = digest('0');
    let receipt = verify_trust(&request_with_attestations(vec![
        bad_signature,
        signed_attestation("att-b", "signer-b", "issuer-b", "key-b", KEY_B),
    ]))
    .unwrap();
    assert!(!receipt.verified);
    assert!(has_failure(
        &receipt.failures,
        TrustFailureCode::InvalidSignature
    ));
}

#[test]
fn unsigned_local_and_transparency_paths_are_explicit_policy_choices() {
    let mut unsigned_request = TrustVerificationRequest {
        policy: sample_policy(false),
        subject: subject(),
        required_claims: claims(),
        attestations: Vec::new(),
        verification_time: NOW.to_string(),
        workflow: TrustWorkflow::LocalUnsigned,
        semantic_package_verification: semantic_verified(),
        transparency_log_digest: None,
    };
    let receipt = verify_trust(&unsigned_request).unwrap();
    assert!(!receipt.verified);
    assert!(has_failure(
        &receipt.failures,
        TrustFailureCode::UnsignedNotAllowed
    ));

    unsigned_request.policy = sample_policy(true);
    let receipt = verify_trust(&unsigned_request).unwrap();
    assert!(receipt.verified, "{:?}", receipt.failures);
    assert!(receipt.unsigned_local_allowed);
    assert_eq!(receipt.signatures_checked, 0);

    let mut transparency = request_with_attestations(vec![
        signed_attestation("att-a", "signer-a", "issuer-a", "key-a", KEY_A),
        signed_attestation("att-b", "signer-b", "issuer-b", "key-b", KEY_B),
    ]);
    transparency.policy.transparency = TransparencyPolicy {
        mode: TransparencyMode::RequireLogDigest,
        required_log_digest: Some(digest('7')),
    };
    let receipt = verify_trust(&transparency).unwrap();
    assert!(!receipt.verified);
    assert!(has_failure(
        &receipt.failures,
        TrustFailureCode::MissingTransparency
    ));

    transparency.transparency_log_digest = Some(digest('7'));
    let receipt = verify_trust(&transparency).unwrap();
    assert!(receipt.verified, "{:?}", receipt.failures);
}

fn request_with_attestations(attestations: Vec<TrustAttestation>) -> TrustVerificationRequest {
    TrustVerificationRequest {
        policy: sample_policy(false),
        subject: subject(),
        required_claims: claims(),
        attestations,
        verification_time: NOW.to_string(),
        workflow: TrustWorkflow::Promotion,
        semantic_package_verification: semantic_verified(),
        transparency_log_digest: None,
    }
}

fn sample_policy(allow_unsigned: bool) -> TrustPolicy {
    TrustPolicy {
        schema_version: "canon.trust.policy.v1".to_string(),
        policy_id: "trust.promotion.v1".to_string(),
        policy_version: "1.0.0".to_string(),
        roots: vec![
            TrustRoot {
                root_id: "root-a".to_string(),
                issuer: "issuer-a".to_string(),
                key_id: "key-a".to_string(),
                key_hex: KEY_A.to_string(),
                offline: true,
                valid_from: Some("2026-01-01T00:00:00Z".to_string()),
                expires_at: Some("2027-01-01T00:00:00Z".to_string()),
            },
            TrustRoot {
                root_id: "root-b".to_string(),
                issuer: "issuer-b".to_string(),
                key_id: "key-b".to_string(),
                key_hex: KEY_B.to_string(),
                offline: true,
                valid_from: Some("2026-01-01T00:00:00Z".to_string()),
                expires_at: Some("2027-01-01T00:00:00Z".to_string()),
            },
        ],
        trusted_signers: vec![
            TrustedSigner {
                signer_id: "signer-a".to_string(),
                issuer: "issuer-a".to_string(),
                key_id: "key-a".to_string(),
                allowed_attestation_kinds: vec!["promotion".to_string()],
                valid_from: Some("2026-01-01T00:00:00Z".to_string()),
                expires_at: Some("2027-01-01T00:00:00Z".to_string()),
            },
            TrustedSigner {
                signer_id: "signer-b".to_string(),
                issuer: "issuer-b".to_string(),
                key_id: "key-b".to_string(),
                allowed_attestation_kinds: vec!["promotion".to_string()],
                valid_from: Some("2026-01-01T00:00:00Z".to_string()),
                expires_at: Some("2027-01-01T00:00:00Z".to_string()),
            },
        ],
        thresholds: vec![TrustThreshold {
            attestation_kind: "promotion".to_string(),
            required_signatures: 2,
            accepted_signer_ids: vec!["signer-a".to_string(), "signer-b".to_string()],
            require_distinct_issuers: true,
            required_claims: vec![
                "audit_digest".to_string(),
                "review_digest".to_string(),
                "source_digest".to_string(),
            ],
        }],
        revocations: Vec::new(),
        unsigned_local_workflows: UnsignedLocalWorkflowPolicy {
            allow: allow_unsigned,
            allowed_package_schemas: vec!["canon.strategy.package.v1".to_string()],
            reason: "local experimentation only".to_string(),
        },
        transparency: TransparencyPolicy {
            mode: TransparencyMode::OfflineRootsOnly,
            required_log_digest: None,
        },
    }
}

fn shuffled_policy(mut policy: TrustPolicy) -> TrustPolicy {
    policy.roots.reverse();
    policy.trusted_signers.reverse();
    policy.thresholds[0].accepted_signer_ids.reverse();
    policy.thresholds[0].required_claims.reverse();
    policy
}

fn subject() -> TrustSubject {
    TrustSubject {
        package_schema: "canon.strategy.package.v1".to_string(),
        package_id: "pkg.strategy".to_string(),
        package_version: "1.2.3".to_string(),
        package_digest: digest('1'),
        oci_manifest_digest: Some(digest('2')),
        oci_subject_digest: Some(digest('3')),
    }
}

fn claims() -> TrustAttestationClaims {
    TrustAttestationClaims {
        audit_digest: Some(digest('4')),
        review_digest: Some(digest('5')),
        source_digest: Some(digest('6')),
        promotion_digest: None,
    }
}

fn signed_attestation(
    attestation_id: &str,
    signer_id: &str,
    issuer: &str,
    key_id: &str,
    key_hex: &str,
) -> TrustAttestation {
    let attestation = TrustAttestation {
        schema_version: "canon.trust.attestation.v1".to_string(),
        attestation_id: attestation_id.to_string(),
        kind: "promotion".to_string(),
        subject: subject(),
        claims: claims(),
        signer_id: signer_id.to_string(),
        issuer: issuer.to_string(),
        issued_at: "2026-06-01T00:00:00Z".to_string(),
        expires_at: Some("2026-12-01T00:00:00Z".to_string()),
        signature: TrustSignature {
            algorithm: LOCAL_TEST_SIGNATURE_ALGORITHM.to_string(),
            key_id: key_id.to_string(),
            value: String::new(),
        },
    };
    sign_attestation_with_local_test_key(attestation, key_hex).expect("attestation signs")
}

fn semantic_verified() -> SemanticPackageVerification {
    SemanticPackageVerification {
        verified: true,
        receipt_digest: Some(digest('a')),
    }
}

fn accepted(
    attestation_id: &str,
    signer_id: &str,
    issuer: &str,
    key_id: &str,
) -> AcceptedAttestation {
    AcceptedAttestation {
        attestation_id: attestation_id.to_string(),
        kind: "promotion".to_string(),
        signer_id: signer_id.to_string(),
        issuer: issuer.to_string(),
        key_id: key_id.to_string(),
    }
}

fn has_failure(failures: &[trust::TrustFailure], code: TrustFailureCode) -> bool {
    failures.iter().any(|failure| failure.code == code)
}

fn digest(hex: char) -> String {
    format!("blake3:{}", hex.to_string().repeat(64))
}
