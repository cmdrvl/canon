#![forbid(unsafe_code)]

//! E4 adjudication harness (bd-1g4x).
//!
//! Layers admitted-sound evidence channels onto the frozen Gate V2 population
//! requests and measures exactly what each channel does to the residual.
//! Rho discipline is enforced structurally:
//!
//! - PAD house-number-span membership (inventory row 3) compiles into a HARD
//!   `AnyOf` constraint. It is the logically sound reading: if the loan's
//!   asserted address frontage exists, then some selected parcel's PAD integer
//!   span must contain one of its parsed numbers. Only integer comparisons on
//!   fixture data decide membership; no floats, no fuzzy string matching.
//! - Asserted size versus MapPLUTO `bldg_area` (rows 6/7) stays EMPIRICAL. It
//!   is measured as a diagnostic band overlap and never constrains.
//! - Geocode discs (row 1) are evaluated as an EMPIRICAL counterfactual. The
//!   landed population now contains a falsification, so the channel is not
//!   admitted as a hard constraint. It still reports its exact separation
//!   power and the source-contract breach instead of disappearing.
//!
//! Verdicts use the predeclared ladder at the bottom of this file. The one
//! absolute invariant under rho soundness: a sound channel must never prune
//! the truth model out of the residual. Any violation is a named finding,
//! not a tolerated failure.

use canon::geo::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS, GeoCompositionRequest,
    GeoCompositionStatus, GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef, GeoHardConstraint,
    GeoHardConstraintKind, GeoTruthPlane, model_satisfies_request, solve_composition,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize)]
struct PopulationFixture {
    cases: Vec<PopulationCase>,
}

#[derive(Debug, Deserialize)]
struct GeodiscFixture {
    discs: Vec<GeodiscEntry>,
}

#[derive(Debug, Deserialize)]
struct GeodiscEntry {
    case_id: String,
    #[serde(default)]
    property_ordinal: usize,
    accuracy_type: String,
    in_disc_bbls: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PopulationCase {
    case_id: String,
    truth_parcels: Vec<String>,
    #[serde(default)]
    pip_parcels: Vec<String>,
    candidate_parcels: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EnrichmentFixture {
    cases: Vec<EnrichmentCase>,
    candidate_parcel_attributes: BTreeMap<String, ParcelAttributes>,
}

#[derive(Debug, Deserialize)]
struct EnrichmentCase {
    case_id: String,
    properties: Vec<PropertyEvidence>,
}

#[derive(Debug, Deserialize)]
struct PropertyEvidence {
    #[serde(default)]
    address_strings: Vec<String>,
    #[serde(default)]
    asserted_size_observations: Vec<AssertedSize>,
}

#[derive(Debug, Deserialize)]
struct AssertedSize {
    /// Null rows are preserved source observations; they carry no band.
    size: Option<String>,
    size_measure: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ParcelAttributes {
    bldg_area: Option<u64>,
    pad: Option<PadSummary>,
}
#[derive(Debug, Deserialize)]
struct PadSummary {
    min_house_number_int: Option<u64>,
    max_house_number_int: Option<u64>,
}

fn population_fixture() -> PopulationFixture {
    serde_json::from_str(include_str!("fixtures/geo/e4_gate_v2_population.json"))
        .expect("Gate V2 population fixture must parse")
}

fn enrichment_fixture() -> EnrichmentFixture {
    serde_json::from_str(include_str!(
        "fixtures/geo/e4_gate_v2_evidence_enrichment.json"
    ))
    .expect("evidence enrichment fixture must parse")
}

/// Every integer token appearing in the case's asserted address strings.
/// Overcapture is deliberate and conservative: a wider number pool widens the
/// satisfying set, which weakens the constraint while keeping it sound.
fn parsed_address_numbers(case: &EnrichmentCase) -> Vec<u64> {
    let mut numbers = BTreeSet::new();
    for address in case_addresses(case) {
        let mut token = String::new();
        for character in address.chars().chain(std::iter::once(' ')) {
            if character.is_ascii_digit() {
                token.push(character);
            } else if !token.is_empty() {
                if let Ok(number) = token.parse::<u64>() {
                    numbers.insert(number);
                }
                token.clear();
            }
        }
    }
    numbers.into_iter().collect()
}

/// All asserted address strings across the case's properties.
fn case_addresses(case: &EnrichmentCase) -> Vec<&str> {
    case.properties
        .iter()
        .flat_map(|property| property.address_strings.iter())
        .map(String::as_str)
        .collect()
}

/// Candidate parcels whose PAD integer span contains at least one parsed
/// number from any asserted address. Parcels without numeric PAD coverage
/// satisfy nothing; that absence is recorded evidence, not an error.
fn pad_span_satisfying_set(
    candidates: &[String],
    attributes: &BTreeMap<String, ParcelAttributes>,
    numbers: &[u64],
) -> Vec<String> {
    candidates
        .iter()
        .filter(|parcel| {
            let Some(pad) = attributes.get(*parcel).and_then(|entry| entry.pad.as_ref()) else {
                return false;
            };
            let (Some(min), Some(max)) = (pad.min_house_number_int, pad.max_house_number_int)
            else {
                return false;
            };
            numbers
                .iter()
                .any(|number| *number >= min && *number <= max)
        })
        .cloned()
        .collect()
}

/// Exact inclusive ±25% integer band: floor(3v/4), ceil(5v/4).
/// The upper endpoint clamps only when the mathematical result is outside the
/// u64 measurement domain. No floating-point rounding enters the receipt.
fn asserted_area_diagnostic_band(value: u64) -> (u64, u64) {
    let value = u128::from(value);
    let low = (value * 3) / 4;
    let high = (value * 5).div_ceil(4);
    (
        u64::try_from(low).expect("three quarters of u64 fits u64"),
        u64::try_from(high).unwrap_or(u64::MAX),
    )
}

fn base_request(case: &PopulationCase) -> GeoCompositionRequest {
    let mut parcels = case.candidate_parcels.clone();
    parcels.sort();
    parcels.dedup();
    let mut preferences = case.pip_parcels.clone();
    preferences.sort();
    preferences.dedup();
    GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: Default::default(),
        universe: GeoCompositionUniverse {
            parcels,
            buildings: Vec::new(),
        },
        hard_constraints: Vec::new(),
        soft_preferences: preferences
            .iter()
            .enumerate()
            .map(|(index, parcel)| canon::geo::GeoSoftPreference {
                id: format!("pip-{index:03}"),
                member: GeoEntityRef::new(GeoEntityLevel::Parcel, parcel.clone()),
                cost_if_absent: 1,
            })
            .collect(),
        max_assignments: 2_097_152,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PadDisposition {
    Applied,
    VacuousAllCandidates,
    NoNumbersParsed,
    InfeasibleNoCandidateCovers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum AdjudicationVerdict {
    ResolvedByJointChannels,
    CollapsedHonestAmbiguity,
    ThinEvidenceUnchangedVacuousChannel,
    UnchangedNonvacuousChannel,
    RefutationFinding,
    GeodiscRefutationFinding,
    EmpiricalDiagnosticOnly,
    BaseConflict,
    /// The bounded search could not finish inside the declared budget on an
    /// applied channel. Never read as collapse or resolution.
    ChannelBudgetFallback,
    /// Truth parcels fall outside the candidate universe or lack the
    /// attribute rows channels read. The recorded L.3 reach limitation;
    /// soundness claims are undefined here and must not fire.
    TruthUnrepresentableReachLimit,
}

#[derive(Debug, Clone, Serialize)]
struct AdjudicationRow {
    case_id: String,
    truth_plane: GeoTruthPlane,
    candidate_count: usize,
    full_truth_recall: bool,
    truth_representable: bool,
    parsed_numbers: Vec<u64>,
    pad_set_size: usize,
    pad_disposition: PadDisposition,
    geodisc_properties: usize,
    geodisc_applied: usize,
    geodisc_empty: usize,
    base_residual_model_count: u64,
    base_counts_saturated: bool,
    after_residual_model_count: u64,
    after_counts_saturated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    after_status: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    after_conflict_ids: Vec<String>,
    truth_survives_base: bool,
    truth_survives_after: bool,
    geodisc_counterfactual_residual_model_count: u64,
    geodisc_counterfactual_counts_saturated: bool,
    geodisc_truth_survives_counterfactual: bool,
    sqft_band_candidate_hits: u64,
    sqft_band_truth_hits: u64,
    verdict: AdjudicationVerdict,
}

fn adjudicate_row(
    population_case: &PopulationCase,
    enrichment_case: &EnrichmentCase,
    attributes: &BTreeMap<String, ParcelAttributes>,
    case_discs: &[GeodiscEntry],
) -> AdjudicationRow {
    let numbers = parsed_address_numbers(enrichment_case);
    let satisfying =
        pad_span_satisfying_set(&population_case.candidate_parcels, attributes, &numbers);

    let mut request = base_request(population_case);
    let disposition = if numbers.is_empty() {
        PadDisposition::NoNumbersParsed
    } else if satisfying.len() == request.universe.parcels.len() {
        PadDisposition::VacuousAllCandidates
    } else if satisfying.is_empty() {
        PadDisposition::InfeasibleNoCandidateCovers
    } else {
        request.hard_constraints.push(GeoHardConstraint {
            id: "pad-house-number-span".to_string(),
            constraint: GeoHardConstraintKind::AnyOf {
                members: satisfying
                    .iter()
                    .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id.clone()))
                    .collect(),
            },
        });
        PadDisposition::Applied
    };

    let base_artifact = solve_composition(&base_request(population_case)).expect("base must solve");
    let mut truth_sorted = population_case.truth_parcels.clone();
    truth_sorted.sort();
    let truth_model = canon::geo::GeoCompositionModel {
        parcels: truth_sorted.clone(),
        buildings: Vec::new(),
    };

    // Representable = truth lives inside the input-side candidate universe.
    // Attribute-row presence gates specific channels, not soundness: the
    // geodisc channel needs coordinates only, which the universe encodes.
    let truth_representable = !truth_sorted.is_empty()
        && truth_sorted
            .iter()
            .all(|parcel| population_case.candidate_parcels.contains(parcel));

    let truth_survives_base = model_satisfies_request(&base_request(population_case), &truth_model)
        .expect("validated request");

    // Geodisc channels are evaluated counterfactually, not admitted. The
    // source assertion plus radius is an empirical premise, and the expanded
    // population contains a representable truth that falsifies it. Keeping a
    // separate request preserves its exact separation power without letting
    // an uncalibrated source contract prune the admitted residual.
    let candidate_set: BTreeSet<&String> = population_case.candidate_parcels.iter().collect();
    let mut geodisc_properties = 0_usize;
    let mut geodisc_applied = 0_usize;
    let mut geodisc_empty = 0_usize;
    let mut geodisc_counterfactual_request = request.clone();
    for disc in case_discs {
        if disc.in_disc_bbls.is_empty() {
            // No MapPLUTO centroid at all near this asserted point.
            geodisc_empty += 1;
            continue;
        }
        geodisc_properties += 1;
        let members: Vec<GeoEntityRef> = disc
            .in_disc_bbls
            .iter()
            .filter(|id| candidate_set.contains(id))
            .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id.clone()))
            .collect();
        if members.is_empty() {
            geodisc_empty += 1;
        } else {
            geodisc_applied += 1;
            geodisc_counterfactual_request
                .hard_constraints
                .push(GeoHardConstraint {
                    id: format!("geodisc-{}-{}", disc.property_ordinal, disc.accuracy_type),
                    constraint: GeoHardConstraintKind::AnyOf { members },
                });
        }
    }
    let hard_channel_applied = matches!(disposition, PadDisposition::Applied);

    let pad_only_artifact = if matches!(disposition, PadDisposition::Applied) {
        solve_composition(&request).expect("pad-only request must solve")
    } else {
        base_artifact.clone()
    };
    let geodisc_counterfactual_artifact = if geodisc_applied > 0 {
        solve_composition(&geodisc_counterfactual_request)
            .expect("geodisc counterfactual request must solve")
    } else {
        pad_only_artifact.clone()
    };
    let after_artifact = &pad_only_artifact;
    let after_status = if hard_channel_applied {
        Some(format!("{:?}", after_artifact.status))
    } else {
        None
    };
    let after_conflict_ids =
        if hard_channel_applied && after_artifact.status == GeoCompositionStatus::Conflict {
            after_artifact.conflict_constraint_ids.clone()
        } else {
            Vec::new()
        };
    let truth_survives_after =
        model_satisfies_request(&request, &truth_model).expect("validated request");
    let geodisc_truth_survives_counterfactual =
        model_satisfies_request(&geodisc_counterfactual_request, &truth_model)
            .expect("validated geodisc counterfactual request");

    // Diagnostic-only empirical channel: asserted SQFT versus MapPLUTO
    // bldg_area within a declared +/-25% band (section 2.1's honest
    // half-width). Never a constraint; recorded for realized residual
    // reduction. It is not expected value of information without a
    // counterfactual outcome distribution and acquisition cost.
    let sqft_values: Vec<u64> = enrichment_case
        .properties
        .iter()
        .flat_map(|property| property.asserted_size_observations.iter())
        .filter(|observation| {
            observation
                .size_measure
                .as_deref()
                .is_some_and(|measure| measure.eq_ignore_ascii_case("sqft"))
        })
        .filter_map(|observation| {
            observation
                .size
                .as_deref()
                .and_then(|size| size.parse::<u64>().ok())
        })
        .collect();
    let mut sqft_band_candidate_hits = 0_u64;
    let mut sqft_band_truth_hits = 0_u64;
    for value in &sqft_values {
        let (low, high) = asserted_area_diagnostic_band(*value);
        for parcel in population_case
            .candidate_parcels
            .iter()
            .chain(&truth_sorted)
        {
            let Some(area) = attributes.get(parcel).and_then(|entry| entry.bldg_area) else {
                continue;
            };
            if area >= low && area <= high {
                if truth_sorted.binary_search(parcel).is_ok() {
                    sqft_band_truth_hits += 1;
                } else {
                    sqft_band_candidate_hits += 1;
                }
            }
        }
    }

    let full_truth_recall = !truth_sorted.is_empty()
        && truth_sorted
            .iter()
            .all(|parcel| population_case.candidate_parcels.contains(parcel));

    let verdict =
        if geodisc_applied > 0 && truth_representable && !geodisc_truth_survives_counterfactual {
            // The empirical counterfactual excludes the recorded collateral. It
            // is a falsification receipt for the proposed source contract, never
            // permission to prune the admitted hard residual.
            AdjudicationVerdict::GeodiscRefutationFinding
        } else if !truth_representable {
            AdjudicationVerdict::TruthUnrepresentableReachLimit
        } else if matches!(disposition, PadDisposition::InfeasibleNoCandidateCovers) {
            AdjudicationVerdict::RefutationFinding
        } else if geodisc_empty > 0 {
            AdjudicationVerdict::GeodiscRefutationFinding
        } else if after_artifact.status == GeoCompositionStatus::Conflict {
            AdjudicationVerdict::BaseConflict
        } else if after_artifact.status == GeoCompositionStatus::BudgetFallback {
            AdjudicationVerdict::ChannelBudgetFallback
        } else if after_artifact.status == GeoCompositionStatus::Resolved {
            AdjudicationVerdict::ResolvedByJointChannels
        } else if residual_is_strictly_smaller(after_artifact, &base_artifact)
            || residual_is_strictly_smaller(&pad_only_artifact, &base_artifact)
        {
            AdjudicationVerdict::CollapsedHonestAmbiguity
        } else if matches!(disposition, PadDisposition::Applied) {
            AdjudicationVerdict::UnchangedNonvacuousChannel
        } else if geodisc_applied > 0 {
            AdjudicationVerdict::EmpiricalDiagnosticOnly
        } else {
            AdjudicationVerdict::ThinEvidenceUnchangedVacuousChannel
        };

    AdjudicationRow {
        case_id: population_case.case_id.clone(),
        truth_plane: GeoTruthPlane::GateV2Historical,
        candidate_count: population_case.candidate_parcels.len(),
        full_truth_recall,
        truth_representable,
        parsed_numbers: numbers,
        pad_set_size: satisfying.len(),
        pad_disposition: disposition,
        geodisc_properties,
        geodisc_applied,
        geodisc_empty,
        base_residual_model_count: base_artifact.summary.residual_model_count,
        base_counts_saturated: base_artifact.summary.residual_model_count_saturated,
        after_residual_model_count: after_artifact.summary.residual_model_count,
        after_counts_saturated: after_artifact.summary.residual_model_count_saturated,
        after_status,
        after_conflict_ids,
        truth_survives_base,
        truth_survives_after,
        geodisc_counterfactual_residual_model_count: geodisc_counterfactual_artifact
            .summary
            .residual_model_count,
        geodisc_counterfactual_counts_saturated: geodisc_counterfactual_artifact
            .summary
            .residual_model_count_saturated,
        geodisc_truth_survives_counterfactual,
        sqft_band_candidate_hits,
        sqft_band_truth_hits,
        verdict,
    }
}

fn residual_is_strictly_smaller(
    candidate: &canon::geo::GeoCompositionArtifact,
    baseline: &canon::geo::GeoCompositionArtifact,
) -> bool {
    let candidate_count = candidate.summary.residual_model_count;
    let baseline_count = baseline.summary.residual_model_count;
    match (
        candidate.summary.residual_model_count_saturated,
        baseline.summary.residual_model_count_saturated,
    ) {
        (false, true) => true,
        (false, false) => candidate_count < baseline_count,
        (true, _) => false,
    }
}

#[derive(Debug, Deserialize)]
struct ExtensionFixture {
    cases: Vec<PopulationCase>,
}

fn extension_fixture() -> ExtensionFixture {
    serde_json::from_str(include_str!("fixtures/geo/e4_h4_extension.json"))
        .expect("H4 extension fixture must parse")
}

fn load_cases() -> Vec<(PopulationCase, EnrichmentCase, Vec<GeodiscEntry>)> {
    let mut population = population_fixture();
    let extension = extension_fixture();
    population.cases.extend(extension.cases);
    let enrichment = enrichment_fixture();
    let geodisc = serde_json::from_str::<GeodiscFixture>(include_str!(
        "fixtures/geo/e4_gate_v2_geodisc.json"
    ))
    .expect("geodisc fixture must parse");
    let mut discs_by_id: BTreeMap<String, Vec<GeodiscEntry>> = BTreeMap::new();
    for disc in geodisc.discs {
        discs_by_id
            .entry(disc.case_id.clone())
            .or_default()
            .push(disc);
    }
    let mut enrichment_by_id: BTreeMap<String, EnrichmentCase> = enrichment
        .cases
        .into_iter()
        .map(|case| (case.case_id.clone(), case))
        .collect();
    let mut cases = population.cases;
    cases.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    cases
        .into_iter()
        .map(|case| {
            // Extension-stratum cases carry no enrichment rows yet: their
            // evidence channels are recorded as not-yet-onboarded rather
            // than fabricated.
            let enriched = enrichment_by_id
                .remove(&case.case_id)
                .unwrap_or(EnrichmentCase {
                    case_id: case.case_id.clone(),
                    properties: Vec::new(),
                });
            let discs = discs_by_id.remove(&case.case_id).unwrap_or_default();
            (case, enriched, discs)
        })
        .collect()
}

fn run_adjudication() -> Vec<AdjudicationRow> {
    let attributes = enrichment_fixture().candidate_parcel_attributes;
    load_cases()
        .iter()
        .map(|(population_case, enrichment_case, discs)| {
            adjudicate_row(population_case, enrichment_case, &attributes, discs)
        })
        .collect()
}

#[test]
fn asserted_area_diagnostic_band_uses_exact_integer_arithmetic_at_boundaries() {
    assert_eq!(asserted_area_diagnostic_band(1), (0, 2));
    assert_eq!(asserted_area_diagnostic_band(4), (3, 5));
    assert_eq!(
        asserted_area_diagnostic_band(u64::MAX),
        (13_835_058_055_282_163_711, u64::MAX)
    );
}

#[test]
fn adjudication_table_is_complete_and_sound_channels_never_prune_truth() {
    let rows = run_adjudication();
    let expected_population = 15 + extension_fixture().cases.len();
    assert_eq!(
        rows.len(),
        expected_population,
        "Gate V2 + H4-extension cases must all adjudicate"
    );

    for row in &rows {
        // The rho invariant is conditional on representability: a channel
        // may only be judged against a truth model the universe can express.
        if row.truth_representable {
            assert!(
                row.truth_survives_base,
                "rho violation on case {}: PAD span pruned representable truth",
                row.case_id
            );
            assert!(
                row.truth_survives_after,
                "rho violation on case {}: admitted hard evidence pruned representable truth",
                row.case_id
            );
            if !row.geodisc_truth_survives_counterfactual {
                assert_eq!(
                    row.verdict,
                    AdjudicationVerdict::GeodiscRefutationFinding,
                    "case {}: empirical geodisc falsification must route to a finding",
                    row.case_id
                );
            }
        }
        assert!(
            row.after_residual_model_count <= row.base_residual_model_count,
            "adding sound constraints may only shrink the residual"
        );
        if row.pad_disposition == PadDisposition::Applied {
            assert!(
                row.pad_set_size < row.candidate_count,
                "applied channel must be non-vacuous"
            );
        }
    }

    // Predeclared verdict ladder covers every row exactly once.
    let classified = rows
        .iter()
        .filter(|row| {
            matches!(
                row.verdict,
                AdjudicationVerdict::ResolvedByJointChannels
                    | AdjudicationVerdict::CollapsedHonestAmbiguity
                    | AdjudicationVerdict::ThinEvidenceUnchangedVacuousChannel
                    | AdjudicationVerdict::UnchangedNonvacuousChannel
                    | AdjudicationVerdict::RefutationFinding
                    | AdjudicationVerdict::GeodiscRefutationFinding
                    | AdjudicationVerdict::EmpiricalDiagnosticOnly
                    | AdjudicationVerdict::BaseConflict
                    | AdjudicationVerdict::ChannelBudgetFallback
                    | AdjudicationVerdict::TruthUnrepresentableReachLimit
            )
        })
        .count();
    assert_eq!(classified, rows.len());
    assert!(
        rows.iter()
            .all(|row| row.truth_plane == GeoTruthPlane::GateV2Historical),
        "current E4 adjudication rows must remain typed to the Gate V2 historical plane"
    );

    // Reach accounting: how many cases are even adjudicable today.
    let representable = rows.iter().filter(|row| row.truth_representable).count();
    println!("representable truth: {representable}/{}", rows.len());

    // Determinism: a second full run produces identical bytes.
    let first = serde_json::to_vec(&rows).expect("serialize");
    let second = serde_json::to_vec(&run_adjudication()).expect("serialize");
    assert_eq!(first, second);

    // Operator-facing table for the bead report.
    for row in &rows {
        println!(
            "{:>16} cand={:>3} recall={:<5} repr={:<5} nums={:?} pad={:>2}/{:<3} {:?} base={:>15}{} hard={:>15}{} geodisc_cf={:>15}{} geodisc_truth={:<5} st={:?} conf={:?} sqft(c/t)={}/{} -> {:?}",
            row.case_id,
            row.candidate_count,
            row.full_truth_recall,
            row.truth_representable,
            row.parsed_numbers,
            row.pad_set_size,
            row.candidate_count,
            row.pad_disposition,
            row.base_residual_model_count,
            if row.base_counts_saturated { "+" } else { "" },
            row.after_residual_model_count,
            if row.after_counts_saturated { "+" } else { "" },
            row.geodisc_counterfactual_residual_model_count,
            if row.geodisc_counterfactual_counts_saturated {
                "+"
            } else {
                ""
            },
            row.geodisc_truth_survives_counterfactual,
            row.after_status,
            row.after_conflict_ids,
            row.sqft_band_candidate_hits,
            row.sqft_band_truth_hits,
            row.verdict,
        );
    }
}

#[test]
#[ignore = "E4 remains open; run explicitly to test the frozen acceptance gate"]
fn e4_acceptance_gate_requires_the_full_population_to_be_reachable() {
    // Frozen before this rerun from bd-1g4x's declared target population. E4
    // cannot pass on the convenient truth-representable subset: candidate
    // reach is part of the end-to-end system, even though solver soundness is
    // scored only where the truth model is representable.
    const REQUIRED_GENUINE_MULTI_PARCEL_CASES: usize = 79;

    let rows = run_adjudication();
    let representable = rows.iter().filter(|row| row.truth_representable).count();
    let rho_violations = rows
        .iter()
        .filter(|row| row.truth_representable && !row.truth_survives_after)
        .count();
    let budget_fallbacks = rows
        .iter()
        .filter(|row| row.after_status.as_deref() == Some("BudgetFallback"))
        .count();
    let empirical_falsifications = rows
        .iter()
        .filter(|row| row.truth_representable && !row.geodisc_truth_survives_counterfactual)
        .count();

    let passed = rows.len() >= REQUIRED_GENUINE_MULTI_PARCEL_CASES
        && representable == rows.len()
        && rho_violations == 0
        && budget_fallbacks == 0;

    assert!(
        passed,
        "E4 OPEN: cases={}/{REQUIRED_GENUINE_MULTI_PARCEL_CASES}, reachable={}/{}, rho_violations={rho_violations}, budget_fallbacks={budget_fallbacks}, empirical_falsifications={empirical_falsifications}",
        rows.len(),
        representable,
        rows.len(),
    );
}
