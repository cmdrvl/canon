#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_AS_OF_RESOLUTION_REQUEST_VERSION, CANON_GEO_AS_OF_RESOLUTION_VERSION,
    CANON_GEO_TILE_IDENTIFIER_STABILITY_REQUEST_VERSION,
    CANON_GEO_TILE_IDENTIFIER_STABILITY_VERSION, GeoAsOfLayerWindow, GeoAsOfParcelLookup,
    GeoAsOfParcelResolutionReason, GeoAsOfParcelResolutionStatus, GeoAsOfResolutionErrorCode,
    GeoAsOfResolutionRequest, GeoBblChangeLedgerRow, GeoClientTileAliasBinding,
    GeoClientTileAliasImpactStatus, GeoEntityLevel, GeoIdentifierCluster, GeoIdentifierErrorCode,
    GeoIdentifierTombstone, GeoMapPlutoParcelVintageRow, GeoTileIdentifierContractDisposition,
    GeoTileIdentifierStabilityRequest, GeoTileIdentifierVintage, canonical_as_of_resolution_bytes,
    canonical_tile_identifier_stability_bytes, check_tile_identifier_stability, resolve_geo_as_of,
};
use serde_json::Value;
use std::collections::BTreeMap;

const AS_OF_SCHEMA: &str = include_str!("../schemas/canon.geo.as_of_resolution.v0.schema.json");
const STABILITY_SCHEMA: &str =
    include_str!("../schemas/canon.geo.tile_identifier_stability.v0.schema.json");

fn digest(hex: char) -> String {
    format!("blake3:{}", hex.to_string().repeat(64))
}

fn layer(
    layer_id: &str,
    vintage_id: &str,
    valid_from_utc_day: &str,
    valid_to_utc_day: &str,
) -> GeoAsOfLayerWindow {
    GeoAsOfLayerWindow {
        layer_id: layer_id.to_string(),
        source_dataset: "warehouse.geo.nyc.mappluto".to_string(),
        vintage_id: vintage_id.to_string(),
        valid_from_utc_day: valid_from_utc_day.to_string(),
        valid_to_utc_day: valid_to_utc_day.to_string(),
        content_digest: digest('a'),
    }
}

fn lookup(lookup_id: &str, bbl_key: &str) -> GeoAsOfParcelLookup {
    GeoAsOfParcelLookup {
        lookup_id: lookup_id.to_string(),
        bbl_key: bbl_key.to_string(),
    }
}

fn parcel_row(
    bbl_key: &str,
    release: &str,
    release_dt: &str,
    valid_from_release_dt: &str,
    valid_to_release_dt: Option<&str>,
    parcel_cluster_id: Option<&str>,
) -> GeoMapPlutoParcelVintageRow {
    GeoMapPlutoParcelVintageRow {
        bbl_key: bbl_key.to_string(),
        release: release.to_string(),
        release_dt: release_dt.to_string(),
        valid_from_release_dt: valid_from_release_dt.to_string(),
        valid_to_release_dt: valid_to_release_dt.map(str::to_string),
        geometry_digest: Some(digest('b')),
        parcel_cluster_id: parcel_cluster_id.map(str::to_string),
        source_record_id: Some(format!("mappluto:{release}:{bbl_key}")),
        source_record_blake3: Some(digest('c')),
    }
}

fn change_event(
    event_id: &str,
    current_release_dt: &str,
    subject_bbl_key: &str,
    predecessors: &[&str],
    successors: &[&str],
) -> GeoBblChangeLedgerRow {
    GeoBblChangeLedgerRow {
        change_event_id: event_id.to_string(),
        event_type: "split".to_string(),
        canon_resolution: Some("successor_requires_as_of_review".to_string()),
        previous_release: Some("20v1".to_string()),
        previous_release_dt: Some("2020-06-01".to_string()),
        current_release: "21v1".to_string(),
        current_release_dt: current_release_dt.to_string(),
        subject_bbl_key: subject_bbl_key.to_string(),
        resolved_bbl_key: None,
        canonical_bbl_key: None,
        predecessor_candidate_bbl_keys: predecessors
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        successor_candidate_bbl_keys: successors
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        source_record_id: Some(format!("bbl-ledger:{event_id}")),
        source_record_blake3: Some(digest('d')),
    }
}

fn as_of_request(as_of_utc_day: &str) -> GeoAsOfResolutionRequest {
    GeoAsOfResolutionRequest {
        version: CANON_GEO_AS_OF_RESOLUTION_REQUEST_VERSION.to_string(),
        as_of_utc_day: as_of_utc_day.to_string(),
        tile_layer: layer(
            "tile-mappluto",
            "mappluto-window",
            "2010-01-01",
            "2026-12-31",
        ),
        client_layer: Some(layer(
            "client-byop",
            "client-2017-fixture",
            "2015-01-01",
            "2020-12-31",
        )),
        lookups: vec![lookup("subject-a", "1000000001")],
        parcel_vintages: Vec::new(),
        change_ledger: Vec::new(),
    }
}

#[test]
fn as_of_resolution_resolves_historical_row_and_refuses_outside_layer_windows() {
    let mut request = as_of_request("2017-06-01");
    request.parcel_vintages = vec![parcel_row(
        "1000000001",
        "17v1",
        "2017-02-01",
        "2010-01-01",
        Some("2020-12-31"),
        Some("cmdrvl:parcel:nyc:bbl:1000000001"),
    )];

    let artifact = resolve_geo_as_of(&request).expect("historical BBL resolves at as-of date");
    assert_eq!(artifact.version, CANON_GEO_AS_OF_RESOLUTION_VERSION);
    assert_eq!(artifact.as_of_utc_day, "2017-06-01");
    assert_eq!(artifact.summary.lookups, 1);
    assert_eq!(artifact.summary.resolved, 1);
    assert_eq!(artifact.summary.abstained, 0);
    let row = artifact.resolutions.first().expect("one resolution");
    assert_eq!(row.status, GeoAsOfParcelResolutionStatus::Resolved);
    assert_eq!(row.reason, GeoAsOfParcelResolutionReason::ActiveAtAsOf);
    assert_eq!(
        row.parcel_cluster_id.as_deref(),
        Some("cmdrvl:parcel:nyc:bbl:1000000001")
    );
    assert_eq!(row.matched_release.as_deref(), Some("17v1"));

    let mut before_tile_window = request.clone();
    before_tile_window.as_of_utc_day = "2009-12-31".to_string();
    let error = resolve_geo_as_of(&before_tile_window)
        .expect_err("as-of before earliest tile vintage refuses");
    assert_eq!(
        error.code,
        GeoAsOfResolutionErrorCode::OutsideAvailableVintage
    );
    assert_eq!(
        error.detail.get("layer_role").map(String::as_str),
        Some("tile_layer")
    );

    let mut before_client_window = request.clone();
    before_client_window.client_layer = Some(layer(
        "client-byop",
        "client-2020-only",
        "2020-01-01",
        "2026-12-31",
    ));
    let error = resolve_geo_as_of(&before_client_window)
        .expect_err("as-of before declared client layer vintage refuses");
    assert_eq!(
        error.code,
        GeoAsOfResolutionErrorCode::OutsideAvailableVintage
    );
    assert_eq!(
        error.detail.get("layer_role").map(String::as_str),
        Some("client_layer")
    );
}

#[test]
fn as_of_resolution_never_matches_future_successor_or_reused_bbl() {
    let mut request = as_of_request("2017-06-01");
    request.client_layer = None;
    request.parcel_vintages = vec![parcel_row(
        "1000000001",
        "26v1",
        "2026-05-01",
        "2021-01-01",
        None,
        Some("cmdrvl:parcel:nyc:bbl:successor-2026"),
    )];
    request.change_ledger = vec![change_event(
        "event-future-successor",
        "2021-01-01",
        "1000000001",
        &["1000000001"],
        &["1000000002"],
    )];

    let artifact = resolve_geo_as_of(&request)
        .expect("future successor row abstains instead of current-only fallback");
    assert_eq!(artifact.summary.resolved, 0);
    assert_eq!(artifact.summary.abstained, 1);
    let row = artifact.resolutions.first().expect("one resolution");
    assert_eq!(row.status, GeoAsOfParcelResolutionStatus::Abstained);
    assert_eq!(row.reason, GeoAsOfParcelResolutionReason::NotPresentAsOf);
    assert_eq!(row.parcel_cluster_id, None);
    assert!(row.change_events.is_empty());
}

#[test]
fn as_of_resolution_surfaces_change_ledger_only_after_event_date() {
    let mut request = as_of_request("2022-01-01");
    request.client_layer = None;
    request.change_ledger = vec![change_event(
        "event-split-2021",
        "2021-01-01",
        "1000000001",
        &["1000000001"],
        &["1000000003", "1000000002"],
    )];

    let artifact = resolve_geo_as_of(&request).expect("past change event is consumable");
    let row = artifact.resolutions.first().expect("one resolution");
    assert_eq!(row.status, GeoAsOfParcelResolutionStatus::Abstained);
    assert_eq!(row.reason, GeoAsOfParcelResolutionReason::ChangedBeforeAsOf);
    assert_eq!(artifact.summary.change_events_used, 1);
    let event = row.change_events.first().expect("change event surfaced");
    assert_eq!(event.change_event_id, "event-split-2021");
    assert_eq!(event.successor_bbl_keys, vec!["1000000002", "1000000003"]);
}

fn cluster(cluster_id: &str, geometry_hex: char, aliases: &[&str]) -> GeoIdentifierCluster {
    GeoIdentifierCluster {
        cluster_id: cluster_id.to_string(),
        entity_level: GeoEntityLevel::Parcel,
        geometry_blake3: digest(geometry_hex),
        aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
    }
}

fn tombstone(
    cluster_id: &str,
    geometry_hex: char,
    reason: &str,
    successors: &[&str],
    survivor: Option<&str>,
) -> GeoIdentifierTombstone {
    GeoIdentifierTombstone {
        cluster_id: cluster_id.to_string(),
        geometry_blake3: digest(geometry_hex),
        reason: reason.to_string(),
        successor_cluster_ids: successors.iter().map(|id| (*id).to_string()).collect(),
        survivor_cluster_id: survivor.map(str::to_string),
    }
}

fn vintage(
    vintage_id: &str,
    clusters: Vec<GeoIdentifierCluster>,
    tombstones: Vec<GeoIdentifierTombstone>,
) -> GeoTileIdentifierVintage {
    GeoTileIdentifierVintage {
        tile_id: "h3:r8:892a100d67fffff".to_string(),
        vintage_id: vintage_id.to_string(),
        clusters,
        tombstones,
    }
}

fn stability_request(
    before: GeoTileIdentifierVintage,
    after: GeoTileIdentifierVintage,
) -> GeoTileIdentifierStabilityRequest {
    GeoTileIdentifierStabilityRequest {
        version: CANON_GEO_TILE_IDENTIFIER_STABILITY_REQUEST_VERSION.to_string(),
        before,
        after,
        client_aliases: Vec::new(),
    }
}

#[test]
fn tile_identifier_stability_rejects_reassignment_and_retired_id_reuse() {
    let before = vintage(
        "refresh-before",
        vec![cluster("cmdrvl:parcel:stable-a", 'a', &[])],
        Vec::new(),
    );
    let after_reassigned = vintage(
        "refresh-after",
        vec![cluster("cmdrvl:parcel:stable-a", 'b', &[])],
        Vec::new(),
    );
    let error = check_tile_identifier_stability(&stability_request(before, after_reassigned))
        .expect_err("reassigning a minted id to different geometry refuses");
    assert_eq!(error.code, GeoIdentifierErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("cluster_id").map(String::as_str),
        Some("cmdrvl:parcel:stable-a")
    );
    assert!(error.detail.contains_key("geometry_blake3_before"));
    assert!(error.detail.contains_key("geometry_blake3_after"));

    let before_with_retired_id = vintage(
        "refresh-before-with-retired",
        vec![cluster("cmdrvl:parcel:stable-a", 'a', &[])],
        vec![tombstone(
            "cmdrvl:parcel:retired-r",
            'c',
            "retired_by_prior_tile_refresh",
            &[],
            None,
        )],
    );
    let after_reused_retired_id = vintage(
        "refresh-after-reused-retired",
        vec![
            cluster("cmdrvl:parcel:stable-a", 'a', &[]),
            cluster("cmdrvl:parcel:retired-r", 'd', &[]),
        ],
        Vec::new(),
    );
    let error = check_tile_identifier_stability(&stability_request(
        before_with_retired_id,
        after_reused_retired_id,
    ))
    .expect_err("retired ids are never reused");
    assert_eq!(error.code, GeoIdentifierErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("cluster_id").map(String::as_str),
        Some("cmdrvl:parcel:retired-r")
    );
}

#[test]
fn tile_identifier_stability_reports_contract_and_client_alias_impacts() {
    let before = vintage(
        "refresh-before",
        vec![
            cluster("cmdrvl:parcel:split-a", 'a', &[]),
            cluster("cmdrvl:parcel:merge-b", 'b', &[]),
            cluster("cmdrvl:parcel:stable-c", 'c', &[]),
        ],
        Vec::new(),
    );
    let after = vintage(
        "refresh-after",
        vec![
            cluster("cmdrvl:parcel:stable-c", 'c', &[]),
            cluster("cmdrvl:parcel:survivor-d", 'd', &["cmdrvl:parcel:merge-b"]),
            cluster("cmdrvl:parcel:split-e", 'e', &[]),
            cluster("cmdrvl:parcel:split-f", 'f', &[]),
        ],
        vec![tombstone(
            "cmdrvl:parcel:split-a",
            'a',
            "split_by_tile_refresh",
            &["cmdrvl:parcel:split-f", "cmdrvl:parcel:split-e"],
            None,
        )],
    );
    let mut request = stability_request(before, after);
    request.client_aliases = vec![
        GeoClientTileAliasBinding {
            client_alias: "client-active".to_string(),
            cluster_id: "cmdrvl:parcel:stable-c".to_string(),
        },
        GeoClientTileAliasBinding {
            client_alias: "client-merged".to_string(),
            cluster_id: "cmdrvl:parcel:merge-b".to_string(),
        },
        GeoClientTileAliasBinding {
            client_alias: "client-split".to_string(),
            cluster_id: "cmdrvl:parcel:split-a".to_string(),
        },
        GeoClientTileAliasBinding {
            client_alias: "client-unknown".to_string(),
            cluster_id: "cmdrvl:parcel:not-in-tile".to_string(),
        },
    ];

    let artifact =
        check_tile_identifier_stability(&request).expect("contract-compatible refresh checks");
    let rules = artifact
        .contract
        .rules
        .iter()
        .map(|rule| (rule.rule_id.as_str(), rule.disposition))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        rules.get("reassign_existing_id"),
        Some(&GeoTileIdentifierContractDisposition::Never)
    );
    assert_eq!(
        rules.get("reuse_retired_id"),
        Some(&GeoTileIdentifierContractDisposition::Never)
    );
    assert_eq!(
        artifact.diff.retained_cluster_ids,
        vec!["cmdrvl:parcel:stable-c"]
    );
    assert_eq!(
        artifact.diff.merged_prior_ids,
        vec!["cmdrvl:parcel:merge-b"]
    );
    assert_eq!(
        artifact.diff.tombstoned_cluster_ids,
        vec!["cmdrvl:parcel:split-a"]
    );
    assert_eq!(artifact.summary.client_aliases, 4);
    assert_eq!(artifact.summary.stale_client_aliases, 3);

    let impacts = artifact
        .client_alias_impacts
        .iter()
        .map(|impact| (impact.client_alias.as_str(), impact))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        impacts["client-active"].status,
        GeoClientTileAliasImpactStatus::Active
    );
    assert_eq!(
        impacts["client-merged"].status,
        GeoClientTileAliasImpactStatus::MergedToSurvivor
    );
    assert_eq!(
        impacts["client-merged"].replacement_cluster_ids,
        vec!["cmdrvl:parcel:survivor-d"]
    );
    assert_eq!(
        impacts["client-split"].status,
        GeoClientTileAliasImpactStatus::Tombstoned
    );
    assert_eq!(
        impacts["client-split"].replacement_cluster_ids,
        vec!["cmdrvl:parcel:split-e", "cmdrvl:parcel:split-f"]
    );
    assert_eq!(
        impacts["client-unknown"].status,
        GeoClientTileAliasImpactStatus::UnknownCluster
    );
}

#[test]
fn as_of_resolution_schema_matches_real_request_and_artifact_instances() {
    let mut request = as_of_request("2017-06-01");
    request.parcel_vintages = vec![parcel_row(
        "1000000001",
        "17v1",
        "2017-02-01",
        "2010-01-01",
        Some("2020-12-31"),
        Some("cmdrvl:parcel:nyc:bbl:1000000001"),
    )];
    let request_value = serde_json::to_value(&request).expect("request serializes");
    let artifact = resolve_geo_as_of(&request).expect("artifact builds");
    let artifact_value: Value = serde_json::from_slice(
        &canonical_as_of_resolution_bytes(&artifact).expect("artifact canonicalizes"),
    )
    .expect("artifact parses");

    assert_contract_schema_instance(
        AS_OF_SCHEMA,
        "canon.geo.as_of_resolution.v0",
        CANON_GEO_AS_OF_RESOLUTION_VERSION,
        Some(("canon_geo_as_of_resolution_request.v0", &request_value)),
        &artifact_value,
    );
}

#[test]
fn tile_identifier_stability_schema_matches_real_request_and_artifact_instances() {
    let before = vintage(
        "refresh-before",
        vec![cluster("cmdrvl:parcel:stable-a", 'a', &[])],
        Vec::new(),
    );
    let after = vintage(
        "refresh-after",
        vec![
            cluster("cmdrvl:parcel:stable-a", 'a', &[]),
            cluster("cmdrvl:parcel:added-b", 'b', &[]),
        ],
        Vec::new(),
    );
    let request = stability_request(before, after);
    let request_value = serde_json::to_value(&request).expect("request serializes");
    let artifact = check_tile_identifier_stability(&request).expect("artifact builds");
    let artifact_value: Value = serde_json::from_slice(
        &canonical_tile_identifier_stability_bytes(&artifact).expect("artifact canonicalizes"),
    )
    .expect("artifact parses");

    assert_contract_schema_instance(
        STABILITY_SCHEMA,
        "canon.geo.tile_identifier_stability.v0",
        CANON_GEO_TILE_IDENTIFIER_STABILITY_VERSION,
        Some((
            "canon_geo_tile_identifier_stability_request.v0",
            &request_value,
        )),
        &artifact_value,
    );
}

fn assert_contract_schema_instance(
    schema_source: &str,
    title: &str,
    version: &str,
    request: Option<(&str, &Value)>,
    artifact: &Value,
) {
    let schema: Value = serde_json::from_str(schema_source).expect("schema parses");
    assert_eq!(
        schema.pointer("/title").and_then(Value::as_str),
        Some(title)
    );
    assert_eq!(
        schema
            .pointer("/properties/version/const")
            .and_then(Value::as_str),
        Some(version)
    );
    assert_eq!(
        schema
            .pointer("/additionalProperties")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_instance_keys_declared(&schema, &schema, artifact, "$");

    if let Some((request_version, request_value)) = request {
        let request_schema = schema
            .pointer("/$defs/request")
            .expect("schema carries request definition");
        assert_eq!(
            request_schema
                .pointer("/properties/version/const")
                .and_then(Value::as_str),
            Some(request_version)
        );
        assert_instance_keys_declared(&schema, request_schema, request_value, "$defs.request");
    }
}

fn assert_instance_keys_declared(root: &Value, schema: &Value, instance: &Value, path: &str) {
    let schema = resolve_schema(root, schema);
    match instance {
        Value::Object(map) => {
            let properties = schema
                .get("properties")
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("schema at {path} does not declare object properties"));
            for (key, value) in map {
                let child_schema = properties
                    .get(key)
                    .unwrap_or_else(|| panic!("schema at {path} does not declare key {key}"));
                assert_instance_keys_declared(root, child_schema, value, &format!("{path}.{key}"));
            }
        }
        Value::Array(values) => {
            let item_schema = schema
                .get("items")
                .unwrap_or_else(|| panic!("schema at {path} does not declare array items"));
            for (index, value) in values.iter().enumerate() {
                assert_instance_keys_declared(
                    root,
                    item_schema,
                    value,
                    &format!("{path}[{index}]"),
                );
            }
        }
        _ => {}
    }
}

fn resolve_schema<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    let Some(reference) = schema.get("$ref").and_then(Value::as_str) else {
        return schema;
    };
    let pointer = reference
        .strip_prefix('#')
        .unwrap_or_else(|| panic!("only local refs are allowed in fixture schemas: {reference}"));
    root.pointer(pointer)
        .unwrap_or_else(|| panic!("unresolvable schema ref {reference}"))
}
