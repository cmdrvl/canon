#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_PRE_RESOLUTION_VERSION, CANON_GEO_REGISTRY_PROPOSAL_VERSION,
    GEO_PRE_RESOLUTION_CMBS_ADDRESS_RULE_ID, GeoPreResolutionBuildReceipt,
    GeoPreResolutionCorpusCapability, GeoPreResolutionCorpusKind, GeoPreResolutionErrorCode,
    GeoPreResolutionProofClass, GeoPreResolutionRequest, GeoPreResolutionRunStatus,
    GeoPreResolutionSourceCorpus, GeoPreResolutionSourceRow, canonical_pre_resolution_bytes,
    materialize_pre_resolution, pre_resolution_corpus_capability, validate_pre_resolution_artifact,
};
use serde_json::Value;

const PRE_RESOLUTION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.pre_resolution.v0.schema.json");

#[test]
fn t60_cmbs_annex_a_pre_resolution_wraps_registry_proposal_and_exact_aliases() {
    let request = fixture_request();
    let artifact =
        materialize_pre_resolution(&request).expect("CMBS Annex A pre-resolution materializes");
    validate_pre_resolution_artifact(&artifact).expect("artifact validates");

    assert_eq!(artifact.version, CANON_GEO_PRE_RESOLUTION_VERSION);
    assert!(
        artifact
            .pre_resolution_id
            .starts_with(&format!("{CANON_GEO_PRE_RESOLUTION_VERSION}:"))
    );
    assert_eq!(
        artifact.source_corpus.corpus_kind,
        GeoPreResolutionCorpusKind::CmbsAnnexA
    );
    assert_eq!(
        artifact.registry_proposal.version,
        CANON_GEO_REGISTRY_PROPOSAL_VERSION
    );
    assert_eq!(artifact.denominators.total_source_rows, 3);
    assert_eq!(artifact.denominators.resolved_rows, 2);
    assert_eq!(artifact.denominators.abstained_rows, 0);
    assert_eq!(artifact.denominators.unresolvable_rows, 1);
    assert_eq!(artifact.denominators.stage1_exact_aliases, 2);
    assert_eq!(artifact.denominators.property_assertions, 2);
    assert_eq!(artifact.unresolvable_rows.len(), 1);
    assert_eq!(artifact.unresolvable_rows[0].row_id, "annexa-row-003");

    let stage1_aliases = artifact
        .stage1_exact_aliases
        .iter()
        .map(|alias| {
            (
                alias.alias.as_str(),
                alias.canonical_type.as_str(),
                alias.rule_id.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        stage1_aliases,
        vec![
            (
                "1355 1 AVENUE",
                "property",
                GEO_PRE_RESOLUTION_CMBS_ADDRESS_RULE_ID
            ),
            (
                "305 EAST 72 STREET",
                "property",
                GEO_PRE_RESOLUTION_CMBS_ADDRESS_RULE_ID
            ),
        ]
    );

    for alias in &artifact.stage1_exact_aliases {
        assert!(
            artifact.registry_proposal.entries.iter().any(|entry| {
                entry.alias == alias.alias
                    && entry.canonical_id == alias.canonical_id
                    && entry.canonical_type == alias.canonical_type
                    && entry.rule_id == alias.rule_id
            }),
            "stage-1 alias {alias:?} must be present in the embedded registry proposal"
        );
    }

    let first = canonical_pre_resolution_bytes(&artifact).expect("canonical bytes");
    let second = canonical_pre_resolution_bytes(&artifact).expect("canonical bytes repeat");
    assert_eq!(first, second, "canonical serialization must be byte-stable");

    let mut reordered = request;
    reordered.rows.reverse();
    let reordered_artifact =
        materialize_pre_resolution(&reordered).expect("reordered request materializes");
    let reordered_bytes =
        canonical_pre_resolution_bytes(&reordered_artifact).expect("reordered canonical bytes");
    assert_eq!(
        first, reordered_bytes,
        "input row ordering must not change the pre-resolution artifact"
    );
}

#[test]
fn t61_reach_none_rows_with_identifier_sets_are_refused() {
    let mut request = fixture_request();
    request.rows = vec![GeoPreResolutionSourceRow {
        row_id: "annexa-row-001".to_string(),
        source_record_id: "cmbs-annexa:0000000000-26-000001:loan-a".to_string(),
        accession: "0000000000-26-000001".to_string(),
        deal_id: "fixture-deal-a".to_string(),
        loan_id: "loan-a".to_string(),
        source_record_blake3: digest("annexa-row-001"),
        asserted_address: Some("305 EAST 72 STREET".to_string()),
        reach: Some("none".to_string()),
        reach_none_reason: Some("no_candidate_parcels".to_string()),
        parcel_set: vec!["parcel:nyc:bbl:1004540041".to_string()],
        building_set: Vec::new(),
    }];
    request.build_receipts[0].row_count = 1;

    let error = materialize_pre_resolution(&request)
        .expect_err("reach=none rows must not carry fabricated member sets");
    assert_eq!(error.code, GeoPreResolutionErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("rows[].parcel_set_or_building_set")
    );
}

#[test]
fn t62_non_address_corpora_cannot_be_promoted_as_address_pre_resolution() {
    assert_eq!(
        pre_resolution_corpus_capability(GeoPreResolutionCorpusKind::GinniePoolNoAddress),
        GeoPreResolutionCorpusCapability::NoAddressField
    );
    assert_eq!(
        pre_resolution_corpus_capability(GeoPreResolutionCorpusKind::ReitScheduleIiiNameOnly),
        GeoPreResolutionCorpusCapability::NameOnly
    );

    let mut request = fixture_request();
    request.source_corpus.corpus_id = "cmdrvl.ginnie.pool".to_string();
    request.source_corpus.corpus_kind = GeoPreResolutionCorpusKind::GinniePoolNoAddress;
    request.source_corpus.native_key_fields = vec!["pool_number".to_string()];

    let error = materialize_pre_resolution(&request)
        .expect_err("Ginnie pool records carry no address field at native grain");
    assert_eq!(error.code, GeoPreResolutionErrorCode::UnsupportedCorpusKind);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("source_corpus.corpus_kind")
    );
    assert_eq!(
        error.detail.get("capability").map(String::as_str),
        Some("NoAddressField")
    );
}

#[test]
fn t63_cancelled_runs_and_ambiguous_exact_addresses_do_not_become_evidence() {
    let mut cancelled = fixture_request();
    cancelled.build_receipts[0].run_status = GeoPreResolutionRunStatus::Cancelled;
    let error = materialize_pre_resolution(&cancelled)
        .expect_err("cancelled pre-resolution runs are not evidence");
    assert_eq!(error.code, GeoPreResolutionErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("build_receipts[].run_status")
    );

    let mut ambiguous = fixture_request();
    ambiguous.rows = vec![
        source_row(
            "annexa-row-001",
            "loan-a",
            "305 EAST 72 STREET",
            &["parcel:nyc:bbl:1004540041"],
            &["building:nyc:bin:1006494"],
        ),
        source_row(
            "annexa-row-002",
            "loan-b",
            "305 EAST 72 STREET",
            &["parcel:nyc:bbl:1004540042"],
            &["building:nyc:bin:1006495"],
        ),
    ];
    ambiguous.build_receipts[0].row_count = 2;
    let artifact =
        materialize_pre_resolution(&ambiguous).expect("ambiguous address rows are abstentions");
    assert_eq!(artifact.denominators.total_source_rows, 2);
    assert_eq!(artifact.denominators.resolved_rows, 0);
    assert_eq!(artifact.denominators.abstained_rows, 2);
    assert!(artifact.stage1_exact_aliases.is_empty());
    assert!(artifact.registry_proposal.entries.is_empty());
}

#[test]
fn t64_pre_resolution_schema_declares_contract_shell_and_corpus_limits() {
    let schema: Value =
        serde_json::from_str(PRE_RESOLUTION_SCHEMA).expect("pre-resolution schema parses");
    assert_eq!(
        schema.get("title").and_then(Value::as_str),
        Some("canon.geo.pre_resolution.v0")
    );
    assert_eq!(
        schema
            .pointer("/properties/version/const")
            .and_then(Value::as_str),
        Some(CANON_GEO_PRE_RESOLUTION_VERSION)
    );
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false)
    );
    assert!(
        schema
            .pointer("/required")
            .and_then(Value::as_array)
            .is_some_and(|required| required
                .iter()
                .any(|value| value.as_str() == Some("registry_proposal")))
    );
    assert!(
        schema
            .pointer("/$defs/source_corpus/properties/corpus_kind/enum")
            .and_then(Value::as_array)
            .is_some_and(|values| values
                .iter()
                .any(|value| value.as_str() == Some("ginnie_pool_no_address"))
                && values
                    .iter()
                    .any(|value| value.as_str() == Some("reit_schedule_iii_name_only")))
    );
}

fn fixture_request() -> GeoPreResolutionRequest {
    GeoPreResolutionRequest {
        version: CANON_GEO_PRE_RESOLUTION_VERSION.to_string(),
        source_corpus: GeoPreResolutionSourceCorpus {
            corpus_id: "cmdrvl.cmbs.annex_a".to_string(),
            corpus_kind: GeoPreResolutionCorpusKind::CmbsAnnexA,
            corpus_version: "fixture-2026-09-02".to_string(),
            temporal_scope: "as_of=2026-08".to_string(),
            native_key_fields: vec![
                "accession".to_string(),
                "loan_id".to_string(),
                "property_address".to_string(),
            ],
        },
        proof_class: GeoPreResolutionProofClass::Fixture,
        build_receipts: vec![GeoPreResolutionBuildReceipt {
            receipt_id: "receipt-001".to_string(),
            query_id: "fixture-query:cmbs-annex-a-pre-resolution:2026-09-02".to_string(),
            source_artifact_blake3: digest("cmbs-annex-a-source-artifact"),
            row_count: 3,
            run_status: GeoPreResolutionRunStatus::Completed,
        }],
        rows: vec![
            source_row(
                "annexa-row-002",
                "loan-b",
                "1355 1 AVENUE",
                &["parcel:nyc:bbl:1014560025"],
                &[],
            ),
            GeoPreResolutionSourceRow {
                row_id: "annexa-row-003".to_string(),
                source_record_id: "cmbs-annexa:0000000000-26-000001:loan-c".to_string(),
                accession: "0000000000-26-000001".to_string(),
                deal_id: "fixture-deal-a".to_string(),
                loan_id: "loan-c".to_string(),
                source_record_blake3: digest("annexa-row-003"),
                asserted_address: None,
                reach: Some("none".to_string()),
                reach_none_reason: Some("no_candidate_parcels".to_string()),
                parcel_set: Vec::new(),
                building_set: Vec::new(),
            },
            source_row(
                "annexa-row-001",
                "loan-a",
                "305 EAST 72 STREET",
                &["parcel:nyc:bbl:1004540041"],
                &["building:nyc:bin:1006494"],
            ),
        ],
    }
}

fn source_row(
    row_id: &str,
    loan_id: &str,
    asserted_address: &str,
    parcel_set: &[&str],
    building_set: &[&str],
) -> GeoPreResolutionSourceRow {
    GeoPreResolutionSourceRow {
        row_id: row_id.to_string(),
        source_record_id: format!("cmbs-annexa:0000000000-26-000001:{loan_id}"),
        accession: "0000000000-26-000001".to_string(),
        deal_id: "fixture-deal-a".to_string(),
        loan_id: loan_id.to_string(),
        source_record_blake3: digest(row_id),
        asserted_address: Some(asserted_address.to_string()),
        reach: Some("full".to_string()),
        reach_none_reason: None,
        parcel_set: parcel_set
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        building_set: building_set
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    }
}

fn digest(input: &str) -> String {
    format!("blake3:{}", blake3::hash(input.as_bytes()).to_hex())
}
