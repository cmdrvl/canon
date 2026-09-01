#![forbid(unsafe_code)]

use canon::geo::{
    canonical_composition_bytes, canonical_owned_tile_reach_bytes,
    canonical_tile_reconciliation_bytes, materialize_owned_tile_reach, materialize_tile_work_unit,
    reconcile_tile_decisions, solve_composition, GeoBuildingCandidate, GeoCompositionArtifact,
    GeoCompositionRequest, GeoCompositionStatus, GeoCompositionUniverse, GeoControlEntityLevel,
    GeoEntityLevel, GeoEntityRef, GeoHardConstraint, GeoHardConstraintKind,
    GeoIdentityParticipation, GeoNativeEntityScope, GeoOwnedTileReachRequest, GeoPlanInventoryRef,
    GeoSourceRelease, GeoTileDecisionBatch, GeoTileDecisionMember, GeoTileDecisionProposal,
    GeoTileDecisionSemantics, GeoTileErrorCode, GeoTileFeatureRef, GeoTilePlacement,
    GeoTileReachState, GeoTileReconciliationRequest, GeoTileSourceBinding, GeoTileTruthReference,
    GeoTileWorkRequest, GeoTileWorkUnitArtifact, CANON_GEO_COMPOSITION_REQUEST_VERSION,
    CANON_GEO_OWNED_TILE_REACH_REQUEST_VERSION, CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION,
    CANON_GEO_TILE_WORK_REQUEST_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS,
};
use h3o::CellIndex;
use std::str::FromStr;

fn adjacent_owner_and_observer() -> (CellIndex, CellIndex) {
    let center = CellIndex::from_str("892a100d26bffff").expect("valid r9 fixture cell");
    let mut neighbors = center
        .grid_disk_safe(1)
        .filter(|cell| *cell != center)
        .collect::<Vec<_>>();
    neighbors.sort();
    let neighbor = neighbors
        .into_iter()
        .next()
        .expect("non-pentagon fixture has a neighbor");
    (
        std::cmp::min(center, neighbor),
        std::cmp::max(center, neighbor),
    )
}

fn source(source_instance_id: &str) -> GeoTileSourceBinding {
    let entity_level = if source_instance_id.contains("parcel") {
        GeoControlEntityLevel::Parcel
    } else {
        GeoControlEntityLevel::Building
    };
    GeoTileSourceBinding {
        source_instance_id: source_instance_id.to_string(),
        release: GeoSourceRelease {
            release_id: format!("{source_instance_id}.release"),
            release_digest: format!(
                "blake3:{}",
                blake3::hash(source_instance_id.as_bytes()).to_hex()
            ),
        },
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level,
            identity_participation: GeoIdentityParticipation::StableAlias,
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: "inventory.fixture.h7".to_string(),
            semantic_hash: format!("blake3:{}", blake3::hash(b"h7-semantic").to_hex()),
            planning_hash: format!("blake3:{}", blake3::hash(b"h7-planning").to_hex()),
        },
    }
}

fn member(
    source_instance_id: &str,
    feature_id: &str,
    home_cell: CellIndex,
) -> GeoTileDecisionMember {
    let source = source(source_instance_id);
    GeoTileDecisionMember {
        candidate_entity_level: source
            .native_entity_level()
            .expect("H7 fixture source is native"),
        source,
        feature_id: feature_id.to_string(),
        home_cell: home_cell.to_string(),
    }
}

fn work_request(center: CellIndex, members: &[GeoTileDecisionMember]) -> GeoTileWorkRequest {
    GeoTileWorkRequest {
        version: CANON_GEO_TILE_WORK_REQUEST_VERSION.to_string(),
        center_cell: center.to_string(),
        halo_k: 1,
        features: members
            .iter()
            .map(|member| GeoTileFeatureRef {
                source: member.source.clone(),
                feature_id: member.feature_id.clone(),
                home_cell: member.home_cell.clone(),
            })
            .collect(),
        max_features: 8,
        max_work_cells: 7,
    }
}

fn proposal(
    payload_blake3: String,
    members: Vec<GeoTileDecisionMember>,
) -> GeoTileDecisionProposal {
    GeoTileDecisionProposal {
        semantics: GeoTileDecisionSemantics::Composition,
        work_unit_blake3: String::new(),
        payload_blake3,
        members,
    }
}

fn decision_batch(
    center: CellIndex,
    available_members: &[GeoTileDecisionMember],
    proposals: Vec<GeoTileDecisionProposal>,
) -> GeoTileDecisionBatch {
    let work_unit = materialize_tile_work_unit(&work_request(center, available_members))
        .expect("bounded center-plus-halo work unit materializes");
    let proposals = proposals
        .into_iter()
        .map(|mut proposal| {
            proposal.work_unit_blake3 = work_unit.work_unit_blake3.clone();
            proposal
        })
        .collect();
    GeoTileDecisionBatch {
        work_unit,
        proposals,
    }
}

fn reconciliation_request(batches: Vec<GeoTileDecisionBatch>) -> GeoTileReconciliationRequest {
    GeoTileReconciliationRequest {
        version: CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION.to_string(),
        halo_k: 1,
        inventory_lineage: None,
        batches,
        max_batches: 4,
        max_proposals: 8,
        max_members_per_decision: 8,
        max_features_per_batch: 8,
        max_work_cells_per_batch: 7,
    }
}

fn owned_reach_request(
    work_units: Vec<GeoTileWorkUnitArtifact>,
    truth_members: Vec<GeoTileDecisionMember>,
) -> GeoOwnedTileReachRequest {
    GeoOwnedTileReachRequest {
        version: CANON_GEO_OWNED_TILE_REACH_REQUEST_VERSION.to_string(),
        halo_k: 1,
        work_units,
        truth_reference: Some(GeoTileTruthReference {
            reference_id: "reference.fixture.h7.accepted_truth".to_string(),
            reference_kind: "synthetic_h7_bounded_reference".to_string(),
            members: truth_members,
        }),
        claimed_truth_reach: GeoTileReachState::PassedAgainstReference,
        max_work_units: 4,
        max_reference_members: 8,
        max_features_per_work_unit: 8,
        max_work_cells_per_work_unit: 7,
    }
}

fn section_local_composition_request(parcel_id: &str, building_id: &str) -> GeoCompositionRequest {
    GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: GeoCompositionUniverse {
            parcels: vec![parcel_id.to_string()],
            buildings: vec![GeoBuildingCandidate {
                id: building_id.to_string(),
                parcel_ids: vec![parcel_id.to_string()],
            }],
        },
        hard_constraints: vec![GeoHardConstraint {
            id: "h7_require_building".to_string(),
            constraint: GeoHardConstraintKind::Require {
                member: GeoEntityRef::new(GeoEntityLevel::Building, building_id),
            },
        }],
        soft_preferences: Vec::new(),
        max_assignments: 4,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn exact_section_payload(parcel_id: &str, building_id: &str) -> (String, GeoCompositionArtifact) {
    let artifact = solve_composition(&section_local_composition_request(parcel_id, building_id))
        .expect("synthetic section-local composition solves exactly");
    assert_eq!(artifact.status, GeoCompositionStatus::Resolved);
    assert_eq!(artifact.summary.candidate_assignments, 4);
    assert!(artifact.summary.structurally_feasible_assignments_complete);
    assert!(artifact.summary.residual_model_count_complete);
    assert_eq!(artifact.summary.residual_model_count, 1);
    assert!(artifact.backbone_complete);
    assert!(artifact
        .factorization
        .iter()
        .all(|component| component.exact));
    let bytes = canonical_composition_bytes(&artifact).expect("composition artifact serializes");
    (
        format!("blake3:{}", blake3::hash(&bytes).to_hex()),
        artifact,
    )
}

fn synthetic_conflict_payload(label: &str) -> String {
    format!(
        "blake3:{}",
        blake3::hash(format!("canon_geo_h7_conflict_fixture.v0:{label}").as_bytes()).to_hex()
    )
}

fn sorted_members(members: &[GeoTileDecisionMember]) -> Vec<GeoTileDecisionMember> {
    let mut sorted = members.to_vec();
    sorted.sort();
    sorted
}

#[test]
fn adjacent_h7_work_units_reconcile_one_exact_composition_with_byte_confluence() {
    let (owner, observer) = adjacent_owner_and_observer();
    let members = vec![
        member("mappluto_parcel", "h7_parcel_100", owner),
        member("derived_building", "h7_building_200", observer),
    ];
    let (payload_blake3, composition) = exact_section_payload("h7_parcel_100", "h7_building_200");
    assert_eq!(composition.hard_forced.parcels, ["h7_parcel_100"]);
    assert_eq!(composition.hard_forced.buildings, ["h7_building_200"]);

    let owner_batch = decision_batch(
        owner,
        &members,
        vec![proposal(payload_blake3.clone(), members.clone())],
    );
    let observer_batch = decision_batch(
        observer,
        &members,
        vec![proposal(payload_blake3.clone(), sorted_members(&members))],
    );
    assert_eq!(owner_batch.work_unit.center_feature_count, 1);
    assert_eq!(owner_batch.work_unit.halo_feature_count, 1);
    assert!(owner_batch.work_unit.features.iter().any(|feature| {
        feature.feature_id == "h7_parcel_100" && feature.placement == GeoTilePlacement::Center
    }));
    assert!(owner_batch.work_unit.features.iter().any(|feature| {
        feature.feature_id == "h7_building_200" && feature.placement == GeoTilePlacement::Halo
    }));

    let reach_request = owned_reach_request(
        vec![
            observer_batch.work_unit.clone(),
            owner_batch.work_unit.clone(),
        ],
        sorted_members(&members),
    );
    let mut permuted_reach = reach_request.clone();
    permuted_reach.work_units.reverse();
    permuted_reach
        .truth_reference
        .as_mut()
        .expect("truth reference exists")
        .members
        .reverse();
    let reach = materialize_owned_tile_reach(&reach_request)
        .expect("owned adjacent sections retain the complete synthetic truth");
    let repeated_reach = materialize_owned_tile_reach(&permuted_reach)
        .expect("reach remains canonical under work-unit and reference order");
    assert_eq!(reach, repeated_reach);
    assert_eq!(
        reach.structural_reach,
        GeoTileReachState::StructurallyCompleteRelativeToInputs
    );
    assert_eq!(reach.truth_reach, GeoTileReachState::PassedAgainstReference);
    assert_eq!(reach.reference_member_count, 2);
    assert_eq!(reach.reached_reference_member_count, 2);
    assert_eq!(
        canonical_owned_tile_reach_bytes(&reach).expect("reach artifact serializes"),
        canonical_owned_tile_reach_bytes(&repeated_reach).expect("repeated reach serializes")
    );

    let original = reconciliation_request(vec![owner_batch.clone(), observer_batch.clone()]);
    let mut permuted = reconciliation_request(vec![observer_batch, owner_batch]);
    for batch in &mut permuted.batches {
        for proposal in &mut batch.proposals {
            proposal.members.reverse();
        }
    }

    let artifact = reconcile_tile_decisions(&original).expect("adjacent H7 work units reconcile");
    let repeated =
        reconcile_tile_decisions(&permuted).expect("completion-order permutation reconciles");
    assert_eq!(artifact, repeated);
    assert_eq!(artifact.batches, 2);
    assert_eq!(artifact.input_proposals, 2);
    assert_eq!(artifact.owned_decisions, 1);
    assert_eq!(artifact.discarded_halo_proposals, 1);
    assert_eq!(artifact.batch_receipts.len(), 2);
    assert_eq!(artifact.decisions.len(), 1);
    let decision = &artifact.decisions[0];
    assert_eq!(decision.owner_cell, owner.to_string());
    assert_eq!(decision.payload_blake3, payload_blake3);
    assert_eq!(decision.members, sorted_members(&members));
    assert_eq!(decision.proposal_copies, 2);
    assert_eq!(
        canonical_tile_reconciliation_bytes(&artifact).expect("artifact serializes"),
        canonical_tile_reconciliation_bytes(&repeated).expect("repeat serializes")
    );
}

#[test]
fn h7_reconciliation_refuses_conflicts_missing_owners_and_halo_only_decisions() {
    let (owner, observer) = adjacent_owner_and_observer();
    let members = vec![
        member("mappluto_parcel", "h7_parcel_100", owner),
        member("derived_building", "h7_building_200", observer),
    ];

    let nonconfluent = reconciliation_request(vec![
        decision_batch(
            owner,
            &members,
            vec![proposal(
                synthetic_conflict_payload("owner"),
                members.clone(),
            )],
        ),
        decision_batch(
            observer,
            &members,
            vec![proposal(
                synthetic_conflict_payload("observer"),
                members.clone(),
            )],
        ),
    ]);
    let error = reconcile_tile_decisions(&nonconfluent)
        .expect_err("same member set with different exact payloads must refuse");
    assert_eq!(error.code, GeoTileErrorCode::NonConfluentDecision);

    let missing_owner = reconciliation_request(vec![decision_batch(
        observer,
        &members,
        vec![proposal(
            synthetic_conflict_payload("missing_owner"),
            members.clone(),
        )],
    )]);
    let error = reconcile_tile_decisions(&missing_owner)
        .expect_err("owner work unit must be present for a reconciled decision");
    assert_eq!(error.code, GeoTileErrorCode::MissingOwnerWorkUnit);

    let halo_only = reconciliation_request(vec![
        decision_batch(owner, &members, vec![]),
        decision_batch(
            observer,
            &members,
            vec![proposal(
                synthetic_conflict_payload("halo_only"),
                members.clone(),
            )],
        ),
    ]);
    let error = reconcile_tile_decisions(&halo_only)
        .expect_err("halo-only observation cannot mint the owned decision");
    assert_eq!(error.code, GeoTileErrorCode::OrphanedDecision);

    let owner_cell = owner.to_string();
    assert!(halo_only
        .batches
        .iter()
        .any(|batch| batch.work_unit.center_cell == owner_cell));
}
