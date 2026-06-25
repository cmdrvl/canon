use canon::namekit::ReasonCode;
use canon::namekit::tenant::{cmbs_tenant_pair_evidence, normalize_cmbs_tenant};

#[test]
fn cmbs_tenant_normalization_converges_sears_variants_without_row_identity() {
    for raw in ["SEARS LLC", "Sears", "Sears Roebuck & Co.", "Sears #1234"] {
        let normalized = normalize_cmbs_tenant(raw);
        assert_eq!(normalized.tenant_core, "sears", "{raw}");
        assert_eq!(normalized.tenant_tokens, ["sears"], "{raw}");
        assert!(normalized.protected_tokens.is_empty(), "{raw}");
        assert!(
            normalized
                .reasons
                .iter()
                .any(|reason| reason.code == ReasonCode::RareTokenSupport),
            "{raw}"
        );
    }

    let sears_llc = normalize_cmbs_tenant("SEARS LLC");
    assert_eq!(sears_llc.stripped_legal_suffixes, ["llc"]);
    assert!(sears_llc.reason_codes().contains(&"legal_suffix_stripped"));
}

#[test]
#[allow(non_snake_case)]
fn CMBS_I001_sears_variants_have_positive_tenant_label_support() {
    for (left, right) in [
        ("Sears", "SEARS LLC"),
        ("Sears", "Sears Roebuck & Co."),
        ("SEARS LLC", "Sears #1234"),
    ] {
        let evidence = cmbs_tenant_pair_evidence(left, right);
        assert!(evidence.same_tenant_label_support, "{left} vs {right}");
        assert!(!evidence.requires_review, "{left} vs {right}");
        assert_eq!(evidence.shared_tokens, ["sears"]);
        assert!(
            evidence
                .reasons
                .iter()
                .any(|reason| reason.code == ReasonCode::RareTokenSupport)
        );
    }
}

#[test]
fn cmbs_tenant_normalization_preserves_dangerous_distinctions() {
    for (raw, protected) in [
        ("Sears Auto Center", &["auto", "center"][..]),
        ("Kmart", &["kmart"][..]),
        ("Transform SR LLC", &["sr", "transform"][..]),
        ("Sears Holdings", &["holdings"][..]),
    ] {
        let normalized = normalize_cmbs_tenant(raw);
        assert_eq!(normalized.protected_tokens, protected, "{raw}");
        assert!(
            normalized
                .reason_codes()
                .contains(&"profile_token_preserved"),
            "{raw}"
        );
    }

    let sears_auto = cmbs_tenant_pair_evidence("Sears", "Sears Auto Center");
    assert!(!sears_auto.same_tenant_label_support);
    assert!(sears_auto.requires_review);
    assert!(
        sears_auto
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::ProtectedTokenConflict)
    );
}
