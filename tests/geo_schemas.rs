//! Schema-drift guard for the seven registered Geo contracts.
//!
//! For each contract this test: (a) pins the schema file's `title` and
//! `properties.version.const`, and asserts top-level `additionalProperties`
//! is `false`; (b) builds a real instance through the library API, serializes
//! it with `serde_json`, and walks every key present in the serialized value
//! to confirm the schema declares it somewhere reachable from the root
//! object (`properties`, `$defs`, `$ref`, array `items`, and `oneOf`
//! alternatives for tagged enums). This does not add a `jsonschema`
//! dependency; it only catches keys the schema forgot to declare.

use canon::geo::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, CANON_GEO_EVIDENCE_REQUEST_VERSION,
    CANON_GEO_POPULATION_REQUEST_VERSION, CANON_GEO_WAREHOUSE_ROWS_VERSION,
    DEFAULT_MAX_MATERIALIZED_MODELS, GeoBuildingCandidate, GeoCompositionModel,
    GeoCompositionRequest, GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef,
    GeoEvidenceClaimRole, GeoEvidenceCompilationRequest, GeoEvidenceRecordRef, GeoHardConstraint,
    GeoHardConstraintKind, GeoLabeledCompositionCase, GeoPopulationEvaluationRequest, GeoRhoBasis,
    GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind, GeoWarehouseEvidenceRow,
    GeoWarehouseParcelRow, GeoWarehouseRowsRequest, compile_evidence, evaluate_population,
    solve_composition,
};
use serde_json::Value;

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
        let resolved = resolve_ref(root, reference);
        assert_instance_matches_schema(root, resolved, instance, path);
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
