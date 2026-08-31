#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_PLAN_VERSION, CANON_GEO_QUESTION_VERSION, CANON_GEO_REGIONAL_INVENTORY_VERSION,
    CANON_GEO_RESOURCE_BUDGET_VERSION, GeoAbstentionDisposition, GeoAbstentionPolicy, GeoAsOf,
    GeoBoundedGeography, GeoBudgetAction, GeoClaimClass, GeoCompositionProfile,
    GeoControlEntityLevel, GeoCoveragePredicate, GeoDateInterval, GeoDiscoveryGap, GeoEgressClass,
    GeoEvidenceClass, GeoGeometryTransformContract, GeoLicenseClass, GeoLocalAcquisitionState,
    GeoLocalArtifactRef, GeoNativeEntityScope, GeoNumericBound, GeoNumericMeasure,
    GeoPlanExternalRequest, GeoPlanGatePlane, GeoPlanGateStatus, GeoPlanGrainStatus,
    GeoPlanRequest, GeoPlanStage, GeoPlanStatus, GeoRegionalInventory, GeoRegionalSourceInstance,
    GeoReleaseSelectionMode, GeoRequestedGrain, GeoResourceBudget, GeoResourceCounter,
    GeoSourceAvailability, GeoSourceRelease, GeoSubjectBinding, GeoSubjectBindingClass,
    GeoTelemetryDeclaration, GeoTelemetryMetric, GeoTelemetrySemanticEffect, GeoTemporalScope,
    GeoValueOrigin, canonical_geo_plan_bytes, capabilities_semantic_hash, compile_geo_plan,
    default_geo_capabilities, validate_geo_plan,
};

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
