#![forbid(unsafe_code)]

mod satisfy_subject {
    pub use canon::geo::*;

    pub mod satisfy {
        include!("../src/geo/satisfy.rs");
    }
}

use canon::{
    geo::*,
    project::{
        ProjectPlan, ProjectPlanCache, ProjectPlanCacheDecision, ProjectPlanNode,
        ProjectPlanNodeClass, ProjectPlanNodeKind, ProjectPlanOutput,
        ProjectPlanOutputMaterialization, ProjectPlanSideEffect, ProjectPlanSideEffectKind,
        ProjectPlanSummary,
    },
};
use satisfy_subject::satisfy::{
    GeoSatisfactionAssignment, GeoSatisfactionFileBinding, GeoSatisfactionFindingCode,
    GeoSatisfactionInput, GeoSatisfactionRunInput, GeoSatisfactionRunInputFileBinding,
    GeoSatisfactionStatus, GeoSatisfyErrorCode, parse_geo_satisfaction_assignment,
    satisfy_geo_acquisition, satisfy_geo_acquisition_for_run,
};
use tempfile::tempdir;

#[test]
fn complete_receipt_satisfies_request_and_updates_inventory_without_paths() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("rows.jsonl");
    let receipt_path = dir.path().join("receipt.json");
    let data = b"{\"native_id\":\"b-1\",\"building_footprint\":\"POLYGON EMPTY\"}\n";
    std::fs::write(&data_path, data).expect("write data");

    let request = acquisition_request(data);
    let receipt = acquisition_receipt(&request, data, GeoAcquisitionTerminalState::Complete);
    write_json(&receipt_path, &receipt);
    let inventory = inventory_for_request(&request);
    let assignment = GeoSatisfactionAssignment {
        request_id: request.request_id.clone(),
        receipt_path: receipt_path.clone(),
    };

    let satisfaction = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: Some(&inventory),
        assignment,
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect("satisfaction");

    assert_eq!(satisfaction.status, GeoSatisfactionStatus::Satisfied);
    assert_eq!(satisfaction.bindings.len(), 1);
    assert_eq!(satisfaction.bindings[0].content_hash, blake3_prefixed(data));
    assert_eq!(
        satisfaction.findings[0].code,
        GeoSatisfactionFindingCode::Satisfied
    );
    let updated = satisfaction
        .updated_inventory
        .as_ref()
        .expect("updated inventory");
    assert_eq!(
        updated.sources[0].local_state.state,
        GeoSourceAvailability::Available
    );
    assert_eq!(
        updated.sources[0]
            .local_state
            .local_ref
            .as_ref()
            .expect("local ref")
            .content_hash,
        blake3_prefixed(data)
    );

    let serialized = serde_json::to_string(&satisfaction).expect("serialize satisfaction");
    assert!(!serialized.contains(dir.path().to_str().expect("utf8 temp path")));
}

#[test]
fn equivalent_executor_protocol_receipts_have_same_semantic_satisfaction_hash() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("rows.jsonl");
    let http_receipt_path = dir.path().join("http.json");
    let object_receipt_path = dir.path().join("object.json");
    let data = b"{\"native_id\":\"b-1\",\"building_footprint\":\"POINT (0 0)\"}\n";
    std::fs::write(&data_path, data).expect("write data");

    let request = acquisition_request(data);
    let mut http_receipt =
        acquisition_receipt(&request, data, GeoAcquisitionTerminalState::Complete);
    let mut object_receipt = http_receipt.clone();
    object_receipt.executor = Some(GeoExecutorTrace {
        executor_kind: GeoExecutorKind::ObjectStore,
        executor_id: "object.executor".to_string(),
        executor_version: "2026.08.31".to_string(),
        tool_id: "object.tool".to_string(),
        tool_version: "1".to_string(),
        executor_request_id: "object-request-7".to_string(),
        executor_query_id: "object-query-9".to_string(),
        executor_attempt_id: Some("attempt-b".to_string()),
    });
    http_receipt
        .executor
        .as_mut()
        .expect("executor")
        .executor_attempt_id = Some("attempt-a".to_string());
    write_json(&http_receipt_path, &http_receipt);
    write_json(&object_receipt_path, &object_receipt);
    let plan = plan_with_acquisition(request.clone());
    let inventory = inventory_for_request(&request);

    let http = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan,
        inventory: Some(&inventory),
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path: http_receipt_path,
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect("http satisfaction");
    let object = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan,
        inventory: Some(&inventory),
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path: object_receipt_path,
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect("object satisfaction");

    assert_ne!(http.receipt_file.digest, object.receipt_file.digest);
    assert_eq!(http.semantic_hash, object.semantic_hash);
    assert_eq!(http.satisfaction_id, object.satisfaction_id);
}

#[test]
fn positive_receipt_with_explicit_run_target_emits_geo_run_input_binding() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("warehouse-rows.json");
    let receipt_path = dir.path().join("receipt.json");
    let data = warehouse_rows_artifact();
    std::fs::write(&data_path, &data).expect("write run input artifact");

    let request = acquisition_request(&data);
    let mut receipt = acquisition_receipt(&request, &data, GeoAcquisitionTerminalState::Complete);
    receipt.local_artifacts[0].media_type = GEO_RUN_JSON_MEDIA_TYPE.to_string();
    write_json(&receipt_path, &receipt);
    let plan = plan_with_acquisition_and_run_node(request.clone());

    let handoff = satisfy_geo_acquisition_for_run(GeoSatisfactionRunInput {
        plan: &plan,
        inventory: None,
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path: receipt_path.clone(),
        },
        run_input_files: vec![run_input_file(
            "artifact.rows",
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &data_path,
        )],
        result_digest_files: Vec::new(),
    })
    .expect("explicit run satisfaction");

    assert_eq!(
        handoff.satisfaction.status,
        GeoSatisfactionStatus::Satisfied
    );
    assert_eq!(handoff.run_input_bindings.len(), 1);
    let binding = &handoff.run_input_bindings[0];
    assert_eq!(binding.node_id, "geo.building.materialize_evidence");
    assert_eq!(binding.binding_id, GEO_ROWS_BINDING_ID);
    assert_eq!(binding.contract_version, CANON_GEO_WAREHOUSE_ROWS_VERSION);
    assert_eq!(binding.content_digest, blake3_prefixed(&data));
    assert_eq!(binding.byte_count, data.len() as u64);
    assert_eq!(binding.bytes, data);
    assert_eq!(handoff.satisfaction.run_input_refs.len(), 1);
    assert_eq!(
        handoff.satisfaction.run_input_refs[0].artifact_id,
        "geo.building.materialize_evidence/input/rows"
    );

    let serialized = serde_json::to_string(&handoff.satisfaction).expect("serialize");
    assert!(!serialized.contains(dir.path().to_str().expect("utf8 temp path")));
}

#[test]
fn byte_tampering_is_rejected_against_receipt_digest() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("rows.jsonl");
    let receipt_path = dir.path().join("receipt.json");
    let data = b"alpha\n";
    std::fs::write(&data_path, data).expect("write original");

    let request = acquisition_request(data);
    let receipt = acquisition_receipt(&request, data, GeoAcquisitionTerminalState::Complete);
    write_json(&receipt_path, &receipt);
    std::fs::write(&data_path, b"bravo\n").expect("tamper with same byte count");

    let error = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: Some(&inventory_for_request(&request)),
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect_err("tampering must fail");

    assert_eq!(error.code, GeoSatisfyErrorCode::FileDigestMismatch);
}

#[test]
fn receipt_bound_to_wrong_request_is_rejected() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("rows.jsonl");
    let receipt_path = dir.path().join("receipt.json");
    let data = b"{\"native_id\":\"b-1\"}\n";
    std::fs::write(&data_path, data).expect("write data");

    let request = acquisition_request(data);
    let mut receipt = acquisition_receipt(&request, data, GeoAcquisitionTerminalState::Complete);
    receipt.request_id = format!(
        "{}:{}",
        CANON_GEO_ACQUISITION_REQUEST_VERSION,
        "0".repeat(64)
    );
    write_json(&receipt_path, &receipt);

    let error = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: None,
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path: receipt_path.clone(),
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect_err("wrong request must fail");

    assert_eq!(error.code, GeoSatisfyErrorCode::ReceiptMismatch);
}

#[test]
fn receipt_digest_mismatch_is_rejected_against_explicit_local_input() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("rows.jsonl");
    let receipt_path = dir.path().join("receipt.json");
    let data = b"{\"native_id\":\"b-1\"}\n";
    std::fs::write(&data_path, data).expect("write data");

    let request = acquisition_request(data);
    let mut receipt = acquisition_receipt(&request, data, GeoAcquisitionTerminalState::Complete);
    receipt.local_artifacts[0].digest.hex_digest = "0".repeat(64);
    write_json(&receipt_path, &receipt);

    let error = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: None,
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect_err("digest mismatch must fail");

    assert_eq!(error.code, GeoSatisfyErrorCode::FileDigestMismatch);
}

#[test]
fn receipt_byte_count_mismatch_is_rejected_against_explicit_local_input() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("rows.jsonl");
    let receipt_path = dir.path().join("receipt.json");
    let data = b"{\"native_id\":\"b-1\"}\n";
    std::fs::write(&data_path, data).expect("write data");

    let request = acquisition_request(data);
    let mut receipt = acquisition_receipt(&request, data, GeoAcquisitionTerminalState::Complete);
    receipt.local_artifacts[0].byte_count += 1;
    write_json(&receipt_path, &receipt);

    let error = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: None,
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect_err("byte count mismatch must fail");

    assert_eq!(error.code, GeoSatisfyErrorCode::FileByteCountMismatch);
}

#[test]
fn non_positive_terminal_state_does_not_emit_run_input_bindings() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("warehouse-rows.json");
    let receipt_path = dir.path().join("receipt.json");
    let data = warehouse_rows_artifact();
    std::fs::write(&data_path, &data).expect("write data");

    let request = acquisition_request(&data);
    let mut receipt = acquisition_receipt(&request, &data, GeoAcquisitionTerminalState::Partial);
    receipt.local_artifacts[0].media_type = GEO_RUN_JSON_MEDIA_TYPE.to_string();
    write_json(&receipt_path, &receipt);

    let handoff = satisfy_geo_acquisition_for_run(GeoSatisfactionRunInput {
        plan: &plan_with_acquisition_and_run_node(request.clone()),
        inventory: None,
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        run_input_files: vec![run_input_file(
            "artifact.rows",
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &data_path,
        )],
        result_digest_files: Vec::new(),
    })
    .expect("partial receipt remains typed");

    assert_eq!(
        handoff.satisfaction.status,
        GeoSatisfactionStatus::NotSatisfied
    );
    assert!(handoff.run_input_bindings.is_empty());
    assert!(handoff.satisfaction.run_input_refs.is_empty());
    assert!(
        handoff
            .satisfaction
            .findings
            .iter()
            .any(|finding| finding.code == GeoSatisfactionFindingCode::Partial)
    );
}

#[test]
fn explicit_run_target_contract_mismatch_refuses_even_when_bytes_match() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("warehouse-rows.json");
    let receipt_path = dir.path().join("receipt.json");
    let data = warehouse_rows_artifact();
    std::fs::write(&data_path, &data).expect("write data");

    let request = acquisition_request(&data);
    let mut receipt = acquisition_receipt(&request, &data, GeoAcquisitionTerminalState::Complete);
    receipt.local_artifacts[0].media_type = GEO_RUN_JSON_MEDIA_TYPE.to_string();
    write_json(&receipt_path, &receipt);

    let error = satisfy_geo_acquisition_for_run(GeoSatisfactionRunInput {
        plan: &plan_with_acquisition_and_run_node(request.clone()),
        inventory: None,
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        run_input_files: vec![run_input_file(
            "artifact.rows",
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &data_path,
        )],
        result_digest_files: Vec::new(),
    })
    .expect_err("wrong node contract must fail");

    assert_eq!(error.code, GeoSatisfyErrorCode::ContractMismatch);
}

#[test]
fn multi_release_receipt_does_not_cross_product_release_artifact_bindings() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("rows.jsonl");
    let receipt_path = dir.path().join("receipt.json");
    let data = b"{\"native_id\":\"b-1\"}\n";
    std::fs::write(&data_path, data).expect("write data");

    let mut request = acquisition_request(data);
    request.releases.push(GeoReleasePin {
        source_instance_id: "source.building.second".to_string(),
        release_id: "release.fixture.second".to_string(),
        release_digest: digest("release-second", b"release second"),
    });
    request.request_id = geo_acquisition_request_id(&request).expect("request id");
    let receipt = acquisition_receipt(&request, data, GeoAcquisitionTerminalState::Complete);
    write_json(&receipt_path, &receipt);

    let satisfaction = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: None,
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path: receipt_path.clone(),
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect("multi-release receipt validates");

    assert_eq!(satisfaction.status, GeoSatisfactionStatus::NotSatisfied);
    assert!(satisfaction.bindings.is_empty());
    assert!(satisfaction.updated_inventory.is_none());
    assert!(
        satisfaction.findings.iter().any(
            |finding| finding.code == GeoSatisfactionFindingCode::ArtifactReleaseRelationAbsent
        )
    );

    let handoff = satisfy_geo_acquisition_for_run(GeoSatisfactionRunInput {
        plan: &plan_with_acquisition_and_run_node(request.clone()),
        inventory: None,
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        run_input_files: vec![run_input_file(
            "artifact.rows",
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &data_path,
        )],
        result_digest_files: Vec::new(),
    })
    .expect("multi-release run handoff remains diagnostic");

    assert_eq!(
        handoff.satisfaction.status,
        GeoSatisfactionStatus::NotSatisfied
    );
    assert!(handoff.run_input_bindings.is_empty());
    assert!(handoff.satisfaction.run_input_refs.is_empty());
}

#[test]
fn non_success_terminal_states_remain_typed_findings() {
    for state in [
        GeoAcquisitionTerminalState::ZeroRows,
        GeoAcquisitionTerminalState::Timeout,
        GeoAcquisitionTerminalState::Canceled,
        GeoAcquisitionTerminalState::Partial,
        GeoAcquisitionTerminalState::UnreadableColumns,
    ] {
        let dir = tempdir().expect("tempdir");
        let data_path = dir.path().join("rows.jsonl");
        let receipt_path = dir.path().join("receipt.json");
        let data = if state == GeoAcquisitionTerminalState::ZeroRows {
            b"[]\n".as_slice()
        } else {
            b"{\"native_id\":\"b-1\"}\n".as_slice()
        };
        std::fs::write(&data_path, data).expect("write data");
        let request = acquisition_request(data);
        let receipt = acquisition_receipt(&request, data, state);
        write_json(&receipt_path, &receipt);

        let satisfaction = satisfy_geo_acquisition(GeoSatisfactionInput {
            plan: &plan_with_acquisition(request.clone()),
            inventory: Some(&inventory_for_request(&request)),
            assignment: GeoSatisfactionAssignment {
                request_id: request.request_id.clone(),
                receipt_path,
            },
            local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
            result_digest_files: Vec::new(),
        })
        .expect("valid non-success receipt");

        assert_eq!(satisfaction.status, GeoSatisfactionStatus::NotSatisfied);
        assert!(satisfaction.bindings.is_empty());
        assert!(satisfaction.updated_inventory.is_none());
        assert!(
            satisfaction
                .findings
                .iter()
                .any(|finding| expected_finding(state) == finding.code)
        );
    }
}

#[test]
fn parses_request_id_receipt_assignment() {
    let request_id = format!(
        "{}:{}",
        CANON_GEO_ACQUISITION_REQUEST_VERSION,
        "a".repeat(64)
    );
    let parsed = parse_geo_satisfaction_assignment(&format!("{request_id}=receipts/receipt.json"))
        .expect("parse assignment");

    assert_eq!(parsed.request_id, request_id);
    assert_eq!(
        parsed.receipt_path,
        std::path::PathBuf::from("receipts/receipt.json")
    );
}

fn acquisition_request(result_bytes: &[u8]) -> GeoAcquisitionRequest {
    let release_digest = digest("release", b"release fixture");
    let mut request = GeoAcquisitionRequest {
        version: CANON_GEO_ACQUISITION_REQUEST_VERSION.to_string(),
        request_id: String::new(),
        discovery_request_id: None,
        bounded_geography: geography(),
        subset: GeoBoundedSubset {
            subset_id: "subset:demo-region".to_string(),
            geography: geography(),
            h3_cells: Vec::new(),
            predicates: vec![GeoSubsetPredicate {
                predicate_id: "region".to_string(),
                kind: GeoSubsetPredicateKind::AdministrativeBoundary,
                expression: "demo-region".to_string(),
            }],
        },
        releases: vec![GeoReleasePin {
            source_instance_id: "source.building.fixture".to_string(),
            release_id: "release.fixture.2026".to_string(),
            release_digest,
        }],
        fields: vec![
            GeoRequestedField {
                field_id: "native_id".to_string(),
                role: GeoFieldRole::Identifier,
                required: true,
            },
            GeoRequestedField {
                field_id: "building_footprint".to_string(),
                role: GeoFieldRole::Geometry,
                required: true,
            },
            GeoRequestedField {
                field_id: "source_record_digest".to_string(),
                role: GeoFieldRole::Digest,
                required: true,
            },
        ],
        projection: Some(GeoProjectionOperation {
            coordinate_reference_system: "EPSG:3857".to_string(),
            operation_id: "fixture-transform".to_string(),
            operation_version: "1".to_string(),
            operation_digest: digest("transform", b"transform fixture"),
        }),
        ordering: vec![GeoOrderingTerm {
            position: 0,
            field_id: "native_id".to_string(),
            direction: GeoOrderDirection::Asc,
            nulls: GeoNullOrdering::Last,
        }],
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
    request.fields.sort();
    request.request_id = geo_acquisition_request_id(&request).expect("request id");
    assert!(result_bytes.len() <= request.ceilings.max_bytes as usize);
    request
}

fn acquisition_receipt(
    request: &GeoAcquisitionRequest,
    bytes: &[u8],
    terminal_state: GeoAcquisitionTerminalState,
) -> GeoAcquisitionReceipt {
    let rows = match terminal_state {
        GeoAcquisitionTerminalState::ZeroRows
        | GeoAcquisitionTerminalState::Timeout
        | GeoAcquisitionTerminalState::Canceled
        | GeoAcquisitionTerminalState::UnreadableColumns => 0,
        GeoAcquisitionTerminalState::Complete | GeoAcquisitionTerminalState::Partial => 1,
    };
    GeoAcquisitionReceipt {
        version: CANON_GEO_ACQUISITION_RECEIPT_VERSION.to_string(),
        request_id: request.request_id.clone(),
        request_semantic_hash: geo_acquisition_request_semantic_hash(request)
            .expect("request semantic hash"),
        terminal_state,
        proof_class: GeoAcquisitionProofClass::Live,
        executor: Some(GeoExecutorTrace {
            executor_kind: GeoExecutorKind::HttpService,
            executor_id: "http.executor".to_string(),
            executor_version: "2026.08.31".to_string(),
            tool_id: "http.tool".to_string(),
            tool_version: "1".to_string(),
            executor_request_id: "http-request-1".to_string(),
            executor_query_id: "http-query-1".to_string(),
            executor_attempt_id: None,
        }),
        fixture_id: None,
        retained_receipt_id: None,
        bounded_geography: request.bounded_geography.clone(),
        subset: request.subset.clone(),
        releases: request.releases.clone(),
        fields: request.fields.clone(),
        projection: request.projection.clone(),
        normalized_executed_request_digest: digest("normalized_request", b"canonical request"),
        pagination: GeoPaginationReceipt {
            requested_page: request.pagination.clone(),
            next_page_token: if terminal_state == GeoAcquisitionTerminalState::Partial {
                Some("next-page".to_string())
            } else {
                None
            },
            rows_truncated: terminal_state == GeoAcquisitionTerminalState::Partial,
            bytes_truncated: false,
        },
        counts: GeoAcquisitionCounts {
            rows,
            bytes: bytes.len() as u64,
        },
        denominators: vec![GeoAcquisitionDenominator {
            denominator_id: "requested_subset".to_string(),
            source: GeoDenominatorSource::RequestedSubset,
            count: 1,
            unit: "row".to_string(),
            description: "fixture requested subset denominator".to_string(),
        }],
        source_digests: vec![digest("source", b"source bytes")],
        result_digests: vec![digest("result", bytes)],
        local_artifacts: vec![GeoLocalArtifactDigest {
            artifact_id: "artifact.rows".to_string(),
            media_type: "application/jsonl".to_string(),
            byte_count: bytes.len() as u64,
            digest: digest("artifact.rows", bytes),
        }],
        unreadable_columns: if terminal_state == GeoAcquisitionTerminalState::UnreadableColumns {
            vec!["building_footprint".to_string()]
        } else {
            Vec::new()
        },
        resumability: GeoAcquisitionResumability {
            resumable: false,
            resume_token: None,
            resume_request_id: None,
            retry_guidance: "retry by satisfying the same request id with a fresh receipt"
                .to_string(),
        },
        terminal_detail: match terminal_state {
            GeoAcquisitionTerminalState::Complete | GeoAcquisitionTerminalState::ZeroRows => None,
            GeoAcquisitionTerminalState::Timeout => Some("executor timed out".to_string()),
            GeoAcquisitionTerminalState::Canceled => Some("executor canceled".to_string()),
            GeoAcquisitionTerminalState::Partial => Some("pagination incomplete".to_string()),
            GeoAcquisitionTerminalState::UnreadableColumns => Some("column unreadable".to_string()),
        },
    }
}

fn plan_with_acquisition(request: GeoAcquisitionRequest) -> GeoPlan {
    GeoPlan {
        version: CANON_GEO_PLAN_VERSION.to_string(),
        plan_id: "fixture-plan".to_string(),
        semantic_hash: blake3_prefixed(b"fixture plan"),
        status: GeoPlanStatus::Partial,
        question_ref: GeoPlanArtifactRef {
            artifact_id: "question.fixture".to_string(),
            semantic_hash: blake3_prefixed(b"question"),
        },
        capabilities_ref: GeoPlanArtifactRef {
            artifact_id: "capabilities.fixture".to_string(),
            semantic_hash: blake3_prefixed(b"capabilities"),
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: "inventory.fixture".to_string(),
            semantic_hash: blake3_prefixed(b"inventory"),
            planning_hash: blake3_prefixed(b"inventory planning"),
        },
        profile_ref: GeoPlanProfileRef {
            version: CANON_GEO_COMPOSITION_PROFILE_VERSION.to_string(),
            selection_level: GeoEntityLevel::Building,
            semantic_hash: blake3_prefixed(b"profile"),
        },
        budget_ref: GeoPlanBudgetRef {
            budget_id: "budget.fixture".to_string(),
            semantic_hash: blake3_prefixed(b"budget"),
            planning_hash: blake3_prefixed(b"budget planning"),
        },
        project_plan: empty_project_plan(),
        geo_nodes: Vec::new(),
        grain_outcomes: Vec::new(),
        external_requests: vec![GeoPlanExternalRequest::Acquisition {
            request,
            handoff: GeoPlanAcquisitionHandoff {
                expected_receipt_contract: CANON_GEO_ACQUISITION_RECEIPT_VERSION.to_string(),
                required_result_digest_algorithm: GeoDigestAlgorithm::Blake3,
                continuation_command: "canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>".to_string(),
            },
        }],
        diagnostics: Vec::new(),
    }
}

fn empty_project_plan() -> ProjectPlan {
    ProjectPlan {
        schema_version: "canon.project.plan.v1".to_string(),
        project_id: "fixture-project".to_string(),
        plan_kind: "fixture".to_string(),
        manifest_digest: blake3_prefixed(b"manifest"),
        lock_digest: blake3_prefixed(b"lock"),
        plan_artifact_path: None,
        graph_hash: blake3_prefixed(b"graph"),
        summary: ProjectPlanSummary {
            total_nodes: 0,
            edge_count: 0,
            computation_nodes: 0,
            external_materialization_nodes: 0,
            review_pause_nodes: 0,
            mutation_gate_nodes: 0,
            export_nodes: 0,
            cache_hits: 0,
            cache_misses: 0,
            runnable_nodes: 0,
            blocked_nodes: 0,
        },
        nodes: Vec::new(),
        next_commands: Default::default(),
        diagnostics: Vec::new(),
    }
}

fn inventory_for_request(request: &GeoAcquisitionRequest) -> GeoRegionalInventory {
    let release = &request.releases[0];
    GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.fixture".to_string(),
        region: request.bounded_geography.clone(),
        sources: vec![GeoRegionalSourceInstance {
            source_instance_id: release.source_instance_id.clone(),
            release: GeoSourceRelease {
                release_id: release.release_id.clone(),
                release_digest: format!("blake3:{}", release.release_digest.hex_digest),
            },
            temporal_scope: GeoTemporalScope {
                valid_time: None,
                transaction_time: None,
                release_time: None,
            },
            lineage_ids: vec!["lineage.fixture".to_string()],
            native_scope: GeoNativeEntityScope::NativeEntity {
                entity_level: GeoControlEntityLevel::Building,
            },
            evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
            coverage: GeoCoveragePredicate {
                coverage_id: "coverage.fixture".to_string(),
                region: request.bounded_geography.clone(),
                predicate: "demo-region".to_string(),
            },
            local_state: GeoLocalAcquisitionState {
                state: GeoSourceAvailability::Missing,
                local_ref: None,
            },
            geometry: None,
            license_class: GeoLicenseClass::PublicRedistributable,
            egress_class: GeoEgressClass::Shareable,
            estimates: Vec::new(),
        }],
        discovery_gaps: Vec::new(),
    }
}

fn plan_with_acquisition_and_run_node(request: GeoAcquisitionRequest) -> GeoPlan {
    let mut plan = plan_with_acquisition(request);
    plan.project_plan.summary.total_nodes = 1;
    plan.project_plan.summary.computation_nodes = 1;
    plan.project_plan.summary.runnable_nodes = 1;
    plan.project_plan.nodes = vec![ProjectPlanNode {
        node_id: "geo.building.materialize_evidence".to_string(),
        kind: ProjectPlanNodeKind::Evidence,
        class: ProjectPlanNodeClass::Computation,
        command: GEO_MATERIALIZE_EVIDENCE_COMMAND.to_string(),
        dependencies: Vec::new(),
        content_hash_inputs: Vec::new(),
        outputs: vec![ProjectPlanOutput {
            output_id: "materialize_evidence".to_string(),
            path: "geo/building/materialize_evidence.json".to_string(),
            content_hash: blake3_prefixed(b"planned output"),
            materialization: ProjectPlanOutputMaterialization::PlannedArtifact,
        }],
        limits: Default::default(),
        cache: ProjectPlanCache {
            eligible: true,
            decision: ProjectPlanCacheDecision::Miss,
            cache_key: blake3_prefixed(b"materialize-evidence node"),
            reason: "fixture planned node".to_string(),
        },
        side_effects: vec![ProjectPlanSideEffect {
            kind: ProjectPlanSideEffectKind::ReadsInput,
            description: "reads explicit local typed input".to_string(),
        }],
        refusal_conditions: Vec::new(),
        runnable: true,
        blocked_by: Vec::new(),
    }];
    plan
}

fn warehouse_rows_artifact() -> Vec<u8> {
    serde_json::to_vec(&GeoWarehouseRowsRequest {
        version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
        profile: GeoCompositionProfile::building(),
        parcel_rows: Vec::new(),
        building_parcel_rows: vec![GeoWarehouseBuildingParcelRow {
            building_id: "building-a".to_string(),
            parcel_id: None,
        }],
        contracts: Vec::new(),
        evidence_rows: Vec::new(),
        max_assignments: 16,
        max_materialized_models: 16,
    })
    .expect("warehouse rows serializes")
}

fn geography() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: "demo-region".to_string(),
        geography_kind: "fixture_region".to_string(),
        description: "demo bounded fixture region".to_string(),
    }
}

fn digest(id: &str, bytes: &[u8]) -> GeoDigest {
    GeoDigest {
        digest_id: id.to_string(),
        algorithm: GeoDigestAlgorithm::Blake3,
        hex_digest: blake3::hash(bytes).to_hex().to_string(),
    }
}

fn blake3_prefixed(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn file_binding(id: &str, path: &std::path::Path) -> GeoSatisfactionFileBinding {
    GeoSatisfactionFileBinding {
        binding_id: id.to_string(),
        path: path.to_path_buf(),
    }
}

fn run_input_file(
    local_artifact_id: &str,
    node_id: &str,
    binding_id: &str,
    contract_version: &str,
    path: &std::path::Path,
) -> GeoSatisfactionRunInputFileBinding {
    GeoSatisfactionRunInputFileBinding {
        local_artifact_id: local_artifact_id.to_string(),
        node_id: node_id.to_string(),
        binding_id: binding_id.to_string(),
        contract_version: contract_version.to_string(),
        path: path.to_path_buf(),
    }
}

fn write_json(path: &std::path::Path, value: &impl serde::Serialize) {
    let bytes = serde_json::to_vec(value).expect("serialize json");
    std::fs::write(path, bytes).expect("write json");
}

fn expected_finding(state: GeoAcquisitionTerminalState) -> GeoSatisfactionFindingCode {
    match state {
        GeoAcquisitionTerminalState::Complete => GeoSatisfactionFindingCode::Satisfied,
        GeoAcquisitionTerminalState::ZeroRows => GeoSatisfactionFindingCode::ZeroRows,
        GeoAcquisitionTerminalState::Timeout => GeoSatisfactionFindingCode::Timeout,
        GeoAcquisitionTerminalState::Canceled => GeoSatisfactionFindingCode::Canceled,
        GeoAcquisitionTerminalState::Partial => GeoSatisfactionFindingCode::Partial,
        GeoAcquisitionTerminalState::UnreadableColumns => {
            GeoSatisfactionFindingCode::UnreadableColumns
        }
    }
}
