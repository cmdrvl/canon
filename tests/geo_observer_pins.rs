use canon::geo::{
    CANON_GEO_ERROR_POPULATION_VERSION, GeoErrorPopulationArtifact, GeoErrorPopulationSubject,
    GeoImageTilePin, GeoObserverErrorCode, GeoTruthPlane, canonical_error_population_bytes,
    select_error_population_subjects, truth_plane_key, validate_error_population_artifact,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const SOURCE_PROFILE: &str = include_str!("../scripts/geo_observer/sources/nyc_ortho.json");
const LICENSE_BYTES: &[u8] = include_bytes!("../scripts/geo_observer/licenses/cc_by_4_0.txt");
const PIN_2024: &str = include_str!("../scripts/geo_observer/pins/nyc_ortho_2024.pins.json");
const PIN_2022: &str = include_str!("../scripts/geo_observer/pins/nyc_ortho_2022.pins.json");
const POPULATION: &str =
    include_str!("../scripts/geo_observer/populations/nyc_h7_observer_population.v0.json");
const SOURCE_POPULATION: &[u8] =
    include_bytes!("../scripts/geo_observer/populations/nyc_h7_observer_source_population.v0.json");
const SOURCE_POPULATION_JSON: &str =
    include_str!("../scripts/geo_observer/populations/nyc_h7_observer_source_population.v0.json");
const POPULATION_SQL: &[u8] =
    include_bytes!("../scripts/geo_observer/populations/nyc_h7_observer_population.sql");

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PinManifest {
    version: String,
    source_profile_id: String,
    rows: Vec<GeoImageTilePin>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceProfile {
    source_profile_id: String,
    source_dataset_prefix: String,
    license: SourceLicense,
    allowed_hosts: Vec<String>,
    forbidden_host_substrings: Vec<String>,
    range_requests_supported: bool,
    vintages: Vec<SourceVintage>,
    default_tiles: Vec<SourceTile>,
    source_name: String,
    tile_path_style: String,
    acquisition_boundary: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceLicense {
    license_id: String,
    license_text_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceVintage {
    year: u16,
    source_dataset: String,
    service_url: String,
    tile_url_template: String,
    flight_start_day: i64,
    flight_end_day: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceTile {
    window_id: String,
    z: u8,
    x: u32,
    y: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePopulation {
    artifact_kind: String,
    population_id: String,
    region: String,
    source_query_path: String,
    source_query_blake3: String,
    selection_frame: String,
    source_pins: SourcePins,
    h7_accepted_multi_bbl_counts: BTreeMap<String, u64>,
    complete_window_counts: BTreeMap<String, u64>,
    excluded_window_counts: BTreeMap<String, u64>,
    subjects: Vec<SourceSubject>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePins {
    loan_issuance_property_build_id: String,
    acris_release_dt: String,
    mappluto_release: String,
    mappluto_variant: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceSubject {
    subject_id: String,
    truth_plane: GeoTruthPlane,
    document_id: String,
    bridge_property_keys: u64,
    parcel_ids: Vec<String>,
    window_blake3: String,
    window: SourceWindow,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceWindow {
    encoding: String,
    mappluto_release: String,
    mappluto_variant: String,
    bbox_crs: String,
    bbox_wgs84_e7: SourceBbox,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceBbox {
    xmin_e7: i64,
    ymin_e7: i64,
    xmax_e7: i64,
    ymax_e7: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PinValidationError {
    code: GeoObserverErrorCode,
    field: &'static str,
    row_index: usize,
    expected: Option<String>,
    actual: Option<String>,
}

fn parse_profile() -> SourceProfile {
    let profile: SourceProfile =
        serde_json::from_str(SOURCE_PROFILE).expect("source profile parses");
    assert!(!profile.source_name.is_empty());
    assert_eq!(profile.tile_path_style, "arcgis_mapserver_z_y_x");
    assert!(
        profile
            .license
            .license_text_path
            .ends_with("scripts/geo_observer/licenses/cc_by_4_0.txt")
    );
    assert!(
        profile.acquisition_boundary.contains("outside Canon"),
        "source profile must keep network acquisition outside Canon runtime"
    );
    assert!(!profile.default_tiles.is_empty());
    for tile in &profile.default_tiles {
        assert!(!tile.window_id.is_empty());
        assert!(tile.z > 0);
        assert!(tile.x > 0);
        assert!(tile.y > 0);
    }
    for vintage in &profile.vintages {
        assert!(
            vintage.service_url.contains(&vintage.year.to_string()),
            "service URL must name the profile vintage"
        );
        assert!(
            vintage.tile_url_template.contains("{z}")
                && vintage.tile_url_template.contains("{x}")
                && vintage.tile_url_template.contains("{y}")
        );
    }
    profile
}

fn pin_manifests() -> Vec<PinManifest> {
    [PIN_2024, PIN_2022]
        .into_iter()
        .map(|source| serde_json::from_str(source).expect("pin manifest parses"))
        .collect()
}

fn parse_population() -> GeoErrorPopulationArtifact {
    serde_json::from_str(POPULATION).expect("population artifact parses")
}

fn parse_source_population() -> SourcePopulation {
    serde_json::from_str(SOURCE_POPULATION_JSON).expect("source population artifact parses")
}

fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn url_host(url: &str) -> Option<&str> {
    let (_, rest) = url.split_once("://")?;
    rest.split('/').next()
}

fn validate_pin_manifest_rows(
    manifest: &PinManifest,
    profile: &SourceProfile,
    license_text_blake3: &str,
    forbidden_license_ids: &BTreeSet<&str>,
) -> Result<(), PinValidationError> {
    if manifest.version != "canon_geo_image_tile_pin.v0" {
        return Err(PinValidationError {
            code: GeoObserverErrorCode::InvalidInput,
            field: "version",
            row_index: 0,
            expected: Some("canon_geo_image_tile_pin.v0".to_string()),
            actual: Some(manifest.version.clone()),
        });
    }
    if manifest.source_profile_id != profile.source_profile_id {
        return Err(PinValidationError {
            code: GeoObserverErrorCode::InvalidInput,
            field: "source_profile_id",
            row_index: 0,
            expected: Some(profile.source_profile_id.clone()),
            actual: Some(manifest.source_profile_id.clone()),
        });
    }
    if manifest.rows.is_empty() {
        return Err(PinValidationError {
            code: GeoObserverErrorCode::InvalidInput,
            field: "rows",
            row_index: 0,
            expected: Some("at least one row".to_string()),
            actual: Some("0".to_string()),
        });
    }
    for (row_index, row) in manifest.rows.iter().enumerate() {
        if forbidden_license_ids.contains(row.license_id.as_str()) {
            return Err(PinValidationError {
                code: GeoObserverErrorCode::ObserverLicenseForbidden,
                field: "license_id",
                row_index,
                expected: None,
                actual: Some(row.license_id.clone()),
            });
        }
        for (field, value) in [
            ("url", row.url.as_str()),
            ("blake3", row.blake3.as_str()),
            ("license_id", row.license_id.as_str()),
            ("license_text_blake3", row.license_text_blake3.as_str()),
            ("source_dataset", row.source_dataset.as_str()),
        ] {
            if value.is_empty() || value.trim() != value {
                return Err(PinValidationError {
                    code: GeoObserverErrorCode::InvalidInput,
                    field,
                    row_index,
                    expected: Some("non-empty canonical string".to_string()),
                    actual: Some(value.to_string()),
                });
            }
        }
        if row.license_id != profile.license.license_id {
            return Err(PinValidationError {
                code: GeoObserverErrorCode::InvalidInput,
                field: "license_id",
                row_index,
                expected: Some(profile.license.license_id.clone()),
                actual: Some(row.license_id.clone()),
            });
        }
        if row.license_text_blake3 != license_text_blake3 {
            return Err(PinValidationError {
                code: GeoObserverErrorCode::InvalidInput,
                field: "license_text_blake3",
                row_index,
                expected: Some(license_text_blake3.to_string()),
                actual: Some(row.license_text_blake3.clone()),
            });
        }
        if !row
            .source_dataset
            .starts_with(&profile.source_dataset_prefix)
            || row.source_dataset.starts_with("fixture.")
        {
            return Err(PinValidationError {
                code: GeoObserverErrorCode::InvalidInput,
                field: "source_dataset",
                row_index,
                expected: Some(format!("{}<vintage>", profile.source_dataset_prefix)),
                actual: Some(row.source_dataset.clone()),
            });
        }
        let vintage = profile
            .vintages
            .iter()
            .find(|vintage| vintage.source_dataset == row.source_dataset)
            .expect("manifest row vintage is listed in source profile");
        if row.vintage.start_day > row.vintage.end_day
            || row.vintage.start_day != vintage.flight_start_day
            || row.vintage.end_day != vintage.flight_end_day
        {
            return Err(PinValidationError {
                code: GeoObserverErrorCode::InvalidInput,
                field: "vintage",
                row_index,
                expected: Some(format!(
                    "{}..{}",
                    vintage.flight_start_day, vintage.flight_end_day
                )),
                actual: Some(format!(
                    "{}..{}",
                    row.vintage.start_day, row.vintage.end_day
                )),
            });
        }
        let Some(host) = url_host(&row.url) else {
            return Err(PinValidationError {
                code: GeoObserverErrorCode::InvalidInput,
                field: "url",
                row_index,
                expected: Some("absolute URL".to_string()),
                actual: Some(row.url.clone()),
            });
        };
        if !profile.allowed_hosts.iter().any(|allowed| allowed == host) {
            return Err(PinValidationError {
                code: GeoObserverErrorCode::InvalidInput,
                field: "url",
                row_index,
                expected: Some(format!("one of {:?}", profile.allowed_hosts)),
                actual: Some(row.url.clone()),
            });
        }
        let host_lower = host.to_ascii_lowercase();
        if profile
            .forbidden_host_substrings
            .iter()
            .any(|forbidden| host_lower.contains(forbidden))
        {
            return Err(PinValidationError {
                code: GeoObserverErrorCode::ObserverLicenseForbidden,
                field: "url",
                row_index,
                expected: Some("non-commercial-basemap host".to_string()),
                actual: Some(row.url.clone()),
            });
        }
        if profile.range_requests_supported {
            if row.byte_range.is_none() || row.etag.as_deref().unwrap_or_default().is_empty() {
                return Err(PinValidationError {
                    code: GeoObserverErrorCode::InvalidInput,
                    field: "byte_range",
                    row_index,
                    expected: Some("byte_range and etag".to_string()),
                    actual: Some(
                        json!({"byte_range": row.byte_range, "etag": row.etag}).to_string(),
                    ),
                });
            }
        } else if row.byte_range.is_some() {
            return Err(PinValidationError {
                code: GeoObserverErrorCode::InvalidInput,
                field: "byte_range",
                row_index,
                expected: Some("null when ranges are unsupported".to_string()),
                actual: Some(format!("{:?}", row.byte_range)),
            });
        }
    }
    Ok(())
}

#[test]
fn license_text_digest_matches_every_pin_row() {
    let profile = parse_profile();
    let license_text_blake3 = blake3_hex(LICENSE_BYTES);
    let manifests = pin_manifests();
    let forbidden_license_ids = BTreeSet::from(["commercial_basemap_tos"]);
    let row_count = manifests
        .iter()
        .map(|manifest| manifest.rows.len())
        .sum::<usize>();

    for manifest in &manifests {
        validate_pin_manifest_rows(
            manifest,
            &profile,
            &license_text_blake3,
            &forbidden_license_ids,
        )
        .unwrap_or_else(|error| {
            panic!(
                "pin row failed validation: row={} field={} code={:?} expected={:?} actual={:?}",
                error.row_index, error.field, error.code, error.expected, error.actual
            )
        });
    }
    eprintln!(
        "license_text_blake3={} license_file_len={} pin_manifest_rows={}",
        license_text_blake3,
        LICENSE_BYTES.len(),
        row_count
    );

    let mut forbidden = manifests[0].clone();
    forbidden.rows[0].license_id = "commercial_basemap_tos".to_string();
    let error = validate_pin_manifest_rows(
        &forbidden,
        &profile,
        &license_text_blake3,
        &forbidden_license_ids,
    )
    .expect_err("commercial basemap license ids refuse");
    assert_eq!(error.code, GeoObserverErrorCode::ObserverLicenseForbidden);
    assert_eq!(error.field, "license_id");

    let mut altered_license = LICENSE_BYTES.to_vec();
    altered_license[0] ^= 1;
    let altered_license_blake3 = blake3_hex(&altered_license);
    let error = validate_pin_manifest_rows(
        &manifests[0],
        &profile,
        &altered_license_blake3,
        &forbidden_license_ids,
    )
    .expect_err("one-byte license drift changes the digest");
    assert_eq!(error.field, "license_text_blake3");
    assert_eq!(
        error.expected.as_deref(),
        Some(altered_license_blake3.as_str())
    );
    assert_eq!(error.actual.as_deref(), Some(license_text_blake3.as_str()));

    let mut fixture_prefixed = manifests[0].clone();
    fixture_prefixed.rows[0].source_dataset = "fixture.nyc_ortho.2024".to_string();
    let error = validate_pin_manifest_rows(
        &fixture_prefixed,
        &profile,
        &license_text_blake3,
        &forbidden_license_ids,
    )
    .expect_err("fixture source_dataset prefixes are not live retained NYC pins");
    assert_eq!(error.field, "source_dataset");
}

#[test]
fn error_population_artifact_validates_and_rejects_mutated_inputs() {
    let population = parse_population();
    validate_error_population_artifact(&population).unwrap_or_else(|error| {
        panic!(
            "population_id={} selection_seed={:?} subjects={} stratum_counts={:?} validation_error={:?}",
            population.population_id,
            population.selection_seed,
            population.subjects.len(),
            population.stratum_counts,
            error
        )
    });
    assert_eq!(population.version, CANON_GEO_ERROR_POPULATION_VERSION);
    assert!(population.subjects.len() >= 40);
    assert_eq!(
        population.stratum_counts.get(truth_plane_key(
            GeoTruthPlane::NonRoundAmountDateLegalBorough
        )),
        Some(&20)
    );
    assert_eq!(
        population
            .stratum_counts
            .get(truth_plane_key(GeoTruthPlane::RoundExactLenderParty)),
        Some(&20)
    );

    let first_bytes = canonical_error_population_bytes(&population).expect("canonical bytes");
    let second_bytes = canonical_error_population_bytes(&population).expect("canonical bytes");
    assert_eq!(first_bytes, second_bytes);
    eprintln!(
        "population_id={} selection_seed={:?} subjects={} canonical_blake3={}",
        population.population_id,
        population.selection_seed,
        population.subjects.len(),
        blake3_hex(&first_bytes)
    );

    let mut unsorted = population.clone();
    unsorted.subjects.swap(0, 1);
    let error = validate_error_population_artifact(&unsorted)
        .expect_err("unsorted subjects refuse invalid_input");
    assert_eq!(error.code, GeoObserverErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("subjects")
    );

    let mut duplicate = population.clone();
    duplicate.subjects[1].subject_id = duplicate.subjects[0].subject_id.clone();
    let error = validate_error_population_artifact(&duplicate)
        .expect_err("duplicate subjects refuse invalid_input");
    assert_eq!(error.code, GeoObserverErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("subjects")
    );

    let mut missing_seed: Value =
        serde_json::from_str(POPULATION).expect("population value parses");
    missing_seed
        .as_object_mut()
        .expect("population is object")
        .remove("selection_seed");
    let missing_seed: GeoErrorPopulationArtifact =
        serde_json::from_value(missing_seed).expect("missing seed still deserializes");
    let error = validate_error_population_artifact(&missing_seed)
        .expect_err("missing selection_seed refuses invalid_input");
    assert_eq!(error.code, GeoObserverErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("selection_seed")
    );

    let mut missing_query: Value =
        serde_json::from_str(POPULATION).expect("population value parses");
    missing_query
        .as_object_mut()
        .expect("population is object")
        .remove("selection_query_blake3");
    let missing_query: GeoErrorPopulationArtifact =
        serde_json::from_value(missing_query).expect("missing query still deserializes");
    let error = validate_error_population_artifact(&missing_query)
        .expect_err("missing selection_query_blake3 refuses invalid_input");
    assert_eq!(error.code, GeoObserverErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("selection_query_blake3")
    );
}

#[test]
fn seeded_selection_replays_from_source_population() {
    let population = parse_population();
    let source = parse_source_population();
    let source_population_blake3 = blake3_hex(SOURCE_POPULATION);
    let source_sql_blake3 = blake3_hex(POPULATION_SQL);
    let source_subjects = source_subjects_to_population_subjects(&source.subjects);
    let requested_counts = requested_counts(&population);

    assert_eq!(
        population.source_population_blake3.as_deref(),
        Some(source_population_blake3.as_str())
    );
    assert_eq!(
        population.selection_query_blake3.as_deref(),
        Some(source_sql_blake3.as_str())
    );
    assert_eq!(source.source_query_blake3, source_sql_blake3);
    assert_eq!(source.artifact_kind, "nyc_h7_observer_source_population.v0");
    assert_eq!(source.population_id, "nyc.h7.observer.source.2026-09");
    assert_eq!(source.region, "nyc");
    assert_eq!(
        source.source_query_path,
        "scripts/geo_observer/populations/nyc_h7_observer_population.sql"
    );
    assert_eq!(
        source.selection_frame,
        "h7_accepted_multi_bbl_with_complete_26v2_mappluto_window"
    );
    assert_eq!(
        source.source_pins.loan_issuance_property_build_id,
        "d5ddd2d9-07dc-44d6-bf8b-b7bfc373dbc3"
    );
    assert_eq!(source.source_pins.acris_release_dt, "2026-08-10");
    assert_eq!(source.source_pins.mappluto_release, "26v2");
    assert_eq!(source.source_pins.mappluto_variant, "shoreline_clipped");
    assert_eq!(
        source.h7_accepted_multi_bbl_counts.get(truth_plane_key(
            GeoTruthPlane::NonRoundAmountDateLegalBorough
        )),
        Some(&35)
    );
    assert_eq!(
        source
            .h7_accepted_multi_bbl_counts
            .get(truth_plane_key(GeoTruthPlane::RoundExactLenderParty)),
        Some(&36)
    );
    assert_eq!(
        source.complete_window_counts.get(truth_plane_key(
            GeoTruthPlane::NonRoundAmountDateLegalBorough
        )),
        Some(&25)
    );
    assert_eq!(
        source
            .complete_window_counts
            .get(truth_plane_key(GeoTruthPlane::RoundExactLenderParty)),
        Some(&30)
    );
    assert_eq!(
        source
            .excluded_window_counts
            .get("missing_or_partial_26v2_mappluto_window"),
        Some(&16)
    );

    for subject in &source.subjects {
        assert!(!subject.document_id.is_empty());
        assert!(subject.bridge_property_keys > 0);
        assert_eq!(
            subject.window.encoding,
            "canon_geo_observer_window.v0.line_digest"
        );
        assert_eq!(
            subject.window.mappluto_release,
            source.source_pins.mappluto_release
        );
        assert_eq!(
            subject.window.mappluto_variant,
            source.source_pins.mappluto_variant
        );
        assert_eq!(subject.window.bbox_crs, "EPSG:4326");
        assert_eq!(
            subject.window_blake3,
            source_window_blake3(subject),
            "source window digest mismatch for {}",
            subject.subject_id
        );
    }

    let selected = select_error_population_subjects(
        &source_subjects,
        population.selection_seed.expect("selection seed present"),
        &requested_counts,
    )
    .expect("recorded seed selects from source population");
    assert_eq!(selected, population.subjects);

    let different_seed_selected = select_error_population_subjects(
        &source_subjects,
        population.selection_seed.expect("selection seed present") + 1,
        &requested_counts,
    )
    .expect("different seed still selects");
    assert_ne!(
        different_seed_selected, population.subjects,
        "selection_seed must be load-bearing"
    );
    eprintln!(
        "seed={} source_population_blake3={} first_subject={} seed_plus_one_first_subject={} selected_len={}",
        population.selection_seed.expect("selection seed present"),
        source_population_blake3,
        population
            .subjects
            .first()
            .map(|subject| subject.subject_id.as_str())
            .unwrap_or("<none>"),
        different_seed_selected
            .first()
            .map(|subject| subject.subject_id.as_str())
            .unwrap_or("<none>"),
        selected.len()
    );
}

fn requested_counts(population: &GeoErrorPopulationArtifact) -> BTreeMap<GeoTruthPlane, usize> {
    population
        .stratum_counts
        .iter()
        .map(|(key, count)| {
            (
                match key.as_str() {
                    "non_round_amount_date_legal_borough" => {
                        GeoTruthPlane::NonRoundAmountDateLegalBorough
                    }
                    "round_exact_lender_party" => GeoTruthPlane::RoundExactLenderParty,
                    other => panic!("unexpected truth-plane stratum {other}"),
                },
                usize::try_from(*count).expect("count fits usize"),
            )
        })
        .collect()
}

fn source_subjects_to_population_subjects(
    source_subjects: &[SourceSubject],
) -> Vec<GeoErrorPopulationSubject> {
    source_subjects
        .iter()
        .map(|subject| GeoErrorPopulationSubject {
            subject_id: subject.subject_id.clone(),
            truth_plane: subject.truth_plane,
            window_blake3: subject.window_blake3.clone(),
            parcel_ids: subject.parcel_ids.clone(),
        })
        .collect()
}

fn source_window_blake3(subject: &SourceSubject) -> String {
    blake3_hex(source_window_payload(subject).as_bytes())
}

fn source_window_payload(subject: &SourceSubject) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        subject.subject_id,
        truth_plane_key(subject.truth_plane),
        subject.window.mappluto_release,
        subject.window.mappluto_variant,
        subject.window.bbox_crs,
        subject.window.bbox_wgs84_e7.xmin_e7,
        subject.window.bbox_wgs84_e7.ymin_e7,
        subject.window.bbox_wgs84_e7.xmax_e7,
        subject.window.bbox_wgs84_e7.ymax_e7
    )
}
