use canon::geo::{
    CANON_GEO_ADJUDICATION_RECEIPT_VERSION, CANON_GEO_ADJUDICATION_REQUEST_VERSION,
    GeoAdjudicationErrorCode, GeoAdjudicationInvalidLabelReason, GeoAdjudicationLabel,
    GeoAdjudicationReceipt, GeoAdjudicationRequest, GeoAdjudicationRetainedLabelRow,
    GeoCanonicalPolygonMm, GeoCanonicalRingMm, GeoImageTilePin, GeoPointMm, GeoTruthPlane,
    GeoValidTimeInterval, adjudication_request_blake3, build_adjudication_requests,
    canonical_adjudication_receipt_bytes, canonical_adjudication_request_bytes,
    revalidate_adjudication_labels, validate_adjudication_receipt,
    validate_adjudication_request_artifact,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const CROP_BYTES: &[u8] = b"retained adjudication crop bytes";
const D0_LABELS_JSON: &str =
    include_str!("../scripts/geo_measurements/fixtures/d0_adjudication/labels.json");
const D0_PINS_JSON: &str =
    include_str!("../scripts/geo_measurements/fixtures/d0_adjudication/pins.json");

#[test]
fn t22_adjudication_receipt_accepts_selected_candidate_with_matching_digests() {
    let request = request();
    let receipt = receipt(
        &request,
        GeoAdjudicationLabel::SelectedParcels(vec!["1004540041".to_string()]),
        GeoTruthPlane::HumanAdjudication,
        "adjudicator:reviewer-1",
    );

    validate_adjudication_receipt(&request, &receipt, CROP_BYTES)
        .expect("selected shown candidate with matching request/crop digests validates");
    canonical_adjudication_request_bytes(&request).expect("request canonicalizes");
    canonical_adjudication_receipt_bytes(&receipt).expect("receipt canonicalizes");
}

#[test]
fn t22_adjudication_receipt_refuses_label_outside_candidates() {
    let request = request();
    let receipt = receipt(
        &request,
        GeoAdjudicationLabel::SelectedParcels(vec!["1004540099".to_string()]),
        GeoTruthPlane::HumanAdjudication,
        "adjudicator:reviewer-1",
    );

    let error = validate_adjudication_receipt(&request, &receipt, CROP_BYTES)
        .expect_err("parcel outside shown candidates must refuse");
    assert_eq!(
        error.code,
        GeoAdjudicationErrorCode::AdjudicationLabelOutsideCandidates
    );
    assert_eq!(
        error.detail.get("parcel_id").map(String::as_str),
        Some("1004540099")
    );
}

#[test]
fn t22_adjudication_receipt_refuses_non_human_truth_plane() {
    let request = request();
    let receipt = receipt(
        &request,
        GeoAdjudicationLabel::SelectedParcels(vec!["1004540041".to_string()]),
        GeoTruthPlane::AddressDerivedControl,
        "adjudicator:reviewer-1",
    );

    let error = validate_adjudication_receipt(&request, &receipt, CROP_BYTES)
        .expect_err("non-human truth plane must refuse");
    assert_eq!(
        error.code,
        GeoAdjudicationErrorCode::AdjudicationLabelOutsideCandidates
    );
    assert_eq!(
        error.detail.get("truth_plane").map(String::as_str),
        Some("address_derived_control")
    );
}

#[test]
fn t22_build_adjudication_requests_refuses_missing_pin() {
    let cases = vec![(
        "case.b".to_string(),
        "subject.missing".to_string(),
        vec!["1004540041".to_string()],
    )];
    let pins = BTreeMap::new();
    let overlays = BTreeMap::from([("case.b".to_string(), polygon(0, 0))]);

    let error = build_adjudication_requests(&cases, &pins, &overlays)
        .expect_err("case without retained pin must refuse");
    assert_eq!(error.code, GeoAdjudicationErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("subject_id")
    );
    assert_eq!(
        error.detail.get("value").map(String::as_str),
        Some("subject.missing")
    );
}

#[test]
fn t22_build_adjudication_requests_orders_by_case_id() {
    let cases = vec![
        (
            "case.b".to_string(),
            "subject.2".to_string(),
            vec!["1004540042".to_string(), "1004540041".to_string()],
        ),
        (
            "case.a".to_string(),
            "subject.1".to_string(),
            vec!["1004540041".to_string()],
        ),
    ];
    let pins = BTreeMap::from([
        ("subject.1".to_string(), tile_pin(b"tile one")),
        ("subject.2".to_string(), tile_pin(b"tile two")),
    ]);
    let overlays = BTreeMap::from([
        ("case.a".to_string(), polygon(0, 0)),
        ("case.b".to_string(), polygon(10, 10)),
    ]);

    let requests = build_adjudication_requests(&cases, &pins, &overlays).expect("requests build");
    let case_ids = requests
        .iter()
        .map(|request| request.case_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(case_ids, vec!["case.a", "case.b"]);
    assert_eq!(
        requests[1].candidate_parcel_ids,
        vec!["1004540041".to_string(), "1004540042".to_string()]
    );
    for request in requests {
        validate_adjudication_request_artifact(&request).expect("built request validates");
    }
}

#[test]
fn t22_adjudication_receipt_refuses_extra_candidate_request_digest() {
    let request = request();
    let mut altered_request = request.clone();
    altered_request
        .candidate_parcel_ids
        .push("1004540099".to_string());
    let mut receipt = receipt(
        &request,
        GeoAdjudicationLabel::SelectedParcels(vec!["1004540041".to_string()]),
        GeoTruthPlane::HumanAdjudication,
        "adjudicator:reviewer-1",
    );
    receipt.request_blake3 =
        adjudication_request_blake3(&altered_request).expect("altered request hashes");

    let error = validate_adjudication_receipt(&request, &receipt, CROP_BYTES)
        .expect_err("request digest from a different candidate set must refuse");
    assert_eq!(
        error.code,
        GeoAdjudicationErrorCode::ImageTileDigestMismatch
    );
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("request_blake3")
    );
    assert!(error.detail.contains_key("request_blake3"));
}

#[test]
fn t22_adjudication_receipt_refuses_non_adjudicator_namespace() {
    let request = request();
    for adjudicator_id in ["observer.rule.v0", "reviewer-1"] {
        let receipt = receipt(
            &request,
            GeoAdjudicationLabel::SelectedParcels(vec!["1004540041".to_string()]),
            GeoTruthPlane::HumanAdjudication,
            adjudicator_id,
        );

        let error = validate_adjudication_receipt(&request, &receipt, CROP_BYTES)
            .expect_err("non-adjudicator namespace must refuse");
        assert_eq!(
            error.code,
            GeoAdjudicationErrorCode::AdjudicationLabelOutsideCandidates
        );
        assert_eq!(
            error.detail.get("adjudicator_id").map(String::as_str),
            Some(adjudicator_id)
        );
    }
}

#[test]
fn t22_revalidates_retained_label_row_to_receipt_without_changing_label() {
    let request = request();
    let row = retained_label_row(
        &request,
        "pin.alpha",
        GeoAdjudicationLabel::SelectedParcels(vec!["1004540041".to_string()]),
        blake3::hash(CROP_BYTES).to_hex().as_str(),
    );
    let pins = BTreeMap::from([("pin.alpha".to_string(), request.tile_pin.clone())]);
    let crop_bytes = BTreeMap::from([("case.alpha".to_string(), CROP_BYTES.to_vec())]);

    let report = revalidate_adjudication_labels(&[row.clone()], &pins, &crop_bytes)
        .expect("retained label row revalidates");

    assert_eq!(report.labels_in, 1);
    assert_eq!(report.receipts_out, 1);
    assert!(report.labels_unchanged);
    assert!(report.d0_labels_invalid.is_empty());
    assert_eq!(report.receipts[0].label, row.label);
    assert_eq!(
        report.receipts[0].request_blake3,
        adjudication_request_blake3(&request).expect("request hashes")
    );
}

#[test]
fn t22_revalidation_reports_missing_crop_bytes_without_promoting_receipt() {
    let request = request();
    let row = retained_label_row(
        &request,
        "pin.alpha",
        GeoAdjudicationLabel::SelectedParcels(vec!["1004540041".to_string()]),
        blake3::hash(CROP_BYTES).to_hex().as_str(),
    );
    let pins = BTreeMap::from([("pin.alpha".to_string(), request.tile_pin.clone())]);
    let report = revalidate_adjudication_labels(&[row], &pins, &BTreeMap::new())
        .expect("missing crop is a row-level invalid label, not a top-level error");

    assert_eq!(report.labels_in, 1);
    assert_eq!(report.receipts_out, 0);
    assert!(report.receipts.is_empty());
    assert_eq!(report.d0_labels_invalid.len(), 1);
    assert_eq!(
        report.d0_labels_invalid[0].reason,
        GeoAdjudicationInvalidLabelReason::MissingCropBytes
    );
    assert_eq!(
        report.d0_labels_invalid[0]
            .detail
            .get("case_id")
            .map(String::as_str),
        Some("case.alpha")
    );
}

#[test]
fn t22_d0_retained_fixture_is_not_promoted_without_typed_pins_and_crop_bytes() {
    let labels = retained_d0_label_rows();
    let pins = typed_d0_pins_with_blake3_only();
    let report = revalidate_adjudication_labels(&labels, &pins, &BTreeMap::new())
        .expect("D0 revalidation reports retained gaps without mutating labels");

    assert_eq!(report.labels_in, 6);
    assert_eq!(report.receipts_out, 0);
    assert!(report.labels_unchanged);
    assert!(report.receipts.is_empty());
    assert_eq!(report.d0_labels_invalid.len(), 6);
    assert_eq!(
        report.labels_in,
        report.receipts_out + report.d0_labels_invalid.len() as u64
    );

    let reasons = report
        .d0_labels_invalid
        .iter()
        .map(|invalid| invalid.reason)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        reasons,
        BTreeSet::from([
            GeoAdjudicationInvalidLabelReason::MissingTilePin,
            GeoAdjudicationInvalidLabelReason::MissingCropBytes,
        ])
    );
    assert_eq!(
        report
            .d0_labels_invalid
            .iter()
            .filter(|invalid| invalid.reason == GeoAdjudicationInvalidLabelReason::MissingCropBytes)
            .count(),
        2
    );
}

#[test]
fn t27_adjudicate_module_has_no_demo_or_region_literals() {
    let source = include_str!("../src/geo/adjudicate.rs").to_ascii_lowercase();
    for literal in [
        "franklin",
        "39049",
        "epsg:3735",
        "1004540041",
        "chimera_wrongly_admitted",
        "asserted_address_core",
        "case_4",
        "solve_composition",
        "reqwest",
        "hyper::",
        "openai",
        "anthropic",
        "gemini",
        "bedrock",
        "invoke_model",
    ] {
        assert!(
            !source.contains(literal),
            "generic adjudicate module must not contain fixture/provider literal {literal}"
        );
    }
}

#[derive(Debug, Deserialize)]
struct D0LabelsFile {
    labels: Vec<D0LabelRow>,
}

#[derive(Debug, Deserialize)]
struct D0LabelRow {
    case_id: String,
    subject_id: String,
    pin_id: String,
    window_blake3: String,
    candidate_parcel_ids: Vec<String>,
    overlay_geometry_blake3: String,
    crop_blake3: String,
    label: D0Label,
    adjudicator_id: String,
    truth_plane: GeoTruthPlane,
    notes_blake3: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum D0Label {
    Selected { selected_parcels: Vec<String> },
    Disposition(String),
}

#[derive(Debug, Deserialize)]
struct D0PinsFile {
    pins: Vec<D0Pin>,
}

#[derive(Debug, Deserialize)]
struct D0Pin {
    pin_id: String,
    source_dataset: String,
    url: String,
    byte_range: Option<(u64, u64)>,
    blake3: Option<String>,
    license_id: String,
    license_text_blake3: String,
}

fn request() -> GeoAdjudicationRequest {
    let overlay = polygon(0, 0);
    let overlay_geometry_blake3 = blake3::hash(&serde_json::to_vec(&overlay).unwrap())
        .to_hex()
        .to_string();
    GeoAdjudicationRequest {
        version: CANON_GEO_ADJUDICATION_REQUEST_VERSION.to_string(),
        case_id: "case.alpha".to_string(),
        subject_id: "subject.alpha".to_string(),
        tile_pin: tile_pin(b"tile alpha"),
        window_blake3: overlay_geometry_blake3.clone(),
        candidate_parcel_ids: vec!["1004540041".to_string(), "1004540042".to_string()],
        overlay_geometry_blake3,
    }
}

fn retained_label_row(
    request: &GeoAdjudicationRequest,
    pin_id: &str,
    label: GeoAdjudicationLabel,
    crop_blake3: &str,
) -> GeoAdjudicationRetainedLabelRow {
    GeoAdjudicationRetainedLabelRow {
        case_id: request.case_id.clone(),
        subject_id: request.subject_id.clone(),
        pin_id: pin_id.to_string(),
        window_blake3: request.window_blake3.clone(),
        candidate_parcel_ids: request.candidate_parcel_ids.clone(),
        overlay_geometry_blake3: request.overlay_geometry_blake3.clone(),
        crop_blake3: crop_blake3.to_string(),
        label,
        adjudicator_id: "adjudicator:reviewer-1".to_string(),
        truth_plane: GeoTruthPlane::HumanAdjudication,
        notes_blake3: None,
    }
}

fn retained_d0_label_rows() -> Vec<GeoAdjudicationRetainedLabelRow> {
    let labels: D0LabelsFile = serde_json::from_str(D0_LABELS_JSON).expect("D0 labels parse");
    labels
        .labels
        .into_iter()
        .map(|row| GeoAdjudicationRetainedLabelRow {
            case_id: row.case_id,
            subject_id: row.subject_id,
            pin_id: row.pin_id,
            window_blake3: row.window_blake3,
            candidate_parcel_ids: row.candidate_parcel_ids,
            overlay_geometry_blake3: row.overlay_geometry_blake3,
            crop_blake3: row.crop_blake3,
            label: d0_label(row.label),
            adjudicator_id: row.adjudicator_id,
            truth_plane: row.truth_plane,
            notes_blake3: row.notes_blake3,
        })
        .collect()
}

fn d0_label(label: D0Label) -> GeoAdjudicationLabel {
    match label {
        D0Label::Selected { selected_parcels } => {
            GeoAdjudicationLabel::SelectedParcels(selected_parcels)
        }
        D0Label::Disposition(disposition) if disposition == "none_visible" => {
            GeoAdjudicationLabel::NoneVisible
        }
        D0Label::Disposition(disposition) if disposition == "unresolvable" => {
            GeoAdjudicationLabel::Unresolvable
        }
        D0Label::Disposition(disposition) => panic!("unsupported D0 label {disposition}"),
    }
}

fn typed_d0_pins_with_blake3_only() -> BTreeMap<String, GeoImageTilePin> {
    let pins: D0PinsFile = serde_json::from_str(D0_PINS_JSON).expect("D0 pins parse");
    pins.pins
        .into_iter()
        .filter_map(|pin| {
            pin.blake3.map(|blake3| {
                (
                    pin.pin_id,
                    GeoImageTilePin {
                        url: pin.url,
                        byte_range: pin.byte_range,
                        etag: None,
                        blake3,
                        vintage: GeoValidTimeInterval {
                            start_day: 19_723,
                            end_day: 20_088,
                        },
                        license_id: pin.license_id,
                        license_text_blake3: pin.license_text_blake3,
                        source_dataset: pin.source_dataset,
                    },
                )
            })
        })
        .collect()
}

fn receipt(
    request: &GeoAdjudicationRequest,
    label: GeoAdjudicationLabel,
    truth_plane: GeoTruthPlane,
    adjudicator_id: &str,
) -> GeoAdjudicationReceipt {
    GeoAdjudicationReceipt {
        version: CANON_GEO_ADJUDICATION_RECEIPT_VERSION.to_string(),
        request_blake3: adjudication_request_blake3(request).expect("request hashes"),
        crop_blake3: blake3::hash(CROP_BYTES).to_hex().to_string(),
        label,
        adjudicator_id: adjudicator_id.to_string(),
        truth_plane,
        notes_blake3: None,
    }
}

fn tile_pin(bytes: &[u8]) -> GeoImageTilePin {
    GeoImageTilePin {
        url: "https://example.test/ortho/tile.bin".to_string(),
        byte_range: Some((0, bytes.len() as u64)),
        etag: Some("fixture-etag".to_string()),
        blake3: blake3::hash(bytes).to_hex().to_string(),
        vintage: GeoValidTimeInterval {
            start_day: 19_800,
            end_day: 19_830,
        },
        license_id: "cc_by_4_0".to_string(),
        license_text_blake3: blake3::hash(b"license text").to_hex().to_string(),
        source_dataset: "fixture.ortho".to_string(),
    }
}

fn polygon(min_x: i64, min_y: i64) -> GeoCanonicalPolygonMm {
    GeoCanonicalPolygonMm {
        exterior: GeoCanonicalRingMm {
            vertices: vec![
                GeoPointMm::new(min_x, min_y),
                GeoPointMm::new(min_x + 10, min_y),
                GeoPointMm::new(min_x + 10, min_y + 10),
                GeoPointMm::new(min_x, min_y + 10),
            ],
        },
        holes: Vec::new(),
    }
}
