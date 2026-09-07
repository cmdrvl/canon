#![forbid(unsafe_code)]

use canon::{
    geo::{
        CANON_GEO_HOME_CELL_ROWS_VERSION, CANON_GEO_NEXT_EVIDENCE_INPUTS_VERSION,
        CANON_GEO_RESOURCE_BUDGET_VERSION, CANON_GEO_SEPARATION_INPUTS_VERSION,
        CANON_GEO_TILE_WORK_REQUEST_VERSION, CANON_GEO_WAREHOUSE_ROWS_VERSION,
        DEFAULT_MAX_MATERIALIZED_MODELS, GEO_REQUEST_BINDING_ID, GEO_ROWS_BINDING_ID,
        GeoAbstentionDisposition, GeoAbstentionPolicy, GeoAsOf, GeoBoundedGeography,
        GeoBudgetAction, GeoClaimClass, GeoCompositionProfile, GeoControlEntityLevel,
        GeoCoveragePredicate, GeoDateInterval, GeoEgressClass, GeoEntityLevel, GeoEntityRef,
        GeoEvidenceClaimRole, GeoEvidenceClass, GeoEvidenceRecordRef, GeoGeometryTransformContract,
        GeoHardConstraintKind, GeoIdentityParticipation, GeoLicenseClass, GeoLocalAcquisitionState,
        GeoLocalArtifactRef, GeoNativeEntityScope, GeoNextActionClass, GeoNextActionKind,
        GeoNextEvidenceCandidateInput, GeoNextEvidenceInputs, GeoNumericBound, GeoNumericMeasure,
        GeoPlan, GeoPlanInventoryRef, GeoPlanRequest, GeoProspectiveObservation,
        GeoProspectiveOutcome, GeoRegionalInventory, GeoRegionalSourceInstance, GeoRequestedGrain,
        GeoResourceBudget, GeoResourceCounter, GeoRhoBasis, GeoRhoContract, GeoRhoObservationKind,
        GeoRunArtifactBinding, GeoRunRequest, GeoRunStatus, GeoSeparationInputs,
        GeoSourceAvailability, GeoSourceRelease, GeoSubjectBinding, GeoSubjectBindingClass,
        GeoTelemetryDeclaration, GeoTelemetryMetric, GeoTelemetrySemanticEffect, GeoTemporalScope,
        GeoTileFeatureRef, GeoTileSourceBinding, GeoTileWorkRequest, GeoValueOrigin,
        GeoWarehouseBuildingParcelRow, GeoWarehouseEvidenceRow, GeoWarehouseRowsRequest,
        compile_geo_plan, default_geo_capabilities, run_geo_plan,
    },
    project::{ProjectRunFailurePolicy, ProjectRunPolicy, digest_bytes, read_node_receipt},
};
use h3o::CellIndex;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    str::FromStr,
};

#[test]
fn t76_geo_run_added_evidence_reuses_bounded_prefix_and_revises_manifest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan();
    let policy = policy(temp.path());

    let first = run_geo_plan(GeoRunRequest::new(
        plan.clone(),
        policy.clone(),
        run_bindings(warehouse_rows()),
    ))
    .expect("first Geo accretion run completes");
    assert_eq!(first.status, GeoRunStatus::Completed);
    assert_eq!(geo_run_manifest_revision_count(temp.path()), 1);
    let first_home = read_geo_receipt(temp.path(), "geo.building.home_cells");
    let first_section = read_geo_receipt(temp.path(), "geo.building.section");

    let second = run_geo_plan(GeoRunRequest::new(
        plan,
        policy,
        run_bindings(warehouse_rows_plus_one_evidence_row()),
    ))
    .expect("second Geo accretion run completes");
    assert_eq!(second.status, GeoRunStatus::Completed);
    assert_eq!(
        geo_run_manifest_revision_count(temp.path()),
        2,
        "T76 added evidence must publish a new immutable Geo run revision"
    );

    let report = second
        .project_run_report
        .as_ref()
        .expect("Geo run embeds project report");
    assert_eq!(
        report.resumed_nodes,
        vec![
            "geo.building.home_cells".to_string(),
            "geo.building.section".to_string()
        ],
        "T76 changed evidence rows should reuse only the home-cell and bounded-section prefix"
    );
    assert_eq!(
        string_set(&report.executed_nodes),
        string_set_from([
            "geo.building.materialize_evidence",
            "geo.building.compile_evidence",
            "geo.building.propagate",
            "geo.building.solve",
            "geo.building.explain",
            "geo.building.separation",
            "geo.building.next_evidence",
        ]),
        "T76 changed evidence rows should recompute only the solver suffix"
    );
    assert_eq!(report.resource_reuse.saved_nodes, 2);
    assert!(
        report
            .resource_reuse
            .estimated_national_extrapolation
            .is_none(),
        "T76 saved work is local deterministic reuse, not national extrapolation"
    );

    let second_home = read_geo_receipt(temp.path(), "geo.building.home_cells");
    let second_section = read_geo_receipt(temp.path(), "geo.building.section");
    assert_eq!(first_home.semantic_hash, second_home.semantic_hash);
    assert_eq!(first_section.semantic_hash, second_section.semantic_hash);
    assert_eq!(
        first_home.outputs[0].content_digest,
        second_home.outputs[0].content_digest
    );
    assert_eq!(
        first_section.outputs[0].content_digest,
        second_section.outputs[0].content_digest
    );
}

fn building_plan() -> GeoPlan {
    compile_geo_plan(GeoPlanRequest {
        question: question(),
        capabilities: default_geo_capabilities().expect("capabilities"),
        inventory: inventory(),
        profile: GeoCompositionProfile::building(),
        budget: budget(),
    })
    .expect("Geo plan compiles")
}

fn run_bindings(rows: GeoWarehouseRowsRequest) -> Vec<GeoRunArtifactBinding> {
    vec![
        GeoRunArtifactBinding::from_json(
            "geo.building.home_cells",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &home_cell_rows(),
        )
        .expect("home cell binding"),
        GeoRunArtifactBinding::from_json(
            "geo.building.section",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            &tile_work_request(),
        )
        .expect("tile work binding"),
        GeoRunArtifactBinding::from_json(
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &rows,
        )
        .expect("warehouse rows binding"),
        GeoRunArtifactBinding::from_json(
            "geo.building.separation",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_SEPARATION_INPUTS_VERSION,
            &separation_inputs(),
        )
        .expect("separation inputs binding"),
        GeoRunArtifactBinding::from_json(
            "geo.building.next_evidence",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_NEXT_EVIDENCE_INPUTS_VERSION,
            &next_evidence_inputs(),
        )
        .expect("next evidence inputs binding"),
    ]
}

fn policy(workspace: &Path) -> ProjectRunPolicy {
    let mut policy = ProjectRunPolicy::new(workspace, "work");
    policy.failure_policy = ProjectRunFailurePolicy::FailFast;
    policy
}

fn question() -> canon::geo::GeoQuestion {
    canon::geo::GeoQuestion {
        version: canon::geo::CANON_GEO_QUESTION_VERSION.to_string(),
        question_id: "question.fixture.accretion".to_string(),
        subject_bindings: vec![GeoSubjectBinding {
            role: "target".to_string(),
            binding_class: GeoSubjectBindingClass::OperatorLabel,
            value: "fixture subject".to_string(),
        }],
        bounded_geography: region(),
        requested_grains: vec![GeoRequestedGrain {
            entity_level: GeoControlEntityLevel::Building,
            required_evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
            optional_evidence_classes: Vec::new(),
        }],
        query_as_of: Some(GeoAsOf {
            utc_day: "2026-08-31".to_string(),
            semantic_id: "query.as_of".to_string(),
            unit: "utc_day".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
        }),
        requested_claim_classes: vec![GeoClaimClass::CollateralComposition],
        presentation_limits: vec![GeoNumericBound {
            semantic_id: "presentation.max_models".to_string(),
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
        resource_budget_ref: "budget.fixture.accretion".to_string(),
    }
}

fn region() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: "region.fixture.accretion".to_string(),
        geography_kind: "bounded_fixture".to_string(),
        description: "One explicitly bounded accretion fixture".to_string(),
    }
}

fn inventory() -> GeoRegionalInventory {
    GeoRegionalInventory {
        version: canon::geo::CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.fixture.accretion".to_string(),
        region: region(),
        sources: vec![GeoRegionalSourceInstance {
            source_instance_id: "source.fixture.buildings".to_string(),
            release: GeoSourceRelease {
                release_id: "release.fixture.one".to_string(),
                release_digest: digest("release.fixture.one"),
            },
            temporal_scope: GeoTemporalScope {
                valid_time: Some(GeoDateInterval {
                    start_utc_day: "2026-01-01".to_string(),
                    end_utc_day: "2026-12-31".to_string(),
                }),
                transaction_time: None,
                release_time: None,
            },
            lineage_ids: vec!["lineage.fixture.accretion".to_string()],
            native_scope: GeoNativeEntityScope::NativeEntity {
                entity_level: GeoControlEntityLevel::Building,
                identity_participation: GeoIdentityParticipation::StableAlias,
            },
            evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
            coverage: GeoCoveragePredicate {
                coverage_id: "coverage.fixture.accretion".to_string(),
                region: region(),
                predicate: "all declared fixture records".to_string(),
            },
            local_state: GeoLocalAcquisitionState {
                state: GeoSourceAvailability::Available,
                local_ref: Some(GeoLocalArtifactRef {
                    artifact_id: "artifact.fixture.buildings".to_string(),
                    contract_version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
                    content_hash: digest("local.fixture.accretion"),
                    media_type: "application/json".to_string(),
                }),
            },
            geometry: Some(GeoGeometryTransformContract {
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
            }),
            license_class: GeoLicenseClass::PublicRedistributable,
            egress_class: GeoEgressClass::Shareable,
            estimates: vec![GeoNumericMeasure {
                semantic_id: "source.rows".to_string(),
                value: 100,
                unit: "row".to_string(),
                origin: GeoValueOrigin::SourceRelease,
            }],
        }],
        discovery_gaps: Vec::new(),
    }
}

fn budget() -> GeoResourceBudget {
    GeoResourceBudget {
        version: CANON_GEO_RESOURCE_BUDGET_VERSION.to_string(),
        budget_id: "budget.fixture.accretion".to_string(),
        deterministic_bounds: vec![
            bound("budget.max_bytes", GeoResourceCounter::Bytes, 1_000_000),
            bound("budget.max_rows", GeoResourceCounter::Rows, 10_000),
            bound("budget.max_cells", GeoResourceCounter::Cells, 64),
            bound("budget.max_candidates", GeoResourceCounter::Candidates, 500),
            bound("budget.max_variables", GeoResourceCounter::Variables, 128),
            bound("budget.max_states", GeoResourceCounter::States, 100_000),
            bound("budget.max_models", GeoResourceCounter::Models, 10_000),
            bound(
                "budget.max_operations",
                GeoResourceCounter::Operations,
                1_000_000,
            ),
        ],
        telemetry: vec![GeoTelemetryDeclaration {
            metric: GeoTelemetryMetric::WallTime,
            unit: "millisecond".to_string(),
            origin: GeoValueOrigin::OperatorPolicy,
            semantic_effect: GeoTelemetrySemanticEffect::None,
        }],
    }
}

fn bound(id: &str, counter: GeoResourceCounter, value: u64) -> GeoNumericBound {
    GeoNumericBound {
        semantic_id: id.to_string(),
        counter,
        value,
        unit: format!("{counter:?}").to_lowercase(),
        origin: GeoValueOrigin::CallerDeclared,
        action: GeoBudgetAction::ReportBudgetFallback,
    }
}

fn home_cell_rows() -> canon::geo::GeoHomeCellRowsRequest {
    canon::geo::GeoHomeCellRowsRequest {
        version: CANON_GEO_HOME_CELL_ROWS_VERSION.to_string(),
        coordinate_crs: "EPSG:4326".to_string(),
        coordinate_decimal_places: 9,
        h3_resolution: 9,
        stability_radius_fixed: 1_000,
        rows: vec![
            home_cell_row("building-a", "rec-building-a"),
            home_cell_row("building-b", "rec-building-b"),
        ],
        max_rows: 16,
    }
}

fn home_cell_row(feature_id: &str, source_record_id: &str) -> canon::geo::GeoHomeCellRow {
    canon::geo::GeoHomeCellRow {
        source: building_tile_source(),
        feature_id: feature_id.to_string(),
        source_record_id: source_record_id.to_string(),
        geometry_sha256: "5ed87d37d872789086452c35f658f5628ba870ca36072c495bb88519592403ed"
            .to_string(),
        representative_point_method: "centroid_of_derived_wgs84_geometry".to_string(),
        longitude: "-73.977264000".to_string(),
        latitude: "40.753429000".to_string(),
        transform_execution_id: Some("fixture-transform-execution".to_string()),
        transform_definition_id: Some("fixture-transform-definition".to_string()),
        claimed_home_cell: Some(center_cell().to_string()),
    }
}

fn tile_work_request() -> GeoTileWorkRequest {
    GeoTileWorkRequest {
        version: CANON_GEO_TILE_WORK_REQUEST_VERSION.to_string(),
        center_cell: center_cell().to_string(),
        halo_k: 1,
        features: vec![
            tile_feature_ref("building-a"),
            tile_feature_ref("building-b"),
        ],
        candidate_reach_reference: None,
        max_features: 16,
        max_work_cells: 7,
    }
}

fn tile_feature_ref(feature_id: &str) -> GeoTileFeatureRef {
    GeoTileFeatureRef {
        source: building_tile_source(),
        feature_id: feature_id.to_string(),
        home_cell: center_cell().to_string(),
    }
}

fn building_tile_source() -> GeoTileSourceBinding {
    GeoTileSourceBinding {
        source_instance_id: "source.fixture.buildings".to_string(),
        release: GeoSourceRelease {
            release_id: "fixture-release-2026-08-31".to_string(),
            release_digest: digest("fixture-release-2026-08-31"),
        },
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level: GeoControlEntityLevel::Building,
            identity_participation: GeoIdentityParticipation::StableAlias,
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: "inventory.fixture.accretion".to_string(),
            semantic_hash: digest("tile-inventory-semantic"),
            planning_hash: digest("tile-inventory-planning"),
        },
    }
}

fn warehouse_rows() -> GeoWarehouseRowsRequest {
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
        contracts: vec![rho_contract()],
        evidence_rows: vec![
            evidence_row("obs.building-set.b", "row-b"),
            evidence_row("obs.building-set.a", "row-a"),
        ],
        max_assignments: 128,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn warehouse_rows_plus_one_evidence_row() -> GeoWarehouseRowsRequest {
    let mut rows = warehouse_rows();
    rows.evidence_rows
        .push(evidence_row("obs.building-set.c", "row-c"));
    rows
}

fn evidence_row(observation_id: &str, record_id: &str) -> GeoWarehouseEvidenceRow {
    GeoWarehouseEvidenceRow {
        observation_id: observation_id.to_string(),
        contract_id: "rho.building-set".to_string(),
        source_record: record(record_id),
        valid_time: None,
        observation: GeoRhoObservationKind::ExactSets {
            level: GeoEntityLevel::Building,
            sets: vec![vec!["building-a".to_string(), "building-b".to_string()]],
        },
    }
}

fn rho_contract() -> GeoRhoContract {
    GeoRhoContract {
        id: "rho.building-set".to_string(),
        version: "1.0.0".to_string(),
        source_dataset: "fixture.buildings".to_string(),
        source_release: "2026-08-31".to_string(),
        source_lineage_ids: vec!["fixture.buildings.release.rho.building-set".to_string()],
        method_id: "fixture-building-candidate-set".to_string(),
        method_version: "1.0.0".to_string(),
        claim_role: GeoEvidenceClaimRole::StableIdentityAnchor,
        basis: GeoRhoBasis::LogicalRelaxation {
            invariant_id: "candidate-set-is-a-superset".to_string(),
        },
    }
}

fn record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "2026-08-31".to_string(),
        record_blake3: blake3::hash(id.as_bytes()).to_hex().to_string(),
    }
}

fn separation_inputs() -> GeoSeparationInputs {
    GeoSeparationInputs {
        version: CANON_GEO_SEPARATION_INPUTS_VERSION.to_string(),
        subject_ref: None,
        prospective: vec![GeoProspectiveObservation {
            id: "obs.prospective.binary-a".to_string(),
            contract_id: "rho.prospective.binary-a".to_string(),
            cost_units: 1,
            outcomes: vec![
                GeoProspectiveOutcome {
                    outcome_id: "outcome.forbid-a".to_string(),
                    induced: vec![GeoHardConstraintKind::Forbid {
                        member: building_member("building-a"),
                    }],
                },
                GeoProspectiveOutcome {
                    outcome_id: "outcome.require-a".to_string(),
                    induced: vec![GeoHardConstraintKind::Require {
                        member: building_member("building-a"),
                    }],
                },
            ],
        }],
    }
}

fn next_evidence_inputs() -> GeoNextEvidenceInputs {
    GeoNextEvidenceInputs {
        version: CANON_GEO_NEXT_EVIDENCE_INPUTS_VERSION.to_string(),
        candidates: vec![GeoNextEvidenceCandidateInput {
            action_id: "action.binary-a".to_string(),
            class: GeoNextActionClass::SeparateResidual,
            kind: GeoNextActionKind::Observe("obs.prospective.binary-a".to_string()),
            observation_id: Some("obs.prospective.binary-a".to_string()),
            cost_units: 1,
            redundant: false,
            lineage_ids: Vec::new(),
            stop_reason: None,
        }],
        policy: None,
        budget: budget(),
        budget_spent: BTreeMap::new(),
    }
}

fn building_member(id: &str) -> GeoEntityRef {
    GeoEntityRef::new(GeoEntityLevel::Building, id)
}

fn center_cell() -> CellIndex {
    CellIndex::from_str("892a100d62bffff").expect("valid fixture cell")
}

fn digest(label: &str) -> String {
    digest_bytes(label.as_bytes())
}

fn string_set(values: &[String]) -> BTreeSet<String> {
    values.iter().cloned().collect()
}

fn string_set_from<const N: usize>(values: [&str; N]) -> BTreeSet<String> {
    values.into_iter().map(str::to_string).collect()
}

fn read_geo_receipt(workspace: &Path, node_id: &str) -> canon::project::ProjectRunNodeReceipt {
    read_node_receipt(&receipt_path(workspace, node_id)).expect("read Geo node receipt")
}

fn receipt_path(workspace: &Path, node_id: &str) -> std::path::PathBuf {
    workspace
        .join("work/receipts")
        .join(format!("{}.json", node_id_token(node_id)))
}

fn node_id_token(node_id: &str) -> String {
    node_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn geo_run_manifest_revision_count(workspace: &Path) -> usize {
    let path = workspace.join("work/geo-run-manifest/revisions");
    fs::read_dir(path)
        .expect("Geo run manifest revisions directory")
        .count()
}
