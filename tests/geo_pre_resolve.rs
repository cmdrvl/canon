#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_NAME_REGION_RESOLUTION_VERSION, CANON_GEO_PRE_RESOLUTION_VERSION,
    CANON_GEO_REGIONAL_INVENTORY_VERSION, CANON_GEO_REGISTRY_PROPOSAL_VERSION,
    GEO_NAME_REGION_ENTITY_REUSE_PROFILE_ID, GEO_PRE_RESOLUTION_CMBS_ADDRESS_RULE_ID, GeoAsOf,
    GeoBoundedGeography, GeoClaimClass, GeoControlEntityLevel, GeoCoveragePredicate,
    GeoEgressClass, GeoEvidenceClass, GeoIdentityParticipation, GeoLicenseClass,
    GeoLocalAcquisitionState, GeoLocalArtifactRef, GeoNameRegionAttributeEvidence,
    GeoNameRegionAttributeField, GeoNameRegionCandidate, GeoNameRegionCellCoverage,
    GeoNameRegionCoverageBasis, GeoNameRegionDescentEdge, GeoNameRegionNameEvidence,
    GeoNameRegionNameOperator, GeoNameRegionRarityPolicy, GeoNameRegionResolutionReason,
    GeoNameRegionResolutionRequest, GeoNameRegionResolutionStatus, GeoNameRegionSourcePin,
    GeoNameRegionSourceRole, GeoNameRegionSubject, GeoNativeEntityScope,
    GeoPreResolutionBuildReceipt, GeoPreResolutionCorpusCapability, GeoPreResolutionCorpusKind,
    GeoPreResolutionErrorCode, GeoPreResolutionProofClass, GeoPreResolutionRequest,
    GeoPreResolutionRunStatus, GeoPreResolutionSourceCorpus, GeoPreResolutionSourceRow,
    GeoRegionalInventory, GeoRegionalSourceInstance, GeoSourceAvailability, GeoSourceRelease,
    GeoTemporalScope, GeoValueOrigin, canonical_name_region_resolution_bytes,
    canonical_pre_resolution_bytes, materialize_name_region_resolution, materialize_pre_resolution,
    pre_resolution_corpus_capability, validate_name_region_resolution_artifact,
    validate_pre_resolution_artifact,
};
use serde_json::Value;

const PRE_RESOLUTION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.pre_resolution.v0.schema.json");
const NAME_REGION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.name_region_resolution.v0.schema.json");

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

#[test]
fn t65_name_region_resolves_unique_mall_to_building_and_parcel_sets() {
    let request = name_region_request(
        GeoControlEntityLevel::Parcel,
        vec![
            mall_candidate(),
            decoy_candidate("poi:overture:decoy-a", "Louisiana Dental Mall"),
        ],
        true,
    );
    let artifact = materialize_name_region_resolution(&request, &name_region_inventory(true))
        .expect("Schedule III-style name-region request materializes");
    validate_name_region_resolution_artifact(&artifact).expect("name-region artifact validates");

    assert_eq!(artifact.version, CANON_GEO_NAME_REGION_RESOLUTION_VERSION);
    assert_eq!(artifact.status, GeoNameRegionResolutionStatus::Resolved);
    assert_eq!(
        artifact.reason,
        GeoNameRegionResolutionReason::UniqueNameRegionMatchDescendedToSupportedGrain
    );
    assert_eq!(artifact.summary.input_rows, 1);
    assert_eq!(artifact.summary.resolved_rows, 1);
    assert_eq!(artifact.summary.region_candidates, 2);
    assert_eq!(artifact.summary.name_matched_candidates, 1);
    assert_eq!(artifact.summary.selected_candidates, 1);
    assert_eq!(artifact.summary.resolved_entity_sets, 2);
    assert!(!artifact.release_claim_allowed);
    assert!(artifact.regional_rarity.discriminating_in_region);

    let sets = artifact
        .selected_sets
        .iter()
        .map(|set| {
            (
                set.entity_level,
                set.entity_ids.clone(),
                set.representation.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        sets,
        vec![
            (
                GeoControlEntityLevel::Parcel,
                vec![
                    "parcel:la:ebrp:001".to_string(),
                    "parcel:la:ebrp:002".to_string(),
                    "parcel:la:ebrp:003".to_string(),
                ],
                "descended_from_named_entity",
            ),
            (
                GeoControlEntityLevel::Building,
                vec![
                    "building:overture:mall-la-a".to_string(),
                    "building:overture:mall-la-b".to_string(),
                ],
                "descended_from_named_entity",
            ),
        ]
    );
    assert!(
        artifact
            .selected_sets
            .iter()
            .all(|set| set.entity_level != GeoControlEntityLevel::Poi),
        "the POI point must not become the resolved answer"
    );

    let first = canonical_name_region_resolution_bytes(&artifact).expect("canonical bytes");
    let mut reordered = request;
    reordered.candidates.reverse();
    reordered.source_pins.reverse();
    reordered.region_cell_coverage.h3_cells.reverse();
    let second_artifact =
        materialize_name_region_resolution(&reordered, &name_region_inventory(true))
            .expect("reordered name-region request materializes");
    let second =
        canonical_name_region_resolution_bytes(&second_artifact).expect("canonical bytes repeat");
    assert_eq!(
        first, second,
        "candidate, source-pin and region-cell ordering must not affect the artifact"
    );
}

#[test]
fn t66_chain_name_in_dense_region_abstains_with_rarity_report() {
    let request = name_region_request(
        GeoControlEntityLevel::Building,
        vec![
            chain_candidate("building:poi:holiday-1", "Holiday Inn Express Downtown"),
            chain_candidate(
                "building:poi:holiday-2",
                "Holiday Inn Express Convention Center",
            ),
            chain_candidate("building:poi:holiday-3", "Holiday Inn Express Airport"),
        ],
        false,
    );
    let artifact = materialize_name_region_resolution(&request, &name_region_inventory(false))
        .expect("chain-name request materializes as an abstention");

    assert_eq!(artifact.status, GeoNameRegionResolutionStatus::Abstained);
    assert_eq!(
        artifact.reason,
        GeoNameRegionResolutionReason::NonDiscriminatingNameInRegion
    );
    assert_eq!(artifact.summary.abstained_rows, 1);
    assert_eq!(artifact.summary.region_candidates, 3);
    assert_eq!(artifact.summary.name_matched_candidates, 3);
    assert_eq!(artifact.summary.resolved_entity_sets, 0);
    assert!(!artifact.regional_rarity.discriminating_in_region);
    assert_eq!(
        artifact.refusals[0].reason,
        GeoNameRegionResolutionReason::NonDiscriminatingNameInRegion
    );
}

#[test]
fn t67_no_parcel_inventory_resolves_building_or_refuses_parcel_without_assumption() {
    let building_request = name_region_request(
        GeoControlEntityLevel::Building,
        vec![building_only_candidate()],
        false,
    );
    let no_parcel_inventory = name_region_inventory(false);
    let building_artifact =
        materialize_name_region_resolution(&building_request, &no_parcel_inventory)
            .expect("building-grain no-parcel request materializes");
    assert_eq!(
        building_artifact.status,
        GeoNameRegionResolutionStatus::Resolved
    );
    assert_eq!(
        building_artifact.selected_sets[0].entity_ids,
        vec!["building:overture:mall-la-a".to_string()]
    );
    assert_eq!(
        building_artifact.selected_sets[0].representation,
        "direct_name_region_candidate"
    );

    let parcel_request = name_region_request(
        GeoControlEntityLevel::Parcel,
        vec![building_only_candidate()],
        false,
    );
    let parcel_artifact = materialize_name_region_resolution(&parcel_request, &no_parcel_inventory)
        .expect("parcel-grain no-parcel request refuses cleanly");
    assert_eq!(
        parcel_artifact.status,
        GeoNameRegionResolutionStatus::Refused
    );
    assert_eq!(
        parcel_artifact.reason,
        GeoNameRegionResolutionReason::ParcelGrainRequestedWithoutParcelInventory
    );
    assert_eq!(parcel_artifact.summary.refused_rows, 1);
    assert!(parcel_artifact.selected_sets.is_empty());
    assert_eq!(
        parcel_artifact.refusals[0].requested_entity_level,
        Some(GeoControlEntityLevel::Parcel)
    );
}

#[test]
fn t68_name_region_refuses_ginnie_as_identifier_join_not_name_resolution() {
    let mut request = name_region_request(
        GeoControlEntityLevel::Building,
        vec![building_only_candidate()],
        false,
    );
    request.source_corpus.corpus_id = "cmdrvl.ginnie.pool".to_string();
    request.source_corpus.corpus_kind = GeoPreResolutionCorpusKind::GinniePoolNoAddress;
    request.source_corpus.native_key_fields = vec!["pool_number".to_string()];

    let artifact = materialize_name_region_resolution(&request, &name_region_inventory(false))
        .expect("Ginnie no-name corpus produces typed refusal");
    assert_eq!(artifact.status, GeoNameRegionResolutionStatus::Refused);
    assert_eq!(
        artifact.reason,
        GeoNameRegionResolutionReason::UnsupportedCorpusKind
    );
    assert!(
        artifact.refusals[0].detail.contains("FHA identifier-join"),
        "Ginnie should be scoped to the identifier path, not forced through name resolution"
    );
}

#[test]
fn t69_name_region_schema_declares_contract_shell_and_no_parcel_refusal() {
    let schema: Value = serde_json::from_str(NAME_REGION_SCHEMA).expect("schema parses");
    assert_eq!(
        schema.get("title").and_then(Value::as_str),
        Some("canon.geo.name_region_resolution.v0")
    );
    assert_eq!(
        schema
            .pointer("/properties/version/const")
            .and_then(Value::as_str),
        Some(CANON_GEO_NAME_REGION_RESOLUTION_VERSION)
    );
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        schema
            .pointer("/properties/release_claim_allowed/const")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert!(
        schema
            .pointer("/$defs/reason/enum")
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|value| {
                value.as_str() == Some("parcel_grain_requested_without_parcel_inventory")
            }))
    );
    assert!(
        schema
            .pointer("/$defs/entity_level/enum")
            .and_then(Value::as_array)
            .is_some_and(
                |values| values.iter().any(|value| value.as_str() == Some("poi"))
                    && values
                        .iter()
                        .any(|value| value.as_str() == Some("building"))
            )
    );

    let artifact = materialize_name_region_resolution(
        &name_region_request(GeoControlEntityLevel::Parcel, vec![mall_candidate()], true),
        &name_region_inventory(true),
    )
    .expect("name-region artifact materializes");
    let encoded = canonical_name_region_resolution_bytes(&artifact).expect("canonical bytes");
    let value: Value = serde_json::from_slice(&encoded).expect("artifact json parses");
    assert_eq!(
        value.get("version").and_then(Value::as_str),
        Some(CANON_GEO_NAME_REGION_RESOLUTION_VERSION)
    );
    assert_eq!(
        value.get("release_claim_allowed").and_then(Value::as_bool),
        Some(false)
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

fn name_region_request(
    requested_entity_level: GeoControlEntityLevel,
    candidates: Vec<GeoNameRegionCandidate>,
    include_parcel_pin: bool,
) -> GeoNameRegionResolutionRequest {
    let mut source_pins = vec![
        source_pin(
            "source.fixture.divisions",
            GeoNameRegionSourceRole::RegionCellCoverage,
        ),
        source_pin(
            "source.fixture.overture_places",
            GeoNameRegionSourceRole::NameBearingEntity,
        ),
        source_pin(
            "source.fixture.overture_places",
            GeoNameRegionSourceRole::AttributeConfirmation,
        ),
        source_pin(
            "source.fixture.containment",
            GeoNameRegionSourceRole::DescentRelation,
        ),
        source_pin(
            "source.fixture.buildings",
            GeoNameRegionSourceRole::NameBearingEntity,
        ),
    ];
    if include_parcel_pin {
        source_pins.push(source_pin(
            "source.fixture.parcels",
            GeoNameRegionSourceRole::DescentRelation,
        ));
    }
    GeoNameRegionResolutionRequest {
        version: CANON_GEO_NAME_REGION_RESOLUTION_VERSION.to_string(),
        source_corpus: GeoPreResolutionSourceCorpus {
            corpus_id: "cmdrvl.reit.schedule_iii".to_string(),
            corpus_kind: GeoPreResolutionCorpusKind::ReitScheduleIiiNameOnly,
            corpus_version: "fixture-2026-09-09".to_string(),
            temporal_scope: "as_of=2026-09".to_string(),
            native_key_fields: vec![
                "city".to_string(),
                "issuer".to_string(),
                "property_name".to_string(),
                "schedule_row_id".to_string(),
                "state".to_string(),
                "zip".to_string(),
            ],
        },
        proof_class: GeoPreResolutionProofClass::Fixture,
        build_receipts: vec![GeoPreResolutionBuildReceipt {
            receipt_id: "receipt.name-region.001".to_string(),
            query_id: "fixture-query:reit-schedule-iii-name-region".to_string(),
            source_artifact_blake3: digest("reit-schedule-iii-source-artifact"),
            row_count: 1,
            run_status: GeoPreResolutionRunStatus::Completed,
        }],
        region: louisiana_region(),
        requested_entity_level,
        requested_claim_classes: vec![GeoClaimClass::CandidateReach],
        source_pins,
        region_cell_coverage: GeoNameRegionCellCoverage {
            coverage_id: "coverage.east-baton-rouge.cells".to_string(),
            source_instance_id: "source.fixture.divisions".to_string(),
            source_record_id: "division_area:us.la.east_baton_rouge".to_string(),
            source_record_blake3: digest("division_area:us.la.east_baton_rouge"),
            basis: GeoNameRegionCoverageBasis::AdministrativeBoundaryCells,
            h3_cells: vec![
                "8844d56a03fffff".to_string(),
                "8844d56a07fffff".to_string(),
                "8844d56a0bfffff".to_string(),
            ],
        },
        subject: GeoNameRegionSubject {
            row_id: "schedule-iii-row-001".to_string(),
            source_record_id: "reit:schedule-iii:mall-of-louisiana".to_string(),
            source_record_blake3: digest("reit:schedule-iii:mall-of-louisiana"),
            asserted_name: "Mall of Louisiana".to_string(),
            asserted_region: "Baton Rouge, LA 70836".to_string(),
            asserted_property_type: Some("regional_mall".to_string()),
            asserted_year_built: Some(1997),
        },
        rarity_policy: GeoNameRegionRarityPolicy {
            policy_id: GEO_NAME_REGION_ENTITY_REUSE_PROFILE_ID.to_string(),
            max_name_matches_for_resolution: 1,
            min_name_score_basis_points: 9000,
            chain_review_min_matches: 2,
        },
        candidates,
    }
}

fn mall_candidate() -> GeoNameRegionCandidate {
    let mut candidate = decoy_candidate("poi:overture:mall-of-louisiana", "Mall of Louisiana");
    candidate.attribute_evidence = vec![
        GeoNameRegionAttributeEvidence {
            field: GeoNameRegionAttributeField::PropertyType,
            source_instance_id: "source.fixture.overture_places".to_string(),
            source_record_id: "place:mall-of-louisiana".to_string(),
            source_record_blake3: digest("place:mall-of-louisiana"),
            asserted_value: "regional_mall".to_string(),
            candidate_value: "regional_mall".to_string(),
            hard_filter_passed: true,
        },
        GeoNameRegionAttributeEvidence {
            field: GeoNameRegionAttributeField::YearBuilt,
            source_instance_id: "source.fixture.overture_places".to_string(),
            source_record_id: "place:mall-of-louisiana".to_string(),
            source_record_blake3: digest("place:mall-of-louisiana"),
            asserted_value: "1997".to_string(),
            candidate_value: "1997".to_string(),
            hard_filter_passed: true,
        },
    ];
    candidate.descent_edges = vec![
        descent(
            candidate.candidate_id.as_str(),
            GeoControlEntityLevel::Building,
            "building:overture:mall-la-a",
        ),
        descent(
            candidate.candidate_id.as_str(),
            GeoControlEntityLevel::Building,
            "building:overture:mall-la-b",
        ),
        descent(
            candidate.candidate_id.as_str(),
            GeoControlEntityLevel::Parcel,
            "parcel:la:ebrp:001",
        ),
        descent(
            candidate.candidate_id.as_str(),
            GeoControlEntityLevel::Parcel,
            "parcel:la:ebrp:002",
        ),
        descent(
            candidate.candidate_id.as_str(),
            GeoControlEntityLevel::Parcel,
            "parcel:la:ebrp:003",
        ),
    ];
    candidate
}

fn decoy_candidate(candidate_id: &str, display_name: &str) -> GeoNameRegionCandidate {
    GeoNameRegionCandidate {
        candidate_id: candidate_id.to_string(),
        entity_level: GeoControlEntityLevel::Poi,
        source_instance_id: "source.fixture.overture_places".to_string(),
        source_record_id: candidate_id.to_string(),
        source_record_blake3: digest(candidate_id),
        display_name: display_name.to_string(),
        region_cell_ids: vec!["8844d56a03fffff".to_string()],
        name_evidence: vec![GeoNameRegionNameEvidence {
            operator: GeoNameRegionNameOperator::Namekit,
            operator_id: "namekit:canonical_surface".to_string(),
            profile_id: GEO_NAME_REGION_ENTITY_REUSE_PROFILE_ID.to_string(),
            profile_hash: Some(digest("geo-name-region-profile")),
            asserted_surface: "mall of louisiana".to_string(),
            candidate_surface: display_name.to_ascii_lowercase(),
            score_basis_points: if display_name == "Mall of Louisiana" {
                10_000
            } else {
                5_000
            },
        }],
        attribute_evidence: Vec::new(),
        descent_edges: Vec::new(),
    }
}

fn chain_candidate(candidate_id: &str, display_name: &str) -> GeoNameRegionCandidate {
    GeoNameRegionCandidate {
        candidate_id: candidate_id.to_string(),
        entity_level: GeoControlEntityLevel::Building,
        source_instance_id: "source.fixture.buildings".to_string(),
        source_record_id: candidate_id.to_string(),
        source_record_blake3: digest(candidate_id),
        display_name: display_name.to_string(),
        region_cell_ids: vec!["8844d56a03fffff".to_string()],
        name_evidence: vec![GeoNameRegionNameEvidence {
            operator: GeoNameRegionNameOperator::TfidfCosine,
            operator_id: "tfidf_cosine:regional_name_topk".to_string(),
            profile_id: GEO_NAME_REGION_ENTITY_REUSE_PROFILE_ID.to_string(),
            profile_hash: Some(digest("geo-name-region-profile")),
            asserted_surface: "holiday inn express".to_string(),
            candidate_surface: "holiday inn express".to_string(),
            score_basis_points: 9_500,
        }],
        attribute_evidence: Vec::new(),
        descent_edges: Vec::new(),
    }
}

fn building_only_candidate() -> GeoNameRegionCandidate {
    GeoNameRegionCandidate {
        candidate_id: "building:overture:mall-la-a".to_string(),
        entity_level: GeoControlEntityLevel::Building,
        source_instance_id: "source.fixture.buildings".to_string(),
        source_record_id: "building:overture:mall-la-a".to_string(),
        source_record_blake3: digest("building:overture:mall-la-a"),
        display_name: "Mall of Louisiana Main Building".to_string(),
        region_cell_ids: vec!["8844d56a03fffff".to_string()],
        name_evidence: vec![GeoNameRegionNameEvidence {
            operator: GeoNameRegionNameOperator::AliasPatchMatch,
            operator_id: "alias_patch_match:regional_place_alias".to_string(),
            profile_id: GEO_NAME_REGION_ENTITY_REUSE_PROFILE_ID.to_string(),
            profile_hash: Some(digest("geo-name-region-profile")),
            asserted_surface: "mall of louisiana".to_string(),
            candidate_surface: "mall of louisiana main building".to_string(),
            score_basis_points: 9_600,
        }],
        attribute_evidence: Vec::new(),
        descent_edges: Vec::new(),
    }
}

fn descent(
    from_id: &str,
    to_level: GeoControlEntityLevel,
    to_id: &str,
) -> GeoNameRegionDescentEdge {
    GeoNameRegionDescentEdge {
        source_instance_id: "source.fixture.containment".to_string(),
        source_record_id: format!("containment:{from_id}:{to_id}"),
        source_record_blake3: digest(format!("containment:{from_id}:{to_id}").as_str()),
        from_level: GeoControlEntityLevel::Poi,
        from_id: from_id.to_string(),
        to_level,
        to_id: to_id.to_string(),
        relation: "contains".to_string(),
    }
}

fn name_region_inventory(include_parcel_source: bool) -> GeoRegionalInventory {
    let mut sources = vec![
        regional_source(
            "source.fixture.divisions",
            GeoControlEntityLevel::Site,
            vec![GeoEvidenceClass::EntityRelation],
        ),
        regional_source(
            "source.fixture.overture_places",
            GeoControlEntityLevel::Poi,
            vec![
                GeoEvidenceClass::AssertedAttribute,
                GeoEvidenceClass::EntityRelation,
            ],
        ),
        regional_source(
            "source.fixture.buildings",
            GeoControlEntityLevel::Building,
            vec![
                GeoEvidenceClass::BuildingFootprint,
                GeoEvidenceClass::AssertedAttribute,
            ],
        ),
        regional_source(
            "source.fixture.containment",
            GeoControlEntityLevel::Building,
            vec![GeoEvidenceClass::EntityRelation],
        ),
    ];
    if include_parcel_source {
        sources.push(regional_source(
            "source.fixture.parcels",
            GeoControlEntityLevel::Parcel,
            vec![GeoEvidenceClass::ParcelGeometry],
        ));
    }
    GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.fixture.name-region".to_string(),
        region: louisiana_region(),
        sources,
        discovery_gaps: Vec::new(),
    }
}

fn regional_source(
    source_instance_id: &str,
    entity_level: GeoControlEntityLevel,
    evidence_classes: Vec<GeoEvidenceClass>,
) -> GeoRegionalSourceInstance {
    GeoRegionalSourceInstance {
        source_instance_id: source_instance_id.to_string(),
        release: GeoSourceRelease {
            release_id: "release.fixture.2026-09-09".to_string(),
            release_digest: digest(format!("{source_instance_id}:release").as_str()),
        },
        temporal_scope: GeoTemporalScope {
            valid_time: None,
            transaction_time: None,
            release_time: Some(GeoAsOf {
                utc_day: "2026-09-09".to_string(),
                semantic_id: "fixture.release_day".to_string(),
                unit: "utc_day".to_string(),
                origin: GeoValueOrigin::SourceRelease,
            }),
        },
        lineage_ids: vec![format!("lineage.{source_instance_id}")],
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level,
            identity_participation: GeoIdentityParticipation::EvidenceOnly,
        },
        evidence_classes,
        coverage: GeoCoveragePredicate {
            coverage_id: format!("coverage.{source_instance_id}"),
            region: louisiana_region(),
            predicate: "fixture bounded region cell coverage".to_string(),
        },
        local_state: GeoLocalAcquisitionState {
            state: GeoSourceAvailability::Available,
            local_ref: Some(GeoLocalArtifactRef {
                artifact_id: format!("local.{source_instance_id}"),
                contract_version: "canon_geo_warehouse_rows.v0".to_string(),
                content_hash: digest(format!("{source_instance_id}:local").as_str()),
                media_type: "application/json".to_string(),
            }),
        },
        geometry: None,
        license_class: GeoLicenseClass::PublicAttributionRequired,
        egress_class: GeoEgressClass::DerivedOnly,
        estimates: Vec::new(),
    }
}

fn source_pin(source_instance_id: &str, role: GeoNameRegionSourceRole) -> GeoNameRegionSourcePin {
    GeoNameRegionSourcePin {
        source_instance_id: source_instance_id.to_string(),
        role,
        release_id: "release.fixture.2026-09-09".to_string(),
        release_digest: digest(format!("{source_instance_id}:release").as_str()),
        local_artifact_id: format!("local.{source_instance_id}"),
        local_content_hash: digest(format!("{source_instance_id}:local").as_str()),
        license_class: GeoLicenseClass::PublicAttributionRequired,
        egress_class: GeoEgressClass::DerivedOnly,
    }
}

fn louisiana_region() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: "us.la.east_baton_rouge.70836".to_string(),
        geography_kind: "zip_plus_city_state".to_string(),
        description: "Baton Rouge, LA 70836".to_string(),
    }
}

fn digest(input: &str) -> String {
    format!("blake3:{}", blake3::hash(input.as_bytes()).to_hex())
}
