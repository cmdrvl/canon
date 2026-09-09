#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::geo::{
    CANON_GEO_COLLATERAL_LEDGER_VERSION, CANON_GEO_CROSS_DEAL_VERSION, GeoCandidateReachStatus,
    GeoClaimClass, GeoCollateralLedger, GeoCollateralLedgerProofClass, GeoCollisionKind,
    GeoCompositionStatus, GeoEntityLevel, GeoEntityRef, GeoEvidenceRecordRef, GeoLedgerRow,
    GeoPariPassuDeclaration, GeoSourceReleasePin, GeoTruthPlane, canonical_collateral_ledger_bytes,
    find_collisions, validate_cross_deal_artifact,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

#[test]
fn t09_reports_shared_parcels_and_keeps_pari_passu_rows() {
    let ledger_a = ledger(
        "0000000000-26-000001",
        "deal-a",
        "loan-a",
        &["1004540041", "1004540042"],
    );
    let ledger_b = ledger(
        "0000000000-26-000002",
        "deal-b",
        "loan-b",
        &["1004540041", "1004540042"],
    );
    let declaration = declaration(
        "1004540041",
        &["0000000000-26-000001", "0000000000-26-000002"],
    );
    let adjacency = BTreeMap::from([
        ("1004540041".to_string(), "1004540".to_string()),
        ("1004540042".to_string(), "1004540".to_string()),
    ]);

    let artifact =
        find_collisions(&[ledger_a, ledger_b], &[declaration], &adjacency).expect("T09 collision");
    validate_cross_deal_artifact(&artifact).expect("T09 artifact validates");

    assert_eq!(artifact.version, CANON_GEO_CROSS_DEAL_VERSION);
    assert_eq!(artifact.ledger_blake3s.len(), 2);
    let shared_parcels = artifact
        .collisions
        .iter()
        .filter(|collision| collision.kind == GeoCollisionKind::SharedParcel)
        .collect::<Vec<_>>();
    assert_eq!(
        shared_parcels.len(),
        2,
        "T09 must keep every shared parcel row, including declared pari passu rows: {:?}",
        artifact.collisions
    );
    let declared = shared_parcels
        .iter()
        .find(|collision| collision.entity.id == "1004540041")
        .expect("declared shared parcel row kept");
    assert!(
        declared.pari_passu,
        "declared pari passu collision must be labeled, not suppressed: {:?}",
        declared
    );
    assert!(
        declared.source_record.is_some(),
        "labeled pari passu row must carry declaration source record"
    );
    assert!(
        !declared.explanation.is_empty(),
        "labeled pari passu row must explain the source-pinned declaration"
    );
    assert_eq!(
        declared.accessions,
        vec![
            "0000000000-26-000001".to_string(),
            "0000000000-26-000002".to_string()
        ]
    );
    assert_eq!(
        declared.loan_ids,
        vec!["loan-a".to_string(), "loan-b".to_string()]
    );
    assert_eq!(declared.sides.len(), 2);

    let undeclared = shared_parcels
        .iter()
        .find(|collision| collision.entity.id == "1004540042")
        .expect("undeclared shared parcel row kept");
    assert!(
        !undeclared.pari_passu,
        "undeclared shared parcel must remain visible as undeclared: {:?}",
        undeclared
    );
    assert!(
        undeclared.source_record.is_none(),
        "undeclared shared parcel must not borrow pari passu provenance"
    );

    assert_eq!(
        artifact.adjacency_concentration,
        vec![canon::geo::GeoAdjacencyConcentration {
            block_id: "1004540".to_string(),
            deal_count: 2,
            accessions: vec![
                "0000000000-26-000001".to_string(),
                "0000000000-26-000002".to_string()
            ],
            loan_ids: vec!["loan-a".to_string(), "loan-b".to_string()],
            parcel_count: 2,
        }],
        "adjacency concentration denominator must include both shared parcels"
    );
}

#[test]
fn t09_partial_pari_passu_declaration_does_not_label_collision() {
    let ledger_a = ledger("0000000000-26-000001", "deal-a", "loan-a", &["1004540041"]);
    let ledger_b = ledger("0000000000-26-000002", "deal-b", "loan-b", &["1004540041"]);
    let partial = declaration("1004540041", &["0000000000-26-000001"]);

    let artifact =
        find_collisions(&[ledger_a, ledger_b], &[partial], &BTreeMap::new()).expect("collision");
    let collision = artifact
        .collisions
        .iter()
        .find(|collision| collision.entity.id == "1004540041")
        .expect("shared parcel row is present");
    assert!(
        !collision.pari_passu,
        "partial declarations must not label collisions: declaration covers one accession, collision covers {:?}",
        collision.accessions
    );
    assert!(
        collision.source_record.is_none(),
        "partial declarations must not attach source provenance"
    );
}

#[test]
fn t09_shared_building_without_declaration_is_reported() {
    let ledger_a = ledger_with_buildings(
        "0000000000-26-000001",
        "deal-a",
        "loan-a",
        &["parcel-a"],
        &["shared-building"],
    );
    let ledger_b = ledger_with_buildings(
        "0000000000-26-000002",
        "deal-b",
        "loan-b",
        &["parcel-b"],
        &["shared-building"],
    );

    let artifact =
        find_collisions(&[ledger_a, ledger_b], &[], &BTreeMap::new()).expect("building collision");
    let collision = artifact
        .collisions
        .iter()
        .find(|collision| {
            collision.kind == GeoCollisionKind::SharedBuilding
                && collision.entity.id == "shared-building"
        })
        .expect("shared building row");
    assert!(
        !collision.pari_passu,
        "undeclared shared building must be reported as an undeclared collision"
    );
    assert_eq!(collision.entity.level, GeoEntityLevel::Building);
    assert_eq!(collision.sides.len(), 2);
}

#[test]
fn t09_three_accessions_sharing_one_entity_emit_one_collision_row() {
    let ledgers = vec![
        ledger("0000000000-26-000001", "deal-a", "loan-a", &["1004540041"]),
        ledger("0000000000-26-000002", "deal-b", "loan-b", &["1004540041"]),
        ledger("0000000000-26-000003", "deal-c", "loan-c", &["1004540041"]),
    ];

    let artifact = find_collisions(&ledgers, &[], &BTreeMap::new()).expect("three-way collision");
    assert_eq!(
        artifact.collisions.len(),
        1,
        "one shared entity across three accessions must not become pairwise rows: {:?}",
        artifact.collisions
    );
    let collision = &artifact.collisions[0];
    assert_eq!(
        collision.accessions,
        vec![
            "0000000000-26-000001".to_string(),
            "0000000000-26-000002".to_string(),
            "0000000000-26-000003".to_string()
        ]
    );
    assert_eq!(collision.sides.len(), 3);
}

#[test]
fn geo_ledger_collision_cli_emits_reported_output_from_input_ledgers() {
    let temp = tempfile::tempdir().expect("temp dir");
    let ledger_a = ledger(
        "0000000000-26-000001",
        "deal-a",
        "loan-a",
        &["1004540041", "1004540042"],
    );
    let ledger_b = ledger(
        "0000000000-26-000002",
        "deal-b",
        "loan-b",
        &["1004540041", "1004540042"],
    );
    let ledger_a_path = temp.path().join("ledger-a.json");
    let ledger_b_path = temp.path().join("ledger-b.json");
    fs::write(
        &ledger_a_path,
        canonical_collateral_ledger_bytes(&ledger_a).expect("ledger a canonicalizes"),
    )
    .expect("write ledger a");
    fs::write(
        &ledger_b_path,
        canonical_collateral_ledger_bytes(&ledger_b).expect("ledger b canonicalizes"),
    )
    .expect("write ledger b");
    let declarations_path = temp.path().join("pari-passu.json");
    fs::write(
        &declarations_path,
        serde_json::to_vec(&vec![declaration(
            "1004540041",
            &["0000000000-26-000001", "0000000000-26-000002"],
        )])
        .expect("declarations serialize"),
    )
    .expect("write declarations");
    let adjacency_path = temp.path().join("adjacency.json");
    fs::write(
        &adjacency_path,
        serde_json::to_vec(&BTreeMap::from([
            ("1004540041".to_string(), "1004540".to_string()),
            ("1004540042".to_string(), "1004540".to_string()),
        ]))
        .expect("adjacency serializes"),
    )
    .expect("write adjacency");

    let assert = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "geo",
            "ledger",
            "collision",
            "--ledgers",
            ledger_a_path.to_str().expect("ledger path utf8"),
            ledger_b_path.to_str().expect("ledger path utf8"),
            "--pari-passu",
            declarations_path.to_str().expect("declarations path utf8"),
            "--adjacency",
            adjacency_path.to_str().expect("adjacency path utf8"),
        ])
        .assert()
        .success();
    assert!(assert.get_output().stderr.is_empty());
    let output: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("CLI output parses");
    assert_eq!(output["version"], CANON_GEO_CROSS_DEAL_VERSION);
    let collisions = output["collisions"].as_array().expect("collisions array");
    assert_eq!(
        collisions.len(),
        2,
        "CLI must emit both shared parcel rows, not a suppressed pari passu subset"
    );
    assert!(collisions.iter().any(|collision| {
        collision["entity"]["id"] == "1004540041" && collision["pari_passu"] == true
    }));
    assert!(collisions.iter().any(|collision| {
        collision["entity"]["id"] == "1004540042" && collision["pari_passu"] == false
    }));
}

fn ledger(
    accession: &str,
    deal_id: &str,
    loan_id: &str,
    parcel_ids: &[&str],
) -> GeoCollateralLedger {
    ledger_with_buildings(accession, deal_id, loan_id, parcel_ids, &[])
}

fn ledger_with_buildings(
    accession: &str,
    deal_id: &str,
    loan_id: &str,
    parcel_ids: &[&str],
    building_ids: &[&str],
) -> GeoCollateralLedger {
    let digest = prefixed_blake3(format!("{accession}:{loan_id}").as_bytes());
    let row = GeoLedgerRow {
        version: CANON_GEO_COLLATERAL_LEDGER_VERSION.to_string(),
        accession: accession.to_string(),
        deal_id: deal_id.to_string(),
        loan_id: loan_id.to_string(),
        reach: GeoCandidateReachStatus::Full,
        reach_none_reason: None,
        parcel_set: Some(sorted_strings(parcel_ids)),
        building_set: Some(sorted_strings(building_ids)),
        deed_ids: Vec::new(),
        truth_plane: Some(GeoTruthPlane::GateV2Historical),
        claim_class: GeoClaimClass::CollateralComposition,
        residual_model_count: 1,
        count_exact: true,
        backbone_complete: true,
        last_observed_present: None,
        source_release_pins: vec![GeoSourceReleasePin {
            source_dataset: "fixture.geo_collision.ledger".to_string(),
            source_release: "fixture-release".to_string(),
            blake3: digest.clone(),
        }],
        composition_blake3: digest.clone(),
        evidence_blake3: digest,
        ambiguous_parcel_set: Vec::new(),
        ambiguous_building_set: Vec::new(),
        property_refs: Vec::new(),
        composition_status: GeoCompositionStatus::Resolved,
    };
    canon::geo::build_collateral_ledger(vec![row], GeoCollateralLedgerProofClass::Fixture)
        .expect("fixture ledger builds")
}

fn declaration(parcel_id: &str, accessions: &[&str]) -> GeoPariPassuDeclaration {
    GeoPariPassuDeclaration {
        entity: GeoEntityRef::new(GeoEntityLevel::Parcel, parcel_id),
        accessions: sorted_strings(accessions),
        source_record: GeoEvidenceRecordRef {
            source_record_id: format!("pari-passu.{parcel_id}"),
            source_vintage: "fixture-vintage".to_string(),
            record_blake3: blake3::hash(format!("pari-passu:{parcel_id}").as_bytes())
                .to_hex()
                .to_string(),
        },
    }
}

fn sorted_strings(values: &[&str]) -> Vec<String> {
    values
        .iter()
        .map(|value| (*value).to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn prefixed_blake3(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}
