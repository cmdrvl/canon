#![forbid(unsafe_code)]

use canon::{
    geo::{
        CANON_GEO_COMPOSITION_VERSION, CANON_GEO_HOME_CELL_ROWS_VERSION, CANON_GEO_RUN_VERSION,
        GeoPlanGrainStatus, GeoRun, GeoRunArtifactRef, GeoRunBlocker, GeoRunBlockerKind,
        GeoRunGrainState, GeoRunNextAction, GeoRunNextActionKind, GeoRunObservation,
        GeoRunOutputRef, GeoRunPhase, GeoRunPlanRef, GeoRunStatus, geo_run_semantic_hash,
    },
    project::{
        CANON_PROJECT_RUN_VERSION, ProjectRunHashRef, ProjectRunNextAction as ProjectNextAction,
        ProjectRunNodeOutcome, ProjectRunNodeReceipt, ProjectRunNodeReport,
        ProjectRunOutputReceipt, ProjectRunReceipt, ProjectRunReport,
    },
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const GEO_RUN_SCHEMA_JSON: &str = include_str!("../schemas/canon.geo.run.v0.schema.json");
const PROJECT_RUN_SCHEMA_JSON: &str = include_str!("../schemas/canon.project.run.v2.schema.json");

#[test]
fn schema_declares_strict_geo_run_contract() {
    let schema = schema();
    assert_eq!(
        schema.get("$schema").and_then(Value::as_str),
        Some("https://json-schema.org/draft/2020-12/schema")
    );
    assert_eq!(
        schema.get("title").and_then(Value::as_str),
        Some("canon.geo.run.v0")
    );
    assert_eq!(
        schema
            .pointer("/properties/version/const")
            .and_then(Value::as_str),
        Some("canon_geo_run.v0")
    );
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        schema
            .pointer("/properties/project_run_report/$ref")
            .and_then(Value::as_str),
        Some("canon.project.run.v2.schema.json")
    );
    assert_eq!(
        schema
            .pointer("/$defs/run_status/enum")
            .and_then(Value::as_array)
            .expect("status enum")
            .first()
            .and_then(Value::as_str),
        Some("COMPLETED")
    );
    assert_eq!(
        schema
            .pointer("/$defs/run_phase/enum")
            .and_then(Value::as_array)
            .expect("phase enum")
            .first()
            .and_then(Value::as_str),
        Some("DRAFTED")
    );
    assert_eq!(
        schema
            .pointer("/$defs/blake3_digest/pattern")
            .and_then(Value::as_str),
        Some("^blake3:[0-9a-f]{64}$")
    );
    assert_eq!(
        schema
            .pointer("/$defs/run_id/pattern")
            .and_then(Value::as_str),
        Some("^canon_geo_run\\.v0:[0-9a-f]{64}$")
    );
    let excluded_values =
        schema_string_values(&schema, "/x-canon-contract/semantic_identity_excludes");
    for excluded in [
        "observation.workspace_path",
        "observation.observed_at_utc",
        "observation.host_id",
        "observation.process_id",
        "observation.resource_observations",
        "project_run_report.failed_nodes",
        "project_run_report.cancelled_nodes",
        "project_run_report.invalidated_nodes",
        "project_run_report.blocked_nodes",
        "project_run_report.next_actions",
        "project_run_report.node_reports",
        "project_run_report.receipt.node_receipts[].semantic_hash",
        "project_run_report.receipt.node_receipts[].failure_message",
        "project_run_report.receipt.node_receipts[].duration_millis",
        "project_run_report.receipt.node_receipts[].resource_observations",
        "project_run_report.receipt.node_receipts[].outputs[].path",
        "paths",
        "ambient_timestamps",
        "host",
        "pid",
        "worker_order",
    ] {
        assert!(
            excluded_values.contains(excluded),
            "semantic identity exclusions must include {excluded}"
        );
    }
    let included_values =
        schema_string_values(&schema, "/x-canon-contract/semantic_identity_includes");
    for included in [
        "plan_ref",
        "artifact_inputs",
        "artifact_inputs.node_id",
        "artifact_inputs.binding_id",
        "output_refs",
        "grain_states.status",
        "blockers.kind",
        "next_actions.kind",
        "deterministic_usage",
        "project_run_report.receipt.node_receipts[].failure_code",
    ] {
        assert!(
            included_values.contains(included),
            "semantic identity inclusions must include {included}"
        );
    }
    assert!(
        !included_values.contains("project_run_report.receipt.node_receipts[].semantic_hash"),
        "Geo semantic identity must exclude project receipt semantic_hash"
    );
    assert!(
        !included_values.contains("project_run_report.receipt.node_receipts[].failure_message"),
        "Geo semantic identity must exclude project failure_message"
    );
    assert_eq!(
        schema
            .pointer("/$defs/run_artifact_ref/required")
            .and_then(Value::as_array)
            .expect("artifact required fields")
            .iter()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "node_id",
            "binding_id",
            "artifact_id",
            "content_digest",
            "media_type",
            "contract_version",
            "byte_count",
        ])
    );
    assert_eq!(
        schema
            .pointer("/$defs/run_status/enum")
            .and_then(Value::as_array)
            .expect("status enum")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>(),
        vec![
            "COMPLETED",
            "PARTIAL",
            "WAITING_FOR_INPUT",
            "UNSUPPORTED_GRAIN",
            "FAILED",
            "CANCELLED",
            "BUDGET_FALLBACK",
            "ABSTAINED",
            "CONTRADICTED",
        ]
    );
    assert!(required_values(&schema, "/$defs/run_plan_ref/required").contains("question_hash"));
    assert!(required_values(&schema, "/$defs/run_plan_ref/required").contains("capabilities_hash"));
    assert!(
        required_values(&schema, "/$defs/run_plan_ref/required").contains("budget_planning_hash")
    );
    assert_struct_object_schemas_are_closed(&schema, "$");
}

#[test]
fn schema_accepts_serialized_complete_run_manifest() {
    let manifest = serialized_complete_run_manifest();
    assert_schema_accepts(&manifest);
    assert_eq!(manifest["version"], "canon_geo_run.v0");
    assert_eq!(manifest["status"], "COMPLETED");
    assert_eq!(manifest["phase"], "SOLVED");
    assert_eq!(
        manifest["project_run_report"]["schema_version"],
        "canon.project.run.v2"
    );
    assert!(
        manifest.get("blockers").is_none(),
        "serde omits empty blockers"
    );
    assert!(
        manifest.get("next_actions").is_none(),
        "serde omits empty next_actions"
    );
}

#[test]
fn schema_accepts_waiting_for_input_run_manifest() {
    let manifest = waiting_for_input_manifest();
    assert_schema_accepts(&manifest);
    assert_eq!(manifest["status"], "WAITING_FOR_INPUT");
    assert_eq!(manifest["phase"], "PREFLIGHTED");
    assert_eq!(manifest["blockers"].as_array().expect("blockers").len(), 1);
    assert_eq!(
        manifest["next_actions"].as_array().expect("actions").len(),
        1
    );
}

#[test]
fn schema_rejects_unknown_fields() {
    let mut manifest = serialized_complete_run_manifest();
    manifest
        .as_object_mut()
        .expect("manifest object")
        .insert("unexpected".to_string(), json!(true));
    assert_schema_rejects(&manifest, "unknown field unexpected");

    let mut nested = serialized_complete_run_manifest();
    nested["plan_ref"]
        .as_object_mut()
        .expect("plan_ref object")
        .insert("receipt_path".to_string(), json!("work/receipt.json"));
    assert_schema_rejects(&nested, "unknown field receipt_path");
}

#[test]
fn schema_rejects_future_run_surfaces_not_in_current_api() {
    for field in [
        "answer",
        "source_release_receipts",
        "dependency_refs",
        "abstentions",
        "contradictions",
        "fallbacks",
    ] {
        let mut manifest = serialized_complete_run_manifest();
        manifest
            .as_object_mut()
            .expect("manifest object")
            .insert(field.to_string(), json!([]));
        assert_schema_rejects(&manifest, &format!("unknown field {field}"));
    }
}

#[test]
fn schema_rejects_bad_version_or_run_id_shape() {
    let mut bad_version = serialized_complete_run_manifest();
    bad_version["version"] = json!("canon_geo_run.v1");
    assert_schema_rejects(&bad_version, "const String(\"canon_geo_run.v0\")");

    let mut bad_run_id = serialized_complete_run_manifest();
    bad_run_id["run_id"] = json!("canon_geo_run.v0:short");
    assert_schema_rejects(&bad_run_id, "pattern ^canon_geo_run\\.v0:[0-9a-f]{64}$");
}

#[test]
fn schema_rejects_mixed_success_and_non_success_state() {
    let mut success_with_blocker = serialized_complete_run_manifest();
    success_with_blocker["blockers"] = json!([blocker()]);
    assert_schema_rejects(&success_with_blocker, "longer than maxItems 0");

    let mut success_with_next_action = serialized_complete_run_manifest();
    success_with_next_action["next_actions"] = json!([next_action()]);
    assert_schema_rejects(&success_with_next_action, "longer than maxItems 0");

    let mut waiting_without_blocker_or_action = waiting_for_input_manifest();
    waiting_without_blocker_or_action["blockers"] = json!([]);
    waiting_without_blocker_or_action["next_actions"] = json!([]);
    assert_schema_rejects(
        &waiting_without_blocker_or_action,
        "anyOf matched 0 alternatives",
    );
}

#[test]
fn schema_rejects_malformed_hashes() {
    let mut uppercase = serialized_complete_run_manifest();
    uppercase["semantic_hash"] = json!(format!("blake3:{}", "A".repeat(64)));
    assert_schema_rejects(&uppercase, "pattern ^blake3:[0-9a-f]{64}$");

    let mut missing_prefix = serialized_complete_run_manifest();
    missing_prefix["plan_ref"]["project_graph_hash"] = json!("1".repeat(64));
    assert_schema_rejects(&missing_prefix, "pattern ^blake3:[0-9a-f]{64}$");

    let mut bad_input_digest = serialized_complete_run_manifest();
    bad_input_digest["artifact_inputs"][0]["content_digest"] = json!(digest_text('g', 63));
    assert_schema_rejects(&bad_input_digest, "pattern ^blake3:[0-9a-f]{64}$");
}

#[test]
fn schema_rejects_observational_data_inside_semantic_fields() {
    let mut host_in_semantic_inputs = serialized_complete_run_manifest();
    host_in_semantic_inputs["plan_ref"]
        .as_object_mut()
        .expect("plan_ref object")
        .insert("host".to_string(), json!("build-agent-17"));
    assert_schema_rejects(&host_in_semantic_inputs, "unknown field host");

    let mut observed_at_in_artifact_input = serialized_complete_run_manifest();
    observed_at_in_artifact_input["artifact_inputs"][0]
        .as_object_mut()
        .expect("artifact input object")
        .insert("observed_at_utc".to_string(), json!("2026-08-31T20:00:00Z"));
    assert_schema_rejects(
        &observed_at_in_artifact_input,
        "unknown field observed_at_utc",
    );

    let mut worker_order_in_output_ref = serialized_complete_run_manifest();
    worker_order_in_output_ref["output_refs"][0]
        .as_object_mut()
        .expect("output ref object")
        .insert("worker_order".to_string(), json!(["worker-b", "worker-a"]));
    assert_schema_rejects(&worker_order_in_output_ref, "unknown field worker_order");
}

fn serialized_complete_run_manifest() -> Value {
    let manifest = complete_run_manifest();
    let bytes = serde_json::to_vec(&manifest).expect("GeoRun serializes");
    serde_json::from_slice(&bytes).expect("manifest parses back")
}

fn complete_run_manifest() -> GeoRun {
    let mut run = GeoRun {
        version: CANON_GEO_RUN_VERSION.to_string(),
        run_id: String::new(),
        semantic_hash: String::new(),
        status: GeoRunStatus::Completed,
        phase: GeoRunPhase::Solved,
        plan_ref: plan_ref(),
        artifact_inputs: vec![artifact_ref(
            "geo.building.home_cells",
            "rows",
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            '2',
            42,
        )],
        output_refs: vec![GeoRunOutputRef {
            artifact_id: "geo.building.solve/solve".to_string(),
            project_node_id: "geo.building.solve".to_string(),
            output_id: "solve".to_string(),
            content_digest: digest('3'),
            byte_count: 42,
            media_type: "application/json".to_string(),
            contract_version: CANON_GEO_COMPOSITION_VERSION.to_string(),
        }],
        grain_states: vec![GeoRunGrainState {
            entity_level: "building".to_string(),
            status: GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse,
            missing_evidence_classes: Vec::new(),
            project_node_ids: vec!["geo.building.solve".to_string()],
            claim_limitation: "truth reach remains separate from representation-relative exactness"
                .to_string(),
            next_action: "validate typed output before advancing".to_string(),
        }],
        blockers: Vec::new(),
        next_actions: Vec::new(),
        deterministic_usage: usage_map([
            ("dependency_semantic_hash_count", 0),
            ("geo.bound_input_artifacts", 1),
            ("geo.bound_input_bytes", 42),
        ]),
        project_run_report: Some(project_run_report()),
        observation: GeoRunObservation {
            workspace_path: Some("work/geo-run".to_string()),
            observed_at_utc: Some("2026-08-31T20:00:00Z".to_string()),
            host_id: Some("host.fixture".to_string()),
            process_id: Some(32451),
            resource_observations: usage_map([("wall_time_millis", 7)]),
        },
    };
    stamp_run_identity(&mut run);
    run
}

fn waiting_for_input_manifest() -> Value {
    let mut run = GeoRun {
        version: CANON_GEO_RUN_VERSION.to_string(),
        run_id: String::new(),
        semantic_hash: String::new(),
        status: GeoRunStatus::WaitingForInput,
        phase: GeoRunPhase::Preflighted,
        plan_ref: plan_ref(),
        artifact_inputs: Vec::new(),
        output_refs: Vec::new(),
        grain_states: vec![GeoRunGrainState {
            entity_level: "building".to_string(),
            status: GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse,
            missing_evidence_classes: Vec::new(),
            project_node_ids: vec!["geo.building.solve".to_string()],
            claim_limitation: "local artifact binding is required before execution".to_string(),
            next_action: "bind the missing planned artifact".to_string(),
        }],
        blockers: vec![blocker()],
        next_actions: vec![next_action()],
        deterministic_usage: BTreeMap::new(),
        project_run_report: None,
        observation: GeoRunObservation::default(),
    };
    stamp_run_identity(&mut run);
    serde_json::to_value(run).expect("GeoRun serializes")
}

fn plan_ref() -> GeoRunPlanRef {
    GeoRunPlanRef {
        plan_id: format!("canon_geo_plan.v0:{}", hex('9')),
        semantic_hash: digest('a'),
        project_id: "geo-run-fixture".to_string(),
        project_graph_hash: digest('b'),
        question_hash: digest('c'),
        capabilities_hash: digest('d'),
        inventory_planning_hash: digest('e'),
        profile_hash: digest('f'),
        budget_planning_hash: digest('0'),
    }
}

fn artifact_ref(
    node_id: &str,
    binding_id: &str,
    contract_version: &str,
    marker: char,
    byte_count: u64,
) -> GeoRunArtifactRef {
    GeoRunArtifactRef {
        node_id: node_id.to_string(),
        binding_id: binding_id.to_string(),
        artifact_id: format!("{node_id}/input/{binding_id}"),
        content_digest: digest(marker),
        media_type: "application/json".to_string(),
        contract_version: contract_version.to_string(),
        byte_count,
    }
}

fn blocker() -> GeoRunBlocker {
    GeoRunBlocker {
        blocker_id: "waiting_for_input:geo.building.home_cells/input/rows".to_string(),
        kind: GeoRunBlockerKind::WaitingForInput,
        project_node_id: Some("geo.building.home_cells".to_string()),
        entity_level: None,
        reason: "materialize-home-cells requires local typed home-cell rows".to_string(),
    }
}

fn next_action() -> GeoRunNextAction {
    GeoRunNextAction {
        action_id: "supply:geo.building.home_cells/input/rows".to_string(),
        kind: GeoRunNextActionKind::SupplyLocalArtifact,
        project_node_id: Some("geo.building.home_cells".to_string()),
        artifact_id: Some("geo.building.home_cells/input/rows".to_string()),
        expected_contract: Some(CANON_GEO_HOME_CELL_ROWS_VERSION.to_string()),
        media_type: Some("application/json".to_string()),
        command: None,
        reason: "bind local bytes by project node id, binding id, artifact id, canonical lowercase BLAKE3 digest, media type, and input contract; filesystem paths remain operational".to_string(),
    }
}

fn project_run_report() -> ProjectRunReport {
    let node_receipt = ProjectRunNodeReceipt {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: "geo-run-fixture".to_string(),
        plan_graph_hash: digest('b'),
        node_id: "geo.building.solve".to_string(),
        node_cache_key: digest('4'),
        content_hash_inputs: vec![ProjectRunHashRef {
            ref_id: "geo.run.input.geo.building.home_cells.rows".to_string(),
            content_hash: digest('2'),
        }],
        dependency_semantic_hashes: BTreeMap::new(),
        dependency_receipt_hashes: BTreeMap::new(),
        outputs: vec![ProjectRunOutputReceipt {
            output_id: "solve".to_string(),
            path: "geo/building/solve.json".to_string(),
            content_digest: digest('3'),
            byte_count: 42,
        }],
        outcome: ProjectRunNodeOutcome::Completed,
        deterministic_usage: usage_map([("geo.exact_residual_models", 1)]),
        duration_millis: 7,
        resource_observations: usage_map([("wall_time_millis", 7)]),
        next_action: ProjectNextAction::ExecuteDependents,
        failure_code: None,
        failure_message: None,
        semantic_hash: digest('5'),
        telemetry_hash: digest('6'),
        receipt_hash: digest('7'),
    };
    ProjectRunReport {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: "geo-run-fixture".to_string(),
        plan_graph_hash: digest('b'),
        run_receipt_hash: digest('8'),
        max_parallelism: 1,
        max_ready_width: 1,
        executed_nodes: vec!["geo.building.solve".to_string()],
        resumed_nodes: Vec::new(),
        failed_nodes: Vec::new(),
        cancelled_nodes: Vec::new(),
        invalidated_nodes: Vec::new(),
        blocked_nodes: Vec::new(),
        next_actions: BTreeMap::new(),
        receipt: ProjectRunReceipt {
            schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
            project_id: "geo-run-fixture".to_string(),
            plan_graph_hash: digest('b'),
            receipt_hash: digest('8'),
            completed_nodes: vec!["geo.building.solve".to_string()],
            failed_nodes: Vec::new(),
            cancelled_nodes: Vec::new(),
            invalidated_nodes: Vec::new(),
            blocked_nodes: Vec::new(),
            node_receipts: vec![node_receipt],
        },
        node_reports: vec![ProjectRunNodeReport {
            node_id: "geo.building.solve".to_string(),
            outcome: ProjectRunNodeOutcome::Completed,
            receipt_hash: Some(digest('7')),
            reason: None,
        }],
    }
}

fn stamp_run_identity(run: &mut GeoRun) {
    let semantic_hash = geo_run_semantic_hash(run).expect("GeoRun semantic hash");
    run.semantic_hash = semantic_hash;
    run.run_id = format!(
        "{CANON_GEO_RUN_VERSION}:{}",
        run.semantic_hash.trim_start_matches("blake3:")
    );
}

fn usage_map(entries: impl IntoIterator<Item = (&'static str, u64)>) -> BTreeMap<String, u64> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

fn digest(marker: char) -> String {
    format!("blake3:{}", hex(marker))
}

fn digest_text(marker: char, width: usize) -> String {
    format!("blake3:{}", marker.to_string().repeat(width))
}

fn hex(marker: char) -> String {
    marker.to_string().repeat(64)
}

fn schema() -> Value {
    serde_json::from_str(GEO_RUN_SCHEMA_JSON).expect("geo run schema parses")
}

fn project_run_schema() -> Value {
    serde_json::from_str(PROJECT_RUN_SCHEMA_JSON).expect("project run schema parses")
}

struct SchemaDocuments {
    geo_run: Value,
    project_run: Value,
}

fn schema_documents() -> SchemaDocuments {
    SchemaDocuments {
        geo_run: schema(),
        project_run: project_run_schema(),
    }
}

fn assert_schema_accepts(instance: &Value) {
    let errors = schema_errors(instance);
    assert!(
        errors.is_empty(),
        "expected schema acceptance, got {errors:#?}\n{}",
        serde_json::to_string_pretty(instance).expect("pretty instance")
    );
}

fn assert_schema_rejects(instance: &Value, expected: &str) {
    let errors = schema_errors(instance);
    assert!(
        errors.iter().any(|error| error.contains(expected)),
        "expected schema rejection containing {expected:?}, got {errors:#?}\n{}",
        serde_json::to_string_pretty(instance).expect("pretty instance")
    );
}

fn schema_errors(instance: &Value) -> Vec<String> {
    let schemas = schema_documents();
    let mut errors = Vec::new();
    validate_schema_node(
        &schemas,
        &schemas.geo_run,
        &schemas.geo_run,
        instance,
        "$",
        &mut errors,
    );
    errors
}

fn validate_schema_node(
    schemas: &SchemaDocuments,
    root: &Value,
    subschema: &Value,
    instance: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    if let Some(reference) = subschema.get("$ref").and_then(Value::as_str) {
        if reference == "canon.project.run.v2.schema.json" {
            validate_schema_node(
                schemas,
                &schemas.project_run,
                &schemas.project_run,
                instance,
                path,
                errors,
            );
            return;
        }
        let resolved = reference
            .strip_prefix('#')
            .and_then(|pointer| root.pointer(pointer))
            .unwrap_or_else(|| panic!("schema reference {reference} resolves"));
        validate_schema_node(schemas, root, resolved, instance, path, errors);
        return;
    }

    if let Some(parts) = subschema.get("allOf").and_then(Value::as_array) {
        for part in parts {
            validate_schema_node(schemas, root, part, instance, path, errors);
        }
    }

    if let Some(condition) = subschema.get("if") {
        let mut condition_errors = Vec::new();
        validate_schema_node(
            schemas,
            root,
            condition,
            instance,
            path,
            &mut condition_errors,
        );
        if condition_errors.is_empty()
            && let Some(then_schema) = subschema.get("then")
        {
            validate_schema_node(schemas, root, then_schema, instance, path, errors);
        }
    }

    if let Some(rejected) = subschema.get("not") {
        let mut rejected_errors = Vec::new();
        validate_schema_node(
            schemas,
            root,
            rejected,
            instance,
            path,
            &mut rejected_errors,
        );
        if rejected_errors.is_empty() {
            errors.push(format!("{path}: not matched a forbidden subschema"));
        }
    }

    if let Some(options) = subschema.get("oneOf").and_then(Value::as_array) {
        let matches = options
            .iter()
            .filter(|option| {
                let mut option_errors = Vec::new();
                validate_schema_node(schemas, root, option, instance, path, &mut option_errors);
                option_errors.is_empty()
            })
            .count();
        if matches != 1 {
            errors.push(format!("{path}: oneOf matched {matches} alternatives"));
        }
    }

    if let Some(options) = subschema.get("anyOf").and_then(Value::as_array) {
        let matches = options
            .iter()
            .filter(|option| {
                let mut option_errors = Vec::new();
                validate_schema_node(schemas, root, option, instance, path, &mut option_errors);
                option_errors.is_empty()
            })
            .count();
        if matches == 0 {
            errors.push(format!("{path}: anyOf matched 0 alternatives"));
        }
    }

    if let Some(expected) = subschema.get("const")
        && instance != expected
    {
        errors.push(format!("{path}: const {expected:?} mismatch"));
    }

    if let Some(values) = subschema.get("enum").and_then(Value::as_array)
        && !values.iter().any(|candidate| candidate == instance)
    {
        errors.push(format!("{path}: enum mismatch"));
    }

    validate_type(subschema, instance, path, errors);
    validate_string(subschema, instance, path, errors);
    validate_number(subschema, instance, path, errors);
    validate_object(schemas, root, subschema, instance, path, errors);
    validate_array(schemas, root, subschema, instance, path, errors);
}

fn validate_type(subschema: &Value, instance: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(expected) = subschema.get("type").and_then(Value::as_str) else {
        return;
    };
    let matches = match expected {
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
        "boolean" => instance.is_boolean(),
        other => panic!("unsupported schema type {other} at {path}"),
    };
    if !matches {
        errors.push(format!("{path}: expected type {expected}"));
    }
}

fn validate_string(subschema: &Value, instance: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(value) = instance.as_str() else {
        return;
    };
    if let Some(minimum) = subschema.get("minLength").and_then(Value::as_u64)
        && value.len() < minimum as usize
    {
        errors.push(format!("{path}: string shorter than minLength {minimum}"));
    }
    if let Some(maximum) = subschema.get("maxLength").and_then(Value::as_u64)
        && value.len() > maximum as usize
    {
        errors.push(format!("{path}: string longer than maxLength {maximum}"));
    }
    if let Some(pattern) = subschema.get("pattern").and_then(Value::as_str)
        && !matches_schema_pattern(pattern, value)
    {
        errors.push(format!("{path}: pattern {pattern} mismatch"));
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
        errors.push(format!("{path}: value below minimum {minimum}"));
    }
}

fn validate_object(
    schemas: &SchemaDocuments,
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
    for (field, value) in object {
        if let Some(property_schema) = properties.and_then(|properties| properties.get(field)) {
            validate_schema_node(
                schemas,
                root,
                property_schema,
                value,
                &format!("{path}.{field}"),
                errors,
            );
        } else if let Some(additional) = subschema.get("additionalProperties") {
            match additional {
                Value::Bool(false) => errors.push(format!("{path}: unknown field {field}")),
                Value::Object(_) => validate_schema_node(
                    schemas,
                    root,
                    additional,
                    value,
                    &format!("{path}.{field}"),
                    errors,
                ),
                _ => {}
            }
        }
    }
}

fn validate_array(
    schemas: &SchemaDocuments,
    root: &Value,
    subschema: &Value,
    instance: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    let Some(items) = instance.as_array() else {
        return;
    };
    if let Some(minimum) = subschema.get("minItems").and_then(Value::as_u64)
        && items.len() < minimum as usize
    {
        errors.push(format!("{path}: shorter than minItems {minimum}"));
    }
    if let Some(maximum) = subschema.get("maxItems").and_then(Value::as_u64)
        && items.len() > maximum as usize
    {
        errors.push(format!("{path}: longer than maxItems {maximum}"));
    }
    if subschema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
        let mut seen = BTreeSet::new();
        for item in items {
            if !seen.insert(serde_json::to_string(item).expect("item serializes")) {
                errors.push(format!("{path}: duplicate array item"));
            }
        }
    }
    if let Some(item_schema) = subschema.get("items") {
        for (index, item) in items.iter().enumerate() {
            validate_schema_node(
                schemas,
                root,
                item_schema,
                item,
                &format!("{path}[{index}]"),
                errors,
            );
        }
    }
}

fn matches_schema_pattern(pattern: &str, value: &str) -> bool {
    match pattern {
        "^blake3:[0-9a-f]{64}$" => value.strip_prefix("blake3:").is_some_and(is_lower_64_hex),
        "^blake3:[0-9a-fA-F]{64}$" => value
            .strip_prefix("blake3:")
            .is_some_and(is_mixed_case_64_hex),
        "^canon_geo_run\\.v0:[0-9a-f]{64}$" => value
            .strip_prefix("canon_geo_run.v0:")
            .is_some_and(is_lower_64_hex),
        "^canon_geo_plan\\.v0:[0-9a-f]{64}$" => value
            .strip_prefix("canon_geo_plan.v0:")
            .is_some_and(is_lower_64_hex),
        "^canon_geo_[a-z_]+\\.v[0-9]+:[0-9a-f]{64}$" => {
            let Some((prefix, hex)) = value.rsplit_once(':') else {
                return false;
            };
            prefix.starts_with("canon_geo_")
                && prefix.contains(".v")
                && prefix.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'
                        || byte == b'.'
                })
                && is_lower_64_hex(hex)
        }
        "^[A-Za-z0-9][A-Za-z0-9._:/@+-]{0,255}$" => bounded_local_id(value, 256),
        "^[a-z0-9][a-z0-9._:-]{0,127}$" => bounded_project_node_id(value),
        "^canon(_|\\.)[A-Za-z0-9_.-]+\\.v[0-9]+$" => contract_version(value),
        "^canon(_|\\.)[A-Za-z0-9_.-]+\\.v[0-9]+(\\|canon(_|\\.)[A-Za-z0-9_.-]+\\.v[0-9]+)*$" => {
            value.split('|').all(contract_version)
        }
        "^canon_geo_(run|plan)\\.v0:[0-9a-f]{64}$" => value
            .strip_prefix("canon_geo_run.v0:")
            .or_else(|| value.strip_prefix("canon_geo_plan.v0:"))
            .is_some_and(is_lower_64_hex),
        other => panic!("unsupported schema pattern {other}"),
    }
}

fn is_lower_64_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_mixed_case_64_hex(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte) || (b'A'..=b'F').contains(&byte)
        })
}

fn bounded_local_id(value: &str, maximum_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_len
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
    let Some(rest) = value
        .strip_prefix("canon_")
        .or_else(|| value.strip_prefix("canon."))
    else {
        return false;
    };
    let Some((name, version)) = rest.rsplit_once(".v") else {
        return false;
    };
    !name.is_empty()
        && !version.is_empty()
        && version.bytes().all(|byte| byte.is_ascii_digit())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

fn assert_struct_object_schemas_are_closed(value: &Value, path: &str) {
    match value {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("object") {
                let additional = object.get("additionalProperties");
                let closed_struct = additional.and_then(Value::as_bool) == Some(false);
                let typed_map = additional.is_some_and(Value::is_object);
                assert!(
                    closed_struct || typed_map,
                    "{path} object schema must be closed or a typed map"
                );
            }
            for (key, child) in object {
                assert_struct_object_schemas_are_closed(child, &format!("{path}.{key}"));
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                assert_struct_object_schemas_are_closed(child, &format!("{path}[{index}]"));
            }
        }
        _ => {}
    }
}

fn schema_string_values<'a>(schema: &'a Value, pointer: &str) -> BTreeSet<&'a str> {
    schema
        .pointer(pointer)
        .unwrap_or_else(|| panic!("schema array {pointer} exists"))
        .as_array()
        .expect("schema pointer is array")
        .iter()
        .map(|value| value.as_str().expect("array value is string"))
        .collect()
}

fn required_values<'a>(schema: &'a Value, pointer: &str) -> BTreeSet<&'a str> {
    schema_string_values(schema, pointer)
}
