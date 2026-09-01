#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_ACQUISITION_RECEIPT_VERSION, CANON_GEO_PLAN_VERSION, CANON_GEO_QUESTION_VERSION,
    CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION, CANON_GEO_REGIONAL_INVENTORY_VERSION,
    CANON_GEO_RESOURCE_BUDGET_VERSION, CANON_GEO_WAREHOUSE_ROWS_VERSION, GEO_RUN_JSON_MEDIA_TYPE,
    GeoAbstentionDisposition, GeoAbstentionPolicy, GeoAcquisitionArtifactReleaseRelation,
    GeoAcquisitionCounts, GeoAcquisitionDenominator, GeoAcquisitionProofClass,
    GeoAcquisitionReceipt, GeoAcquisitionRequest, GeoAcquisitionResumability,
    GeoAcquisitionTerminalState, GeoAsOf, GeoBoundedGeography, GeoBudgetAction, GeoClaimClass,
    GeoCompositionProfile, GeoControlEntityLevel, GeoCoveragePredicate, GeoDateInterval,
    GeoDenominatorSource, GeoDigest, GeoDigestAlgorithm, GeoDiscoveryGap, GeoEgressClass,
    GeoEvidenceClass, GeoExecutorKind, GeoExecutorTrace, GeoGeometryTransformContract,
    GeoIdentityParticipation, GeoInventoryAdvancementEffect, GeoLicenseClass,
    GeoLocalAcquisitionState, GeoLocalArtifactDigest, GeoLocalArtifactRef, GeoNativeEntityScope,
    GeoNumericBound, GeoNumericMeasure, GeoPaginationReceipt, GeoPlanErrorCode,
    GeoPlanExternalRequest, GeoPlanGatePlane, GeoPlanGateStatus, GeoPlanGrainStatus,
    GeoPlanReplanRequest, GeoPlanRequest, GeoPlanStage, GeoPlanStatus, GeoRegionalInventory,
    GeoRegionalInventoryAdvancement, GeoRegionalSourceInstance, GeoReleasePin,
    GeoReleaseSelectionMode, GeoRequestedGrain, GeoResourceBudget, GeoResourceCounter,
    GeoSatisfactionAssignment, GeoSatisfactionFileBinding, GeoSatisfactionInput,
    GeoSatisfactionStatus, GeoSourceAvailability, GeoSourceRelease, GeoSubjectBinding,
    GeoSubjectBindingClass, GeoSubsetPredicate, GeoSubsetPredicateKind, GeoTelemetryDeclaration,
    GeoTelemetryMetric, GeoTelemetrySemanticEffect, GeoTemporalScope, GeoValueOrigin,
    GeoWarehouseBuildingParcelRow, GeoWarehouseRowsRequest, canonical_geo_plan_bytes,
    capabilities_semantic_hash, compile_geo_plan, default_geo_capabilities,
    geo_acquisition_request_id, geo_acquisition_request_semantic_hash, geo_discovery_request_id,
    geo_plan_semantic_hash, geo_regional_inventory_advancement_semantic_hash,
    regional_inventory_planning_hash, regional_inventory_semantic_hash,
    replan_geo_plan_from_inventory_advancement, satisfy_geo_acquisition, validate_geo_plan,
};
use tempfile::tempdir;

fn digest(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}

fn region() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: "region.fixture.one".to_string(),
        geography_kind: "bounded_fixture".to_string(),
        description: "One explicitly bounded planning fixture".to_string(),
    }
}

fn question(include_parcel: bool) -> canon::geo::GeoQuestion {
    let mut requested_grains = vec![GeoRequestedGrain {
        entity_level: GeoControlEntityLevel::Building,
        required_evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
        optional_evidence_classes: vec![GeoEvidenceClass::AddressSet],
    }];
    if include_parcel {
        requested_grains.push(GeoRequestedGrain {
            entity_level: GeoControlEntityLevel::Parcel,
            required_evidence_classes: vec![GeoEvidenceClass::ParcelGeometry],
            optional_evidence_classes: Vec::new(),
        });
    }
    canon::geo::GeoQuestion {
        version: CANON_GEO_QUESTION_VERSION.to_string(),
        question_id: "question.fixture.plan".to_string(),
        subject_bindings: vec![GeoSubjectBinding {
            role: "target".to_string(),
            binding_class: GeoSubjectBindingClass::OperatorLabel,
            value: "fixture subject".to_string(),
        }],
        bounded_geography: region(),
        requested_grains,
        query_as_of: None,
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
        resource_budget_ref: "budget.fixture.plan".to_string(),
    }
}

fn budget() -> GeoResourceBudget {
    GeoResourceBudget {
        version: CANON_GEO_RESOURCE_BUDGET_VERSION.to_string(),
        budget_id: "budget.fixture.plan".to_string(),
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
        unit: format!("{:?}", counter).to_lowercase(),
        origin: GeoValueOrigin::CallerDeclared,
        action: GeoBudgetAction::ReportBudgetFallback,
    }
}

fn source(
    source_instance_id: &str,
    level: GeoControlEntityLevel,
    evidence_class: GeoEvidenceClass,
    availability: GeoSourceAvailability,
) -> GeoRegionalSourceInstance {
    GeoRegionalSourceInstance {
        source_instance_id: source_instance_id.to_string(),
        release: GeoSourceRelease {
            release_id: "release.fixture.one".to_string(),
            release_digest: digest("release.fixture.one"),
        },
        temporal_scope: GeoTemporalScope {
            valid_time: None,
            transaction_time: None,
            release_time: None,
        },
        lineage_ids: vec!["lineage.fixture.one".to_string()],
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level: level,
            identity_participation: GeoIdentityParticipation::StableAlias,
        },
        evidence_classes: vec![evidence_class],
        coverage: GeoCoveragePredicate {
            coverage_id: "coverage.fixture.one".to_string(),
            region: region(),
            predicate: "all declared fixture records".to_string(),
        },
        local_state: GeoLocalAcquisitionState {
            state: availability,
            local_ref: if availability == GeoSourceAvailability::Available {
                Some(GeoLocalArtifactRef {
                    artifact_id: format!("artifact.{source_instance_id}"),
                    contract_version: "canon_geo_warehouse_rows.v0".to_string(),
                    content_hash: digest("local.fixture.one"),
                    media_type: "application/json".to_string(),
                })
            } else {
                None
            },
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
    }
}

fn inventory(source_id: &str, availability: GeoSourceAvailability) -> GeoRegionalInventory {
    GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.fixture.plan".to_string(),
        region: region(),
        sources: vec![source(
            source_id,
            GeoControlEntityLevel::Building,
            GeoEvidenceClass::BuildingFootprint,
            availability,
        )],
        discovery_gaps: Vec::new(),
    }
}

fn request(
    question: canon::geo::GeoQuestion,
    inventory: GeoRegionalInventory,
    budget: GeoResourceBudget,
) -> GeoPlanRequest {
    GeoPlanRequest {
        question,
        capabilities: default_geo_capabilities().expect("capabilities"),
        inventory,
        profile: GeoCompositionProfile::building(),
        budget,
    }
}

#[test]
fn plans_one_bounded_factorized_building_chain_over_the_shared_project_dag() {
    let plan = compile_geo_plan(request(
        question(false),
        inventory(
            "arbitrary-building-source-a",
            GeoSourceAvailability::Available,
        ),
        budget(),
    ))
    .expect("Geo plan compiles");

    assert_eq!(plan.version, CANON_GEO_PLAN_VERSION);
    assert_eq!(plan.status, GeoPlanStatus::Planned);
    assert_eq!(plan.project_plan.nodes.len(), 5);
    assert_eq!(plan.project_plan.nodes.len(), plan.geo_nodes.len());
    assert!(plan.external_requests.is_empty());
    assert_eq!(
        plan.grain_outcomes[0].status,
        GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse
    );
    let solve = plan
        .geo_nodes
        .iter()
        .find(|node| node.stage == GeoPlanStage::FactorAndSolveExactResidual)
        .expect("solve overlay");
    assert!(solve.bounded_section_required);
    assert!(solve.incidence_factorization_required);
    let scope = solve
        .exact_solve_scope
        .as_ref()
        .expect("solve names its bounded section and incidence-component scope");
    let solve_project_node = plan
        .project_plan
        .nodes
        .iter()
        .find(|node| node.node_id == solve.project_node_id)
        .expect("solve project node");
    assert_eq!(
        solve_project_node.dependencies,
        vec![
            "geo.building.compile_evidence".to_string(),
            "geo.building.section".to_string()
        ]
    );
    assert_eq!(
        scope.bounded_section.producer_node_id,
        "geo.building.section"
    );
    assert_eq!(
        scope.evidence_compilation.producer_node_id,
        "geo.building.compile_evidence"
    );
    assert_eq!(
        scope.component_key_field,
        "canon_geo_composition.v0.factorization[].key"
    );
    assert!(
        solve
            .preconditions
            .iter()
            .any(|gate| gate.plane == GeoPlanGatePlane::CandidateReach
                && gate.detail.contains("not repaired by the solver"))
    );
    let allowed = request(
        question(false),
        inventory(
            "arbitrary-building-source-a",
            GeoSourceAvailability::Available,
        ),
        budget(),
    )
    .capabilities
    .commands
    .implemented
    .into_iter()
    .map(|command| command.command)
    .collect::<std::collections::BTreeSet<_>>();
    assert!(
        plan.project_plan
            .nodes
            .iter()
            .all(|node| allowed.contains(&node.command)),
        "the planner must emit only compiled operator-declared leaves"
    );
    assert_eq!(
        plan.project_plan
            .next_commands
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["geo.building.home_cells".to_string()]
    );
    assert!(plan.geo_nodes.iter().all(|node| {
        node.deterministic_bounds.len() == node.cost_estimate_ranges.len()
            && node.cost_estimate_ranges.iter().all(|estimate| {
                estimate.semantic_effect == GeoTelemetrySemanticEffect::None
                    && estimate.lower_bound <= estimate.upper_bound
            })
    }));
    validate_geo_plan(&plan).expect("plan validates");
    assert_eq!(
        canonical_geo_plan_bytes(&plan).expect("first bytes"),
        canonical_geo_plan_bytes(&plan).expect("second bytes")
    );
}

#[test]
fn evidence_only_source_cannot_plan_stable_identity_but_can_plan_other_claims() {
    let mut evidence_only_inventory = inventory(
        "arbitrary-evidence-only-buildings",
        GeoSourceAvailability::Available,
    );
    evidence_only_inventory.sources[0].native_scope = GeoNativeEntityScope::NativeEntity {
        entity_level: GeoControlEntityLevel::Building,
        identity_participation: GeoIdentityParticipation::EvidenceOnly,
    };

    let non_identity_plan = compile_geo_plan(request(
        question(false),
        evidence_only_inventory.clone(),
        budget(),
    ))
    .expect("evidence-only source plans non-identity composition");
    assert_eq!(non_identity_plan.status, GeoPlanStatus::Planned);
    assert!(
        non_identity_plan
            .geo_nodes
            .iter()
            .all(|node| !node.claim_classes.contains(&GeoClaimClass::StableIdentity))
    );

    let mut stable_identity_question = question(false);
    stable_identity_question
        .requested_claim_classes
        .push(GeoClaimClass::StableIdentity);
    let stable_identity_plan = compile_geo_plan(request(
        stable_identity_question.clone(),
        evidence_only_inventory,
        budget(),
    ))
    .expect("unsupported stable identity remains a typed plan outcome");
    assert_eq!(stable_identity_plan.status, GeoPlanStatus::Unsupported);
    assert!(stable_identity_plan.geo_nodes.is_empty());
    assert_eq!(
        stable_identity_plan.grain_outcomes[0].status,
        GeoPlanGrainStatus::UnsupportedByInventory
    );
    assert!(
        stable_identity_plan.grain_outcomes[0]
            .claim_limitation
            .contains("cannot begin")
    );

    let stable_identity_plan = compile_geo_plan(request(
        stable_identity_question,
        inventory(
            "arbitrary-stable-building-source",
            GeoSourceAvailability::Available,
        ),
        budget(),
    ))
    .expect("stable-alias source plans stable identity");
    assert_eq!(stable_identity_plan.status, GeoPlanStatus::Planned);
    assert!(
        stable_identity_plan
            .geo_nodes
            .iter()
            .all(|node| node.claim_classes.contains(&GeoClaimClass::StableIdentity))
    );
}

#[test]
fn parcel_free_inventory_preserves_building_plan_and_marks_parcel_unsupported() {
    let plan = compile_geo_plan(request(
        question(true),
        inventory("building-only-source", GeoSourceAvailability::Available),
        budget(),
    ))
    .expect("partial plan compiles");

    assert_eq!(plan.status, GeoPlanStatus::Partial);
    assert!(plan.grain_outcomes.iter().any(|outcome| {
        outcome.entity_level == GeoControlEntityLevel::Building
            && outcome.status == GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse
    }));
    assert!(plan.grain_outcomes.iter().any(|outcome| {
        outcome.entity_level == GeoControlEntityLevel::Parcel
            && outcome.status == GeoPlanGrainStatus::UnsupportedByProfile
    }));
    assert!(
        plan.project_plan
            .nodes
            .iter()
            .any(|node| node.node_id == "geo.building.solve")
    );
    assert!(
        !plan
            .project_plan
            .nodes
            .iter()
            .any(|node| node.node_id == "geo.parcel.solve")
    );
}

#[test]
fn missing_local_geometry_emits_typed_acquisition_and_no_solve() {
    let plan = compile_geo_plan(request(
        question(false),
        inventory("remote-building-source", GeoSourceAvailability::Missing),
        budget(),
    ))
    .expect("waiting plan compiles");

    assert_eq!(plan.status, GeoPlanStatus::Partial);
    assert_eq!(
        plan.grain_outcomes[0].status,
        GeoPlanGrainStatus::WaitingForAcquisition
    );
    assert_eq!(plan.external_requests.len(), 1);
    let GeoPlanExternalRequest::Acquisition { request, handoff } = &plan.external_requests[0]
    else {
        panic!("expected acquisition request");
    };
    assert_eq!(request.positive_path_min_rows, 1);
    assert_eq!(request.releases[0].release_id, "release.fixture.one");
    assert!(request.projection.is_some());
    assert_eq!(
        handoff.expected_receipt_contract,
        "canon_geo_acquisition_receipt.v0"
    );
    assert_eq!(
        handoff.required_result_digest_algorithm,
        canon::geo::GeoDigestAlgorithm::Blake3
    );
    assert!(handoff.continuation_command.starts_with("canon geo plan "));
    assert!(
        !plan
            .project_plan
            .nodes
            .iter()
            .any(|node| node.node_id.ends_with(".solve"))
    );
}

#[test]
fn available_but_unusable_local_contract_requires_non_overwriting_repair() {
    let mut inventory = inventory(
        "wrong-contract-building-source",
        GeoSourceAvailability::Available,
    );
    inventory.sources[0]
        .local_state
        .local_ref
        .as_mut()
        .expect("available fixture source has local ref")
        .contract_version = "canon_geo_unknown_rows.v9".to_string();

    let plan = compile_geo_plan(request(question(false), inventory, budget()))
        .expect("an unusable local representation gets an explicit repair finding");

    assert_eq!(plan.status, GeoPlanStatus::Unsupported);
    assert_eq!(
        plan.grain_outcomes[0].status,
        GeoPlanGrainStatus::UnsupportedByInventory
    );
    assert!(plan.external_requests.is_empty());
    assert!(plan.project_plan.nodes.is_empty());
    assert!(
        plan.grain_outcomes[0]
            .next_action
            .contains("distinct versioned source instance")
    );
    assert!(
        plan.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("cannot overwrite"))
    );
}

#[test]
fn replans_from_validated_advanced_inventory_into_new_bounded_plan() {
    let question = question(false);
    let base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    assert_eq!(base_plan.status, GeoPlanStatus::Partial);
    assert!(base_plan.project_plan.nodes.is_empty());

    let advancement = validated_inventory_advancement(&base_plan, &base_inventory);
    let replanned = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan: base_plan.clone(),
        base_inventory: base_inventory.clone(),
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement.clone(),
    })
    .expect("explicit replan from advanced inventory");

    assert_ne!(replanned.plan_id, base_plan.plan_id);
    assert_ne!(replanned.semantic_hash, base_plan.semantic_hash);
    assert_eq!(replanned.status, GeoPlanStatus::Planned);
    assert_eq!(replanned.external_requests, Vec::new());
    assert_eq!(replanned.project_plan.nodes.len(), 5);
    assert!(replanned.project_plan.nodes.iter().all(|node| {
        node.command.starts_with("canon geo ")
            && !node.command.contains("link-sources")
            && !node.command.contains("reconcile-tiles")
            && !node.command.contains("materialize-geometry")
    }));
    assert_eq!(
        replanned.inventory_ref.semantic_hash,
        advancement.advanced_inventory_semantic_hash
    );
    assert_eq!(
        replanned.inventory_ref.planning_hash,
        regional_inventory_planning_hash(&advancement.advanced_inventory)
            .expect("advanced planning hash")
    );
    assert!(replanned.geo_nodes.iter().any(|node| node.stage
        == GeoPlanStage::FactorAndSolveExactResidual
        && node.bounded_section_required
        && node.incidence_factorization_required));
    assert_eq!(
        base_plan.project_plan.nodes.len(),
        0,
        "explicit replan must not mutate the original plan"
    );
    validate_geo_plan(&replanned).expect("replanned artifact validates");
}

#[test]
fn replan_identity_is_source_instance_and_telemetry_independent() {
    let question = question(false);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget_a = budget();
    let mut budget_b = budget();
    budget_b.telemetry = vec![GeoTelemetryDeclaration {
        metric: GeoTelemetryMetric::CpuTime,
        unit: "microsecond".to_string(),
        origin: GeoValueOrigin::OperatorPolicy,
        semantic_effect: GeoTelemetrySemanticEffect::None,
    }];
    let inventory_a = inventory("remote-source-a", GeoSourceAvailability::Missing);
    let inventory_b = inventory("renamed-remote-source-z", GeoSourceAvailability::Missing);

    let base_a = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: inventory_a.clone(),
        profile: profile.clone(),
        budget: budget_a.clone(),
    })
    .expect("first base acquisition plan");
    let base_b = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: inventory_b.clone(),
        profile: profile.clone(),
        budget: budget_b.clone(),
    })
    .expect("renamed base acquisition plan");
    assert_eq!(base_a.plan_id, base_b.plan_id);

    let advancement_a = validated_inventory_advancement(&base_a, &inventory_a);
    let advancement_b = validated_inventory_advancement(&base_b, &inventory_b);
    let replanned_a = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan: base_a,
        base_inventory: inventory_a.clone(),
        question: question.clone(),
        capabilities: capabilities.clone(),
        profile: profile.clone(),
        budget: budget_a,
        inventory_advancement: advancement_a,
    })
    .expect("first explicit replan");
    let replanned_b = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan: base_b,
        base_inventory: inventory_b.clone(),
        question,
        capabilities,
        profile,
        budget: budget_b,
        inventory_advancement: advancement_b,
    })
    .expect("renamed explicit replan");

    assert_ne!(
        replanned_a.inventory_ref.semantic_hash,
        replanned_b.inventory_ref.semantic_hash
    );
    assert_eq!(
        replanned_a.inventory_ref.planning_hash,
        replanned_b.inventory_ref.planning_hash
    );
    assert_eq!(
        replanned_a.project_plan.graph_hash,
        replanned_b.project_plan.graph_hash
    );
    assert_eq!(replanned_a.semantic_hash, replanned_b.semantic_hash);
    assert_eq!(replanned_a.plan_id, replanned_b.plan_id);
    assert_eq!(replanned_a.project_plan.nodes.len(), 5);
    validate_geo_plan(&replanned_a).expect("first replanned artifact validates");
    validate_geo_plan(&replanned_b).expect("renamed replanned artifact validates");
}

#[test]
fn replan_rejects_question_that_does_not_match_base_plan() {
    let question = question(false);
    let base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    let advancement = validated_inventory_advancement(&base_plan, &base_inventory);
    let mut wrong_question = question;
    wrong_question.question_id = "question.fixture.other".to_string();

    let error = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory,
        question: wrong_question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    })
    .expect_err("replan must bind the same original question artifact");

    assert_eq!(error.code, GeoPlanErrorCode::ContractViolation);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("question")
    );
}

#[test]
fn replan_rejects_noop_advanced_inventory() {
    let question = question(false);
    let base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    let mut advancement = validated_inventory_advancement(&base_plan, &base_inventory);
    advancement.advanced_inventory = base_inventory;
    advancement.advanced_inventory_id = base_plan.inventory_ref.inventory_id.clone();
    advancement.advanced_inventory_semantic_hash = base_plan.inventory_ref.semantic_hash.clone();

    let error = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory: inventory("remote-building-source", GeoSourceAvailability::Missing),
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    })
    .expect_err("replan must reject no-op inventory advancement");

    assert_eq!(error.code, GeoPlanErrorCode::ContractViolation);
    assert!(
        error
            .message
            .contains("inventory must carry each advanced local artifact exactly once"),
        "a no-op inventory cannot carry the advancement's newly available source artifact"
    );
}

#[test]
fn replan_rejects_tampered_advanced_inventory_artifact() {
    let question = question(false);
    let base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    let mut advancement = validated_inventory_advancement(&base_plan, &base_inventory);
    advancement
        .advanced_inventory
        .discovery_gaps
        .push(GeoDiscoveryGap {
            gap_id: "gap.fixture.tampered".to_string(),
            requested_entity_level: Some(GeoControlEntityLevel::Building),
            requested_evidence_class: GeoEvidenceClass::AddressSet,
            reason: "planted semantic-hash drift".to_string(),
            next_command: "canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>".to_string(),
        });

    let error = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory,
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    })
    .expect_err("replan must validate the advanced inventory content");

    assert_eq!(error.code, GeoPlanErrorCode::ContractViolation);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("advanced_inventory_semantic_hash")
    );
}

#[test]
fn replan_rejects_bounded_subset_drift_from_its_declared_hash() {
    let question = question(false);
    let base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    let mut advancement = validated_inventory_advancement(&base_plan, &base_inventory);
    advancement.bounded_subset.predicates[0].expression = "region.fixture.other".to_string();
    refresh_advancement_identity(&mut advancement);

    let error = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory,
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    })
    .expect_err("replan must bind bounded subset bytes to their declared hash");

    assert_eq!(error.code, GeoPlanErrorCode::ContractViolation);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("bounded_subset_hash")
    );
}

#[test]
fn replan_rejects_self_consistent_bounded_subset_drift_from_acquisition_request() {
    let question = question(false);
    let base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    let mut advancement = validated_inventory_advancement(&base_plan, &base_inventory);
    advancement.bounded_subset.predicates[0].expression = "region.fixture.other".to_string();
    advancement.bounded_subset_hash = format!(
        "blake3:{}",
        blake3::hash(
            &serde_json::to_vec(&advancement.bounded_subset).expect("bounded subset serializes")
        )
        .to_hex()
    );
    refresh_advancement_identity(&mut advancement);

    let error = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory,
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    })
    .expect_err("replan must bind bounded subset bytes to the base acquisition request");

    assert_eq!(error.code, GeoPlanErrorCode::ContractViolation);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("bounded_subset")
    );
}

#[test]
fn replan_accepts_nonsemantic_bounded_subset_order_from_validated_receipt() {
    let question = question(false);
    let base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let mut base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    let GeoPlanExternalRequest::Acquisition { request, .. } = &mut base_plan.external_requests[0]
    else {
        panic!("fixture plan must contain an acquisition request");
    };
    request.subset.predicates.push(GeoSubsetPredicate {
        predicate_id: "question_bounded_geography.secondary".to_string(),
        kind: GeoSubsetPredicateKind::AdministrativeBoundary,
        expression: request.bounded_geography.geography_id.clone(),
    });
    request.subset.predicates.sort();
    request.request_id = geo_acquisition_request_id(request).expect("refresh request id");
    base_plan.semantic_hash = geo_plan_semantic_hash(&base_plan).expect("refresh plan hash");
    base_plan.plan_id = format!(
        "{}:{}",
        CANON_GEO_PLAN_VERSION,
        base_plan.semantic_hash.trim_start_matches("blake3:")
    );
    validate_geo_plan(&base_plan).expect("expanded acquisition plan validates");

    let mut advancement = validated_inventory_advancement(&base_plan, &base_inventory);
    advancement.bounded_subset.predicates.reverse();
    advancement.bounded_subset_hash = format!(
        "blake3:{}",
        blake3::hash(
            &serde_json::to_vec(&advancement.bounded_subset).expect("bounded subset serializes")
        )
        .to_hex()
    );
    refresh_advancement_identity(&mut advancement);

    let replanned = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory,
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    })
    .expect("nonsemantic subset order must remain admissible");

    assert_eq!(replanned.status, GeoPlanStatus::Planned);
    assert_eq!(replanned.project_plan.nodes.len(), 5);
}

#[test]
fn replan_rejects_local_artifact_relabelled_away_from_receipt_result_digest() {
    let question = question(false);
    let base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    let mut advancement = validated_inventory_advancement(&base_plan, &base_inventory);
    let relabelled_hash = digest("not-the-receipt-result");
    advancement.source_advancements[0].local_ref.content_hash = relabelled_hash.clone();
    advancement.advanced_inventory.sources[0]
        .local_state
        .local_ref
        .as_mut()
        .expect("advanced local ref")
        .content_hash = relabelled_hash;
    refresh_advancement_identity(&mut advancement);

    let error = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory,
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    })
    .expect_err("replan must retain the receipt result-digest binding");

    assert_eq!(error.code, GeoPlanErrorCode::ContractViolation);
    assert!(error.message.contains("receipt result digest"));
}

#[test]
fn replan_rejects_partial_release_coverage_in_a_multi_release_advancement() {
    let question = question(false);
    let mut base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let mut base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    let mut advancement = validated_inventory_advancement(&base_plan, &base_inventory);

    let second_release_digest = geo_digest("release.second", b"release second");
    let mut second_source = source(
        "remote-building-source-second",
        GeoControlEntityLevel::Building,
        GeoEvidenceClass::BuildingFootprint,
        GeoSourceAvailability::Missing,
    );
    second_source.release = GeoSourceRelease {
        release_id: "release.fixture.second".to_string(),
        release_digest: format!("blake3:{}", second_release_digest.hex_digest),
    };
    base_inventory.sources.push(second_source.clone());
    base_inventory.sources.sort();
    advancement.advanced_inventory.sources.push(second_source);
    advancement.advanced_inventory.sources.sort();

    let GeoPlanExternalRequest::Acquisition { request, .. } = &mut base_plan.external_requests[0]
    else {
        panic!("fixture plan must contain an acquisition request");
    };
    request.releases.push(GeoReleasePin {
        source_instance_id: "remote-building-source-second".to_string(),
        release_id: "release.fixture.second".to_string(),
        release_digest: second_release_digest,
    });
    request.releases.sort();
    request.request_id = geo_acquisition_request_id(request).expect("refresh request id");
    let request_id = request.request_id.clone();
    let request_semantic_hash =
        geo_acquisition_request_semantic_hash(request).expect("request semantic hash");

    base_plan.inventory_ref.semantic_hash =
        regional_inventory_semantic_hash(&base_inventory).expect("base inventory semantic hash");
    base_plan.inventory_ref.planning_hash =
        regional_inventory_planning_hash(&base_inventory).expect("base inventory planning hash");
    base_plan.semantic_hash = geo_plan_semantic_hash(&base_plan).expect("refresh plan hash");
    base_plan.plan_id = format!(
        "{}:{}",
        CANON_GEO_PLAN_VERSION,
        base_plan.semantic_hash.trim_start_matches("blake3:")
    );

    advancement.plan_id = base_plan.plan_id.clone();
    advancement.plan_semantic_hash = base_plan.semantic_hash.clone();
    advancement.request_id = request_id;
    advancement.request_semantic_hash = request_semantic_hash;
    advancement.base_inventory_semantic_hash = base_plan.inventory_ref.semantic_hash.clone();
    refresh_advancement_identity(&mut advancement);

    let error = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory,
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    })
    .expect_err("a multi-release acquisition cannot advance only a strict subset of releases");

    assert_eq!(error.code, GeoPlanErrorCode::ContractViolation);
    assert!(
        error
            .message
            .contains("cover every pinned acquisition release")
    );
    assert_eq!(
        error
            .detail
            .get("expected_release_count")
            .map(String::as_str),
        Some("2")
    );
    assert_eq!(
        error
            .detail
            .get("advanced_release_count")
            .map(String::as_str),
        Some("1")
    );
}

#[test]
fn replan_rejects_forged_nested_receipt_execution() {
    let question = question(false);
    let base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    let mut advancement = validated_inventory_advancement(&base_plan, &base_inventory);
    advancement.receipt_execution.proof_class = GeoAcquisitionProofClass::Retained;
    advancement.receipt_execution.terminal_state = GeoAcquisitionTerminalState::ZeroRows;
    advancement.receipt_execution.retained_receipt_id = Some("retained.forged".to_string());
    advancement.receipt_execution.executor_request_id = None;
    advancement.receipt_execution.executor_query_id = None;

    let error = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory,
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    })
    .expect_err("replan must validate nested receipt execution, not only top-level proof fields");

    assert_eq!(error.code, GeoPlanErrorCode::ContractViolation);
    assert!(error.message.contains("receipt execution"));
}

#[test]
fn replan_rejects_self_consistent_undeclared_extra_source() {
    let question = question(false);
    let base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    let mut advancement = validated_inventory_advancement(&base_plan, &base_inventory);
    let mut extra_source = source(
        "undeclared-extra-source",
        GeoControlEntityLevel::Building,
        GeoEvidenceClass::BuildingFootprint,
        GeoSourceAvailability::Available,
    );
    extra_source.release = GeoSourceRelease {
        release_id: "release.fixture.extra".to_string(),
        release_digest: digest("release.fixture.extra"),
    };
    extra_source
        .local_state
        .local_ref
        .as_mut()
        .expect("extra available source")
        .content_hash = digest("undeclared-extra-source-bytes");
    advancement.advanced_inventory.sources.push(extra_source);
    refresh_advancement_identity(&mut advancement);

    let error = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory,
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    })
    .expect_err("replan must reject undeclared advanced-inventory source additions");

    assert_eq!(error.code, GeoPlanErrorCode::ContractViolation);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("advanced_inventory_transition")
    );
}

#[test]
fn replan_rejects_self_consistent_undeclared_source_change() {
    let question = question(false);
    let base_inventory = inventory("remote-building-source", GeoSourceAvailability::Missing);
    let capabilities = default_geo_capabilities().expect("capabilities");
    let profile = GeoCompositionProfile::building();
    let budget = budget();
    let base_plan = compile_geo_plan(GeoPlanRequest {
        question: question.clone(),
        capabilities: capabilities.clone(),
        inventory: base_inventory.clone(),
        profile: profile.clone(),
        budget: budget.clone(),
    })
    .expect("base acquisition plan");
    let mut advancement = validated_inventory_advancement(&base_plan, &base_inventory);
    advancement.advanced_inventory.sources[0]
        .temporal_scope
        .release_time = Some(GeoAsOf {
        utc_day: "2026-09-01".to_string(),
        semantic_id: "release_time.fixture.tampered".to_string(),
        unit: "utc_day".to_string(),
        origin: GeoValueOrigin::CallerDeclared,
    });
    refresh_advancement_identity(&mut advancement);

    let error = replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory,
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    })
    .expect_err("replan must reject undeclared advanced-inventory source mutations");

    assert_eq!(error.code, GeoPlanErrorCode::ContractViolation);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("advanced_inventory_transition")
    );
}

#[test]
fn unknown_source_with_as_of_emits_a_typed_discovery_request() {
    let mut discovery_question = question(false);
    discovery_question.query_as_of = Some(GeoAsOf {
        utc_day: "2026-08-31".to_string(),
        semantic_id: "query.as_of".to_string(),
        unit: "utc_day".to_string(),
        origin: GeoValueOrigin::CallerDeclared,
    });
    let inventory = GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.discovery.plan".to_string(),
        region: region(),
        sources: Vec::new(),
        discovery_gaps: vec![GeoDiscoveryGap {
            gap_id: "gap.building_footprint".to_string(),
            requested_entity_level: Some(GeoControlEntityLevel::Building),
            requested_evidence_class: GeoEvidenceClass::BuildingFootprint,
            reason: "no local source instance is declared".to_string(),
            next_command: "route the typed discovery request to an external catalog executor"
                .to_string(),
        }],
    };

    let plan = compile_geo_plan(request(discovery_question, inventory, budget()))
        .expect("discovery plan compiles");

    assert_eq!(plan.status, GeoPlanStatus::Partial);
    assert!(plan.project_plan.nodes.is_empty());
    let GeoPlanExternalRequest::Discovery { request, .. } = &plan.external_requests[0] else {
        panic!("expected typed discovery request");
    };
    assert_eq!(
        request.release_selection.mode,
        GeoReleaseSelectionMode::LatestNotAfterAsOf
    );
    assert_eq!(request.release_selection.as_of_utc_day, "2026-08-31");
    assert_eq!(request.column_readability_probe.fields.len(), 3);
}

#[test]
fn discovery_handoff_ids_and_prose_do_not_change_planning_identity() {
    let mut discovery_question = question(false);
    discovery_question.query_as_of = Some(GeoAsOf {
        utc_day: "2026-08-31".to_string(),
        semantic_id: "query.as_of".to_string(),
        unit: "utc_day".to_string(),
        origin: GeoValueOrigin::CallerDeclared,
    });
    let inventory = GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.discovery.plan".to_string(),
        region: region(),
        sources: Vec::new(),
        discovery_gaps: vec![GeoDiscoveryGap {
            gap_id: "gap.building_footprint".to_string(),
            requested_entity_level: Some(GeoControlEntityLevel::Building),
            requested_evidence_class: GeoEvidenceClass::BuildingFootprint,
            reason: "no local source instance is declared".to_string(),
            next_command: "route to an external catalog executor".to_string(),
        }],
    };
    let plan =
        compile_geo_plan(request(discovery_question, inventory, budget())).expect("discovery plan");
    let mut relabelled = plan.clone();
    let GeoPlanExternalRequest::Discovery { gap_id, request } =
        &mut relabelled.external_requests[0]
    else {
        panic!("expected discovery request");
    };
    *gap_id = "renamed.operator.gap".to_string();
    request.column_readability_probe.probe_id = "renamed.operator.probe".to_string();
    request.request_id = geo_discovery_request_id(request).expect("recompute request id");

    assert_ne!(plan.external_requests, relabelled.external_requests);
    assert_eq!(
        geo_plan_semantic_hash(&plan).expect("original hash"),
        geo_plan_semantic_hash(&relabelled).expect("relabelled hash")
    );
    validate_geo_plan(&relabelled).expect("relabelled discovery provenance remains valid");

    let canonical = canonical_geo_plan_bytes(&plan).expect("canonical discovery plan");
    let mut reordered = plan.clone();
    let GeoPlanExternalRequest::Discovery { request, .. } = &mut reordered.external_requests[0]
    else {
        panic!("expected discovery request");
    };
    request.fields.reverse();
    request.required_steps.reverse();
    request.column_readability_probe.fields.reverse();
    validate_geo_plan(&reordered).expect("nested discovery order is not semantic identity");
    assert_eq!(
        canonical_geo_plan_bytes(&reordered).expect("canonical reordered discovery bytes"),
        canonical
    );
}

#[test]
fn source_names_and_telemetry_do_not_change_planning_identity() {
    let first = compile_geo_plan(request(
        question(false),
        inventory("arbitrary-source-a", GeoSourceAvailability::Available),
        budget(),
    ))
    .expect("first plan");
    let mut changed_budget = budget();
    changed_budget.telemetry = vec![GeoTelemetryDeclaration {
        metric: GeoTelemetryMetric::CpuTime,
        unit: "microsecond".to_string(),
        origin: GeoValueOrigin::OperatorPolicy,
        semantic_effect: GeoTelemetrySemanticEffect::None,
    }];
    let second = compile_geo_plan(request(
        question(false),
        inventory("renamed-source-z", GeoSourceAvailability::Available),
        changed_budget,
    ))
    .expect("second plan");

    assert_ne!(
        first.inventory_ref.semantic_hash,
        second.inventory_ref.semantic_hash
    );
    assert_ne!(
        first.budget_ref.semantic_hash,
        second.budget_ref.semantic_hash
    );
    assert_eq!(
        first.inventory_ref.planning_hash,
        second.inventory_ref.planning_hash
    );
    assert_eq!(
        first.budget_ref.planning_hash,
        second.budget_ref.planning_hash
    );
    assert_eq!(
        first.project_plan.graph_hash,
        second.project_plan.graph_hash
    );
    assert_eq!(first.semantic_hash, second.semantic_hash);
    assert_eq!(first.plan_id, second.plan_id);
}

#[test]
fn canonical_plan_bytes_normalize_semantically_unordered_external_requests() {
    let mut multi_request_question = question(false);
    multi_request_question.requested_grains[0]
        .required_evidence_classes
        .push(GeoEvidenceClass::AddressSet);
    multi_request_question.requested_grains[0]
        .required_evidence_classes
        .sort();
    let mut multi_source_inventory =
        inventory("remote-building-source", GeoSourceAvailability::Missing);
    multi_source_inventory.sources.push(source(
        "remote-address-source",
        GeoControlEntityLevel::Building,
        GeoEvidenceClass::AddressSet,
        GeoSourceAvailability::Missing,
    ));
    multi_source_inventory.sources.sort();

    let plan = compile_geo_plan(request(
        multi_request_question,
        multi_source_inventory,
        budget(),
    ))
    .expect("two-source acquisition plan");
    assert_eq!(plan.external_requests.len(), 2);
    let canonical = canonical_geo_plan_bytes(&plan).expect("canonical original bytes");

    let mut reordered = plan.clone();
    reordered.external_requests.reverse();
    for external_request in &mut reordered.external_requests {
        let GeoPlanExternalRequest::Acquisition { request, .. } = external_request else {
            panic!("expected acquisition request");
        };
        request.fields.reverse();
        request.ordering.reverse();
    }
    validate_geo_plan(&reordered).expect("request order is not semantic identity");
    assert_eq!(
        canonical_geo_plan_bytes(&reordered).expect("canonical reordered bytes"),
        canonical
    );
}

#[test]
fn unavailable_source_name_is_provenance_not_acquisition_planning_identity() {
    let first = compile_geo_plan(request(
        question(false),
        inventory("remote-source-a", GeoSourceAvailability::Missing),
        budget(),
    ))
    .expect("first acquisition plan");
    let second = compile_geo_plan(request(
        question(false),
        inventory("renamed-remote-source-z", GeoSourceAvailability::Missing),
        budget(),
    ))
    .expect("renamed acquisition plan");

    assert_ne!(first.external_requests, second.external_requests);
    assert_eq!(
        first.inventory_ref.planning_hash,
        second.inventory_ref.planning_hash
    );
    assert_eq!(first.semantic_hash, second.semantic_hash);
    assert_eq!(first.plan_id, second.plan_id);
}

#[test]
fn explanatory_prose_does_not_change_planning_identity() {
    let plan = compile_geo_plan(request(
        question(false),
        inventory("building-source", GeoSourceAvailability::Available),
        budget(),
    ))
    .expect("plan");
    let original_bytes = canonical_geo_plan_bytes(&plan).expect("canonical plan");
    let mut reworded = plan.clone();
    reworded
        .diagnostics
        .push("operator-only diagnostic".to_string());
    reworded.grain_outcomes[0].claim_limitation = "reworded explanation".to_string();
    reworded.grain_outcomes[0].next_action = "reworded operator handoff".to_string();
    reworded.geo_nodes[0].preconditions[0].detail = "reworded gate explanation".to_string();
    reworded.geo_nodes[0].cost_estimate_ranges[0].basis =
        "reworded non-semantic estimate basis".to_string();
    reworded.geo_nodes[0].transitions.success = "reworded success handoff".to_string();

    assert_eq!(
        geo_plan_semantic_hash(&plan).expect("original hash"),
        geo_plan_semantic_hash(&reworded).expect("reworded hash")
    );
    validate_geo_plan(&reworded).expect("reworded prose preserves semantic validity");
    assert_ne!(
        original_bytes,
        canonical_geo_plan_bytes(&reworded).expect("reworded canonical plan")
    );
}

#[test]
fn validation_rejects_a_solve_relabelled_as_unbounded() {
    let mut plan = compile_geo_plan(request(
        question(false),
        inventory("building-source", GeoSourceAvailability::Available),
        budget(),
    ))
    .expect("plan");
    let solve = plan
        .geo_nodes
        .iter_mut()
        .find(|node| node.stage == GeoPlanStage::FactorAndSolveExactResidual)
        .expect("solve overlay");
    solve.bounded_section_required = false;
    let error = validate_geo_plan(&plan).expect_err("unbounded solve refuses");
    assert!(error.message.contains("bounded section"));
}

#[test]
fn validation_rejects_a_solve_after_failed_candidate_reach() {
    let mut plan = compile_geo_plan(request(
        question(false),
        inventory("building-source", GeoSourceAvailability::Available),
        budget(),
    ))
    .expect("plan");
    let solve = plan
        .geo_nodes
        .iter_mut()
        .find(|node| node.stage == GeoPlanStage::FactorAndSolveExactResidual)
        .expect("solve overlay");
    let reach = solve
        .preconditions
        .iter_mut()
        .find(|precondition| precondition.plane == GeoPlanGatePlane::CandidateReach)
        .expect("reach precondition");
    reach.status = GeoPlanGateStatus::FailedAgainstReference;

    let error = validate_geo_plan(&plan).expect_err("known failed reach blocks solve");
    assert!(error.message.contains("must stop the grain"));
}

#[test]
fn planner_does_not_emit_a_leaf_with_a_mismatched_capability_contract() {
    let mut request = request(
        question(false),
        inventory("building-source", GeoSourceAvailability::Available),
        budget(),
    );
    let solve = request
        .capabilities
        .commands
        .implemented
        .iter_mut()
        .find(|command| command.command == "canon geo solve --request <REQUEST.json>")
        .expect("solve capability");
    solve.output_contract = "wrong.contract.v0".to_string();
    request.capabilities.semantic_hash =
        capabilities_semantic_hash(&request.capabilities).expect("capability hash");

    let plan = compile_geo_plan(request).expect("missing leaf is a typed per-grain outcome");
    assert_eq!(
        plan.grain_outcomes[0].status,
        GeoPlanGrainStatus::MissingLeafCapability
    );
    assert!(plan.project_plan.nodes.is_empty());
}

#[test]
fn supported_grain_requires_positive_deterministic_solver_ceilings() {
    let mut incomplete_budget = budget();
    incomplete_budget
        .deterministic_bounds
        .retain(|bound| bound.counter != GeoResourceCounter::States);
    let error = compile_geo_plan(request(
        question(false),
        inventory("building-source", GeoSourceAvailability::Available),
        incomplete_budget,
    ))
    .expect_err("unbounded solver state space must refuse during planning");

    assert_eq!(error.code, canon::geo::GeoPlanErrorCode::InvalidInput);
    assert_eq!(error.detail["missing_counters"], "states");
}

fn validated_inventory_advancement(
    plan: &canon::geo::GeoPlan,
    inventory: &GeoRegionalInventory,
) -> GeoRegionalInventoryAdvancement {
    let dir = tempdir().expect("tempdir");
    let data_path = dir.path().join("warehouse-rows.json");
    let receipt_path = dir.path().join("receipt.json");
    let data = warehouse_rows_artifact();
    std::fs::write(&data_path, &data).expect("write warehouse rows");
    let acquisition = plan_acquisition_request(plan);
    let receipt = acquisition_receipt(acquisition, &data);
    write_json(&receipt_path, &receipt);

    let satisfaction = satisfy_geo_acquisition(GeoSatisfactionInput {
        plan,
        inventory: Some(inventory),
        assignment: GeoSatisfactionAssignment {
            request_id: acquisition.request_id.clone(),
            receipt_path,
        },
        local_artifact_files: vec![file_binding("artifact.rows", &data_path)],
        result_digest_files: Vec::new(),
    })
    .expect("validated acquisition satisfaction");

    assert_eq!(satisfaction.status, GeoSatisfactionStatus::Satisfied);
    let advancement = satisfaction
        .inventory_advancement
        .expect("warehouse rows advance regional inventory");
    assert_eq!(
        advancement.effect,
        GeoInventoryAdvancementEffect::LocalAvailabilityOnly
    );
    advancement
}

fn refresh_advancement_identity(advancement: &mut GeoRegionalInventoryAdvancement) {
    advancement.advanced_inventory_semantic_hash =
        regional_inventory_semantic_hash(&advancement.advanced_inventory)
            .expect("advanced inventory semantic hash");
    advancement.semantic_hash = geo_regional_inventory_advancement_semantic_hash(advancement)
        .expect("advancement semantic hash");
    advancement.advancement_id = format!(
        "{}:{}",
        CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION,
        advancement.semantic_hash.trim_start_matches("blake3:")
    );
}

fn plan_acquisition_request(plan: &canon::geo::GeoPlan) -> &GeoAcquisitionRequest {
    let [GeoPlanExternalRequest::Acquisition { request, .. }] = plan.external_requests.as_slice()
    else {
        panic!("fixture plan must contain exactly one acquisition request");
    };
    request
}

fn acquisition_receipt(request: &GeoAcquisitionRequest, bytes: &[u8]) -> GeoAcquisitionReceipt {
    GeoAcquisitionReceipt {
        version: CANON_GEO_ACQUISITION_RECEIPT_VERSION.to_string(),
        request_id: request.request_id.clone(),
        request_semantic_hash: geo_acquisition_request_semantic_hash(request)
            .expect("request semantic hash"),
        terminal_state: GeoAcquisitionTerminalState::Complete,
        proof_class: GeoAcquisitionProofClass::Live,
        executor: Some(GeoExecutorTrace {
            executor_kind: GeoExecutorKind::QueryEngine,
            executor_id: "fixture.external.executor".to_string(),
            executor_version: "2026-09-01".to_string(),
            tool_id: "fixture.query".to_string(),
            tool_version: "1".to_string(),
            executor_request_id: "request.fixture.1".to_string(),
            executor_query_id: "query.fixture.1".to_string(),
            executor_attempt_id: None,
        }),
        fixture_id: None,
        retained_receipt_id: None,
        bounded_geography: request.bounded_geography.clone(),
        subset: request.subset.clone(),
        releases: request.releases.clone(),
        fields: request.fields.clone(),
        projection: request.projection.clone(),
        normalized_executed_request_digest: geo_digest("normalized_request", b"request"),
        pagination: GeoPaginationReceipt {
            requested_page: request.pagination.clone(),
            next_page_token: None,
            rows_truncated: false,
            bytes_truncated: false,
        },
        counts: GeoAcquisitionCounts {
            rows: 1,
            bytes: bytes.len() as u64,
        },
        denominators: vec![GeoAcquisitionDenominator {
            denominator_id: "requested_subset".to_string(),
            source: GeoDenominatorSource::RequestedSubset,
            count: 1,
            unit: "row".to_string(),
            description: "fixture requested subset denominator".to_string(),
        }],
        source_digests: vec![geo_digest("source", b"source bytes")],
        result_digests: vec![geo_digest("result", bytes)],
        local_artifacts: vec![GeoLocalArtifactDigest {
            artifact_id: "artifact.rows".to_string(),
            media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
            byte_count: bytes.len() as u64,
            digest: geo_digest("artifact.rows", bytes),
        }],
        artifact_release_relations: vec![artifact_release_relation(request)],
        unreadable_columns: Vec::new(),
        resumability: GeoAcquisitionResumability {
            resumable: false,
            resume_token: None,
            resume_request_id: None,
            retry_guidance: "satisfy the same request with a fresh receipt".to_string(),
        },
        terminal_detail: None,
    }
}

fn artifact_release_relation(
    request: &GeoAcquisitionRequest,
) -> GeoAcquisitionArtifactReleaseRelation {
    let release = request
        .releases
        .first()
        .expect("fixture request has one release pin");
    GeoAcquisitionArtifactReleaseRelation {
        local_artifact_id: "artifact.rows".to_string(),
        source_instance_id: release.source_instance_id.clone(),
        release_id: release.release_id.clone(),
        release_digest: format!("blake3:{}", release.release_digest.hex_digest),
    }
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
    .expect("warehouse rows serialize")
}

fn geo_digest(id: &str, bytes: &[u8]) -> GeoDigest {
    GeoDigest {
        digest_id: id.to_string(),
        algorithm: GeoDigestAlgorithm::Blake3,
        hex_digest: blake3::hash(bytes).to_hex().to_string(),
    }
}

fn file_binding(id: &str, path: &std::path::Path) -> GeoSatisfactionFileBinding {
    GeoSatisfactionFileBinding {
        binding_id: id.to_string(),
        path: path.to_path_buf(),
    }
}

fn write_json(path: &std::path::Path, value: &impl serde::Serialize) {
    let bytes = serde_json::to_vec(value).expect("serialize json");
    std::fs::write(path, bytes).expect("write json");
}

// Keep these types exercised so future temporal additions do not silently
// project source time into a timeless planning contract.
#[allow(dead_code)]
fn _temporal_contract_examples() -> (GeoAsOf, GeoDateInterval) {
    (
        GeoAsOf {
            utc_day: "2026-08-31".to_string(),
            semantic_id: "query.as_of".to_string(),
            unit: "utc_day".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
        },
        GeoDateInterval {
            start_utc_day: "2026-01-01".to_string(),
            end_utc_day: "2026-12-31".to_string(),
        },
    )
}
