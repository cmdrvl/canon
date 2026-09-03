#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_GEOMETRY_REQUEST_VERSION, CANON_GEO_LOCAL_FRAME_VERSION,
    CANON_GEO_PROVIDER_TILE_BUILD_VERSION, GeoAffineProjectionMm, GeoArtifactFieldClassification,
    GeoArtifactFieldLicenseClass, GeoGeometryErrorCode, GeoGeometryFeatureInput,
    GeoGeometryTileRequest, GeoLicenseClass, GeoLocalFrameContract, GeoProjectionProvenance,
    GeoProviderGeometryFidelity, GeoProviderGeometryTileBuildRequest, GeoProviderTileCoverageState,
    GeoProviderTileFeatureContract, GeoProviderTileFeatureProvenance, GeoProviderTileFieldLocator,
    GeoProviderTileLicensePosture, GeoProviderTileRedactionClass, GeoProviderTileSource,
    GeoProviderTileSourceCoverage, GeoProviderTileSourceProvenance, GeoProviderTileSubsetKind,
    GeoProviderTileSubsetPredicate, GeoRedactionDefaultAction, GeoSourceAxisDomain,
    GeoSourceGeometry, GeoSourcePointDecimal, GeoSourcePointFixed,
    canonical_redacted_artifact_bytes, geo_redacted_artifact_content_blake3,
    materialize_provider_geometry_tile, redact_geo_artifact, redact_geometry_tile_artifact,
    validate_redacted_artifact,
};
use serde_json::{Value, json};

#[test]
fn generic_redaction_requires_geometry_field_classification() {
    let artifact = decision_fixture();
    let error = redact_geo_artifact(
        "canon_geo_fixture_decision.v0",
        &artifact,
        &decision_fixture_classifications(false),
    )
    .expect_err("unclassified geometry-bearing field must refuse");

    assert_eq!(error.code, GeoGeometryErrorCode::InvalidLicensePosture);
    assert_eq!(
        error.detail.get("field_path").map(String::as_str),
        Some("$.candidates[0].licensed_geometry")
    );
}

#[test]
fn generic_redaction_strips_planted_coordinates_and_preserves_decision_replay() {
    let artifact = decision_fixture();
    let redacted = redact_geo_artifact(
        "canon_geo_fixture_decision.v0",
        &artifact,
        &decision_fixture_classifications(true),
    )
    .expect("redaction succeeds");

    validate_redacted_artifact(&redacted).expect("redacted artifact validates");
    assert!(redacted.redacted);
    assert_eq!(
        redacted.egress_policy.default_action,
        GeoRedactionDefaultAction::ShareRedactedProjection
    );
    assert_eq!(
        redacted.artifact.pointer("/decision"),
        artifact.pointer("/decision"),
        "decision subtree must replay exactly"
    );
    assert_eq!(
        redacted.artifact.pointer("/denominator"),
        artifact.pointer("/denominator"),
        "denominator subtree must survive redaction"
    );
    assert_eq!(
        redacted
            .artifact
            .pointer("/candidates/0/licensed_geometry")
            .and_then(Value::as_str),
        Some("[REDACTED]")
    );

    let canonical = canonical_redacted_artifact_bytes(&redacted).expect("canonical bytes");
    let text = String::from_utf8(canonical).expect("canonical JSON is UTF-8");
    assert!(!text.contains("123456789"));
    assert!(!text.contains("987654321"));
    assert!(text.contains("candidate_count"));
    assert!(text.contains("parcel-client-1"));
    assert!(text.contains("blake3:"));
}

#[test]
fn redacted_artifact_hash_is_deterministic_under_shuffled_classifications() {
    let artifact = decision_fixture();
    let ordered = decision_fixture_classifications(true);
    let mut shuffled = ordered.clone();
    shuffled.reverse();

    let first = redact_geo_artifact("canon_geo_fixture_decision.v0", &artifact, &ordered)
        .expect("ordered classifications redact");
    let second = redact_geo_artifact("canon_geo_fixture_decision.v0", &artifact, &shuffled)
        .expect("shuffled classifications redact");

    assert_eq!(
        canonical_redacted_artifact_bytes(&first).expect("first canonical"),
        canonical_redacted_artifact_bytes(&second).expect("second canonical")
    );
    assert_eq!(
        first.redacted_artifact_blake3,
        geo_redacted_artifact_content_blake3(&second).expect("second digest")
    );
}

#[test]
fn provider_geometry_tile_redaction_uses_client_license_posture_as_default_deny() {
    let tile = materialize_provider_geometry_tile(&provider_tile_build_request())
        .expect("provider tile materializes");
    let redacted = redact_geometry_tile_artifact(&tile).expect("provider tile redacts");
    let canonical = canonical_redacted_artifact_bytes(&redacted).expect("canonical bytes");
    let text = String::from_utf8(canonical).expect("canonical JSON is UTF-8");

    assert!(redacted.redacted);
    assert_eq!(
        redacted.egress_policy.default_action,
        GeoRedactionDefaultAction::ShareRedactedProjection
    );
    assert!(
        redacted
            .egress_policy
            .full_artifact_requires_explicit_operator_action
    );
    assert_eq!(
        redacted.egress_policy.client_restricted_source_ids,
        vec!["source.client.parcels".to_string()]
    );
    assert_eq!(
        redacted.egress_policy.attribution_requirements,
        vec!["ORNL and FEMA Geospatial Response Office".to_string()]
    );
    assert!(text.contains(&tile.provider_tile.as_ref().unwrap().tile_content_blake3));
    assert!(text.contains("building-1"));
    assert!(text.contains("parcel-1"));
    assert!(text.contains("[REDACTED]"));
    assert!(
        !text.contains("123456789"),
        "redacted artifact must not leak planted licensed x coordinate"
    );
    assert!(
        !text.contains("987654321"),
        "redacted artifact must not leak planted licensed y coordinate"
    );
    assert_eq!(tile.total_canonical_vertices, 8);
}

fn decision_fixture() -> Value {
    json!({
        "version": "canon_geo_fixture_decision.v0",
        "tile_id": "892a100d26bffff",
        "decision": {
            "winner_id": "building-1",
            "abstention_state": "accepted",
            "score_basis_points": 9975,
            "reason_code": "contains_point_and_unique_owner"
        },
        "denominator": {
            "candidate_count": 2,
            "score_denominator_basis_points": 10000
        },
        "candidates": [
            {
                "feature_id": "parcel-client-1",
                "source_instance_id": "source.client.parcels",
                "licensed_geometry": {
                    "coordinates": [123456789, 987654321]
                }
            },
            {
                "feature_id": "building-1",
                "source_instance_id": "source.fema.structures",
                "score_basis_points": 9975
            }
        ]
    })
}

fn decision_fixture_classifications(include_geometry: bool) -> Vec<GeoArtifactFieldClassification> {
    let mut classifications = vec![
        classification(
            "$.tile_id",
            GeoArtifactFieldLicenseClass::Identifier,
            None,
            false,
            "tile identifier",
        ),
        classification(
            "$.decision",
            GeoArtifactFieldLicenseClass::Public,
            None,
            false,
            "decision result and reason code",
        ),
        classification(
            "$.denominator",
            GeoArtifactFieldLicenseClass::DerivedMeasure,
            None,
            false,
            "candidate and score denominators",
        ),
        classification(
            "$.candidates[0].feature_id",
            GeoArtifactFieldLicenseClass::Identifier,
            Some("source.client.parcels"),
            false,
            "candidate identifier needed for replay",
        ),
        classification(
            "$.candidates[0].source_instance_id",
            GeoArtifactFieldLicenseClass::Identifier,
            Some("source.client.parcels"),
            false,
            "source identifier needed for replay",
        ),
        classification(
            "$.candidates[1].feature_id",
            GeoArtifactFieldLicenseClass::Identifier,
            Some("source.fema.structures"),
            false,
            "candidate identifier needed for replay",
        ),
    ];
    if include_geometry {
        classifications.push(classification(
            "$.candidates[0].licensed_geometry",
            GeoArtifactFieldLicenseClass::LicensedGeometry,
            Some("source.client.parcels"),
            true,
            "client parcel geometry is licensed and reconstructive",
        ));
    }
    classifications
}

fn provider_tile_build_request() -> GeoProviderGeometryTileBuildRequest {
    let mut geometry_request = GeoGeometryTileRequest {
        version: CANON_GEO_GEOMETRY_REQUEST_VERSION.to_string(),
        frame: frame(),
        features: vec![
            GeoGeometryFeatureInput {
                feature_id: "parcel-1".to_string(),
                source_crs: "LOCAL:TEST-METRES".to_string(),
                geometry: polygon(rectangle(
                    "123456.789",
                    "987654.321",
                    "123466.789",
                    "987664.321",
                    false,
                )),
            },
            GeoGeometryFeatureInput {
                feature_id: "building-1".to_string(),
                source_crs: "LOCAL:TEST-METRES".to_string(),
                geometry: polygon(rectangle("2", "2", "4", "4", false)),
            },
        ],
        max_vertices_per_geometry: 64,
        max_geometry_bytes_per_tile: 100_000,
    };
    geometry_request
        .features
        .sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
    let fema_source_digest = blake3_hex(b"fema-source-tile");
    let client_source_digest = blake3_hex(b"client-parcel-layer");

    GeoProviderGeometryTileBuildRequest {
        version: CANON_GEO_PROVIDER_TILE_BUILD_VERSION.to_string(),
        tile_id: "892a100d26bffff".to_string(),
        geometry_request,
        subset: GeoProviderTileSubsetPredicate {
            kind: GeoProviderTileSubsetKind::H3CellSetAndSourceCoverageIntersection,
            predicate_id: "tile-892a100d26bffff-k0-byop-redaction".to_string(),
            h3_resolution: 9,
            center_cell: "892a100d26bffff".to_string(),
            halo_k: 0,
            work_cells: vec!["892a100d26bffff".to_string()],
            source_coverages: vec![
                GeoProviderTileSourceCoverage {
                    source_instance_id: "source.client.parcels".to_string(),
                    h3_cell: "892a100d26bffff".to_string(),
                    coverage_state: GeoProviderTileCoverageState::Partial,
                },
                GeoProviderTileSourceCoverage {
                    source_instance_id: "source.fema.structures".to_string(),
                    h3_cell: "892a100d26bffff".to_string(),
                    coverage_state: GeoProviderTileCoverageState::Complete,
                },
            ],
        },
        sources: vec![
            GeoProviderTileSource {
                source_instance_id: "source.client.parcels".to_string(),
                release_id: "client-parcels-2026-q3".to_string(),
                release_digest: blake3_hex(b"client-parcels-release"),
                license_class: GeoLicenseClass::RestrictedLocalOnly,
                license_expression: "LicenseRef-Client-Parcel-Local".to_string(),
                attribution_required: false,
                attribution_text: None,
                provenance: GeoProviderTileSourceProvenance::ClientDeclared {
                    vendor: "client-declared-parcel-vendor".to_string(),
                    vintage: "2026-Q3".to_string(),
                    source_crs: "LOCAL:TEST-METRES".to_string(),
                    coverage_extent: "h3:892a100d26bffff".to_string(),
                    mutual_exclusivity_declared: false,
                },
            },
            GeoProviderTileSource {
                source_instance_id: "source.fema.structures".to_string(),
                release_id: "fema-usa-structures-2026-fixture".to_string(),
                release_digest: blake3_hex(b"fema-release"),
                license_class: GeoLicenseClass::PublicAttributionRequired,
                license_expression: "CC-BY-4.0".to_string(),
                attribution_required: true,
                attribution_text: Some("ORNL and FEMA Geospatial Response Office".to_string()),
                provenance: GeoProviderTileSourceProvenance::CanonFullProvenance {
                    source_path: "tiles/fema/892a100d26bffff.jsonl".to_string(),
                    source_digest: fema_source_digest.clone(),
                    source_record_count: 1,
                },
            },
        ],
        license_posture: GeoProviderTileLicensePosture {
            posture_id: "mixed-byop-local-v0".to_string(),
            output_license_expression: "LicenseRef-Mixed-BYOP-Local".to_string(),
            redistribution_notice:
                "Contains client-declared parcel geometry; raw tile stays local.".to_string(),
            attribution_requirements: vec!["ORNL and FEMA Geospatial Response Office".to_string()],
            client_restricted_source_ids: vec!["source.client.parcels".to_string()],
        },
        feature_contracts: vec![
            GeoProviderTileFeatureContract {
                feature_id: "parcel-1".to_string(),
                source_instance_id: "source.client.parcels".to_string(),
                source_feature_id: "client-parcel-1".to_string(),
                decision_geometry_fidelity: GeoProviderGeometryFidelity::SourceFidelity,
                display_geometry_fidelity: Some(GeoProviderGeometryFidelity::DisplaySimplified),
                license_class: GeoLicenseClass::RestrictedLocalOnly,
                redaction_class: GeoProviderTileRedactionClass::LocalOnly,
                provenance: GeoProviderTileFeatureProvenance {
                    source_instance_id: "source.client.parcels".to_string(),
                    source_path: "client/parcels.gpkg".to_string(),
                    source_digest: client_source_digest,
                    source_record_id: "client-parcel-row-1".to_string(),
                    record_ordinal: 3,
                    field_locators: vec![GeoProviderTileFieldLocator {
                        field_path: "$.geometry".to_string(),
                        source_path: "client/parcels.gpkg".to_string(),
                        record_ordinal: 3,
                    }],
                },
            },
            GeoProviderTileFeatureContract {
                feature_id: "building-1".to_string(),
                source_instance_id: "source.fema.structures".to_string(),
                source_feature_id: "fema-building-1".to_string(),
                decision_geometry_fidelity: GeoProviderGeometryFidelity::SourceFidelity,
                display_geometry_fidelity: Some(GeoProviderGeometryFidelity::DisplaySimplified),
                license_class: GeoLicenseClass::PublicAttributionRequired,
                redaction_class: GeoProviderTileRedactionClass::ShareableAttributionRequired,
                provenance: GeoProviderTileFeatureProvenance {
                    source_instance_id: "source.fema.structures".to_string(),
                    source_path: "tiles/fema/892a100d26bffff.jsonl".to_string(),
                    source_digest: fema_source_digest,
                    source_record_id: "fema-row-1".to_string(),
                    record_ordinal: 7,
                    field_locators: vec![GeoProviderTileFieldLocator {
                        field_path: "$.geometry".to_string(),
                        source_path: "tiles/fema/892a100d26bffff.jsonl".to_string(),
                        record_ordinal: 7,
                    }],
                },
            },
        ],
        allow_vendor_simplified_decision_geometry: false,
    }
}

fn frame() -> GeoLocalFrameContract {
    GeoLocalFrameContract {
        version: CANON_GEO_LOCAL_FRAME_VERSION.to_string(),
        frame_id: "tile:892a100d26bffff:local-mm:redaction:v0".to_string(),
        tile_id: "892a100d26bffff".to_string(),
        source_crs: "LOCAL:TEST-METRES".to_string(),
        source_axis_domain: GeoSourceAxisDomain::Planar,
        source_decimal_places: 3,
        source_origin: GeoSourcePointFixed { x: 0, y: 0 },
        affine: GeoAffineProjectionMm {
            x_from_source_x_numerator: 1,
            x_from_source_y_numerator: 0,
            y_from_source_x_numerator: 0,
            y_from_source_y_numerator: 1,
            denominator: 1,
        },
        projection: GeoProjectionProvenance {
            method_id: "test-fixed-affine".to_string(),
            method_version: "1.0.0".to_string(),
            parameters_blake3: blake3_hex(b"test-fixed-affine-v1"),
            max_projection_error_micrometres: 0,
        },
        max_abs_coordinate_mm: 2_000_000_000,
    }
}

fn classification(
    field_path: &str,
    license_class: GeoArtifactFieldLicenseClass,
    source_instance_id: Option<&str>,
    reconstructive: bool,
    rationale: &str,
) -> GeoArtifactFieldClassification {
    GeoArtifactFieldClassification {
        field_path: field_path.to_string(),
        license_class,
        source_instance_id: source_instance_id.map(str::to_string),
        reconstructive,
        rationale: rationale.to_string(),
    }
}

fn polygon(exterior: Vec<GeoSourcePointDecimal>) -> GeoSourceGeometry {
    GeoSourceGeometry::Polygon {
        exterior,
        holes: Vec::new(),
    }
}

fn rectangle(
    min_x: &str,
    min_y: &str,
    max_x: &str,
    max_y: &str,
    clockwise: bool,
) -> Vec<GeoSourcePointDecimal> {
    let mut points = vec![
        point(min_x, min_y),
        point(max_x, min_y),
        point(max_x, max_y),
        point(min_x, max_y),
        point(min_x, min_y),
    ];
    if clockwise {
        points.reverse();
    }
    points
}

fn point(x: &str, y: &str) -> GeoSourcePointDecimal {
    GeoSourcePointDecimal {
        x: x.to_string(),
        y: y.to_string(),
    }
}

fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
