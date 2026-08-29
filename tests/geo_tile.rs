#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_HOME_CELL_ROWS_VERSION, CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION,
    CANON_GEO_TILE_WORK_REQUEST_VERSION, GeoHomeCellParity, GeoHomeCellRow, GeoHomeCellRowsRequest,
    GeoTileDecisionBatch, GeoTileDecisionMember, GeoTileDecisionProposal, GeoTileErrorCode,
    GeoTileFeatureRef, GeoTilePlacement, GeoTileReconciliationRequest, GeoTileWorkRequest,
    canonical_home_cell_assignment_bytes, canonical_tile_reconciliation_bytes,
    canonical_tile_work_unit_bytes, materialize_home_cells, materialize_tile_work_unit,
    reconcile_tile_decisions,
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

fn home_cell_row(feature_id: &str, claimed_home_cell: Option<String>) -> GeoHomeCellRow {
    GeoHomeCellRow {
        source_name: "mappluto".to_string(),
        feature_id: feature_id.to_string(),
        source_snapshot: "26v2/2026-08-01/geom-v3".to_string(),
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
    first.source_snapshot = "26v1/2026-05-01/geom-v3".to_string();
    second.source_snapshot = "26v2/2026-08-01/geom-v3".to_string();
    let request = home_cell_request(vec![first, second]);
    let error = materialize_home_cells(&request)
        .expect_err("one source name must not collapse two temporal snapshots");
    assert_eq!(error.code, GeoTileErrorCode::MixedSourceSnapshot);

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
