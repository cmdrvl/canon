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
        ProjectExtensionDagRequest, ProjectPlan, ProjectPlanCache, ProjectPlanCacheDecision,
        ProjectPlanNode, ProjectPlanNodeClass, ProjectPlanNodeKind, ProjectPlanOutput,
        ProjectPlanOutputMaterialization, ProjectPlanSideEffect, ProjectPlanSideEffectKind,
        compile_extension_project_plan,
    },
};
use satisfy_subject::satisfy::{
    GeoInventoryAdvancementEffect, GeoSatisfactionArtifactReleaseRelation,
    GeoSatisfactionAssignment, GeoSatisfactionFileBinding, GeoSatisfactionFindingCode,
    GeoSatisfactionInput, GeoSatisfactionRunInput, GeoSatisfactionRunInputFileBinding,
    GeoSatisfactionStatus, GeoSatisfyErrorCode, geo_regional_inventory_advancement_semantic_hash,
    parse_geo_satisfaction_assignment, satisfy_geo_acquisition, satisfy_geo_acquisition_for_run,
    satisfy_geo_acquisition_with_relations,
};
use tempfile::tempdir;

#[test]
fn complete_receipt_satisfies_request_and_updates_inventory_without_paths() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("warehouse-rows.json");
    let receipt_path = dir.path().join("receipt.json");
    let data = warehouse_rows_artifact();
    std::fs::write(&data_path, &data).expect("write data");

    let request = acquisition_request(&data);
    let mut receipt = acquisition_receipt(&request, &data, GeoAcquisitionTerminalState::Complete);
    receipt.local_artifacts[0].media_type = GEO_RUN_JSON_MEDIA_TYPE.to_string();
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
    assert_eq!(
        satisfaction.bindings[0].content_hash,
        blake3_prefixed(&data)
    );
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
        blake3_prefixed(&data)
    );
    let advancement = satisfaction
        .inventory_advancement
        .as_ref()
        .expect("inventory advancement");
    assert_eq!(
        advancement.effect,
        GeoInventoryAdvancementEffect::LocalAvailabilityOnly
    );
    assert_eq!(advancement.base_inventory_id, inventory.inventory_id);
    assert_eq!(advancement.advanced_inventory_id, updated.inventory_id);
    assert_eq!(
        advancement.advanced_inventory_semantic_hash,
        regional_inventory_semantic_hash(updated).expect("advanced inventory hash")
    );
    assert_eq!(advancement.bounded_subset, request.subset);
    assert_eq!(advancement.denominators, receipt.denominators);
    assert_eq!(
        advancement.receipt_execution.executor_request_id.as_deref(),
        Some("http-request-1")
    );
    assert_eq!(
        inventory.sources[0].local_state.state,
        GeoSourceAvailability::Missing,
        "advancement must not mutate the base inventory"
    );
    let base_planning_hash =
        regional_inventory_planning_hash(&inventory).expect("base planning hash");
    let advanced_planning_hash =
        regional_inventory_planning_hash(updated).expect("advanced planning hash");
    assert_ne!(
        base_planning_hash, advanced_planning_hash,
        "local availability and content identity must deterministically invalidate planning"
    );
    assert_eq!(
        advanced_planning_hash,
        regional_inventory_planning_hash(&advancement.advanced_inventory)
            .expect("repeat advanced planning hash")
    );

    let serialized = serde_json::to_string(&satisfaction).expect("serialize satisfaction");
    assert!(!serialized.contains(dir.path().to_str().expect("utf8 temp path")));
}

#[test]
fn receipt_native_multi_release_artifacts_advance_inventory_and_read_back() {
    let dir = tempdir().expect("tempdir");
    let first_path = dir.path().join("warehouse-release-a.json");
    let second_path = dir.path().join("warehouse-release-b.json");
    let receipt_path = dir.path().join("receipt.json");
    let first = warehouse_rows_artifact_for("building-a");
    let second = warehouse_rows_artifact_for("building-b");
    std::fs::write(&first_path, &first).expect("write first data");
    std::fs::write(&second_path, &second).expect("write second data");

    let mut request = acquisition_request(&first);
    request.releases.push(GeoReleasePin {
        source_instance_id: "source.building.second".to_string(),
        release_id: "release.fixture.second".to_string(),
        release_digest: digest("release-second", b"release second"),
    });
    request.request_id = geo_acquisition_request_id(&request).expect("request id");
    let mut receipt = acquisition_receipt(&request, &first, GeoAcquisitionTerminalState::Complete);
    receipt.counts.rows = 2;
    receipt.counts.bytes = (first.len() + second.len()) as u64;
    receipt.source_digests = request
        .releases
        .iter()
        .enumerate()
        .map(|(index, release)| GeoDigest {
            digest_id: format!("source.release.{index}"),
            algorithm: release.release_digest.algorithm,
            hex_digest: release.release_digest.hex_digest.clone(),
        })
        .collect();
    receipt.result_digests = vec![
        digest("result.first", &first),
        digest("result.second", &second),
    ];
    receipt.local_artifacts = vec![
        GeoLocalArtifactDigest {
            artifact_id: "artifact.first".to_string(),
            media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
            byte_count: first.len() as u64,
            digest: digest("artifact.first", &first),
        },
        GeoLocalArtifactDigest {
            artifact_id: "artifact.second".to_string(),
            media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
            byte_count: second.len() as u64,
            digest: digest("artifact.second", &second),
        },
    ];
    receipt.artifact_release_relations = vec![
        artifact_release_relation("artifact.first", &request.releases[0]),
        artifact_release_relation("artifact.second", &request.releases[1]),
    ];
    write_json(&receipt_path, &receipt);

    let inventory = inventory_for_request(&request);
    let satisfaction = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: Some(&inventory),
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![
            file_binding("artifact.first", &first_path),
            file_binding("artifact.second", &second_path),
        ],
        result_digest_files: Vec::new(),
    })
    .expect("receipt-native multi-release satisfaction");

    assert_eq!(satisfaction.status, GeoSatisfactionStatus::Satisfied);
    assert_eq!(satisfaction.bindings.len(), 2);
    assert!(satisfaction.bindings.iter().any(|binding| {
        binding.release_id == request.releases[0].release_id
            && binding.content_hash == blake3_prefixed(&first)
    }));
    assert!(satisfaction.bindings.iter().any(|binding| {
        binding.release_id == request.releases[1].release_id
            && binding.content_hash == blake3_prefixed(&second)
    }));

    let advancement = satisfaction
        .inventory_advancement
        .as_ref()
        .expect("multi-release advancement");
    assert_eq!(advancement.source_advancements.len(), 2);
    assert_eq!(advancement.denominators, receipt.denominators);
    assert_eq!(advancement.source_digests, receipt.source_digests);
    assert_eq!(advancement.result_digests, receipt.result_digests);

    let updated = satisfaction
        .updated_inventory
        .as_ref()
        .expect("updated inventory");
    assert!(updated.sources.iter().all(|source| {
        source.local_state.state == GeoSourceAvailability::Available
            && source.local_state.local_ref.is_some()
    }));
    let readback_bytes = serde_json::to_vec(updated).expect("serialize updated inventory");
    let readback: GeoRegionalInventory =
        serde_json::from_slice(&readback_bytes).expect("read back updated inventory");
    assert_eq!(&readback, updated);
    assert_eq!(
        regional_inventory_semantic_hash(&readback).expect("readback semantic hash"),
        advancement.advanced_inventory_semantic_hash
    );
    assert_eq!(
        geo_regional_inventory_advancement_semantic_hash(advancement)
            .expect("advancement semantic hash"),
        advancement.semantic_hash
    );
}

#[test]
fn receipt_native_relations_reject_unmapped_extra_local_artifact_before_advancement() {
    let dir = tempdir().expect("tempdir");
    let first_path = dir.path().join("warehouse-release-a.json");
    let second_path = dir.path().join("warehouse-release-b.json");
    let extra_path = dir.path().join("warehouse-unmapped.json");
    let receipt_path = dir.path().join("receipt.json");
    let first = warehouse_rows_artifact_for("building-a");
    let second = warehouse_rows_artifact_for("building-b");
    let extra = warehouse_rows_artifact_for("building-unmapped");
    std::fs::write(&first_path, &first).expect("write first data");
    std::fs::write(&second_path, &second).expect("write second data");
    std::fs::write(&extra_path, &extra).expect("write extra data");

    let mut request = acquisition_request(&first);
    request.releases.push(GeoReleasePin {
        source_instance_id: "source.building.second".to_string(),
        release_id: "release.fixture.second".to_string(),
        release_digest: digest("release-second", b"release second"),
    });
    request.request_id = geo_acquisition_request_id(&request).expect("request id");
    let mut receipt = acquisition_receipt(&request, &first, GeoAcquisitionTerminalState::Complete);
    receipt.counts.rows = 3;
    receipt.counts.bytes = (first.len() + second.len() + extra.len()) as u64;
    receipt.result_digests = vec![
        digest("result.first", &first),
        digest("result.second", &second),
        digest("result.unmapped", &extra),
    ];
    receipt.local_artifacts = vec![
        GeoLocalArtifactDigest {
            artifact_id: "artifact.first".to_string(),
            media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
            byte_count: first.len() as u64,
            digest: digest("artifact.first", &first),
        },
        GeoLocalArtifactDigest {
            artifact_id: "artifact.second".to_string(),
            media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
            byte_count: second.len() as u64,
            digest: digest("artifact.second", &second),
        },
        GeoLocalArtifactDigest {
            artifact_id: "artifact.unmapped".to_string(),
            media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
            byte_count: extra.len() as u64,
            digest: digest("artifact.unmapped", &extra),
        },
    ];
    receipt.artifact_release_relations = vec![
        artifact_release_relation("artifact.first", &request.releases[0]),
        artifact_release_relation("artifact.second", &request.releases[1]),
    ];
    write_json(&receipt_path, &receipt);

    let error = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: Some(&inventory_for_request(&request)),
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![
            file_binding("artifact.first", &first_path),
            file_binding("artifact.second", &second_path),
            file_binding("artifact.unmapped", &extra_path),
        ],
        result_digest_files: Vec::new(),
    })
    .expect_err("unmapped local artifact relation must refuse before advancement");

    assert_eq!(error.code, GeoSatisfyErrorCode::ReceiptMismatch);
    assert!(error.message.contains("cover every local artifact"));
    assert_eq!(
        error.detail.get("local_artifact_id").map(String::as_str),
        Some("artifact.unmapped")
    );
}

#[test]
fn receipt_native_relations_do_not_advance_truncated_multi_release_results() {
    let dir = tempdir().expect("tempdir");
    let first_path = dir.path().join("partial-release-a.json");
    let second_path = dir.path().join("partial-release-b.json");
    let receipt_path = dir.path().join("receipt.json");
    let first = warehouse_rows_artifact_for("partial-building-a");
    let second = warehouse_rows_artifact_for("partial-building-b");
    std::fs::write(&first_path, &first).expect("write first partial data");
    std::fs::write(&second_path, &second).expect("write second partial data");

    let mut request = acquisition_request(&first);
    request.releases.push(GeoReleasePin {
        source_instance_id: "source.building.second".to_string(),
        release_id: "release.fixture.second".to_string(),
        release_digest: digest("release-second", b"release second"),
    });
    request.request_id = geo_acquisition_request_id(&request).expect("request id");
    let mut receipt = acquisition_receipt(&request, &first, GeoAcquisitionTerminalState::Partial);
    receipt.counts.bytes = (first.len() + second.len()) as u64;
    receipt.result_digests = vec![
        digest("result.first", &first),
        digest("result.second", &second),
    ];
    receipt.local_artifacts = vec![
        GeoLocalArtifactDigest {
            artifact_id: "artifact.first".to_string(),
            media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
            byte_count: first.len() as u64,
            digest: digest("artifact.first", &first),
        },
        GeoLocalArtifactDigest {
            artifact_id: "artifact.second".to_string(),
            media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
            byte_count: second.len() as u64,
            digest: digest("artifact.second", &second),
        },
    ];
    receipt.artifact_release_relations = vec![
        artifact_release_relation("artifact.first", &request.releases[0]),
        artifact_release_relation("artifact.second", &request.releases[1]),
    ];
    write_json(&receipt_path, &receipt);

    let satisfaction = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: Some(&inventory_for_request(&request)),
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![
            file_binding("artifact.first", &first_path),
            file_binding("artifact.second", &second_path),
        ],
        result_digest_files: Vec::new(),
    })
    .expect("truncated receipt remains a typed diagnostic");

    assert_eq!(satisfaction.status, GeoSatisfactionStatus::NotSatisfied);
    assert!(satisfaction.bindings.is_empty());
    assert!(satisfaction.inventory_advancement.is_none());
    assert!(satisfaction.updated_inventory.is_none());
    assert!(satisfaction.findings.iter().any(|finding| {
        finding.code == GeoSatisfactionFindingCode::Partial
            && finding.detail.get("rows_truncated").map(String::as_str) == Some("true")
    }));
}

#[test]
fn retained_zero_row_receipt_is_typed_proof_not_inventory_advancement() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("zero-rows.jsonl");
    let receipt_path = dir.path().join("receipt.json");
    let data = b"";
    std::fs::write(&data_path, data).expect("write retained zero-row data");

    let request = acquisition_request(data);
    let mut receipt = acquisition_receipt(&request, data, GeoAcquisitionTerminalState::ZeroRows);
    receipt.proof_class = GeoAcquisitionProofClass::Retained;
    receipt.retained_receipt_id = Some("retained:zero-row-proof".to_string());
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
    .expect("retained zero rows remain a valid receipt finding");

    assert_eq!(satisfaction.status, GeoSatisfactionStatus::NotSatisfied);
    assert_eq!(
        satisfaction
            .receipt_execution
            .retained_receipt_id
            .as_deref(),
        Some("retained:zero-row-proof")
    );
    assert!(satisfaction.bindings.is_empty());
    assert!(satisfaction.inventory_advancement.is_none());
    assert!(satisfaction.updated_inventory.is_none());
    assert!(satisfaction.findings.iter().any(|finding| {
        finding.code == GeoSatisfactionFindingCode::ZeroRows
            && finding.detail.get("rows").map(String::as_str) == Some("0")
    }));
}

#[test]
fn live_untyped_jsonl_satisfies_receipt_but_cannot_advance_inventory() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("rows.jsonl");
    let receipt_path = dir.path().join("receipt.json");
    let data = b"{\"native_id\":\"b-1\",\"building_footprint\":\"POLYGON EMPTY\"}\n";
    std::fs::write(&data_path, data).expect("write data");

    let request = acquisition_request(data);
    let receipt = acquisition_receipt(&request, data, GeoAcquisitionTerminalState::Complete);
    write_json(&receipt_path, &receipt);
    let inventory = inventory_for_request(&request);

    let satisfaction = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: Some(&inventory),
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect("receipt validation remains independent from inventory advancement");

    assert_eq!(satisfaction.status, GeoSatisfactionStatus::Satisfied);
    assert!(satisfaction.inventory_advancement.is_none());
    assert!(satisfaction.updated_inventory.is_none());
    assert!(satisfaction.findings.iter().any(|finding| {
        finding.code == GeoSatisfactionFindingCode::InventoryAdvancementUnsupportedArtifact
    }));
    assert_eq!(
        inventory.sources[0].local_state.state,
        GeoSourceAvailability::Missing
    );
}

#[test]
fn version_label_alone_cannot_advance_an_invalid_warehouse_artifact() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("invalid-warehouse-rows.json");
    let receipt_path = dir.path().join("receipt.json");
    let data = br#"{"version":"canon_geo_warehouse_rows.v0"}"#;
    std::fs::write(&data_path, data).expect("write data");

    let request = acquisition_request(data);
    let mut receipt = acquisition_receipt(&request, data, GeoAcquisitionTerminalState::Complete);
    receipt.local_artifacts[0].media_type = GEO_RUN_JSON_MEDIA_TYPE.to_string();
    write_json(&receipt_path, &receipt);
    let inventory = inventory_for_request(&request);

    let error = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: Some(&inventory),
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect_err("a version label cannot replace validation of the typed warehouse rows");

    assert_eq!(error.code, GeoSatisfyErrorCode::ContractMismatch);
    assert_eq!(
        error.detail.get("local_artifact_id").map(String::as_str),
        Some("artifact.rows")
    );
}

#[test]
fn inventory_advancement_requires_each_exact_plan_inventory_identity() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("warehouse-rows.json");
    let receipt_path = dir.path().join("receipt.json");
    let data = warehouse_rows_artifact();
    std::fs::write(&data_path, &data).expect("write data");

    let request = acquisition_request(&data);
    let mut receipt = acquisition_receipt(&request, &data, GeoAcquisitionTerminalState::Complete);
    receipt.local_artifacts[0].media_type = GEO_RUN_JSON_MEDIA_TYPE.to_string();
    write_json(&receipt_path, &receipt);
    let inventory = inventory_for_request(&request);
    let base_plan = plan_with_acquisition(request.clone());

    for field in ["inventory_id", "semantic_hash", "planning_hash"] {
        let mut plan = base_plan.clone();
        match field {
            "inventory_id" => plan.inventory_ref.inventory_id = "inventory.other".to_string(),
            "semantic_hash" => {
                plan.inventory_ref.semantic_hash = blake3_prefixed(b"other inventory semantic hash")
            }
            "planning_hash" => {
                plan.inventory_ref.planning_hash = blake3_prefixed(b"other inventory planning hash")
            }
            _ => unreachable!("fixed mismatch field"),
        }
        refresh_plan_identity(&mut plan);
        validate_geo_plan(&plan).expect("mismatched reference remains a structurally valid plan");

        let error = satisfy_geo_acquisition(GeoSatisfactionInput {
            plan: &plan,
            inventory: Some(&inventory),
            assignment: GeoSatisfactionAssignment {
                request_id: request.request_id.clone(),
                receipt_path: receipt_path.clone(),
            },
            local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
            result_digest_files: Vec::new(),
        })
        .expect_err("inventory reference mismatch must fail closed");

        assert_eq!(error.code, GeoSatisfyErrorCode::ContractMismatch);
        assert_eq!(error.detail.get("field").map(String::as_str), Some(field));
    }
}

#[test]
fn inventory_advancement_rejects_an_invalid_geo_plan() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("warehouse-rows.json");
    let receipt_path = dir.path().join("receipt.json");
    let data = warehouse_rows_artifact();
    std::fs::write(&data_path, &data).expect("write data");

    let request = acquisition_request(&data);
    let mut receipt = acquisition_receipt(&request, &data, GeoAcquisitionTerminalState::Complete);
    receipt.local_artifacts[0].media_type = GEO_RUN_JSON_MEDIA_TYPE.to_string();
    write_json(&receipt_path, &receipt);
    let inventory = inventory_for_request(&request);
    let mut plan = plan_with_acquisition(request.clone());
    plan.semantic_hash = blake3_prefixed(b"tampered plan");

    let error = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan,
        inventory: Some(&inventory),
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect_err("invalid plan must not advance inventory");

    assert_eq!(error.code, GeoSatisfyErrorCode::ContractMismatch);
    assert_eq!(
        error.detail.get("plan_error_code").map(String::as_str),
        Some("ContractViolation")
    );
}

#[test]
fn non_live_proof_validates_bytes_but_never_advances_available_inventory() {
    for proof_class in [
        GeoAcquisitionProofClass::Fixture,
        GeoAcquisitionProofClass::Retained,
    ] {
        let dir = tempdir().expect("tempdir");
        let data_path = dir.path().join("rows.jsonl");
        let receipt_path = dir.path().join("receipt.json");
        let data = b"{\"native_id\":\"b-1\"}\n";
        std::fs::write(&data_path, data).expect("write data");

        let request = acquisition_request(data);
        let mut receipt =
            acquisition_receipt(&request, data, GeoAcquisitionTerminalState::Complete);
        receipt.proof_class = proof_class;
        match proof_class {
            GeoAcquisitionProofClass::Fixture => {
                receipt.executor = None;
                receipt.fixture_id = Some("fixture:geo-satisfy".to_string());
            }
            GeoAcquisitionProofClass::Retained => {
                receipt.retained_receipt_id = Some("retained:geo-satisfy".to_string());
            }
            GeoAcquisitionProofClass::Live => unreachable!(),
        }
        write_json(&receipt_path, &receipt);
        let inventory = inventory_for_request(&request);

        let satisfaction = satisfy_geo_acquisition(GeoSatisfactionInput {
            plan: &plan_with_acquisition(request.clone()),
            inventory: Some(&inventory),
            assignment: GeoSatisfactionAssignment {
                request_id: request.request_id.clone(),
                receipt_path,
            },
            local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
            result_digest_files: Vec::new(),
        })
        .expect("non-live receipt bytes remain valid evidence");

        assert_eq!(satisfaction.status, GeoSatisfactionStatus::Satisfied);
        assert_eq!(satisfaction.bindings.len(), 1);
        assert!(satisfaction.inventory_advancement.is_none());
        assert!(satisfaction.updated_inventory.is_none());
        assert_eq!(
            inventory.sources[0].local_state.state,
            GeoSourceAvailability::Missing
        );
    }
}

#[test]
fn narrower_h3_subset_cannot_advance_region_wide_inventory() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("warehouse-rows.json");
    let receipt_path = dir.path().join("receipt.json");
    let data = warehouse_rows_artifact();
    std::fs::write(&data_path, &data).expect("write data");

    let mut request = acquisition_request(&data);
    request.subset.h3_cells = vec!["872830828ffffff".to_string()];
    request.request_id = geo_acquisition_request_id(&request).expect("request id");
    let mut receipt = acquisition_receipt(&request, &data, GeoAcquisitionTerminalState::Complete);
    receipt.local_artifacts[0].media_type = GEO_RUN_JSON_MEDIA_TYPE.to_string();
    write_json(&receipt_path, &receipt);
    let inventory = inventory_for_request(&request);

    let error = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: Some(&inventory),
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect_err("narrow H3 subset must not become region-wide availability");

    assert_eq!(error.code, GeoSatisfyErrorCode::ContractMismatch);
    assert_eq!(
        error.detail.get("h3_cell_count").map(String::as_str),
        Some("1")
    );
}

#[test]
fn equivalent_executor_protocol_receipts_have_same_semantic_satisfaction_hash() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("warehouse-rows.json");
    let http_receipt_path = dir.path().join("http.json");
    let object_receipt_path = dir.path().join("object.json");
    let data = warehouse_rows_artifact();
    std::fs::write(&data_path, &data).expect("write data");

    let request = acquisition_request(&data);
    let mut http_receipt =
        acquisition_receipt(&request, &data, GeoAcquisitionTerminalState::Complete);
    http_receipt.local_artifacts[0].media_type = GEO_RUN_JSON_MEDIA_TYPE.to_string();
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
    assert_ne!(
        http.inventory_advancement
            .as_ref()
            .expect("http advancement")
            .receipt_file
            .digest,
        object
            .inventory_advancement
            .as_ref()
            .expect("object advancement")
            .receipt_file
            .digest
    );
    assert_eq!(
        http.inventory_advancement
            .as_ref()
            .expect("http advancement")
            .semantic_hash,
        object
            .inventory_advancement
            .as_ref()
            .expect("object advancement")
            .semantic_hash
    );
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
    assert!(satisfaction.findings.iter().any(
        |finding| finding.code == GeoSatisfactionFindingCode::ArtifactReleaseRelationAmbiguous
    ));

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
fn one_release_with_multiple_artifacts_remains_ambiguous() {
    let dir = tempdir().expect("tempdir");
    let first_path = dir.path().join("first.jsonl");
    let second_path = dir.path().join("second.jsonl");
    let receipt_path = dir.path().join("receipt.json");
    let first = b"{\"native_id\":\"b-1\"}\n";
    let second = b"{\"native_id\":\"b-2\"}\n";
    std::fs::write(&first_path, first).expect("write first");
    std::fs::write(&second_path, second).expect("write second");

    let request = acquisition_request(first);
    let mut receipt = acquisition_receipt(&request, first, GeoAcquisitionTerminalState::Complete);
    receipt.counts.bytes = (first.len() + second.len()) as u64;
    receipt.result_digests = vec![
        digest("result.first", first),
        digest("result.second", second),
    ];
    receipt.local_artifacts = vec![
        GeoLocalArtifactDigest {
            artifact_id: "artifact.first".to_string(),
            media_type: "application/jsonl".to_string(),
            byte_count: first.len() as u64,
            digest: digest("artifact.first", first),
        },
        GeoLocalArtifactDigest {
            artifact_id: "artifact.second".to_string(),
            media_type: "application/jsonl".to_string(),
            byte_count: second.len() as u64,
            digest: digest("artifact.second", second),
        },
    ];
    write_json(&receipt_path, &receipt);

    let satisfaction = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &plan_with_acquisition(request.clone()),
        inventory: Some(&inventory_for_request(&request)),
        assignment: GeoSatisfactionAssignment {
            request_id: request.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![
            file_binding("artifact.first", &first_path),
            file_binding("artifact.second", &second_path),
        ],
        result_digest_files: Vec::new(),
    })
    .expect("ambiguous receipt remains diagnostic");

    assert_eq!(satisfaction.status, GeoSatisfactionStatus::NotSatisfied);
    assert!(satisfaction.bindings.is_empty());
    assert!(satisfaction.inventory_advancement.is_none());
    assert!(satisfaction.findings.iter().any(|finding| {
        finding.code == GeoSatisfactionFindingCode::ArtifactReleaseRelationAmbiguous
    }));
}

#[test]
fn caller_relations_cannot_disambiguate_or_reuse_multi_release_artifacts() {
    let dir = tempdir().expect("tempdir");
    let first_path = dir.path().join("first.jsonl");
    let second_path = dir.path().join("second.jsonl");
    let receipt_path = dir.path().join("receipt.json");
    let first = b"{\"native_id\":\"b-1\"}\n";
    let second = b"{\"native_id\":\"b-2\"}\n";
    std::fs::write(&first_path, first).expect("write first");
    std::fs::write(&second_path, second).expect("write second");

    let mut request = acquisition_request(first);
    request.releases.push(GeoReleasePin {
        source_instance_id: "source.building.second".to_string(),
        release_id: "release.fixture.second".to_string(),
        release_digest: digest("release-second", b"release second"),
    });
    request.request_id = geo_acquisition_request_id(&request).expect("request id");
    let mut receipt = acquisition_receipt(&request, first, GeoAcquisitionTerminalState::Complete);
    receipt.counts.bytes = (first.len() + second.len()) as u64;
    receipt.result_digests = vec![
        digest("result.first", first),
        digest("result.second", second),
    ];
    receipt.local_artifacts = vec![
        GeoLocalArtifactDigest {
            artifact_id: "artifact.first".to_string(),
            media_type: "application/jsonl".to_string(),
            byte_count: first.len() as u64,
            digest: digest("artifact.first", first),
        },
        GeoLocalArtifactDigest {
            artifact_id: "artifact.second".to_string(),
            media_type: "application/jsonl".to_string(),
            byte_count: second.len() as u64,
            digest: digest("artifact.second", second),
        },
    ];
    write_json(&receipt_path, &receipt);

    let plan = plan_with_acquisition(request.clone());
    let inventory = inventory_for_request(&request);
    let swapped = vec![
        artifact_release_relation("artifact.second", &request.releases[0]),
        artifact_release_relation("artifact.first", &request.releases[1]),
    ];
    let reused = vec![
        artifact_release_relation("artifact.first", &request.releases[0]),
        artifact_release_relation("artifact.first", &request.releases[1]),
    ];

    for relations in [swapped, reused] {
        let satisfaction = satisfy_geo_acquisition_with_relations(
            GeoSatisfactionInput {
                plan: &plan,
                inventory: Some(&inventory),
                assignment: GeoSatisfactionAssignment {
                    request_id: request.request_id.clone(),
                    receipt_path: receipt_path.clone(),
                },
                local_artifact_files: vec![
                    file_binding("artifact.first", &first_path),
                    file_binding("artifact.second", &second_path),
                ],
                result_digest_files: Vec::new(),
            },
            relations,
        )
        .expect("multi-release relation remains diagnostic");

        assert_eq!(satisfaction.status, GeoSatisfactionStatus::NotSatisfied);
        assert!(satisfaction.bindings.is_empty());
        assert!(satisfaction.inventory_advancement.is_none());
        assert!(satisfaction.updated_inventory.is_none());
        assert!(satisfaction.findings.iter().any(|finding| {
            finding.code == GeoSatisfactionFindingCode::ArtifactReleaseRelationAmbiguous
        }));
    }
}

#[test]
fn explicit_artifact_release_relation_must_match_release_pin() {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("rows.jsonl");
    let receipt_path = dir.path().join("receipt.json");
    let data = b"{\"native_id\":\"b-1\"}\n";
    std::fs::write(&data_path, data).expect("write data");

    let request = acquisition_request(data);
    let receipt = acquisition_receipt(&request, data, GeoAcquisitionTerminalState::Complete);
    write_json(&receipt_path, &receipt);

    let error = satisfy_geo_acquisition_with_relations(
        GeoSatisfactionInput {
            plan: &plan_with_acquisition(request.clone()),
            inventory: Some(&inventory_for_request(&request)),
            assignment: GeoSatisfactionAssignment {
                request_id: request.request_id.clone(),
                receipt_path,
            },
            local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
            result_digest_files: Vec::new(),
        },
        vec![GeoSatisfactionArtifactReleaseRelation {
            local_artifact_id: "artifact.rows".to_string(),
            source_instance_id: request.releases[0].source_instance_id.clone(),
            release_id: request.releases[0].release_id.clone(),
            release_digest: format!("blake3:{}", "0".repeat(64)),
        }],
    )
    .expect_err("wrong relation digest must fail");

    assert_eq!(error.code, GeoSatisfyErrorCode::ReceiptMismatch);
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
        artifact_release_relations: Vec::new(),
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
    let inventory = inventory_for_request(&request);
    let mut plan = GeoPlan {
        version: CANON_GEO_PLAN_VERSION.to_string(),
        plan_id: String::new(),
        semantic_hash: String::new(),
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
            inventory_id: inventory.inventory_id.clone(),
            semantic_hash: regional_inventory_semantic_hash(&inventory)
                .expect("inventory semantic hash"),
            planning_hash: regional_inventory_planning_hash(&inventory)
                .expect("inventory planning hash"),
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
    };
    refresh_plan_identity(&mut plan);
    validate_geo_plan(&plan).expect("Geo satisfaction fixture plan validates");
    plan
}

fn refresh_plan_identity(plan: &mut GeoPlan) {
    plan.semantic_hash = geo_plan_semantic_hash(plan).expect("Geo plan semantic hash");
    plan.plan_id = format!(
        "{CANON_GEO_PLAN_VERSION}:{}",
        plan.semantic_hash.trim_start_matches("blake3:")
    );
}

fn empty_project_plan() -> ProjectPlan {
    compile_extension_project_plan(ProjectExtensionDagRequest::offline_read_only(
        "fixture-project",
        blake3_prefixed(b"manifest"),
        blake3_prefixed(b"lock"),
        Vec::new(),
    ))
    .expect("empty project plan")
}

fn inventory_for_request(request: &GeoAcquisitionRequest) -> GeoRegionalInventory {
    GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.fixture".to_string(),
        region: request.bounded_geography.clone(),
        sources: request
            .releases
            .iter()
            .enumerate()
            .map(|(index, release)| GeoRegionalSourceInstance {
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
                lineage_ids: vec![format!("lineage.fixture.{index}")],
                native_scope: GeoNativeEntityScope::NativeEntity {
                    entity_level: GeoControlEntityLevel::Building,
                    identity_participation: GeoIdentityParticipation::EvidenceOnly,
                },
                evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
                coverage: GeoCoveragePredicate {
                    coverage_id: format!("coverage.fixture.{index}"),
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
            })
            .collect(),
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
    warehouse_rows_artifact_for("building-a")
}

fn warehouse_rows_artifact_for(building_id: &str) -> Vec<u8> {
    serde_json::to_vec(&GeoWarehouseRowsRequest {
        version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
        profile: GeoCompositionProfile::building(),
        parcel_rows: Vec::new(),
        building_parcel_rows: vec![GeoWarehouseBuildingParcelRow {
            building_id: building_id.to_string(),
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

fn artifact_release_relation(
    local_artifact_id: &str,
    release: &GeoReleasePin,
) -> GeoSatisfactionArtifactReleaseRelation {
    GeoSatisfactionArtifactReleaseRelation {
        local_artifact_id: local_artifact_id.to_string(),
        source_instance_id: release.source_instance_id.clone(),
        release_id: release.release_id.clone(),
        release_digest: format!("blake3:{}", release.release_digest.hex_digest),
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
