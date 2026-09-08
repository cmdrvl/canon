#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, CANON_GEO_ERROR_POPULATION_VERSION,
    CANON_GEO_EVIDENCE_REQUEST_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS, GEO_COUNT_OBSERVER_ID,
    GeoBuildingCandidate, GeoCompositionProfile, GeoCompositionRequest, GeoCompositionUniverse,
    GeoErrorPopulationArtifact, GeoErrorPopulationSubject, GeoEvidenceCompilationRequest,
    GeoEvidenceDisposition, GeoImageTilePin, GeoObservationKind, GeoObservationPayload,
    GeoObservationRow, GeoObserverErrorCode, GeoRhoBasis, GeoRhoContract, GeoRhoObservationKind,
    GeoTruthPlane, GeoValidTimeInterval, admit_characterized_count_observations,
    canonical_observer_characterization_bytes, characterize_count_observer, compile_evidence,
    frozen_count_observer_contract, frozen_count_rho_contract, measure_observer_effect,
    solve_composition, truth_plane_key, validate_count_band_not_narrowed, verify_replay,
    widen_count_observation_row,
};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn interval() -> GeoValidTimeInterval {
    GeoValidTimeInterval {
        start_day: 19_723,
        end_day: 19_723,
    }
}

fn tile_pin(tile_bytes: &[u8]) -> GeoImageTilePin {
    GeoImageTilePin {
        url: "s3://fixture/observer-count/tile.bin".to_string(),
        byte_range: Some((0, tile_bytes.len() as u64)),
        etag: Some("\"fixture-count-etag\"".to_string()),
        blake3: hex(tile_bytes),
        vintage: interval(),
        license_id: "cc_by_4_0".to_string(),
        license_text_blake3: hex(b"Creative Commons Attribution 4.0 fixture text"),
        source_dataset: "fixture.nyc_ortho.2024".to_string(),
    }
}

fn placeholder_contract() -> canon::geo::GeoObserverContract {
    frozen_count_observer_contract(
        "count.tiny.fixture",
        hex(b"fixture frozen count weights"),
        "onnxruntime-cpu-fixture-int8-single-thread",
        "population.nyc.count.fixture",
        hex(b"placeholder count characterization"),
    )
}

fn contract_with_characterization(
    characterization_blake3: &str,
) -> canon::geo::GeoObserverContract {
    frozen_count_observer_contract(
        "count.tiny.fixture",
        hex(b"fixture frozen count weights"),
        "onnxruntime-cpu-fixture-int8-single-thread",
        "population.nyc.count.fixture",
        characterization_blake3,
    )
}

fn population() -> GeoErrorPopulationArtifact {
    let subjects = vec![
        subject("s1", GeoTruthPlane::NonRoundAmountDateLegalBorough),
        subject("s2", GeoTruthPlane::NonRoundAmountDateLegalBorough),
        subject("s3", GeoTruthPlane::RoundExactLenderParty),
        subject("s4", GeoTruthPlane::RoundExactLenderParty),
        subject("s5", GeoTruthPlane::RoundExactLenderParty),
    ];
    GeoErrorPopulationArtifact {
        version: CANON_GEO_ERROR_POPULATION_VERSION.to_string(),
        population_id: "population.nyc.count.fixture".to_string(),
        region: "nyc".to_string(),
        selection_seed: Some(7),
        selection_query_blake3: Some(hex(b"fixture count population query")),
        source_population_blake3: Some(hex(b"fixture count source population")),
        subjects,
        declared_before_observer_ids: vec![GEO_COUNT_OBSERVER_ID.to_string()],
        stratum_counts: BTreeMap::from([
            (
                truth_plane_key(GeoTruthPlane::NonRoundAmountDateLegalBorough).to_string(),
                2,
            ),
            (
                truth_plane_key(GeoTruthPlane::RoundExactLenderParty).to_string(),
                3,
            ),
        ]),
    }
}

fn subject(subject_id: &str, truth_plane: GeoTruthPlane) -> GeoErrorPopulationSubject {
    GeoErrorPopulationSubject {
        subject_id: subject_id.to_string(),
        truth_plane,
        window_blake3: window_digest(subject_id),
        parcel_ids: vec![format!("parcel.{subject_id}")],
    }
}

fn window_digest(subject_id: &str) -> String {
    hex(format!("window:{subject_id}").as_bytes())
}

fn count_row(subject_id: &str, raw_count: u64) -> GeoObservationRow {
    let tile_bytes = format!("fixture tile bytes {subject_id}");
    let crop_bytes = format!("fixture crop bytes {subject_id}");
    let label_bytes = format!("{{\"count\":{raw_count}}}");
    GeoObservationRow {
        id: format!("obs:count:{subject_id}:2024"),
        observer_id: GEO_COUNT_OBSERVER_ID.to_string(),
        tile_pins: vec![tile_pin(tile_bytes.as_bytes())],
        window_blake3: window_digest(subject_id),
        kind: GeoObservationKind::StructureCountInWindow,
        payload: GeoObservationPayload::StructureCountInWindow {
            min: raw_count,
            max: raw_count,
        },
        raw_count: Some(raw_count),
        crop_blake3: hex(crop_bytes.as_bytes()),
        label_blake3: hex(label_bytes.as_bytes()),
    }
}

fn reference_counts() -> BTreeMap<String, u64> {
    BTreeMap::from([
        ("s1".to_string(), 6),
        ("s2".to_string(), 1),
        ("s3".to_string(), 3),
        ("s4".to_string(), 2),
        ("s5".to_string(), 4),
    ])
}

fn seven_building_universe() -> GeoCompositionUniverse {
    GeoCompositionUniverse {
        parcels: Vec::new(),
        buildings: (1..=7)
            .map(|index| GeoBuildingCandidate {
                id: format!("building.{index}"),
                parcel_ids: Vec::new(),
            })
            .collect(),
    }
}

fn building_request(universe: GeoCompositionUniverse) -> GeoCompositionRequest {
    GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::building(),
        universe,
        hard_constraints: Vec::new(),
        soft_preferences: Vec::new(),
        max_assignments: 256,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn characterization() -> (
    canon::geo::GeoObserverCharacterizationArtifact,
    canon::geo::GeoObserverContract,
    GeoRhoContract,
) {
    let artifact = characterize_count_observer(
        &placeholder_contract(),
        &[
            count_row("s1", 6),
            count_row("s2", 2),
            count_row("s3", 3),
            count_row("s4", 1),
            count_row("s5", 4),
            count_row("s9", 9),
        ],
        &population(),
        &reference_counts(),
    )
    .expect("count observer characterization succeeds");
    let characterization_blake3 = hex(&canonical_observer_characterization_bytes(&artifact)
        .expect("characterization canonicalizes"));
    let contract = contract_with_characterization(&characterization_blake3);
    let rho = frozen_count_rho_contract(
        "fixture.nyc_ortho.2024",
        "2024",
        "population.nyc.count.fixture",
        characterization_blake3,
    );
    (artifact, contract, rho)
}

#[test]
fn t34_count_observer_characterizes_named_population_and_excludes_outside_rows() {
    let (artifact, _, _) = characterization();
    let aggregate = artifact
        .per_kind
        .get("structure_count_in_window")
        .expect("structure-count aggregate");
    assert_eq!(artifact.rows_total, 6);
    assert_eq!(aggregate.compared, 5);
    assert_eq!(aggregate.exact_agreement, 3);
    assert_eq!(aggregate.max_abs_error, 1);
    assert_eq!(aggregate.error_band.lower_slack, 1);
    assert_eq!(aggregate.error_band.upper_slack, 1);
    assert!(artifact.missing_subject_ids.is_empty());
    assert_eq!(artifact.rows_outside_population, ["obs:count:s9:2024"]);

    let non_round = artifact
        .per_truth_plane
        .get(truth_plane_key(
            GeoTruthPlane::NonRoundAmountDateLegalBorough,
        ))
        .expect("non-round plane characterization");
    assert_eq!(non_round.compared, 2);
    assert_eq!(non_round.exact_agreement, 1);
    assert_eq!(non_round.error_band.upper_slack, 1);

    let mut wrong_population = population();
    wrong_population.population_id = "population.nyc.other.fixture".to_string();
    let error = characterize_count_observer(
        &placeholder_contract(),
        &[count_row("s1", 6)],
        &wrong_population,
        &reference_counts(),
    )
    .expect_err("population mismatch must refuse");
    assert_eq!(
        error.code,
        GeoObserverErrorCode::ObserverErrorUncharacterized
    );
}

#[test]
fn t35_count_band_admits_through_rho_and_changes_residual_only_when_characterized() {
    let (characterization, contract, rho) = characterization();
    let raw_row = count_row("s1", 6);
    let widened = widen_count_observation_row(&raw_row, &characterization)
        .expect("characterization widens the raw count");
    assert_eq!(
        widened.payload,
        GeoObservationPayload::StructureCountInWindow { min: 5, max: 7 }
    );

    let mut narrowed = widened.clone();
    narrowed.payload = GeoObservationPayload::StructureCountInWindow { min: 6, max: 6 };
    let error = validate_count_band_not_narrowed(&narrowed, &characterization)
        .expect_err("hand-narrowed count bands must refuse");
    assert_eq!(
        error.code,
        GeoObserverErrorCode::ObserverBandNotFromCharacterization
    );

    let universe = seven_building_universe();
    let artifact = admit_characterized_count_observations(
        &contract,
        std::slice::from_ref(&widened),
        std::slice::from_ref(&rho),
        &["commercial_basemap_tos".to_string()],
        &universe,
        &characterization,
    )
    .expect("widened count rows admit");
    assert_eq!(artifact.rho_observations.len(), 1);

    let baseline_request = building_request(universe.clone());
    let baseline = solve_composition(&baseline_request).expect("baseline solves");
    assert_eq!(baseline.summary.residual_model_count, 127);

    let compiled = compile_evidence(&GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::building(),
        universe,
        contracts: vec![rho.clone()],
        observations: artifact.rho_observations.clone(),
        max_assignments: 256,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    })
    .expect("flagged count band compiles");
    assert_eq!(compiled.composition_request.hard_constraints.len(), 1);
    assert_eq!(
        compiled.admissions[0].disposition,
        GeoEvidenceDisposition::HardConstraint
    );
    let GeoRhoObservationKind::IntegerSumBand {
        min, max, values, ..
    } = &artifact.rho_observations[0].observation
    else {
        panic!("count row must become an integer-sum band");
    };
    assert_eq!((*min, *max, values.len()), (5, 7, 7));

    let after = solve_composition(&compiled.composition_request).expect("count-band solve");
    assert_eq!(after.summary.residual_model_count, 29);

    let effect = measure_observer_effect(
        GEO_COUNT_OBSERVER_ID,
        contract.characterization_blake3.clone(),
        &[("case.count.fixture".to_string(), baseline, after)],
        &BTreeMap::from([(
            "case.count.fixture".to_string(),
            truth_plane_key(GeoTruthPlane::NonRoundAmountDateLegalBorough).to_string(),
        )]),
    )
    .expect("count observer effect measures narrowing");
    assert_eq!(effect.totals.changed, 1);
    assert_eq!(effect.totals.redundant, 0);

    let mut unflagged = rho;
    if let GeoRhoBasis::EmpiricalCalibration {
        admissible_hard_band,
        ..
    } = &mut unflagged.basis
    {
        *admissible_hard_band = false;
    }
    let diagnostic = compile_evidence(&GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::building(),
        universe: seven_building_universe(),
        contracts: vec![unflagged],
        observations: artifact.rho_observations,
        max_assignments: 256,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    })
    .expect("unflagged empirical count compiles diagnostically");
    assert!(diagnostic.composition_request.hard_constraints.is_empty());
    assert_eq!(
        diagnostic.admissions[0].disposition,
        GeoEvidenceDisposition::DiagnosticOnly
    );
}

#[test]
fn t36_count_observer_replay_uses_retained_bytes_and_effect_refuses_widening() {
    let (characterization, contract, rho) = characterization();
    let raw_row = count_row("s1", 6);
    let widened = widen_count_observation_row(&raw_row, &characterization).expect("row widens");
    let artifact = admit_characterized_count_observations(
        &contract,
        &[widened],
        &[rho],
        &["commercial_basemap_tos".to_string()],
        &seven_building_universe(),
        &characterization,
    )
    .expect("count row admits");
    let row = &artifact.rows[0];
    let tile_bytes = b"fixture tile bytes s1".to_vec();
    let crop_bytes = b"fixture crop bytes s1".to_vec();
    let label_bytes = b"{\"count\":6}".to_vec();
    verify_replay(
        &artifact,
        &BTreeMap::from([
            (row.tile_pins[0].blake3.clone(), tile_bytes.clone()),
            (row.crop_blake3.clone(), crop_bytes.clone()),
            (row.label_blake3.clone(), label_bytes.clone()),
        ]),
    )
    .expect("retained bytes replay");

    let mut bad_label_bytes = label_bytes;
    bad_label_bytes[0] = b'[';
    let error = verify_replay(
        &artifact,
        &BTreeMap::from([
            (row.tile_pins[0].blake3.clone(), tile_bytes),
            (row.crop_blake3.clone(), crop_bytes),
            (row.label_blake3.clone(), bad_label_bytes),
        ]),
    )
    .expect_err("changed label bytes must not replay");
    assert_eq!(error.code, GeoObserverErrorCode::ImageTileDigestMismatch);

    let small = solve_composition(&building_request(GeoCompositionUniverse {
        parcels: Vec::new(),
        buildings: vec![GeoBuildingCandidate {
            id: "building.1".to_string(),
            parcel_ids: Vec::new(),
        }],
    }))
    .expect("small solve");
    let large =
        solve_composition(&building_request(seven_building_universe())).expect("large solve");
    let error = measure_observer_effect(
        GEO_COUNT_OBSERVER_ID,
        contract.characterization_blake3,
        &[("case.widened".to_string(), small, large)],
        &BTreeMap::new(),
    )
    .expect_err("observer effect must not widen residuals");
    assert_eq!(error.code, GeoObserverErrorCode::ObserverEffectWidened);
}

#[test]
fn t37_count_observer_has_no_runtime_invocation_or_hosted_model_literals() {
    let source_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/geo/observer_count.rs");
    let source = fs::read_to_string(source_path).expect("count observer source reads");
    assert!(
        forbidden_count_observer_literal(&source).is_none(),
        "src/geo/observer_count.rs must not invoke acquisition runtimes"
    );

    let seeded = format!("{source}\nfn bad() {{ std::process::Command::new(\"runner\"); }}\n");
    assert_eq!(
        forbidden_count_observer_literal(&seeded),
        Some("std::process::command")
    );
    assert!(!source.to_ascii_lowercase().contains("case_4"));
    assert!(!source.to_ascii_lowercase().contains("franklin"));
}

fn forbidden_count_observer_literal(source: &str) -> Option<&'static str> {
    let folded = source.to_ascii_lowercase();
    [
        "std::process::command",
        "command::new",
        "reqwest",
        "hyper::",
        "onnx",
        "ort::",
        "tract::",
        "tract_onnx",
        "openai",
        "anthropic",
        "gemini",
        "bedrock",
        "invoke_model",
    ]
    .into_iter()
    .find(|literal| folded.contains(literal))
}
