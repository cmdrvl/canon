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
//! - Geocode discs (row 1) are unavailable offline (parcel coordinates are
//!   not landed) and recorded as such rather than approximated.
//!
//! Verdicts use the predeclared ladder at the bottom of this file. The one
//! absolute invariant under rho soundness: a sound channel must never prune
//! the truth model out of the residual. Any violation is a named finding,
//! not a tolerated failure.

use canon::geo::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS, GeoCompositionRequest,
    GeoCompositionStatus, GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef, GeoHardConstraint,
    GeoHardConstraintKind, model_satisfies_request, solve_composition,
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

fn base_request(case: &PopulationCase) -> GeoCompositionRequest {
    let mut parcels = case.candidate_parcels.clone();
    parcels.sort();
    parcels.dedup();
    let mut preferences = case.pip_parcels.clone();
    preferences.sort();
    preferences.dedup();
    GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
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

    // Soundness is only decidable against a representable truth: inside the
    // candidate universe AND carrying the attribute rows channels read.
    // Otherwise this is the recorded reach limitation, not a rho verdict.
    let truth_representable = !truth_sorted.is_empty()
        && truth_sorted.iter().all(|parcel| {
            population_case.candidate_parcels.contains(parcel) && attributes.contains_key(parcel)
        });

    let truth_survives_base = model_satisfies_request(&base_request(population_case), &truth_model)
        .expect("validated request");

    // Geodisc channels: one sound constraint per ASSERTED property — every
    // property's frontage must exist inside the collateral set, so some
    // selected parcel must sit within that property's declared tier radius.
    let candidate_set: BTreeSet<&String> = population_case.candidate_parcels.iter().collect();
    let mut geodisc_properties = 0_usize;
    let mut geodisc_applied = 0_usize;
    let mut geodisc_empty = 0_usize;
    let mut joint_request = request.clone();
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
            joint_request.hard_constraints.push(GeoHardConstraint {
                id: format!("geodisc-{}-{}", disc.property_ordinal, disc.accuracy_type),
                constraint: GeoHardConstraintKind::AnyOf { members },
            });
        }
    }
    let any_channel_applied = matches!(disposition, PadDisposition::Applied) || geodisc_applied > 0;

    let pad_only_artifact = if matches!(disposition, PadDisposition::Applied) {
        solve_composition(&request).expect("pad-only request must solve")
    } else {
        base_artifact.clone()
    };
    let joint_artifact = if any_channel_applied {
        solve_composition(&joint_request).expect("joint request must solve")
    } else {
        base_artifact.clone()
    };
    let after_artifact = &joint_artifact;
    let after_status = if any_channel_applied {
        Some(format!("{:?}", after_artifact.status))
    } else {
        None
    };
    let after_conflict_ids =
        if any_channel_applied && after_artifact.status == GeoCompositionStatus::Conflict {
            after_artifact.conflict_constraint_ids.clone()
        } else {
            Vec::new()
        };
    let truth_survives_after =
        model_satisfies_request(&joint_request, &truth_model).expect("validated request");

    // Diagnostic-only empirical channel: asserted SQFT versus MapPLUTO
    // bldg_area within a declared +/-25% band (section 2.1's honest
    // half-width). Never a constraint; recorded for the VoI table.
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
        let low = (*value as f64 * 0.75).floor() as u64;
        let high = (*value as f64 * 1.25).ceil() as u64;
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

    let verdict = if !truth_representable {
        AdjudicationVerdict::TruthUnrepresentableReachLimit
    } else if matches!(disposition, PadDisposition::InfeasibleNoCandidateCovers) {
        AdjudicationVerdict::RefutationFinding
    } else if geodisc_empty > 0 {
        AdjudicationVerdict::GeodiscRefutationFinding
    } else if joint_artifact.status == GeoCompositionStatus::Conflict {
        AdjudicationVerdict::BaseConflict
    } else if after_artifact.status == GeoCompositionStatus::BudgetFallback {
        AdjudicationVerdict::ChannelBudgetFallback
    } else if after_artifact.status == GeoCompositionStatus::Resolved {
        AdjudicationVerdict::ResolvedByJointChannels
    } else if after_artifact.summary.residual_model_count
        < base_artifact.summary.residual_model_count
        || pad_only_artifact.summary.residual_model_count
            < base_artifact.summary.residual_model_count
    {
        AdjudicationVerdict::CollapsedHonestAmbiguity
    } else if geodisc_applied > 0 || matches!(disposition, PadDisposition::Applied) {
        AdjudicationVerdict::UnchangedNonvacuousChannel
    } else {
        AdjudicationVerdict::ThinEvidenceUnchangedVacuousChannel
    };

    AdjudicationRow {
        case_id: population_case.case_id.clone(),
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
        base_counts_saturated: base_artifact.summary.summary_counts_saturated,
        after_residual_model_count: after_artifact.summary.residual_model_count,
        after_counts_saturated: after_artifact.summary.summary_counts_saturated,
        after_status,
        after_conflict_ids,
        truth_survives_base,
        truth_survives_after,
        sqft_band_candidate_hits,
        sqft_band_truth_hits,
        verdict,
    }
}

fn load_cases() -> Vec<(PopulationCase, EnrichmentCase, Vec<GeodiscEntry>)> {
    let population = population_fixture();
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
            let enriched = enrichment_by_id.remove(&case.case_id).expect("joined case");
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
fn adjudication_table_is_complete_and_sound_channels_never_prune_truth() {
    let rows = run_adjudication();
    assert_eq!(rows.len(), 15, "every Gate V2 case must be adjudicated");

    for row in &rows {
        // The rho invariant is conditional on representability: a channel
        // may only be judged against a truth model the universe can express.
        if row.truth_representable {
            assert!(
                row.truth_survives_base && row.truth_survives_after,
                "rho violation on case {}: a sound channel pruned a representable truth",
                row.case_id
            );
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
                    | AdjudicationVerdict::RefutationFinding
                    | AdjudicationVerdict::GeodiscRefutationFinding
                    | AdjudicationVerdict::BaseConflict
                    | AdjudicationVerdict::ChannelBudgetFallback
                    | AdjudicationVerdict::TruthUnrepresentableReachLimit
            )
        })
        .count();
    assert_eq!(classified, rows.len());

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
            "{:>16} cand={:>3} recall={:<5} repr={:<5} nums={:?} pad={:>2}/{:<3} {:?} base={:>15}{} after={:>15}{} st={:?} conf={:?} sqft(c/t)={}/{} -> {:?}",
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
            row.after_status,
            row.after_conflict_ids,
            row.sqft_band_candidate_hits,
            row.sqft_band_truth_hits,
            row.verdict,
        );
    }
}
