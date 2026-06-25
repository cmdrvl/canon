mod namekit {
    pub use canon::namekit::*;
}

#[path = "../src/namekit/normalize.rs"]
mod normalize_impl;

use canon::namekit::ReasonCode;
use normalize_impl::{
    NormalizationView, normalize_normality, normalize_openrefine_fingerprint, normalize_text,
};
use serde::Deserialize;
use std::{collections::BTreeSet, fs};

const DECISION_DOC: &str = "docs/namekit/unicode-normalization-decision.md";
const FIXTURE: &str = "tests/fixtures/namekit/normalization/unicode_normality.jsonl";

#[derive(Debug, Deserialize)]
struct NormalizationFixture {
    case_id: String,
    fixture_id: String,
    profile: String,
    source: String,
    raw: String,
    normalized: String,
    fingerprint: String,
    lossy: bool,
    reasons: Vec<NormalizationReason>,
    profile_boundary: String,
    ascii_equivalent_raw: String,
    ascii_equivalent_normalized: String,
    ascii_equivalent_reasons: Vec<String>,
    protected_semantics_preserved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct NormalizationReason {
    code: ReasonCode,
    stage: String,
    lossy: bool,
    source: String,
    detail: serde_json::Value,
}

#[test]
fn namekit_normalize_decision_rejects_hidden_locale_and_runtime_dependencies() {
    let doc = fs::read_to_string(DECISION_DOC).expect("unicode normalization decision doc");

    for required in [
        "must not call Python",
        "host ICU",
        "locale-sensitive OS APIs",
        "OpenRefine-style fingerprint",
        "ReasonCode::ALL",
        "tests/fixtures/namekit/normalization/unicode_normality.jsonl",
    ] {
        assert!(
            doc.contains(required),
            "decision doc must mention `{required}`"
        );
    }
}

#[test]
fn namekit_normalize_fixture_locks_unicode_contract_shape() {
    let fixtures = load_fixtures();
    assert_eq!(fixtures.len(), 5);

    let ids = fixtures
        .iter()
        .map(|fixture| fixture.case_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), fixtures.len(), "case ids must be unique");

    for fixture in &fixtures {
        assert!(!fixture.raw.trim().is_empty());
        assert!(!fixture.normalized.trim().is_empty());
        assert!(!fixture.fingerprint.trim().is_empty());
        assert!(!fixture.profile.trim().is_empty());
        assert!(!fixture.profile_boundary.trim().is_empty());
        assert!(
            fixture.lossy,
            "normalization fixture rows record lossy transforms"
        );
        assert!(
            fixture.protected_semantics_preserved,
            "generic Unicode cleanup must not erase profile-protected semantics"
        );
        assert_eq!(fixture.normalized, fixture.ascii_equivalent_normalized);
        assert_ordered_reasons(fixture);
    }
}

#[test]
fn namekit_normalize_impl_matches_fixture_outputs() {
    for fixture in load_fixtures() {
        let normalized = match fixture.source.as_str() {
            "normality" => normalize_normality(&fixture.raw),
            "openrefine_fingerprint" => normalize_openrefine_fingerprint(&fixture.raw),
            other => panic!("unsupported fixture source {other}"),
        };

        assert_eq!(
            normalized.normalized, fixture.normalized,
            "{}",
            fixture.case_id
        );
        assert_eq!(
            normalized.fingerprint, fixture.fingerprint,
            "{}",
            fixture.case_id
        );
        assert_eq!(normalized.lossy, fixture.lossy, "{}", fixture.case_id);
        assert_eq!(
            normalized.reason_codes(),
            fixture
                .reasons
                .iter()
                .map(|reason| reason.code.as_str())
                .collect::<Vec<_>>(),
            "{}",
            fixture.case_id
        );
    }
}

#[test]
fn namekit_normalize_impl_keeps_unknown_non_ascii_visible() {
    let normalized = normalize_text("東京 Bank", NormalizationView::Normality);

    assert_eq!(normalized.normalized, "東京 bank");
    assert!(
        !normalized.reason_codes().contains(&"unicode_folded"),
        "unknown non-ASCII letters are preserved rather than erased"
    );
}

#[test]
fn namekit_normalize_lossless_path_emits_no_loss_reason() {
    let normalized = normalize_normality("sears");

    assert_eq!(normalized.normalized, "sears");
    assert_eq!(normalized.fingerprint, "sears");
    assert!(!normalized.lossy);
    assert_eq!(
        normalized.reason_codes(),
        ["no_loss", "source_parity_reference"]
    );
}

#[test]
fn namekit_normalize_repeated_runs_are_byte_identical() {
    let first = normalize_openrefine_fingerprint("  Sears—Roebuck,  Inc. ");
    let second = normalize_openrefine_fingerprint("  Sears—Roebuck,  Inc. ");

    let first_json = serde_json::to_vec(&first).expect("first normalization serializes");
    let second_json = serde_json::to_vec(&second).expect("second normalization serializes");

    assert_eq!(first, second);
    assert_eq!(first_json, second_json);
}

#[test]
#[allow(non_snake_case)]
fn NK_U004_accented_names_record_unicode_fold_reason() {
    let fixtures = load_fixtures();
    let accented = fixtures
        .iter()
        .filter(|fixture| fixture.fixture_id == "NK_U004")
        .collect::<Vec<_>>();

    assert_eq!(accented.len(), 2);
    assert!(
        accented
            .iter()
            .any(|fixture| fixture.profile == "regab_firm_identity")
    );
    assert!(
        accented
            .iter()
            .any(|fixture| fixture.profile == "cmbs_tenant_label")
    );

    for fixture in accented {
        let codes = reason_codes(fixture);
        assert!(codes.contains(&"unicode_folded"));
        assert!(codes.contains(&"punctuation_removed"));
        assert!(codes.contains(&"whitespace_collapsed"));
        assert!(!fixture.raw.is_ascii());
        assert!(
            !fixture
                .ascii_equivalent_reasons
                .contains(&"unicode_folded".to_string()),
            "ASCII-equivalent path must not invent Unicode folding"
        );
    }
}

#[test]
#[allow(non_snake_case)]
fn NK_U005_variants_share_openrefine_fingerprint_without_suffix_stripping() {
    let fixtures = load_fixtures();
    let variants = fixtures
        .iter()
        .filter(|fixture| fixture.fixture_id == "NK_U005")
        .collect::<Vec<_>>();

    assert_eq!(variants.len(), 3);
    let sears_variants = variants
        .iter()
        .filter(|fixture| fixture.source == "openrefine_fingerprint")
        .collect::<Vec<_>>();
    assert_eq!(sears_variants.len(), 2);

    let fingerprints = sears_variants
        .iter()
        .map(|fixture| fixture.fingerprint.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        fingerprints,
        BTreeSet::from(["inc roebuck sears"]),
        "token order variants must share one OpenRefine-style fingerprint"
    );

    for fixture in sears_variants {
        let codes = reason_codes(fixture);
        assert!(codes.contains(&"tokens_sorted"));
        assert!(
            !codes.contains(&"legal_suffix_stripped"),
            "generic fingerprinting must not do profile suffix policy"
        );
        assert!(fixture.fingerprint.contains("inc"));
    }
}

#[test]
fn normalization_reason_order_matches_reason_code_contract() {
    for fixture in load_fixtures() {
        assert_ordered_reasons(&fixture);
    }
}

fn load_fixtures() -> Vec<NormalizationFixture> {
    let fixture = fs::read_to_string(FIXTURE).expect("unicode normalization fixture");
    fixture
        .lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str::<NormalizationFixture>(line)
                .unwrap_or_else(|error| panic!("fixture line {} parses: {error}", index + 1))
        })
        .collect()
}

fn assert_ordered_reasons(fixture: &NormalizationFixture) {
    assert!(
        fixture
            .reasons
            .windows(2)
            .all(|pair| pair[0].code.order() < pair[1].code.order()),
        "{} reasons must follow ReasonCode::ALL ordering",
        fixture.case_id
    );

    let mut seen = BTreeSet::new();
    for reason in &fixture.reasons {
        assert!(
            seen.insert(reason.code.as_str()),
            "{} has duplicate reason code {}",
            fixture.case_id,
            reason.code.as_str()
        );
        assert!(
            !reason.stage.trim().is_empty(),
            "{} reason {} needs a stage",
            fixture.case_id,
            reason.code.as_str()
        );
        assert!(
            !reason.source.trim().is_empty(),
            "{} reason {} needs a source",
            fixture.case_id,
            reason.code.as_str()
        );
        assert!(
            reason.detail.is_object(),
            "{} reason {} needs object detail",
            fixture.case_id,
            reason.code.as_str()
        );
        assert_eq!(
            reason.lossy,
            reason.code.is_lossy(),
            "{} reason {} lossy flag follows ReasonCode",
            fixture.case_id,
            reason.code.as_str()
        );
    }

    for code in &fixture.ascii_equivalent_reasons {
        ReasonCode::try_from(code.as_str()).unwrap_or_else(|error| {
            panic!(
                "{} ascii equivalent reason `{code}` must be valid: {error}",
                fixture.case_id
            )
        });
    }
    assert!(
        !fixture.ascii_equivalent_raw.trim().is_empty(),
        "{} must name the ASCII-equivalent input",
        fixture.case_id
    );
}

fn reason_codes(fixture: &NormalizationFixture) -> BTreeSet<&'static str> {
    fixture
        .reasons
        .iter()
        .map(|reason| reason.code.as_str())
        .collect()
}
