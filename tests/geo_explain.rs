#![forbid(unsafe_code)]

use canon::{
    geo::{
        CANON_GEO_COMPOSITION_REQUEST_VERSION, CANON_GEO_EVIDENCE_COMPILATION_VERSION,
        CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_EXPLANATION_VERSION,
        CANON_GEO_SEPARATION_REQUEST_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS,
        GEO_EXPLAIN_OUTPUT_ID, GEO_EXPLAIN_STAGE_COMMAND, GeoCompositionProfile,
        GeoCompositionRequest, GeoCompositionStatus, GeoCompositionUniverse, GeoEntityLevel,
        GeoEntityRef, GeoEvidenceClaimRole, GeoEvidenceCompilationArtifact,
        GeoEvidenceCompilationReference, GeoEvidenceCompilationRequest, GeoEvidenceDisposition,
        GeoEvidenceRecordRef, GeoExplanationArtifact, GeoExplanationBudget,
        GeoExplanationErrorCode, GeoHardConstraint, GeoHardConstraintKind,
        GeoObservationSeparation, GeoProjectNodeExecutor, GeoProspectiveObservation,
        GeoProspectiveOutcome, GeoReliabilityOrder, GeoRhoBasis, GeoRhoContract, GeoRhoObservation,
        GeoRhoObservationKind, GeoSeparationRequest, canonical_composition_bytes,
        canonical_evidence_compilation_bytes, canonical_explanation_bytes, compile_evidence,
        correction_sets, minimal_core, separate, solve_composition, validate_explanation_artifact,
        verify_minimal_core_members,
    },
    project::{
        ProjectDependencyOutput, ProjectNodeExecutionContext, ProjectNodeExecutor,
        ProjectPlanCache, ProjectPlanCacheDecision, ProjectPlanNode, ProjectPlanNodeClass,
        ProjectPlanNodeKind, ProjectPlanOutput, ProjectPlanOutputMaterialization,
        ProjectPlanSideEffect, ProjectPlanSideEffectKind, digest_bytes,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize)]
struct WorkedCorpus {
    cases: Vec<WorkedCase>,
}

#[derive(Debug, Deserialize)]
struct WorkedCase {
    case_id: String,
    request: GeoCompositionRequest,
}

#[test]
fn t05_minimal_core_names_source_records_and_refuses_superset_core() {
    let evidence = chimera_evidence();
    let request = evidence.composition_request.clone();
    let order = chimera_order();
    let budget = GeoExplanationBudget {
        max_core_solves: 64,
        max_cores: 8,
        max_hitting_sets: 8,
    };

    let baseline = solve_composition(&request).expect("chimera request should solve to conflict");
    eprintln!(
        "T05 baseline status={:?} conflict_constraint_ids={:?}",
        baseline.status, baseline.conflict_constraint_ids
    );
    assert_eq!(baseline.status, GeoCompositionStatus::Conflict);

    let artifact = minimal_core(&request, &evidence, &order, &budget)
        .unwrap_or_else(|error| panic!("T05 minimal_core failed: {error:?}"));
    let canonical =
        canonical_explanation_bytes(&artifact).expect("T05 explanation canonical bytes");
    let reparsed: GeoExplanationArtifact =
        serde_json::from_slice(&canonical).expect("T05 canonical explanation parses");
    assert_eq!(artifact, reparsed);
    let core = artifact
        .cores
        .first()
        .unwrap_or_else(|| panic!("T05 no core emitted: {artifact:?}"));
    eprintln!(
        "T05 request_blake3={} evidence_blake3={} order={:?} core={:?}",
        artifact.request_blake3, artifact.evidence_blake3, order, core
    );

    assert_eq!(
        core.constraint_ids,
        ["asserted_address_core", "chimera_wrongly_admitted"]
    );
    assert!(core.minimal, "T05 core must carry a minimality claim");
    assert_eq!(
        core.source_record_ids.len(),
        2,
        "T05 source records={:?}",
        core.source_record_ids
    );
    assert_eq!(
        core.deletion_checks.len(),
        core.constraint_ids.len(),
        "T05 deletion table must cover every core member: {:?}",
        core.deletion_checks
    );
    assert!(
        core.deletion_checks
            .iter()
            .all(|check| check.status_after_deletion != GeoCompositionStatus::Conflict),
        "T05 every single deletion must restore satisfiability: {:?}",
        core.deletion_checks
    );
    assert!(
        core.constraint_ids
            .iter()
            .all(|id| !id.bytes().all(|byte| byte.is_ascii_digit())),
        "T05 core must not name numeric constraint indices: {:?}",
        core.constraint_ids
    );
    assert_eq!(core.source_record_refs.len(), 2);
    assert_eq!(core.admitted_values.len(), 2);

    let superset = vec![
        "asserted_address_core".to_string(),
        "area_majority_buildings".to_string(),
        "chimera_wrongly_admitted".to_string(),
    ];
    let error = verify_minimal_core_members(&request, &evidence, &order, &superset)
        .expect_err("T05 superset core must fail the deletion re-solve");
    eprintln!("T05 negative error={error:?}");
    assert_eq!(error.code, GeoExplanationErrorCode::CoreNotMinimal);
    assert_eq!(
        error.detail.get("constraint_id").map(String::as_str),
        Some("area_majority_buildings")
    );
}

#[test]
fn t06_correction_sets_hit_every_core_and_ceiling_drops_minimality_claims() {
    let evidence = chimera_evidence();
    let request = evidence.composition_request.clone();
    let order = chimera_order();
    let budget = GeoExplanationBudget {
        max_core_solves: 64,
        max_cores: 8,
        max_hitting_sets: 8,
    };
    let mut artifact =
        minimal_core(&request, &evidence, &order, &budget).expect("T06 initial core");
    correction_sets(&mut artifact, &request, &evidence, &budget)
        .unwrap_or_else(|error| panic!("T06 correction_sets failed: {error:?}"));
    eprintln!(
        "T06 budget={budget:?} cores={:?} corrections={:?} counters={:?}",
        artifact.cores, artifact.correction_sets, artifact.counters
    );

    assert!(artifact.cores_complete);
    assert!(artifact.explanation_complete);
    assert_eq!(artifact.cores.len(), 2);
    assert!(
        artifact.correction_sets.iter().all(|set| {
            artifact
                .cores
                .iter()
                .all(|core| intersects(&set.observation_ids, &core.observation_ids))
        }),
        "T06 every correction set must hit every core: {:?}",
        artifact.correction_sets
    );
    for set in &artifact.correction_sets {
        assert_minimal_hitting_set(set, &artifact.cores);
    }

    let mut bad = artifact.clone();
    bad.correction_sets[0].observation_ids = vec!["obs.area_majority_buildings".to_string()];
    let error = validate_explanation_artifact(&bad)
        .expect_err("T06 mutated correction set must miss an enumerated core");
    eprintln!("T06 negative correction error={error:?}");
    assert_eq!(error.code, GeoExplanationErrorCode::InvalidInput);

    let ceiling = GeoExplanationBudget {
        max_core_solves: 64,
        max_cores: 1,
        max_hitting_sets: 8,
    };
    let mut ceiling_artifact =
        minimal_core(&request, &evidence, &order, &budget).expect("T06 initial ceiling core");
    correction_sets(&mut ceiling_artifact, &request, &evidence, &ceiling)
        .expect("T06 ceiling is a typed fallback artifact");
    eprintln!(
        "T06 ceiling cores_complete={} explanation_complete={} cores={:?} corrections={:?} counters={:?}",
        ceiling_artifact.cores_complete,
        ceiling_artifact.explanation_complete,
        ceiling_artifact.cores,
        ceiling_artifact.correction_sets,
        ceiling_artifact.counters
    );
    assert!(!ceiling_artifact.cores_complete);
    assert!(!ceiling_artifact.explanation_complete);
    assert!(
        ceiling_artifact.cores.iter().all(|core| !core.minimal),
        "T06 ceiling must drop every core minimality claim"
    );
    assert!(
        ceiling_artifact
            .correction_sets
            .iter()
            .all(|set| !set.minimal),
        "T06 ceiling must drop every correction-set minimality claim"
    );
}

#[test]
fn t20_explanation_not_conflict_refuses_resolved_and_ambiguous_inputs() {
    let clean = worked_case("case_1_clean_rooftop");
    let ambiguous = worked_case("case_6_dense_one_parcel_multi_building");
    for (name, request, expected_status) in [
        (
            "case_1_clean_rooftop",
            clean,
            GeoCompositionStatus::Resolved,
        ),
        (
            "case_6_dense_one_parcel_multi_building",
            ambiguous,
            GeoCompositionStatus::Ambiguous,
        ),
    ] {
        let solved = solve_composition(&request).expect("T20 request should solve");
        eprintln!(
            "T20 name={name} status={:?} residual_count={}",
            solved.status, solved.summary.residual_model_count
        );
        assert_eq!(solved.status, expected_status);
        let evidence = evidence_mapping_for_request(&request);
        let order = order_for_evidence(&evidence);
        let error = minimal_core(
            &request,
            &evidence,
            &order,
            &GeoExplanationBudget::default(),
        )
        .expect_err("T20 non-conflict input must refuse");
        eprintln!("T20 name={name} error={error:?}");
        assert_eq!(error.code, GeoExplanationErrorCode::ExplanationNotConflict);
        assert_eq!(
            error.detail.get("status").map(String::as_str),
            Some(match expected_status {
                GeoCompositionStatus::Resolved => "resolved",
                GeoCompositionStatus::Ambiguous => "ambiguous",
                GeoCompositionStatus::Conflict => "conflict",
                GeoCompositionStatus::BudgetFallback => "budget_fallback",
            })
        );
    }

    let evidence = chimera_evidence();
    minimal_core(
        &evidence.composition_request,
        &evidence,
        &chimera_order(),
        &GeoExplanationBudget::default(),
    )
    .expect("T20 chimera conflict must not refuse");
}

#[test]
fn t21_separation_marks_inexact_residual_counts_without_value_or_probability_fields() {
    let request = GeoSeparationRequest {
        version: CANON_GEO_SEPARATION_REQUEST_VERSION.to_string(),
        subject_ref: None,
        request: oversized_component_request(),
        prospective: vec![GeoProspectiveObservation {
            id: "obs.prospective.structure-count".to_string(),
            contract_id: "contract.prospective.structure-count".to_string(),
            cost_units: 9,
            outcomes: vec![
                GeoProspectiveOutcome {
                    outcome_id: "outcome.require-p00".to_string(),
                    induced: vec![GeoHardConstraintKind::Require {
                        member: GeoEntityRef::new(GeoEntityLevel::Parcel, "p00"),
                    }],
                },
                GeoProspectiveOutcome {
                    outcome_id: "outcome.require-p01".to_string(),
                    induced: vec![GeoHardConstraintKind::Require {
                        member: GeoEntityRef::new(GeoEntityLevel::Parcel, "p01"),
                    }],
                },
            ],
        }],
    };

    let baseline = solve_composition(&request.request).expect("T21 baseline solves to fallback");
    assert_eq!(baseline.status, GeoCompositionStatus::BudgetFallback);
    assert!(!baseline.summary.residual_model_count_complete);
    let artifact = separate(&request, &GeoExplanationBudget::default())
        .unwrap_or_else(|error| panic!("T21 separation failed: {error:?}"));
    eprintln!(
        "T21 fallback baseline={} complete={} saturated={} rows={:?}",
        artifact.baseline_model_count,
        baseline.summary.residual_model_count_complete,
        baseline.summary.residual_model_count_saturated,
        artifact.per_observation
    );
    assert_eq!(
        artifact.baseline_model_count,
        baseline.summary.residual_model_count
    );
    let row = only_observation(&artifact.per_observation);
    assert!(!row.redundant);
    assert!(
        row.per_outcome.iter().all(|outcome| !outcome.count_exact),
        "T21 inexact baseline must make every outcome inexact: {:?}",
        row.per_outcome
    );
    let keys = serialized_key_set(&serde_json::to_value(&artifact).expect("artifact serializes"));
    for forbidden in ["expect", "probab", "voi", "likelihood"] {
        assert!(
            keys.iter()
                .all(|key| !key.to_ascii_lowercase().contains(forbidden)),
            "T21 serialized field names must not contain {forbidden}: {:?}",
            keys
        );
    }
}

#[test]
fn t21_separation_exact_counts_partition_the_ambiguous_building_case() {
    let base = worked_case("case_6_dense_one_parcel_multi_building");
    let request = GeoSeparationRequest {
        version: CANON_GEO_SEPARATION_REQUEST_VERSION.to_string(),
        subject_ref: None,
        request: base,
        prospective: vec![GeoProspectiveObservation {
            id: "obs.prospective.building-identity".to_string(),
            contract_id: "contract.prospective.building-identity".to_string(),
            cost_units: 3,
            outcomes: vec![
                GeoProspectiveOutcome {
                    outcome_id: "outcome.building-1076314".to_string(),
                    induced: vec![GeoHardConstraintKind::Require {
                        member: GeoEntityRef::new(GeoEntityLevel::Building, "1076314"),
                    }],
                },
                GeoProspectiveOutcome {
                    outcome_id: "outcome.building-1085187".to_string(),
                    induced: vec![GeoHardConstraintKind::Require {
                        member: GeoEntityRef::new(GeoEntityLevel::Building, "1085187"),
                    }],
                },
            ],
        }],
    };

    let artifact = separate(&request, &GeoExplanationBudget::default())
        .unwrap_or_else(|error| panic!("T21 exact separation failed: {error:?}"));
    let row = only_observation(&artifact.per_observation);
    let counts = row
        .per_outcome
        .iter()
        .map(|outcome| outcome.residual_model_count)
        .collect::<Vec<_>>();
    eprintln!("T21 exact induced counts={counts:?} row={row:?}");
    assert_eq!(counts, [1, 1]);
    assert!(
        row.per_outcome.iter().all(|outcome| outcome.count_exact),
        "T21 exact case must keep exact flags"
    );
    assert_eq!(row.worst_case_remaining, 1);
    assert!(!row.redundant);
}

#[test]
fn t27_explain_module_has_no_fixture_or_cohort_literals_and_scan_is_case_insensitive() {
    let source = std::fs::read_to_string("src/geo/explain.rs").expect("explain source");
    let lower = source.to_ascii_lowercase();
    for forbidden in [
        "franklin",
        "39049",
        "epsg:3735",
        "1004540041",
        "chimera_wrongly_admitted",
        "asserted_address_core",
        "case_4",
    ] {
        assert!(
            !lower.contains(forbidden),
            "T27 explain module must remain source-generic; found {forbidden}"
        );
    }
    let scratch = "Franklin CASE_4 asserted_address_core";
    let lower_scratch = scratch.to_ascii_lowercase();
    assert!(lower_scratch.contains("franklin"));
    assert!(lower_scratch.contains("case_4"));
    assert!(lower_scratch.contains("asserted_address_core"));
}

#[test]
fn t75_explain_stage_executor_emits_registered_explanation_output() {
    let compilation = valid_conflict_compilation();
    let compile_bytes =
        canonical_evidence_compilation_bytes(&compilation).expect("compiled bytes serialize");
    let mut composition =
        solve_composition(&compilation.composition_request).expect("conflict request solves");
    assert_eq!(composition.status, GeoCompositionStatus::Conflict);
    composition.evidence_compilation = Some(GeoEvidenceCompilationReference {
        version: compilation.version.clone(),
        request_version: compilation.request_version.clone(),
        blake3: blake3::hash(&compile_bytes).to_hex().to_string(),
    });
    let solve_bytes = canonical_composition_bytes(&composition).expect("solve bytes serialize");

    let node = explain_node();
    let mut executor = GeoProjectNodeExecutor::new();
    let result = executor
        .execute(
            &node,
            &ProjectNodeExecutionContext {
                node_id: node.node_id.clone(),
                dependency_semantic_hashes: BTreeMap::from([
                    (
                        "geo.building.compile_evidence".to_string(),
                        digest_bytes(&compile_bytes),
                    ),
                    ("geo.building.solve".to_string(), digest_bytes(&solve_bytes)),
                ]),
                dependency_outputs: BTreeMap::from([
                    (
                        "geo.building.compile_evidence".to_string(),
                        vec![dependency_output("compile_evidence", compile_bytes)],
                    ),
                    (
                        "geo.building.solve".to_string(),
                        vec![dependency_output("solve", solve_bytes)],
                    ),
                ]),
            },
        )
        .expect("explain stage executor should run");
    let bytes = result
        .outputs
        .get(GEO_EXPLAIN_OUTPUT_ID)
        .unwrap_or_else(|| panic!("missing {GEO_EXPLAIN_OUTPUT_ID} output"));
    let artifact: GeoExplanationArtifact =
        serde_json::from_slice(bytes).expect("explanation output parses");
    validate_explanation_artifact(&artifact).expect("stage output validates");
    assert_eq!(artifact.version, CANON_GEO_EXPLANATION_VERSION);
    assert!(artifact.cores[0].minimal);
    assert!(!artifact.cores[0].source_record_ids.is_empty());
    assert_eq!(result.deterministic_usage["explanation_cores"], 1);
}

fn chimera_evidence() -> GeoEvidenceCompilationArtifact {
    serde_json::from_str(include_str!(
        "fixtures/geo/chimera_evidence_compilation.json"
    ))
    .expect("chimera evidence fixture parses")
}

fn chimera_order() -> GeoReliabilityOrder {
    GeoReliabilityOrder {
        contract_ids_most_reliable_first: vec![
            "asserted_address_core".to_string(),
            "area_majority_buildings".to_string(),
            "chimera_wrongly_admitted".to_string(),
        ],
    }
}

fn worked_case(id: &str) -> GeoCompositionRequest {
    let corpus: WorkedCorpus =
        serde_json::from_str(include_str!("fixtures/geo/e4_worked_cases.json"))
            .expect("worked cases parse");
    corpus
        .cases
        .into_iter()
        .find(|case| case.case_id == id)
        .unwrap_or_else(|| panic!("worked case {id} not found"))
        .request
}

fn evidence_mapping_for_request(request: &GeoCompositionRequest) -> GeoEvidenceCompilationArtifact {
    let admissions = request
        .hard_constraints
        .iter()
        .map(|constraint| {
            let observation_id = format!("obs.{}", constraint.id);
            let contract = GeoRhoContract {
                id: constraint.id.clone(),
                version: "v1".to_string(),
                source_dataset: "fixture.e4.mapping".to_string(),
                source_release: "fixture-release".to_string(),
                source_lineage_ids: vec![format!("fixture.lineage.{}", constraint.id)],
                method_id: "fixture.mapping".to_string(),
                method_version: "v1".to_string(),
                claim_role: GeoEvidenceClaimRole::StableIdentityAnchor,
                basis: GeoRhoBasis::LogicalRelaxation {
                    invariant_id: format!("fixture.invariant.{}", constraint.id),
                },
            };
            canon::geo::GeoEvidenceAdmission {
                observation_id: observation_id.clone(),
                contract,
                source_records: vec![GeoEvidenceRecordRef {
                    source_record_id: format!("fixture.record.{}", constraint.id),
                    source_vintage: "fixture-release".to_string(),
                    record_blake3: blake3::hash(constraint.id.as_bytes()).to_hex().to_string(),
                }],
                valid_time: None,
                observation: observation_for_constraint(&constraint.constraint),
                disposition: GeoEvidenceDisposition::HardConstraint,
                admission_reason: None,
                generated_ids: vec![constraint.id.clone()],
            }
        })
        .collect();
    GeoEvidenceCompilationArtifact {
        version: CANON_GEO_EVIDENCE_COMPILATION_VERSION.to_string(),
        request_version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        composition_request: request.clone(),
        admissions,
    }
}

fn observation_for_constraint(constraint: &GeoHardConstraintKind) -> GeoRhoObservationKind {
    match constraint {
        GeoHardConstraintKind::AllowedSets { level, sets } => GeoRhoObservationKind::ExactSets {
            level: *level,
            sets: sets.clone(),
        },
        GeoHardConstraintKind::AnyOf { members } => GeoRhoObservationKind::ExistentialMembership {
            members: members.clone(),
        },
        GeoHardConstraintKind::IntegerSumBand {
            level,
            measure,
            values,
            min,
            max,
        } => GeoRhoObservationKind::IntegerSumBand {
            level: *level,
            measure: measure.clone(),
            values: values.clone(),
            min: *min,
            max: *max,
        },
        _ => GeoRhoObservationKind::ExactSets {
            level: GeoEntityLevel::Parcel,
            sets: Vec::new(),
        },
    }
}

fn order_for_evidence(evidence: &GeoEvidenceCompilationArtifact) -> GeoReliabilityOrder {
    GeoReliabilityOrder {
        contract_ids_most_reliable_first: evidence
            .admissions
            .iter()
            .map(|admission| admission.contract.id.clone())
            .collect(),
    }
}

fn intersects(left: &[String], right: &[String]) -> bool {
    left.iter().any(|value| right.contains(value))
}

fn assert_minimal_hitting_set(
    set: &canon::geo::GeoCorrectionSet,
    cores: &[canon::geo::GeoMinimalCore],
) {
    for removed in &set.observation_ids {
        let reduced = set
            .observation_ids
            .iter()
            .filter(|candidate| *candidate != removed)
            .cloned()
            .collect::<Vec<_>>();
        let missed = cores
            .iter()
            .find(|core| !intersects(&reduced, &core.observation_ids))
            .map(|core| core.observation_ids.clone());
        assert!(
            missed.is_some(),
            "T06 correction set is not minimal after removing {removed}: set={:?} cores={:?}",
            set,
            cores
        );
    }
}

fn only_observation(rows: &[GeoObservationSeparation]) -> &GeoObservationSeparation {
    assert_eq!(rows.len(), 1, "expected one separation observation row");
    &rows[0]
}

fn serialized_key_set(value: &Value) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    collect_keys(value, &mut keys);
    keys
}

fn collect_keys(value: &Value, keys: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                keys.insert(key.clone());
                collect_keys(value, keys);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_keys(item, keys);
            }
        }
        _ => {}
    }
}

fn oversized_component_request() -> GeoCompositionRequest {
    let parcels = (0..12)
        .map(|index| format!("p{index:02}"))
        .collect::<Vec<_>>();
    GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        universe: GeoCompositionUniverse {
            parcels: parcels.clone(),
            buildings: Vec::new(),
        },
        hard_constraints: vec![GeoHardConstraint {
            id: "whole_level".to_string(),
            constraint: GeoHardConstraintKind::AllowedSets {
                level: GeoEntityLevel::Parcel,
                sets: vec![parcels],
            },
        }],
        soft_preferences: Vec::new(),
        max_assignments: 1_000,
        max_materialized_models: 0,
    }
}

fn valid_conflict_compilation() -> GeoEvidenceCompilationArtifact {
    let request = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        universe: GeoCompositionUniverse {
            parcels: vec!["parcel-a".to_string(), "parcel-b".to_string()],
            buildings: Vec::new(),
        },
        contracts: vec![contract("contract-a"), contract("contract-b")],
        observations: vec![
            GeoRhoObservation {
                id: "obs-a".to_string(),
                contract_id: "contract-a".to_string(),
                source_records: vec![source_record("record-a")],
                valid_time: None,
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![vec!["parcel-a".to_string()]],
                },
            },
            GeoRhoObservation {
                id: "obs-b".to_string(),
                contract_id: "contract-b".to_string(),
                source_records: vec![source_record("record-b")],
                valid_time: None,
                observation: GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![vec!["parcel-b".to_string()]],
                },
            },
        ],
        max_assignments: 16,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };
    compile_evidence(&request).expect("valid conflict evidence compiles")
}

fn contract(id: &str) -> GeoRhoContract {
    GeoRhoContract {
        id: id.to_string(),
        version: "v1".to_string(),
        source_dataset: "fixture.valid-conflict".to_string(),
        source_release: "fixture-release".to_string(),
        source_lineage_ids: vec![format!("fixture.lineage.{id}")],
        method_id: "fixture.exact-set".to_string(),
        method_version: "v1".to_string(),
        claim_role: GeoEvidenceClaimRole::StableIdentityAnchor,
        basis: GeoRhoBasis::LogicalRelaxation {
            invariant_id: format!("fixture.invariant.{id}"),
        },
    }
}

fn source_record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "fixture-release".to_string(),
        record_blake3: blake3::hash(id.as_bytes()).to_hex().to_string(),
    }
}

fn dependency_output(output_id: &str, bytes: Vec<u8>) -> ProjectDependencyOutput {
    ProjectDependencyOutput {
        output_id: output_id.to_string(),
        content_digest: digest_bytes(&bytes),
        byte_count: bytes.len() as u64,
        bytes,
    }
}

fn explain_node() -> ProjectPlanNode {
    ProjectPlanNode {
        node_id: "geo.building.explain".to_string(),
        kind: ProjectPlanNodeKind::Solve,
        class: ProjectPlanNodeClass::Computation,
        command: GEO_EXPLAIN_STAGE_COMMAND.to_string(),
        dependencies: vec![
            "geo.building.compile_evidence".to_string(),
            "geo.building.solve".to_string(),
        ],
        content_hash_inputs: Vec::new(),
        outputs: vec![ProjectPlanOutput {
            output_id: GEO_EXPLAIN_OUTPUT_ID.to_string(),
            path: "geo/building/explanation.json".to_string(),
            content_hash: String::new(),
            materialization: ProjectPlanOutputMaterialization::PlannedArtifact,
        }],
        limits: BTreeMap::from([
            ("geo.explanation.max_core_solves".to_string(), 64),
            ("geo.explanation.max_cores".to_string(), 8),
            ("geo.explanation.max_hitting_sets".to_string(), 8),
        ]),
        cache: ProjectPlanCache {
            eligible: true,
            decision: ProjectPlanCacheDecision::Miss,
            cache_key: "geo.building.explain.fixture".to_string(),
            reason: "fixture stage executor test".to_string(),
        },
        side_effects: vec![
            ProjectPlanSideEffect {
                kind: ProjectPlanSideEffectKind::ReadsInput,
                description: "reads declared dependency artifacts".to_string(),
            },
            ProjectPlanSideEffect {
                kind: ProjectPlanSideEffectKind::WritesArtifact,
                description: "writes explanation artifact".to_string(),
            },
        ],
        refusal_conditions: Vec::new(),
        runnable: true,
        blocked_by: Vec::new(),
    }
}
