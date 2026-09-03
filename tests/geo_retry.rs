#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_ACQUISITION_RECEIPT_VERSION, CANON_GEO_ACQUISITION_REQUEST_VERSION,
    CANON_GEO_COMPOSITION_VERSION, CANON_GEO_PLAN_VERSION, CANON_GEO_RETRY_LOOP_VERSION,
    CANON_GEO_RUN_VERSION, GEO_RUN_JSON_MEDIA_TYPE, GeoAcquisitionCounts,
    GeoAcquisitionDenominator, GeoAcquisitionProofClass, GeoAcquisitionReceipt,
    GeoAcquisitionRequest, GeoAcquisitionResumability, GeoAcquisitionTerminalState,
    GeoBoundedGeography, GeoBoundedSubset, GeoDenominatorSource, GeoDigest, GeoDigestAlgorithm,
    GeoFieldRole, GeoLocalArtifactDigest, GeoPaginationReceipt, GeoPaginationRequest,
    GeoPlanGrainStatus, GeoReleasePin, GeoRequestedField, GeoRetryErrorCode, GeoRetryLoopArtifact,
    GeoRetryPolicy, GeoRetryTerminal, GeoRowByteCeilings, GeoRun, GeoRunBlocker, GeoRunBlockerKind,
    GeoRunGrainState, GeoRunObservation, GeoRunOutputRef, GeoRunPhase, GeoRunPlanRef, GeoRunStatus,
    GeoSubsetPredicate, GeoSubsetPredicateKind, canonical_retry_loop_bytes,
    geo_acquisition_request_id, geo_acquisition_request_semantic_hash, geo_run_semantic_hash,
    next_retry_pass, record_pass, validate_geo_acquisition_receipt,
    validate_geo_acquisition_request, validate_geo_run, validate_retry_loop_artifact,
};
use canon::project::{
    CANON_PROJECT_RUN_VERSION, ProjectRunHashRef, ProjectRunNextAction as ProjectNextAction,
    ProjectRunNodeOutcome, ProjectRunNodeReceipt, ProjectRunOutputReceipt, ProjectRunReceipt,
    ProjectRunReport,
};
use serde_json::Value;
use std::collections::BTreeMap;

const RETRY_LOOP_SCHEMA: &str = include_str!("../schemas/canon.geo.retry_loop.v0.schema.json");

#[test]
fn t10_bounded_abstain_regeocode_retry_loop_records_two_passes_and_ceiling() {
    let policy = retry_policy(2);
    let request_hash = geo_acquisition_request_semantic_hash(&policy.regeocode_request_template)
        .expect("request hashes");
    let mut loop_state = retry_loop(policy);
    let run_1 = abstaining_run("pass-1", "geocode_ambiguous");
    let run_2 = abstaining_run("pass-2", "geocode_ambiguous");

    let next = next_retry_pass(&loop_state, &run_1).expect("next pass succeeds");
    assert_eq!(
        next.as_ref(),
        Some(&loop_state.policy.regeocode_request_template)
    );

    let receipt = receipt_for_request(&loop_state.policy.regeocode_request_template);
    assert_eq!(receipt.request_semantic_hash, request_hash);
    record_pass(&mut loop_state, &run_1, Some(&receipt)).unwrap_or_else(|error| {
        panic!(
            "first retry pass rejected: {error:?}\npolicy max_passes={} request_hash={} run_status={:?} run_blockers={:?} run_hash={}",
            loop_state.policy.max_passes,
            request_hash,
            run_1.status,
            run_1.blockers,
            run_1.semantic_hash
        )
    });

    assert_eq!(loop_state.passes.len(), 1);
    assert_eq!(loop_state.terminal, None);

    let second_next = next_retry_pass(&loop_state, &run_2).expect("second next pass succeeds");
    assert_eq!(
        second_next.as_ref(),
        Some(&loop_state.policy.regeocode_request_template)
    );
    record_pass(&mut loop_state, &run_2, Some(&receipt)).unwrap_or_else(|error| {
        panic!(
            "second retry pass rejected: {error:?}\npasses={:#?}\nrun_status={:?} run_blockers={:?} run_hash={}",
            loop_state.passes,
            run_2.status,
            run_2.blockers,
            run_2.semantic_hash
        )
    });

    assert_eq!(loop_state.passes.len(), 2);
    assert_eq!(
        loop_state.terminal,
        Some(GeoRetryTerminal::AbstainedAtCeiling)
    );
    assert_eq!(loop_state.passes[1].abstention_reason, "geocode_ambiguous");
    assert!(loop_state.passes.iter().all(|pass| {
        pass.plan_blake3.starts_with("blake3:")
            && pass.run_blake3.starts_with("blake3:")
            && pass.regeocode.is_some()
            && pass
                .receipt_blake3
                .as_deref()
                .is_some_and(|digest| digest.starts_with("blake3:"))
    }));
    assert_eq!(
        next_retry_pass(&loop_state, &run_2).expect("ceiling is an abstention, not refusal"),
        None
    );
    validate_retry_loop_artifact(&loop_state).expect("final loop validates");
}

#[test]
fn t10_retry_policy_unbounded_refuses() {
    let mut loop_state = retry_loop(retry_policy(1));
    loop_state.policy.max_passes = 0;

    let error = record_pass(
        &mut loop_state,
        &abstaining_run("unbounded", "geocode_ambiguous"),
        None,
    )
    .expect_err("max_passes 0 must refuse");
    assert_eq!(error.code, GeoRetryErrorCode::RetryPolicyUnbounded);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("max_passes"),
        "unbounded policy refusal must name max_passes: {error:?}"
    );
}

#[test]
fn t10_resolved_after_retry_pass_sets_terminal_and_stops() {
    let mut loop_state = retry_loop(retry_policy(2));
    let receipt = receipt_for_request(&loop_state.policy.regeocode_request_template);

    let resolved = completed_run("resolved-after-first-pass");
    record_pass(&mut loop_state, &resolved, Some(&receipt)).expect("resolved retry pass records");

    assert_eq!(loop_state.passes.len(), 1);
    assert_eq!(loop_state.terminal, Some(GeoRetryTerminal::Resolved));
    assert_eq!(
        next_retry_pass(&loop_state, &resolved).expect("terminal resolved stops retry"),
        None
    );
}

#[test]
fn t10_receipt_request_digest_mismatch_refuses() {
    let mut loop_state = retry_loop(retry_policy(2));
    let other_receipt = receipt_for_request(&alternate_request());

    let error = record_pass(
        &mut loop_state,
        &abstaining_run("mismatched-receipt", "geocode_ambiguous"),
        Some(&other_receipt),
    )
    .expect_err("receipt for a different request must refuse");

    assert_eq!(error.code, GeoRetryErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("receipt.request_semantic_hash")
    );
    assert!(
        error.detail.contains_key("receipt_blake3"),
        "receipt mismatch must name the receipt digest: {error:?}"
    );
}

#[test]
fn t10_duplicate_run_hash_refuses() {
    let mut loop_state = retry_loop(retry_policy(2));
    let run = abstaining_run("duplicate", "geocode_ambiguous");
    record_pass(&mut loop_state, &run, None).expect("first pass records");

    let error = record_pass(&mut loop_state, &run, None)
        .expect_err("same run semantic hash cannot pad passes");
    assert_eq!(error.code, GeoRetryErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("duplicate_run_blake3").map(String::as_str),
        Some(run.semantic_hash.as_str()),
        "duplicate refusal must carry the repeated run hash: {error:?}"
    );
}

#[test]
fn t27_retry_module_has_no_instance_or_acquisition_client_literals() {
    let source = include_str!("../src/geo/retry.rs");
    let forbidden = [
        "1004540041",
        "chimera_wrongly_admitted",
        "asserted_address_core",
        "case_4",
        "franklin",
        "solve_composition",
        "reqwest",
        "hyper::",
        "geocodio",
        "nominatim",
        "std::process::command",
        "command::new",
        "mcp",
    ];

    let lower = source.to_ascii_lowercase();
    let hits = forbidden
        .iter()
        .filter(|literal| lower.contains(**literal))
        .copied()
        .collect::<Vec<_>>();
    assert!(hits.is_empty(), "T27 literal scan failed: {hits:?}");

    let scratch = "Franklin CASE_4";
    let scratch_lower = scratch.to_ascii_lowercase();
    assert!(scratch_lower.contains("franklin"));
    assert!(scratch_lower.contains("case_4"));
}

#[test]
fn retry_loop_schema_matches_a_real_instance() {
    let mut loop_state = retry_loop(retry_policy(1));
    let run = abstaining_run("schema", "geocode_ambiguous");
    record_pass(&mut loop_state, &run, None).expect("schema pass records");
    let canonical = canonical_retry_loop_bytes(&loop_state).expect("canonical bytes");
    let instance: Value = serde_json::from_slice(&canonical).expect("canonical JSON parses");
    let schema: Value = serde_json::from_str(RETRY_LOOP_SCHEMA).expect("schema parses");

    assert_eq!(
        schema.get("title").and_then(Value::as_str),
        Some("canon.geo.retry_loop.v0")
    );
    assert_eq!(
        schema
            .pointer("/properties/version/const")
            .and_then(Value::as_str),
        Some(CANON_GEO_RETRY_LOOP_VERSION)
    );
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false)
    );
    assert_declares_keys(&schema, "", &instance);
}

fn retry_loop(policy: GeoRetryPolicy) -> GeoRetryLoopArtifact {
    GeoRetryLoopArtifact {
        version: CANON_GEO_RETRY_LOOP_VERSION.to_string(),
        subject_id: "fixture.retry.subject".to_string(),
        policy,
        passes: Vec::new(),
        terminal: None,
    }
}

fn retry_policy(max_passes: u8) -> GeoRetryPolicy {
    GeoRetryPolicy {
        max_passes,
        regeocode_request_template: acquisition_request("retry"),
    }
}

fn acquisition_request(seed: &str) -> GeoAcquisitionRequest {
    let geography = GeoBoundedGeography {
        geography_id: format!("fixture.retry.{seed}.geography"),
        geography_kind: "bounded_fixture_region".to_string(),
        description: "bounded fixture region for retry-loop tests".to_string(),
    };
    let subset = GeoBoundedSubset {
        subset_id: format!("fixture.retry.{seed}.subset"),
        geography: geography.clone(),
        h3_cells: Vec::new(),
        predicates: vec![GeoSubsetPredicate {
            predicate_id: "subject_ids".to_string(),
            kind: GeoSubsetPredicateKind::ExplicitIdentifiers,
            expression: format!("subject_id = 'fixture.retry.{seed}.subject'"),
        }],
    };
    let mut request = GeoAcquisitionRequest {
        version: CANON_GEO_ACQUISITION_REQUEST_VERSION.to_string(),
        request_id: String::new(),
        discovery_request_id: None,
        bounded_geography: geography,
        subset,
        releases: vec![release_pin(seed)],
        fields: vec![GeoRequestedField {
            field_id: "address_text".to_string(),
            role: GeoFieldRole::Identifier,
            required: true,
        }],
        projection: None,
        ordering: Vec::new(),
        pagination: GeoPaginationRequest {
            page_size_rows: 10,
            page_token: None,
        },
        ceilings: GeoRowByteCeilings {
            max_rows: 10,
            max_bytes: 4096,
        },
        positive_path_min_rows: 1,
    };
    request.ordering.push(canon::geo::GeoOrderingTerm {
        position: 1,
        field_id: "address_text".to_string(),
        direction: canon::geo::GeoOrderDirection::Asc,
        nulls: canon::geo::GeoNullOrdering::Last,
    });
    request.request_id = geo_acquisition_request_id(&request).expect("request id");
    validate_geo_acquisition_request(&request).expect("valid acquisition request");
    request
}

fn alternate_request() -> GeoAcquisitionRequest {
    acquisition_request("alternate")
}

fn release_pin(seed: &str) -> GeoReleasePin {
    GeoReleasePin {
        source_instance_id: format!("fixture.retry.{seed}.source"),
        release_id: format!("fixture.retry.{seed}.release"),
        release_digest: digest_struct(&format!("release:{seed}")),
    }
}

fn receipt_for_request(request: &GeoAcquisitionRequest) -> GeoAcquisitionReceipt {
    let receipt = GeoAcquisitionReceipt {
        version: CANON_GEO_ACQUISITION_RECEIPT_VERSION.to_string(),
        request_id: request.request_id.clone(),
        request_semantic_hash: geo_acquisition_request_semantic_hash(request)
            .expect("request semantic hash"),
        terminal_state: GeoAcquisitionTerminalState::Complete,
        proof_class: GeoAcquisitionProofClass::Fixture,
        executor: None,
        fixture_id: Some("fixture.retry.receipt".to_string()),
        retained_receipt_id: None,
        bounded_geography: request.bounded_geography.clone(),
        subset: request.subset.clone(),
        releases: request.releases.clone(),
        fields: request.fields.clone(),
        projection: request.projection.clone(),
        normalized_executed_request_digest: digest_struct("normalized-executed-request"),
        pagination: GeoPaginationReceipt {
            requested_page: request.pagination.clone(),
            next_page_token: None,
            rows_truncated: false,
            bytes_truncated: false,
        },
        counts: GeoAcquisitionCounts { rows: 1, bytes: 64 },
        denominators: vec![GeoAcquisitionDenominator {
            denominator_id: "requested-subset".to_string(),
            source: GeoDenominatorSource::RequestedSubset,
            count: 1,
            unit: "row".to_string(),
            description: "one fixture subject requested".to_string(),
        }],
        source_digests: vec![digest_struct("source")],
        result_digests: vec![digest_struct("result")],
        local_artifacts: vec![GeoLocalArtifactDigest {
            artifact_id: "fixture.retry.result".to_string(),
            media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
            byte_count: 64,
            digest: digest_struct("local-artifact"),
        }],
        artifact_release_relations: Vec::new(),
        unreadable_columns: Vec::new(),
        resumability: GeoAcquisitionResumability {
            resumable: false,
            resume_token: None,
            resume_request_id: None,
            retry_guidance: "terminal fixture receipt requires no resume action".to_string(),
        },
        terminal_detail: None,
    };
    validate_geo_acquisition_receipt(request, &receipt).expect("valid acquisition receipt");
    receipt
}

fn abstaining_run(seed: &str, blocker_id: &str) -> GeoRun {
    let mut run = base_run(seed, GeoRunStatus::Abstained);
    run.phase = GeoRunPhase::Solved;
    run.blockers = vec![GeoRunBlocker {
        blocker_id: blocker_id.to_string(),
        kind: GeoRunBlockerKind::WaitingForInput,
        project_node_id: Some("geo.building.solve".to_string()),
        entity_level: Some("building".to_string()),
        reason: "bounded retry fixture run abstained before a fresh acquisition pass".to_string(),
    }];
    stamp_run_identity(&mut run);
    validate_geo_run(&run).expect("valid abstaining run");
    run
}

fn completed_run(seed: &str) -> GeoRun {
    let mut run = base_run(seed, GeoRunStatus::Completed);
    run.phase = GeoRunPhase::Solved;
    run.output_refs = vec![GeoRunOutputRef {
        artifact_id: "geo.building.solve/solve".to_string(),
        project_node_id: "geo.building.solve".to_string(),
        output_id: "solve".to_string(),
        content_digest: digest_label(&format!("{seed}:solve")),
        byte_count: 42,
        media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
        contract_version: CANON_GEO_COMPOSITION_VERSION.to_string(),
        resolved_claim: None,
    }];
    run.grain_states = vec![GeoRunGrainState {
        entity_level: "building".to_string(),
        status: GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse,
        missing_evidence_classes: Vec::new(),
        project_node_ids: vec!["geo.building.solve".to_string()],
        claim_limitation: "truth reach remains separate from representation-relative exactness"
            .to_string(),
        next_action: "validate typed output before advancing".to_string(),
    }];
    run.project_run_report = Some(project_run_report(seed));
    stamp_run_identity(&mut run);
    validate_geo_run(&run).expect("valid completed run");
    run
}

fn base_run(seed: &str, status: GeoRunStatus) -> GeoRun {
    let plan_hash = digest_label(&format!("{seed}:plan"));
    GeoRun {
        version: CANON_GEO_RUN_VERSION.to_string(),
        run_id: String::new(),
        semantic_hash: String::new(),
        status,
        phase: GeoRunPhase::Preflighted,
        plan_ref: GeoRunPlanRef {
            plan_id: format!(
                "{CANON_GEO_PLAN_VERSION}:{}",
                plan_hash.trim_start_matches("blake3:")
            ),
            semantic_hash: plan_hash,
            project_id: format!("geo.retry.{seed}.project"),
            project_graph_hash: digest_label(&format!("{seed}:project-graph")),
            question_hash: digest_label(&format!("{seed}:question")),
            capabilities_hash: digest_label(&format!("{seed}:capabilities")),
            inventory_planning_hash: digest_label(&format!("{seed}:inventory")),
            profile_hash: digest_label(&format!("{seed}:profile")),
            budget_planning_hash: digest_label(&format!("{seed}:budget")),
        },
        artifact_inputs: Vec::new(),
        acquisition_satisfactions: Vec::new(),
        output_refs: Vec::new(),
        grain_states: vec![GeoRunGrainState {
            entity_level: "building".to_string(),
            status: GeoPlanGrainStatus::WaitingForAcquisition,
            missing_evidence_classes: vec!["address_point".to_string()],
            project_node_ids: Vec::new(),
            claim_limitation: "local acquisition is required before deterministic execution"
                .to_string(),
            next_action: "record the emitted acquisition request and rerun".to_string(),
        }],
        blockers: Vec::new(),
        next_actions: Vec::new(),
        deterministic_usage: BTreeMap::new(),
        project_run_report: None,
        observation: GeoRunObservation::default(),
    }
}

fn project_run_report(seed: &str) -> ProjectRunReport {
    let node_receipt = ProjectRunNodeReceipt {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: format!("geo.retry.{seed}.project"),
        plan_graph_hash: digest_label(&format!("{seed}:project-graph")),
        node_id: "geo.building.solve".to_string(),
        node_cache_key: digest_label(&format!("{seed}:node-cache")),
        content_hash_inputs: vec![ProjectRunHashRef {
            ref_id: "geo.run.input.geo.building.home_cells.rows".to_string(),
            content_hash: digest_label(&format!("{seed}:input")),
        }],
        dependency_semantic_hashes: BTreeMap::new(),
        dependency_receipt_hashes: BTreeMap::new(),
        outputs: vec![ProjectRunOutputReceipt {
            output_id: "solve".to_string(),
            path: "geo/building/solve.json".to_string(),
            content_digest: digest_label(&format!("{seed}:solve")),
            byte_count: 42,
        }],
        outcome: ProjectRunNodeOutcome::Completed,
        deterministic_usage: BTreeMap::new(),
        duration_millis: 0,
        resource_observations: BTreeMap::new(),
        next_action: ProjectNextAction::ExecuteDependents,
        failure_code: None,
        failure_message: None,
        semantic_hash: digest_label(&format!("{seed}:node-semantic")),
        telemetry_hash: digest_label(&format!("{seed}:node-telemetry")),
        receipt_hash: digest_label(&format!("{seed}:node-receipt")),
    };
    ProjectRunReport {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: format!("geo.retry.{seed}.project"),
        plan_graph_hash: digest_label(&format!("{seed}:project-graph")),
        run_receipt_hash: digest_label(&format!("{seed}:run-receipt")),
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
            project_id: format!("geo.retry.{seed}.project"),
            plan_graph_hash: digest_label(&format!("{seed}:project-graph")),
            receipt_hash: digest_label(&format!("{seed}:run-receipt")),
            completed_nodes: vec!["geo.building.solve".to_string()],
            failed_nodes: Vec::new(),
            cancelled_nodes: Vec::new(),
            invalidated_nodes: Vec::new(),
            blocked_nodes: Vec::new(),
            node_receipts: vec![node_receipt],
        },
        node_reports: Vec::new(),
    }
}

fn stamp_run_identity(run: &mut GeoRun) {
    run.semantic_hash.clear();
    run.run_id.clear();
    run.semantic_hash = geo_run_semantic_hash(run).expect("run semantic hash");
    run.run_id = format!(
        "{CANON_GEO_RUN_VERSION}:{}",
        run.semantic_hash.trim_start_matches("blake3:")
    );
}

fn assert_declares_keys(schema: &Value, pointer: &str, instance: &Value) {
    let subschema = if pointer.is_empty() {
        schema
    } else {
        schema.pointer(pointer).expect("schema pointer resolves")
    };
    match instance {
        Value::Object(object) => {
            let properties = subschema
                .get("properties")
                .and_then(Value::as_object)
                .expect("object schema has properties");
            for (key, value) in object {
                let child_schema = properties
                    .get(key)
                    .unwrap_or_else(|| panic!("{pointer}: key {key} is not declared"));
                if let Some(reference) = child_schema.get("$ref").and_then(Value::as_str)
                    && reference.starts_with("#/")
                {
                    assert_declares_keys(schema, &reference[1..], value);
                    continue;
                }
                assert_declares_keys_inline(schema, child_schema, value);
            }
        }
        Value::Array(items) => {
            if let Some(item_schema) = subschema.get("items") {
                for item in items {
                    assert_declares_keys_inline(schema, item_schema, item);
                }
            }
        }
        _ => {}
    }
}

fn assert_declares_keys_inline(schema: &Value, subschema: &Value, instance: &Value) {
    if let Some(reference) = subschema.get("$ref").and_then(Value::as_str) {
        if reference.starts_with("#/") {
            assert_declares_keys(schema, &reference[1..], instance);
        }
        return;
    }
    match instance {
        Value::Object(_) => assert_declares_keys(schema, "", instance),
        Value::Array(items) => {
            if let Some(item_schema) = subschema.get("items") {
                for item in items {
                    assert_declares_keys_inline(schema, item_schema, item);
                }
            }
        }
        _ => {}
    }
}

fn digest_struct(seed: &str) -> GeoDigest {
    GeoDigest {
        digest_id: seed.to_string(),
        algorithm: GeoDigestAlgorithm::Blake3,
        hex_digest: digest_label(seed).trim_start_matches("blake3:").to_string(),
    }
}

fn digest_label(seed: &str) -> String {
    format!("blake3:{}", blake3::hash(seed.as_bytes()).to_hex())
}
