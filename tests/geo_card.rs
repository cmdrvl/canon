use canon::geo::{
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_GEOMETRY_VALUE_VERSION,
    GeoArtifactFieldClassification, GeoArtifactFieldLicenseClass, GeoBoundingBoxMm,
    GeoCandidateReachStatus, GeoCanonicalGeometryMm, GeoCanonicalPolygonMm, GeoCanonicalRingMm,
    GeoCompositionArtifact, GeoCompositionStatus, GeoCompositionUniverse, GeoEntityLevel,
    GeoEntityRef, GeoEvidenceCardBuildContext, GeoEvidenceCardCoverage,
    GeoEvidenceCardCoverageState, GeoEvidenceCardProofClass, GeoEvidenceCardSubjectRef,
    GeoEvidenceClaimRole, GeoEvidenceCompilationArtifact, GeoEvidenceCompilationReference,
    GeoEvidenceCompilationRequest, GeoEvidenceRecordRef, GeoImageTilePin, GeoPointMm,
    GeoQuantizationAudit, GeoRhoBasis, GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind,
    GeoSourceReleasePin, GeoTruthPlane, GeoTruthRepresentationGrain, GeoTypedGeometry,
    GeoValidTimeInterval, build_evidence_card_with_context, build_reach_none_evidence_card,
    canonical_evidence_card_bytes, canonical_evidence_compilation_bytes, compile_evidence,
    minimal_core, reliability_order_from_evidence, solve_composition,
    validate_evidence_card_artifact, verify_evidence_card_tile_replay,
};
use std::collections::BTreeMap;

fn hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn prefixed_hex(bytes: &[u8]) -> String {
    format!("blake3:{}", hex(bytes))
}

fn source_record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "fixture-release".to_string(),
        record_blake3: hex(id.as_bytes()),
    }
}

fn contract(id: &str) -> GeoRhoContract {
    GeoRhoContract {
        id: id.to_string(),
        version: "v1".to_string(),
        source_dataset: "fixture.card.evidence".to_string(),
        source_release: "fixture-release".to_string(),
        source_lineage_ids: vec![format!("fixture.lineage.{id}")],
        method_id: "fixture.card.rho".to_string(),
        method_version: "v1".to_string(),
        claim_role: GeoEvidenceClaimRole::StableIdentityAnchor,
        basis: GeoRhoBasis::LogicalRelaxation {
            invariant_id: format!("fixture.invariant.{id}"),
        },
    }
}

fn universe(ids: &[&str]) -> GeoCompositionUniverse {
    GeoCompositionUniverse {
        parcels: ids.iter().map(|id| (*id).to_string()).collect(),
        buildings: Vec::new(),
    }
}

fn exact_sets_observation(id: &str, contract_id: &str, sets: Vec<Vec<&str>>) -> GeoRhoObservation {
    GeoRhoObservation {
        id: id.to_string(),
        contract_id: contract_id.to_string(),
        source_records: vec![source_record(&format!("{id}-row"))],
        valid_time: None,
        observation: GeoRhoObservationKind::ExactSets {
            level: GeoEntityLevel::Parcel,
            sets: sets
                .into_iter()
                .map(|set| set.into_iter().map(str::to_string).collect())
                .collect(),
        },
    }
}

fn any_of_observation(id: &str, contract_id: &str, ids: &[&str]) -> GeoRhoObservation {
    GeoRhoObservation {
        id: id.to_string(),
        contract_id: contract_id.to_string(),
        source_records: vec![source_record(&format!("{id}-row"))],
        valid_time: None,
        observation: GeoRhoObservationKind::ExistentialMembership {
            members: ids
                .iter()
                .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, *id))
                .collect(),
        },
    }
}

fn compile_and_solve(
    evidence_request: GeoEvidenceCompilationRequest,
) -> (GeoEvidenceCompilationArtifact, GeoCompositionArtifact) {
    let evidence = compile_evidence(&evidence_request).expect("fixture evidence compiles");
    let mut composition =
        solve_composition(&evidence.composition_request).expect("compiled request solves");
    let evidence_bytes =
        canonical_evidence_compilation_bytes(&evidence).expect("evidence serializes");
    composition.evidence_compilation = Some(GeoEvidenceCompilationReference {
        version: evidence.version.clone(),
        request_version: evidence.request_version.clone(),
        blake3: hex(&evidence_bytes),
    });
    (evidence, composition)
}

fn resolved_evidence_and_composition() -> (GeoEvidenceCompilationArtifact, GeoCompositionArtifact) {
    compile_and_solve(GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: universe(&[
            "bbl.1012920001",
            "bbl.1012920026",
            "bbl.1012930001",
            "bbl.1012930026",
        ]),
        contracts: vec![contract("exact-two-lot")],
        observations: vec![exact_sets_observation(
            "obs.237_park.address_breaks_tie",
            "exact-two-lot",
            vec![vec!["bbl.1012920001", "bbl.1012920026"]],
        )],
        max_assignments: 64,
        max_materialized_models: 64,
    })
}

fn ambiguous_evidence_and_composition() -> (GeoEvidenceCompilationArtifact, GeoCompositionArtifact)
{
    compile_and_solve(GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: universe(&["bbl.1012920001", "bbl.1012920026"]),
        contracts: vec![contract("roof-point-containment")],
        observations: vec![any_of_observation(
            "obs.237_park.two_lots_contain_point",
            "roof-point-containment",
            &["bbl.1012920001", "bbl.1012920026"],
        )],
        max_assignments: 16,
        max_materialized_models: 16,
    })
}

fn conflict_evidence_and_composition() -> (GeoEvidenceCompilationArtifact, GeoCompositionArtifact) {
    compile_and_solve(GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: universe(&["bbl.1012920001", "bbl.1012920026"]),
        contracts: vec![contract("exact-left"), contract("exact-right")],
        observations: vec![
            exact_sets_observation("obs.left", "exact-left", vec![vec!["bbl.1012920001"]]),
            exact_sets_observation("obs.right", "exact-right", vec![vec!["bbl.1012920026"]]),
        ],
        max_assignments: 16,
        max_materialized_models: 16,
    })
}

fn square(min_x: i64, min_y: i64, max_x: i64, max_y: i64) -> GeoCanonicalPolygonMm {
    GeoCanonicalPolygonMm {
        exterior: GeoCanonicalRingMm {
            vertices: vec![
                GeoPointMm::new(min_x, min_y),
                GeoPointMm::new(max_x, min_y),
                GeoPointMm::new(max_x, max_y),
                GeoPointMm::new(min_x, max_y),
            ],
        },
        holes: Vec::new(),
    }
}

fn geometry(min_x: i64, min_y: i64) -> GeoTypedGeometry {
    GeoTypedGeometry {
        version: CANON_GEO_GEOMETRY_VALUE_VERSION.to_string(),
        source_crs: "LOCAL:FIXTURE".to_string(),
        local_frame_id: "fixture.card.frame".to_string(),
        coordinate_unit: "millimetre".to_string(),
        coordinate_scale: 1,
        vertex_count: 4,
        bbox: GeoBoundingBoxMm {
            min_x,
            min_y,
            max_x: min_x + 10,
            max_y: min_y + 10,
        },
        quantization: GeoQuantizationAudit {
            max_abs_snap_error_numerator_mm: 0,
            affine_denominator: 1,
            max_abs_snap_error_micrometres_ceiling: 0,
            projection_error_envelope_micrometres: 0,
            combined_error_envelope_micrometres: 0,
            minimum_nonzero_bbox_extent_mm: Some(10),
            endpoint_distance_error_ppm_upper_bound: Some(0),
        },
        geometry: GeoCanonicalGeometryMm::Polygon {
            polygon: square(min_x, min_y, min_x + 10, min_y + 10),
        },
    }
}

fn geometry_by_id(ids: &[&str]) -> BTreeMap<String, GeoTypedGeometry> {
    ids.iter()
        .enumerate()
        .map(|(index, id)| ((*id).to_string(), geometry((index as i64) * 20, 0)))
        .collect()
}

fn tile_pin(tile_bytes: &[u8]) -> GeoImageTilePin {
    GeoImageTilePin {
        url: "https://example.test/ortho/tile.bin".to_string(),
        byte_range: Some((0, tile_bytes.len() as u64)),
        etag: Some("fixture-etag".to_string()),
        blake3: hex(tile_bytes),
        vintage: GeoValidTimeInterval {
            start_day: 19_700,
            end_day: 19_730,
        },
        license_id: "cc_by_4_0".to_string(),
        license_text_blake3: hex(b"fixture license"),
        source_dataset: "fixture.ortho.2024".to_string(),
    }
}

fn source_pin(id: &str) -> GeoSourceReleasePin {
    GeoSourceReleasePin {
        source_dataset: format!("fixture.{id}"),
        source_release: "fixture-release".to_string(),
        blake3: prefixed_hex(id.as_bytes()),
    }
}

fn field_classifications() -> Vec<GeoArtifactFieldClassification> {
    vec![
        GeoArtifactFieldClassification {
            field_path: "$.candidate_parcels[].geometry".to_string(),
            license_class: GeoArtifactFieldLicenseClass::LicensedGeometry,
            source_instance_id: Some("fixture.mappluto".to_string()),
            reconstructive: true,
            rationale: "retained parcel geometry is enough to draw candidates offline".to_string(),
        },
        GeoArtifactFieldClassification {
            field_path: "$.composition_status".to_string(),
            license_class: GeoArtifactFieldLicenseClass::Public,
            source_instance_id: None,
            reconstructive: false,
            rationale: "decision state is shareable".to_string(),
        },
        GeoArtifactFieldClassification {
            field_path: "$.geometry_source_pins".to_string(),
            license_class: GeoArtifactFieldLicenseClass::Identifier,
            source_instance_id: None,
            reconstructive: false,
            rationale: "source pins identify retained geometry bytes".to_string(),
        },
        GeoArtifactFieldClassification {
            field_path: "$.multi_containment_cardinality".to_string(),
            license_class: GeoArtifactFieldLicenseClass::DerivedMeasure,
            source_instance_id: None,
            reconstructive: false,
            rationale: "cardinal count is derived from candidates".to_string(),
        },
    ]
}

fn card_context() -> GeoEvidenceCardBuildContext {
    GeoEvidenceCardBuildContext {
        proof_class: GeoEvidenceCardProofClass::Fixture,
        subject_ref: Some(GeoEvidenceCardSubjectRef {
            accession: "fixture-accession".to_string(),
            deal_id: "fixture-deal".to_string(),
            loan_id: "fixture-loan".to_string(),
            deed_ids: vec!["fixture-deed".to_string()],
        }),
        truth_plane: Some(GeoTruthPlane::GateV2Historical),
        reach: GeoCandidateReachStatus::Full,
        reach_none_reason: None,
        coverage: GeoEvidenceCardCoverage {
            state: GeoEvidenceCardCoverageState::Covered,
            reason: None,
        },
        answer_grain: GeoTruthRepresentationGrain::UnitLot,
        answer_grain_caveat: None,
        geometry_source_pins: vec![source_pin("mappluto.geom")],
        home_cell: Some("892a100d2d3ffff".to_string()),
        halo_members: vec![GeoEntityRef::new(GeoEntityLevel::Parcel, "bbl.1012930001")],
        multi_containment_cardinality: None,
        field_classifications: field_classifications(),
    }
}

#[test]
fn evidence_card_resolved_artifact_carries_rejected_candidates_and_replays_tile_pin() {
    let (evidence, composition) = resolved_evidence_and_composition();
    assert_eq!(composition.status, GeoCompositionStatus::Resolved);
    let tile_bytes = b"retained fixture ortho tile bytes";
    let card = build_evidence_card_with_context(
        "subject.237_park",
        &composition,
        &evidence,
        None,
        &tile_pin(tile_bytes),
        &geometry_by_id(&[
            "bbl.1012920001",
            "bbl.1012920026",
            "bbl.1012930001",
            "bbl.1012930026",
        ]),
        card_context(),
    )
    .expect("resolved evidence card builds");

    assert_eq!(card.candidate_parcels.len(), 4);
    assert_eq!(
        card.forced.parcels,
        vec!["bbl.1012920001".to_string(), "bbl.1012920026".to_string()]
    );
    assert_eq!(card.ambiguous_members.len(), 0);
    assert!(
        card.candidate_parcels
            .iter()
            .any(|candidate| candidate.id == "bbl.1012930001" && candidate.in_halo),
        "card must retain rejected halo candidates rather than only the winner"
    );
    assert!(card.explanation_blake3.is_none());
    assert_eq!(card.reach, GeoCandidateReachStatus::Full);
    assert_eq!(card.coverage.state, GeoEvidenceCardCoverageState::Covered);
    assert_eq!(card.evidence_admissions.len(), 1);
    assert!(card.composition_blake3.starts_with("blake3:"));
    assert!(card.evidence_blake3.starts_with("blake3:"));
    assert!(card.request_blake3.starts_with("blake3:"));
    let bytes_by_blake3 = BTreeMap::from([(hex(tile_bytes), tile_bytes.to_vec())]);
    verify_evidence_card_tile_replay(&card, &bytes_by_blake3)
        .expect("card replays from retained tile bytes");
    canonical_evidence_card_bytes(&card).expect("card canonicalizes");
}

#[test]
fn evidence_card_abstention_distinguishes_ambiguous_candidates_from_no_coverage() {
    let (evidence, composition) = ambiguous_evidence_and_composition();
    assert_eq!(composition.status, GeoCompositionStatus::Ambiguous);
    let tile_bytes = b"ambiguous retained fixture ortho tile bytes";
    let mut context = card_context();
    context.multi_containment_cardinality = Some(2);
    let card = build_evidence_card_with_context(
        "subject.237_park_no_address_channel",
        &composition,
        &evidence,
        None,
        &tile_pin(tile_bytes),
        &geometry_by_id(&["bbl.1012920001", "bbl.1012920026"]),
        context,
    )
    .expect("ambiguous evidence card builds");
    assert_eq!(card.candidate_parcels.len(), 2);
    assert_eq!(card.ambiguous_members.len(), 2);
    assert!(card.reach_none_reason.is_none());
    assert_eq!(card.multi_containment_cardinality, 2);

    let mut no_coverage_context = card_context();
    no_coverage_context.reach = GeoCandidateReachStatus::None;
    no_coverage_context.reach_none_reason = Some("client_layer_does_not_cover_tile".to_string());
    no_coverage_context.coverage = GeoEvidenceCardCoverage {
        state: GeoEvidenceCardCoverageState::Absent,
        reason: Some("client_layer_does_not_cover_tile".to_string()),
    };
    no_coverage_context.halo_members.clear();
    let no_coverage = build_reach_none_evidence_card(
        "subject.byop.partial_coverage",
        &tile_pin(b"partial coverage tile"),
        no_coverage_context,
    )
    .expect("reach-none card builds");
    assert_eq!(no_coverage.reach, GeoCandidateReachStatus::None);
    assert_eq!(
        no_coverage.coverage.state,
        GeoEvidenceCardCoverageState::Absent
    );
    assert!(no_coverage.candidate_parcels.is_empty());
    assert!(no_coverage.ambiguous_members.is_empty());
    assert_eq!(no_coverage.composition_blake3, "");
}

#[test]
fn evidence_card_conflict_requires_matching_explanation_chain_and_keeps_source_records() {
    let (evidence, composition) = conflict_evidence_and_composition();
    assert_eq!(composition.status, GeoCompositionStatus::Conflict);
    let explanation = minimal_core(
        &evidence.composition_request,
        &evidence,
        &reliability_order_from_evidence(&evidence),
        &Default::default(),
    )
    .expect("conflict explanation builds");
    let card = build_evidence_card_with_context(
        "subject.chimera_conflict",
        &composition,
        &evidence,
        Some(&explanation),
        &tile_pin(b"conflict retained fixture tile"),
        &geometry_by_id(&["bbl.1012920001", "bbl.1012920026"]),
        card_context(),
    )
    .expect("conflict evidence card builds");
    assert!(card.explanation_blake3.is_some());
    assert!(
        !card.conflicting_records.is_empty(),
        "conflict card must retain source records from the chained explanation"
    );

    let mut edited = explanation.clone();
    edited.request_blake3 = prefixed_hex(b"wrong request");
    let error = build_evidence_card_with_context(
        "subject.chimera_conflict",
        &composition,
        &evidence,
        Some(&edited),
        &tile_pin(b"conflict retained fixture tile"),
        &geometry_by_id(&["bbl.1012920001", "bbl.1012920026"]),
        card_context(),
    )
    .expect_err("edited explanation digest must refuse");
    assert_eq!(
        error.code,
        canon::geo::GeoCardErrorCode::CardArtifactMismatch
    );
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("request_blake3")
    );
}

#[test]
fn evidence_card_refuses_foreign_evidence_missing_geometry_and_fixture_live_relabel() {
    let (evidence, mut composition) = resolved_evidence_and_composition();
    composition
        .evidence_compilation
        .as_mut()
        .expect("composition binds evidence")
        .blake3 = hex(b"wrong evidence");
    let error = build_evidence_card_with_context(
        "subject.237_park",
        &composition,
        &evidence,
        None,
        &tile_pin(b"tile"),
        &geometry_by_id(&[
            "bbl.1012920001",
            "bbl.1012920026",
            "bbl.1012930001",
            "bbl.1012930026",
        ]),
        card_context(),
    )
    .expect_err("foreign evidence digest must refuse");
    assert_eq!(
        error.code,
        canon::geo::GeoCardErrorCode::CardArtifactMismatch
    );

    let (evidence, composition) = resolved_evidence_and_composition();
    let error = build_evidence_card_with_context(
        "subject.237_park",
        &composition,
        &evidence,
        None,
        &tile_pin(b"tile"),
        &geometry_by_id(&["bbl.1012920001"]),
        card_context(),
    )
    .expect_err("candidate missing retained geometry must refuse");
    assert_eq!(
        error.code,
        canon::geo::GeoCardErrorCode::CardArtifactMismatch
    );
    assert_eq!(
        error.detail.get("member").map(String::as_str),
        Some("bbl.1012920026")
    );

    let mut retained_context = card_context();
    retained_context.proof_class = GeoEvidenceCardProofClass::RetainedArtifact;
    let error = build_evidence_card_with_context(
        "subject.237_park",
        &composition,
        &evidence,
        None,
        &tile_pin(b"tile"),
        &geometry_by_id(&[
            "bbl.1012920001",
            "bbl.1012920026",
            "bbl.1012930001",
            "bbl.1012930026",
        ]),
        retained_context,
    )
    .expect_err("fixture source relabeled as retained must refuse");
    assert_eq!(error.code, canon::geo::GeoCardErrorCode::InvalidInput);
}

#[test]
fn evidence_card_carries_billing_lot_representation_grain_caveat() {
    let (evidence, composition) = ambiguous_evidence_and_composition();
    let mut context = card_context();
    context.answer_grain = GeoTruthRepresentationGrain::BillingLot;
    context.answer_grain_caveat = Some(
        "condo unit lots have no landed unit geometry; billing lot represents the candidate geometry grain"
            .to_string(),
    );
    let card = build_evidence_card_with_context(
        "subject.condo_unit_represented_by_billing_lot",
        &composition,
        &evidence,
        None,
        &tile_pin(b"billing lot representation tile"),
        &geometry_by_id(&["bbl.1012920001", "bbl.1012920026"]),
        context,
    )
    .expect("billing-lot representation card builds with caveat");
    assert_eq!(card.answer_grain, GeoTruthRepresentationGrain::BillingLot);
    assert!(
        card.answer_grain_caveat
            .as_deref()
            .unwrap_or_default()
            .contains("billing lot represents")
    );

    let mut missing = card.clone();
    missing.answer_grain_caveat = None;
    let error = validate_evidence_card_artifact(&missing)
        .expect_err("billing-lot representation without caveat must refuse");
    assert_eq!(error.code, canon::geo::GeoCardErrorCode::InvalidInput);
}

#[test]
fn evidence_card_module_has_no_decoder_or_fetch_dependency_literals() {
    let source = std::fs::read_to_string("src/geo/card.rs").expect("card module source");
    let lower = source.to_ascii_lowercase();
    for forbidden in [
        "reqwest", "ureq", "tokio::", "http://", "https://", ".png", ".svg", ".tiff", "geotiff",
    ] {
        assert!(
            !lower.contains(forbidden),
            "card module must stay data-only; found {forbidden}"
        );
    }
}
