use canon::geo::{
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_OBSERVATION_ROWS_VERSION,
    CANON_GEO_OBSERVER_ADMISSION_REQUEST_VERSION, CANON_GEO_OBSERVER_VERSION,
    DEFAULT_MAX_MATERIALIZED_MODELS, GeoBuildingCandidate, GeoCompositionProfile,
    GeoCompositionUniverse, GeoEvidenceClaimRole, GeoEvidenceCompilationRequest,
    GeoEvidenceDisposition, GeoImageTilePin, GeoObservationKind, GeoObservationPayload,
    GeoObservationRow, GeoObserverAdmissionRequest, GeoObserverContract, GeoObserverErrorCode,
    GeoObserverIdentity, GeoRhoBasis, GeoRhoContract, GeoRhoObservationKind, GeoValidTimeInterval,
    admit_observations_with_universe, admit_observer_request, canonical_observation_rows_bytes,
    canonical_observer_admission_request_bytes, compile_evidence, solve_composition,
    to_rho_observation, validate_observation_rows_artifact, validate_observer_admission_request,
    verify_replay,
};
use std::collections::BTreeMap;

fn hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn interval() -> GeoValidTimeInterval {
    GeoValidTimeInterval {
        start_day: 19_800,
        end_day: 19_830,
    }
}

fn universe() -> GeoCompositionUniverse {
    GeoCompositionUniverse {
        parcels: Vec::new(),
        buildings: vec![
            GeoBuildingCandidate {
                id: "building.alpha".to_string(),
                parcel_ids: Vec::new(),
            },
            GeoBuildingCandidate {
                id: "building.beta".to_string(),
                parcel_ids: Vec::new(),
            },
        ],
    }
}

fn tile_pin(tile_bytes: &[u8]) -> GeoImageTilePin {
    GeoImageTilePin {
        url: "https://example.test/ortho/2024/tile.bin".to_string(),
        byte_range: Some((0, tile_bytes.len() as u64)),
        etag: Some("fixture-etag-2024".to_string()),
        blake3: hex(tile_bytes),
        vintage: interval(),
        license_id: "cc_by_4_0".to_string(),
        license_text_blake3: hex(b"Creative Commons Attribution 4.0 fixture text"),
        source_dataset: "fixture.ortho.2024".to_string(),
    }
}

fn observer_contract(identity: GeoObserverIdentity) -> GeoObserverContract {
    GeoObserverContract {
        id: "observer.structure_count.v0".to_string(),
        version: CANON_GEO_OBSERVER_VERSION.to_string(),
        identity,
        output_kinds: vec![
            GeoObservationKind::StructureCountInWindow,
            GeoObservationKind::FootprintOutline,
            GeoObservationKind::HeightOrFloors,
            GeoObservationKind::PresentAtVintage,
            GeoObservationKind::AbsentAtVintage,
            GeoObservationKind::ChangeEvent,
        ],
        error_population_id: "population.nyc.observer.fixture.v0".to_string(),
        characterization_blake3: hex(b"fixture observer characterization"),
        rho_contract_ids: vec!["rho.structure_count.v0".to_string()],
    }
}

fn rule_based_contract() -> GeoObserverContract {
    observer_contract(GeoObserverIdentity::RuleBased {
        rule_id: "rule.footprint_null_observer".to_string(),
        rule_version: "v0".to_string(),
    })
}

fn rho_contract(admissible_hard_band: bool, claim_role: GeoEvidenceClaimRole) -> GeoRhoContract {
    GeoRhoContract {
        id: "rho.structure_count.v0".to_string(),
        version: "v0".to_string(),
        source_dataset: "fixture.ortho.2024".to_string(),
        source_release: "2024".to_string(),
        source_lineage_ids: vec![
            "characterization.fixture.v0".to_string(),
            "population.nyc.observer.fixture.v0".to_string(),
        ],
        method_id: "observer.structure_count.band".to_string(),
        method_version: "v0".to_string(),
        claim_role,
        basis: GeoRhoBasis::EmpiricalCalibration {
            population_id: "population.nyc.observer.fixture.v0".to_string(),
            calibration_blake3: hex(b"fixture observer characterization"),
            falsification_rule_id: "structure_count_truth_outside_band".to_string(),
            admissible_hard_band,
        },
    }
}

fn count_row(tile_bytes: &[u8], crop_bytes: &[u8], label_bytes: &[u8]) -> GeoObservationRow {
    GeoObservationRow {
        id: "obs.structure_count.1".to_string(),
        observer_id: "observer.structure_count.v0".to_string(),
        tile_pins: vec![tile_pin(tile_bytes)],
        window_blake3: hex(b"window geometry"),
        kind: GeoObservationKind::StructureCountInWindow,
        payload: GeoObservationPayload::StructureCountInWindow { min: 1, max: 1 },
        crop_blake3: hex(crop_bytes),
        label_blake3: hex(label_bytes),
    }
}

fn bytes_by_digest(
    tile_bytes: &[u8],
    crop_bytes: &[u8],
    label_bytes: &[u8],
) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        (hex(tile_bytes), tile_bytes.to_vec()),
        (hex(crop_bytes), crop_bytes.to_vec()),
        (hex(label_bytes), label_bytes.to_vec()),
    ])
}

fn observer_admission_request(row: GeoObservationRow) -> GeoObserverAdmissionRequest {
    GeoObserverAdmissionRequest {
        version: CANON_GEO_OBSERVER_ADMISSION_REQUEST_VERSION.to_string(),
        contract: rule_based_contract(),
        rows: vec![row],
        rho_contracts: vec![rho_contract(
            false,
            GeoEvidenceClaimRole::AttributeObservation,
        )],
        forbidden_license_ids: vec!["commercial_basemap_tos".to_string()],
        universe: universe(),
    }
}

#[test]
fn t13_observer_admission_refuses_missing_provenance_uncharacterized_error_and_forbidden_license() {
    let tile_bytes = b"fixture tile bytes";
    let crop_bytes = b"fixture crop bytes";
    let label_bytes = b"{\"count\":1}";
    let row = count_row(tile_bytes, crop_bytes, label_bytes);
    let rho = vec![rho_contract(
        false,
        GeoEvidenceClaimRole::AttributeObservation,
    )];
    let forbidden = vec!["commercial_basemap_tos".to_string()];

    let frozen_without_weights = observer_contract(GeoObserverIdentity::FrozenWeight {
        model_id: "model.structure_count".to_string(),
        weight_blake3: String::new(),
        arithmetic_contract: "integer-only.v0".to_string(),
    });
    let error = admit_observations_with_universe(
        &frozen_without_weights,
        std::slice::from_ref(&row),
        &rho,
        &forbidden,
        &universe(),
    )
    .expect_err("empty frozen weights must refuse");
    assert_eq!(error.code, GeoObserverErrorCode::ObserverMissingProvenance);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("identity.weight_blake3")
    );

    let mut uncharacterized = rule_based_contract();
    uncharacterized.characterization_blake3.clear();
    let error = admit_observations_with_universe(
        &uncharacterized,
        std::slice::from_ref(&row),
        &rho,
        &forbidden,
        &universe(),
    )
    .expect_err("empty characterization must refuse");
    assert_eq!(
        error.code,
        GeoObserverErrorCode::ObserverErrorUncharacterized
    );
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("characterization_blake3")
    );

    let mut forbidden_row = row.clone();
    forbidden_row.tile_pins[0].license_id = "commercial_basemap_tos".to_string();
    let error = admit_observations_with_universe(
        &rule_based_contract(),
        &[forbidden_row],
        &rho,
        &forbidden,
        &universe(),
    )
    .expect_err("commercial basemap license must refuse");
    assert_eq!(error.code, GeoObserverErrorCode::ObserverLicenseForbidden);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("license_id")
    );

    let error = admit_observations_with_universe(
        &rule_based_contract(),
        std::slice::from_ref(&row),
        &rho,
        &[],
        &universe(),
    )
    .expect_err("an empty forbidden-license policy is not a gate");
    assert_eq!(error.code, GeoObserverErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("forbidden_license_ids")
    );

    let artifact = admit_observations_with_universe(
        &rule_based_contract(),
        &[row],
        &rho,
        &forbidden,
        &universe(),
    )
    .expect("CC BY fixture pin admits");
    assert_eq!(artifact.version, CANON_GEO_OBSERVATION_ROWS_VERSION);
    assert!(artifact.not_admitted_ids.is_empty());
    assert_eq!(artifact.rho_observations.len(), 1);
    validate_observation_rows_artifact(&artifact).expect("admitted artifact validates");
    canonical_observation_rows_bytes(&artifact).expect("admitted artifact has canonical bytes");

    let request = observer_admission_request(artifact.rows[0].clone());
    validate_observer_admission_request(&request).expect("admission request validates");
    canonical_observer_admission_request_bytes(&request).expect("admission request canonicalizes");
    let from_request = admit_observer_request(&request).expect("request admits through rho");
    assert_eq!(from_request.rho_observations, artifact.rho_observations);

    let mut request_without_policy = request;
    request_without_policy.forbidden_license_ids.clear();
    let error = validate_observer_admission_request(&request_without_policy)
        .expect_err("empty forbidden-license policy refuses at the request boundary");
    assert_eq!(error.code, GeoObserverErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("forbidden_license_ids")
    );
}

#[test]
fn t13_observer_kind_serialization_is_exactly_the_six_section_18_4_kinds() {
    let variants = [
        GeoObservationKind::StructureCountInWindow,
        GeoObservationKind::FootprintOutline,
        GeoObservationKind::HeightOrFloors,
        GeoObservationKind::PresentAtVintage,
        GeoObservationKind::AbsentAtVintage,
        GeoObservationKind::ChangeEvent,
    ]
    .into_iter()
    .map(|kind| serde_json::to_value(kind).expect("kind serializes"))
    .collect::<Vec<_>>();
    assert_eq!(
        variants,
        vec![
            serde_json::json!("structure_count_in_window"),
            serde_json::json!("footprint_outline"),
            serde_json::json!("height_or_floors"),
            serde_json::json!("present_at_vintage"),
            serde_json::json!("absent_at_vintage"),
            serde_json::json!("change_event"),
        ]
    );
    assert!(
        !variants
            .iter()
            .any(|variant| variant.as_str() == Some("location")),
        "a location-proposing observer kind is outside the D6 contract"
    );
}

#[test]
fn t14_observer_replay_verifies_pinned_tile_crop_and_label_without_regeneration() {
    let tile_bytes = b"fixture tile bytes";
    let crop_bytes = b"fixture crop bytes";
    let label_bytes = b"{\"count\":1}";
    let row = count_row(tile_bytes, crop_bytes, label_bytes);
    let artifact = admit_observations_with_universe(
        &rule_based_contract(),
        std::slice::from_ref(&row),
        &[rho_contract(
            false,
            GeoEvidenceClaimRole::AttributeObservation,
        )],
        &["commercial_basemap_tos".to_string()],
        &universe(),
    )
    .expect("fixture observation admits");
    let bytes_by_blake3 = bytes_by_digest(tile_bytes, crop_bytes, label_bytes);
    verify_replay(&artifact, &bytes_by_blake3).expect("stored bytes replay");

    let mut flipped_tile = bytes_by_blake3.clone();
    let mut changed_tile = tile_bytes.to_vec();
    changed_tile[0] ^= 1;
    flipped_tile.insert(row.tile_pins[0].blake3.clone(), changed_tile);
    let error = verify_replay(&artifact, &flipped_tile).expect_err("tile digest mismatch refuses");
    assert_eq!(error.code, GeoObserverErrorCode::ImageTileDigestMismatch);
    assert_eq!(
        error.detail.get("tile").map(String::as_str),
        Some(row.tile_pins[0].blake3.as_str())
    );

    let mut flipped_crop = bytes_by_blake3.clone();
    let mut changed_crop = crop_bytes.to_vec();
    changed_crop[0] ^= 1;
    flipped_crop.insert(row.crop_blake3.clone(), changed_crop);
    let error = verify_replay(&artifact, &flipped_crop).expect_err("crop digest mismatch refuses");
    assert_eq!(error.code, GeoObserverErrorCode::ImageTileDigestMismatch);
    assert_eq!(
        error.detail.get("crop").map(String::as_str),
        Some(row.crop_blake3.as_str())
    );

    let mut regenerated = artifact.clone();
    regenerated.rows[0].label_blake3 = hex(b"{\"count\":2}");
    let error = verify_replay(&regenerated, &bytes_by_blake3)
        .expect_err("changed stored row digest refuses as regeneration");
    assert_eq!(
        error.code,
        GeoObserverErrorCode::ObservationRegeneratedAtReplay
    );
    assert_eq!(
        error.detail.get("observation_id").map(String::as_str),
        Some(row.id.as_str())
    );

    let source = include_str!("../src/geo/observer.rs").to_ascii_lowercase();
    for forbidden in [
        "reqwest",
        "hyper::",
        "openai",
        "anthropic",
        "gemini",
        "bedrock",
        "invoke_model",
        "std::process::command",
        "logicalrelaxation",
        "observer:",
    ] {
        assert!(
            !source.contains(forbidden),
            "observer replay path must not invoke or smuggle {forbidden}"
        );
    }
}

#[test]
fn t15_present_at_vintage_stays_temporal_diagnostic_and_does_not_constrain_identity() {
    let tile_bytes = b"fixture tile bytes";
    let crop_bytes = b"fixture crop bytes";
    let label_bytes = b"{\"present\":true}";
    let mut row = count_row(tile_bytes, crop_bytes, label_bytes);
    row.id = "obs.present.1".to_string();
    row.kind = GeoObservationKind::PresentAtVintage;
    row.payload = GeoObservationPayload::PresentAtVintage {
        interval: interval(),
    };
    let contract = rule_based_contract();
    let universe = universe();
    let rho = rho_contract(false, GeoEvidenceClaimRole::TemporalOccupancy);

    let observation =
        to_rho_observation(&row, &contract, &universe).expect("vintage observation emits rho row");
    assert_eq!(observation.valid_time, Some(interval()));
    assert!(matches!(
        observation.observation,
        GeoRhoObservationKind::ExistentialMembership { .. }
    ));

    let artifact = admit_observations_with_universe(
        &contract,
        &[row],
        std::slice::from_ref(&rho),
        &["commercial_basemap_tos".to_string()],
        &universe,
    )
    .expect("vintage row admits diagnostically");
    assert_eq!(artifact.diagnostic_only_ids, vec!["obs.present.1"]);
    assert_eq!(artifact.rho_observations.len(), 1);

    let compilation = compile_evidence(&GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::building(),
        universe,
        contracts: vec![rho],
        observations: artifact.rho_observations,
        max_assignments: 64,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    })
    .expect("temporal observer row compiles as diagnostic");
    assert!(compilation.composition_request.hard_constraints.is_empty());
    assert_eq!(
        compilation.admissions[0].disposition,
        GeoEvidenceDisposition::DiagnosticOnly
    );
    let solved = solve_composition(&compilation.composition_request)
        .expect("diagnostic-only request still solves");
    assert_eq!(solved.summary.residual_model_count, 3);
}

#[test]
fn t68_observer_count_band_enters_solver_only_under_flagged_empirical_rho_contract() {
    let tile_bytes = b"fixture tile bytes";
    let crop_bytes = b"fixture crop bytes";
    let label_bytes = b"{\"count\":1}";
    let row = count_row(tile_bytes, crop_bytes, label_bytes);
    let contract = rule_based_contract();
    let composition_universe = universe();
    let observation = to_rho_observation(&row, &contract, &composition_universe)
        .expect("count observation emits rho row");

    let request = |rho: GeoRhoContract| GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::building(),
        universe: universe(),
        contracts: vec![rho],
        observations: vec![observation.clone()],
        max_assignments: 64,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };

    let unflagged = compile_evidence(&request(rho_contract(
        false,
        GeoEvidenceClaimRole::AttributeObservation,
    )))
    .expect("unflagged empirical count remains diagnostic");
    assert!(unflagged.composition_request.hard_constraints.is_empty());
    assert_eq!(
        unflagged.admissions[0].admission_reason.as_deref(),
        Some("rho_band_not_admissible")
    );
    assert_eq!(
        solve_composition(&unflagged.composition_request)
            .expect("unflagged request solves")
            .summary
            .residual_model_count,
        3
    );

    let mut malformed = rho_contract(true, GeoEvidenceClaimRole::AttributeObservation);
    if let GeoRhoBasis::EmpiricalCalibration {
        calibration_blake3, ..
    } = &mut malformed.basis
    {
        calibration_blake3.clear();
    }
    let error = compile_evidence(&request(malformed))
        .expect_err("flagged empirical hard band still requires calibration digest");
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("contracts[].basis.calibration_blake3")
    );

    let flagged = compile_evidence(&request(rho_contract(
        true,
        GeoEvidenceClaimRole::AttributeObservation,
    )))
    .expect("flagged characterized count becomes a hard band");
    assert_eq!(flagged.composition_request.hard_constraints.len(), 1);
    let hard = &flagged.composition_request.hard_constraints[0];
    assert_eq!(
        hard.id,
        "rho:rho.structure_count.v0@v0:obs.structure_count.1"
    );
    let solved =
        solve_composition(&flagged.composition_request).expect("flagged count-band request solves");
    assert_eq!(solved.summary.residual_model_count, 2);
}
