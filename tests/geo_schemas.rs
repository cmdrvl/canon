//! Schema-drift guard for the registered Geo contracts.
//!
//! For each contract this test: (a) pins the schema file's `title` and
//! `properties.version.const`, and asserts top-level `additionalProperties`
//! is `false`; (b) builds a real instance through the library API, serializes
//! it with `serde_json`, and walks every key present in the serialized value
//! to confirm the schema declares it somewhere reachable from the root
//! object (`properties`, `$defs`, `$ref`, array `items`, and `oneOf`
//! alternatives for tagged enums). This does not add a `jsonschema`
//! dependency; it only catches keys the schema forgot to declare.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use canon::entity::run::link::multisource::EntitySourceRole;
use canon::geo::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, CANON_GEO_EVIDENCE_REQUEST_VERSION,
    CANON_GEO_GEOMETRY_REQUEST_VERSION, CANON_GEO_LOCAL_FRAME_VERSION,
    CANON_GEO_MULTISOURCE_REQUEST_VERSION, CANON_GEO_POPULATION_REQUEST_VERSION,
    CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION, CANON_GEO_TILE_WORK_REQUEST_VERSION,
    CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION, CANON_GEO_WAREHOUSE_ROWS_VERSION,
    DEFAULT_MAX_MATERIALIZED_MODELS, GeoAffineProjectionMm, GeoBuildingCandidate,
    GeoCompositionModel, GeoCompositionRequest, GeoCompositionUniverse, GeoEntityLevel,
    GeoEntityRef, GeoEvidenceClaimRole, GeoEvidenceCompilationRequest, GeoEvidenceRecordRef,
    GeoExactSourceUnitMm, GeoGeometryFeatureInput, GeoGeometryTileRequest, GeoHardConstraint,
    GeoHardConstraintKind, GeoLabeledCompositionCase, GeoLocalFrameContract, GeoMultisourceRequest,
    GeoMultisourceSource, GeoPopulationEvaluationRequest, GeoProjectionProvenance, GeoRhoBasis,
    GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind, GeoSourceAxisDomain,
    GeoSourceGeometry, GeoSourcePointDecimal, GeoSourcePointFixed, GeoTileDecisionBatch,
    GeoTileDecisionMember, GeoTileDecisionProposal, GeoTileFeatureRef,
    GeoTileReconciliationRequest, GeoTileWorkRequest, GeoWarehouseEvidenceRow,
    GeoWarehouseGeometryRow, GeoWarehouseGeometryRowsRequest, GeoWarehouseParcelRow,
    GeoWarehouseRowsRequest, compile_evidence, evaluate_population, materialize_geo_multisource,
    materialize_geometry_tile, materialize_tile_work_unit, materialize_warehouse_geometry,
    reconcile_tile_decisions, solve_composition,
};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::{fs, path::Path};

const COMPOSITION_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.composition_request.v0.schema.json");
const COMPOSITION_SCHEMA: &str = include_str!("../schemas/canon.geo.composition.v0.schema.json");
const EVIDENCE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.evidence_request.v0.schema.json");
const EVIDENCE_COMPILATION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.evidence_compilation.v0.schema.json");
const POPULATION_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.population_request.v0.schema.json");
const POPULATION_EVALUATION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.population_evaluation.v0.schema.json");
const WAREHOUSE_ROWS_SCHEMA: &str =
    include_str!("../schemas/canon.geo.warehouse_rows.v0.schema.json");
const GEOMETRY_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.geometry_request.v0.schema.json");
const GEOMETRY_TILE_SCHEMA: &str =
    include_str!("../schemas/canon.geo.geometry_tile.v0.schema.json");
const WAREHOUSE_GEOMETRY_ROWS_SCHEMA: &str =
    include_str!("../schemas/canon.geo.warehouse_geometry_rows.v0.schema.json");
const WAREHOUSE_GEOMETRY_SCHEMA: &str =
    include_str!("../schemas/canon.geo.warehouse_geometry.v0.schema.json");
const TILE_WORK_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.tile_work_request.v0.schema.json");
const TILE_WORK_UNIT_SCHEMA: &str =
    include_str!("../schemas/canon.geo.tile_work_unit.v0.schema.json");
const TILE_RECONCILIATION_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.tile_reconciliation_request.v0.schema.json");
const TILE_RECONCILIATION_SCHEMA: &str =
    include_str!("../schemas/canon.geo.tile_reconciliation.v0.schema.json");
const MULTISOURCE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.multisource_request.v0.schema.json");
const MULTISOURCE_ARTIFACT_SCHEMA: &str =
    include_str!("../schemas/canon.entity.multisource_link.v1.schema.json");

fn parsed(source: &str) -> Value {
    serde_json::from_str(source).expect("schema file must be valid JSON")
}

fn assert_schema_shape(schema: &Value, expected_title: &str, expected_version_const: &str) {
    assert_eq!(
        schema.get("title").and_then(Value::as_str),
        Some(expected_title),
        "title mismatch"
    );
    let version_const = schema
        .pointer("/properties/version/const")
        .and_then(Value::as_str);
    assert_eq!(
        version_const,
        Some(expected_version_const),
        "properties.version.const mismatch"
    );
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false),
        "top-level additionalProperties must be false"
    );
}

/// Resolve a `$ref` such as `#/$defs/entity_ref` against the schema root.
fn resolve_ref<'a>(schema: &'a Value, reference: &str) -> &'a Value {
    let path = reference
        .strip_prefix('#')
        .expect("only local $ref pointers are supported");
    schema
        .pointer(path)
        .unwrap_or_else(|| panic!("$ref {reference} does not resolve"))
}

/// Recursively assert that every key present in `instance` is declared by
/// `subschema` (following `$ref`, `oneOf` alternatives whose `const`/`enum`
/// match a discriminant key, `items` for arrays, and `additionalProperties`
/// pattern schemas). `root` is the whole schema document, needed to resolve
/// `$ref`.
fn assert_instance_matches_schema(root: &Value, subschema: &Value, instance: &Value, path: &str) {
    if let Some(reference) = subschema.get("$ref").and_then(Value::as_str) {
        if reference.starts_with('#') {
            let resolved = resolve_ref(root, reference);
            assert_instance_matches_schema(root, resolved, instance, path);
        } else {
            let source = match reference {
                "canon.geo.geometry_tile.v0.schema.json" => GEOMETRY_TILE_SCHEMA,
                _ => panic!("external $ref {reference} is not registered in the schema test"),
            };
            let external = parsed(source);
            assert_instance_matches_schema(&external, &external, instance, path);
        }
        return;
    }

    if let Some(alternatives) = subschema.get("oneOf").and_then(Value::as_array) {
        if instance.is_null() {
            let has_null_alt = alternatives
                .iter()
                .any(|alt| alt.get("type").and_then(Value::as_str) == Some("null"));
            assert!(has_null_alt, "{path}: null value but no null alternative");
            return;
        }
        if instance.is_boolean() || instance.is_number() || instance.is_string() {
            // Scalar oneOf alternative (e.g. Option<bool>, Option<u64>): the
            // schema declared it, nothing further to walk into.
            return;
        }
        let Value::Object(object) = instance else {
            panic!("{path}: expected an object for oneOf, got {instance:?}");
        };
        // Pick the alternative whose object-level shape matches: prefer one
        // declaring a `kind` const equal to the instance's `kind`, else the
        // first alternative whose required properties are all present.
        let chosen = alternatives
            .iter()
            .find(|alt| {
                let Some(kind_const) = alt
                    .pointer("/properties/kind/const")
                    .and_then(Value::as_str)
                else {
                    return false;
                };
                object.get("kind").and_then(Value::as_str) == Some(kind_const)
            })
            .or_else(|| {
                alternatives.iter().find(|alt| {
                    let required = alt
                        .get("required")
                        .and_then(Value::as_array)
                        .map(|values| values.as_slice())
                        .unwrap_or(&[]);
                    required
                        .iter()
                        .all(|key| object.contains_key(key.as_str().unwrap_or("")))
                })
            })
            .unwrap_or_else(|| panic!("{path}: no oneOf alternative matches {instance:?}"));
        assert_instance_matches_schema(root, chosen, instance, path);
        return;
    }

    match instance {
        Value::Object(object) => {
            let properties = subschema
                .get("properties")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for (key, value) in object {
                let child_path = format!("{path}.{key}");
                let child_schema = properties.get(key).unwrap_or_else(|| {
                    panic!(
                        "{child_path}: key not declared in schema properties at {path} (available: {:?})",
                        properties.keys().collect::<Vec<_>>()
                    )
                });
                assert_instance_matches_schema(root, child_schema, value, &child_path);
            }
        }
        Value::Array(items) => {
            if let Some(item_schema) = subschema.get("items") {
                for (index, item) in items.iter().enumerate() {
                    assert_instance_matches_schema(
                        root,
                        item_schema,
                        item,
                        &format!("{path}[{index}]"),
                    );
                }
            } else if !items.is_empty() {
                panic!("{path}: array has items but schema declares no `items`");
            }
        }
        _ => {
            // Scalars (string/number/bool/null) terminate the walk: the
            // parent already confirmed the key is declared.
        }
    }
}

fn assert_drift_free(
    schema_source: &str,
    expected_title: &str,
    expected_version: &str,
    instance: &Value,
) {
    let schema = parsed(schema_source);
    assert_schema_shape(&schema, expected_title, expected_version);
    assert_instance_matches_schema(&schema, &schema, instance, "$");
}

fn small_universe() -> GeoCompositionUniverse {
    GeoCompositionUniverse {
        parcels: vec!["parcel-a".to_string(), "parcel-b".to_string()],
        buildings: vec![GeoBuildingCandidate {
            id: "building-a".to_string(),
            parcel_ids: vec!["parcel-a".to_string()],
        }],
    }
}

fn composition_request() -> GeoCompositionRequest {
    GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        universe: small_universe(),
        hard_constraints: vec![GeoHardConstraint {
            id: "any-of-parcels".to_string(),
            constraint: GeoHardConstraintKind::AnyOf {
                members: vec![
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a"),
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-b"),
                ],
            },
        }],
        soft_preferences: Vec::new(),
        max_assignments: 64,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn evidence_request() -> GeoEvidenceCompilationRequest {
    GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: small_universe(),
        contracts: vec![GeoRhoContract {
            id: "contract-1".to_string(),
            version: "v1".to_string(),
            source_dataset: "fixture:dataset".to_string(),
            source_release: "fixture-v1".to_string(),
            source_lineage_ids: vec!["fixture:upstream-dataset".to_string()],
            method_id: "fixture:method".to_string(),
            method_version: "v1".to_string(),
            claim_role: GeoEvidenceClaimRole::AttributeObservation,
            basis: GeoRhoBasis::LogicalRelaxation {
                invariant_id: "fixture:invariant".to_string(),
            },
        }],
        observations: vec![GeoRhoObservation {
            id: "obs-1".to_string(),
            contract_id: "contract-1".to_string(),
            source_records: vec![GeoEvidenceRecordRef {
                source_record_id: "row-1".to_string(),
                source_vintage: "fixture-v1".to_string(),
                record_blake3: blake3::hash(b"row-1").to_hex().to_string(),
            }],
            valid_time: None,
            observation: GeoRhoObservationKind::ExistentialMembership {
                members: vec![
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-a"),
                    GeoEntityRef::new(GeoEntityLevel::Parcel, "parcel-b"),
                ],
            },
        }],
        max_assignments: 64,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn warehouse_rows_request() -> GeoWarehouseRowsRequest {
    let evidence = evidence_request();
    GeoWarehouseRowsRequest {
        version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
        parcel_rows: evidence
            .universe
            .parcels
            .iter()
            .cloned()
            .map(|parcel_id| GeoWarehouseParcelRow { parcel_id })
            .collect(),
        building_parcel_rows: Vec::new(),
        contracts: evidence.contracts,
        evidence_rows: evidence
            .observations
            .into_iter()
            .flat_map(|observation| {
                observation
                    .source_records
                    .into_iter()
                    .map(move |source_record| GeoWarehouseEvidenceRow {
                        observation_id: observation.id.clone(),
                        contract_id: observation.contract_id.clone(),
                        source_record,
                        valid_time: observation.valid_time,
                        observation: observation.observation.clone(),
                    })
            })
            .collect(),
        max_assignments: evidence.max_assignments,
        max_materialized_models: evidence.max_materialized_models,
    }
}

fn geometry_request() -> GeoGeometryTileRequest {
    GeoGeometryTileRequest {
        version: CANON_GEO_GEOMETRY_REQUEST_VERSION.to_string(),
        frame: GeoLocalFrameContract {
            version: CANON_GEO_LOCAL_FRAME_VERSION.to_string(),
            frame_id: "tile:fixture:local-mm:v1".to_string(),
            tile_id: "fixture".to_string(),
            source_crs: "LOCAL:FIXTURE".to_string(),
            source_axis_domain: GeoSourceAxisDomain::Planar,
            source_decimal_places: 3,
            source_origin: GeoSourcePointFixed { x: 0, y: 0 },
            affine: GeoAffineProjectionMm {
                x_from_source_x_numerator: 1,
                x_from_source_y_numerator: 0,
                y_from_source_x_numerator: 0,
                y_from_source_y_numerator: 1,
                denominator: 1,
            },
            projection: GeoProjectionProvenance {
                method_id: "fixture-affine".to_string(),
                method_version: "v1".to_string(),
                parameters_blake3: blake3::hash(b"fixture-affine").to_hex().to_string(),
                max_projection_error_micrometres: 200,
            },
            max_abs_coordinate_mm: 2_000_000,
        },
        features: vec![GeoGeometryFeatureInput {
            feature_id: "parcel-a".to_string(),
            source_crs: "LOCAL:FIXTURE".to_string(),
            geometry: GeoSourceGeometry::Polygon {
                exterior: vec![
                    source_point("0", "0"),
                    source_point("5", "0"),
                    source_point("5", "5"),
                    source_point("0", "5"),
                    source_point("0", "0"),
                ],
                holes: Vec::new(),
            },
        }],
        max_vertices_per_geometry: 100,
        max_geometry_bytes_per_tile: 100_000,
    }
}

fn warehouse_geometry_request() -> GeoWarehouseGeometryRowsRequest {
    let points = [
        (980_252.301_632_881_2_f64, 191_655.610_172_272_3_f64),
        (980_352.301_632_881_2, 191_655.610_172_272_3),
        (980_352.301_632_881_2, 191_755.610_172_272_3),
        (980_252.301_632_881_2, 191_755.610_172_272_3),
        (980_252.301_632_881_2, 191_655.610_172_272_3),
    ];
    let mut wkb = Vec::new();
    wkb.push(1);
    wkb.extend_from_slice(&3_u32.to_le_bytes());
    wkb.extend_from_slice(&1_u32.to_le_bytes());
    wkb.extend_from_slice(&(points.len() as u32).to_le_bytes());
    for (x, y) in points {
        wkb.extend_from_slice(&x.to_le_bytes());
        wkb.extend_from_slice(&y.to_le_bytes());
    }
    let sha256: String = Sha256::digest(&wkb)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    GeoWarehouseGeometryRowsRequest {
        version: CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION.to_string(),
        tile_id: "892a100d26bffff".to_string(),
        frame_id: "tile:892a100d26bffff:epsg2263-mm:v0".to_string(),
        source_crs: "EPSG:2263".to_string(),
        source_srid: 2263,
        source_decimal_places: 9,
        source_origin: source_point("980000", "191000"),
        source_unit_to_millimetres: GeoExactSourceUnitMm {
            unit_id: "us-survey-foot".to_string(),
            numerator: 1_200_000,
            denominator: 3_937,
        },
        rows: vec![GeoWarehouseGeometryRow {
            feature_id: "parcel-a".to_string(),
            source_record_id: "mn/000000/1".to_string(),
            source_dataset: "nyc_dcp_mappluto".to_string(),
            source_release: "26v2".to_string(),
            source_release_date: "2026-08-01".to_string(),
            source_geometry_contract_version: "nyc_dcp_mappluto_geometry_evidence.v3".to_string(),
            source_archive_sha256:
                "e06eca9034731bc23f058bf532090e3c1ea6aed44a8128c6928f33872da34ab5".to_string(),
            source_crs: "EPSG:2263".to_string(),
            source_srid: 2263,
            source_geom_wkb_base64: BASE64_STANDARD.encode(&wkb),
            source_geom_wkb_sha256: sha256,
            transform_execution_id: "sha256-execution-26v2".to_string(),
            transform_definition_id: "sha256-definition-hpgn".to_string(),
        }],
        max_abs_coordinate_mm: 1_000_000,
        max_vertices_per_geometry: 10_000,
        max_geometry_bytes_per_tile: 1_000_000,
    }
}

fn tile_work_request() -> GeoTileWorkRequest {
    GeoTileWorkRequest {
        version: CANON_GEO_TILE_WORK_REQUEST_VERSION.to_string(),
        center_cell: "892a100d26bffff".to_string(),
        halo_k: 1,
        features: vec![GeoTileFeatureRef {
            source_name: "parcel".to_string(),
            feature_id: "parcel-a".to_string(),
            home_cell: "892a100d26bffff".to_string(),
        }],
        max_features: 8,
        max_work_cells: 7,
    }
}

fn tile_reconciliation_request() -> GeoTileReconciliationRequest {
    let work_unit =
        materialize_tile_work_unit(&tile_work_request()).expect("tile work unit materializes");
    GeoTileReconciliationRequest {
        version: CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION.to_string(),
        halo_k: 1,
        batches: vec![GeoTileDecisionBatch {
            work_unit,
            proposals: vec![GeoTileDecisionProposal {
                payload_blake3: format!("blake3:{}", blake3::hash(b"fixture decision").to_hex()),
                members: vec![GeoTileDecisionMember {
                    source_name: "parcel".to_string(),
                    feature_id: "parcel-a".to_string(),
                    home_cell: "892a100d26bffff".to_string(),
                }],
            }],
        }],
        max_batches: 4,
        max_proposals: 8,
        max_members_per_decision: 8,
        max_features_per_batch: 8,
        max_work_cells_per_batch: 7,
    }
}

fn source_point(x: &str, y: &str) -> GeoSourcePointDecimal {
    GeoSourcePointDecimal {
        x: x.to_string(),
        y: y.to_string(),
    }
}

fn multisource_request(root: &Path) -> GeoMultisourceRequest {
    let source_specs = [
        ("parcel", EntitySourceRole::Reference, "entity:parcel"),
        ("property", EntitySourceRole::Target, "entity:parcel"),
        ("footprint", EntitySourceRole::Peer, "entity:other"),
    ];
    let sources = source_specs
        .into_iter()
        .map(|(name, role, canonical_id)| {
            let rows_path = root.join(format!("{name}.csv"));
            fs::write(
                &rows_path,
                format!("source_row_id,anchor_id,canonical_id\n{name}-1,shared,{canonical_id}\n"),
            )
            .expect("write multisource schema fixture");
            GeoMultisourceSource {
                name: name.to_string(),
                role,
                rows_path,
                local_id_column: Some("source_row_id".to_string()),
                anchor_namespace: Some("fixture-anchor".to_string()),
                anchor_column: Some("anchor_id".to_string()),
                canonical_id_column: Some("canonical_id".to_string()),
            }
        })
        .collect();
    GeoMultisourceRequest {
        version: CANON_GEO_MULTISOURCE_REQUEST_VERSION.to_string(),
        sources,
        comparison_graph: Vec::new(),
        default_pair_budget: 8,
    }
}

#[test]
fn composition_request_schema_matches_a_real_instance() {
    let request = composition_request();
    let instance = serde_json::to_value(&request).expect("request must serialize");
    assert_drift_free(
        COMPOSITION_REQUEST_SCHEMA,
        "canon.geo.composition_request.v0",
        "canon_geo_composition_request.v0",
        &instance,
    );
}

#[test]
fn geometry_request_schema_matches_a_real_instance() {
    let request = geometry_request();
    let instance = serde_json::to_value(&request).expect("geometry request must serialize");
    assert_drift_free(
        GEOMETRY_REQUEST_SCHEMA,
        "canon.geo.geometry_request.v0",
        CANON_GEO_GEOMETRY_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn geometry_tile_schema_matches_a_real_instance() {
    let artifact =
        materialize_geometry_tile(&geometry_request()).expect("geometry request materializes");
    let instance = serde_json::to_value(&artifact).expect("geometry tile must serialize");
    assert_drift_free(
        GEOMETRY_TILE_SCHEMA,
        "canon.geo.geometry_tile.v0",
        "canon_geo_geometry_tile.v0",
        &instance,
    );
}

#[test]
fn warehouse_geometry_rows_schema_matches_a_real_instance() {
    let request = warehouse_geometry_request();
    let instance = serde_json::to_value(&request).expect("warehouse geometry rows serialize");
    assert_drift_free(
        WAREHOUSE_GEOMETRY_ROWS_SCHEMA,
        "canon.geo.warehouse_geometry_rows.v0",
        CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION,
        &instance,
    );
}

#[test]
fn warehouse_geometry_schema_matches_a_real_instance() {
    let artifact = materialize_warehouse_geometry(&warehouse_geometry_request())
        .expect("warehouse geometry materializes");
    let instance = serde_json::to_value(&artifact).expect("warehouse geometry serializes");
    assert_drift_free(
        WAREHOUSE_GEOMETRY_SCHEMA,
        "canon.geo.warehouse_geometry.v0",
        "canon_geo_warehouse_geometry.v0",
        &instance,
    );
}

#[test]
fn tile_work_request_schema_matches_a_real_instance() {
    let request = tile_work_request();
    let instance = serde_json::to_value(&request).expect("tile-work request must serialize");
    assert_drift_free(
        TILE_WORK_REQUEST_SCHEMA,
        "canon.geo.tile_work_request.v0",
        CANON_GEO_TILE_WORK_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn tile_work_unit_schema_matches_a_real_instance() {
    let artifact =
        materialize_tile_work_unit(&tile_work_request()).expect("tile work unit materializes");
    let instance = serde_json::to_value(&artifact).expect("tile work unit must serialize");
    assert_drift_free(
        TILE_WORK_UNIT_SCHEMA,
        "canon.geo.tile_work_unit.v0",
        "canon_geo_tile_work_unit.v0",
        &instance,
    );
}

#[test]
fn tile_reconciliation_request_schema_matches_a_real_instance() {
    let request = tile_reconciliation_request();
    let instance = serde_json::to_value(&request).expect("tile reconciliation request serializes");
    assert_drift_free(
        TILE_RECONCILIATION_REQUEST_SCHEMA,
        "canon.geo.tile_reconciliation_request.v0",
        CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn tile_reconciliation_schema_matches_a_real_instance() {
    let artifact = reconcile_tile_decisions(&tile_reconciliation_request())
        .expect("tile reconciliation succeeds");
    let instance = serde_json::to_value(&artifact).expect("tile reconciliation must serialize");
    assert_drift_free(
        TILE_RECONCILIATION_SCHEMA,
        "canon.geo.tile_reconciliation.v0",
        "canon_geo_tile_reconciliation.v0",
        &instance,
    );
}

#[test]
fn multisource_request_schema_matches_a_real_instance() {
    let temp = tempfile::tempdir().expect("tempdir");
    let request = multisource_request(temp.path());
    let instance = serde_json::to_value(&request).expect("multisource request must serialize");
    assert_drift_free(
        MULTISOURCE_REQUEST_SCHEMA,
        "canon.geo.multisource_request.v0",
        CANON_GEO_MULTISOURCE_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn multisource_artifact_schema_matches_a_real_instance() {
    let temp = tempfile::tempdir().expect("tempdir");
    let request = multisource_request(temp.path());
    let artifact = materialize_geo_multisource(&request, &temp.path().join("rows.csv"))
        .expect("multisource request materializes");
    let instance = serde_json::to_value(&artifact).expect("multisource artifact serializes");
    assert_drift_free(
        MULTISOURCE_ARTIFACT_SCHEMA,
        "canon.entity.multisource_link.v1",
        "canon_entity_multisource_link.v1",
        &instance,
    );
}

#[test]
fn composition_artifact_schema_matches_a_real_instance() {
    let artifact = solve_composition(&composition_request()).expect("request must solve");
    let instance = serde_json::to_value(&artifact).expect("artifact must serialize");
    assert_drift_free(
        COMPOSITION_SCHEMA,
        "canon.geo.composition.v0",
        "canon_geo_composition.v0",
        &instance,
    );
}

#[test]
fn evidence_request_schema_matches_a_real_instance() {
    let request = evidence_request();
    let instance = serde_json::to_value(&request).expect("request must serialize");
    assert_drift_free(
        EVIDENCE_REQUEST_SCHEMA,
        "canon.geo.evidence_request.v0",
        "canon_geo_evidence_request.v0",
        &instance,
    );
}

#[test]
fn warehouse_rows_schema_matches_a_real_instance() {
    let rows = warehouse_rows_request();
    let instance = serde_json::to_value(&rows).expect("warehouse rows must serialize");
    assert_drift_free(
        WAREHOUSE_ROWS_SCHEMA,
        "canon.geo.warehouse_rows.v0",
        CANON_GEO_WAREHOUSE_ROWS_VERSION,
        &instance,
    );
}

#[test]
fn evidence_compilation_artifact_schema_matches_a_real_instance() {
    let artifact = compile_evidence(&evidence_request()).expect("evidence must compile");
    let instance = serde_json::to_value(&artifact).expect("artifact must serialize");
    assert_drift_free(
        EVIDENCE_COMPILATION_SCHEMA,
        "canon.geo.evidence_compilation.v0",
        "canon_geo_evidence_compilation.v0",
        &instance,
    );
}

#[test]
fn population_request_schema_matches_a_real_instance() {
    let request = GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "case-1".to_string(),
            evidence: evidence_request(),
            truth: GeoCompositionModel {
                parcels: vec!["parcel-a".to_string()],
                buildings: Vec::new(),
            },
        }],
        max_cases: 8,
    };
    let instance = serde_json::to_value(&request).expect("request must serialize");
    assert_drift_free(
        POPULATION_REQUEST_SCHEMA,
        "canon.geo.population_request.v0",
        "canon_geo_population_request.v0",
        &instance,
    );
}

#[test]
fn population_evaluation_artifact_schema_matches_a_real_instance() {
    let request = GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "case-1".to_string(),
            evidence: evidence_request(),
            truth: GeoCompositionModel {
                parcels: vec!["parcel-a".to_string()],
                buildings: Vec::new(),
            },
        }],
        max_cases: 8,
    };
    let artifact = evaluate_population(&request).expect("population must evaluate");
    let instance = serde_json::to_value(&artifact).expect("artifact must serialize");
    assert_drift_free(
        POPULATION_EVALUATION_SCHEMA,
        "canon.geo.population_evaluation.v0",
        "canon_geo_population_evaluation.v0",
        &instance,
    );
}
