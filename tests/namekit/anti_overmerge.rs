use canon::namekit::explain::{
    MergeEvidenceRole, NamekitReason, ProtectedTokenLane, ReasonCode, ReasonStage,
    protected_token_conflict_reason, protected_token_preserved_reason, sort_reasons,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;

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

#[test]
fn namekit_anti_overmerge_negative_fixtures_cover_required_pairs() {
    let rows = anti_overmerge_rows();
    let case_ids = rows
        .iter()
        .map(|row| string(row, "case_id"))
        .collect::<BTreeSet<_>>();

    for required in [
        "sears_vs_sears_auto_center_tenant",
        "sears_vs_kmart_distinct_brand",
        "sears_vs_transform_sr_successor",
        "pnc_bank_vs_midland_loan_services",
        "pnc_bank_vs_pnc_capital_markets",
        "platform_label_vs_regulated_firm",
        "parent_subsidiary_spv_near_name",
    ] {
        assert!(
            case_ids.contains(required),
            "missing fixture case {required}"
        );
    }

    for row in &rows {
        assert!(
            bool_field(row, "support_views_may_be_similar"),
            "{} must prove support-like views can still be unsafe",
            string(row, "case_id")
        );
        assert!(
            !strings(row, "protected_tokens").is_empty(),
            "{} must name protected tokens",
            string(row, "case_id")
        );
        assert!(
            !string(row, "why_auto_merge_wrong").is_empty(),
            "{} must explain why auto-merge is wrong",
            string(row, "case_id")
        );

        let evidence = row["expected_evidence"]
            .as_array()
            .expect("expected_evidence must be an array");
        assert!(
            evidence.iter().any(|item| item["kind"] == "cannot_link")
                || evidence.iter().any(|item| item["kind"] == "relation_hint"),
            "{} must carry cannot_link or relation_hint evidence",
            string(row, "case_id")
        );
        assert!(
            evidence.iter().all(|item| item["kind"] != "low_similarity"),
            "{} must not rely on low similarity as the negative proof",
            string(row, "case_id")
        );
    }
}

#[test]
fn protected_token_lanes_cover_tenant_regulated_platform_and_distinctness() {
    let rows = anti_overmerge_rows();
    let lanes = rows
        .iter()
        .flat_map(|row| strings(row, "protected_lanes"))
        .collect::<BTreeSet<_>>();

    for required in [
        ProtectedTokenLane::TenantProtectedBrand.as_str(),
        ProtectedTokenLane::RegulatedLegalIdentity.as_str(),
        ProtectedTokenLane::PlatformCategory.as_str(),
        ProtectedTokenLane::ProfileDistinctness.as_str(),
    ] {
        assert!(
            lanes.contains(required),
            "missing protected lane {required}"
        );
    }

    let pnc_capital = rows
        .iter()
        .find(|row| string(row, "case_id") == "pnc_bank_vs_pnc_capital_markets")
        .expect("PNC affiliate case exists");
    assert_eq!(strings(pnc_capital, "support_views"), ["pnc"]);
    assert!(
        pnc_capital["expected_evidence"]
            .as_array()
            .expect("evidence")
            .iter()
            .any(
                |item| item["kind"] == "relation_hint" && item["code"] == "affiliate_relation_hint"
            )
    );
}

fn anti_overmerge_rows() -> Vec<Value> {
    fs::read_to_string("tests/fixtures/namekit/anti_overmerge/negative_pairs.jsonl")
        .expect("anti-overmerge fixture is readable")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("anti-overmerge line is valid JSON"))
        .collect()
}

fn string<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or("")
}

fn strings<'a>(row: &'a Value, key: &str) -> Vec<&'a str> {
    row[key]
        .as_array()
        .expect("fixture field must be an array")
        .iter()
        .map(|value| value.as_str().expect("fixture item must be a string"))
        .collect()
}

fn bool_field(row: &Value, key: &str) -> bool {
    row[key]
        .as_bool()
        .unwrap_or_else(|| panic!("{key} must be a bool in {row}"))
}
