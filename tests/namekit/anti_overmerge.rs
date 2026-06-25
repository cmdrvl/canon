use canon::namekit::explain::{
    MergeEvidenceRole, NamekitReason, ProtectedTokenLane, ReasonCode, ReasonStage,
    protected_token_conflict_reason, protected_token_preserved_reason, sort_reasons,
};
use serde_json::json;

#[test]
fn protected_token_reason_codes_are_profile_scoped_and_non_support() {
    let reason = protected_token_conflict_reason(
        "cmbs_tenant_label",
        ProtectedTokenLane::TenantProtectedBrand,
        "sears",
        "sears auto center",
    );

    assert_eq!(reason.code.as_str(), "protected_token_conflict");
    assert_eq!(reason.stage, ReasonStage::ProtectedToken);
    assert_eq!(reason.merge_evidence_role(), MergeEvidenceRole::CannotLink);
    assert!(!reason.can_support_merge());
    assert_eq!(reason.detail["profile_id"], "cmbs_tenant_label");
    assert_eq!(reason.detail["lane"], "tenant_protected_brand");
    assert_eq!(reason.detail["left_token"], "sears");
    assert_eq!(reason.detail["right_token"], "sears auto center");

    let payload = serde_json::to_value(reason).expect("reason serializes");
    assert_eq!(payload["code"], "protected_token_conflict");
    assert_eq!(payload["stage"], "protected_token");
    assert_eq!(payload["detail"]["profile_id"], "cmbs_tenant_label");
}

#[test]
fn namekit_anti_overmerge_preserve_reasons_are_review_context_not_support() {
    let reason = protected_token_preserved_reason(
        "regab_firm_identity",
        ProtectedTokenLane::RegulatedLegalIdentity,
        "national association",
    );

    assert_eq!(reason.code, ReasonCode::ProfileTokenPreserved);
    assert_eq!(
        reason.merge_evidence_role(),
        MergeEvidenceRole::ReviewContext
    );
    assert!(!reason.can_support_merge());
    assert_eq!(reason.detail["profile_id"], "regab_firm_identity");
    assert_eq!(reason.detail["lane"], "regulated_legal_identity");
    assert_eq!(reason.detail["token"], "national association");
}

#[test]
fn namekit_anti_overmerge_support_role_is_explicit_and_narrow() {
    let support = NamekitReason::new(ReasonCode::RareTokenSupport, ReasonStage::Tfidf);
    let cutoff = NamekitReason::new(ReasonCode::MetricCutoff, ReasonStage::Similarity);
    let transform = NamekitReason::new(ReasonCode::LegalSuffixStripped, ReasonStage::LegalSuffix);
    let conflict = protected_token_conflict_reason(
        "cmbs_tenant_label",
        ProtectedTokenLane::TenantProtectedBrand,
        "sears",
        "kmart",
    );

    assert_eq!(support.merge_evidence_role(), MergeEvidenceRole::Support);
    assert!(support.can_support_merge());
    assert_eq!(
        cutoff.merge_evidence_role(),
        MergeEvidenceRole::ReviewContext
    );
    assert_eq!(
        transform.merge_evidence_role(),
        MergeEvidenceRole::Transform
    );
    assert_eq!(
        conflict.merge_evidence_role(),
        MergeEvidenceRole::CannotLink
    );
    assert!(!cutoff.can_support_merge());
    assert!(!transform.can_support_merge());
    assert!(!conflict.can_support_merge());
}

#[test]
fn protected_token_reason_order_is_stable_for_review_and_explain() {
    let mut reasons = vec![
        protected_token_preserved_reason(
            "regab_firm_identity",
            ProtectedTokenLane::RegulatedLegalIdentity,
            "bank",
        ),
        protected_token_conflict_reason(
            "cmbs_tenant_label",
            ProtectedTokenLane::TenantProtectedBrand,
            "sears",
            "sears auto center",
        ),
        NamekitReason::new(ReasonCode::RareTokenSupport, ReasonStage::Tfidf)
            .with_detail("token", "roebuck"),
    ];

    sort_reasons(&mut reasons);
    let payload = serde_json::to_value(&reasons).expect("reasons serialize");
    assert_eq!(
        payload,
        json!([
            {
                "code": "rare_token_support",
                "stage": "tfidf",
                "lossy": false,
                "summary": "a rare token contributed positive support evidence",
                "detail": {"token": "roebuck"}
            },
            {
                "code": "protected_token_conflict",
                "stage": "protected_token",
                "lossy": false,
                "summary": "protected tokens conflict and must not support an auto-merge",
                "source": "canon_profile",
                "detail": {
                    "lane": "tenant_protected_brand",
                    "left_token": "sears",
                    "profile_id": "cmbs_tenant_label",
                    "right_token": "sears auto center"
                }
            },
            {
                "code": "profile_token_preserved",
                "stage": "profile_policy",
                "lossy": false,
                "summary": "profile policy preserved a token that another view might drop",
                "source": "canon_profile",
                "detail": {
                    "lane": "regulated_legal_identity",
                    "profile_id": "regab_firm_identity",
                    "token": "bank"
                }
            }
        ])
    );
}
