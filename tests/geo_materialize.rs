#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_WAREHOUSE_ROWS_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS, GeoCompositionProfile,
    GeoCompositionStatus, GeoEntityLevel, GeoEntityProjectionStatus, GeoEntityRef,
    GeoEvidenceClaimRole, GeoEvidenceRecordRef, GeoIntegerMeasure, GeoIntegerMemberValue,
    GeoIntegerValueOrigin, GeoMaterializationErrorCode, GeoProjectedEntityLevel, GeoRhoBasis,
    GeoRhoContract, GeoRhoObservationKind, GeoWarehouseBuildingParcelRow, GeoWarehouseEvidenceRow,
    GeoWarehouseParcelRow, GeoWarehouseRowsRequest, canonical_materialized_evidence_request_bytes,
    compile_evidence, materialize_warehouse_rows, solve_composition,
};

fn record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "26v1".to_string(),
        record_blake3: blake3::hash(id.as_bytes()).to_hex().to_string(),
    }
}

fn contract() -> GeoRhoContract {
    GeoRhoContract {
        id: "rho.parcel-set".to_string(),
        version: "1.0.0".to_string(),
        source_dataset: "SOURCE.EXPORTED_PARCEL_FACTS".to_string(),
        source_release: "26v1".to_string(),
        source_lineage_ids: vec!["SOURCE.NYC_DCP_MAPPLUTO_HOT:26v1".to_string()],
        method_id: "predicate-c-positive-area".to_string(),
        method_version: "1.0.0".to_string(),
        claim_role: GeoEvidenceClaimRole::StableIdentityAnchor,
        basis: GeoRhoBasis::LogicalRelaxation {
            invariant_id: "candidate-set-is-a-superset".to_string(),
        },
    }
}

fn rows() -> GeoWarehouseRowsRequest {
    let observation = GeoRhoObservationKind::ExactSets {
        level: GeoEntityLevel::Parcel,
        sets: vec![vec!["parcel-a".to_string(), "parcel-b".to_string()]],
    };
    GeoWarehouseRowsRequest {
        version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        parcel_rows: vec![
            GeoWarehouseParcelRow {
                parcel_id: "parcel-b".to_string(),
            },
            GeoWarehouseParcelRow {
                parcel_id: "parcel-a".to_string(),
            },
        ],
        building_parcel_rows: vec![
            GeoWarehouseBuildingParcelRow {
                building_id: "building-b".to_string(),
                parcel_id: None,
            },
            GeoWarehouseBuildingParcelRow {
                building_id: "building-a".to_string(),
                parcel_id: Some("parcel-b".to_string()),
            },
            GeoWarehouseBuildingParcelRow {
                building_id: "building-a".to_string(),
                parcel_id: Some("parcel-a".to_string()),
            },
        ],
        contracts: vec![contract()],
        evidence_rows: vec![
            GeoWarehouseEvidenceRow {
                observation_id: "obs.parcel-set".to_string(),
                contract_id: "rho.parcel-set".to_string(),
                source_record: record("row-b"),
                valid_time: None,
                observation: observation.clone(),
            },
            GeoWarehouseEvidenceRow {
                observation_id: "obs.parcel-set".to_string(),
                contract_id: "rho.parcel-set".to_string(),
                source_record: record("row-a"),
                valid_time: None,
                observation,
            },
        ],
        max_assignments: 128,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn building_contract() -> GeoRhoContract {
    GeoRhoContract {
        id: "rho.building-set".to_string(),
        version: "1.0.0".to_string(),
        source_dataset: "SOURCE.EXPORTED_BUILDING_FACTS".to_string(),
        source_release: "26v1".to_string(),
        source_lineage_ids: vec!["SOURCE.BUILDINGS_HOT:26v1".to_string()],
        method_id: "source-building-candidate-set".to_string(),
        method_version: "1.0.0".to_string(),
        claim_role: GeoEvidenceClaimRole::StableIdentityAnchor,
        basis: GeoRhoBasis::LogicalRelaxation {
            invariant_id: "candidate-set-is-a-superset".to_string(),
        },
    }
}

fn building_rows() -> GeoWarehouseRowsRequest {
    let observation = GeoRhoObservationKind::ExactSets {
        level: GeoEntityLevel::Building,
        sets: vec![vec!["building-a".to_string(), "building-b".to_string()]],
    };
    GeoWarehouseRowsRequest {
        version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
        profile: GeoCompositionProfile::building(),
        parcel_rows: Vec::new(),
        building_parcel_rows: vec![
            GeoWarehouseBuildingParcelRow {
                building_id: "building-b".to_string(),
                parcel_id: None,
            },
            GeoWarehouseBuildingParcelRow {
                building_id: "building-a".to_string(),
                parcel_id: None,
            },
        ],
        contracts: vec![building_contract()],
        evidence_rows: vec![
            GeoWarehouseEvidenceRow {
                observation_id: "obs.building-set".to_string(),
                contract_id: "rho.building-set".to_string(),
                source_record: record("building-row-b"),
                valid_time: None,
                observation: observation.clone(),
            },
            GeoWarehouseEvidenceRow {
                observation_id: "obs.building-set".to_string(),
                contract_id: "rho.building-set".to_string(),
                source_record: record("building-row-a"),
                valid_time: None,
                observation,
            },
        ],
        max_assignments: 128,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

#[test]
fn materializer_groups_relational_grains_without_counting_sources_as_constraints() {
    let request = materialize_warehouse_rows(&rows()).expect("rows should materialize");

    assert_eq!(request.profile, GeoCompositionProfile::parcel());
    assert_eq!(request.universe.parcels, ["parcel-a", "parcel-b"]);
    assert_eq!(request.universe.buildings[0].id, "building-a");
    assert_eq!(
        request.universe.buildings[0].parcel_ids,
        ["parcel-a", "parcel-b"]
    );
    assert_eq!(request.universe.buildings[1].id, "building-b");
    assert!(request.universe.buildings[1].parcel_ids.is_empty());
    assert_eq!(request.observations.len(), 1);
    assert_eq!(
        request.observations[0]
            .source_records
            .iter()
            .map(|record| record.source_record_id.as_str())
            .collect::<Vec<_>>(),
        ["row-a", "row-b"]
    );

    let compilation = compile_evidence(&request).expect("materialized request should compile");
    assert_eq!(compilation.admissions.len(), 1);
    assert_eq!(compilation.composition_request.hard_constraints.len(), 1);
}

#[test]
fn materializer_rejects_omitted_profile() {
    let error = serde_json::from_value::<GeoWarehouseRowsRequest>(serde_json::json!({
        "version": CANON_GEO_WAREHOUSE_ROWS_VERSION,
        "parcel_rows": [
            { "parcel_id": "parcel-a" },
            { "parcel_id": "parcel-b" }
        ],
        "building_parcel_rows": [],
        "contracts": [contract()],
        "evidence_rows": [{
            "observation_id": "obs.parcel-set",
            "contract_id": "rho.parcel-set",
            "source_record": record("row-a"),
            "observation": {
                "kind": "exact_sets",
                "level": "parcel",
                "sets": [["parcel-a", "parcel-b"]]
            }
        }],
        "max_assignments": 128,
        "max_materialized_models": DEFAULT_MAX_MATERIALIZED_MODELS
    }))
    .expect_err("profile is required");

    assert!(error.to_string().contains("missing field `profile`"));
}

#[test]
fn warehouse_rows_reject_unknown_fields_at_every_row_layer() {
    fn expect_unknown_field(value: serde_json::Value) {
        let error =
            serde_json::from_value::<GeoWarehouseRowsRequest>(value).expect_err("must reject");
        assert!(error.to_string().contains("unknown field"));
    }

    let mut root = serde_json::to_value(rows()).expect("rows must serialize");
    root.as_object_mut()
        .expect("root is an object")
        .insert("unexpected".to_string(), serde_json::json!(true));
    expect_unknown_field(root);

    let mut parcel = serde_json::to_value(rows()).expect("rows must serialize");
    parcel["parcel_rows"][0]
        .as_object_mut()
        .expect("parcel row is an object")
        .insert("unexpected".to_string(), serde_json::json!(true));
    expect_unknown_field(parcel);

    let mut building = serde_json::to_value(rows()).expect("rows must serialize");
    building["building_parcel_rows"][0]
        .as_object_mut()
        .expect("building row is an object")
        .insert("unexpected".to_string(), serde_json::json!(true));
    expect_unknown_field(building);

    let mut evidence = serde_json::to_value(rows()).expect("rows must serialize");
    evidence["evidence_rows"][0]
        .as_object_mut()
        .expect("evidence row is an object")
        .insert("unexpected".to_string(), serde_json::json!(true));
    expect_unknown_field(evidence);
}

#[test]
fn materializer_canonicalizes_nested_observation_payloads_before_grouping() {
    let mut exact_rows = rows();
    exact_rows.evidence_rows[0].observation = GeoRhoObservationKind::ExactSets {
        level: GeoEntityLevel::Parcel,
        sets: vec![
            vec!["parcel-b".to_string(), "parcel-a".to_string()],
            vec!["parcel-a".to_string()],
        ],
    };
    exact_rows.evidence_rows[1].observation = GeoRhoObservationKind::ExactSets {
        level: GeoEntityLevel::Parcel,
        sets: vec![
            vec!["parcel-a".to_string()],
            vec!["parcel-a".to_string(), "parcel-b".to_string()],
        ],
    };

    let request = materialize_warehouse_rows(&exact_rows).expect("ordered variants should group");
    assert_eq!(request.observations.len(), 1);
    match &request.observations[0].observation {
        GeoRhoObservationKind::ExactSets { sets, .. } => assert_eq!(
            sets,
            &vec![
                vec!["parcel-a".to_string()],
                vec!["parcel-a".to_string(), "parcel-b".to_string()]
            ]
        ),
        other => panic!("unexpected observation {other:?}"),
    }

    let mut existential_rows = rows();
    existential_rows.evidence_rows[0].observation = GeoRhoObservationKind::ExistentialMembership {
        members: vec![
            GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-b"),
            GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a"),
        ],
    };
    existential_rows.evidence_rows[1].observation = GeoRhoObservationKind::ExistentialMembership {
        members: vec![
            GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a"),
            GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-b"),
        ],
    };

    let request =
        materialize_warehouse_rows(&existential_rows).expect("member order variants should group");
    match &request.observations[0].observation {
        GeoRhoObservationKind::ExistentialMembership { members } => assert_eq!(
            members
                .iter()
                .map(|member| (member.level, member.id.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (GeoEntityLevel::Parcel, "parcel-a"),
                (GeoEntityLevel::Parcel, "parcel-b")
            ]
        ),
        other => panic!("unexpected observation {other:?}"),
    }

    let measure = GeoIntegerMeasure {
        semantic_id: "count.fixture".to_string(),
        unit: "unit".to_string(),
        value_origin: GeoIntegerValueOrigin::SourceAsserted,
    };
    let mut band_rows = rows();
    band_rows.evidence_rows[0].observation = GeoRhoObservationKind::IntegerSumBand {
        level: GeoEntityLevel::Parcel,
        measure: measure.clone(),
        values: vec![
            GeoIntegerMemberValue {
                id: "parcel-b".to_string(),
                value: 2,
            },
            GeoIntegerMemberValue {
                id: "parcel-a".to_string(),
                value: 1,
            },
        ],
        min: 1,
        max: 3,
    };
    band_rows.evidence_rows[1].observation = GeoRhoObservationKind::IntegerSumBand {
        level: GeoEntityLevel::Parcel,
        measure,
        values: vec![
            GeoIntegerMemberValue {
                id: "parcel-a".to_string(),
                value: 1,
            },
            GeoIntegerMemberValue {
                id: "parcel-b".to_string(),
                value: 2,
            },
        ],
        min: 1,
        max: 3,
    };

    let request =
        materialize_warehouse_rows(&band_rows).expect("value order variants should group");
    match &request.observations[0].observation {
        GeoRhoObservationKind::IntegerSumBand { values, .. } => assert_eq!(
            values
                .iter()
                .map(|value| (value.id.as_str(), value.value))
                .collect::<Vec<_>>(),
            vec![("parcel-a", 1), ("parcel-b", 2)]
        ),
        other => panic!("unexpected observation {other:?}"),
    }
}

#[test]
fn materializer_preserves_prefer_member_as_soft_preference() {
    let mut rows = rows();
    let preference = GeoRhoObservationKind::PreferMember {
        member: GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-b"),
        cost_if_absent: 7,
    };
    for row in &mut rows.evidence_rows {
        row.observation = preference.clone();
    }

    let request = materialize_warehouse_rows(&rows).expect("preference rows should materialize");
    assert_eq!(request.observations[0].observation, preference);
    let artifact = compile_evidence(&request).expect("preference request should compile");
    assert!(artifact.composition_request.hard_constraints.is_empty());
    assert_eq!(artifact.composition_request.soft_preferences.len(), 1);
    assert_eq!(
        artifact.composition_request.soft_preferences[0].member,
        GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-b")
    );
}

#[test]
fn building_profile_materializes_parcel_free_request_with_exact_ordered_bytes() {
    let request = materialize_warehouse_rows(&building_rows()).expect("rows should materialize");

    assert_eq!(request.profile, GeoCompositionProfile::building());
    assert!(request.universe.parcels.is_empty());
    assert_eq!(
        request
            .universe
            .buildings
            .iter()
            .map(|building| {
                (
                    building.id.as_str(),
                    building
                        .parcel_ids
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("building-a", Vec::<&str>::new()),
            ("building-b", Vec::<&str>::new())
        ]
    );
    compile_evidence(&request).expect("building-profile request should compile");

    let row_a_hash = blake3::hash(b"building-row-a").to_hex().to_string();
    let row_b_hash = blake3::hash(b"building-row-b").to_hex().to_string();
    let expected = format!(
        concat!(
            r#"{{"version":"canon_geo_evidence_request.v0","#,
            r#""profile":{{"version":"canon_geo_composition_profile.v0","selection_level":"building"}},"#,
            r#""universe":{{"parcels":[],"buildings":["#,
            r#"{{"id":"building-a","parcel_ids":[]}},"#,
            r#"{{"id":"building-b","parcel_ids":[]}}"#,
            r#"]}},"#,
            r#""contracts":["#,
            r#"{{"id":"rho.building-set","version":"1.0.0","#,
            r#""source_dataset":"SOURCE.EXPORTED_BUILDING_FACTS","source_release":"26v1","#,
            r#""source_lineage_ids":["SOURCE.BUILDINGS_HOT:26v1"],"#,
            r#""method_id":"source-building-candidate-set","method_version":"1.0.0","#,
            r#""claim_role":"stable_identity_anchor","#,
            r#""basis":{{"kind":"logical_relaxation","invariant_id":"candidate-set-is-a-superset"}}}}"#,
            r#"],"#,
            r#""observations":["#,
            r#"{{"id":"obs.building-set","contract_id":"rho.building-set","#,
            r#""source_records":["#,
            r#"{{"source_record_id":"building-row-a","source_vintage":"26v1","record_blake3":"{}"}},"#,
            r#"{{"source_record_id":"building-row-b","source_vintage":"26v1","record_blake3":"{}"}}"#,
            r#"],"#,
            r#""observation":{{"kind":"exact_sets","level":"building","sets":[["building-a","building-b"]]}}}}"#,
            r#"],"#,
            r#""max_assignments":128,"max_materialized_models":{}"#,
            r#"}}"#
        ),
        row_a_hash, row_b_hash, DEFAULT_MAX_MATERIALIZED_MODELS
    );
    let actual =
        String::from_utf8(canonical_materialized_evidence_request_bytes(&request).unwrap())
            .expect("canonical bytes must be UTF-8 JSON");
    assert_eq!(actual, expected);
}

#[test]
fn building_profile_bytes_are_stable_under_input_row_permutations() {
    let original = building_rows();
    let mut permuted = original.clone();
    permuted.building_parcel_rows.reverse();
    permuted.evidence_rows.reverse();

    let original = materialize_warehouse_rows(&original).expect("original rows materialize");
    let permuted = materialize_warehouse_rows(&permuted).expect("permuted rows materialize");
    assert_eq!(original, permuted);
    assert_eq!(
        canonical_materialized_evidence_request_bytes(&original).unwrap(),
        canonical_materialized_evidence_request_bytes(&permuted).unwrap()
    );
}

#[test]
fn building_profile_warehouse_rows_solve_to_entity_projection_handoff() {
    let request = materialize_warehouse_rows(&building_rows()).expect("rows should materialize");
    let compiled = compile_evidence(&request).expect("request should compile");
    let artifact =
        solve_composition(&compiled.composition_request).expect("building request should solve");
    let projection = artifact
        .entity_projection
        .expect("building solve must emit entity projection");

    let building = projection
        .levels
        .iter()
        .find(|entry| entry.level == GeoProjectedEntityLevel::Building)
        .expect("building projection");
    assert_eq!(building.status, GeoEntityProjectionStatus::ExactResidual);
    assert_eq!(
        building.residual_status,
        Some(GeoCompositionStatus::Resolved)
    );
    assert_eq!(building.candidates, ["building-a", "building-b"]);
    assert_eq!(building.hard_forced, ["building-a", "building-b"]);
    assert!(building.backbone_complete);
    assert_eq!(building.residual_model_count, Some(1));
    assert_eq!(
        building.residual_sets,
        [vec!["building-a".to_string(), "building-b".to_string()]]
    );

    let parcel = projection
        .levels
        .iter()
        .find(|entry| entry.level == GeoProjectedEntityLevel::Parcel)
        .expect("parcel projection");
    assert_eq!(parcel.status, GeoEntityProjectionStatus::Suppressed);
    assert!(parcel.residual_model_count.is_none());
    assert!(parcel.candidates.is_empty());
    assert!(!parcel.backbone_complete);
}

#[test]
fn materializer_is_byte_deterministic_under_input_row_permutations() {
    let original = rows();
    let mut permuted = original.clone();
    permuted.parcel_rows.reverse();
    permuted.building_parcel_rows.reverse();
    permuted.evidence_rows.reverse();

    let original = materialize_warehouse_rows(&original).expect("original rows materialize");
    let permuted = materialize_warehouse_rows(&permuted).expect("permuted rows materialize");
    assert_eq!(original, permuted);
    assert_eq!(
        canonical_materialized_evidence_request_bytes(&original).unwrap(),
        canonical_materialized_evidence_request_bytes(&permuted).unwrap()
    );
}

#[test]
fn materializer_rejects_duplicate_warehouse_grains_and_semantic_conflicts() {
    let mut duplicate = rows();
    duplicate.parcel_rows.push(GeoWarehouseParcelRow {
        parcel_id: "parcel-a".to_string(),
    });
    let error = materialize_warehouse_rows(&duplicate).expect_err("duplicate grain must fail");
    assert_eq!(error.code, GeoMaterializationErrorCode::InvalidInput);
    assert_eq!(error.detail["parcel_id"], "parcel-a");

    let mut conflict = rows();
    conflict.evidence_rows[1].observation = GeoRhoObservationKind::ExactSets {
        level: GeoEntityLevel::Parcel,
        sets: vec![vec!["parcel-a".to_string()]],
    };
    let error = materialize_warehouse_rows(&conflict).expect_err("conflicting rows must fail");
    assert_eq!(error.code, GeoMaterializationErrorCode::InvalidInput);
    assert_eq!(error.detail["observation_id"], "obs.parcel-set");
}

#[test]
fn building_profile_rejects_parcel_rows_and_parcel_incidences() {
    let mut parcel_rows = building_rows();
    parcel_rows.parcel_rows.push(GeoWarehouseParcelRow {
        parcel_id: "parcel-a".to_string(),
    });
    let error = materialize_warehouse_rows(&parcel_rows).expect_err("parcel rows must fail");
    assert_eq!(error.code, GeoMaterializationErrorCode::InvalidInput);
    assert_eq!(error.detail["selection_level"], "building");
    assert_eq!(error.detail["field"], "parcel_rows");

    let mut incidence = building_rows();
    incidence.building_parcel_rows[0].parcel_id = Some("parcel-a".to_string());
    let error = materialize_warehouse_rows(&incidence).expect_err("parcel incidence must fail");
    assert_eq!(error.code, GeoMaterializationErrorCode::InvalidInput);
    assert_eq!(error.detail["selection_level"], "building");
    assert_eq!(error.detail["field"], "building_parcel_rows.parcel_id");
    assert_eq!(error.detail["building_id"], "building-b");
    assert_eq!(error.detail["parcel_id"], "parcel-a");
}

#[test]
fn building_profile_dangling_members_still_fail_through_the_compiler() {
    let mut dangling = building_rows();
    let observation = GeoRhoObservationKind::ExactSets {
        level: GeoEntityLevel::Building,
        sets: vec![vec!["building-c".to_string()]],
    };
    for row in &mut dangling.evidence_rows {
        row.observation = observation.clone();
    }

    let error = materialize_warehouse_rows(&dangling).expect_err("unknown building must fail");
    assert_eq!(error.code, GeoMaterializationErrorCode::Evidence);
    assert_eq!(error.detail["composition_code"], "InvalidInput");
    assert_eq!(error.detail["level"], "building");
    assert_eq!(error.detail["member_id"], "building-c");
}

#[test]
fn materializer_cannot_bypass_the_evidence_compiler() {
    let mut invalid = rows();
    invalid.building_parcel_rows[1].parcel_id = Some("parcel-outside-tile".to_string());

    let error = materialize_warehouse_rows(&invalid).expect_err("unknown parcel must fail");
    assert_eq!(error.code, GeoMaterializationErrorCode::Evidence);
    assert_eq!(error.detail["composition_code"], "InvalidInput");
    assert_eq!(error.detail["parcel_id"], "parcel-outside-tile");
}
