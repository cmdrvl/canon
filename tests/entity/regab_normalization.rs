use canon::{
    entity::profiles::regab::{
        REGAB_FIRM_NORMALIZATION_VERSION, RegabReviewCue, normalize_regab_firm_name,
    },
    namekit::legal_suffix::{
        LEGAL_SUFFIX_PRESERVED_PROFILE, LEGAL_SUFFIX_STRIPPED, PROTECTED_LEGAL_TOKEN_RETAINED,
    },
};

#[test]
fn regab_firm_normalization_preserves_regulated_legal_form_terms() {
    let national_association = normalize_regab_firm_name("PNC Bank, National Association");
    let abbreviated = normalize_regab_firm_name("PNC Bank N.A.");

    assert_eq!(
        national_association.version,
        REGAB_FIRM_NORMALIZATION_VERSION
    );
    assert_eq!(
        national_association.firm_core,
        "pnc bank national association"
    );
    assert_eq!(abbreviated.firm_core, "pnc bank n a");
    assert_eq!(
        national_association.regulated_form_key,
        abbreviated.regulated_form_key
    );
    assert_eq!(
        national_association.regulated_form_key,
        "pnc bank national association"
    );
    assert!(
        national_association
            .legal_form
            .preserved_terms
            .contains(&"national association".to_string())
    );
    assert!(
        abbreviated
            .legal_form
            .preserved_terms
            .contains(&"n a".to_string())
    );
    assert!(
        national_association
            .reason_codes
            .contains(&LEGAL_SUFFIX_PRESERVED_PROFILE.to_string())
    );
    assert!(
        abbreviated
            .reason_codes
            .contains(&PROTECTED_LEGAL_TOKEN_RETAINED.to_string())
    );
}

#[test]
fn regab_hierarchy_anti_collapse_flags_divisions_without_rewriting_parentage() {
    let normalized = normalize_regab_firm_name(
        "Midland Loan Services, a division of PNC Bank, National Association",
    );

    assert_eq!(
        normalized.firm_core,
        "midland loan services a division of pnc bank national association"
    );
    assert_eq!(
        normalized.regulated_form_key,
        "midland loan services a division of pnc bank national association"
    );
    assert!(
        normalized
            .review_cues
            .contains(&RegabReviewCue::DivisionBoundary)
    );
    assert!(
        normalized
            .reason_codes
            .contains(&"division_boundary".to_string())
    );
    assert!(
        normalized
            .legal_form
            .preserved_terms
            .contains(&"bank".to_string())
    );
    assert!(
        normalized
            .legal_form
            .preserved_terms
            .contains(&"national association".to_string())
    );
}

#[test]
fn regab_firm_normalization_strips_unprotected_company_suffixes_only() {
    let normalized = normalize_regab_firm_name("3650 REIT Loan Servicing LLC");

    assert_eq!(normalized.firm_core, "3650 reit loan servicing");
    assert_eq!(normalized.regulated_form_key, "3650 reit loan servicing");
    assert_eq!(normalized.legal_form.stripped_terms, ["llc"]);
    assert!(
        normalized
            .reason_codes
            .contains(&LEGAL_SUFFIX_STRIPPED.to_string())
    );
}

#[test]
fn regab_platform_and_role_words_stay_review_cues_not_support_evidence() {
    let normalized = normalize_regab_firm_name("Platform Servicer Agent Category");

    assert!(
        normalized
            .review_cues
            .contains(&RegabReviewCue::PlatformLabel)
    );
    assert!(
        normalized
            .review_cues
            .contains(&RegabReviewCue::AgentOrCapacityRole)
    );
    assert!(
        normalized
            .reason_codes
            .contains(&"platform_label_guard".to_string())
    );
    assert!(
        normalized
            .reason_codes
            .contains(&"role_capacity_guard".to_string())
    );
}
