use canon::geo::{
    CANON_GEO_ADJUDICATION_RECEIPT_VERSION, CANON_GEO_ADJUDICATION_REQUEST_VERSION,
    GeoAdjudicationErrorCode, GeoAdjudicationLabel, GeoAdjudicationReceipt, GeoAdjudicationRequest,
    GeoCanonicalPolygonMm, GeoCanonicalRingMm, GeoImageTilePin, GeoPointMm, GeoTruthPlane,
    GeoValidTimeInterval, adjudication_request_blake3, build_adjudication_requests,
    canonical_adjudication_receipt_bytes, canonical_adjudication_request_bytes,
    validate_adjudication_receipt, validate_adjudication_request_artifact,
};
use std::collections::BTreeMap;

const CROP_BYTES: &[u8] = b"retained adjudication crop bytes";

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
