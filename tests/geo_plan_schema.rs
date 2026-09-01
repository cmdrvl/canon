#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_QUESTION_VERSION, CANON_GEO_REGIONAL_INVENTORY_VERSION,
    CANON_GEO_RESOURCE_BUDGET_VERSION, GeoAbstentionDisposition, GeoAbstentionPolicy,
    GeoBoundedGeography, GeoBudgetAction, GeoClaimClass, GeoCompositionProfile,
    GeoControlEntityLevel, GeoCoveragePredicate, GeoDigestAlgorithm, GeoEgressClass,
    GeoEvidenceClass, GeoGeometryTransformContract, GeoIdentityParticipation, GeoLicenseClass,
    GeoLocalAcquisitionState, GeoLocalArtifactRef, GeoNativeEntityScope, GeoNumericBound,
    GeoNumericMeasure, GeoPlan, GeoPlanExternalRequest, GeoPlanRequest, GeoQuestion,
    GeoRegionalInventory, GeoRegionalSourceInstance, GeoRequestedGrain, GeoResourceBudget,
    GeoResourceCounter, GeoSourceAvailability, GeoSourceRelease, GeoSubjectBinding,
    GeoSubjectBindingClass, GeoTelemetryDeclaration, GeoTelemetryMetric,
    GeoTelemetrySemanticEffect, GeoTemporalScope, GeoValueOrigin, compile_geo_plan,
    default_geo_capabilities, validate_geo_plan,
};
use serde_json::Value;
use std::collections::BTreeSet;

const GEO_PLAN_SCHEMA_JSON: &str = include_str!("../schemas/canon.geo.plan.v0.schema.json");
const PROJECT_PLAN_SCHEMA_JSON: &str = include_str!("../schemas/canon.project.plan.v1.schema.json");
const ACQUISITION_REQUEST_SCHEMA_JSON: &str =
    include_str!("../schemas/canon.geo.acquisition_request.v0.schema.json");
const DISCOVERY_REQUEST_SCHEMA_JSON: &str =
    include_str!("../schemas/canon.geo.discovery_request.v0.schema.json");

#[test]
fn schema_declares_strict_geo_plan_overlay_contract() {
    let schema = schema();
    assert_eq!(
        schema.get("$schema").and_then(Value::as_str),
        Some("https://json-schema.org/draft/2020-12/schema")
    );
    assert_eq!(
        schema.get("title").and_then(Value::as_str),
        Some("canon.geo.plan.v0")
    );
    assert_eq!(
        schema
            .pointer("/properties/version/const")
            .and_then(Value::as_str),
        Some("canon_geo_plan.v0")
    );
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(count_project_plan_refs(&schema), 1);
    assert_eq!(
        schema
            .pointer("/properties/project_plan/$ref")
            .and_then(Value::as_str),
        Some("canon.project.plan.v1.schema.json")
    );
    assert_eq!(
        schema
            .pointer("/$defs/blake3_digest/pattern")
            .and_then(Value::as_str),
        Some("^blake3:[0-9a-f]{64}$")
    );
    assert_eq!(
        schema
            .pointer("/$defs/plan_id/pattern")
            .and_then(Value::as_str),
        Some("^canon_geo_plan\\.v0:[0-9a-f]{64}$")
    );
    assert!(
        !schema_enum_values(&schema, "/$defs/plan_stage/enum").contains("validate_intent"),
        "schema stage enum must stay faithful to GeoPlanStage"
    );
    assert!(
        schema_enum_values(&schema, "/$defs/gate_status/enum").contains("passed_against_reference")
    );
    assert!(
        schema_enum_values(&schema, "/$defs/gate_status/enum").contains("failed_against_reference")
    );
    assert!(
        required_values(&schema, "/$defs/geo_node_overlay/required")
            .contains("deterministic_bounds")
    );
    assert!(
        required_values(&schema, "/$defs/geo_node_overlay/required")
            .contains("cost_estimate_ranges")
    );
    assert!(
        required_values(&schema, "/$defs/exact_solve_scope/required")
            .contains("component_key_field")
    );
    assert_eq!(
        schema
            .pointer("/$defs/exact_solve_scope/properties/component_key_field/const")
            .and_then(Value::as_str),
        Some("canon_geo_composition.v0.factorization[].key")
    );
    assert!(
        required_values(&schema, "/$defs/acquisition_handoff/required")
            .contains("expected_receipt_contract")
    );
    assert!(
        required_values(&schema, "/$defs/acquisition_handoff/required")
            .contains("required_result_digest_algorithm")
    );
    assert!(
        required_values(&schema, "/$defs/acquisition_handoff/required")
            .contains("continuation_command")
    );
    assert_eq!(
        schema
            .pointer("/$defs/acquisition_handoff/properties/expected_receipt_contract/const")
            .and_then(Value::as_str),
        Some("canon_geo_acquisition_receipt.v0")
    );
    assert_eq!(
        schema
            .pointer("/$defs/acquisition_handoff/properties/required_result_digest_algorithm/const")
            .and_then(Value::as_str),
        Some("blake3")
    );
    assert_eq!(
        schema
            .pointer("/$defs/acquisition_handoff/properties/continuation_command/const")
            .and_then(Value::as_str),
        Some(
            "canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>"
        )
    );
    assert_eq!(
        external_request_kinds(&schema),
        BTreeSet::from(["acquisition", "discovery", "discovery_gap"])
    );
    assert_eq!(
        schema
            .pointer("/x-canon-contract/offline")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        schema
            .pointer("/x-canon-contract/exactly_one_embedded_project_plan_dag")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        schema
            .pointer("/x-canon-contract/telemetry_control_fallback")
            .and_then(Value::as_bool),
        None,
        "telemetry must not be a semantic-control knob"
    );
    assert_object_schemas_are_closed(&schema, "$");
}

#[test]
fn compiled_geo_plan_serializes_to_schema_shape() {
    let plan = building_plan();
    validate_geo_plan(&plan).expect("compiled plan validates");

    let instance = serde_json::to_value(&plan).expect("plan serializes");
    assert_schema_accepts(&instance);
    assert_eq!(instance["version"], "canon_geo_plan.v0");
    assert_eq!(
        instance["project_plan"]["schema_version"],
        "canon.project.plan.v1"
    );
    assert_eq!(instance["status"], "planned");
    assert!(is_lower_blake3(
        instance["semantic_hash"].as_str().expect("semantic hash")
    ));
    assert!(
        serde_json::from_value::<GeoPlan>(instance).is_ok(),
        "schema-positive instance must remain serde-readable"
    );
}

#[test]
fn schema_accepts_acquisition_external_request_handoff() {
    let plan = acquisition_plan();
    validate_geo_plan(&plan).expect("acquisition plan validates");
    assert_eq!(plan.external_requests.len(), 1);
    let GeoPlanExternalRequest::Acquisition { request, handoff } = &plan.external_requests[0]
    else {
        panic!("expected acquisition request");
    };
    assert_eq!(request.positive_path_min_rows, 1);
    assert_eq!(
        handoff.expected_receipt_contract,
        "canon_geo_acquisition_receipt.v0"
    );
    assert_eq!(
        handoff.required_result_digest_algorithm,
        GeoDigestAlgorithm::Blake3
    );
    assert_eq!(
        handoff.continuation_command,
        "canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>"
    );

    let instance = serde_json::to_value(plan).expect("acquisition plan serializes");
    assert_schema_accepts(&instance);
}

#[test]
fn schema_rejects_invalid_acquisition_handoff() {
    let mut missing = serde_json::to_value(acquisition_plan()).expect("plan serializes");
    acquisition_request_mut(&mut missing)
        .as_object_mut()
        .expect("acquisition request object")
        .remove("handoff");
    assert_schema_rejects(&missing, "oneOf matched 0 alternatives");

    let mut wrong_receipt = serde_json::to_value(acquisition_plan()).expect("plan serializes");
    acquisition_handoff_mut(&mut wrong_receipt)["expected_receipt_contract"] =
        Value::String("canon_geo_acquisition_receipt.v1".to_string());
    assert_schema_rejects(&wrong_receipt, "oneOf matched 0 alternatives");
    assert!(
        validate_geo_plan(
            &serde_json::from_value::<GeoPlan>(wrong_receipt)
                .expect("serde reads wrong receipt contract strings")
        )
        .is_err(),
        "planner validator rejects wrong acquisition receipt handoff"
    );

    let mut wrong_digest = serde_json::to_value(acquisition_plan()).expect("plan serializes");
    acquisition_handoff_mut(&mut wrong_digest)["required_result_digest_algorithm"] =
        Value::String("sha256".to_string());
    assert_schema_rejects(&wrong_digest, "oneOf matched 0 alternatives");
    assert!(
        validate_geo_plan(
            &serde_json::from_value::<GeoPlan>(wrong_digest)
                .expect("serde reads wrong digest algorithm")
        )
        .is_err(),
        "planner validator rejects non-BLAKE3 acquisition result digest handoff"
    );

    let mut wrong_continuation = serde_json::to_value(acquisition_plan()).expect("plan serializes");
    acquisition_handoff_mut(&mut wrong_continuation)["continuation_command"] =
        Value::String("canon geo plan".to_string());
    assert_schema_rejects(&wrong_continuation, "oneOf matched 0 alternatives");
    assert!(
        validate_geo_plan(
            &serde_json::from_value::<GeoPlan>(wrong_continuation)
                .expect("serde reads wrong continuation command")
        )
        .is_err(),
        "planner validator rejects shortened acquisition continuation handoff"
    );
}

#[test]
fn schema_rejects_unbounded_solve() {
    let mut instance = serde_json::to_value(building_plan()).expect("plan serializes");
    let solve = solve_overlay_mut(&mut instance);
    solve["bounded_section_required"] = Value::Bool(false);
    solve["incidence_factorization_required"] = Value::Bool(false);

    assert_schema_rejects(&instance, "const Bool(true)");
    let typed: GeoPlan = serde_json::from_value(instance).expect("serde still reads booleans");
    assert!(
        validate_geo_plan(&typed).is_err(),
        "planner validator also rejects unbounded exact solve nodes"
    );
}

#[test]
fn schema_rejects_drifted_component_key_field() {
    let mut instance = serde_json::to_value(building_plan()).expect("plan serializes");
    solve_overlay_mut(&mut instance)["exact_solve_scope"]["component_key_field"] =
        Value::String("canon_geo_composition.v0.factorization[].component_id".to_string());

    assert_schema_rejects(&instance, "canon_geo_composition.v0.factorization[].key");
    let typed: GeoPlan =
        serde_json::from_value(instance).expect("serde still reads component key strings");
    assert!(
        validate_geo_plan(&typed).is_err(),
        "planner validator fixes the component key field to the composition factorization key"
    );
}

#[test]
fn schema_rejects_solve_before_reach_or_factor() {
    let mut instance = serde_json::to_value(building_plan()).expect("plan serializes");
    let solve = solve_overlay_mut(&mut instance);
    let preconditions = solve["preconditions"]
        .as_array_mut()
        .expect("solve preconditions array");
    preconditions.retain(|precondition| precondition["plane"] != "candidate_reach");

    assert_schema_rejects(&instance, "contains");

    let mut missing_scope = serde_json::to_value(building_plan()).expect("plan serializes");
    solve_overlay_mut(&mut missing_scope)
        .as_object_mut()
        .expect("solve overlay object")
        .remove("exact_solve_scope");
    assert_schema_rejects(&missing_scope, "missing required field exact_solve_scope");
}

#[test]
fn schema_preserves_supported_grains_when_parcel_is_unsupported() {
    let mut instance = serde_json::to_value(building_plus_unsupported_parcel_plan())
        .expect("partial plan serializes");
    assert_eq!(instance["status"], "partial");
    assert_schema_accepts(&instance);
    assert!(
        instance["grain_outcomes"]
            .as_array()
            .expect("grain outcomes")
            .iter()
            .any(|outcome| outcome["entity_level"] == "building"
                && outcome["status"] == "planned_relative_to_declared_universe")
    );
    assert!(
        instance["grain_outcomes"]
            .as_array()
            .expect("grain outcomes")
            .iter()
            .any(|outcome| outcome["entity_level"] == "parcel"
                && outcome["status"] != "planned_relative_to_declared_universe")
    );

    instance["status"] = Value::String("unsupported".to_string());
    assert_schema_rejects(&instance, "not");
}

#[test]
fn schema_rejects_telemetry_as_semantic_identity_control() {
    let schema = schema();
    let excluded = schema
        .pointer("/x-canon-contract/semantic_identity_excludes")
        .and_then(Value::as_array)
        .expect("semantic exclusion list");
    assert!(excluded.iter().any(|value| value == "telemetry"));
    assert!(excluded.iter().any(|value| value == "cpu_time"));
    assert!(excluded.iter().any(|value| value == "currency_cost"));

    let mut instance = serde_json::to_value(building_plan()).expect("plan serializes");
    solve_overlay_mut(&mut instance)["cost_estimate_ranges"][0]["semantic_effect"] =
        Value::String("controls_planning_semantics".to_string());
    assert_schema_rejects(&instance, "enum");
}

#[test]
fn schema_rejects_non_lowercase_or_wrong_width_digests() {
    let mut uppercase = serde_json::to_value(building_plan()).expect("plan serializes");
    uppercase["semantic_hash"] = Value::String(
        "blake3:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
    );
    assert_schema_rejects(&uppercase, "pattern ^blake3:[0-9a-f]{64}$");

    let mut short_plan_id = serde_json::to_value(building_plan()).expect("plan serializes");
    short_plan_id["plan_id"] = Value::String("canon_geo_plan.v0:abc123".to_string());
    assert_schema_rejects(&short_plan_id, "pattern ^canon_geo_plan\\.v0:[0-9a-f]{64}$");
}

#[test]
fn schema_rejects_unknown_fields() {
    let mut instance = serde_json::to_value(building_plan()).expect("plan serializes");
    instance["unexpected"] = Value::Bool(true);
    assert_schema_rejects(&instance, "additional property");

    let mut nested = serde_json::to_value(building_plan()).expect("plan serializes");
    nested["geo_nodes"][0]["unexpected"] = Value::Bool(true);
    assert_schema_rejects(&nested, "additional property");
}

fn building_plan() -> GeoPlan {
    compile_geo_plan(GeoPlanRequest {
        question: building_question(),
        capabilities: default_geo_capabilities().expect("default capabilities"),
        inventory: building_inventory(),
        profile: GeoCompositionProfile::building(),
        budget: budget(),
    })
    .expect("building plan compiles")
}

fn building_plus_unsupported_parcel_plan() -> GeoPlan {
    let mut question = building_question();
    question.question_id = "question.fixture.building-plus-parcel".to_string();
    question.requested_grains.push(GeoRequestedGrain {
        entity_level: GeoControlEntityLevel::Parcel,
        required_evidence_classes: vec![GeoEvidenceClass::ParcelGeometry],
        optional_evidence_classes: Vec::new(),
    });
    compile_geo_plan(GeoPlanRequest {
        question,
        capabilities: default_geo_capabilities().expect("default capabilities"),
        inventory: building_inventory(),
        profile: GeoCompositionProfile::building(),
        budget: budget(),
    })
    .expect("partial plan compiles")
}

fn acquisition_plan() -> GeoPlan {
    compile_geo_plan(GeoPlanRequest {
        question: building_question(),
        capabilities: default_geo_capabilities().expect("default capabilities"),
        inventory: remote_building_inventory(),
        profile: GeoCompositionProfile::building(),
        budget: acquisition_budget(),
    })
    .expect("acquisition plan compiles")
}

fn building_question() -> GeoQuestion {
    GeoQuestion {
        version: CANON_GEO_QUESTION_VERSION.to_string(),
        question_id: "question.fixture.building".to_string(),
        subject_bindings: vec![GeoSubjectBinding {
            role: "operator_case".to_string(),
            binding_class: GeoSubjectBindingClass::OperatorLabel,
            value: "case-building".to_string(),
        }],
        bounded_geography: region(),
        requested_grains: vec![GeoRequestedGrain {
            entity_level: GeoControlEntityLevel::Building,
            required_evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
            optional_evidence_classes: vec![GeoEvidenceClass::AddressSet],
        }],
        query_as_of: None,
        requested_claim_classes: vec![GeoClaimClass::StableIdentity, GeoClaimClass::CandidateReach],
        presentation_limits: vec![GeoNumericBound {
            semantic_id: "question.presentation.max_models".to_string(),
            counter: GeoResourceCounter::Models,
            value: 16,
            unit: "model".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
            action: GeoBudgetAction::TruncatePresentationOnly,
        }],
        abstention_policy: GeoAbstentionPolicy {
            unsupported_grain: GeoAbstentionDisposition::ReportUnsupported,
            unresolved_residual: GeoAbstentionDisposition::ReportResidual,
            budget_fallback: GeoAbstentionDisposition::ReportResidual,
        },
        decision_policy: None,
        resource_budget_ref: "budget.fixture.geo-plan".to_string(),
    }
}

fn building_inventory() -> GeoRegionalInventory {
    GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.fixture.geo-plan".to_string(),
        region: region(),
        sources: vec![GeoRegionalSourceInstance {
            source_instance_id: "source.fixture.buildings".to_string(),
            release: GeoSourceRelease {
                release_id: "release.fixture.buildings".to_string(),
                release_digest: digest("release.fixture.buildings"),
            },
            temporal_scope: GeoTemporalScope {
                valid_time: None,
                transaction_time: None,
                release_time: None,
            },
            lineage_ids: vec!["lineage.fixture.buildings".to_string()],
            native_scope: GeoNativeEntityScope::NativeEntity {
                entity_level: GeoControlEntityLevel::Building,
                identity_participation: GeoIdentityParticipation::StableAlias,
            },
            evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
            coverage: GeoCoveragePredicate {
                coverage_id: "coverage.fixture.geo-plan".to_string(),
                region: region(),
                predicate: "all declared records in the fixture region".to_string(),
            },
            local_state: GeoLocalAcquisitionState {
                state: GeoSourceAvailability::Available,
                local_ref: Some(GeoLocalArtifactRef {
                    artifact_id: "local.fixture.buildings".to_string(),
                    contract_version: "canon_geo_warehouse_rows.v0".to_string(),
                    content_hash: digest("local.fixture.buildings"),
                    media_type: "application/json".to_string(),
                }),
            },
            geometry: None,
            license_class: GeoLicenseClass::PublicRedistributable,
            egress_class: GeoEgressClass::Shareable,
            estimates: vec![GeoNumericMeasure {
                semantic_id: "source.estimated_rows".to_string(),
                value: 2,
                unit: "row".to_string(),
                origin: GeoValueOrigin::SourceRelease,
            }],
        }],
        discovery_gaps: Vec::new(),
    }
}

fn remote_building_inventory() -> GeoRegionalInventory {
    let mut inventory = building_inventory();
    let source = inventory
        .sources
        .first_mut()
        .expect("fixture inventory has one source");
    source.source_instance_id = "source.fixture.remote-buildings".to_string();
    source.local_state = GeoLocalAcquisitionState {
        state: GeoSourceAvailability::Missing,
        local_ref: None,
    };
    source.geometry = Some(GeoGeometryTransformContract {
        geometry_contract_version: "geometry.fixture.v1".to_string(),
        coordinate_reference_system: "EPSG:4326".to_string(),
        transform_id: "identity.fixture".to_string(),
        transform_digest: digest("identity.fixture"),
        numeric_error_bounds: vec![GeoNumericMeasure {
            semantic_id: "transform.error".to_string(),
            value: 0,
            unit: "millimeter".to_string(),
            origin: GeoValueOrigin::AdapterContract,
        }],
    });
    inventory
}

fn budget() -> GeoResourceBudget {
    GeoResourceBudget {
        version: CANON_GEO_RESOURCE_BUDGET_VERSION.to_string(),
        budget_id: "budget.fixture.geo-plan".to_string(),
        deterministic_bounds: vec![
            GeoNumericBound {
                semantic_id: "budget.rows".to_string(),
                counter: GeoResourceCounter::Rows,
                value: 100,
                unit: "row".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::RefuseBeforeWork,
            },
            GeoNumericBound {
                semantic_id: "budget.bytes".to_string(),
                counter: GeoResourceCounter::Bytes,
                value: 1_000_000,
                unit: "byte".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::RefuseBeforeWork,
            },
            GeoNumericBound {
                semantic_id: "budget.candidates".to_string(),
                counter: GeoResourceCounter::Candidates,
                value: 128,
                unit: "candidate".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::ReportBudgetFallback,
            },
            GeoNumericBound {
                semantic_id: "budget.cells".to_string(),
                counter: GeoResourceCounter::Cells,
                value: 64,
                unit: "cell".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::ReportBudgetFallback,
            },
            GeoNumericBound {
                semantic_id: "budget.variables".to_string(),
                counter: GeoResourceCounter::Variables,
                value: 512,
                unit: "variable".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::ReportBudgetFallback,
            },
            GeoNumericBound {
                semantic_id: "budget.states".to_string(),
                counter: GeoResourceCounter::States,
                value: 65_536,
                unit: "state".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::ReportBudgetFallback,
            },
            GeoNumericBound {
                semantic_id: "budget.models".to_string(),
                counter: GeoResourceCounter::Models,
                value: 16,
                unit: "model".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::ReportBudgetFallback,
            },
            GeoNumericBound {
                semantic_id: "budget.operations".to_string(),
                counter: GeoResourceCounter::Operations,
                value: 1_000_000,
                unit: "operation".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::ReportBudgetFallback,
            },
            GeoNumericBound {
                semantic_id: "budget.proof_bytes".to_string(),
                counter: GeoResourceCounter::ProofBytes,
                value: 65_536,
                unit: "byte".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::TruncatePresentationOnly,
            },
        ],
        telemetry: vec![
            GeoTelemetryDeclaration {
                metric: GeoTelemetryMetric::WallTime,
                unit: "millisecond".to_string(),
                origin: GeoValueOrigin::OperatorPolicy,
                semantic_effect: GeoTelemetrySemanticEffect::None,
            },
            GeoTelemetryDeclaration {
                metric: GeoTelemetryMetric::CurrencyCost,
                unit: "usd_cent".to_string(),
                origin: GeoValueOrigin::OperatorPolicy,
                semantic_effect: GeoTelemetrySemanticEffect::None,
            },
        ],
    }
}

fn acquisition_budget() -> GeoResourceBudget {
    GeoResourceBudget {
        version: CANON_GEO_RESOURCE_BUDGET_VERSION.to_string(),
        budget_id: "budget.fixture.geo-plan".to_string(),
        deterministic_bounds: vec![
            GeoNumericBound {
                semantic_id: "budget.rows".to_string(),
                counter: GeoResourceCounter::Rows,
                value: 100,
                unit: "row".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::RefuseBeforeWork,
            },
            GeoNumericBound {
                semantic_id: "budget.bytes".to_string(),
                counter: GeoResourceCounter::Bytes,
                value: 1_000_000,
                unit: "byte".to_string(),
                origin: GeoValueOrigin::CallerDeclared,
                action: GeoBudgetAction::RefuseBeforeWork,
            },
        ],
        telemetry: Vec::new(),
    }
}

fn region() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: "region.fixture.geo-plan".to_string(),
        geography_kind: "fixture_region".to_string(),
        description: "fixture bounded geography for plan schema tests".to_string(),
    }
}

fn digest(input: &str) -> String {
    format!("blake3:{}", blake3::hash(input.as_bytes()).to_hex())
}

fn solve_overlay_mut(instance: &mut Value) -> &mut Value {
    instance["geo_nodes"]
        .as_array_mut()
        .expect("geo nodes")
        .iter_mut()
        .find(|node| node["stage"] == "factor_and_solve_exact_residual")
        .expect("solve overlay")
}

fn acquisition_request_mut(instance: &mut Value) -> &mut Value {
    instance["external_requests"]
        .as_array_mut()
        .expect("external requests")
        .iter_mut()
        .find(|request| request["kind"] == "acquisition")
        .expect("acquisition external request")
}

fn acquisition_handoff_mut(instance: &mut Value) -> &mut Value {
    &mut acquisition_request_mut(instance)["handoff"]
}

fn schema() -> Value {
    serde_json::from_str(GEO_PLAN_SCHEMA_JSON).expect("geo plan schema parses")
}

fn assert_schema_accepts(instance: &Value) {
    let errors = schema_errors(instance);
    assert!(
        errors.is_empty(),
        "expected schema acceptance, got {errors:#?}"
    );
}

fn assert_schema_rejects(instance: &Value, expected: &str) {
    let errors = schema_errors(instance);
    assert!(
        !errors.is_empty(),
        "expected schema rejection containing {expected:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains(expected)),
        "expected an error containing {expected:?}, got {errors:#?}"
    );
}

fn schema_errors(instance: &Value) -> Vec<String> {
    let root = schema();
    let mut errors = Vec::new();
    validate_schema_node(&root, &root, instance, "$", &mut errors);
    errors
}

fn validate_schema_node(
    root: &Value,
    subschema: &Value,
    instance: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    if let Some(reference) = subschema.get("$ref").and_then(Value::as_str) {
        validate_ref(root, reference, instance, path, errors);
        return;
    }

    if let Some(all_of) = subschema.get("allOf").and_then(Value::as_array) {
        for part in all_of {
            validate_schema_node(root, part, instance, path, errors);
        }
    }

    if let Some(condition) = subschema.get("if") {
        let mut condition_errors = Vec::new();
        validate_schema_node(root, condition, instance, path, &mut condition_errors);
        if condition_errors.is_empty()
            && let Some(then_schema) = subschema.get("then")
        {
            validate_schema_node(root, then_schema, instance, path, errors);
        }
    }

    if let Some(rejected) = subschema.get("not") {
        let mut rejected_errors = Vec::new();
        validate_schema_node(root, rejected, instance, path, &mut rejected_errors);
        if rejected_errors.is_empty() {
            errors.push(format!("{path}: not matched a forbidden subschema"));
        }
    }

    if let Some(options) = subschema.get("oneOf").and_then(Value::as_array) {
        let matched = options
            .iter()
            .filter(|option| {
                let mut option_errors = Vec::new();
                validate_schema_node(root, option, instance, path, &mut option_errors);
                option_errors.is_empty()
            })
            .count();
        if matched != 1 {
            errors.push(format!("{path}: oneOf matched {matched} alternatives"));
        }
        return;
    }

    if let Some(expected) = subschema.get("const")
        && instance != expected
    {
        errors.push(format!(
            "{path}: const {expected:?} did not match {instance:?}"
        ));
    }
    if let Some(values) = subschema.get("enum").and_then(Value::as_array)
        && !values.iter().any(|value| value == instance)
    {
        errors.push(format!("{path}: enum did not contain {instance:?}"));
    }

    validate_type(subschema, instance, path, errors);
    validate_string(subschema, instance, path, errors);
    validate_number(subschema, instance, path, errors);
    validate_object(root, subschema, instance, path, errors);
    validate_array(root, subschema, instance, path, errors);
}

fn validate_ref(
    root: &Value,
    reference: &str,
    instance: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    if let Some(pointer) = reference.strip_prefix('#') {
        let resolved = root
            .pointer(pointer)
            .unwrap_or_else(|| panic!("local ref {reference} resolves"));
        validate_schema_node(root, resolved, instance, path, errors);
        return;
    }

    let (schema_file, fragment) = reference
        .split_once('#')
        .map_or((reference, ""), |(schema_file, fragment)| {
            (schema_file, fragment)
        });
    let external_root: Value = serde_json::from_str(match schema_file {
        "canon.project.plan.v1.schema.json" => PROJECT_PLAN_SCHEMA_JSON,
        "canon.geo.acquisition_request.v0.schema.json" => ACQUISITION_REQUEST_SCHEMA_JSON,
        "canon.geo.discovery_request.v0.schema.json" => DISCOVERY_REQUEST_SCHEMA_JSON,
        _ => panic!("unregistered external schema {reference}"),
    })
    .expect("external schema parses");
    let resolved = if fragment.is_empty() {
        &external_root
    } else {
        external_root
            .pointer(fragment)
            .unwrap_or_else(|| panic!("external ref {reference} resolves"))
    };
    validate_schema_node(&external_root, resolved, instance, path, errors);
}

fn validate_type(subschema: &Value, instance: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(expected) = subschema.get("type").and_then(Value::as_str) else {
        return;
    };
    let matched = match expected {
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "boolean" => instance.is_boolean(),
        "integer" => instance.as_u64().is_some() || instance.as_i64().is_some(),
        "null" => instance.is_null(),
        other => panic!("unsupported schema type {other} at {path}"),
    };
    if !matched {
        errors.push(format!(
            "{path}: type {expected} did not match {instance:?}"
        ));
    }
}

fn validate_string(subschema: &Value, instance: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(value) = instance.as_str() else {
        return;
    };
    if let Some(minimum) = subschema.get("minLength").and_then(Value::as_u64)
        && value.len() < minimum as usize
    {
        errors.push(format!("{path}: minLength {minimum} failed"));
    }
    if let Some(maximum) = subschema.get("maxLength").and_then(Value::as_u64)
        && value.len() > maximum as usize
    {
        errors.push(format!("{path}: maxLength {maximum} failed"));
    }
    if let Some(pattern) = subschema.get("pattern").and_then(Value::as_str)
        && !matches_schema_pattern(pattern, value)
    {
        errors.push(format!("{path}: pattern {pattern} failed for {value:?}"));
    }
}

fn validate_number(subschema: &Value, instance: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(minimum) = subschema.get("minimum").and_then(Value::as_i64) else {
        return;
    };
    let Some(value) = instance
        .as_i64()
        .or_else(|| instance.as_u64().map(|value| value as i64))
    else {
        return;
    };
    if value < minimum {
        errors.push(format!("{path}: minimum {minimum} failed"));
    }
}

fn validate_object(
    root: &Value,
    subschema: &Value,
    instance: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    let Some(object) = instance.as_object() else {
        return;
    };
    if let Some(required) = subschema.get("required").and_then(Value::as_array) {
        for field in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(field) {
                errors.push(format!("{path}: missing required field {field}"));
            }
        }
    }

    let properties = subschema.get("properties").and_then(Value::as_object);
    for (key, value) in object {
        if let Some(property_schema) = properties.and_then(|properties| properties.get(key)) {
            validate_schema_node(
                root,
                property_schema,
                value,
                &format!("{path}.{key}"),
                errors,
            );
            continue;
        }
        match subschema.get("additionalProperties") {
            Some(Value::Bool(false)) => {
                errors.push(format!("{path}: additional property {key}"));
            }
            Some(extra_schema) if extra_schema.is_object() => {
                validate_schema_node(root, extra_schema, value, &format!("{path}.{key}"), errors);
            }
            _ => {}
        }
    }
}

fn validate_array(
    root: &Value,
    subschema: &Value,
    instance: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    let Some(array) = instance.as_array() else {
        return;
    };
    if let Some(minimum) = subschema.get("minItems").and_then(Value::as_u64)
        && array.len() < minimum as usize
    {
        errors.push(format!("{path}: minItems {minimum} failed"));
    }
    if subschema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
        let distinct = array
            .iter()
            .map(|value| serde_json::to_string(value).expect("JSON value serializes"))
            .collect::<BTreeSet<_>>();
        if distinct.len() != array.len() {
            errors.push(format!("{path}: uniqueItems failed"));
        }
    }
    if let Some(items) = subschema.get("items") {
        for (index, value) in array.iter().enumerate() {
            validate_schema_node(root, items, value, &format!("{path}[{index}]"), errors);
        }
    }
    if let Some(contains) = subschema.get("contains") {
        let matches = array
            .iter()
            .filter(|value| {
                let mut contains_errors = Vec::new();
                validate_schema_node(root, contains, value, path, &mut contains_errors);
                contains_errors.is_empty()
            })
            .count();
        let minimum = subschema
            .get("minContains")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        if matches < minimum as usize {
            errors.push(format!(
                "{path}: contains matched {matches}, below minContains {minimum}"
            ));
        }
    }
}

fn matches_schema_pattern(pattern: &str, value: &str) -> bool {
    match pattern {
        "^blake3:[0-9a-f]{64}$" => is_lower_blake3(value),
        "^canon_geo_plan\\.v0:[0-9a-f]{64}$" => value
            .strip_prefix("canon_geo_plan.v0:")
            .is_some_and(is_lower_64_hex),
        "^canon_geo_acquisition_request\\.v0:[0-9a-f]{64}$" => value
            .strip_prefix("canon_geo_acquisition_request.v0:")
            .is_some_and(is_lower_64_hex),
        "^canon_geo_discovery_request\\.v0:[0-9a-f]{64}$" => value
            .strip_prefix("canon_geo_discovery_request.v0:")
            .is_some_and(is_lower_64_hex),
        "^[A-Za-z0-9][A-Za-z0-9._:/@+-]{0,127}$" => bounded_id(value),
        "^[a-z0-9][a-z0-9._:-]{0,127}$" => bounded_project_node_id(value),
        "^canon(_|\\.)[A-Za-z0-9_.-]+\\.v[0-9]+$" => contract_version(value),
        "^[0-9a-f]+$" => !value.is_empty() && value.bytes().all(is_lower_hex_byte),
        "^[0-9a-f]{64}$" => is_lower_64_hex(value),
        "^[0-9a-f]{128}$" => value.len() == 128 && value.bytes().all(is_lower_hex_byte),
        "^[0-9]{4}-[0-9]{2}-[0-9]{2}$" => {
            value.len() == 10
                && value.bytes().enumerate().all(|(index, byte)| {
                    matches!(index, 4 | 7) == (byte == b'-')
                        && (byte == b'-' || byte.is_ascii_digit())
                })
        }
        "^blake3:[0-9a-fA-F]{64}$" => value
            .strip_prefix("blake3:")
            .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())),
        other => panic!("unsupported schema pattern {other}"),
    }
}

fn is_lower_blake3(value: &str) -> bool {
    value.strip_prefix("blake3:").is_some_and(is_lower_64_hex)
}

fn is_lower_64_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(is_lower_hex_byte)
}

fn is_lower_hex_byte(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn bounded_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'.' | b'_' | b':' | b'/' | b'@' | b'+' | b'-')
        })
}

fn bounded_project_node_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b':' | b'-')
        })
}

fn contract_version(value: &str) -> bool {
    (value.starts_with("canon_") || value.starts_with("canon."))
        && value.rsplit_once(".v").is_some_and(|(_, suffix)| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn count_project_plan_refs(value: &Value) -> usize {
    match value {
        Value::Object(object) => {
            let here = usize::from(
                object.get("$ref").and_then(Value::as_str)
                    == Some("canon.project.plan.v1.schema.json"),
            );
            here + object.values().map(count_project_plan_refs).sum::<usize>()
        }
        Value::Array(values) => values.iter().map(count_project_plan_refs).sum(),
        _ => 0,
    }
}

fn assert_object_schemas_are_closed(value: &Value, path: &str) {
    match value {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("object") {
                assert_eq!(
                    object.get("additionalProperties").and_then(Value::as_bool),
                    Some(false),
                    "{path} must set additionalProperties false"
                );
            }
            for (key, child) in object {
                assert_object_schemas_are_closed(child, &format!("{path}.{key}"));
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                assert_object_schemas_are_closed(child, &format!("{path}[{index}]"));
            }
        }
        _ => {}
    }
}

fn schema_enum_values<'a>(schema: &'a Value, pointer: &str) -> BTreeSet<&'a str> {
    schema
        .pointer(pointer)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("schema enum {pointer} exists"))
        .iter()
        .map(|value| value.as_str().expect("enum value is a string"))
        .collect()
}

fn required_values<'a>(schema: &'a Value, pointer: &str) -> BTreeSet<&'a str> {
    schema
        .pointer(pointer)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("required array {pointer} exists"))
        .iter()
        .map(|value| value.as_str().expect("required value is a string"))
        .collect()
}

fn external_request_kinds(schema: &Value) -> BTreeSet<&str> {
    schema
        .pointer("/$defs/external_request/oneOf")
        .and_then(Value::as_array)
        .expect("external request oneOf exists")
        .iter()
        .map(|variant| {
            variant
                .pointer("/properties/kind/const")
                .and_then(Value::as_str)
                .expect("external request variant has kind const")
        })
        .collect()
}
