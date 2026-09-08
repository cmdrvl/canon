#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_POPULATION_REQUEST_VERSION,
    DEFAULT_MAX_MATERIALIZED_MODELS, GEO_ACRIS_DOCUMENT_LEGAL_COMPLETENESS_CONTRACT_ID,
    GeoCollateralCompletenessAssertion, GeoCollateralCompletenessCaseAssertion,
    GeoCollateralCompletenessContractSource, GeoCollateralCompletenessOverlayRequest,
    GeoCompositionModel, GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef,
    GeoEvidenceCompilationRequest, GeoEvidenceRecordRef, GeoH7PopulationArtifact,
    GeoH7SourceRecordRole, GeoLabeledCompositionCase, GeoPopulationCaseStatus,
    GeoPopulationEvaluationRequest, GeoPopulationEvidenceStackRequest, GeoRhoObservation,
    GeoRhoObservationKind, GeoTruthPlane, build_collateral_completeness_overlay,
    evaluate_population, stack_population_evidence,
};
use flate2::{Compression, GzBuilder, read::GzDecoder};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    fs::File,
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
};

const FULL_REACH_POPULATION: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/population_request_roll_universe_pluto_vintage_condo_representation_widened.json.gz";
const H7_POPULATION: &str =
    "scripts/geo_measurements/fixtures/d1_residuals/canon_geo_h7_population.v0.json";
const SOURCE_HUNT: &str =
    "scripts/geo_measurements/fixtures/e4_completeness_source_hunt_2026-09-08/source_hunt.json";
const OUT_DIR: &str = "scripts/geo_measurements/fixtures/e4_collateral_completeness_2026-09-08";
const ACRIS_COMPLETENESS_OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_collateral_completeness_2026-09-08/overlay_request_acris_document_legal_completeness.json.gz";
const ACRIS_COMPLETENESS_MEASUREMENT: &str =
    "scripts/geo_measurements/fixtures/e4_collateral_completeness_2026-09-08/measurement.json";

const PROBE_CASE_FRAGMENTS: [&str; 3] = ["13ff4751", "3991a574", "f5588ba9"];

#[test]
fn acris_completeness_handoff_is_definitional_and_ready_to_score() {
    let expected = build_acris_completeness_overlay();
    let actual: GeoPopulationEvidenceStackRequest = read_json_gz(ACRIS_COMPLETENESS_OVERLAY);
    assert_eq!(
        actual, expected,
        "committed ACRIS completeness overlay must replay from retained H7 source pins"
    );

    let measurement: Value = read_json(ACRIS_COMPLETENESS_MEASUREMENT);
    assert_eq!(
        measurement["version"],
        "canon_geo_e4_collateral_completeness_handoff.v0"
    );
    assert_eq!(measurement["bead"], "bd-2gvl");
    assert_eq!(
        measurement["proof_class"],
        "retained_definitional_on_this_population_not_independent_precision"
    );
    assert!(
        measurement["proof_boundary"]
            .as_str()
            .expect("proof boundary")
            .contains("must not be cited as independent precision")
    );
    assert_eq!(measurement["release_claim_allowed"], false);
    assert_eq!(measurement["frozen_denominator"], 77);
    assert_eq!(measurement["reported_frozen_denominator"], 79);
    assert_eq!(measurement["retained_population_deficit"], 7);
    assert_eq!(measurement["retained_population_denominator"], 70);
    assert_eq!(measurement["source_hunt_path"], SOURCE_HUNT);
    assert_eq!(
        measurement["score_handoff"]["population_path"],
        FULL_REACH_POPULATION
    );
    assert_eq!(
        measurement["score_handoff"]["acris_completeness_overlay_path"],
        ACRIS_COMPLETENESS_OVERLAY
    );
    assert_eq!(
        measurement["score_handoff"]["status"],
        "READY_TO_SCORE_by_bd_1g4x_definitional_mechanism_check_not_independent_precision"
    );
    assert_eq!(measurement["overlay_summary"]["overlay_cases"], 70);
    assert_eq!(measurement["overlay_summary"]["hard_observations"], 140);
    assert_eq!(
        measurement["score_handoff"]["overlay_sha256"],
        sha256_file(repo_path(ACRIS_COMPLETENESS_OVERLAY))
    );
    assert_eq!(
        measurement["score_handoff"]["overlay_uncompressed_sha256"],
        sha256_bytes(&read_gzip_uncompressed_bytes(repo_path(
            ACRIS_COMPLETENESS_OVERLAY
        )))
    );
}

#[test]
fn acris_completeness_overlay_stacks_without_rewriting_reach_or_truth() {
    let population = read_json_gz::<GeoPopulationEvaluationRequest>(FULL_REACH_POPULATION);
    let overlay = read_json_gz::<GeoPopulationEvidenceStackRequest>(ACRIS_COMPLETENESS_OVERLAY);
    let stacked = stack_population_evidence(&population, &overlay)
        .expect("ACRIS completeness overlay stacks");

    assert_eq!(
        universe_by_case(&stacked.base_population),
        universe_by_case(&stacked.population),
        "completeness scoring must not add candidates"
    );
    assert_eq!(
        truth_by_case(&stacked.base_population),
        truth_by_case(&stacked.population),
        "completeness scoring must not rewrite truth labels"
    );
    assert_eq!(stacked.summary.overlay_cases, 70);
    assert_eq!(stacked.summary.changed_cases, 70);
    assert_eq!(stacked.summary.added_contracts, 70);
    assert_eq!(stacked.summary.added_observations, 140);
    assert_eq!(stacked.summary.hard_constraint_observations, 140);
    assert_eq!(stacked.summary.diagnostic_observations, 0);
    assert_eq!(stacked.summary.soft_preference_observations, 0);
}

#[test]
fn acris_completeness_probe_cases_emit_truth_all_of_and_exact_cardinality() {
    let population = read_json_gz::<GeoPopulationEvaluationRequest>(FULL_REACH_POPULATION);
    let overlay = read_json_gz::<GeoPopulationEvidenceStackRequest>(ACRIS_COMPLETENESS_OVERLAY);
    let population_by_fragment = population_by_fragment(&population);
    let overlay_by_case = overlay_by_case(&overlay);

    for fragment in PROBE_CASE_FRAGMENTS {
        let case = population_by_fragment
            .get(fragment)
            .unwrap_or_else(|| panic!("{fragment} population case"));
        let overlay_case = overlay_by_case
            .get(&case.id)
            .unwrap_or_else(|| panic!("{fragment} overlay case"));
        assert_eq!(overlay_case.contracts.len(), 1);
        assert_eq!(
            overlay_case.contracts[0].id,
            GEO_ACRIS_DOCUMENT_LEGAL_COMPLETENESS_CONTRACT_ID
        );
        assert_eq!(overlay_case.observations.len(), 2);

        let all_of = all_of_observation(&overlay_case.observations);
        let exact_cardinality = exact_cardinality_observation(&overlay_case.observations);
        let truth = case.truth.parcels.iter().cloned().collect::<BTreeSet<_>>();
        assert_eq!(all_of, truth, "{fragment} AllOf members must equal truth");
        assert_eq!(
            exact_cardinality,
            case.truth.parcels.len(),
            "{fragment} exact cardinality must equal truth lot count"
        );
        for observation in &overlay_case.observations {
            assert!(
                observation.source_records.iter().all(|record| {
                    record.source_record_id.contains("STG_GEO_NYC_ACRIS_LEGALS")
                        && is_lower_blake3(&record.record_blake3)
                }),
                "{fragment} completeness observations must retain ACRIS legal row pins"
            );
        }
    }
}

#[test]
fn partial_or_unasserted_completeness_source_emits_no_stack_request() {
    let source_record = fixture_source_record("fixture:complete-source:row");
    let request = GeoCollateralCompletenessOverlayRequest::acris_document_legal(
        acris_contract_source(),
        vec![
            GeoCollateralCompletenessCaseAssertion {
                case_id: "case-partial".to_string(),
                level: GeoEntityLevel::Parcel,
                members: vec![GeoEntityRef::new(GeoEntityLevel::Parcel, "p1")],
                source_records: vec![source_record.clone()],
                assertion: GeoCollateralCompletenessAssertion::Partial,
            },
            GeoCollateralCompletenessCaseAssertion {
                case_id: "case-unasserted".to_string(),
                level: GeoEntityLevel::Parcel,
                members: vec![GeoEntityRef::new(GeoEntityLevel::Parcel, "p2")],
                source_records: vec![source_record],
                assertion: GeoCollateralCompletenessAssertion::Unasserted,
            },
        ],
        2,
        4,
    );
    let artifact = build_collateral_completeness_overlay(&request)
        .expect("partial and unasserted completeness requests abstain safely");
    assert!(artifact.stack_request.is_none());
    assert_eq!(artifact.summary.requested_cases, 2);
    assert_eq!(artifact.summary.asserted_complete_cases, 0);
    assert_eq!(artifact.summary.abstained_cases, 2);
    assert_eq!(artifact.summary.emitted_observations, 0);
}

#[test]
fn partial_or_unasserted_completeness_claims_do_not_narrow_the_residual() {
    let population = partial_completeness_negative_population();
    let baseline = evaluate_population(&population).expect("baseline population evaluates");
    let baseline_case = baseline
        .cases
        .first()
        .expect("baseline has one scored case");
    assert_eq!(baseline_case.status, GeoPopulationCaseStatus::Ambiguous);
    assert_eq!(baseline_case.residual_model_count, Some(7));
    assert_eq!(baseline_case.hard_constraint_observations, 0);

    for assertion in [
        GeoCollateralCompletenessAssertion::Partial,
        GeoCollateralCompletenessAssertion::Unasserted,
    ] {
        let request = GeoCollateralCompletenessOverlayRequest::acris_document_legal(
            acris_contract_source(),
            vec![GeoCollateralCompletenessCaseAssertion {
                case_id: "case-completeness-negative".to_string(),
                level: GeoEntityLevel::Parcel,
                members: vec![
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "p1"),
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "p2"),
                ],
                source_records: vec![fixture_source_record(
                    "fixture:partial-completeness-negative:row",
                )],
                assertion,
            }],
            1,
            2,
        );
        let artifact = build_collateral_completeness_overlay(&request)
            .expect("partial and unasserted completeness requests abstain safely");
        let scored_population = if let Some(stack_request) = artifact.stack_request.as_ref() {
            stack_population_evidence(&population, stack_request)
                .expect("emitted partial/unasserted overlay stacks")
                .population
        } else {
            population.clone()
        };
        let scored = evaluate_population(&scored_population)
            .expect("partial/unasserted completeness population evaluates");
        let scored_case = scored.cases.first().expect("scored case exists");

        assert_eq!(scored_case.status, baseline_case.status, "{assertion:?}");
        assert_eq!(
            scored_case.residual_model_count, baseline_case.residual_model_count,
            "{assertion:?} must not collapse feasible subsets"
        );
        assert_eq!(
            scored_case.hard_constraint_observations, baseline_case.hard_constraint_observations,
            "{assertion:?} must not emit pruning evidence"
        );
        assert!(
            scored_case.truth_model_in_residual.unwrap_or(false),
            "{assertion:?} must leave the complete truth model feasible"
        );
    }
}

#[test]
#[ignore = "writes committed bd-2gvl measurement artifacts when CANON_GEO_COMPLETENESS_WRITE=1"]
fn rewrite_acris_completeness_handoff_artifacts() {
    if env::var("CANON_GEO_COMPLETENESS_WRITE").as_deref() != Ok("1") {
        eprintln!("set CANON_GEO_COMPLETENESS_WRITE=1 to rewrite the fixture artifacts");
        return;
    }

    let overlay = build_acris_completeness_overlay();
    fs::create_dir_all(repo_path(OUT_DIR)).expect("measurement output dir can be created");
    write_json_gz(repo_path(ACRIS_COMPLETENESS_OVERLAY), &overlay);
    write_pretty_json(
        repo_path(ACRIS_COMPLETENESS_MEASUREMENT),
        &measurement_artifact(&overlay),
    );
}

fn build_acris_completeness_overlay() -> GeoPopulationEvidenceStackRequest {
    let population = read_json_gz::<GeoPopulationEvaluationRequest>(FULL_REACH_POPULATION);
    let h7 = read_json::<GeoH7PopulationArtifact>(H7_POPULATION);
    let h7_cases = retained_h7_cases_by_subject(&h7, &population);
    let assertions = population
        .cases
        .iter()
        .map(|case| {
            let h7_case = h7_cases
                .get(&case.id)
                .unwrap_or_else(|| panic!("{} retained H7 source record case", case.id));
            let legal_records = h7_case
                .source_records
                .iter()
                .filter(|record| record.role == GeoH7SourceRecordRole::AcrisLegal)
                .cloned()
                .collect::<Vec<_>>();
            assert!(
                !legal_records.is_empty(),
                "{} must carry ACRIS legal source records",
                case.id
            );
            let legal_parcels = legal_records
                .iter()
                .flat_map(|record| record.parcel_ids.iter().cloned())
                .collect::<BTreeSet<_>>();
            let truth = case.truth.parcels.iter().cloned().collect::<BTreeSet<_>>();
            let universe = case
                .evidence
                .universe
                .parcels
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            assert_eq!(
                legal_parcels, truth,
                "{} retained ACRIS legal rows are the E4 truth set",
                case.id
            );
            assert!(
                truth.is_subset(&universe),
                "{} full-reach population must contain every truth parcel before completeness",
                case.id
            );
            GeoCollateralCompletenessCaseAssertion {
                case_id: case.id.clone(),
                level: GeoEntityLevel::Parcel,
                members: legal_parcels
                    .into_iter()
                    .map(|parcel| GeoEntityRef::new(GeoEntityLevel::Parcel, parcel))
                    .collect(),
                source_records: legal_records
                    .into_iter()
                    .map(|record| record.source_record)
                    .collect(),
                assertion: GeoCollateralCompletenessAssertion::AssertedComplete,
            }
        })
        .collect::<Vec<_>>();
    let request = GeoCollateralCompletenessOverlayRequest::acris_document_legal(
        acris_contract_source(),
        assertions,
        population.cases.len(),
        population.cases.len() * 2,
    );
    let artifact =
        build_collateral_completeness_overlay(&request).expect("ACRIS completeness overlay builds");
    assert_eq!(artifact.summary.requested_cases, 70);
    assert_eq!(artifact.summary.asserted_complete_cases, 70);
    assert_eq!(artifact.summary.abstained_cases, 0);
    assert_eq!(artifact.summary.emitted_observations, 140);
    artifact
        .stack_request
        .expect("asserted complete cases emit a stack request")
}

fn measurement_artifact(overlay: &GeoPopulationEvidenceStackRequest) -> Value {
    let overlay_bytes = serde_json::to_vec(overlay).expect("overlay serializes");
    json!({
        "version": "canon_geo_e4_collateral_completeness_handoff.v0",
        "bead": "bd-2gvl",
        "proof_class": "retained_definitional_on_this_population_not_independent_precision",
        "release_claim_allowed": false,
        "frozen_denominator": 77,
        "reported_frozen_denominator": 79,
        "retained_population_deficit": 7,
        "denominator_ruling": {
            "bead": "bd-1g4x",
            "commit": "a0025b3",
            "issued_by": "Zac",
            "previous_frozen_denominator": 79,
            "restated_frozen_denominator": 77,
            "evaluated_subjects": 70,
            "previous_subject_deficit": 9,
            "restated_subject_deficit": 7,
            "excluded_case_fragments": [
                "6eb465fd908bc59f",
                "aacff3c254d36ae6"
            ],
            "kept_case_fragments": [
                "ced7ad9f0d74abf7"
            ],
            "reason": "H4 extension duplicates with no H7 loan key are excluded under PLAN_CANON_GEO.md H.7; the distinct-loan ced7ad9f0d74abf7 case remains in the denominator pending an identity bridge."
        },
        "retained_population_denominator": 70,
        "source_hunt_path": SOURCE_HUNT,
        "proof_boundary": "ACRIS document legal rows assert collateral-set completeness and are production-relevant evidence, but this E4 truth plane is derived from the same ACRIS legals joined on matched mortgage DOCUMENT_ID. The score is a mechanism/replay check and must not be cited as independent precision.",
        "source_instance": {
            "source_dataset": "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_LEGALS",
            "source_release": "2026-08-10",
            "completeness_grain": "bound_mortgage_document_legal_rows",
            "typed_relation": "all_of_exact_cardinality",
            "preconditions": [
                "the ACRIS document is bound as the collateral-defining mortgage or consolidation instrument",
                "the adapter reads the complete nonblank legal row set for the document_id and release",
                "partial-lot, air-rights, subterranean-rights, and easement flags are handled by profile rules or cause abstention"
            ]
        },
        "baseline": {
            "reach_full_partial_none": [70, 0, 0],
            "resolved": 7,
            "exactly_correct": 7,
            "false_merges": 0,
            "truth_exclusions": 15
        },
        "overlay_summary": {
            "overlay_cases": overlay.case_overlays.len(),
            "hard_observations": overlay.case_overlays.iter().map(|case| case.observations.len()).sum::<usize>(),
            "contracts_per_case": 1,
            "observations_per_case": 2
        },
        "score_handoff": {
            "status": "READY_TO_SCORE_by_bd_1g4x_definitional_mechanism_check_not_independent_precision",
            "population_path": FULL_REACH_POPULATION,
            "acris_completeness_overlay_path": ACRIS_COMPLETENESS_OVERLAY,
            "overlay_sha256": sha256_file(repo_path(ACRIS_COMPLETENESS_OVERLAY)),
            "overlay_uncompressed_sha256": sha256_bytes(&overlay_bytes),
            "candidate_universe_expected_unchanged": true,
            "admission_only": true,
            "precision_claim_allowed": false
        }
    })
}

fn retained_h7_cases_by_subject<'a>(
    h7: &'a GeoH7PopulationArtifact,
    population: &GeoPopulationEvaluationRequest,
) -> BTreeMap<String, &'a canon::geo::GeoH7PopulationCaseArtifact> {
    let requested = population
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut cases = BTreeMap::new();
    for case in &h7.cases {
        if requested.contains(case.subject_id.as_str()) {
            cases.entry(case.subject_id.clone()).or_insert(case);
        }
    }
    assert_eq!(cases.len(), population.cases.len());
    cases
}

fn acris_contract_source() -> GeoCollateralCompletenessContractSource {
    GeoCollateralCompletenessContractSource {
        source_dataset: "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_LEGALS".to_string(),
        source_release: "2026-08-10".to_string(),
        source_lineage_ids: vec![
            "EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT:release_dt=2026-08-10".to_string(),
        ],
    }
}

fn partial_completeness_negative_population() -> GeoPopulationEvaluationRequest {
    GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "case-completeness-negative".to_string(),
            evidence: GeoEvidenceCompilationRequest {
                version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
                profile: Default::default(),
                universe: GeoCompositionUniverse {
                    parcels: parcel_ids(&["p1", "p2", "p3"]),
                    buildings: Vec::new(),
                },
                contracts: Vec::new(),
                observations: Vec::new(),
                max_assignments: 64,
                max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
            },
            truth_plane: GeoTruthPlane::GateV2Historical,
            truth: GeoCompositionModel {
                parcels: parcel_ids(&["p1", "p2", "p3"]),
                buildings: Vec::new(),
            },
        }],
        max_cases: 1,
    }
}

fn parcel_ids(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_string()).collect()
}

fn fixture_source_record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "fixture-v1".to_string(),
        record_blake3: blake3::hash(id.as_bytes()).to_hex().to_string(),
    }
}

fn universe_by_case(population: &GeoPopulationEvaluationRequest) -> BTreeMap<String, Vec<String>> {
    population
        .cases
        .iter()
        .map(|case| (case.id.clone(), case.evidence.universe.parcels.clone()))
        .collect()
}

fn truth_by_case(
    population: &GeoPopulationEvaluationRequest,
) -> BTreeMap<String, GeoCompositionModel> {
    population
        .cases
        .iter()
        .map(|case| (case.id.clone(), case.truth.clone()))
        .collect()
}

fn population_by_fragment(
    population: &GeoPopulationEvaluationRequest,
) -> BTreeMap<&'static str, &canon::geo::GeoLabeledCompositionCase> {
    let mut cases = BTreeMap::new();
    for fragment in PROBE_CASE_FRAGMENTS {
        let matches = population
            .cases
            .iter()
            .filter(|case| case.id.contains(fragment))
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "{fragment} must match one population case"
        );
        cases.insert(fragment, matches[0]);
    }
    cases
}

fn overlay_by_case(
    overlay: &GeoPopulationEvidenceStackRequest,
) -> BTreeMap<String, &canon::geo::GeoPopulationCaseEvidenceOverlay> {
    overlay
        .case_overlays
        .iter()
        .map(|case| (case.case_id.clone(), case))
        .collect()
}

fn all_of_observation(observations: &[GeoRhoObservation]) -> BTreeSet<String> {
    observations
        .iter()
        .find_map(|observation| {
            let GeoRhoObservationKind::AllOf { members } = &observation.observation else {
                return None;
            };
            Some(members.iter().map(|member| member.id.clone()).collect())
        })
        .expect("AllOf observation")
}

fn exact_cardinality_observation(observations: &[GeoRhoObservation]) -> usize {
    observations
        .iter()
        .find_map(|observation| {
            let GeoRhoObservationKind::ExactCardinality { count, .. } = observation.observation
            else {
                return None;
            };
            Some(count)
        })
        .expect("ExactCardinality observation")
}

fn is_lower_blake3(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn read_json_gz<T: DeserializeOwned>(relative: &str) -> T {
    let file = File::open(repo_path(relative)).expect("open gzipped fixture");
    serde_json::from_reader(GzDecoder::new(BufReader::new(file))).expect("parse gzipped fixture")
}

fn read_json<T: DeserializeOwned>(relative: &str) -> T {
    let file = File::open(repo_path(relative)).expect("open JSON fixture");
    serde_json::from_reader(BufReader::new(file)).expect("parse JSON fixture")
}

fn read_gzip_uncompressed_bytes(path: impl AsRef<Path>) -> Vec<u8> {
    let file = File::open(path).expect("open gzipped fixture");
    let mut decoder = GzDecoder::new(BufReader::new(file));
    let mut bytes = Vec::new();
    decoder
        .read_to_end(&mut bytes)
        .expect("read gzipped fixture");
    bytes
}

fn write_json_gz(path: impl AsRef<Path>, value: &GeoPopulationEvidenceStackRequest) {
    let file = File::create(path).expect("create gzipped fixture");
    let mut encoder = GzBuilder::new()
        .mtime(0)
        .write(file, Compression::default());
    serde_json::to_writer(&mut encoder, value).expect("write gzipped JSON");
    encoder.finish().expect("finish gzip writer");
}

fn write_pretty_json(path: impl AsRef<Path>, value: &Value) {
    let mut file = File::create(path).expect("create JSON fixture");
    serde_json::to_writer_pretty(&mut file, value).expect("write pretty JSON");
    file.write_all(b"\n").expect("write trailing newline");
}

fn sha256_file(path: impl AsRef<Path>) -> String {
    let mut file = File::open(path).expect("open fixture for sha256");
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer).expect("read fixture for sha256");
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    format!("{:x}", hasher.finalize())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
