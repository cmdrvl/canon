#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_HOME_CELL_ROWS_VERSION, CANON_GEO_REGIONAL_INVENTORY_VERSION,
    CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION, CANON_GEO_TILE_WORK_REQUEST_VERSION,
    GeoBoundedGeography, GeoControlEntityLevel, GeoControlRelation, GeoCoveragePredicate,
    GeoEgressClass, GeoEvidenceClass, GeoHomeCellParity, GeoHomeCellRow, GeoHomeCellRowsRequest,
    GeoIdentityParticipation, GeoLicenseClass, GeoLocalAcquisitionState, GeoNativeEntityScope,
    GeoPlanInventoryRef, GeoRegionalInventory, GeoRegionalSourceInstance, GeoSourceAvailability,
    GeoSourceRelease, GeoTemporalScope, GeoTileDecisionBatch, GeoTileDecisionMember,
    GeoTileDecisionProposal, GeoTileDecisionSemantics, GeoTileErrorCode, GeoTileFeatureRef,
    GeoTileInventoryLineage, GeoTilePlacement, GeoTileReconciliationRequest, GeoTileSourceBinding,
    GeoTileWorkRequest, canonical_home_cell_assignment_bytes, canonical_tile_reconciliation_bytes,
    canonical_tile_work_unit_bytes, materialize_home_cells, materialize_tile_work_unit,
    reconcile_tile_decisions, regional_inventory_planning_hash, regional_inventory_semantic_hash,
};
use h3o::{CellIndex, Resolution};
use std::{collections::BTreeSet, str::FromStr};

fn center_and_neighbor() -> (CellIndex, CellIndex) {
    let center = CellIndex::from_str("892a100d26bffff").expect("valid fixture cell");
    let neighbor = center
        .grid_disk_safe(1)
        .find(|cell| *cell != center)
        .expect("non-pentagon fixture has a neighbor");
    (center, neighbor)
}

fn outside_k1(center: CellIndex) -> CellIndex {
    let k1 = center.grid_disk_safe(1).collect::<BTreeSet<_>>();
    center
        .grid_disk_safe(2)
        .find(|cell| !k1.contains(cell))
        .expect("k2 has a cell outside k1")
}

fn release_digest(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}

fn fixture_inventory_ref() -> GeoPlanInventoryRef {
    GeoPlanInventoryRef {
        inventory_id: "inventory.fixture.tile".to_string(),
        semantic_hash: release_digest("fixture-inventory-semantic"),
        planning_hash: release_digest("fixture-inventory-planning"),
    }
}

fn native_source(
    source_instance_id: &str,
    entity_level: GeoControlEntityLevel,
    identity_participation: GeoIdentityParticipation,
) -> GeoTileSourceBinding {
    GeoTileSourceBinding {
        source_instance_id: source_instance_id.to_string(),
        release: GeoSourceRelease {
            release_id: format!("{source_instance_id}.release"),
            release_digest: release_digest(source_instance_id),
        },
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level,
            identity_participation,
        },
        inventory_ref: fixture_inventory_ref(),
    }
}

fn observation_source(source_instance_id: &str) -> GeoTileSourceBinding {
    GeoTileSourceBinding {
        source_instance_id: source_instance_id.to_string(),
        release: GeoSourceRelease {
            release_id: format!("{source_instance_id}.release"),
            release_digest: release_digest(source_instance_id),
        },
        native_scope: GeoNativeEntityScope::ObservationOnly,
        inventory_ref: fixture_inventory_ref(),
    }
}

fn fixture_source(source_instance_id: &str) -> GeoTileSourceBinding {
    let level = if source_instance_id.contains("parcel") {
        GeoControlEntityLevel::Parcel
    } else {
        GeoControlEntityLevel::Building
    };
    native_source(
        source_instance_id,
        level,
        GeoIdentityParticipation::StableAlias,
    )
}

fn feature(source_instance_id: &str, feature_id: &str, home_cell: CellIndex) -> GeoTileFeatureRef {
    feature_from_source(fixture_source(source_instance_id), feature_id, home_cell)
}

fn feature_from_source(
    source: GeoTileSourceBinding,
    feature_id: &str,
    home_cell: CellIndex,
) -> GeoTileFeatureRef {
    GeoTileFeatureRef {
        source,
        feature_id: feature_id.to_string(),
        home_cell: home_cell.to_string(),
    }
}

fn work_request(center: CellIndex, features: Vec<GeoTileFeatureRef>) -> GeoTileWorkRequest {
    GeoTileWorkRequest {
        version: CANON_GEO_TILE_WORK_REQUEST_VERSION.to_string(),
        center_cell: center.to_string(),
        halo_k: 1,
        features,
        max_features: 16,
        max_work_cells: 7,
    }
}

fn home_cell_row(feature_id: &str, claimed_home_cell: Option<String>) -> GeoHomeCellRow {
    GeoHomeCellRow {
        source: native_source(
            "mappluto",
            GeoControlEntityLevel::Parcel,
            GeoIdentityParticipation::StableAlias,
        ),
        feature_id: feature_id.to_string(),
        source_record_id: format!("mn/000000/{feature_id}"),
        geometry_sha256: "5ed87d37d872789086452c35f658f5628ba870ca36072c495bb88519592403ed"
            .to_string(),
        representative_point_method: "centroid_of_derived_wgs84_geometry".to_string(),
        longitude: "-73.977264000".to_string(),
        latitude: "40.753429000".to_string(),
        transform_execution_id: Some("sha256-execution-26v2".to_string()),
        transform_definition_id: Some("sha256-definition-hpgn".to_string()),
        claimed_home_cell,
    }
}

fn home_cell_request(rows: Vec<GeoHomeCellRow>) -> GeoHomeCellRowsRequest {
    GeoHomeCellRowsRequest {
        version: CANON_GEO_HOME_CELL_ROWS_VERSION.to_string(),
        coordinate_crs: "EPSG:4326".to_string(),
        coordinate_decimal_places: 9,
        h3_resolution: 9,
        stability_radius_fixed: 1_000,
        rows,
        max_rows: 16,
    }
}

fn payload(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}

fn member(
    source_instance_id: &str,
    feature_id: &str,
    home_cell: CellIndex,
) -> GeoTileDecisionMember {
    let source = fixture_source(source_instance_id);
    member_from_source(
        source.clone(),
        source
            .native_entity_level()
            .expect("fixture member source is native"),
        feature_id,
        home_cell,
    )
}

fn member_from_source(
    source: GeoTileSourceBinding,
    candidate_entity_level: GeoControlEntityLevel,
    feature_id: &str,
    home_cell: CellIndex,
) -> GeoTileDecisionMember {
    GeoTileDecisionMember {
        candidate_entity_level,
        source,
        feature_id: feature_id.to_string(),
        home_cell: home_cell.to_string(),
    }
}

fn proposal(
    payload_blake3: String,
    members: Vec<GeoTileDecisionMember>,
) -> GeoTileDecisionProposal {
    proposal_with_semantics(
        GeoTileDecisionSemantics::Composition,
        payload_blake3,
        members,
    )
}

fn proposal_with_semantics(
    semantics: GeoTileDecisionSemantics,
    payload_blake3: String,
    members: Vec<GeoTileDecisionMember>,
) -> GeoTileDecisionProposal {
    GeoTileDecisionProposal {
        semantics,
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
    let features = available_members
        .iter()
        .map(|member| GeoTileFeatureRef {
            source: member.source.clone(),
            feature_id: member.feature_id.clone(),
            home_cell: member.home_cell.clone(),
        })
        .collect();
    let work_unit = materialize_tile_work_unit(&work_request(center, features))
        .expect("decision work unit materializes");
    let mut proposals = proposals;
    for proposal in &mut proposals {
        proposal.work_unit_blake3 = work_unit.work_unit_blake3.clone();
    }
    GeoTileDecisionBatch {
        work_unit,
        proposals,
    }
}

fn reconciliation_request(mut batches: Vec<GeoTileDecisionBatch>) -> GeoTileReconciliationRequest {
    for batch in &mut batches {
        for proposal in &mut batch.proposals {
            if proposal.work_unit_blake3.is_empty() {
                proposal.work_unit_blake3 = batch.work_unit.work_unit_blake3.clone();
            }
        }
    }
    GeoTileReconciliationRequest {
        version: CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION.to_string(),
        halo_k: 1,
        inventory_lineage: None,
        batches,
        max_batches: 8,
        max_proposals: 32,
        max_members_per_decision: 8,
        max_features_per_batch: 16,
        max_work_cells_per_batch: 7,
    }
}

fn inventory_for_sources(sources: Vec<GeoTileSourceBinding>) -> GeoRegionalInventory {
    let region = GeoBoundedGeography {
        geography_id: "region.fixture.tile".to_string(),
        geography_kind: "h3_test_fixture".to_string(),
        description: "bounded tile authority fixture".to_string(),
    };
    let mut sources = sources;
    sources.sort();
    sources.dedup_by(|left, right| left.source_instance_id == right.source_instance_id);
    GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.fixture.tile".to_string(),
        region: region.clone(),
        sources: sources
            .into_iter()
            .map(|source| GeoRegionalSourceInstance {
                source_instance_id: source.source_instance_id,
                release: source.release,
                temporal_scope: GeoTemporalScope {
                    valid_time: None,
                    transaction_time: None,
                    release_time: None,
                },
                lineage_ids: vec!["lineage.fixture.tile".to_string()],
                native_scope: source.native_scope,
                evidence_classes: vec![GeoEvidenceClass::AssertedAttribute],
                coverage: GeoCoveragePredicate {
                    coverage_id: "coverage.fixture.tile".to_string(),
                    region: region.clone(),
                    predicate: "all fixture rows".to_string(),
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

fn with_inventory_lineage(
    mut request: GeoTileReconciliationRequest,
) -> GeoTileReconciliationRequest {
    let inventory = inventory_for_sources(
        request
            .batches
            .iter()
            .flat_map(|batch| batch.work_unit.features.iter())
            .map(|feature| feature.source.clone())
            .collect(),
    );
    let inventory_ref = GeoPlanInventoryRef {
        inventory_id: inventory.inventory_id.clone(),
        semantic_hash: regional_inventory_semantic_hash(&inventory).unwrap(),
        planning_hash: regional_inventory_planning_hash(&inventory).unwrap(),
    };
    for batch in &mut request.batches {
        for feature in &mut batch.work_unit.features {
            feature.source.inventory_ref = inventory_ref.clone();
        }
        let work_request = GeoTileWorkRequest {
            version: batch.work_unit.request_version.clone(),
            center_cell: batch.work_unit.center_cell.clone(),
            halo_k: batch.work_unit.halo_k,
            features: batch
                .work_unit
                .features
                .iter()
                .map(|feature| GeoTileFeatureRef {
                    source: feature.source.clone(),
                    feature_id: feature.feature_id.clone(),
                    home_cell: feature.home_cell.clone(),
                })
                .collect(),
            max_features: batch.work_unit.max_features,
            max_work_cells: batch.work_unit.max_work_cells,
        };
        batch.work_unit = materialize_tile_work_unit(&work_request).unwrap();
        for proposal in &mut batch.proposals {
            proposal.work_unit_blake3 = batch.work_unit.work_unit_blake3.clone();
            for member in &mut proposal.members {
                member.source.inventory_ref = inventory_ref.clone();
            }
        }
    }
    request.inventory_lineage = Some(GeoTileInventoryLineage {
        inventory_ref,
        inventory,
    });
    request
}

#[test]
fn tile_work_unit_is_center_plus_controlled_halo_and_byte_deterministic() {
    let (center, neighbor) = center_and_neighbor();
    let original = work_request(
        center,
        vec![
            feature("parcel", "p-halo", neighbor),
            feature("building", "b-center", center),
            feature("parcel", "p-center", center),
        ],
    );
    let mut permuted = original.clone();
    permuted.features.reverse();

    let artifact = materialize_tile_work_unit(&original).expect("work unit materializes");
    let repeated = materialize_tile_work_unit(&permuted).expect("permuted work unit materializes");
    assert_eq!(artifact.h3_resolution, 9);
    assert_eq!(artifact.work_cells.len(), 7);
    assert!(artifact.work_cells.contains(&center.to_string()));
    assert_eq!(artifact.center_feature_count, 2);
    assert_eq!(artifact.halo_feature_count, 1);
    assert_eq!(artifact, repeated);
    assert_eq!(
        canonical_tile_work_unit_bytes(&artifact).unwrap(),
        canonical_tile_work_unit_bytes(&repeated).unwrap()
    );
    assert_eq!(artifact.features[0].source.source_instance_id, "building");
    assert_eq!(artifact.features[0].placement, GeoTilePlacement::Center);
    assert!(
        artifact
            .features
            .iter()
            .any(|feature| feature.placement == GeoTilePlacement::Halo)
    );
}

#[test]
fn v1_source_binding_is_required_and_source_release_scopes_duplicate_identity() {
    let missing_binding = serde_json::json!({
        "version": CANON_GEO_TILE_WORK_REQUEST_VERSION,
        "center_cell": "892a100d26bffff",
        "halo_k": 1,
        "features": [{
            "feature_id": "same-id",
            "home_cell": "892a100d26bffff"
        }],
        "max_features": 8,
        "max_work_cells": 7
    });
    serde_json::from_value::<GeoTileWorkRequest>(missing_binding)
        .expect_err("a feature without its source/release/native-scope binding must refuse");

    let center = CellIndex::from_str("892a100d26bffff").unwrap();
    let mut missing_inventory_ref = serde_json::to_value(work_request(
        center,
        vec![feature("parcel", "missing-inventory", center)],
    ))
    .unwrap();
    missing_inventory_ref["features"][0]["source"]
        .as_object_mut()
        .unwrap()
        .remove("inventory_ref");
    serde_json::from_value::<GeoTileWorkRequest>(missing_inventory_ref)
        .expect_err("v1 source bindings require their plan-shaped inventory reference");

    let (center, _) = center_and_neighbor();
    let request = work_request(
        center,
        vec![
            feature_from_source(
                native_source(
                    "overture-building",
                    GeoControlEntityLevel::Building,
                    GeoIdentityParticipation::StableAlias,
                ),
                "same-id",
                center,
            ),
            feature_from_source(
                native_source(
                    "fema-building",
                    GeoControlEntityLevel::Building,
                    GeoIdentityParticipation::StableAlias,
                ),
                "same-id",
                center,
            ),
        ],
    );
    let artifact = materialize_tile_work_unit(&request)
        .expect("equal provider-local ids from distinct source releases remain distinct");
    assert_eq!(artifact.features.len(), 2);
    assert_ne!(artifact.features[0].source, artifact.features[1].source);
}

#[test]
fn mixed_tile_preserves_aliasless_evidence_without_cross_level_candidate_promotion() {
    let (center, _) = center_and_neighbor();
    let stable_building = native_source(
        "overture-building",
        GeoControlEntityLevel::Building,
        GeoIdentityParticipation::StableAlias,
    );
    let aliasless_building = native_source(
        "microsoft-building",
        GeoControlEntityLevel::Building,
        GeoIdentityParticipation::EvidenceOnly,
    );
    let address = native_source(
        "overture-address",
        GeoControlEntityLevel::Address,
        GeoIdentityParticipation::EvidenceOnly,
    );
    let poi = native_source(
        "overture-place",
        GeoControlEntityLevel::Poi,
        GeoIdentityParticipation::StableAlias,
    );
    let observation = observation_source("geocode-observation");
    let stable_parcel = native_source(
        "mappluto-parcel",
        GeoControlEntityLevel::Parcel,
        GeoIdentityParticipation::StableAlias,
    );
    let sources = [
        (stable_building.clone(), "building-stable"),
        (aliasless_building.clone(), "building-aliasless"),
        (address.clone(), "address-evidence"),
        (poi.clone(), "poi-stable"),
        (observation.clone(), "geocode-point"),
        (stable_parcel.clone(), "parcel-stable"),
    ];
    let work_unit = materialize_tile_work_unit(&work_request(
        center,
        sources
            .iter()
            .map(|(source, feature_id)| feature_from_source(source.clone(), feature_id, center))
            .collect(),
    ))
    .expect("one bounded section may retain every declared evidence level");

    let valid_members = vec![
        member_from_source(
            stable_building,
            GeoControlEntityLevel::Building,
            "building-stable",
            center,
        ),
        member_from_source(
            aliasless_building,
            GeoControlEntityLevel::Building,
            "building-aliasless",
            center,
        ),
    ];
    let valid = with_inventory_lineage(reconciliation_request(vec![GeoTileDecisionBatch {
        work_unit: work_unit.clone(),
        proposals: vec![proposal_with_semantics(
            GeoTileDecisionSemantics::StableIdentity {
                entity_level: GeoControlEntityLevel::Building,
            },
            payload("building candidate"),
            valid_members,
        )],
    }]));
    let artifact = reconcile_tile_decisions(&valid)
        .expect("same-level evidence-only building observations may support the candidate");
    let expected_inventory_ref = valid
        .inventory_lineage
        .as_ref()
        .expect("stable identity carries validated lineage")
        .inventory_ref
        .clone();
    assert_eq!(artifact.inventory_ref, Some(expected_inventory_ref.clone()));
    assert_eq!(
        artifact.decisions[0].inventory_ref,
        Some(expected_inventory_ref)
    );
    let aliasless = artifact.decisions[0]
        .members
        .iter()
        .find(|member| member.feature_id == "building-aliasless")
        .expect("alias-less evidence member is retained explicitly");
    assert!(!aliasless.may_contribute_stable_alias());
    assert_eq!(
        aliasless.candidate_entity_level,
        GeoControlEntityLevel::Building
    );

    let evidence_only_composition = reconciliation_request(vec![GeoTileDecisionBatch {
        work_unit: work_unit.clone(),
        proposals: vec![proposal(
            payload("evidence-only composition"),
            vec![member_from_source(
                native_source(
                    "microsoft-building",
                    GeoControlEntityLevel::Building,
                    GeoIdentityParticipation::EvidenceOnly,
                ),
                GeoControlEntityLevel::Building,
                "building-aliasless",
                center,
            )],
        )],
    }]);
    let composition = reconcile_tile_decisions(&evidence_only_composition)
        .expect("evidence-only members may form a composition decision without alias authority");
    assert_eq!(
        composition.decisions[0].semantics,
        GeoTileDecisionSemantics::Composition
    );

    let mixed_level_composition = reconciliation_request(vec![GeoTileDecisionBatch {
        work_unit: work_unit.clone(),
        proposals: vec![proposal(
            payload("mixed-level composition"),
            vec![
                member_from_source(
                    stable_parcel.clone(),
                    GeoControlEntityLevel::Parcel,
                    "parcel-stable",
                    center,
                ),
                member_from_source(
                    native_source(
                        "microsoft-building",
                        GeoControlEntityLevel::Building,
                        GeoIdentityParticipation::EvidenceOnly,
                    ),
                    GeoControlEntityLevel::Building,
                    "building-aliasless",
                    center,
                ),
            ],
        )],
    }]);
    let mixed = reconcile_tile_decisions(&mixed_level_composition)
        .expect("composition may relate native candidates without asserting cross-level identity");
    assert_eq!(mixed.decisions[0].members.len(), 2);
    assert_eq!(
        mixed.decisions[0].semantics,
        GeoTileDecisionSemantics::Composition
    );

    let evidence_only_mint =
        with_inventory_lineage(reconciliation_request(vec![GeoTileDecisionBatch {
            work_unit: work_unit.clone(),
            proposals: vec![proposal_with_semantics(
                GeoTileDecisionSemantics::StableIdentity {
                    entity_level: GeoControlEntityLevel::Building,
                },
                payload("evidence-only identity mint"),
                vec![member_from_source(
                    native_source(
                        "microsoft-building",
                        GeoControlEntityLevel::Building,
                        GeoIdentityParticipation::EvidenceOnly,
                    ),
                    GeoControlEntityLevel::Building,
                    "building-aliasless",
                    center,
                )],
            )],
        }]));
    let error = reconcile_tile_decisions(&evidence_only_mint)
        .expect_err("evidence-only composition cannot be relabeled as stable identity");
    assert_eq!(error.code, GeoTileErrorCode::InvalidCandidateMember);
    assert!(error.message.contains("same-level stable-alias"));

    let parcel_anchor_laundering =
        with_inventory_lineage(reconciliation_request(vec![GeoTileDecisionBatch {
            work_unit: work_unit.clone(),
            proposals: vec![proposal_with_semantics(
                GeoTileDecisionSemantics::StableIdentity {
                    entity_level: GeoControlEntityLevel::Building,
                },
                payload("parcel anchor laundering"),
                vec![
                    member_from_source(
                        stable_parcel,
                        GeoControlEntityLevel::Parcel,
                        "parcel-stable",
                        center,
                    ),
                    member_from_source(
                        native_source(
                            "microsoft-building",
                            GeoControlEntityLevel::Building,
                            GeoIdentityParticipation::EvidenceOnly,
                        ),
                        GeoControlEntityLevel::Building,
                        "building-aliasless",
                        center,
                    ),
                ],
            )],
        }]));
    let error = reconcile_tile_decisions(&parcel_anchor_laundering)
        .expect_err("stable identity cannot launder a cross-level evidence-only member");
    assert_eq!(error.code, GeoTileErrorCode::InvalidCandidateMember);
    assert!(error.message.contains("another entity level"));

    let evidence_relabel = reconciliation_request(vec![GeoTileDecisionBatch {
        work_unit: work_unit.clone(),
        proposals: vec![proposal(
            payload("evidence relabel attack"),
            vec![member_from_source(
                native_source(
                    "microsoft-building",
                    GeoControlEntityLevel::Building,
                    GeoIdentityParticipation::StableAlias,
                ),
                GeoControlEntityLevel::Building,
                "building-aliasless",
                center,
            )],
        )],
    }]);
    let error = reconcile_tile_decisions(&evidence_relabel)
        .expect_err("a proposal cannot relabel EvidenceOnly membership as StableAlias");
    assert_eq!(error.code, GeoTileErrorCode::InvalidDecision);

    let observation_relabel = reconciliation_request(vec![GeoTileDecisionBatch {
        work_unit: work_unit.clone(),
        proposals: vec![proposal(
            payload("observation relabel attack"),
            vec![member_from_source(
                native_source(
                    "geocode-observation",
                    GeoControlEntityLevel::Building,
                    GeoIdentityParticipation::StableAlias,
                ),
                GeoControlEntityLevel::Building,
                "geocode-point",
                center,
            )],
        )],
    }]);
    let error = reconcile_tile_decisions(&observation_relabel)
        .expect_err("a proposal cannot relabel ObservationOnly membership as NativeEntity");
    assert_eq!(error.code, GeoTileErrorCode::InvalidDecision);

    for (source, feature_id) in [
        (address, "address-evidence"),
        (poi, "poi-stable"),
        (observation, "geocode-point"),
    ] {
        let invalid = reconciliation_request(vec![GeoTileDecisionBatch {
            work_unit: work_unit.clone(),
            proposals: vec![proposal(
                payload(feature_id),
                vec![member_from_source(
                    source,
                    GeoControlEntityLevel::Building,
                    feature_id,
                    center,
                )],
            )],
        }]);
        let error = reconcile_tile_decisions(&invalid)
            .expect_err("non-building evidence must not become a building candidate variable");
        assert_eq!(error.code, GeoTileErrorCode::InvalidCandidateMember);
    }
}

#[test]
fn stable_identity_rejects_leaf_laundered_alias_authority_and_missing_lineage() {
    let mut inventory_source = native_source(
        "generic-building-evidence",
        GeoControlEntityLevel::Building,
        GeoIdentityParticipation::EvidenceOnly,
    );
    let inventory = inventory_for_sources(vec![inventory_source.clone()]);
    let inventory_ref = GeoPlanInventoryRef {
        inventory_id: inventory.inventory_id.clone(),
        semantic_hash: regional_inventory_semantic_hash(&inventory).unwrap(),
        planning_hash: regional_inventory_planning_hash(&inventory).unwrap(),
    };
    inventory_source.inventory_ref = inventory_ref.clone();

    let mut laundered_source = inventory_source.clone();
    laundered_source.native_scope = GeoNativeEntityScope::NativeEntity {
        entity_level: GeoControlEntityLevel::Building,
        identity_participation: GeoIdentityParticipation::StableAlias,
    };
    let mut row = home_cell_row("laundered-building", None);
    row.source = laundered_source.clone();
    let assignment = materialize_home_cells(&home_cell_request(vec![row]))
        .expect("self-consistent laundered leaf bytes pass non-authoritative materialization");
    let center = CellIndex::from_str(&assignment.tile_work_features[0].home_cell).unwrap();
    let member = member_from_source(
        laundered_source,
        GeoControlEntityLevel::Building,
        "laundered-building",
        center,
    );
    let batch = decision_batch(
        center,
        std::slice::from_ref(&member),
        vec![proposal_with_semantics(
            GeoTileDecisionSemantics::StableIdentity {
                entity_level: GeoControlEntityLevel::Building,
            },
            payload("laundered stable identity"),
            vec![member.clone()],
        )],
    );

    let missing_lineage = reconciliation_request(vec![batch.clone()]);
    let error = reconcile_tile_decisions(&missing_lineage)
        .expect_err("stable identity cannot rely on caller-declared alias authority alone");
    assert_eq!(error.code, GeoTileErrorCode::InvalidInventoryLineage);

    let mut laundering = reconciliation_request(vec![batch]);
    laundering.inventory_lineage = Some(GeoTileInventoryLineage {
        inventory_ref,
        inventory,
    });
    let error = reconcile_tile_decisions(&laundering)
        .expect_err("leaf-to-proposal StableAlias laundering must disagree with inventory truth");
    assert_eq!(error.code, GeoTileErrorCode::InvalidInventoryLineage);
    assert!(error.message.contains("native scope"));
}

#[test]
fn cross_level_relation_decisions_are_explicit_and_cannot_be_same_as() {
    let (center, _) = center_and_neighbor();
    let building = native_source(
        "overture-building",
        GeoControlEntityLevel::Building,
        GeoIdentityParticipation::StableAlias,
    );
    let parcel = native_source(
        "mappluto-parcel",
        GeoControlEntityLevel::Parcel,
        GeoIdentityParticipation::StableAlias,
    );
    let poi = native_source(
        "overture-place",
        GeoControlEntityLevel::Poi,
        GeoIdentityParticipation::StableAlias,
    );
    let building_member = member_from_source(
        building.clone(),
        GeoControlEntityLevel::Building,
        "building-1",
        center,
    );
    let parcel_member =
        member_from_source(parcel, GeoControlEntityLevel::Parcel, "parcel-1", center);
    let poi_member = member_from_source(poi, GeoControlEntityLevel::Poi, "poi-1", center);
    let available_members = vec![
        building_member.clone(),
        parcel_member.clone(),
        poi_member.clone(),
    ];

    let relation_semantics = GeoTileDecisionSemantics::Relation {
        relation: GeoControlRelation::On,
        from_entity_level: GeoControlEntityLevel::Building,
        to_entity_level: GeoControlEntityLevel::Parcel,
    };
    let request = reconciliation_request(vec![decision_batch(
        center,
        &available_members,
        vec![proposal_with_semantics(
            relation_semantics,
            payload("building-on-parcel"),
            vec![parcel_member.clone(), building_member.clone()],
        )],
    )]);
    let artifact = reconcile_tile_decisions(&request)
        .expect("cross-level relation is retained as relation semantics, not equality");
    assert_eq!(artifact.owned_decisions, 1);
    assert_eq!(artifact.inventory_ref, None);
    assert_eq!(artifact.decisions[0].inventory_ref, None);
    assert_eq!(artifact.decisions[0].semantics, relation_semantics);
    assert_eq!(artifact.decisions[0].members.len(), 2);

    let same_as_relation = reconciliation_request(vec![decision_batch(
        center,
        &available_members,
        vec![proposal_with_semantics(
            GeoTileDecisionSemantics::Relation {
                relation: GeoControlRelation::SameAs,
                from_entity_level: GeoControlEntityLevel::Poi,
                to_entity_level: GeoControlEntityLevel::Building,
            },
            payload("same-as-relation"),
            vec![poi_member.clone(), building_member.clone()],
        )],
    )]);
    let error = reconcile_tile_decisions(&same_as_relation)
        .expect_err("same_as cannot bypass stable-identity inventory authority");
    assert_eq!(error.code, GeoTileErrorCode::InvalidDecision);
    assert!(error.message.contains("same_as"));

    let same_level_relation = reconciliation_request(vec![decision_batch(
        center,
        &[
            building_member.clone(),
            member_from_source(
                native_source(
                    "fema-building",
                    GeoControlEntityLevel::Building,
                    GeoIdentityParticipation::StableAlias,
                ),
                GeoControlEntityLevel::Building,
                "building-2",
                center,
            ),
        ],
        vec![proposal_with_semantics(
            GeoTileDecisionSemantics::Relation {
                relation: GeoControlRelation::On,
                from_entity_level: GeoControlEntityLevel::Building,
                to_entity_level: GeoControlEntityLevel::Building,
            },
            payload("same-level-relation"),
            vec![
                building_member.clone(),
                member_from_source(
                    native_source(
                        "fema-building",
                        GeoControlEntityLevel::Building,
                        GeoIdentityParticipation::StableAlias,
                    ),
                    GeoControlEntityLevel::Building,
                    "building-2",
                    center,
                ),
            ],
        )],
    )]);
    let error = reconcile_tile_decisions(&same_level_relation)
        .expect_err("relation semantics cannot collapse same-level equivalence");
    assert_eq!(error.code, GeoTileErrorCode::InvalidDecision);
    assert!(error.message.contains("distinct entity levels"));

    let wrong_level_relation = reconciliation_request(vec![decision_batch(
        center,
        &available_members,
        vec![proposal_with_semantics(
            relation_semantics,
            payload("wrong-relation-level"),
            vec![building_member, poi_member],
        )],
    )]);
    let error = reconcile_tile_decisions(&wrong_level_relation)
        .expect_err("relation members must match the declared cross-level endpoints");
    assert_eq!(error.code, GeoTileErrorCode::InvalidCandidateMember);
    assert!(
        error
            .message
            .contains("outside the declared cross-level relation")
    );
}

#[test]
fn decision_semantics_bind_confluence_scope_and_decision_identity() {
    let (center, _) = center_and_neighbor();
    let members = vec![member("building", "b-1", center)];
    let digest = payload("same payload bytes");
    let request = with_inventory_lineage(reconciliation_request(vec![decision_batch(
        center,
        &members,
        vec![
            proposal(digest.clone(), members.clone()),
            proposal_with_semantics(
                GeoTileDecisionSemantics::StableIdentity {
                    entity_level: GeoControlEntityLevel::Building,
                },
                digest,
                members.clone(),
            ),
        ],
    )]));
    let artifact = reconcile_tile_decisions(&request)
        .expect("same members under distinct declared semantics are distinct decisions");
    assert_eq!(artifact.owned_decisions, 2);
    assert_ne!(
        artifact.decisions[0].decision_id,
        artifact.decisions[1].decision_id
    );
    let semantics = artifact
        .decisions
        .iter()
        .map(|decision| decision.semantics)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        semantics,
        BTreeSet::from([
            GeoTileDecisionSemantics::Composition,
            GeoTileDecisionSemantics::StableIdentity {
                entity_level: GeoControlEntityLevel::Building,
            },
        ])
    );
}

#[test]
fn home_cells_bind_representative_points_and_report_parity_without_becoming_truth() {
    // h3o's known answer for (40.753429, -73.977264). The historical
    // Snowflake-helper receipt returned 892a100d26bffff for this point; keeping
    // the h3o answer explicit prevents parity tests from normalizing that
    // observed disagreement away.
    let expected = "892a100d62bffff".to_string();
    let neighbor = CellIndex::from_str(&expected)
        .unwrap()
        .grid_disk_safe(1)
        .find(|cell| cell.to_string() != expected)
        .unwrap()
        .to_string();
    let original = home_cell_request(vec![
        home_cell_row("unclaimed", None),
        home_cell_row("match", Some(expected.clone())),
        home_cell_row("mismatch", Some(neighbor)),
    ]);
    let mut permuted = original.clone();
    permuted.rows.reverse();

    let artifact = materialize_home_cells(&original).expect("home cells materialize");
    let repeated = materialize_home_cells(&permuted).expect("permuted rows materialize");
    assert_eq!(artifact, repeated);
    assert_eq!(artifact.features[0].home_cell, expected);
    assert_eq!(artifact.summary.total, 3);
    assert_eq!(artifact.summary.claimed, 2);
    assert_eq!(artifact.summary.matches, 1);
    assert_eq!(artifact.summary.mismatches, 1);
    assert_eq!(artifact.summary.unclaimed, 1);
    assert_eq!(artifact.summary.max_minimum_stability_halo_k, 0);
    assert_eq!(artifact.tile_work_features.len(), 3);
    assert!(
        artifact
            .features
            .iter()
            .all(|feature| feature.home_cell == expected)
    );
    assert!(artifact.features.iter().any(|feature| {
        feature.feature_id == "mismatch" && feature.parity == GeoHomeCellParity::Mismatch
    }));
    assert_eq!(
        canonical_home_cell_assignment_bytes(&artifact).unwrap(),
        canonical_home_cell_assignment_bytes(&repeated).unwrap()
    );
}

#[test]
fn home_cells_refuse_bad_coordinates_digests_transforms_and_claimed_resolution() {
    let mut request = home_cell_request(vec![home_cell_row("bad-coordinate", None)]);
    request.rows[0].latitude = "91.000000000".to_string();
    let error = materialize_home_cells(&request).expect_err("latitude outside WGS84 must refuse");
    assert_eq!(error.code, GeoTileErrorCode::InvalidCoordinate);

    let mut request = home_cell_request(vec![home_cell_row("bad-digest", None)]);
    request.rows[0].geometry_sha256 = "ABC".to_string();
    let error = materialize_home_cells(&request).expect_err("bad digest must refuse");
    assert_eq!(error.code, GeoTileErrorCode::InvalidSourceDigest);

    let mut request = home_cell_request(vec![home_cell_row("half-transform", None)]);
    request.rows[0].transform_definition_id = None;
    let error = materialize_home_cells(&request).expect_err("half transform binding must refuse");
    assert_eq!(error.code, GeoTileErrorCode::InvalidInput);

    let coarse = CellIndex::from_str("892a100d26bffff")
        .unwrap()
        .parent(Resolution::Eight)
        .unwrap()
        .to_string();
    let request = home_cell_request(vec![home_cell_row("wrong-resolution", Some(coarse))]);
    let error = materialize_home_cells(&request).expect_err("claimed r8 under r9 must refuse");
    assert_eq!(error.code, GeoTileErrorCode::ResolutionMismatch);

    let mut first = home_cell_row("mixed-a", None);
    let mut second = home_cell_row("mixed-b", None);
    first.source.release.release_id = "26v1".to_string();
    first.source.release.release_digest = release_digest("26v1");
    second.source.release.release_id = "26v2".to_string();
    second.source.release.release_digest = release_digest("26v2");
    let request = home_cell_request(vec![first, second]);
    let error = materialize_home_cells(&request)
        .expect_err("one source name must not collapse two temporal snapshots");
    assert_eq!(error.code, GeoTileErrorCode::IncompatibleSourceBinding);

    let mut request = home_cell_request(vec![home_cell_row("wide-envelope", None)]);
    request.stability_radius_fixed = 100_001;
    let error = materialize_home_cells(&request)
        .expect_err("coordinate envelope wider than 0.0001 degrees must refuse");
    assert_eq!(error.code, GeoTileErrorCode::InvalidInput);
}

#[test]
fn home_cells_expose_boundary_sensitivity_as_a_minimum_halo_requirement() {
    let center = CellIndex::from_str("892a100d26bffff").unwrap();
    let vertex = center.boundary()[0];
    let mut row = home_cell_row("boundary", None);
    row.longitude = format!("{:.9}", vertex.lng());
    row.latitude = format!("{:.9}", vertex.lat());
    let artifact = materialize_home_cells(&home_cell_request(vec![row]))
        .expect("boundary probe materializes as an explicit sensitivity set");
    assert!(artifact.features[0].stability_cells.len() > 1);
    assert!(artifact.features[0].minimum_stability_halo_k >= 1);
    assert_eq!(artifact.summary.boundary_sensitive, 1);
    assert!(artifact.summary.max_minimum_stability_halo_k >= 1);
}

#[test]
fn tile_work_unit_refuses_reach_resolution_duplicate_and_halo_budget_defects() {
    let (center, _) = center_and_neighbor();
    let outside = outside_k1(center);
    let error = materialize_tile_work_unit(&work_request(
        center,
        vec![feature("parcel", "outside", outside)],
    ))
    .expect_err("outside feature must not disappear silently");
    assert_eq!(error.code, GeoTileErrorCode::FeatureOutsideHalo);

    let coarse = center
        .parent(Resolution::Eight)
        .expect("r9 fixture has an r8 parent");
    let error = materialize_tile_work_unit(&work_request(
        center,
        vec![feature("parcel", "wrong-resolution", coarse)],
    ))
    .expect_err("mixed resolutions must refuse");
    assert_eq!(error.code, GeoTileErrorCode::ResolutionMismatch);

    let duplicate = feature("parcel", "duplicate", center);
    let error =
        materialize_tile_work_unit(&work_request(center, vec![duplicate.clone(), duplicate]))
            .expect_err("duplicate feature grain must refuse");
    assert_eq!(error.code, GeoTileErrorCode::DuplicateFeature);

    let mut over_budget = work_request(center, vec![]);
    over_budget.halo_k = 2;
    over_budget.max_work_cells = 7;
    let error = materialize_tile_work_unit(&over_budget)
        .expect_err("halo upper bound must be checked before enumeration");
    assert_eq!(error.code, GeoTileErrorCode::HaloBudgetExceeded);
    assert_eq!(error.detail["upper_bound"], "19");

    let mut noncanonical = work_request(center, vec![]);
    noncanonical.center_cell = noncanonical.center_cell.to_ascii_uppercase();
    let error = materialize_tile_work_unit(&noncanonical)
        .expect_err("alternate H3 text encodings must not enter canonical artifacts");
    assert_eq!(error.code, GeoTileErrorCode::InvalidH3Cell);

    let mut pathological_budget = work_request(center, vec![]);
    pathological_budget.max_work_cells = 100_001;
    let error = materialize_tile_work_unit(&pathological_budget)
        .expect_err("caller-declared budgets cannot exceed the kernel ceiling");
    assert_eq!(error.code, GeoTileErrorCode::InvalidInput);
    assert_eq!(error.detail["hard_max"], "100000");

    let mut uppercase_release = feature("parcel", "uppercase-release", center);
    uppercase_release.source.release.release_digest = format!("blake3:{}", "A".repeat(64));
    let error = materialize_tile_work_unit(&work_request(center, vec![uppercase_release]))
        .expect_err("uppercase source release digests are not canonical BLAKE3");
    assert_eq!(error.code, GeoTileErrorCode::InvalidSourceDigest);
}

#[test]
fn adjacent_tiles_reconcile_one_owner_without_duplicate_minting() {
    let (first, second) = center_and_neighbor();
    let members = vec![
        member("building", "b-1", second),
        member("parcel", "p-1", first),
    ];
    let digest = payload("same exact local solution");
    let original = reconciliation_request(vec![
        decision_batch(
            first,
            &members,
            vec![proposal(digest.clone(), members.clone())],
        ),
        decision_batch(
            second,
            &members,
            vec![proposal(digest.clone(), members.clone())],
        ),
    ]);
    let mut permuted = original.clone();
    permuted.batches.reverse();
    for batch in &mut permuted.batches {
        batch.proposals[0].members.reverse();
    }

    let artifact = reconcile_tile_decisions(&original).expect("adjacent tiles reconcile");
    let repeated = reconcile_tile_decisions(&permuted).expect("permuted tiles reconcile");
    assert_eq!(artifact, repeated);
    assert_eq!(artifact.input_proposals, 2);
    assert_eq!(artifact.owned_decisions, 1);
    assert_eq!(artifact.discarded_halo_proposals, 1);
    assert_eq!(artifact.batch_receipts.len(), 2);
    assert!(
        artifact
            .batch_receipts
            .iter()
            .all(|receipt| receipt.work_unit_blake3.starts_with("blake3:"))
    );
    assert_eq!(artifact.decisions[0].proposal_copies, 2);
    assert_eq!(
        artifact.decisions[0].owner_cell,
        std::cmp::min(first, second).to_string()
    );
    assert!(
        artifact.decisions[0]
            .decision_id
            .starts_with("geo-decision:")
    );
    assert_eq!(
        canonical_tile_reconciliation_bytes(&artifact).unwrap(),
        canonical_tile_reconciliation_bytes(&repeated).unwrap()
    );
}

#[test]
fn reconciliation_refuses_orphaned_and_nonconfluent_boundary_decisions() {
    let (first, second) = center_and_neighbor();
    let owner = std::cmp::min(first, second);
    let halo = std::cmp::max(first, second);
    let members = vec![
        member("parcel", "p-1", first),
        member("building", "b-1", second),
    ];

    let orphaned = reconciliation_request(vec![
        decision_batch(owner, &members, vec![]),
        decision_batch(
            halo,
            &members,
            vec![proposal(payload("halo-only"), members.clone())],
        ),
    ]);
    let error = reconcile_tile_decisions(&orphaned)
        .expect_err("a proposal observed only from the halo must refuse");
    assert_eq!(error.code, GeoTileErrorCode::OrphanedDecision);

    let nonconfluent = reconciliation_request(vec![
        decision_batch(
            first,
            &members,
            vec![proposal(payload("first"), members.clone())],
        ),
        decision_batch(
            second,
            &members,
            vec![proposal(payload("different"), members.clone())],
        ),
    ]);
    let error = reconcile_tile_decisions(&nonconfluent)
        .expect_err("different payloads for the same members must refuse");
    assert_eq!(error.code, GeoTileErrorCode::NonConfluentDecision);

    let uppercase_payload = reconciliation_request(vec![decision_batch(
        first,
        &members,
        vec![proposal(
            format!("blake3:{}", "A".repeat(64)),
            members.clone(),
        )],
    )]);
    let error = reconcile_tile_decisions(&uppercase_payload)
        .expect_err("uppercase decision payload digests are not canonical BLAKE3");
    assert_eq!(error.code, GeoTileErrorCode::InvalidDecision);
}

#[test]
fn reconciliation_refuses_cross_batch_source_release_drift() {
    let (first, second) = center_and_neighbor();
    let mut first_source = native_source(
        "building-source",
        GeoControlEntityLevel::Building,
        GeoIdentityParticipation::StableAlias,
    );
    first_source.release.release_id = "release-a".to_string();
    first_source.release.release_digest = release_digest("release-a");
    let mut second_source = first_source.clone();
    second_source.release.release_id = "release-b".to_string();
    second_source.release.release_digest = release_digest("release-b");
    let first_member = member_from_source(
        first_source,
        GeoControlEntityLevel::Building,
        "building-1",
        first,
    );
    let second_member = member_from_source(
        second_source,
        GeoControlEntityLevel::Building,
        "building-1",
        second,
    );
    let request = reconciliation_request(vec![
        decision_batch(first, std::slice::from_ref(&first_member), vec![]),
        decision_batch(second, std::slice::from_ref(&second_member), vec![]),
    ]);
    let error = reconcile_tile_decisions(&request)
        .expect_err("one source instance cannot drift across releases between tile batches");
    assert_eq!(error.code, GeoTileErrorCode::IncompatibleSourceBinding);
}

#[test]
fn reconciliation_requires_the_owner_batch_and_bounds_every_proposal() {
    let (first, second) = center_and_neighbor();
    let owner = std::cmp::min(first, second);
    let halo = std::cmp::max(first, second);
    let members = vec![
        member("parcel", "p-1", owner),
        member("building", "b-1", halo),
    ];
    let missing_owner = reconciliation_request(vec![decision_batch(
        halo,
        &members,
        vec![proposal(payload("missing-owner"), members.clone())],
    )]);
    let error = reconcile_tile_decisions(&missing_owner)
        .expect_err("reconciliation cannot prove ownership without the owner batch");
    assert_eq!(error.code, GeoTileErrorCode::MissingOwnerWorkUnit);

    let mut corrupted = decision_batch(first, &members, vec![]);
    corrupted.work_unit.center_feature_count += 1;
    let error = reconcile_tile_decisions(&reconciliation_request(vec![corrupted]))
        .expect_err("reconciliation must validate the exact work-unit artifact");
    assert_eq!(error.code, GeoTileErrorCode::InvalidWorkUnit);

    let mut wrong_work_unit = decision_batch(
        first,
        &members,
        vec![proposal(payload("wrong-work-unit"), members.clone())],
    );
    wrong_work_unit.proposals[0].work_unit_blake3 = payload("different work unit");
    let error = reconcile_tile_decisions(&reconciliation_request(vec![wrong_work_unit]))
        .expect_err("proposal cannot float free of its exact bounded work unit");
    assert_eq!(error.code, GeoTileErrorCode::InvalidDecision);
    assert!(error.message.contains("embedded canonical work unit"));

    let unavailable = member("parcel", "not-in-work-unit", first);
    let bad_reach = reconciliation_request(vec![decision_batch(
        first,
        &[],
        vec![proposal(payload("unavailable"), vec![unavailable])],
    )]);
    let error = reconcile_tile_decisions(&bad_reach)
        .expect_err("a local decision cannot name an unreachable member");
    assert_eq!(error.code, GeoTileErrorCode::InvalidDecision);
}
