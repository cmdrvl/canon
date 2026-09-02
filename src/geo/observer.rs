#![forbid(unsafe_code)]

//! Offline observer-lane contracts.
//!
//! Network acquisition and model execution stay outside Canon's deterministic
//! runtime. This module only validates retained pins, declared populations, and
//! deterministic selection inputs that later observer artifacts bind by digest.

use super::{evaluation::GeoTruthPlane, evidence::GeoValidTimeInterval};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_ERROR_POPULATION_VERSION: &str = "canon_geo_error_population.v0";

const SPLITMIX64_INCREMENT: u64 = 0x9E37_79B9_7F4A_7C15;
const SPLITMIX64_MULTIPLIER_1: u64 = 0xBF58_476D_1CE4_E5B9;
const SPLITMIX64_MULTIPLIER_2: u64 = 0x94D0_49BB_1331_11EB;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoImageTilePin {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_range: Option<(u64, u64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    pub blake3: String,
    pub vintage: GeoValidTimeInterval,
    pub license_id: String,
    pub license_text_blake3: String,
    pub source_dataset: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoErrorPopulationSubject {
    pub subject_id: String,
    pub truth_plane: GeoTruthPlane,
    pub window_blake3: String,
    pub parcel_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoErrorPopulationArtifact {
    pub version: String,
    pub population_id: String,
    pub region: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_query_blake3: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_population_blake3: Option<String>,
    pub subjects: Vec<GeoErrorPopulationSubject>,
    pub declared_before_observer_ids: Vec<String>,
    pub stratum_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoObserverErrorCode {
    UnsupportedVersion,
    InvalidInput,
    ObserverLicenseForbidden,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoObserverError {
    pub code: GeoObserverErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoObserverError {
    fn new(
        code: GeoObserverErrorCode,
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail: detail
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    fn invalid(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoObserverErrorCode::InvalidInput, message, detail)
    }

    fn invalid_field(
        field: &'static str,
        message: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self::invalid(
            message,
            [("field", field.to_string()), ("value", value.into())],
        )
    }
}

impl fmt::Display for GeoObserverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoObserverError {}

pub fn validate_error_population_artifact(
    artifact: &GeoErrorPopulationArtifact,
) -> Result<(), GeoObserverError> {
    if artifact.version != CANON_GEO_ERROR_POPULATION_VERSION {
        return Err(GeoObserverError::new(
            GeoObserverErrorCode::UnsupportedVersion,
            "Unsupported Geo error-population artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_ERROR_POPULATION_VERSION),
            ],
        ));
    }

    validate_canonical_string("population_id", &artifact.population_id)?;
    validate_canonical_string("region", &artifact.region)?;
    match artifact.selection_seed {
        Some(seed) if seed != 0 => {}
        Some(_) => {
            return Err(GeoObserverError::invalid_field(
                "selection_seed",
                "Geo error-population selection_seed must be present and nonzero",
                "0",
            ));
        }
        None => {
            return Err(GeoObserverError::invalid_field(
                "selection_seed",
                "Geo error-population selection_seed must be present and nonzero",
                "<missing>",
            ));
        }
    }
    validate_required_blake3(
        "selection_query_blake3",
        artifact.selection_query_blake3.as_deref(),
    )?;
    validate_required_blake3(
        "source_population_blake3",
        artifact.source_population_blake3.as_deref(),
    )?;
    validate_declared_before_observer_ids(&artifact.declared_before_observer_ids)?;
    validate_error_population_subjects(&artifact.subjects)?;
    validate_stratum_counts(&artifact.subjects, &artifact.stratum_counts)?;
    Ok(())
}

pub fn canonical_error_population_bytes(
    artifact: &GeoErrorPopulationArtifact,
) -> Result<Vec<u8>, GeoObserverError> {
    validate_error_population_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoObserverError::invalid(
            "Geo error-population artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn select_error_population_subjects(
    candidates: &[GeoErrorPopulationSubject],
    seed: u64,
    counts_by_truth_plane: &BTreeMap<GeoTruthPlane, usize>,
) -> Result<Vec<GeoErrorPopulationSubject>, GeoObserverError> {
    if seed == 0 {
        return Err(GeoObserverError::invalid_field(
            "selection_seed",
            "Geo error-population selection_seed must be present and nonzero",
            "0",
        ));
    }
    if counts_by_truth_plane.is_empty() {
        return Err(GeoObserverError::invalid_field(
            "stratum_counts",
            "Geo error-population selection requires at least one stratum",
            "0",
        ));
    }

    let mut grouped = BTreeMap::<GeoTruthPlane, Vec<&GeoErrorPopulationSubject>>::new();
    for subject in candidates {
        grouped
            .entry(subject.truth_plane)
            .or_default()
            .push(subject);
    }

    let mut selected = Vec::new();
    for (truth_plane, requested_count) in counts_by_truth_plane {
        if *requested_count == 0 {
            return Err(GeoObserverError::invalid_field(
                "stratum_counts",
                "Geo error-population stratum counts must be positive",
                "0",
            ));
        }
        let Some(subjects) = grouped.get_mut(truth_plane) else {
            return Err(GeoObserverError::invalid_field(
                "subjects",
                "Geo error-population source candidates are missing a requested stratum",
                truth_plane_key(*truth_plane),
            ));
        };
        subjects.sort_by(|left, right| left.subject_id.cmp(&right.subject_id));
        if subjects.len() < *requested_count {
            return Err(GeoObserverError::invalid(
                "Geo error-population source candidates cannot satisfy the requested stratum count",
                [
                    ("field", "stratum_counts".to_string()),
                    ("truth_plane", truth_plane_key(*truth_plane).to_string()),
                    ("available", subjects.len().to_string()),
                    ("requested", requested_count.to_string()),
                ],
            ));
        }

        let mut ranked = subjects
            .iter()
            .enumerate()
            .map(|(index, subject)| {
                (
                    splitmix64(seed.wrapping_add(index as u64)),
                    subject.subject_id.as_str(),
                    *subject,
                )
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| (left.0, left.1).cmp(&(right.0, right.1)));
        selected.extend(
            ranked
                .into_iter()
                .take(*requested_count)
                .map(|(_, _, subject)| subject.clone()),
        );
    }

    selected.sort_by(|left, right| left.subject_id.cmp(&right.subject_id));
    Ok(selected)
}

pub fn splitmix64(value: u64) -> u64 {
    let mut z = value.wrapping_add(SPLITMIX64_INCREMENT);
    z = (z ^ (z >> 30)).wrapping_mul(SPLITMIX64_MULTIPLIER_1);
    z = (z ^ (z >> 27)).wrapping_mul(SPLITMIX64_MULTIPLIER_2);
    z ^ (z >> 31)
}

pub fn truth_plane_key(truth_plane: GeoTruthPlane) -> &'static str {
    match truth_plane {
        GeoTruthPlane::NonRoundAmountDateLegalBorough => "non_round_amount_date_legal_borough",
        GeoTruthPlane::RoundExactLenderParty => "round_exact_lender_party",
        GeoTruthPlane::GateV2Historical => "gate_v2_historical",
        GeoTruthPlane::AddressDerivedControl => "address_derived_control",
        GeoTruthPlane::HumanAdjudication => "human_adjudication",
    }
}

fn validate_declared_before_observer_ids(ids: &[String]) -> Result<(), GeoObserverError> {
    if ids.is_empty() {
        return Err(GeoObserverError::invalid_field(
            "declared_before_observer_ids",
            "Geo error-population artifacts must bind at least one future observer id",
            "0",
        ));
    }
    let mut previous: Option<&str> = None;
    for id in ids {
        validate_canonical_string("declared_before_observer_ids[]", id)?;
        if let Some(previous_id) = previous
            && previous_id >= id.as_str()
        {
            return Err(GeoObserverError::invalid(
                "Geo error-population observer ids must be strictly sorted and unique",
                [
                    ("field", "declared_before_observer_ids".to_string()),
                    ("previous", previous_id.to_string()),
                    ("current", id.clone()),
                ],
            ));
        }
        previous = Some(id.as_str());
    }
    Ok(())
}

fn validate_error_population_subjects(
    subjects: &[GeoErrorPopulationSubject],
) -> Result<(), GeoObserverError> {
    if subjects.is_empty() {
        return Err(GeoObserverError::invalid_field(
            "subjects",
            "Geo error-population artifacts must contain at least one subject",
            "0",
        ));
    }
    let mut previous: Option<&str> = None;
    for subject in subjects {
        validate_canonical_string("subjects[].subject_id", &subject.subject_id)?;
        match subject.truth_plane {
            GeoTruthPlane::NonRoundAmountDateLegalBorough
            | GeoTruthPlane::RoundExactLenderParty => {}
            _ => {
                return Err(GeoObserverError::invalid(
                    "Geo error-population subjects must use a controlling H.7 truth plane",
                    [
                        ("field", "subjects[].truth_plane".to_string()),
                        ("subject_id", subject.subject_id.clone()),
                        (
                            "truth_plane",
                            truth_plane_key(subject.truth_plane).to_string(),
                        ),
                    ],
                ));
            }
        }
        validate_blake3("subjects[].window_blake3", &subject.window_blake3)?;
        validate_parcel_ids(subject)?;
        if let Some(previous_id) = previous
            && previous_id >= subject.subject_id.as_str()
        {
            return Err(GeoObserverError::invalid(
                "Geo error-population subjects must be strictly sorted and unique",
                [
                    ("field", "subjects".to_string()),
                    ("previous_subject_id", previous_id.to_string()),
                    ("subject_id", subject.subject_id.clone()),
                ],
            ));
        }
        previous = Some(subject.subject_id.as_str());
    }
    Ok(())
}

fn validate_parcel_ids(subject: &GeoErrorPopulationSubject) -> Result<(), GeoObserverError> {
    if subject.parcel_ids.is_empty() {
        return Err(GeoObserverError::invalid(
            "Geo error-population subjects must carry at least one parcel id",
            [
                ("field", "subjects[].parcel_ids".to_string()),
                ("subject_id", subject.subject_id.clone()),
            ],
        ));
    }
    let mut seen = BTreeSet::new();
    let mut previous: Option<&str> = None;
    for parcel_id in &subject.parcel_ids {
        validate_canonical_string("subjects[].parcel_ids[]", parcel_id)?;
        if !seen.insert(parcel_id.as_str())
            || previous.is_some_and(|previous_id| previous_id >= parcel_id.as_str())
        {
            return Err(GeoObserverError::invalid(
                "Geo error-population subject parcel ids must be strictly sorted and unique",
                [
                    ("field", "subjects[].parcel_ids".to_string()),
                    ("subject_id", subject.subject_id.clone()),
                    ("parcel_id", parcel_id.clone()),
                ],
            ));
        }
        previous = Some(parcel_id.as_str());
    }
    Ok(())
}

fn validate_stratum_counts(
    subjects: &[GeoErrorPopulationSubject],
    stratum_counts: &BTreeMap<String, u64>,
) -> Result<(), GeoObserverError> {
    if stratum_counts.is_empty() {
        return Err(GeoObserverError::invalid_field(
            "stratum_counts",
            "Geo error-population artifacts must declare per-truth-plane counts",
            "0",
        ));
    }
    let mut expected = BTreeMap::<String, u64>::new();
    for subject in subjects {
        let key = truth_plane_key(subject.truth_plane).to_string();
        *expected.entry(key).or_default() += 1;
    }
    for required in [
        GeoTruthPlane::NonRoundAmountDateLegalBorough,
        GeoTruthPlane::RoundExactLenderParty,
    ] {
        if expected
            .get(truth_plane_key(required))
            .copied()
            .unwrap_or(0)
            == 0
        {
            return Err(GeoObserverError::invalid(
                "Geo error-population artifacts must represent both H.7 truth planes",
                [
                    ("field", "stratum_counts".to_string()),
                    ("missing_truth_plane", truth_plane_key(required).to_string()),
                ],
            ));
        }
    }
    if stratum_counts != &expected {
        return Err(GeoObserverError::invalid(
            "Geo error-population stratum_counts must match subjects",
            [("field", "stratum_counts".to_string())],
        ));
    }
    Ok(())
}

fn validate_canonical_string(field: &'static str, value: &str) -> Result<(), GeoObserverError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoObserverError::invalid_field(
            field,
            "Geo observer string fields must be non-empty and canonical-trimmed",
            value,
        ));
    }
    Ok(())
}

fn validate_required_blake3(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), GeoObserverError> {
    match value {
        Some(value) => validate_blake3(field, value),
        None => Err(GeoObserverError::invalid_field(
            field,
            "Geo observer BLAKE3 fields must be present",
            "<missing>",
        )),
    }
}

fn validate_blake3(field: &'static str, value: &str) -> Result<(), GeoObserverError> {
    if value.len() != 64
        || !value.chars().all(|ch| ch.is_ascii_hexdigit())
        || value.chars().any(|ch| ch.is_ascii_uppercase())
    {
        return Err(GeoObserverError::invalid_field(
            field,
            "Geo observer BLAKE3 fields must be lowercase fixed-width hex",
            value,
        ));
    }
    Ok(())
}
