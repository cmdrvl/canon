#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_ACQUISITION_RECEIPT_VERSION, CANON_GEO_ACQUISITION_REQUEST_VERSION,
    CANON_GEO_DISCOVERY_REQUEST_VERSION, GeoAcquisitionCounts, GeoAcquisitionDenominator,
    GeoAcquisitionProofClass, GeoAcquisitionReceipt, GeoAcquisitionRequest,
    GeoAcquisitionResumability, GeoAcquisitionTerminalState, GeoBoundedGeography, GeoBoundedSubset,
    GeoColumnReadabilityProbe, GeoControlEntityLevel, GeoDenominatorSource, GeoDigest,
    GeoDigestAlgorithm, GeoDiscoveryErrorCode, GeoDiscoveryReleaseSelectionPolicy,
    GeoDiscoveryRequest, GeoDiscoveryStep, GeoEvidenceClass, GeoExecutorKind, GeoExecutorTrace,
    GeoFieldRole, GeoLocalArtifactDigest, GeoNullOrdering, GeoOrderDirection, GeoOrderingTerm,
    GeoPaginationReceipt, GeoPaginationRequest, GeoProjectionOperation, GeoReleasePin,
    GeoReleaseSelectionMode, GeoRequestedField, GeoRowByteCeilings, GeoSubsetPredicate,
    GeoSubsetPredicateKind, canonical_geo_acquisition_request_bytes,
    geo_acquisition_receipt_satisfies_positive_gate, geo_acquisition_request_id,
    geo_acquisition_request_semantic_hash, geo_discovery_request_id,
    validate_geo_acquisition_receipt, validate_geo_acquisition_request,
    validate_geo_discovery_request,
};
use sha2::{Digest as _, Sha256};

fn blake3_digest(digest_id: &str, bytes: &[u8]) -> GeoDigest {
    GeoDigest {
        digest_id: digest_id.to_string(),
        algorithm: GeoDigestAlgorithm::Blake3,
        hex_digest: blake3::hash(bytes).to_hex().to_string(),
    }
}

fn sha256_digest(digest_id: &str, bytes: &[u8]) -> GeoDigest {
    GeoDigest {
        digest_id: digest_id.to_string(),
        algorithm: GeoDigestAlgorithm::Sha256,
        hex_digest: Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

fn region() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: "region.fixture.unknown-source-franklin-oh".to_string(),
        geography_kind: "county_fips".to_string(),
        description: "Operator-declared Franklin County, Ohio discovery scope".to_string(),
    }
}

fn other_region() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: "region.fixture.other".to_string(),
        geography_kind: "county_fips".to_string(),
        description: "Different region".to_string(),
    }
}

fn subset_for(geography: GeoBoundedGeography) -> GeoBoundedSubset {
    GeoBoundedSubset {
        subset_id: "subset.fixture.franklin-r8-k1".to_string(),
        geography,
        h3_cells: vec![
            "882a107707fffff".to_string(),
            "882a10770fffffff".to_string(),
        ],
        predicates: vec![GeoSubsetPredicate {
            predicate_id: "predicate.h3.r8.k1".to_string(),
            kind: GeoSubsetPredicateKind::H3Cells,
            expression: "declared h3 r8 center plus controlled halo k=1".to_string(),
        }],
    }
}

fn fields() -> Vec<GeoRequestedField> {
    vec![
        GeoRequestedField {
            field_id: "source_record_id".to_string(),
            role: GeoFieldRole::Identifier,
            required: true,
        },
        GeoRequestedField {
            field_id: "geometry_wkb_sha256".to_string(),
            role: GeoFieldRole::Digest,
            required: true,
        },
        GeoRequestedField {
            field_id: "h3_cell".to_string(),
            role: GeoFieldRole::Ordering,
            required: true,
        },
    ]
}

fn fields_with_geometry() -> Vec<GeoRequestedField> {
    let mut requested_fields = fields();
    requested_fields.push(GeoRequestedField {
        field_id: "footprint_wkb".to_string(),
        role: GeoFieldRole::Geometry,
        required: true,
    });
    requested_fields
}

fn release_pin(source_instance_id: &str, release_id: &str, digest: GeoDigest) -> GeoReleasePin {
    GeoReleasePin {
        source_instance_id: source_instance_id.to_string(),
        release_id: release_id.to_string(),
        release_digest: digest,
    }
}

fn release_pins() -> Vec<GeoReleasePin> {
    vec![
        release_pin(
            "source.fixture.building-footprints",
            "release.fixture.2026-08-31",
            sha256_digest("release.fixture.building-footprints", b"release-a"),
        ),
        release_pin(
            "source.fixture.address-points",
            "release.fixture.addresses.2026-08-31",
            blake3_digest("release.fixture.address-points", b"release-b"),
        ),
    ]
}

fn ceilings(max_rows: u64, max_bytes: u64) -> GeoRowByteCeilings {
    GeoRowByteCeilings {
        max_rows,
        max_bytes,
    }
}

fn projection() -> GeoProjectionOperation {
    GeoProjectionOperation {
        coordinate_reference_system: "EPSG:4326".to_string(),
        operation_id: "identity-wgs84".to_string(),
        operation_version: "v1".to_string(),
        operation_digest: sha256_digest("projection.identity-wgs84", b"identity-wgs84:v1"),
    }
}

fn with_discovery_id(mut request: GeoDiscoveryRequest) -> GeoDiscoveryRequest {
    request.request_id = geo_discovery_request_id(&request).expect("discovery id computes");
    request
}

fn with_acquisition_id(mut request: GeoAcquisitionRequest) -> GeoAcquisitionRequest {
    request.request_id = geo_acquisition_request_id(&request).expect("acquisition id computes");
    request
}

fn discovery_request() -> GeoDiscoveryRequest {
    let geography = region();
    let subset = subset_for(geography.clone());
    let requested_fields = fields();
    with_discovery_id(GeoDiscoveryRequest {
        version: CANON_GEO_DISCOVERY_REQUEST_VERSION.to_string(),
        request_id: String::new(),
        bounded_geography: geography,
        subset: subset.clone(),
        requested_entity_levels: vec![GeoControlEntityLevel::Building],
        requested_evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
        release_selection: GeoDiscoveryReleaseSelectionPolicy {
            as_of_utc_day: "2026-08-31".to_string(),
            mode: GeoReleaseSelectionMode::LatestNotAfterAsOf,
            candidate_release_ids: Vec::new(),
        },
        releases: Vec::new(),
        fields: requested_fields.clone(),
        required_steps: vec![
            GeoDiscoveryStep::CatalogSearch,
            GeoDiscoveryStep::ListReleases,
            GeoDiscoveryStep::DescribeSchema,
            GeoDiscoveryStep::ColumnReadabilityProbe,
        ],
        column_readability_probe: GeoColumnReadabilityProbe {
            probe_id: "probe.fixture.real-column-read".to_string(),
            fields: requested_fields
                .iter()
                .map(|field| field.field_id.clone())
                .collect(),
            subset,
            ceilings: ceilings(5, 8192),
        },
        ceilings: ceilings(5, 8192),
    })
}

fn acquisition_request() -> GeoAcquisitionRequest {
    let geography = region();
    with_acquisition_id(GeoAcquisitionRequest {
        version: CANON_GEO_ACQUISITION_REQUEST_VERSION.to_string(),
        request_id: String::new(),
        discovery_request_id: Some(discovery_request().request_id),
        bounded_geography: geography.clone(),
        subset: subset_for(geography),
        releases: release_pins(),
        fields: fields(),
        projection: None,
        ordering: vec![
            GeoOrderingTerm {
                position: 1,
                field_id: "source_record_id".to_string(),
                direction: GeoOrderDirection::Asc,
                nulls: GeoNullOrdering::Last,
            },
            GeoOrderingTerm {
                position: 0,
                field_id: "h3_cell".to_string(),
                direction: GeoOrderDirection::Asc,
                nulls: GeoNullOrdering::Last,
            },
        ],
        pagination: GeoPaginationRequest {
            page_size_rows: 10,
            page_token: None,
        },
        ceilings: ceilings(10, 1_048_576),
        positive_path_min_rows: 1,
    })
}

fn geometric_acquisition_request() -> GeoAcquisitionRequest {
    let mut request = acquisition_request();
    request.fields = fields_with_geometry();
    request.projection = Some(projection());
    with_acquisition_id(request)
}

fn executor_trace(kind: GeoExecutorKind, executor_id: &str) -> GeoExecutorTrace {
    GeoExecutorTrace {
        executor_kind: kind,
        executor_id: executor_id.to_string(),
        executor_version: "v1".to_string(),
        tool_id: format!("{executor_id}.bounded-export"),
        tool_version: "v1".to_string(),
        executor_request_id: format!("{executor_id}-request-001"),
        executor_query_id: format!("{executor_id}-query-001"),
        executor_attempt_id: Some(format!("{executor_id}-attempt-001")),
    }
}

fn live_receipt(
    request: &GeoAcquisitionRequest,
    terminal_state: GeoAcquisitionTerminalState,
    rows: u64,
) -> GeoAcquisitionReceipt {
    GeoAcquisitionReceipt {
        version: CANON_GEO_ACQUISITION_RECEIPT_VERSION.to_string(),
        request_id: request.request_id.clone(),
        request_semantic_hash: geo_acquisition_request_semantic_hash(request)
            .expect("request hash computes"),
        terminal_state,
        proof_class: GeoAcquisitionProofClass::Live,
        executor: Some(executor_trace(
            GeoExecutorKind::QueryEngine,
            "fixture-query-engine",
        )),
        fixture_id: None,
        retained_receipt_id: None,
        bounded_geography: request.bounded_geography.clone(),
        subset: request.subset.clone(),
        releases: request.releases.clone(),
        fields: request.fields.clone(),
        projection: request.projection.clone(),
        normalized_executed_request_digest: sha256_digest(
            "executor.normalized_request",
            b"select source_record_id, geometry_wkb_sha256 from bounded_fixture order by h3_cell, source_record_id",
        ),
        pagination: GeoPaginationReceipt {
            requested_page: request.pagination.clone(),
            next_page_token: None,
            rows_truncated: false,
            bytes_truncated: false,
        },
        counts: GeoAcquisitionCounts {
            rows,
            bytes: rows * 256,
        },
        denominators: vec![GeoAcquisitionDenominator {
            denominator_id: "denominator.result.rows".to_string(),
            source: GeoDenominatorSource::ResultArtifact,
            count: rows,
            unit: "row".to_string(),
            description: "Rows in the bounded subset result artifact".to_string(),
        }],
        source_digests: vec![request.releases[0].release_digest.clone()],
        result_digests: vec![blake3_digest("result.rows", format!("rows:{rows}").as_bytes())],
        local_artifacts: vec![GeoLocalArtifactDigest {
            artifact_id: "artifact.fixture.rows".to_string(),
            media_type: "application/jsonl".to_string(),
            byte_count: rows * 256,
            digest: blake3_digest("artifact.rows", format!("artifact:{rows}").as_bytes()),
        }],
        unreadable_columns: Vec::new(),
        resumability: GeoAcquisitionResumability {
            resumable: false,
            resume_token: None,
            resume_request_id: None,
            retry_guidance: "terminal receipt requires no resume action".to_string(),
        },
        terminal_detail: None,
    }
}

#[test]
fn unknown_region_discovery_allows_zero_release_pins_with_release_selection_policy() {
    let request = discovery_request();
    assert!(request.releases.is_empty());
    validate_geo_discovery_request(&request)
        .expect("unknown-region discovery is release-policy driven");
}

#[test]
fn unknown_region_discovery_requires_release_selection_policy() {
    let mut wire = serde_json::to_value(discovery_request()).expect("request serializes");
    wire.as_object_mut()
        .expect("object")
        .remove("release_selection")
        .expect("release_selection present");
    assert!(
        serde_json::from_value::<GeoDiscoveryRequest>(wire).is_err(),
        "unknown-region discovery still requires an as-of/release-selection policy"
    );

    let mut exact_without_candidates = discovery_request();
    exact_without_candidates.release_selection.mode = GeoReleaseSelectionMode::ExactReleaseIds;
    exact_without_candidates = with_discovery_id(exact_without_candidates);
    let error = validate_geo_discovery_request(&exact_without_candidates)
        .expect_err("exact release selection cannot be empty");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
}

#[test]
fn discovery_rejects_metadata_only_steps_as_column_readability() {
    let mut request = discovery_request();
    request.required_steps = vec![
        GeoDiscoveryStep::CatalogSearch,
        GeoDiscoveryStep::ListReleases,
        GeoDiscoveryStep::DescribeSchema,
    ];
    request = with_discovery_id(request);
    let error = validate_geo_discovery_request(&request).expect_err("probe step is required");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
    assert!(
        error
            .message
            .contains("metadata/list/describe discovery is not column readability")
    );
}

#[test]
fn discovery_column_readability_probe_must_cover_all_requested_fields() {
    let mut request = discovery_request();
    request.column_readability_probe.fields =
        vec!["source_record_id".to_string(), "h3_cell".to_string()];
    request = with_discovery_id(request);

    let error = validate_geo_discovery_request(&request)
        .expect_err("probe must cover every requested field");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
    assert!(
        error
            .message
            .contains("probe must cover every requested field")
    );
}

#[test]
fn discovery_rejects_mismatched_top_level_and_subset_geography() {
    let mut request = discovery_request();
    request.subset.geography = other_region();
    request.column_readability_probe.subset = request.subset.clone();
    request = with_discovery_id(request);
    let error = validate_geo_discovery_request(&request).expect_err("geography drift rejects");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
    assert!(error.message.contains("bounded_geography must equal"));
}

#[test]
fn sha256_release_pins_are_valid_for_acquisition() {
    let request = acquisition_request();
    assert_eq!(
        request.releases[0].release_digest.algorithm,
        GeoDigestAlgorithm::Sha256
    );
    validate_geo_acquisition_request(&request).expect("SHA-256 release pins are valid");

    let mut sha512 = request.clone();
    sha512.releases[0].release_digest = GeoDigest {
        digest_id: "release.fixture.building-footprints.sha512".to_string(),
        algorithm: GeoDigestAlgorithm::Sha512,
        hex_digest: "a".repeat(128),
    };
    sha512 = with_acquisition_id(sha512);
    validate_geo_acquisition_request(&sha512).expect("SHA-512 release pins are valid");

    let mut malformed = request.clone();
    malformed.releases[0].release_digest.hex_digest = "a".repeat(63);
    malformed = with_acquisition_id(malformed);
    let error =
        validate_geo_acquisition_request(&malformed).expect_err("malformed SHA-256 rejects");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);

    let weak = serde_json::json!({
        "digest_id": "source.etag",
        "algorithm": "sha1",
        "hex_digest": "a".repeat(40)
    });
    assert!(
        serde_json::from_value::<GeoDigest>(weak).is_err(),
        "weak source metadata checksums are not v0 verification digests"
    );
}

#[test]
fn digest_ids_must_be_present_where_digest_contracts_are_used() {
    let mut request = acquisition_request();
    request.releases[0].release_digest.digest_id.clear();
    request = with_acquisition_id(request);
    let error =
        validate_geo_acquisition_request(&request).expect_err("empty release digest ids reject");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);

    let request = acquisition_request();
    let mut receipt = live_receipt(&request, GeoAcquisitionTerminalState::Complete, 2);
    receipt.normalized_executed_request_digest.digest_id = " ".to_string();
    let error = validate_geo_acquisition_receipt(&request, &receipt)
        .expect_err("empty receipt digest ids reject");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
}

#[test]
fn acquisition_requires_release_pins_and_bounded_subset() {
    let mut no_release = acquisition_request();
    no_release.releases.clear();
    no_release = with_acquisition_id(no_release);
    assert_eq!(
        validate_geo_acquisition_request(&no_release)
            .expect_err("acquisition still requires release pins")
            .code,
        GeoDiscoveryErrorCode::InvalidInput
    );

    let mut unbounded = acquisition_request();
    unbounded.subset.h3_cells.clear();
    unbounded.subset.predicates.clear();
    unbounded = with_acquisition_id(unbounded);
    assert_eq!(
        validate_geo_acquisition_request(&unbounded)
            .expect_err("unbounded acquisition rejects")
            .code,
        GeoDiscoveryErrorCode::InvalidInput
    );

    let mut malformed_discovery_id = acquisition_request();
    malformed_discovery_id.discovery_request_id = Some("not-a-discovery-request-id".to_string());
    malformed_discovery_id = with_acquisition_id(malformed_discovery_id);
    let error = validate_geo_acquisition_request(&malformed_discovery_id)
        .expect_err("discovery request id shape rejects");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);

    let mut impossible_positive_gate = acquisition_request();
    impossible_positive_gate.positive_path_min_rows =
        impossible_positive_gate.ceilings.max_rows + 1;
    impossible_positive_gate = with_acquisition_id(impossible_positive_gate);
    let error = validate_geo_acquisition_request(&impossible_positive_gate)
        .expect_err("positive gate cannot exceed request row ceiling");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
}

#[test]
fn release_pins_are_unique_by_source_instance_and_release_id() {
    let mut request = acquisition_request();
    request.releases.push(release_pin(
        "source.fixture.building-footprints",
        "release.fixture.2026-08-31",
        blake3_digest("release.fixture.same-key-different-digest", b"different"),
    ));
    request = with_acquisition_id(request);
    let error = validate_geo_acquisition_request(&request)
        .expect_err("duplicate release key rejects even with a different digest");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
    assert!(
        error
            .message
            .contains("unique source_instance_id/release_id")
    );

    let mut discovery = discovery_request();
    discovery.releases = vec![
        release_pin(
            "source.fixture.building-footprints",
            "release.fixture.2026-08-31",
            sha256_digest("release.fixture.building-footprints", b"release-a"),
        ),
        release_pin(
            "source.fixture.building-footprints",
            "release.fixture.2026-08-31",
            blake3_digest("release.fixture.same-key-different-digest", b"different"),
        ),
    ];
    discovery = with_discovery_id(discovery);
    let error = validate_geo_discovery_request(&discovery)
        .expect_err("discovery release pins use the same key uniqueness rule");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
}

#[test]
fn projection_is_required_only_for_geometry_fields_and_receipts_match_request() {
    let generic = acquisition_request();
    validate_geo_acquisition_request(&generic)
        .expect("generic non-geometry acquisition omits projection");
    let generic_wire = serde_json::to_value(&generic).expect("generic request serializes");
    assert!(
        generic_wire.get("projection").is_none(),
        "serde omits projection when no geometry field is requested"
    );

    let mut unexpected_projection = generic.clone();
    unexpected_projection.projection = Some(projection());
    unexpected_projection = with_acquisition_id(unexpected_projection);
    let error = validate_geo_acquisition_request(&unexpected_projection)
        .expect_err("non-geometry acquisition must not carry projection");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);

    let geometry = geometric_acquisition_request();
    validate_geo_acquisition_request(&geometry)
        .expect("geometry acquisition requires and accepts projection");
    let geometry_wire = serde_json::to_value(&geometry).expect("geometry request serializes");
    assert!(
        geometry_wire.get("projection").is_some(),
        "serde includes projection when geometry is requested"
    );

    let mut missing_projection = geometry.clone();
    missing_projection.projection = None;
    missing_projection = with_acquisition_id(missing_projection);
    let error = validate_geo_acquisition_request(&missing_projection)
        .expect_err("geometry fields require projection");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);

    let mut receipt_drift = live_receipt(&generic, GeoAcquisitionTerminalState::Complete, 2);
    receipt_drift.projection = Some(projection());
    let error = validate_geo_acquisition_receipt(&generic, &receipt_drift)
        .expect_err("receipt projection must equal request projection");
    assert_eq!(error.code, GeoDiscoveryErrorCode::ReceiptMismatch);
}

#[test]
fn positive_path_gate_allows_zero() {
    let mut request = acquisition_request();
    request.positive_path_min_rows = 0;
    request = with_acquisition_id(request);
    validate_geo_acquisition_request(&request)
        .expect("positive_path_min_rows=0 disables the positive-row gate");

    let receipt = live_receipt(&request, GeoAcquisitionTerminalState::Complete, 2);
    validate_geo_acquisition_receipt(&request, &receipt)
        .expect("receipt validity is independent from a disabled positive gate");
    assert!(geo_acquisition_receipt_satisfies_positive_gate(
        &request, &receipt
    ));
}

#[test]
fn acquisition_rejects_mismatched_top_level_and_subset_geography() {
    let mut request = acquisition_request();
    request.subset.geography = other_region();
    request = with_acquisition_id(request);
    let error = validate_geo_acquisition_request(&request).expect_err("geography drift rejects");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
    assert!(error.message.contains("bounded_geography must equal"));
}

#[test]
fn deterministic_request_ids_ignore_permutation_not_protocol_telemetry() {
    let mut left = acquisition_request();
    let mut right = acquisition_request();
    right.releases.reverse();
    right.fields.reverse();
    right.subset.h3_cells.reverse();
    right.ordering.reverse();
    right = with_acquisition_id(right);
    left = with_acquisition_id(left);

    assert_eq!(left.request_id, right.request_id);
    assert_eq!(
        canonical_geo_acquisition_request_bytes(&left).expect("left canonicalizes"),
        canonical_geo_acquisition_request_bytes(&right).expect("right canonicalizes")
    );

    let warehouse = live_receipt(&left, GeoAcquisitionTerminalState::Complete, 2);
    let mut local_file = warehouse.clone();
    local_file.executor = Some(executor_trace(
        GeoExecutorKind::LocalFile,
        "fixture-local-file",
    ));
    local_file.normalized_executed_request_digest = sha256_digest(
        "executor.normalized_request",
        b"local file manifest equivalent rows",
    );

    validate_geo_acquisition_receipt(&left, &warehouse).expect("warehouse receipt validates");
    validate_geo_acquisition_receipt(&left, &local_file).expect("local receipt validates");
    assert_eq!(warehouse.request_id, local_file.request_id);
    assert_eq!(
        warehouse.request_semantic_hash,
        local_file.request_semantic_hash
    );
}

#[test]
fn complete_receipt_cannot_be_paginated_but_can_be_below_positive_gate() {
    let request = acquisition_request();
    let mut truncated = live_receipt(&request, GeoAcquisitionTerminalState::Complete, 2);
    truncated.pagination.next_page_token = Some("page-2".to_string());
    assert_eq!(
        validate_geo_acquisition_receipt(&request, &truncated)
            .expect_err("pagination truncation cannot be complete")
            .code,
        GeoDiscoveryErrorCode::InvalidInput
    );

    let mut min_rows = acquisition_request();
    min_rows.positive_path_min_rows = 3;
    min_rows = with_acquisition_id(min_rows);
    let below_gate = live_receipt(&min_rows, GeoAcquisitionTerminalState::Complete, 2);
    validate_geo_acquisition_receipt(&min_rows, &below_gate)
        .expect("below-gate COMPLETE remains an honest valid receipt");
    assert!(!geo_acquisition_receipt_satisfies_positive_gate(
        &min_rows,
        &below_gate
    ));
}

#[test]
fn complete_and_zero_row_receipts_cannot_claim_resumability() {
    let request = acquisition_request();
    let mut complete = live_receipt(&request, GeoAcquisitionTerminalState::Complete, 2);
    complete.resumability.resumable = true;
    complete.resumability.resume_token = Some("executor-resume-token".to_string());
    let error = validate_geo_acquisition_receipt(&request, &complete)
        .expect_err("complete receipts are not resumable");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);

    let mut zero_rows = live_receipt(&request, GeoAcquisitionTerminalState::ZeroRows, 0);
    zero_rows.resumability.resumable = true;
    zero_rows.resumability.resume_request_id = Some(request.request_id.clone());
    let error = validate_geo_acquisition_receipt(&request, &zero_rows)
        .expect_err("zero-row findings are terminal, not resumable");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
}

#[test]
fn resume_request_id_must_be_a_canonical_acquisition_request_id() {
    let request = acquisition_request();
    let mut receipt = live_receipt(&request, GeoAcquisitionTerminalState::Partial, 1);
    receipt.terminal_detail = Some("executor paused after writing a partial page".to_string());
    receipt.resumability.resumable = true;
    receipt.resumability.resume_request_id = Some("not-an-acquisition-request-id".to_string());
    receipt.resumability.retry_guidance = "resume with the retained request id".to_string();

    let error = validate_geo_acquisition_receipt(&request, &receipt)
        .expect_err("malformed resume request ids reject");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
}

#[test]
fn zero_rows_is_valid_finding_but_not_positive_path_success() {
    let request = acquisition_request();
    let receipt = live_receipt(&request, GeoAcquisitionTerminalState::ZeroRows, 0);
    validate_geo_acquisition_receipt(&request, &receipt).expect("zero rows is a valid receipt");
    assert!(!geo_acquisition_receipt_satisfies_positive_gate(
        &request, &receipt
    ));
}

#[test]
fn timeout_cancel_and_partial_retain_executor_ids_and_resumability() {
    for (state, rows) in [
        (GeoAcquisitionTerminalState::Timeout, 0),
        (GeoAcquisitionTerminalState::Canceled, 0),
        (GeoAcquisitionTerminalState::Partial, 1),
    ] {
        let request = acquisition_request();
        let mut receipt = live_receipt(&request, state, rows);
        receipt.terminal_detail = Some(format!("{state:?} retained by executor"));
        receipt.resumability.resumable = false;
        receipt.resumability.retry_guidance =
            format!("{state:?} is not resumable; retry by reissuing the acquisition request");
        validate_geo_acquisition_receipt(&request, &receipt).unwrap_or_else(|error| {
            panic!("{state:?} should validate without resume token: {error}")
        });

        receipt.resumability.retry_guidance.clear();
        let error = validate_geo_acquisition_receipt(&request, &receipt)
            .expect_err("incomplete terminal states need retry guidance");
        assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);

        let mut fabricated_resume = live_receipt(&request, state, rows);
        fabricated_resume.terminal_detail = Some(format!("{state:?} retained by executor"));
        fabricated_resume.resumability.resumable = true;
        fabricated_resume.resumability.retry_guidance =
            "resume the retained executor request".to_string();
        let error = validate_geo_acquisition_receipt(&request, &fabricated_resume)
            .expect_err("resumable states need an actual resume handle");
        assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);

        fabricated_resume.resumability.resume_request_id = Some(request.request_id.clone());
        validate_geo_acquisition_receipt(&request, &fabricated_resume)
            .unwrap_or_else(|error| panic!("{state:?} should validate with resume id: {error}"));
    }
}

#[test]
fn unreadable_columns_is_distinct_from_zero_rows_and_execution_failure() {
    let request = acquisition_request();
    let mut receipt = live_receipt(&request, GeoAcquisitionTerminalState::UnreadableColumns, 0);
    receipt.terminal_detail = Some("executor could not read a requested column".to_string());
    receipt.unreadable_columns = vec!["geometry_wkb_sha256".to_string()];
    validate_geo_acquisition_receipt(&request, &receipt)
        .expect("requested unreadable column validates");

    receipt.unreadable_columns = vec!["not_requested".to_string()];
    assert_eq!(
        validate_geo_acquisition_receipt(&request, &receipt)
            .expect_err("unreadable column must be a requested field")
            .code,
        GeoDiscoveryErrorCode::InvalidInput
    );

    let mut materialized_rows =
        live_receipt(&request, GeoAcquisitionTerminalState::UnreadableColumns, 0);
    materialized_rows.terminal_detail =
        Some("executor could not read a requested column".to_string());
    materialized_rows.unreadable_columns = vec!["geometry_wkb_sha256".to_string()];
    materialized_rows.counts.rows = 1;
    let error = validate_geo_acquisition_receipt(&request, &materialized_rows)
        .expect_err("unreadable-column receipts must not claim materialized rows");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
}

#[test]
fn fixture_retained_and_live_proof_classes_stay_disjoint() {
    let request = acquisition_request();
    let mut live = live_receipt(&request, GeoAcquisitionTerminalState::Complete, 2);
    validate_geo_acquisition_receipt(&request, &live).expect("live proof validates");

    live.fixture_id = Some("fixture:leak".to_string());
    assert_eq!(
        validate_geo_acquisition_receipt(&request, &live)
            .expect_err("live cannot also be fixture")
            .code,
        GeoDiscoveryErrorCode::InvalidInput
    );

    let mut fixture = live_receipt(&request, GeoAcquisitionTerminalState::Complete, 2);
    fixture.proof_class = GeoAcquisitionProofClass::Fixture;
    fixture.executor = None;
    fixture.fixture_id = Some("fixture:contract-only".to_string());
    validate_geo_acquisition_receipt(&request, &fixture).expect("fixture proof validates");

    fixture.retained_receipt_id = Some("retained:leak".to_string());
    assert_eq!(
        validate_geo_acquisition_receipt(&request, &fixture)
            .expect_err("fixture cannot also be retained")
            .code,
        GeoDiscoveryErrorCode::InvalidInput
    );
}

#[test]
fn secrets_and_protocol_endpoints_are_rejected_from_requests() {
    let mut request = geometric_acquisition_request();
    request
        .projection
        .as_mut()
        .expect("geometry request has projection")
        .operation_id = "https://warehouse.example/query".to_string();
    request = with_acquisition_id(request);
    let error = validate_geo_acquisition_request(&request).expect_err("endpoint leaks reject");
    assert_eq!(error.code, GeoDiscoveryErrorCode::SecretMaterial);

    let mut secret = acquisition_request();
    secret.subset.predicates[0].expression = "Authorization: Bearer secret-token".to_string();
    secret = with_acquisition_id(secret);
    let error = validate_geo_acquisition_request(&secret).expect_err("secret leaks reject");
    assert_eq!(error.code, GeoDiscoveryErrorCode::SecretMaterial);

    let mut embedded_endpoint = acquisition_request();
    embedded_endpoint.subset.predicates[0].expression =
        "bounded subset delegated through https://warehouse.example/query".to_string();
    embedded_endpoint = with_acquisition_id(embedded_endpoint);
    let error = validate_geo_acquisition_request(&embedded_endpoint)
        .expect_err("embedded protocol endpoint leaks reject");
    assert_eq!(error.code, GeoDiscoveryErrorCode::SecretMaterial);
}

#[test]
fn receipt_requires_normalized_executed_request_digest_but_value_may_equal_canon_hash() {
    let request = acquisition_request();
    let mut wire = serde_json::to_value(live_receipt(
        &request,
        GeoAcquisitionTerminalState::Complete,
        2,
    ))
    .expect("receipt serializes");
    wire.as_object_mut()
        .expect("object")
        .remove("normalized_executed_request_digest")
        .expect("normalized digest present");
    assert!(
        serde_json::from_value::<GeoAcquisitionReceipt>(wire).is_err(),
        "normalized executed request digest is required"
    );

    let mut receipt = live_receipt(&request, GeoAcquisitionTerminalState::Complete, 2);
    let canon_hash = geo_acquisition_request_semantic_hash(&request).expect("hash computes");
    receipt.normalized_executed_request_digest = GeoDigest {
        digest_id: "executor.normalized_request".to_string(),
        algorithm: GeoDigestAlgorithm::Blake3,
        hex_digest: canon_hash
            .strip_prefix("blake3:")
            .expect("prefixed hash")
            .to_string(),
    };
    validate_geo_acquisition_receipt(&request, &receipt)
        .expect("separate digest fields/domains are enough even if values coincide");
}

#[test]
fn receipt_rejects_projection_drift_and_ceiling_excess() {
    let request = geometric_acquisition_request();
    let mut projection_drift = live_receipt(&request, GeoAcquisitionTerminalState::Complete, 2);
    projection_drift
        .projection
        .as_mut()
        .expect("geometry receipt has projection")
        .operation_version = "v2".to_string();
    assert_eq!(
        validate_geo_acquisition_receipt(&request, &projection_drift)
            .expect_err("projection drift rejects")
            .code,
        GeoDiscoveryErrorCode::ReceiptMismatch
    );

    let mut too_many_rows = live_receipt(&request, GeoAcquisitionTerminalState::Complete, 11);
    too_many_rows.counts.bytes = 512;
    too_many_rows.local_artifacts[0].byte_count = 512;
    assert_eq!(
        validate_geo_acquisition_receipt(&request, &too_many_rows)
            .expect_err("row ceiling rejects")
            .code,
        GeoDiscoveryErrorCode::InvalidInput
    );

    let mut byte_limited = geometric_acquisition_request();
    byte_limited.ceilings.max_bytes = 600;
    byte_limited = with_acquisition_id(byte_limited);
    let mut split_artifacts = live_receipt(&byte_limited, GeoAcquisitionTerminalState::Complete, 2);
    split_artifacts
        .local_artifacts
        .push(GeoLocalArtifactDigest {
            artifact_id: "artifact.fixture.sidecar".to_string(),
            media_type: "application/json".to_string(),
            byte_count: 512,
            digest: blake3_digest("artifact.sidecar", b"sidecar"),
        });
    let error = validate_geo_acquisition_receipt(&byte_limited, &split_artifacts)
        .expect_err("aggregate local artifacts cannot exceed the request byte ceiling");
    assert_eq!(error.code, GeoDiscoveryErrorCode::InvalidInput);
}
