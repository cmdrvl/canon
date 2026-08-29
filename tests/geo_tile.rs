#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION, CANON_GEO_TILE_WORK_REQUEST_VERSION,
    GeoTileDecisionBatch, GeoTileDecisionMember, GeoTileDecisionProposal, GeoTileErrorCode,
    GeoTileFeatureRef, GeoTilePlacement, GeoTileReconciliationRequest, GeoTileWorkRequest,
    canonical_tile_reconciliation_bytes, canonical_tile_work_unit_bytes,
    materialize_tile_work_unit, reconcile_tile_decisions,
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

fn feature(source_name: &str, feature_id: &str, home_cell: CellIndex) -> GeoTileFeatureRef {
    GeoTileFeatureRef {
        source_name: source_name.to_string(),
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

fn payload(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}

fn member(source_name: &str, feature_id: &str, home_cell: CellIndex) -> GeoTileDecisionMember {
    GeoTileDecisionMember {
        source_name: source_name.to_string(),
        feature_id: feature_id.to_string(),
        home_cell: home_cell.to_string(),
    }
}

fn proposal(
    payload_blake3: String,
    members: Vec<GeoTileDecisionMember>,
) -> GeoTileDecisionProposal {
    GeoTileDecisionProposal {
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
        .map(|member| {
            feature(
                &member.source_name,
                &member.feature_id,
                CellIndex::from_str(&member.home_cell).unwrap(),
            )
        })
        .collect();
    GeoTileDecisionBatch {
        work_unit: materialize_tile_work_unit(&work_request(center, features))
            .expect("decision work unit materializes"),
        proposals,
    }
}

fn reconciliation_request(batches: Vec<GeoTileDecisionBatch>) -> GeoTileReconciliationRequest {
    GeoTileReconciliationRequest {
        version: CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION.to_string(),
        halo_k: 1,
        batches,
        max_batches: 8,
        max_proposals: 32,
        max_members_per_decision: 8,
        max_features_per_batch: 16,
        max_work_cells_per_batch: 7,
    }
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
    assert_eq!(artifact.features[0].source_name, "building");
    assert_eq!(artifact.features[0].placement, GeoTilePlacement::Center);
    assert!(
        artifact
            .features
            .iter()
            .any(|feature| feature.placement == GeoTilePlacement::Halo)
    );
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
