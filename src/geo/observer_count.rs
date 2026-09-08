#![forbid(unsafe_code)]

//! Frozen-weight structure-count observer measurement contracts.
//!
//! The model runner is external acquisition. Canon only admits retained rows
//! after a named error characterization and measures their residual effect.

use super::{
    composition::{
        GeoCompositionArtifact, GeoCompositionStatus, GeoCompositionUniverse, GeoEntityLevel,
        GeoEntityRef,
    },
    evidence::{GeoEvidenceClaimRole, GeoRhoAdmissionPolicy, GeoRhoBasis, GeoRhoContract},
    observer::{
        CANON_GEO_OBSERVER_VERSION, GeoErrorPopulationArtifact, GeoObservationKind,
        GeoObservationPayload, GeoObservationRow, GeoObservationRowsArtifact, GeoObserverContract,
        GeoObserverError, GeoObserverErrorCode, GeoObserverIdentity,
        admit_observations_with_universe, canonical_error_population_bytes, truth_plane_key,
        validate_error_population_artifact, validate_observer_contract,
    },
    observer_null::{
        CANON_GEO_OBSERVER_CHARACTERIZATION_VERSION, GeoObserverCharacterizationArtifact,
        GeoObserverErrorBand, GeoObserverKindCharacterization,
        canonical_observer_characterization_bytes, validate_observer_characterization_artifact,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CANON_GEO_OBSERVER_EFFECT_VERSION: &str = "canon_geo_observer_effect.v0";
pub const GEO_COUNT_OBSERVER_ID: &str = "observer.count.frozen";
pub const GEO_COUNT_RHO_CONTRACT_ID: &str = "rho.structure_count.v0";
pub const GEO_COUNT_METHOD_ID: &str = "observer.count.frozen.structure_count_band";
pub const GEO_COUNT_METHOD_VERSION: &str = "v0";
pub const GEO_COUNT_FALSIFICATION_RULE_ID: &str = "structure_count_truth_outside_band";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObserverEffectCase {
    pub case_id: String,
    pub model_count_before: u64,
    pub model_count_after: u64,
    pub backbone_gained: Vec<GeoEntityRef>,
    pub backbone_lost: Vec<GeoEntityRef>,
    pub status_before: GeoCompositionStatus,
    pub status_after: GeoCompositionStatus,
    pub redundant: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObserverEffectTruthPlaneTotals {
    pub cases: u64,
    pub changed: u64,
    pub redundant: u64,
    pub conflict_introduced: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObserverEffectTotals {
    pub cases: u64,
    pub changed: u64,
    pub redundant: u64,
    pub conflict_introduced: u64,
    pub per_truth_plane: BTreeMap<String, GeoObserverEffectTruthPlaneTotals>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObserverEffectArtifact {
    pub version: String,
    pub observer_id: String,
    pub characterization_blake3: String,
    pub denominator: u64,
    pub cases: Vec<GeoObserverEffectCase>,
    pub totals: GeoObserverEffectTotals,
}

#[derive(Debug, Clone, Default)]
struct CountAccumulator {
    compared: u64,
    exact_agreement: u64,
    max_abs_error: u64,
    lower_slack: u64,
    upper_slack: u64,
}

impl CountAccumulator {
    fn observe(&mut self, raw_count: u64, reference_count: u64) -> Result<(), GeoObserverError> {
        self.compared = checked_add(self.compared, 1, "observer count compared rows")?;
        if raw_count == reference_count {
            self.exact_agreement = checked_add(
                self.exact_agreement,
                1,
                "observer count exact agreement rows",
            )?;
        }
        let signed_error = signed_count_delta(raw_count, reference_count)?;
        self.max_abs_error = self.max_abs_error.max(signed_error.unsigned_abs());
        if raw_count < reference_count {
            self.lower_slack = self.lower_slack.max(reference_count - raw_count);
        } else {
            self.upper_slack = self.upper_slack.max(raw_count - reference_count);
        }
        Ok(())
    }

    fn artifact(self) -> GeoObserverKindCharacterization {
        GeoObserverKindCharacterization {
            compared: self.compared,
            exact_agreement: self.exact_agreement,
            max_abs_error: self.max_abs_error,
            error_band: GeoObserverErrorBand {
                lower_slack: self.lower_slack,
                upper_slack: self.upper_slack,
            },
        }
    }
}

pub fn frozen_count_observer_contract(
    model_id: impl Into<String>,
    weight_blake3: impl Into<String>,
    arithmetic_contract: impl Into<String>,
    population_id: impl Into<String>,
    characterization_blake3: impl Into<String>,
) -> GeoObserverContract {
    GeoObserverContract {
        id: GEO_COUNT_OBSERVER_ID.to_string(),
        version: CANON_GEO_OBSERVER_VERSION.to_string(),
        identity: GeoObserverIdentity::FrozenWeight {
            model_id: model_id.into(),
            weight_blake3: weight_blake3.into(),
            arithmetic_contract: arithmetic_contract.into(),
        },
        output_kinds: vec![
            GeoObservationKind::StructureCountInWindow,
            GeoObservationKind::PresentAtVintage,
            GeoObservationKind::AbsentAtVintage,
        ],
        error_population_id: population_id.into(),
        characterization_blake3: characterization_blake3.into(),
        rho_contract_ids: vec![GEO_COUNT_RHO_CONTRACT_ID.to_string()],
    }
}

pub fn frozen_count_rho_contract(
    source_dataset: impl Into<String>,
    source_release: impl Into<String>,
    population_id: impl Into<String>,
    characterization_blake3: impl Into<String>,
) -> GeoRhoContract {
    let population_id = population_id.into();
    let characterization_blake3 = characterization_blake3.into();
    let mut source_lineage_ids = vec![population_id.clone(), characterization_blake3.clone()];
    source_lineage_ids.sort();
    source_lineage_ids.dedup();
    GeoRhoContract {
        id: GEO_COUNT_RHO_CONTRACT_ID.to_string(),
        version: "v0".to_string(),
        source_dataset: source_dataset.into(),
        source_release: source_release.into(),
        source_lineage_ids,
        method_id: GEO_COUNT_METHOD_ID.to_string(),
        method_version: GEO_COUNT_METHOD_VERSION.to_string(),
        claim_role: GeoEvidenceClaimRole::AttributeObservation,
        basis: GeoRhoBasis::EmpiricalCalibration {
            population_id,
            calibration_blake3: characterization_blake3,
            falsification_rule_id: GEO_COUNT_FALSIFICATION_RULE_ID.to_string(),
            admissible_hard_band: true,
            admission_policy: GeoRhoAdmissionPolicy::Declared,
        },
    }
}

pub fn characterize_count_observer(
    contract: &GeoObserverContract,
    rows: &[GeoObservationRow],
    population: &GeoErrorPopulationArtifact,
    reference: &BTreeMap<String, u64>,
) -> Result<GeoObserverCharacterizationArtifact, GeoObserverError> {
    validate_count_contract_shape(contract)?;
    validate_error_population_artifact(population)?;
    if contract.error_population_id != population.population_id {
        return Err(observer_error(
            GeoObserverErrorCode::ObserverErrorUncharacterized,
            "Geo count observer characterization population does not match the observer contract",
            [
                (
                    "contract_population_id",
                    contract.error_population_id.clone(),
                ),
                ("population_id", population.population_id.clone()),
            ],
        ));
    }

    let population_blake3 = digest_bytes(&canonical_error_population_bytes(population)?);
    let subjects_by_id = population
        .subjects
        .iter()
        .map(|subject| (subject.subject_id.as_str(), subject))
        .collect::<BTreeMap<_, _>>();

    let mut aggregate = CountAccumulator::default();
    let mut per_truth_plane = BTreeMap::<String, CountAccumulator>::new();
    let mut seen_subject_ids = BTreeSet::new();
    let mut rows_outside_population = Vec::new();
    let mut sorted_rows = rows.to_vec();
    sorted_rows.sort_by(|left, right| left.id.cmp(&right.id));

    for row in &sorted_rows {
        validate_count_row_contract(contract, row)?;
        let Some(subject_id) = subject_id_from_count_observation_id(&row.id) else {
            rows_outside_population.push(row.id.clone());
            continue;
        };
        let Some(subject) = subjects_by_id.get(subject_id) else {
            rows_outside_population.push(row.id.clone());
            continue;
        };
        if subject.window_blake3 != row.window_blake3 {
            rows_outside_population.push(row.id.clone());
            continue;
        }
        let Some(reference_count) = reference.get(subject_id).copied() else {
            continue;
        };
        if !seen_subject_ids.insert(subject_id.to_string()) {
            return Err(observer_invalid(
                "Geo count observer characterization accepts at most one row per population subject",
                [("subject_id", subject_id.to_string())],
            ));
        }
        let raw_count = raw_count(row)?;
        aggregate.observe(raw_count, reference_count)?;
        per_truth_plane
            .entry(truth_plane_key(subject.truth_plane).to_string())
            .or_default()
            .observe(raw_count, reference_count)?;
    }

    let mut missing_subject_ids = population
        .subjects
        .iter()
        .filter(|subject| {
            !seen_subject_ids.contains(subject.subject_id.as_str())
                || !reference.contains_key(subject.subject_id.as_str())
        })
        .map(|subject| subject.subject_id.clone())
        .collect::<Vec<_>>();
    missing_subject_ids.sort();
    rows_outside_population.sort();
    rows_outside_population.dedup();

    let artifact = GeoObserverCharacterizationArtifact {
        version: CANON_GEO_OBSERVER_CHARACTERIZATION_VERSION.to_string(),
        observer_id: contract.id.clone(),
        population_blake3,
        rows_total: u64::try_from(rows.len())
            .map_err(|_| observer_overflow("observer count row total"))?,
        missing_subject_ids,
        rows_outside_population,
        per_kind: BTreeMap::from([(
            observation_kind_key(GeoObservationKind::StructureCountInWindow).to_string(),
            aggregate.artifact(),
        )]),
        per_truth_plane: per_truth_plane
            .into_iter()
            .map(|(key, value)| (key, value.artifact()))
            .collect(),
        method: "frozen_weight_count_vs_landed_footprint_plane".to_string(),
        is_null_baseline: false,
        non_redundant_case_ids: Vec::new(),
    };
    validate_observer_characterization_artifact(&artifact)?;
    Ok(artifact)
}

pub fn widen_count_observation_row(
    row: &GeoObservationRow,
    characterization: &GeoObserverCharacterizationArtifact,
) -> Result<GeoObservationRow, GeoObserverError> {
    validate_observer_characterization_artifact(characterization)?;
    let raw_count = raw_count(row)?;
    let band = count_error_band(characterization)?;
    let min = raw_count.saturating_sub(band.lower_slack);
    let max = raw_count
        .checked_add(band.upper_slack)
        .ok_or_else(|| observer_overflow("observer count upper band"))?;
    let mut widened = row.clone();
    widened.raw_count = Some(raw_count);
    widened.payload = GeoObservationPayload::StructureCountInWindow { min, max };
    Ok(widened)
}

pub fn admit_characterized_count_observations(
    contract: &GeoObserverContract,
    rows: &[GeoObservationRow],
    rho: &[GeoRhoContract],
    forbidden_license_ids: &[String],
    universe: &GeoCompositionUniverse,
    characterization: &GeoObserverCharacterizationArtifact,
) -> Result<GeoObservationRowsArtifact, GeoObserverError> {
    validate_count_contract_shape(contract)?;
    validate_observer_characterization_artifact(characterization)?;
    let characterization_blake3 = digest_bytes(&canonical_observer_characterization_bytes(
        characterization,
    )?);
    if contract.characterization_blake3 != characterization_blake3 {
        return Err(observer_error(
            GeoObserverErrorCode::ObserverErrorUncharacterized,
            "Geo count observer contract does not bind the supplied characterization",
            [
                (
                    "contract_characterization_blake3",
                    contract.characterization_blake3.clone(),
                ),
                ("characterization_blake3", characterization_blake3),
            ],
        ));
    }
    for row in rows {
        validate_count_band_not_narrowed(row, characterization)?;
    }
    admit_observations_with_universe(contract, rows, rho, forbidden_license_ids, universe)
}

pub fn validate_count_band_not_narrowed(
    row: &GeoObservationRow,
    characterization: &GeoObserverCharacterizationArtifact,
) -> Result<(), GeoObserverError> {
    let raw_count = row.raw_count.ok_or_else(|| {
        observer_error(
            GeoObserverErrorCode::ObserverBandNotFromCharacterization,
            "Geo count observer rows require raw_count before admission",
            [("observation_id", row.id.clone())],
        )
    })?;
    let band = count_error_band(characterization)?;
    let expected_min = raw_count.saturating_sub(band.lower_slack);
    let expected_max = raw_count
        .checked_add(band.upper_slack)
        .ok_or_else(|| observer_overflow("observer count expected upper band"))?;
    let GeoObservationPayload::StructureCountInWindow { min, max } = &row.payload else {
        return Err(observer_error(
            GeoObserverErrorCode::ObserverBandNotFromCharacterization,
            "Geo count observer rows require a structure-count band payload",
            [("observation_id", row.id.clone())],
        ));
    };
    if *min > expected_min || *max < expected_max {
        return Err(observer_error(
            GeoObserverErrorCode::ObserverBandNotFromCharacterization,
            "Geo count observer band is narrower than the characterized error band",
            [
                ("observation_id", row.id.clone()),
                ("band", format!("{min}..{max}")),
                (
                    "characterized_band",
                    format!("{expected_min}..{expected_max}"),
                ),
            ],
        ));
    }
    Ok(())
}

pub fn measure_observer_effect(
    observer_id: impl Into<String>,
    characterization_blake3: impl Into<String>,
    before_after: &[(String, GeoCompositionArtifact, GeoCompositionArtifact)],
    truth_planes: &BTreeMap<String, String>,
) -> Result<GeoObserverEffectArtifact, GeoObserverError> {
    let observer_id = observer_id.into();
    validate_nonempty("observer_id", &observer_id)?;
    let characterization_blake3 = characterization_blake3.into();
    validate_blake3("characterization_blake3", &characterization_blake3)?;

    let mut cases = Vec::with_capacity(before_after.len());
    let mut totals = GeoObserverEffectTotals {
        cases: 0,
        changed: 0,
        redundant: 0,
        conflict_introduced: 0,
        per_truth_plane: BTreeMap::new(),
    };
    for (case_id, before, after) in before_after {
        validate_nonempty("case_id", case_id)?;
        let case = effect_case(case_id, before, after)?;
        totals.cases = checked_add(totals.cases, 1, "observer effect cases")?;
        if case.redundant {
            totals.redundant = checked_add(totals.redundant, 1, "observer effect redundant")?;
        } else {
            totals.changed = checked_add(totals.changed, 1, "observer effect changed")?;
        }
        if before.status != GeoCompositionStatus::Conflict
            && after.status == GeoCompositionStatus::Conflict
        {
            totals.conflict_introduced =
                checked_add(totals.conflict_introduced, 1, "observer effect conflicts")?;
        }
        let plane = truth_planes
            .get(case_id)
            .map(String::as_str)
            .unwrap_or("unlabeled")
            .to_string();
        update_plane_totals(&mut totals.per_truth_plane, plane, &case)?;
        cases.push(case);
    }
    cases.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    let denominator = totals.cases;
    let artifact = GeoObserverEffectArtifact {
        version: CANON_GEO_OBSERVER_EFFECT_VERSION.to_string(),
        observer_id,
        characterization_blake3,
        denominator,
        cases,
        totals,
    };
    validate_observer_effect_artifact(&artifact)?;
    Ok(artifact)
}

pub fn validate_observer_effect_artifact(
    artifact: &GeoObserverEffectArtifact,
) -> Result<(), GeoObserverError> {
    if artifact.version != CANON_GEO_OBSERVER_EFFECT_VERSION {
        return Err(observer_error(
            GeoObserverErrorCode::UnsupportedVersion,
            "Unsupported Geo observer effect artifact version",
            [
                ("actual", artifact.version.clone()),
                ("expected", CANON_GEO_OBSERVER_EFFECT_VERSION.to_string()),
            ],
        ));
    }
    validate_nonempty("observer_id", &artifact.observer_id)?;
    validate_blake3("characterization_blake3", &artifact.characterization_blake3)?;
    let case_count = u64::try_from(artifact.cases.len())
        .map_err(|_| observer_overflow("observer effect case count"))?;
    if artifact.denominator != artifact.totals.cases || artifact.denominator != case_count {
        return Err(observer_invalid(
            "Geo observer effect denominator must equal the case count",
            [
                ("denominator", artifact.denominator.to_string()),
                ("cases", artifact.cases.len().to_string()),
                ("totals.cases", artifact.totals.cases.to_string()),
            ],
        ));
    }
    if checked_add(
        artifact.totals.changed,
        artifact.totals.redundant,
        "observer effect changed plus redundant",
    )? != artifact.totals.cases
    {
        return Err(observer_invalid(
            "Geo observer effect changed plus redundant must equal total cases",
            [
                ("changed", artifact.totals.changed.to_string()),
                ("redundant", artifact.totals.redundant.to_string()),
                ("cases", artifact.totals.cases.to_string()),
            ],
        ));
    }
    let mut previous: Option<&str> = None;
    for case in &artifact.cases {
        validate_nonempty("cases[].case_id", &case.case_id)?;
        if previous.is_some_and(|previous| previous >= case.case_id.as_str()) {
            return Err(observer_invalid(
                "Geo observer effect cases must be sorted and distinct",
                [("case_id", case.case_id.clone())],
            ));
        }
        previous = Some(case.case_id.as_str());
        if case.model_count_after > case.model_count_before || !case.backbone_lost.is_empty() {
            return Err(observer_error(
                GeoObserverErrorCode::ObserverEffectWidened,
                "Geo observer effect widened a residual or lost backbone members",
                [
                    ("case_id", case.case_id.clone()),
                    ("model_count_before", case.model_count_before.to_string()),
                    ("model_count_after", case.model_count_after.to_string()),
                ],
            ));
        }
    }
    let plane_cases = artifact
        .totals
        .per_truth_plane
        .values()
        .map(|plane| plane.cases)
        .try_fold(0_u64, |acc, value| {
            checked_add(acc, value, "observer effect plane case sum")
        })?;
    if plane_cases != artifact.totals.cases {
        return Err(observer_invalid(
            "Geo observer effect per-plane totals must cover every case",
            [
                ("plane_cases", plane_cases.to_string()),
                ("cases", artifact.totals.cases.to_string()),
            ],
        ));
    }
    Ok(())
}

pub fn canonical_observer_effect_bytes(
    artifact: &GeoObserverEffectArtifact,
) -> Result<Vec<u8>, GeoObserverError> {
    validate_observer_effect_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        observer_invalid(
            "Geo observer effect artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

fn effect_case(
    case_id: &str,
    before: &GeoCompositionArtifact,
    after: &GeoCompositionArtifact,
) -> Result<GeoObserverEffectCase, GeoObserverError> {
    let model_count_before = before.summary.residual_model_count;
    let model_count_after = after.summary.residual_model_count;
    if model_count_after > model_count_before {
        return Err(observer_error(
            GeoObserverErrorCode::ObserverEffectWidened,
            "Geo observer effect cannot widen residual model counts",
            [
                ("case_id", case_id.to_string()),
                ("model_count_before", model_count_before.to_string()),
                ("model_count_after", model_count_after.to_string()),
            ],
        ));
    }
    let before_backbone = backbone_members(before);
    let after_backbone = backbone_members(after);
    let backbone_gained = after_backbone
        .difference(&before_backbone)
        .cloned()
        .collect::<Vec<_>>();
    let backbone_lost = before_backbone
        .difference(&after_backbone)
        .cloned()
        .collect::<Vec<_>>();
    if !backbone_lost.is_empty() {
        return Err(observer_error(
            GeoObserverErrorCode::ObserverEffectWidened,
            "Geo observer effect cannot remove backbone members",
            [
                ("case_id", case_id.to_string()),
                ("backbone_lost", backbone_lost.len().to_string()),
            ],
        ));
    }
    let redundant = before.status == after.status
        && model_count_before == model_count_after
        && backbone_gained.is_empty()
        && backbone_lost.is_empty();
    Ok(GeoObserverEffectCase {
        case_id: case_id.to_string(),
        model_count_before,
        model_count_after,
        backbone_gained,
        backbone_lost,
        status_before: before.status,
        status_after: after.status,
        redundant,
    })
}

fn update_plane_totals(
    per_truth_plane: &mut BTreeMap<String, GeoObserverEffectTruthPlaneTotals>,
    plane: String,
    case: &GeoObserverEffectCase,
) -> Result<(), GeoObserverError> {
    let entry = per_truth_plane
        .entry(plane)
        .or_insert(GeoObserverEffectTruthPlaneTotals {
            cases: 0,
            changed: 0,
            redundant: 0,
            conflict_introduced: 0,
        });
    entry.cases = checked_add(entry.cases, 1, "observer effect plane cases")?;
    if case.redundant {
        entry.redundant = checked_add(entry.redundant, 1, "observer effect plane redundant")?;
    } else {
        entry.changed = checked_add(entry.changed, 1, "observer effect plane changed")?;
    }
    if case.status_before != GeoCompositionStatus::Conflict
        && case.status_after == GeoCompositionStatus::Conflict
    {
        entry.conflict_introduced = checked_add(
            entry.conflict_introduced,
            1,
            "observer effect plane conflicts",
        )?;
    }
    Ok(())
}

fn backbone_members(artifact: &GeoCompositionArtifact) -> BTreeSet<GeoEntityRef> {
    artifact
        .hard_forced
        .parcels
        .iter()
        .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id.clone()))
        .chain(
            artifact
                .hard_forced
                .buildings
                .iter()
                .map(|id| GeoEntityRef::new(GeoEntityLevel::Building, id.clone())),
        )
        .collect()
}

fn validate_count_contract_shape(contract: &GeoObserverContract) -> Result<(), GeoObserverError> {
    validate_observer_contract(contract)?;
    if contract.id != GEO_COUNT_OBSERVER_ID {
        return Err(observer_invalid(
            "Geo count observer requires the frozen count observer contract",
            [("observer_id", contract.id.clone())],
        ));
    }
    if !matches!(&contract.identity, GeoObserverIdentity::FrozenWeight { .. }) {
        return Err(observer_invalid(
            "Geo count observer requires a frozen-weight identity",
            [("observer_id", contract.id.clone())],
        ));
    }
    if !contract
        .output_kinds
        .contains(&GeoObservationKind::StructureCountInWindow)
    {
        return Err(observer_error(
            GeoObserverErrorCode::ObserverErrorUncharacterized,
            "Geo count observer contracts must declare structure-count output",
            [("observer_id", contract.id.clone())],
        ));
    }
    Ok(())
}

fn validate_count_row_contract(
    contract: &GeoObserverContract,
    row: &GeoObservationRow,
) -> Result<(), GeoObserverError> {
    if row.observer_id != contract.id {
        return Err(observer_error(
            GeoObserverErrorCode::ObserverMissingProvenance,
            "Geo count rows must bind the observer contract id",
            [
                ("observer_id", row.observer_id.clone()),
                ("contract_id", contract.id.clone()),
            ],
        ));
    }
    if row.kind != GeoObservationKind::StructureCountInWindow {
        return Err(observer_error(
            GeoObserverErrorCode::ObserverErrorUncharacterized,
            "Geo count characterization only accepts structure-count rows",
            [("observation_id", row.id.clone())],
        ));
    }
    raw_count(row).map(|_| ())
}

fn raw_count(row: &GeoObservationRow) -> Result<u64, GeoObserverError> {
    if let Some(raw_count) = row.raw_count {
        return Ok(raw_count);
    }
    match &row.payload {
        GeoObservationPayload::StructureCountInWindow { min, max } if min == max => Ok(*min),
        GeoObservationPayload::StructureCountInWindow { .. } => Err(observer_error(
            GeoObserverErrorCode::ObserverBandNotFromCharacterization,
            "Geo count observer widened rows must retain raw_count",
            [("observation_id", row.id.clone())],
        )),
        _ => Err(observer_error(
            GeoObserverErrorCode::ObserverErrorUncharacterized,
            "Geo count observer rows require a structure-count payload",
            [("observation_id", row.id.clone())],
        )),
    }
}

fn count_error_band(
    characterization: &GeoObserverCharacterizationArtifact,
) -> Result<&GeoObserverErrorBand, GeoObserverError> {
    characterization
        .per_kind
        .get(observation_kind_key(
            GeoObservationKind::StructureCountInWindow,
        ))
        .map(|entry| &entry.error_band)
        .ok_or_else(|| {
            observer_error(
                GeoObserverErrorCode::ObserverErrorUncharacterized,
                "Geo count observer characterization lacks structure-count error band",
                [(
                    "kind",
                    observation_kind_key(GeoObservationKind::StructureCountInWindow),
                )],
            )
        })
}

fn subject_id_from_count_observation_id(row_id: &str) -> Option<&str> {
    let rest = row_id.strip_prefix("obs:count:")?;
    rest.rsplit_once(':').map(|(subject_id, _)| subject_id)
}

fn observation_kind_key(kind: GeoObservationKind) -> &'static str {
    match kind {
        GeoObservationKind::StructureCountInWindow => "structure_count_in_window",
        GeoObservationKind::FootprintOutline => "footprint_outline",
        GeoObservationKind::HeightOrFloors => "height_or_floors",
        GeoObservationKind::PresentAtVintage => "present_at_vintage",
        GeoObservationKind::AbsentAtVintage => "absent_at_vintage",
        GeoObservationKind::ChangeEvent => "change_event",
    }
}

fn validate_nonempty(field: &'static str, value: &str) -> Result<(), GeoObserverError> {
    if value.is_empty() || value.trim() != value {
        return Err(observer_invalid(
            "Geo count observer string fields must be non-empty and canonical-trimmed",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn validate_blake3(field: &'static str, value: &str) -> Result<(), GeoObserverError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(observer_invalid(
            "Geo count observer BLAKE3 fields must be lowercase fixed-width hex",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn checked_add(left: u64, right: u64, context: &'static str) -> Result<u64, GeoObserverError> {
    left.checked_add(right)
        .ok_or_else(|| observer_overflow(context))
}

fn signed_count_delta(raw_count: u64, reference_count: u64) -> Result<i64, GeoObserverError> {
    if raw_count >= reference_count {
        let delta = raw_count - reference_count;
        i64::try_from(delta).map_err(|_| observer_overflow("observer count signed error"))
    } else {
        let delta = reference_count - raw_count;
        let magnitude =
            i64::try_from(delta).map_err(|_| observer_overflow("observer count signed error"))?;
        magnitude
            .checked_neg()
            .ok_or_else(|| observer_overflow("observer count signed error"))
    }
}

fn digest_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn observer_error(
    code: GeoObserverErrorCode,
    message: impl Into<String>,
    detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
) -> GeoObserverError {
    GeoObserverError {
        code,
        message: message.into(),
        detail: detail
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
    }
}

fn observer_invalid(
    message: impl Into<String>,
    detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
) -> GeoObserverError {
    observer_error(GeoObserverErrorCode::InvalidInput, message, detail)
}

fn observer_overflow(context: &'static str) -> GeoObserverError {
    observer_error(
        GeoObserverErrorCode::ArithmeticOverflow,
        "Geo count observer arithmetic overflow",
        [("context", context)],
    )
}
