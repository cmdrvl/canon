#![forbid(unsafe_code)]

use canon::geo::{compare_property_sets, GeoEntityLevel};
use canon::geo::{
    diff_tile_identifier_vintages, normalize_nyc_bbl, registry_entries_for_clusters,
    registry_proposal_from_ledger_json, GeoIdentifierCluster, GeoIdentifierErrorCode,
    GeoIdentifierTombstone, GeoPropertyDocumentAssertion, GeoPropertySetRelation,
    GeoTileIdentifierVintage, CANON_GEO_REGISTRY_PROPOSAL_VERSION, GEO_BBL_NORMALIZATION_RULE_ID,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn digest(hex: char) -> String {
    format!("blake3:{}", hex.to_string().repeat(64))
}

fn cluster(
    cluster_id: &str,
    entity_level: GeoEntityLevel,
    geometry_blake3: String,
    aliases: &[&str],
) -> GeoIdentifierCluster {
    GeoIdentifierCluster {
        cluster_id: cluster_id.to_string(),
        entity_level,
        geometry_blake3,
        aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
    }
}

fn tombstone(cluster_id: &str, geometry_blake3: String) -> GeoIdentifierTombstone {
    GeoIdentifierTombstone {
        cluster_id: cluster_id.to_string(),
        geometry_blake3,
        reason: "retired_by_tile_refresh".to_string(),
    }
}

fn vintage(
    vintage_id: &str,
    clusters: Vec<GeoIdentifierCluster>,
    tombstones: Vec<GeoIdentifierTombstone>,
) -> GeoTileIdentifierVintage {
    GeoTileIdentifierVintage {
        tile_id: "tile:r8:stable".to_string(),
        vintage_id: vintage_id.to_string(),
        clusters,
        tombstones,
    }
}

fn property(
    property_id: &str,
    accession: &str,
    loan_id: &str,
    parcels: &[&str],
    buildings: &[&str],
) -> GeoPropertyDocumentAssertion {
    GeoPropertyDocumentAssertion {
        property_id: property_id.to_string(),
        document_alias: format!("cmbs:annexa:{accession}:{loan_id}"),
        accession: accession.to_string(),
        loan_id: loan_id.to_string(),
        parcel_ids: parcels.iter().map(|id| (*id).to_string()).collect(),
        building_ids: buildings.iter().map(|id| (*id).to_string()).collect(),
    }
}

fn assert_no_probabilistic_fields(value: &Value) {
    match value {
        Value::Object(map) => {
            for key in map.keys() {
                assert!(
                    !matches!(key.as_str(), "score" | "confidence" | "probability"),
                    "property set algebra must not serialize probabilistic field {key}"
                );
            }
            for nested in map.values() {
                assert_no_probabilistic_fields(nested);
            }
        }
        Value::Array(values) => {
            for nested in values {
                assert_no_probabilistic_fields(nested);
            }
        }
        _ => {}
    }
}

#[test]
fn t54_alias_role_namespace_preserved_for_registry_entries() {
    let clusters = vec![
        cluster(
            "cmdrvl:parcel:01J7X0000000000000000A",
            GeoEntityLevel::Parcel,
            digest('a'),
            &["parcel:nyc:bbl:1004540041"],
        ),
        cluster(
            "cmdrvl:parcel:01J7X0000000000000000B",
            GeoEntityLevel::Parcel,
            digest('b'),
            &["attom:parcel:1004540041"],
        ),
    ];

    let entries = registry_entries_for_clusters(&clusters).expect("aliases become entries");
    let aliases = entries
        .iter()
        .map(|entry| entry.alias.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(entries.len(), 2);
    assert!(aliases.contains("parcel:nyc:bbl:1004540041"));
    assert!(aliases.contains("attom:parcel:1004540041"));

    let stripped_suffixes = entries
        .iter()
        .map(|entry| entry.alias.rsplit(':').next().expect("alias suffix"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        stripped_suffixes.len(),
        1,
        "fixture must catch an implementation that dedupes by terminal value only"
    );
    println!("T54 aliases in: {aliases:?}");
    println!(
        "T54 canonical ids out: {:?}",
        entries
            .iter()
            .map(|entry| (entry.alias.as_str(), entry.canonical_id.as_str()))
            .collect::<BTreeMap<_, _>>()
    );
}

#[test]
fn t55_bbl_normalization_is_versioned_and_exact_match_only() {
    let normalization = normalize_nyc_bbl("1014477501.0").expect("zero suffix normalizes");
    assert_eq!(normalization.rule_id, GEO_BBL_NORMALIZATION_RULE_ID);
    assert_eq!(normalization.normalized, "1014477501");

    let canonical_id = "cmdrvl:parcel:01J7X0000000000000000C";
    let normalized_alias = format!("parcel:nyc:bbl:{}", normalization.normalized);
    let raw_alias = format!("parcel:nyc:bbl:{}", normalization.input);
    let entries = registry_entries_for_clusters(&[cluster(
        canonical_id,
        GeoEntityLevel::Parcel,
        digest('c'),
        &[normalized_alias.as_str()],
    )])
    .expect("normalized alias proposal entries");
    let exact_lookup = entries
        .iter()
        .map(|entry| (entry.alias.as_str(), entry.canonical_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        exact_lookup.get(normalized_alias.as_str()).copied(),
        Some(canonical_id)
    );
    assert!(
        !exact_lookup.contains_key(raw_alias.as_str()),
        "raw warehouse projection must not resolve without the declared normalization step"
    );

    let error = normalize_nyc_bbl("1014477501.5").expect_err("nonzero decimal suffix refuses");
    assert_eq!(error.code, GeoIdentifierErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("rule_id").map(String::as_str),
        Some(GEO_BBL_NORMALIZATION_RULE_ID)
    );
    println!(
        "T55 normalization rule={} input={} normalized={} canonical_id={canonical_id}",
        normalization.rule_id, normalization.input, normalization.normalized
    );
}

#[test]
fn t56_tile_refresh_id_stability_requires_tombstone_or_retained_alias() {
    let cluster_a = "cmdrvl:parcel:01J7X0000000000000000D";
    let cluster_b = "cmdrvl:parcel:01J7X0000000000000000E";
    let cluster_c = "cmdrvl:parcel:01J7X0000000000000000F";
    let before = vintage(
        "v1",
        vec![cluster(
            cluster_a,
            GeoEntityLevel::Parcel,
            digest('d'),
            &["parcel:nyc:bbl:1000000001"],
        )],
        vec![],
    );
    let after_tombstone = vintage(
        "v2",
        vec![cluster(
            cluster_b,
            GeoEntityLevel::Parcel,
            digest('e'),
            &["parcel:nyc:bbl:1000000002"],
        )],
        vec![tombstone(cluster_a, digest('d'))],
    );
    let diff = diff_tile_identifier_vintages(&before, &after_tombstone)
        .expect("tombstone plus added cluster is valid");
    assert_eq!(diff.added_cluster_ids, vec![cluster_b.to_string()]);
    assert_eq!(diff.tombstoned_cluster_ids, vec![cluster_a.to_string()]);
    println!(
        "T56 tombstone diff v1={:?} v2={:?} diff={diff:?}",
        before
            .clusters
            .iter()
            .map(|cluster| cluster.cluster_id.as_str())
            .collect::<Vec<_>>(),
        after_tombstone
            .clusters
            .iter()
            .map(|cluster| cluster.cluster_id.as_str())
            .collect::<Vec<_>>()
    );

    let reassigned = vintage(
        "v2-bad",
        vec![cluster(
            cluster_a,
            GeoEntityLevel::Parcel,
            digest('f'),
            &["parcel:nyc:bbl:1000000001"],
        )],
        vec![],
    );
    let error = diff_tile_identifier_vintages(&before, &reassigned)
        .expect_err("same minted id with changed geometry refuses");
    let geometry_before = digest('d');
    let geometry_after = digest('f');
    assert_eq!(error.code, GeoIdentifierErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("cluster_id").map(String::as_str),
        Some(cluster_a)
    );
    assert_eq!(
        error
            .detail
            .get("geometry_blake3_before")
            .map(String::as_str),
        Some(geometry_before.as_str())
    );
    assert_eq!(
        error
            .detail
            .get("geometry_blake3_after")
            .map(String::as_str),
        Some(geometry_after.as_str())
    );

    let merge_before = vintage(
        "merge-v1",
        vec![
            cluster(
                cluster_a,
                GeoEntityLevel::Parcel,
                digest('d'),
                &["parcel:nyc:bbl:1000000001"],
            ),
            cluster(
                cluster_b,
                GeoEntityLevel::Parcel,
                digest('e'),
                &["parcel:nyc:bbl:1000000002"],
            ),
        ],
        vec![],
    );
    let merge_after = vintage(
        "merge-v2",
        vec![cluster(
            cluster_c,
            GeoEntityLevel::Parcel,
            digest('f'),
            &[cluster_a, cluster_b, "parcel:nyc:bbl:1000000003"],
        )],
        vec![],
    );
    let merge_diff = diff_tile_identifier_vintages(&merge_before, &merge_after)
        .expect("merged clusters retain prior ids as aliases");
    assert_eq!(
        merge_diff.merged_prior_ids,
        vec![cluster_a.to_string(), cluster_b.to_string()]
    );

    let bad_merge_after = vintage(
        "merge-v2-bad",
        vec![cluster(
            cluster_c,
            GeoEntityLevel::Parcel,
            digest('f'),
            &[cluster_a, "parcel:nyc:bbl:1000000003"],
        )],
        vec![],
    );
    let bad_merge = diff_tile_identifier_vintages(&merge_before, &bad_merge_after)
        .expect_err("dropped prior id must be named");
    assert_eq!(bad_merge.code, GeoIdentifierErrorCode::InvalidInput);
    assert_eq!(
        bad_merge.detail.get("cluster_id").map(String::as_str),
        Some(cluster_b)
    );
}

#[test]
fn t57_property_set_algebra_is_exact_and_scoreless() {
    let parcel_a = "cmdrvl:parcel:01J7X0000000000000000G";
    let parcel_b = "cmdrvl:parcel:01J7X0000000000000000H";
    let parcel_c = "cmdrvl:parcel:01J7X0000000000000000I";
    let building_a = "cmdrvl:building:01J7X000000000000000A";
    let building_b = "cmdrvl:building:01J7X000000000000000B";
    let left = property(
        "cmdrvl:property:01J7X000000000000000A",
        "0000000000-26-000001",
        "loan-a",
        &[parcel_a, parcel_b],
        &[building_a],
    );
    let equal = property(
        "cmdrvl:property:01J7X000000000000000B",
        "0000000000-26-000002",
        "loan-b",
        &[parcel_b, parcel_a],
        &[building_a],
    );
    let superset = property(
        "cmdrvl:property:01J7X000000000000000C",
        "0000000000-26-000003",
        "loan-c",
        &[parcel_a, parcel_b, parcel_c],
        &[building_a, building_b],
    );
    let overlapping = property(
        "cmdrvl:property:01J7X000000000000000D",
        "0000000000-26-000004",
        "loan-d",
        &[parcel_b, parcel_c],
        &[building_b],
    );

    let same = compare_property_sets(&left, &equal).expect("equal sets compare");
    assert_eq!(same.relation, GeoPropertySetRelation::SameCollateral);

    let larger = compare_property_sets(&superset, &left).expect("superset compares");
    assert_eq!(larger.relation, GeoPropertySetRelation::LeftSuperset);
    assert_eq!(larger.left_only_parcel_ids, vec![parcel_c.to_string()]);
    assert_eq!(larger.left_only_building_ids, vec![building_b.to_string()]);

    let smaller = compare_property_sets(&left, &superset).expect("subset compares");
    assert_eq!(smaller.relation, GeoPropertySetRelation::LeftSubset);

    let intersecting = compare_property_sets(&left, &overlapping).expect("intersection compares");
    assert_eq!(intersecting.relation, GeoPropertySetRelation::Intersects);
    assert_eq!(intersecting.shared_parcel_ids, vec![parcel_b.to_string()]);
    assert!(intersecting.shared_building_ids.is_empty());

    let serialized = serde_json::to_value(&intersecting).expect("comparison serializes");
    assert_no_probabilistic_fields(&serialized);
    println!(
        "T57 left members parcels={:?} buildings={:?}; right parcels={:?} buildings={:?}; relation={:?}",
        left.parcel_ids,
        left.building_ids,
        overlapping.parcel_ids,
        overlapping.building_ids,
        intersecting.relation
    );
}

#[test]
fn t58_registry_proposal_from_ledger_rows_preserves_denominator_and_skips_reach_none() {
    let ledger_json = br#"{
      "version": "canon_geo_collateral_ledger.v0",
      "rows": [
        {
          "accession": "0000000000-26-000001",
          "deal_id": "fixture-deal-a",
          "loan_id": "loan-a",
          "reach": "full",
          "parcel_set": ["parcel:nyc:bbl:1004540041", "attom:parcel:1004540041"],
          "building_set": ["building:nyc:bin:1006494"],
          "score": 0.99
        },
        {
          "accession": "0000000000-26-000001",
          "deal_id": "fixture-deal-a",
          "loan_id": "loan-b",
          "reach": "none",
          "reach_none_reason": "no_candidate_parcels"
        }
      ]
    }"#;

    let proposal =
        registry_proposal_from_ledger_json(ledger_json).expect("ledger rows project to proposal");
    assert_eq!(proposal.version, CANON_GEO_REGISTRY_PROPOSAL_VERSION);
    assert_eq!(proposal.summary.ledger_rows, 2);
    assert_eq!(proposal.summary.skipped_reach_none_rows, 1);
    assert_eq!(proposal.summary.unique_parcel_aliases, 2);
    assert_eq!(proposal.summary.unique_building_aliases, 1);
    assert_eq!(proposal.summary.property_assertions, 1);
    assert_eq!(proposal.summary.entries, 4);
    assert!(proposal.source_ledger_blake3.starts_with("blake3:"));

    let by_alias = proposal
        .entries
        .iter()
        .map(|entry| (entry.alias.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        by_alias
            .get("parcel:nyc:bbl:1004540041")
            .map(|entry| entry.canonical_type.as_str()),
        Some("parcel")
    );
    assert_eq!(
        by_alias
            .get("attom:parcel:1004540041")
            .map(|entry| entry.canonical_type.as_str()),
        Some("parcel")
    );
    assert_ne!(
        by_alias["parcel:nyc:bbl:1004540041"].canonical_id.as_str(),
        by_alias["attom:parcel:1004540041"].canonical_id.as_str(),
        "role namespace stripping would collapse two distinct parcel aliases"
    );
    assert_eq!(
        by_alias
            .get("building:nyc:bin:1006494")
            .map(|entry| entry.canonical_type.as_str()),
        Some("building")
    );
    assert_eq!(
        by_alias
            .get("cmbs:annexa:0000000000-26-000001:loan-a")
            .map(|entry| entry.canonical_type.as_str()),
        Some("property")
    );

    let assertion = proposal
        .property_assertions
        .first()
        .expect("one document assertion");
    assert_eq!(assertion.accession, "0000000000-26-000001");
    assert_eq!(assertion.loan_id, "loan-a");
    assert_eq!(assertion.parcel_ids.len(), 2);
    assert_eq!(assertion.building_ids.len(), 1);
    assert!(assertion
        .parcel_ids
        .iter()
        .all(|id| id.starts_with("cmdrvl:parcel:")));
    assert!(assertion
        .building_ids
        .iter()
        .all(|id| id.starts_with("cmdrvl:building:")));

    let serialized = serde_json::to_value(&proposal).expect("proposal serializes");
    assert_no_probabilistic_fields(&serialized);
    println!(
        "T58 proposal source={} aliases={:?} canonical_ids={:?}",
        proposal.source_ledger_blake3,
        by_alias.keys().collect::<Vec<_>>(),
        proposal
            .entries
            .iter()
            .map(|entry| (entry.alias.as_str(), entry.canonical_id.as_str()))
            .collect::<BTreeMap<_, _>>()
    );

    let reach_none_with_sets = br#"{
      "rows": [
        {
          "accession": "0000000000-26-000001",
          "deal_id": "fixture-deal-a",
          "loan_id": "loan-reach-none",
          "reach": "none",
          "reach_none_reason": "no_candidate_parcels",
          "parcel_set": ["parcel:nyc:bbl:1004540041"]
        }
      ]
    }"#;
    let error = registry_proposal_from_ledger_json(reach_none_with_sets)
        .expect_err("reach none must not fabricate identifier sets");
    assert_eq!(error.code, GeoIdentifierErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("parcel_set_or_building_set")
    );

    let confused_level = br#"{
      "rows": [
        {
          "accession": "0000000000-26-000001",
          "deal_id": "fixture-deal-a",
          "loan_id": "loan-confused",
          "reach": "full",
          "parcel_set": ["cmdrvl:building:01J7X000000000000000Z"]
        }
      ]
    }"#;
    let error =
        registry_proposal_from_ledger_json(confused_level).expect_err("wrong level refuses");
    assert_eq!(error.code, GeoIdentifierErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("parcel_set")
    );
}
