#![forbid(unsafe_code)]

use canon::geo::{
    GeoCompositionArtifact, GeoCompositionStatus, GeoRhoAdmissionPolicy, GeoRhoBasis,
    canonical_evidence_compilation_bytes, compile_evidence, descriptive::*, solve_composition,
};
use serde_json::json;

fn digest(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}

fn request() -> GeoDescriptiveAssetRequest {
    let attrs = |name: &str, kind: &str, units: u64, year: u16| {
        json!({
            "property_name": name, "property_type": kind, "unit_count": units, "year_built": year,
        })
    };
    let record = |id: &str| json!({"source_record_id": id, "source_vintage": "v1", "record_blake3": blake3::hash(id.as_bytes()).to_hex().to_string()});
    let policies: Vec<_> = ["property_name", "property_type", "unit_count", "year_built"].into_iter().map(|channel| json!({
        "channel": channel, "tolerance": if channel == "year_built" { 1 } else { 0 },
        "contract": {"id": channel, "version": "1", "source_dataset": "synthetic-assessor",
            "source_release": "v1", "source_lineage_ids": ["synthetic"],
            "method_id": "fixture-declared-compatible-attributes", "method_version": "1",
            "claim_role": "attribute_observation", "basis": {"kind": "logical_relaxation", "invariant_id": "synthetic-exact-values"}}
    })).collect();
    serde_json::from_value(json!({
        "version": CANON_GEO_DESCRIPTIVE_ASSET_REQUEST_VERSION,
        "profile": {"profile_id": GEO_DESCRIPTIVE_ASSET_PROFILE_ID, "selection_level": "parcel", "channels": policies},
        "bounded_geography": {"geography_id": "fixture-region", "geography_kind": "fixture", "description": "Synthetic bounded inventory"},
        "inventory_source": {"source_instance_id": "fixture", "release": {"release_id": "v1", "release_digest": digest("inventory")},
            "native_scope": {"kind": "native_entity", "entity_level": "parcel", "identity_participation": "stable_alias"},
            "inventory_ref": {"inventory_id": "fixture-inventory", "semantic_hash": digest("inventory"), "planning_hash": digest("inventory-plan")}},
        "claim": {"claim_id": "claim", "as_of": "2026-09-17", "attributes": attrs("Named Community", "multifamily", 120, 1985), "source_records": [record("claim")]},
        "candidates": [
            {"id": "p1", "attributes": attrs("Named Community", "multifamily", 120, 1985), "source_records": [record("p1")]},
            {"id": "p2", "attributes": attrs("Another Community", "multifamily", 120, 1986), "source_records": [record("p2")]},
            {"id": "p3", "attributes": attrs("Named Community", "industrial", 120, 1985), "source_records": [record("p3")]},
            {"id": "p4", "attributes": attrs("Named Community", "multifamily", 121, 1985), "source_records": [record("p4")]},
            {"id": "p5", "attributes": attrs("Named Community", "multifamily", 120, 1990), "source_records": [record("p5")]}
        ],
        "max_candidates": 1000, "max_assignments": 1000000, "max_materialized_models": 1000,
    })).unwrap()
}

fn solve(input: &GeoDescriptiveAssetRequest) -> GeoCompositionArtifact {
    let materialized = materialize_descriptive_asset(input).unwrap();
    let compiled = compile_evidence(&materialized.evidence).unwrap();
    solve_composition(&compiled.composition_request).unwrap()
}

#[test]
fn geography_and_descriptors_narrow_without_address_and_soft_name_never_forces() {
    let input = request();
    let output = solve(&input);
    assert_eq!(output.summary.parcel_candidates, 5);
    assert_eq!(output.summary.residual_model_count, 2);
    assert!(output.summary.residual_model_count_complete);
    assert_ne!(output.status, GeoCompositionStatus::Resolved);
    let models: Vec<_> = output
        .residual_models
        .iter()
        .map(|model| model.parcels.clone())
        .collect();
    assert_eq!(models, vec![vec!["p1".to_string()], vec!["p2".to_string()]]);
    let materialized = materialize_descriptive_asset(&input).unwrap();
    assert_eq!(materialized.channels["unit_count"].incompatible, 1);
    let mut narrowed = input;
    narrowed
        .profile
        .channels
        .iter_mut()
        .find(|p| p.channel == GeoDescriptiveChannel::YearBuilt)
        .unwrap()
        .tolerance = 0;
    assert_eq!(solve(&narrowed).summary.residual_model_count, 1);
}

#[test]
fn missing_values_preserve_candidates_and_absent_claim_emits_no_constraint() {
    let mut input = request();
    input.candidates[3].attributes.unit_count = None;
    assert_eq!(solve(&input).summary.residual_model_count, 3);
    input.claim.attributes.unit_count = None;
    let materialized = materialize_descriptive_asset(&input).unwrap();
    assert!(!materialized.channels["unit_count"].claim_present);
    assert!(
        materialized
            .evidence
            .observations
            .iter()
            .all(|o| o.id != "descriptive:unit_count")
    );
    assert_eq!(solve(&input).summary.residual_model_count, 3);
}

#[test]
fn contradictions_are_empty_residuals_and_uncalibrated_bands_stay_diagnostic() {
    let mut input = request();
    input.claim.attributes.unit_count = Some(777);
    assert_eq!(solve(&input).summary.residual_model_count, 0);
    input
        .profile
        .channels
        .iter_mut()
        .find(|p| p.channel == GeoDescriptiveChannel::UnitCount)
        .unwrap()
        .contract
        .basis = GeoRhoBasis::EmpiricalCalibration {
        population_id: "unverified-fixture".to_string(),
        calibration_blake3: blake3::hash(b"fixture-only").to_hex().to_string(),
        falsification_rule_id: "compare-with-independent-truth".to_string(),
        admissible_hard_band: false,
        admission_policy: GeoRhoAdmissionPolicy::Declared,
    };
    assert_eq!(solve(&input).summary.residual_model_count, 3);
}

#[test]
fn input_order_does_not_change_compiled_evidence() {
    let input = request();
    let bytes = |r: &GeoDescriptiveAssetRequest| {
        canonical_evidence_compilation_bytes(
            &compile_evidence(&materialize_descriptive_asset(r).unwrap().evidence).unwrap(),
        )
        .unwrap()
    };
    let mut shuffled = input.clone();
    shuffled.candidates.reverse();
    shuffled.profile.channels.reverse();
    assert_eq!(bytes(&input), bytes(&shuffled));
}

#[test]
fn bad_declarations_budgets_and_address_leakage_are_rejected() {
    let input = request();
    let mut bad = input.clone();
    bad.candidates[0].attributes.year_built = Some(0);
    assert!(materialize_descriptive_asset(&bad).is_err());
    bad = input.clone();
    bad.max_candidates = 4;
    assert!(materialize_descriptive_asset(&bad).is_err());
    bad = input.clone();
    bad.candidates[0].source_records.clear();
    assert!(materialize_descriptive_asset(&bad).is_err());
    bad = input.clone();
    bad.candidates[1].id = bad.candidates[0].id.clone();
    assert!(materialize_descriptive_asset(&bad).is_err());
    bad = input.clone();
    bad.profile.channels.remove(0);
    assert!(materialize_descriptive_asset(&bad).is_err());
    let mut value = serde_json::to_value(&input).unwrap();
    value["claim"]["address"] = json!("withheld address");
    assert!(serde_json::from_value::<GeoDescriptiveAssetRequest>(value).is_err());
}

#[test]
fn bounded_inventory_handles_hundreds_of_candidates_without_guessing() {
    let mut input = request();
    let base = input.candidates[0].clone();
    input.candidates = (0..200)
        .map(|i| {
            let mut candidate = base.clone();
            candidate.id = format!("p{i:04}");
            candidate.attributes.unit_count = Some(100 + i);
            candidate
        })
        .collect();
    let result = solve(&input);
    assert!(result.summary.residual_model_count_complete);
    assert_eq!(result.summary.residual_model_count, 1);
    assert!(result.backbone_complete);
    assert_eq!(result.hard_forced.parcels, vec!["p0020"]);
}
