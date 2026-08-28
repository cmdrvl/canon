#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_WAREHOUSE_ROWS_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS, GeoEvidenceClaimRole,
    GeoEvidenceRecordRef, GeoMaterializationErrorCode, GeoRhoBasis, GeoRhoContract,
    GeoRhoObservationKind, GeoWarehouseBuildingParcelRow, GeoWarehouseEvidenceRow,
    GeoWarehouseParcelRow, GeoWarehouseRowsRequest, canonical_materialized_evidence_request_bytes,
    compile_evidence, materialize_warehouse_rows,
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
        level: canon::geo::GeoEntityLevel::Parcel,
        sets: vec![vec!["parcel-a".to_string(), "parcel-b".to_string()]],
    };
    GeoWarehouseRowsRequest {
        version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
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

#[test]
fn materializer_groups_relational_grains_without_counting_sources_as_constraints() {
    let request = materialize_warehouse_rows(&rows()).expect("rows should materialize");

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
        level: canon::geo::GeoEntityLevel::Parcel,
        sets: vec![vec!["parcel-a".to_string()]],
    };
    let error = materialize_warehouse_rows(&conflict).expect_err("conflicting rows must fail");
    assert_eq!(error.code, GeoMaterializationErrorCode::InvalidInput);
    assert_eq!(error.detail["observation_id"], "obs.parcel-set");
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
