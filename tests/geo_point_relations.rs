#![forbid(unsafe_code)]

use canon::geo::*;
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn hash(label: &str) -> String {
    blake3::hash(label.as_bytes()).to_hex().to_string()
}
fn record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.into(),
        source_vintage: "2026-10-20".into(),
        record_blake3: hash(id),
    }
}
fn point(x: i64, y: i64) -> GeoSourcePointDecimal {
    GeoSourcePointDecimal {
        x: x.to_string(),
        y: y.to_string(),
    }
}
fn ring(x: i64, y: i64, size: i64) -> Vec<GeoSourcePointDecimal> {
    vec![
        point(x, y),
        point(x + size, y),
        point(x + size, y + size),
        point(x, y + size),
        point(x, y),
    ]
}
fn square(id: &str, x: i64) -> GeoPointParcelCandidate {
    GeoPointParcelCandidate {
        parcel_id: id.into(),
        source_record: record(id),
        geometry: Some(GeoGeometryFeatureInput {
            feature_id: id.into(),
            source_crs: "local-mm".into(),
            geometry: GeoSourceGeometry::Polygon {
                exterior: ring(x, 0, 100),
                holes: Vec::new(),
            },
        }),
        retained_admission_refusal: None,
    }
}
fn fixture() -> GeoPointParcelRequest {
    GeoPointParcelRequest {
        version: CANON_GEO_POINT_PARCEL_REQUEST_VERSION.into(),
        target_ref: "synthetic-target".into(),
        target_relation: GeoPointTargetRelation::ProposedComponentOf,
        target_source_records: vec![record("target-source")],
        source_as_of: "2026-10-20".into(),
        source_transform: GeoPointSourceTransform {
            method_id: "fixture-identity".into(),
            method_version: "v1".into(),
            parameters_blake3: hash("identity transform"),
            source_accuracy_mm: None,
        },
        family: GeoRhoContract {
            id: "plan-family".into(),
            version: "v1".into(),
            source_dataset: "synthetic-plan".into(),
            source_release: "2026-10-20".into(),
            source_lineage_ids: vec!["plan:one".into()],
            method_id: "external-point-observation".into(),
            method_version: "v1".into(),
            claim_role: GeoEvidenceClaimRole::AttributeObservation,
            basis: GeoRhoBasis::Uncalibrated {
                reason: "source accuracy is unknown".into(),
                admission_policy: GeoRhoAdmissionPolicy::SoftWithWeight { cost_if_absent: 1 },
            },
        },
        inventory_source: GeoTileSourceBinding {
            source_instance_id: "synthetic-parcels".into(),
            release: GeoSourceRelease {
                release_id: "v1".into(),
                release_digest: format!("blake3:{}", hash("inventory release")),
            },
            native_scope: GeoNativeEntityScope::NativeEntity {
                entity_level: GeoControlEntityLevel::Parcel,
                identity_participation: GeoIdentityParticipation::EvidenceOnly,
            },
            inventory_ref: GeoPlanInventoryRef {
                inventory_id: "synthetic-inventory".into(),
                semantic_hash: format!("blake3:{}", hash("inventory")),
                planning_hash: format!("blake3:{}", hash("inventory planning")),
            },
        },
        frame: GeoLocalFrameContract {
            version: CANON_GEO_LOCAL_FRAME_VERSION.into(),
            frame_id: "test:local-mm".into(),
            tile_id: "892a100d26bffff".into(),
            source_crs: "local-mm".into(),
            source_axis_domain: GeoSourceAxisDomain::Planar,
            source_decimal_places: 0,
            source_origin: GeoSourcePointFixed { x: 0, y: 0 },
            affine: GeoAffineProjectionMm {
                x_from_source_x_numerator: 1,
                x_from_source_y_numerator: 0,
                y_from_source_x_numerator: 0,
                y_from_source_y_numerator: 1,
                denominator: 1,
            },
            projection: GeoProjectionProvenance {
                method_id: "fixture-identity".into(),
                method_version: "v1".into(),
                parameters_blake3: hash("identity frame"),
                max_projection_error_micrometres: 0,
            },
            max_abs_coordinate_mm: 2_000_000,
        },
        points: [
            ("p1", 10, 10),
            ("p2", 20, 20),
            ("p3", 0, 50),
            ("p4", 150, 50),
        ]
        .into_iter()
        .map(|(id, x, y)| GeoParcelPointObservation {
            point_id: id.into(),
            source_record: record(id),
            frame_id: "test:local-mm".into(),
            coordinate: point(x, y),
        })
        .collect(),
        candidates: vec![
            square("a", 0),
            square("wrong-neighbor", 200),
            GeoPointParcelCandidate {
                parcel_id: "missing".into(),
                source_record: record("missing"),
                geometry: None,
                retained_admission_refusal: None,
            },
        ],
        acquisition_truncated: false,
        max_relation_pairs: 1000,
        max_predicates: 1000,
        max_vertices_per_geometry: 1000,
        max_geometry_bytes: 1_000_000,
        max_report_bytes: 1_000_000,
        max_assignments: 10_000,
        max_materialized_models: 1000,
    }
}
fn outcome(report: &GeoPointParcelRelations, parcel: &str, point: &str) -> GeoPointParcelOutcome {
    report
        .candidates
        .iter()
        .find(|c| c.parcel_id == parcel)
        .unwrap()
        .groups
        .iter()
        .find(|g| g.point_ids.iter().any(|id| id == point))
        .unwrap()
        .outcome
}

#[test]
fn native_locations_keep_boundary_neighbor_and_missing_geometry_distinct() {
    let request = fixture();
    let report = build_point_parcel_relations(&request).unwrap();
    assert_eq!(report.candidates.len(), 3);
    assert_eq!(outcome(&report, "a", "p1"), GeoPointParcelOutcome::Interior);
    assert_eq!(outcome(&report, "a", "p3"), GeoPointParcelOutcome::Boundary);
    assert_eq!(outcome(&report, "a", "p4"), GeoPointParcelOutcome::Exterior);
    assert_eq!(
        outcome(&report, "wrong-neighbor", "p1"),
        GeoPointParcelOutcome::Exterior
    );
    assert_eq!(
        outcome(&report, "missing", "p1"),
        GeoPointParcelOutcome::UnavailableGeometry
    );
    assert!(report.bbox_exterior_pairs > 0);
    assert_eq!(report.source_transform.source_accuracy_mm, None);
    let evidence = materialize_point_parcel_evidence(&request).unwrap();
    let compiled = compile_evidence(&evidence).unwrap();
    validate_evidence_compilation_artifact(&compiled).unwrap();
    assert!(compiled.composition_request.hard_constraints.is_empty());
    assert_eq!(compiled.composition_request.soft_preferences.len(), 1);
    assert_eq!(
        compiled.composition_request.soft_preferences[0].member.id,
        "a"
    );
    assert_eq!(compiled.composition_request.universe.parcels.len(), 3);
    assert!(compiled.admissions.iter().any(|a| matches!(
        a.observation,
        GeoRhoObservationKind::PointParcelRelations { .. }
    ) && a.disposition
        == GeoEvidenceDisposition::DiagnosticOnly));
}

#[test]
fn accuracy_and_transform_provenance_never_grant_hard_identity_authority() {
    let request = fixture();
    let baseline = build_point_parcel_relations(&request).unwrap();
    let mut changed = request.clone();
    changed.source_transform.source_accuracy_mm = Some(1_000_000);
    changed.source_transform.parameters_blake3 = hash("different uncalibrated controls");
    let report = build_point_parcel_relations(&changed).unwrap();
    assert_ne!(baseline.source_transform, report.source_transform);
    assert_eq!(baseline.candidates, report.candidates);
    let compiled = compile_evidence(&materialize_point_parcel_evidence(&changed).unwrap()).unwrap();
    validate_evidence_compilation_artifact(&compiled).unwrap();
    assert!(compiled.composition_request.hard_constraints.is_empty());
    assert_eq!(compiled.composition_request.soft_preferences.len(), 1);
    // Actual changed source coordinates must change spatial support, regardless
    // of an unchanged target label or a declared accuracy number.
    for point in &mut changed.points {
        point.coordinate.x = "250".into();
    }
    let moved = compile_evidence(&materialize_point_parcel_evidence(&changed).unwrap()).unwrap();
    assert_eq!(moved.composition_request.soft_preferences.len(), 1);
    assert_eq!(
        moved.composition_request.soft_preferences[0].member.id,
        "wrong-neighbor"
    );
    assert!(moved.composition_request.hard_constraints.is_empty());
    changed.source_transform.parameters_blake3 = "corrupt".into();
    assert!(validate_point_parcel_request(&changed).is_err());
}

#[test]
fn holes_multipolygons_and_overlaps_are_native_relations_without_uniqueness() {
    let mut request = fixture();
    request.candidates[0].geometry.as_mut().unwrap().geometry = GeoSourceGeometry::Polygon {
        exterior: ring(0, 0, 100),
        holes: vec![ring(40, 40, 20)],
    };
    request.points[1].coordinate = point(50, 50);
    request.points[2].coordinate = point(50, 40);
    request.points[3].coordinate = point(450, 50);
    request.candidates[1].geometry.as_mut().unwrap().geometry = GeoSourceGeometry::MultiPolygon {
        polygons: vec![
            GeoSourcePolygon {
                exterior: ring(200, 0, 100),
                holes: Vec::new(),
            },
            GeoSourcePolygon {
                exterior: ring(400, 0, 100),
                holes: Vec::new(),
            },
        ],
    };
    let report = build_point_parcel_relations(&request).unwrap();
    assert_eq!(
        outcome(&report, "a", "p2"),
        GeoPointParcelOutcome::Exterior,
        "a hole must not identify its host parcel"
    );
    assert_eq!(outcome(&report, "a", "p3"), GeoPointParcelOutcome::Boundary);
    assert_eq!(
        outcome(&report, "wrong-neighbor", "p4"),
        GeoPointParcelOutcome::Interior
    );
    let mut overlap = fixture();
    overlap.candidates[1] = square("overlap", 5);
    let evidence = materialize_point_parcel_evidence(&overlap).unwrap();
    let compiled = compile_evidence(&evidence).unwrap();
    assert_eq!(compiled.composition_request.soft_preferences.len(), 2);
    assert!(compiled.composition_request.hard_constraints.is_empty());
}

#[test]
fn duplicate_dependent_points_do_not_multiply_member_weight_and_order_is_stable() {
    let request = fixture();
    let before = materialize_point_parcel_evidence(&request).unwrap();
    let mut duplicated = request.clone();
    let mut extra = duplicated.points[0].clone();
    extra.point_id = "same-family-copy".into();
    duplicated.points.push(extra);
    let after = materialize_point_parcel_evidence(&duplicated).unwrap();
    assert_eq!(
        compile_evidence(&before)
            .unwrap()
            .composition_request
            .soft_preferences,
        compile_evidence(&after)
            .unwrap()
            .composition_request
            .soft_preferences
    );
    let mut reversed = request.clone();
    reversed.points.reverse();
    reversed.candidates.reverse();
    reversed.target_source_records.reverse();
    assert_eq!(
        serde_json::to_vec(&before).unwrap(),
        serde_json::to_vec(&materialize_point_parcel_evidence(&reversed).unwrap()).unwrap()
    );
}

#[test]
fn mixed_frames_bad_geometry_and_predicate_budget_never_become_no_hits() {
    let mut request = fixture();
    request.points[0].frame_id = "foreign-frame".into();
    let report = build_point_parcel_relations(&request).unwrap();
    assert_eq!(
        outcome(&report, "a", "p1"),
        GeoPointParcelOutcome::MixedFrame
    );
    let mut budget = fixture();
    budget.max_predicates = 0;
    let report = build_point_parcel_relations(&budget).unwrap();
    assert_eq!(
        outcome(&report, "a", "p1"),
        GeoPointParcelOutcome::BudgetExhausted
    );
    assert_eq!(
        outcome(&report, "wrong-neighbor", "p1"),
        GeoPointParcelOutcome::Exterior,
        "admitted bbox can still establish an exterior pair"
    );
    assert!(
        compile_evidence(&materialize_point_parcel_evidence(&budget).unwrap())
            .unwrap()
            .composition_request
            .soft_preferences
            .is_empty()
    );
    let mut broken = fixture();
    let GeoSourceGeometry::Polygon { exterior, .. } =
        &mut broken.candidates[0].geometry.as_mut().unwrap().geometry
    else {
        unreachable!()
    };
    exterior.insert(3, exterior[1].clone());
    let report = build_point_parcel_relations(&broken).unwrap();
    assert_eq!(
        outcome(&report, "a", "p1"),
        GeoPointParcelOutcome::GeometryAdmissionRefused
    );
    assert_eq!(
        report
            .candidates
            .iter()
            .find(|c| c.parcel_id == "a")
            .unwrap()
            .groups[0]
            .problem
            .as_ref()
            .unwrap()
            .code,
        GeoGeometryErrorCode::DuplicateVertex
    );
}

#[test]
fn arithmetic_failure_and_retained_admission_failures_remain_unknowns() {
    let mut request = fixture();
    request.frame.affine.x_from_source_x_numerator = 2;
    request.points[0].coordinate.x = i64::MAX.to_string();
    request.candidates[2].retained_admission_refusal = Some(GeoGeometryError {
        code: GeoGeometryErrorCode::SelfIntersection,
        message: "retained native refusal".into(),
        detail: Default::default(),
        budget: None,
    });
    let report = build_point_parcel_relations(&request).unwrap();
    assert_eq!(
        outcome(&report, "a", "p1"),
        GeoPointParcelOutcome::ArithmeticFailure
    );
    assert_eq!(
        outcome(&report, "missing", "p1"),
        GeoPointParcelOutcome::RetainedAdmissionRefused
    );
    assert_eq!(report.candidates.len(), 3);
}

#[test]
fn truncated_acquisition_corrupt_provenance_and_forged_accounting_refuse() {
    let mut request = fixture();
    request.acquisition_truncated = true;
    assert!(build_point_parcel_relations(&request).is_err());
    request = fixture();
    request.max_relation_pairs = 1;
    assert_eq!(
        build_point_parcel_relations(&request).unwrap_err().detail["field"],
        "max_relation_pairs"
    );
    request = fixture();
    request.points[0].source_record.record_blake3 = "corrupt".into();
    assert!(build_point_parcel_relations(&request).is_err());
    request = fixture();
    request.inventory_source.release.release_digest = "corrupt".into();
    assert_eq!(
        validate_point_parcel_request(&request).unwrap_err().detail["field"],
        "inventory_source.release.release_digest"
    );
    request = fixture();
    request.max_report_bytes = 1;
    assert_eq!(
        build_point_parcel_relations(&request).unwrap_err().detail["field"],
        "max_report_bytes"
    );
    let evidence = materialize_point_parcel_evidence(&fixture()).unwrap();
    let mut compiled = compile_evidence(&evidence).unwrap();
    validate_evidence_compilation_artifact(&compiled).unwrap();
    compiled
        .admissions
        .iter_mut()
        .find(|a| {
            matches!(
                a.observation,
                GeoRhoObservationKind::PointParcelRelations { .. }
            )
        })
        .unwrap()
        .admission_reason = Some("forged reason".into());
    assert!(validate_evidence_compilation_artifact(&compiled).is_err());
    let mut report = build_point_parcel_relations(&fixture()).unwrap();
    report.candidates.pop();
    assert!(validate_point_parcel_relations(&report, &evidence.universe).is_err());
    let mut report = build_point_parcel_relations(&fixture()).unwrap();
    report.candidates[0].groups[0].point_ids.pop();
    assert!(validate_point_parcel_relations(&report, &evidence.universe).is_err());
    let mut logical = fixture();
    logical.family.basis = GeoRhoBasis::LogicalRelaxation {
        invariant_id: "a point is not identity authority".into(),
    };
    assert!(materialize_point_parcel_evidence(&logical).is_err());
}

#[test]
fn diagnostic_accounting_cannot_satisfy_a_hard_corroboration_policy() {
    let mut request = fixture();
    request.max_predicates = 0;
    let mut evidence = materialize_point_parcel_evidence(&request).unwrap();
    let mut policy = request.family.clone();
    policy.id = "calibrated-other-source".into();
    policy.source_lineage_ids = vec!["other-source".into()];
    policy.basis = GeoRhoBasis::EmpiricalCalibration {
        population_id: "synthetic-policy-control".into(),
        calibration_blake3: hash("synthetic calibration"),
        falsification_rule_id: "synthetic-only-not-live".into(),
        admissible_hard_band: true,
        admission_policy: GeoRhoAdmissionPolicy::HardOnlyWhenCorroborated {
            minimum_distinct_contracts: 1,
            corroborating_contract_ids: vec![request.family.id.clone()],
            fallback: GeoRhoAdmissionFallback::DiagnosticOnly,
        },
    };
    evidence.observations.push(GeoRhoObservation {
        id: "other-claim".into(),
        contract_id: policy.id.clone(),
        source_records: vec![record("other-source-row")],
        valid_time: None,
        observation: GeoRhoObservationKind::IntegerSumBand {
            level: GeoEntityLevel::Parcel,
            measure: GeoIntegerMeasure {
                semantic_id: "synthetic:count".into(),
                unit: "count".into(),
                value_origin: GeoIntegerValueOrigin::ExactDerived,
            },
            values: vec![GeoIntegerMemberValue {
                id: "a".into(),
                value: 1,
            }],
            min: 1,
            max: 1,
        },
    });
    evidence.contracts.push(policy);
    let compiled = compile_evidence(&evidence).unwrap();
    validate_evidence_compilation_artifact(&compiled).unwrap();
    assert!(
        compiled.composition_request.hard_constraints.is_empty(),
        "diagnostic source accounting must not grant hard admission"
    );
    assert_eq!(
        compiled
            .admissions
            .iter()
            .find(|a| a.observation_id == "other-claim")
            .unwrap()
            .disposition,
        GeoEvidenceDisposition::DiagnosticOnly
    );
    evidence.observations.push(GeoRhoObservation {
        id: "independent-observation-not-accounting".into(),
        contract_id: request.family.id.clone(),
        source_records: vec![record("ordinary-support-row")],
        valid_time: None,
        observation: GeoRhoObservationKind::PreferMember {
            member: GeoEntityRef::new(GeoEntityLevel::Parcel, "a"),
            cost_if_absent: 1,
        },
    });
    let corroborated = compile_evidence(&evidence).unwrap();
    validate_evidence_compilation_artifact(&corroborated).unwrap();
    assert_eq!(
        corroborated.composition_request.hard_constraints.len(),
        1,
        "otherwise-admissible calibrated band becomes hard with an actual observation, not accounting alone"
    );
}

fn public_command(root: &Path, argv: &[Value]) -> Value {
    let args: Vec<_> = argv
        .iter()
        .map(|v| v.as_str().expect("documented string argv"))
        .collect();
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--no-witness")
        .args(&args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn published_output(root: &Path, run: &Value, id: &str) -> Value {
    let outputs: Vec<_> = run["project_run_report"]["receipt"]["node_receipts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|node| node["outputs"].as_array().unwrap())
        .filter(|output| output["output_id"] == id)
        .collect();
    assert_eq!(outputs.len(), 1, "one declared producer for {id}");
    let output = outputs[0];
    let bytes = fs::read(root.join(output["path"].as_str().unwrap())).unwrap();
    assert_eq!(output["byte_count"].as_u64().unwrap(), bytes.len() as u64);
    assert_eq!(
        output["content_digest"],
        format!("blake3:{}", blake3::hash(&bytes).to_hex())
    );
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn installed_operator_example_runs_native_points_then_reuses_and_refreshes_through_public_cli() {
    let guide = agent_guide::agent_guide().unwrap();
    let example = guide["examples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "synthetic_point_parcel_not_live")
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for directory in example["create_directories"].as_array().unwrap() {
        fs::create_dir_all(root.join(directory.as_str().unwrap())).unwrap();
    }
    for (name, value) in example["write_files"].as_object().unwrap() {
        fs::write(root.join(name), serde_json::to_vec(value).unwrap()).unwrap();
    }
    let mut results = Vec::new();
    for step in example["steps"].as_array().unwrap() {
        let result = public_command(root, step["argv"].as_array().unwrap());
        fs::write(
            root.join(step["stdout_file"].as_str().unwrap()),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
        results.push(result);
    }
    let cold = &results[1];
    let warm = &results[3];
    assert_eq!(
        cold["project_run_report"]["executed_nodes"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(
        warm["project_run_report"]["resumed_nodes"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert!(
        warm["project_run_report"]["executed_nodes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(cold["semantic_hash"], warm["semantic_hash"]);
    assert_eq!(cold["output_refs"], warm["output_refs"]);
    assert_eq!(
        results[4],
        public_command(root, example["steps"][4]["argv"].as_array().unwrap()),
        "two fresh inspections of the same stored warm run are identical"
    );
    let compiled: GeoEvidenceCompilationArtifact = serde_json::from_value(published_output(
        &root.join("work"),
        cold,
        "compile_evidence",
    ))
    .unwrap();
    assert_eq!(compiled.composition_request.universe.parcels.len(), 3);
    assert!(compiled.composition_request.hard_constraints.is_empty());
    assert_eq!(compiled.composition_request.soft_preferences.len(), 1);
    assert_eq!(
        compiled.composition_request.soft_preferences[0].member.id,
        "parcel-a"
    );
    let report = compiled
        .admissions
        .iter()
        .find_map(|a| match &a.observation {
            GeoRhoObservationKind::PointParcelRelations { report } => Some(report),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        outcome(report, "parcel-a", "boundary-point"),
        GeoPointParcelOutcome::Boundary
    );
    assert_eq!(
        outcome(report, "parcel-missing", "point-1"),
        GeoPointParcelOutcome::UnavailableGeometry
    );
    let answer = published_output(&root.join("work"), cold, "answer");
    assert_eq!(answer["version"], CANON_GEO_ANSWER_VERSION);
    assert_eq!(answer["further_work"]["source"], "not_in_scope");
    let run_args = example["steps"][1]["argv"].as_array().unwrap();
    let mut changed = example["write_files"]["point-input.json"].clone();
    changed["points"][0]["source_record"]["record_blake3"] =
        Value::String(hash("changed retained source"));
    fs::write(
        root.join("point-input.json"),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    let refreshed = public_command(root, run_args);
    assert_eq!(
        refreshed["project_run_report"]["resumed_nodes"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "home cells and bounded section are unaffected"
    );
    assert_eq!(
        refreshed["project_run_report"]["executed_nodes"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert_ne!(refreshed["semantic_hash"], cold["semantic_hash"]);
    changed["acquisition_truncated"] = Value::Bool(true);
    fs::write(
        root.join("point-input.json"),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    let invalid = public_command(root, run_args);
    assert_eq!(invalid["phase"], "PREFLIGHTED");
    assert!(invalid["project_run_report"].is_null());
    let invalid_run: GeoRun = serde_json::from_value(invalid.clone()).unwrap();
    assert!(invalid_run.output_refs.is_empty());
    assert!(
        invalid["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["kind"] == "INVALID_INPUT")
    );
    for (case, expected_reason) in [
        ("omitted-candidate", "candidate universe must equal"),
        ("changed-release", "source/release must equal"),
        ("corrupt-json", "key must be a string"),
    ] {
        let mut input = example["write_files"]["point-input.json"].clone();
        if case == "omitted-candidate" {
            input["candidates"].as_array_mut().unwrap().pop();
        }
        if case == "changed-release" {
            input["inventory_source"]["release"]["release_id"] =
                Value::String("other-release".into());
        }
        fs::write(
            root.join("point-input.json"),
            if case == "corrupt-json" {
                b"{bad-json".to_vec()
            } else {
                serde_json::to_vec(&input).unwrap()
            },
        )
        .unwrap();
        let work_dir = format!("work-{case}");
        fs::create_dir(root.join(&work_dir)).unwrap();
        let mut args = run_args.to_vec();
        let work_index = args.iter().position(|a| a == "--work-dir").unwrap() + 1;
        args[work_index] = Value::String(work_dir);
        let result = public_command(root, &args);
        let run: GeoRun = serde_json::from_value(result.clone()).unwrap();
        assert!(
            !run.output_refs.iter().any(|o| o.output_id == "answer"),
            "{case}: {result}"
        );
        if case == "corrupt-json" {
            assert!(run.project_run_report.is_none());
            assert!(
                result["blockers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|b| b["reason"].as_str().unwrap().contains(expected_reason)),
                "{result}"
            );
        } else {
            assert_eq!(result["status"], "FAILED");
            assert!(
                result["project_run_report"]["node_reports"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|n| n["reason"]
                        .as_str()
                        .is_some_and(|reason| reason.contains(expected_reason))),
                "{result}"
            );
        }
    }
}
