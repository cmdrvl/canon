use canon::geo::{
    CANON_GEO_TEMPORAL_CONTAINMENT_VERSION, GeoContainmentAsOfQuery, GeoEntityLevel,
    GeoLifecycleErrorCode, GeoTemporalContainmentArtifact, GeoTemporalContainmentCluster,
    GeoTemporalContainmentEdge, GeoTemporalContainmentInterval, GeoTemporalContainmentRelation,
    GeoTemporalContainmentSourceReceipt, GeoTemporalContainmentSummary,
    canonical_temporal_containment_bytes, containment_as_of,
    validate_temporal_containment_artifact,
};

#[test]
fn containment_as_of_answers_the_2020_shape_without_leaking_to_2019() {
    let artifact = temporal_containment_fixture();
    validate_temporal_containment_artifact(&artifact).expect("fixture is canonical");

    let as_2020 = containment_as_of(
        &artifact,
        &GeoContainmentAsOfQuery {
            as_of_utc_day: "2020-06-01".to_string(),
            parent_cluster_id: Some(parcel_id()),
            child_cluster_id: None,
        },
    )
    .expect("2020 query succeeds");
    assert_eq!(as_2020.edges.len(), 7);
    assert_eq!(as_2020.summary.parent_clusters, 1);
    assert_eq!(as_2020.summary.child_clusters, 7);
    assert!(
        as_2020
            .edges
            .iter()
            .all(|edge| edge.parent_cluster_id == parcel_id())
    );

    let as_2019 = containment_as_of(
        &artifact,
        &GeoContainmentAsOfQuery {
            as_of_utc_day: "2019-06-01".to_string(),
            parent_cluster_id: Some(parcel_id()),
            child_cluster_id: None,
        },
    )
    .expect("2019 query succeeds");
    assert!(
        as_2019.edges.is_empty(),
        "2019 query must not return buildings whose containment starts in 2020"
    );
}

#[test]
fn canonical_bytes_are_deterministic_under_edge_and_cluster_order_shuffle() {
    let canonical = temporal_containment_fixture();
    let mut shuffled = canonical.clone();
    shuffled.clusters.reverse();
    shuffled.edges.reverse();
    shuffled.summary = GeoTemporalContainmentSummary {
        clusters: shuffled.clusters.len() as u64,
        edges: shuffled.edges.len() as u64,
    };

    let left = canonical_temporal_containment_bytes(&canonical).expect("canonical bytes");
    let right = canonical_temporal_containment_bytes(&shuffled).expect("shuffled canonical bytes");
    assert_eq!(left, right);
}

#[test]
fn validator_rejects_unsorted_and_duplicate_edges() {
    let mut unsorted = temporal_containment_fixture();
    unsorted.edges.swap(0, 1);
    let error = validate_temporal_containment_artifact(&unsorted)
        .expect_err("validator rejects non-canonical edge order");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(error.detail.get("field").map(String::as_str), Some("edges"));

    let mut duplicate = temporal_containment_fixture();
    duplicate.edges[1].child_cluster_id = duplicate.edges[0].child_cluster_id.clone();
    duplicate.edges[1].edge_id = "edge-duplicate-id-kept-distinct".to_string();
    let error = validate_temporal_containment_artifact(&duplicate)
        .expect_err("validator rejects duplicate semantic containment");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(error.detail.get("field").map(String::as_str), Some("edges"));
}

#[test]
fn validator_rejects_unknown_or_misleveled_endpoints() {
    let mut unknown = temporal_containment_fixture();
    unknown.edges[0].child_cluster_id = "cmdrvl:building:missing".to_string();
    let error = validate_temporal_containment_artifact(&unknown)
        .expect_err("validator rejects unknown child cluster");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("edges[].child_cluster_id")
    );

    let mut misleveled = temporal_containment_fixture();
    misleveled.edges[0].child_level = GeoEntityLevel::Parcel;
    let error = validate_temporal_containment_artifact(&misleveled)
        .expect_err("validator rejects endpoint level mismatch");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("edges[].child_cluster_id")
    );
}

#[test]
fn validator_rejects_bad_intervals_and_missing_receipts() {
    let mut bad_interval = temporal_containment_fixture();
    bad_interval.edges[0].valid_interval.start_utc_day = "2021-01-01".to_string();
    bad_interval.edges[0].valid_interval.end_utc_day = "2020-01-01".to_string();
    let error = validate_temporal_containment_artifact(&bad_interval)
        .expect_err("validator rejects inverted intervals");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("valid_interval")
    );

    let mut missing_receipt = temporal_containment_fixture();
    missing_receipt.edges[0].source_receipt.rule_id.clear();
    let error = validate_temporal_containment_artifact(&missing_receipt)
        .expect_err("validator rejects missing source receipt rule_id");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("source_receipt.rule_id")
    );

    let mut bad_digest = temporal_containment_fixture();
    bad_digest.edges[0].source_receipt.source_record_blake3 = "sha256:not-a-blake3".to_string();
    let error = validate_temporal_containment_artifact(&bad_digest)
        .expect_err("validator rejects non-blake3 receipt digest");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("source_receipt.source_record_blake3")
    );
}

fn temporal_containment_fixture() -> GeoTemporalContainmentArtifact {
    let mut clusters = vec![GeoTemporalContainmentCluster {
        cluster_id: parcel_id(),
        entity_level: GeoEntityLevel::Parcel,
    }];
    clusters.extend((1..=7).map(|building| GeoTemporalContainmentCluster {
        cluster_id: building_id(building),
        entity_level: GeoEntityLevel::Building,
    }));
    clusters.sort_by(|left, right| left.cluster_id.cmp(&right.cluster_id));

    let mut edges = (1..=7)
        .map(|building| GeoTemporalContainmentEdge {
            edge_id: format!("edge-b{building:02}-part-of-p201"),
            parent_cluster_id: parcel_id(),
            parent_level: GeoEntityLevel::Parcel,
            child_cluster_id: building_id(building),
            child_level: GeoEntityLevel::Building,
            relation: GeoTemporalContainmentRelation::PartOf,
            valid_interval: GeoTemporalContainmentInterval {
                start_utc_day: "2020-01-01".to_string(),
                end_utc_day: "2020-12-31".to_string(),
            },
            source_receipt: GeoTemporalContainmentSourceReceipt {
                receipt_id: format!("receipt-building-{building:02}"),
                source_dataset: "fixture.nyc.lifecycle".to_string(),
                source_record_id: format!("dob-job:fixture:{building:02}"),
                source_record_blake3: blake3_uri(&format!("dob-job:fixture:{building:02}")),
                proof_class: "fixture".to_string(),
                rule_id: "geo_temporal_containment_fixture.v1".to_string(),
            },
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        left.parent_cluster_id
            .cmp(&right.parent_cluster_id)
            .then_with(|| left.child_cluster_id.cmp(&right.child_cluster_id))
            .then_with(|| {
                left.valid_interval
                    .start_utc_day
                    .cmp(&right.valid_interval.start_utc_day)
            })
            .then_with(|| {
                left.valid_interval
                    .end_utc_day
                    .cmp(&right.valid_interval.end_utc_day)
            })
            .then_with(|| left.edge_id.cmp(&right.edge_id))
    });

    GeoTemporalContainmentArtifact {
        version: CANON_GEO_TEMPORAL_CONTAINMENT_VERSION.to_string(),
        mart_id: "fixture.nyc.lifecycle.2019-2020".to_string(),
        summary: GeoTemporalContainmentSummary {
            clusters: clusters.len() as u64,
            edges: edges.len() as u64,
        },
        clusters,
        edges,
    }
}

fn parcel_id() -> String {
    "cmdrvl:parcel:nyc:bbl:fixture-201".to_string()
}

fn building_id(number: u8) -> String {
    format!("cmdrvl:building:nyc:bin:fixture-201-{number:02}")
}

fn blake3_uri(input: &str) -> String {
    format!("blake3:{}", blake3::hash(input.as_bytes()).to_hex())
}
