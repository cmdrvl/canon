#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, CANON_GEO_EVIDENCE_REQUEST_VERSION,
    DEFAULT_MAX_MATERIALIZED_MODELS, GeoBuildingCandidate, GeoCompositionModel,
    GeoCompositionProfile, GeoCompositionRequest, GeoCompositionUniverse, GeoEntityLevel,
    GeoEntityRef, GeoEvidenceClaimRole, GeoEvidenceCompilationRequest, GeoEvidenceRecordRef,
    GeoHardConstraint, GeoHardConstraintKind, GeoIntegerMeasure, GeoIntegerMemberValue,
    GeoIntegerValueOrigin, GeoPropagationBudget, GeoPropagationErrorCode, GeoPropagatorKind,
    GeoPrunedValue, GeoRhoBasis, GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind,
    apply_prunings, canonical_propagation_bytes, check_soundness, compile_evidence, propagate,
    solve_composition, validate_propagation_artifact,
};
use serde::Deserialize;
use std::collections::BTreeSet;

const T03_SEED: u64 = 0xd2a0_2026_0902_0001;

#[derive(Debug, Deserialize)]
struct WorkedCorpus {
    cases: Vec<WorkedCase>,
}

#[derive(Debug, Deserialize)]
struct WorkedCase {
    case_id: String,
    request: GeoCompositionRequest,
}

#[derive(Clone)]
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 16
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
}

#[test]
fn t01_additive_band_forces_required_member_without_overpruning() {
    let request = additive_force_request();
    let artifact =
        propagate(&request, None, &GeoPropagationBudget::default()).expect("propagation succeeds");

    assert!(artifact.fixpoint_reached);
    assert_eq!(artifact.prunings.len(), 1);
    assert_eq!(
        artifact.prunings[0].propagator,
        GeoPropagatorKind::AdditiveBand
    );
    assert_eq!(artifact.prunings[0].value, GeoPrunedValue::Forced);
    assert_eq!(
        artifact.prunings[0].member,
        GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-c")
    );
    assert_eq!(artifact.prunings[0].constraint_ids, ["band:declared-area"]);
    assert!(artifact.prunings[0].evidence_ids.is_empty());
    assert_model_sets_equal("T01 additive force", &request, &artifact);
}

#[test]
fn t02_cardinality_excludes_building_with_no_available_containing_parcel() {
    let parcels = (1..=7)
        .map(|index| format!("parcel-{index}"))
        .collect::<Vec<_>>();
    let buildings = (1..=7)
        .map(|index| GeoBuildingCandidate {
            id: format!("building-{index}"),
            parcel_ids: vec![format!("parcel-{index}")],
        })
        .collect::<Vec<_>>();
    let request = request(
        parcels,
        buildings,
        vec![
            GeoHardConstraint {
                id: "forbid-parcel-7".to_string(),
                constraint: GeoHardConstraintKind::Forbid {
                    member: GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-7"),
                },
            },
            GeoHardConstraint {
                id: "building-count-band".to_string(),
                constraint: GeoHardConstraintKind::Cardinality {
                    level: GeoEntityLevel::Building,
                    min: 6,
                    max: 6,
                },
            },
        ],
    );

    let artifact =
        propagate(&request, None, &GeoPropagationBudget::default()).expect("propagation succeeds");
    let building_7 = artifact
        .prunings
        .iter()
        .find(|pruning| pruning.member.id == "building-7")
        .expect("unreachable building is excluded");
    assert_eq!(building_7.value, GeoPrunedValue::Excluded);
    assert_eq!(building_7.propagator, GeoPropagatorKind::Cardinality);
    assert_eq!(
        building_7.constraint_ids,
        ["building-count-band", "forbid-parcel-7"]
    );
    assert_eq!(
        artifact
            .prunings
            .iter()
            .filter(|pruning| {
                pruning.member.level == GeoEntityLevel::Building
                    && pruning.value == GeoPrunedValue::Forced
            })
            .count(),
        6,
        "after excluding building-7, the [6,6] count band forces the remaining buildings"
    );
    assert_model_sets_equal("T02 cardinality", &request, &artifact);
}

#[test]
fn t02_source_exclusivity_reasons_name_constraint_and_evidence_ids() {
    let compilation =
        compile_evidence(&single_exact_set_evidence()).expect("evidence compiles to allowed set");
    let artifact = propagate(
        &compilation.composition_request,
        Some(&compilation),
        &GeoPropagationBudget::default(),
    )
    .expect("source exclusivity propagation succeeds");

    assert_eq!(artifact.prunings.len(), 3);
    for pruning in &artifact.prunings {
        assert_eq!(pruning.propagator, GeoPropagatorKind::SourceExclusivity);
        assert_eq!(pruning.constraint_ids, ["rho:contract-exact@v1:obs-exact"]);
        assert_eq!(pruning.evidence_ids, ["obs-exact"]);
    }
    assert_model_sets_equal(
        "T02 source exclusivity",
        &compilation.composition_request,
        &artifact,
    );
}

#[test]
fn t03_propagation_preserves_fixture_and_seeded_random_model_sets() {
    let corpus: WorkedCorpus =
        serde_json::from_str(include_str!("fixtures/geo/e4_worked_cases.json"))
            .expect("worked-case fixture parses");
    let mut nonempty_prune_cases = 0_usize;
    for case in corpus.cases {
        let artifact = propagate(&case.request, None, &GeoPropagationBudget::default())
            .unwrap_or_else(|error| panic!("{} propagation failed: {error}", case.case_id));
        if !artifact.prunings.is_empty() {
            nonempty_prune_cases += 1;
        }
        assert_model_sets_equal(&case.case_id, &case.request, &artifact);
    }

    let mut rng = Lcg::new(T03_SEED);
    for index in 0..200 {
        let request = seeded_request(index, &mut rng);
        let artifact = propagate(&request, None, &GeoPropagationBudget::default())
            .unwrap_or_else(|error| panic!("seeded component {index} failed: {error}"));
        if !artifact.prunings.is_empty() {
            nonempty_prune_cases += 1;
        }
        assert_model_sets_equal(&format!("seeded component {index}"), &request, &artifact);
    }

    assert!(
        nonempty_prune_cases > 0,
        "T03 seed {T03_SEED:#x} must include nonempty sound pruning cases"
    );
}

#[test]
fn t03_unsound_injected_pruning_is_detected_by_model_set_comparison() {
    let request = request(
        vec!["parcel-a".to_string(), "parcel-b".to_string()],
        Vec::new(),
        vec![GeoHardConstraint {
            id: "at-least-one-parcel".to_string(),
            constraint: GeoHardConstraintKind::AnyOf {
                members: vec![
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a"),
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-b"),
                ],
            },
        }],
    );
    let mut artifact =
        propagate(&request, None, &GeoPropagationBudget::default()).expect("baseline artifact");
    artifact.prunings.push(canon::geo::GeoPruning {
        member: GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a"),
        value: GeoPrunedValue::Excluded,
        propagator: GeoPropagatorKind::AdditiveBand,
        constraint_ids: vec!["fixture:unsound-prune".to_string()],
        evidence_ids: Vec::new(),
    });
    artifact.counters.insert(
        "pruning_count".to_string(),
        u64::try_from(artifact.prunings.len()).expect("test pruning count fits"),
    );

    let error = check_soundness(&request, &artifact)
        .expect_err("an injected over-prune must change the exact model set");
    assert_eq!(
        error.code,
        GeoPropagationErrorCode::PropagationUnsoundDetected
    );
    assert_ne!(
        error.detail.get("model_count_before"),
        error.detail.get("model_count_after"),
        "the negative must prove the injected pruning changed the exact residual"
    );
    assert!(
        error.detail.contains_key("member"),
        "soundness failures must name a differing member for downstream explanation"
    );
}

#[test]
fn t04_propagation_bytes_are_identical_on_rerun() {
    let request = additive_force_request();
    let first =
        propagate(&request, None, &GeoPropagationBudget::default()).expect("first propagation");
    let second =
        propagate(&request, None, &GeoPropagationBudget::default()).expect("second propagation");

    assert_eq!(first, second);
    assert_eq!(
        canonical_propagation_bytes(&first).expect("first canonical bytes"),
        canonical_propagation_bytes(&second).expect("second canonical bytes")
    );
}

#[test]
fn t04_validation_rejects_reason_that_names_only_constraint_index() {
    let request = additive_force_request();
    let mut artifact =
        propagate(&request, None, &GeoPropagationBudget::default()).expect("baseline artifact");
    artifact.prunings[0].constraint_ids = vec!["0".to_string()];

    let error = validate_propagation_artifact(&artifact).expect_err("index-only reason must fail");
    assert_eq!(error.code, GeoPropagationErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("constraint_ids")
    );
}

#[test]
fn t19_budget_fallback_is_typed_and_retained_prunings_stay_sound() {
    let request = additive_force_request();
    let artifact = propagate(
        &request,
        None,
        &GeoPropagationBudget {
            max_fixpoint_rounds: 1,
            ..GeoPropagationBudget::default()
        },
    )
    .expect("budgeted propagation keeps justified round-one prunings");

    assert!(!artifact.fixpoint_reached);
    assert_eq!(artifact.prunings.len(), 1);
    let fallback = artifact
        .budget_fallback
        .as_ref()
        .expect("non-fixpoint artifact names the fallback");
    assert_eq!(fallback.propagator, GeoPropagatorKind::AdditiveBand);
    assert_eq!(fallback.counter, "max_fixpoint_rounds");
    assert_eq!(fallback.configured, 1);
    assert_eq!(
        artifact
            .counters
            .get("fallback.max_fixpoint_rounds")
            .copied(),
        Some(1)
    );
    assert_model_sets_equal("T19 retained fallback prunings", &request, &artifact);
}

#[test]
fn t19_uninformative_constraints_produce_empty_fixpoint_not_wrong_pruning() {
    let request = request(
        vec!["parcel-a".to_string(), "parcel-b".to_string()],
        Vec::new(),
        vec![GeoHardConstraint {
            id: "wide-count-band".to_string(),
            constraint: GeoHardConstraintKind::Cardinality {
                level: GeoEntityLevel::Parcel,
                min: 1,
                max: 2,
            },
        }],
    );

    let artifact =
        propagate(&request, None, &GeoPropagationBudget::default()).expect("propagation succeeds");
    assert!(artifact.fixpoint_reached);
    assert!(artifact.prunings.is_empty());
    assert_model_sets_equal("T19 uninformative", &request, &artifact);
}

#[test]
fn t19_zero_budget_is_rejected_before_any_partial_artifact() {
    let error = propagate(
        &additive_force_request(),
        None,
        &GeoPropagationBudget {
            max_fixpoint_rounds: 0,
            ..GeoPropagationBudget::default()
        },
    )
    .expect_err("zero fixpoint budget is invalid");
    assert_eq!(error.code, GeoPropagationErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("max_fixpoint_rounds")
    );
}

#[test]
fn t27_propagator_module_has_no_demo_or_cohort_literals_and_scan_is_case_insensitive() {
    let source = std::fs::read_to_string("src/geo/propagate.rs").expect("propagate source");
    let lower = source.to_ascii_lowercase();
    for forbidden in [
        "franklin",
        "case_4",
        "d1_residual",
        "h7_population",
        "cmbs",
        "mappluto",
    ] {
        assert!(
            !lower.contains(forbidden),
            "propagator must remain source-generic; found {forbidden}"
        );
    }

    let bad_example = "Franklin CASE_4 D1_Residual";
    let lower_bad = bad_example.to_ascii_lowercase();
    assert!(lower_bad.contains("franklin"));
    assert!(lower_bad.contains("case_4"));
    assert!(lower_bad.contains("d1_residual"));
}

fn additive_force_request() -> GeoCompositionRequest {
    request(
        vec![
            "parcel-a".to_string(),
            "parcel-b".to_string(),
            "parcel-c".to_string(),
        ],
        Vec::new(),
        vec![GeoHardConstraint {
            id: "band:declared-area".to_string(),
            constraint: GeoHardConstraintKind::IntegerSumBand {
                level: GeoEntityLevel::Parcel,
                measure: integer_measure(),
                values: vec![
                    GeoIntegerMemberValue {
                        id: "parcel-a".to_string(),
                        value: 100,
                    },
                    GeoIntegerMemberValue {
                        id: "parcel-b".to_string(),
                        value: 200,
                    },
                    GeoIntegerMemberValue {
                        id: "parcel-c".to_string(),
                        value: 5_000,
                    },
                ],
                min: 5_000,
                max: 5_200,
            },
        }],
    )
}

fn single_exact_set_evidence() -> GeoEvidenceCompilationRequest {
    GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        universe: GeoCompositionUniverse {
            parcels: vec![
                "parcel-a".to_string(),
                "parcel-b".to_string(),
                "parcel-c".to_string(),
            ],
            buildings: Vec::new(),
        },
        contracts: vec![GeoRhoContract {
            id: "contract-exact".to_string(),
            version: "v1".to_string(),
            source_dataset: "fixture:source-exclusivity".to_string(),
            source_release: "fixture-release".to_string(),
            source_lineage_ids: vec!["fixture:lineage".to_string()],
            method_id: "fixture:exact-set-method".to_string(),
            method_version: "v1".to_string(),
            claim_role: GeoEvidenceClaimRole::AttributeObservation,
            basis: GeoRhoBasis::LogicalRelaxation {
                invariant_id: "fixture:exact-set-invariant".to_string(),
            },
        }],
        observations: vec![GeoRhoObservation {
            id: "obs-exact".to_string(),
            contract_id: "contract-exact".to_string(),
            source_records: vec![GeoEvidenceRecordRef {
                source_record_id: "source-record-1".to_string(),
                source_vintage: "fixture-release".to_string(),
                record_blake3: blake3::hash(b"source-record-1").to_hex().to_string(),
            }],
            valid_time: None,
            observation: GeoRhoObservationKind::ExactSets {
                level: GeoEntityLevel::Parcel,
                sets: vec![vec!["parcel-a".to_string()]],
            },
        }],
        max_assignments: 128,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn seeded_request(index: usize, rng: &mut Lcg) -> GeoCompositionRequest {
    let parcel_count = 3 + usize::try_from(rng.below(4)).expect("small count fits usize");
    let building_count = 1 + usize::try_from(rng.below(4)).expect("small count fits usize");
    let parcels = (0..parcel_count)
        .map(|parcel| format!("parcel-{index}-{parcel}"))
        .collect::<Vec<_>>();
    let buildings = (0..building_count)
        .map(|building| GeoBuildingCandidate {
            id: format!("building-{index}-{building}"),
            parcel_ids: vec![
                parcels
                    [usize::try_from(rng.below(parcel_count as u64)).expect("parcel index fits")]
                .clone(),
            ],
        })
        .collect::<Vec<_>>();
    let max = 10 + rng.below(2);
    let values = parcels
        .iter()
        .enumerate()
        .map(|(parcel_index, id)| GeoIntegerMemberValue {
            id: id.clone(),
            value: if parcel_index == 0 {
                10
            } else {
                1 + rng.below(2)
            },
        })
        .collect::<Vec<_>>();
    let mut allowed_sets = vec![vec![parcels[0].clone()]];
    if max > 10 {
        allowed_sets.push(vec![parcels[0].clone(), parcels[1].clone()]);
    }
    request(
        parcels.clone(),
        buildings,
        vec![
            GeoHardConstraint {
                id: format!("band:seeded:{index}"),
                constraint: GeoHardConstraintKind::IntegerSumBand {
                    level: GeoEntityLevel::Parcel,
                    measure: integer_measure(),
                    values,
                    min: 10,
                    max,
                },
            },
            GeoHardConstraint {
                id: format!("source-exclusive:seeded:{index}"),
                constraint: GeoHardConstraintKind::AllowedSets {
                    level: GeoEntityLevel::Parcel,
                    sets: allowed_sets,
                },
            },
            GeoHardConstraint {
                id: format!("parcel-count:seeded:{index}"),
                constraint: GeoHardConstraintKind::Cardinality {
                    level: GeoEntityLevel::Parcel,
                    min: 1,
                    max: parcel_count,
                },
            },
        ],
    )
}

fn request(
    parcels: Vec<String>,
    buildings: Vec<GeoBuildingCandidate>,
    hard_constraints: Vec<GeoHardConstraint>,
) -> GeoCompositionRequest {
    GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        universe: GeoCompositionUniverse { parcels, buildings },
        hard_constraints,
        soft_preferences: Vec::new(),
        max_assignments: 65_536,
        max_materialized_models: 1_000_000,
    }
}

fn integer_measure() -> GeoIntegerMeasure {
    GeoIntegerMeasure {
        semantic_id: "fixture:area".to_string(),
        unit: "square_foot".to_string(),
        value_origin: GeoIntegerValueOrigin::SourceAsserted,
    }
}

fn assert_model_sets_equal(
    label: &str,
    request: &GeoCompositionRequest,
    artifact: &canon::geo::GeoPropagationArtifact,
) {
    let before =
        solve_composition(request).unwrap_or_else(|error| panic!("{label} solve: {error}"));
    let narrowed =
        apply_prunings(request, artifact).unwrap_or_else(|error| panic!("{label} apply: {error}"));
    let after = solve_composition(&narrowed)
        .unwrap_or_else(|error| panic!("{label} narrowed solve: {error}"));
    assert!(
        before.summary.residual_models_materialized,
        "{label} original residual must be materialized for black-box set comparison"
    );
    assert!(
        after.summary.residual_models_materialized,
        "{label} narrowed residual must be materialized for black-box set comparison"
    );
    assert_eq!(
        before.summary.residual_model_count, after.summary.residual_model_count,
        "{label} residual counts changed"
    );
    assert_eq!(
        model_set(&before.residual_models),
        model_set(&after.residual_models),
        "{label}"
    );
}

fn model_set(models: &[GeoCompositionModel]) -> BTreeSet<GeoCompositionModel> {
    models.iter().cloned().collect()
}
