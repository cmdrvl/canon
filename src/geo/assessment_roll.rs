#![forbid(unsafe_code)]

//! Assessment-roll owner evidence for bounded parcel populations.
//!
//! This module is an offline stage: it consumes already-exported assessment
//! roll rows and ACRIS party rows. It widens parcel universes by BBL block and
//! emits owner exact-match exclusions plus affiliate token preferences.

use super::{
    composition::{
        GeoCompositionModel, GeoCompositionProfile, GeoCompositionUniverse, GeoEntityLevel,
        GeoEntityRef, GeoIntegerMeasure, GeoIntegerMemberValue, GeoIntegerValueOrigin,
    },
    evaluation::{GeoLabeledCompositionCase, GeoPopulationEvaluationRequest},
    evidence::{
        CANON_GEO_EVIDENCE_REQUEST_VERSION, GeoEvidenceClaimRole, GeoEvidenceCompilationRequest,
        GeoEvidenceRecordRef, GeoRhoBasis, GeoRhoContract, GeoRhoObservation,
        GeoRhoObservationKind, compile_evidence,
    },
    stack::{
        CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION, GeoPopulationCaseEvidenceOverlay,
        GeoPopulationEvidenceStackRequest, stack_population_evidence,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION: &str =
    "canon_geo_assessment_roll_owner_request.v0";
pub const CANON_GEO_ASSESSMENT_ROLL_OWNER_VERSION: &str = "canon_geo_assessment_roll_owner.v0";

pub const GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID: &str =
    "rho.owner.assessment_roll_exact_match";
pub const GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID: &str =
    "rho.owner.assessment_roll_affiliate_preference";
pub const GEO_ASSESSMENT_ROLL_OWNER_EXACT_OBSERVATION_PREFIX: &str =
    "obs.owner.assessment_roll_exact_match";
pub const GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_OBSERVATION_PREFIX: &str =
    "obs.owner.assessment_roll_affiliate_preference";

const OWNER_NOT_EXACT_MEASURE_ID: &str = "assessment_roll.owner_not_exact";
const OWNER_NOT_EXACT_UNIT: &str = "lots";
const EXACT_METHOD_ID: &str = "assessment-roll-owner-exact-exclusion";
const AFFILIATE_METHOD_ID: &str = "assessment-roll-owner-token-preference";
const OWNER_METHOD_VERSION: &str = "1.0.0";
const OWNER_CONTRACT_VERSION: &str = "1.0.0";

const STOP_WORDS: &[&str] = &[
    "LLC",
    "INC",
    "CORP",
    "L",
    "P",
    "LP",
    "THE",
    "OF",
    "CO",
    "LTD",
    "OWNER",
    "OWNERS",
    "REALTY",
    "ASSOCIATES",
    "HOLDINGS",
    "COMPANY",
    "PROPERTY",
    "PROPERTIES",
    "TENANTS",
    "APARTMENT",
    "APARTMENTS",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAssessmentRollOwnerProofClass {
    Fixture,
    ObservedWarehouseSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollOwnerContractSource {
    pub source_dataset: String,
    pub source_release: String,
    pub source_lineage_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollOwnerCalibration {
    pub population_id: String,
    pub calibration_blake3: String,
    pub exact_falsification_rule_id: String,
    pub affiliate_falsification_rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollCaseDocument {
    pub case_id: String,
    pub document_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollLotRow {
    pub bbl: String,
    pub owner: String,
    pub gross_sqft: String,
    pub units: String,
    pub condo_number: String,
    pub source_record_id: String,
    pub source_vintage: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollPartyRow {
    pub document_id: String,
    pub party_type: String,
    /// Must already be the `PARTY_NAME_NORM` value from
    /// `STG_GEO_NYC_ACRIS_PARTIES`.
    pub party_name_norm: String,
    pub source_record_id: String,
    pub source_vintage: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollOwnerRequest {
    pub version: String,
    pub proof_class: GeoAssessmentRollOwnerProofClass,
    pub population: GeoPopulationEvaluationRequest,
    pub case_documents: Vec<GeoAssessmentRollCaseDocument>,
    pub contract_source: GeoAssessmentRollOwnerContractSource,
    pub calibration: GeoAssessmentRollOwnerCalibration,
    pub roll_rows: Vec<GeoAssessmentRollLotRow>,
    pub party_rows: Vec<GeoAssessmentRollPartyRow>,
    pub max_cases: usize,
    pub max_roll_rows: usize,
    pub max_party_rows: usize,
    pub max_overlay_observations: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollOwnerCaseSummary {
    pub case_id: String,
    pub document_id: String,
    pub input_universe_parcels: u64,
    pub widened_universe_parcels: u64,
    pub added_roll_lots: u64,
    pub party_rows: u64,
    pub exact_match_lots: u64,
    pub affiliate_preference_lots: u64,
    pub hard_observation_emitted: bool,
    pub soft_observations_emitted: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollOwnerSummary {
    pub cases: u64,
    pub roll_rows: u64,
    pub party_rows: u64,
    pub widened_cases: u64,
    pub added_roll_lots: u64,
    pub owner_overlay_cases: u64,
    pub exact_hard_observations: u64,
    pub exact_match_lots: u64,
    pub affiliate_soft_observations: u64,
    pub affiliate_preference_lots: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollOwnerArtifact {
    pub version: String,
    pub request_version: String,
    pub request_blake3: String,
    pub proof_class: GeoAssessmentRollOwnerProofClass,
    pub summary: GeoAssessmentRollOwnerSummary,
    pub cases: Vec<GeoAssessmentRollOwnerCaseSummary>,
    pub widened_population: GeoPopulationEvaluationRequest,
    pub overlay: GeoPopulationEvidenceStackRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAssessmentRollOwnerErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    SourceRecordCollision,
    Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollOwnerError {
    pub code: GeoAssessmentRollOwnerErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoAssessmentRollOwnerError {
    fn new(
        code: GeoAssessmentRollOwnerErrorCode,
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
        Self::new(
            GeoAssessmentRollOwnerErrorCode::InvalidInput,
            message,
            detail,
        )
    }

    fn invalid_field(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self::invalid(
            "Geo assessment-roll owner request contains an invalid field",
            [("field", field.into()), ("value", value.into())],
        )
    }

    fn budget(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(
            GeoAssessmentRollOwnerErrorCode::BudgetExceeded,
            message,
            detail,
        )
    }

    fn overflow(field: &str) -> Self {
        Self::new(
            GeoAssessmentRollOwnerErrorCode::ArithmeticOverflow,
            "Geo assessment-roll owner accounting overflowed",
            [("field", field)],
        )
    }
}

impl fmt::Display for GeoAssessmentRollOwnerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoAssessmentRollOwnerError {}

pub fn produce_assessment_roll_owner_evidence(
    request: &GeoAssessmentRollOwnerRequest,
) -> Result<GeoAssessmentRollOwnerArtifact, GeoAssessmentRollOwnerError> {
    let request = canonicalize_assessment_roll_owner_request(request)?;
    let request_blake3 = digest_prefixed(&request)?;
    let roll_by_bbl = roll_rows_by_bbl(&request.roll_rows)?;
    let roll_by_block = roll_rows_by_block(&request.roll_rows)?;
    let parties_by_document = party_rows_by_document(&request.party_rows);
    let document_by_case = case_documents_by_case(&request)?;
    let exact_contract = exact_contract(&request);
    let affiliate_contract = affiliate_contract(&request);

    let mut widened_population = request.population.clone();
    let mut overlays = Vec::new();
    let mut summaries = Vec::with_capacity(widened_population.cases.len());
    let mut summary = GeoAssessmentRollOwnerSummary {
        cases: widened_population.cases.len() as u64,
        roll_rows: request.roll_rows.len() as u64,
        party_rows: request.party_rows.len() as u64,
        widened_cases: 0,
        added_roll_lots: 0,
        owner_overlay_cases: 0,
        exact_hard_observations: 0,
        exact_match_lots: 0,
        affiliate_soft_observations: 0,
        affiliate_preference_lots: 0,
    };

    for case in &mut widened_population.cases {
        ensure_parcel_population_case(case)?;
        let input_parcels = case.evidence.universe.parcels.clone();
        let widened_parcels = widened_parcel_universe(&input_parcels, &roll_by_block)?;
        let added_roll_lots = widened_parcels
            .len()
            .checked_sub(input_parcels.len())
            .ok_or_else(|| GeoAssessmentRollOwnerError::overflow("added_roll_lots"))?;
        if added_roll_lots > 0 {
            summary.widened_cases = checked_inc(summary.widened_cases, "widened_cases")?;
            summary.added_roll_lots = checked_add(
                summary.added_roll_lots,
                added_roll_lots as u64,
                "added_roll_lots",
            )?;
        }
        case.evidence.universe.parcels = widened_parcels.clone();

        let document_id = document_by_case
            .get(case.id.as_str())
            .ok_or_else(|| {
                GeoAssessmentRollOwnerError::invalid(
                    "Geo assessment-roll owner request is missing a case document binding",
                    [("case_id", case.id.as_str())],
                )
            })?
            .clone();
        let party_rows = parties_by_document
            .get(document_id.as_str())
            .cloned()
            .unwrap_or_default();
        let borrower_norms = party_rows
            .iter()
            .map(|party| party.party_name_norm.clone())
            .collect::<BTreeSet<_>>();
        let mut exact_lots = Vec::new();
        let mut affiliate_lots = Vec::new();
        if !borrower_norms.is_empty() {
            for parcel_id in &widened_parcels {
                let match_kind = roll_by_bbl
                    .get(parcel_id)
                    .map(|row| assessment_roll_owner_match(&row.owner, &borrower_norms))
                    .unwrap_or(GeoAssessmentRollOwnerMatch::NoOwner);
                match match_kind {
                    GeoAssessmentRollOwnerMatch::Exact => exact_lots.push(parcel_id.clone()),
                    GeoAssessmentRollOwnerMatch::Token => affiliate_lots.push(parcel_id.clone()),
                    GeoAssessmentRollOwnerMatch::NoOwner | GeoAssessmentRollOwnerMatch::None => {}
                }
            }
        }

        let mut contracts = Vec::new();
        let mut observations = Vec::new();
        if !exact_lots.is_empty() {
            contracts.push(exact_contract.clone());
            observations.push(exact_observation(
                &case.id,
                &widened_parcels,
                &exact_lots,
                &party_rows,
                &roll_by_bbl,
            )?);
            summary.exact_hard_observations =
                checked_inc(summary.exact_hard_observations, "exact_hard_observations")?;
            summary.exact_match_lots = checked_add(
                summary.exact_match_lots,
                exact_lots.len() as u64,
                "exact_match_lots",
            )?;
        }
        if !affiliate_lots.is_empty() {
            contracts.push(affiliate_contract.clone());
            for lot in &affiliate_lots {
                observations.push(affiliate_observation(
                    &case.id,
                    lot,
                    &affiliate_lots,
                    &party_rows,
                    &roll_by_bbl,
                )?);
            }
            summary.affiliate_soft_observations = checked_add(
                summary.affiliate_soft_observations,
                affiliate_lots.len() as u64,
                "affiliate_soft_observations",
            )?;
            summary.affiliate_preference_lots = checked_add(
                summary.affiliate_preference_lots,
                affiliate_lots.len() as u64,
                "affiliate_preference_lots",
            )?;
        }
        if !observations.is_empty() {
            summary.owner_overlay_cases =
                checked_inc(summary.owner_overlay_cases, "owner_overlay_cases")?;
            overlays.push(GeoPopulationCaseEvidenceOverlay {
                case_id: case.id.clone(),
                expected_base_evidence_blake3: None,
                contracts,
                observations,
            });
        }

        summaries.push(GeoAssessmentRollOwnerCaseSummary {
            case_id: case.id.clone(),
            document_id,
            input_universe_parcels: input_parcels.len() as u64,
            widened_universe_parcels: widened_parcels.len() as u64,
            added_roll_lots: added_roll_lots as u64,
            party_rows: party_rows.len() as u64,
            exact_match_lots: exact_lots.len() as u64,
            affiliate_preference_lots: affiliate_lots.len() as u64,
            hard_observation_emitted: !exact_lots.is_empty(),
            soft_observations_emitted: affiliate_lots.len() as u64,
        });
    }

    let overlay = GeoPopulationEvidenceStackRequest {
        version: CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION.to_string(),
        max_overlay_cases: widened_population.cases.len(),
        max_overlay_observations: request.max_overlay_observations,
        case_overlays: overlays,
    };
    let produced_observations = overlay_observation_count(&overlay)?;
    if produced_observations > request.max_overlay_observations {
        return Err(GeoAssessmentRollOwnerError::budget(
            "Geo assessment-roll owner overlay exceeds its declared observation budget",
            [
                ("observations", produced_observations.to_string()),
                (
                    "max_overlay_observations",
                    request.max_overlay_observations.to_string(),
                ),
            ],
        ));
    }

    let artifact = GeoAssessmentRollOwnerArtifact {
        version: CANON_GEO_ASSESSMENT_ROLL_OWNER_VERSION.to_string(),
        request_version: request.version.clone(),
        request_blake3,
        proof_class: request.proof_class,
        summary,
        cases: summaries,
        widened_population,
        overlay,
    };
    validate_assessment_roll_owner_artifact(&artifact)?;
    Ok(artifact)
}

pub fn canonicalize_assessment_roll_owner_request(
    request: &GeoAssessmentRollOwnerRequest,
) -> Result<GeoAssessmentRollOwnerRequest, GeoAssessmentRollOwnerError> {
    if request.version != CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION {
        return Err(GeoAssessmentRollOwnerError::new(
            GeoAssessmentRollOwnerErrorCode::UnsupportedVersion,
            "Unsupported Geo assessment-roll owner request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION),
            ],
        ));
    }
    if request.max_cases == 0 || request.population.cases.len() > request.max_cases {
        return Err(GeoAssessmentRollOwnerError::budget(
            "Geo assessment-roll owner request exceeds its case budget",
            [
                ("cases", request.population.cases.len().to_string()),
                ("max_cases", request.max_cases.to_string()),
            ],
        ));
    }
    if request.max_roll_rows == 0 || request.roll_rows.len() > request.max_roll_rows {
        return Err(GeoAssessmentRollOwnerError::budget(
            "Geo assessment-roll owner request exceeds its roll-row budget",
            [
                ("roll_rows", request.roll_rows.len().to_string()),
                ("max_roll_rows", request.max_roll_rows.to_string()),
            ],
        ));
    }
    if request.party_rows.len() > request.max_party_rows {
        return Err(GeoAssessmentRollOwnerError::budget(
            "Geo assessment-roll owner request exceeds its party-row budget",
            [
                ("party_rows", request.party_rows.len().to_string()),
                ("max_party_rows", request.max_party_rows.to_string()),
            ],
        ));
    }
    if request.max_overlay_observations == 0 {
        return Err(GeoAssessmentRollOwnerError::budget(
            "Geo assessment-roll owner request requires a positive observation budget",
            [("max_overlay_observations", "0")],
        ));
    }
    validate_identifier(
        "contract_source.source_dataset",
        &request.contract_source.source_dataset,
    )?;
    validate_identifier(
        "contract_source.source_release",
        &request.contract_source.source_release,
    )?;
    validate_identifier(
        "calibration.population_id",
        &request.calibration.population_id,
    )?;
    validate_unprefixed_blake3(
        "calibration.calibration_blake3",
        &request.calibration.calibration_blake3,
    )?;
    validate_identifier(
        "calibration.exact_falsification_rule_id",
        &request.calibration.exact_falsification_rule_id,
    )?;
    validate_identifier(
        "calibration.affiliate_falsification_rule_id",
        &request.calibration.affiliate_falsification_rule_id,
    )?;

    let mut canonical = request.clone();
    canonical.population = canonicalize_population(&canonical.population)?;
    sort_distinct_strings(
        "contract_source.source_lineage_ids",
        &mut canonical.contract_source.source_lineage_ids,
    )?;
    if canonical.contract_source.source_lineage_ids.is_empty() {
        return Err(GeoAssessmentRollOwnerError::invalid_field(
            "contract_source.source_lineage_ids",
            "[]",
        ));
    }
    for binding in &canonical.case_documents {
        validate_identifier("case_documents[].case_id", &binding.case_id)?;
        validate_identifier("case_documents[].document_id", &binding.document_id)?;
    }
    canonical
        .case_documents
        .sort_by(|left, right| left.case_id.cmp(&right.case_id));
    reject_adjacent_duplicates(
        "case_documents[].case_id",
        canonical
            .case_documents
            .iter()
            .map(|binding| binding.case_id.as_str()),
    )?;
    validate_case_document_coverage(&canonical)?;

    for row in &canonical.roll_rows {
        validate_bbl("roll_rows[].bbl", &row.bbl)?;
        validate_identifier("roll_rows[].source_record_id", &row.source_record_id)?;
        validate_identifier("roll_rows[].source_vintage", &row.source_vintage)?;
    }
    canonical.roll_rows.sort_by(|left, right| {
        (left.bbl.as_str(), left.source_record_id.as_str())
            .cmp(&(right.bbl.as_str(), right.source_record_id.as_str()))
    });
    reject_adjacent_duplicates(
        "roll_rows[].bbl",
        canonical.roll_rows.iter().map(|row| row.bbl.as_str()),
    )?;

    for row in &canonical.party_rows {
        validate_identifier("party_rows[].document_id", &row.document_id)?;
        validate_identifier("party_rows[].party_type", &row.party_type)?;
        if row.party_type != "1" {
            return Err(GeoAssessmentRollOwnerError::invalid(
                "Geo assessment-roll owner stage accepts only ACRIS party_type 1 rows",
                [
                    ("field", "party_rows[].party_type"),
                    ("value", row.party_type.as_str()),
                ],
            ));
        }
        validate_identifier("party_rows[].party_name_norm", &row.party_name_norm)?;
        let normalized = normalize_assessment_roll_owner_name(&row.party_name_norm);
        if normalized != row.party_name_norm {
            return Err(GeoAssessmentRollOwnerError::invalid(
                "ACRIS party names must already equal STG_GEO_NYC_ACRIS_PARTIES.PARTY_NAME_NORM",
                [
                    ("field", "party_rows[].party_name_norm"),
                    ("value", row.party_name_norm.as_str()),
                    ("normalized", normalized.as_str()),
                ],
            ));
        }
        validate_identifier("party_rows[].source_record_id", &row.source_record_id)?;
        validate_identifier("party_rows[].source_vintage", &row.source_vintage)?;
    }
    canonical.party_rows.sort_by(|left, right| {
        (
            left.document_id.as_str(),
            left.party_name_norm.as_str(),
            left.source_record_id.as_str(),
        )
            .cmp(&(
                right.document_id.as_str(),
                right.party_name_norm.as_str(),
                right.source_record_id.as_str(),
            ))
    });
    reject_adjacent_duplicates(
        "party_rows[].source_record_id",
        canonical
            .party_rows
            .iter()
            .map(|row| row.source_record_id.as_str()),
    )?;

    Ok(canonical)
}

pub fn canonical_assessment_roll_owner_bytes(
    artifact: &GeoAssessmentRollOwnerArtifact,
) -> Result<Vec<u8>, GeoAssessmentRollOwnerError> {
    validate_assessment_roll_owner_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoAssessmentRollOwnerError::invalid(
            "Geo assessment-roll owner artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn validate_assessment_roll_owner_artifact(
    artifact: &GeoAssessmentRollOwnerArtifact,
) -> Result<(), GeoAssessmentRollOwnerError> {
    if artifact.version != CANON_GEO_ASSESSMENT_ROLL_OWNER_VERSION
        || artifact.request_version != CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION
    {
        return Err(GeoAssessmentRollOwnerError::new(
            GeoAssessmentRollOwnerErrorCode::UnsupportedVersion,
            "Unsupported Geo assessment-roll owner artifact version",
            [
                ("actual_version", artifact.version.as_str()),
                ("actual_request_version", artifact.request_version.as_str()),
            ],
        ));
    }
    validate_prefixed_blake3("request_blake3", &artifact.request_blake3)?;
    if artifact.widened_population.version
        != super::evaluation::CANON_GEO_POPULATION_REQUEST_VERSION
    {
        return Err(GeoAssessmentRollOwnerError::invalid_field(
            "widened_population.version",
            artifact.widened_population.version.as_str(),
        ));
    }
    if artifact.overlay.version != CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION {
        return Err(GeoAssessmentRollOwnerError::invalid_field(
            "overlay.version",
            artifact.overlay.version.as_str(),
        ));
    }
    if artifact.cases.len() != artifact.widened_population.cases.len() {
        return Err(GeoAssessmentRollOwnerError::invalid(
            "Geo assessment-roll owner artifact case summaries must match widened population cases",
            [
                ("cases", artifact.cases.len().to_string()),
                (
                    "widened_population.cases",
                    artifact.widened_population.cases.len().to_string(),
                ),
            ],
        ));
    }
    reject_adjacent_duplicates(
        "cases[].case_id",
        artifact.cases.iter().map(|case| case.case_id.as_str()),
    )?;
    reject_adjacent_duplicates(
        "overlay.case_overlays[].case_id",
        artifact
            .overlay
            .case_overlays
            .iter()
            .map(|overlay| overlay.case_id.as_str()),
    )?;
    validate_overlay_observations_compile(&artifact.widened_population, &artifact.overlay)?;
    Ok(())
}

pub fn normalize_assessment_roll_owner_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character == ' ' {
                ' '
            } else if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn assessment_roll_owner_tokens(value: &str) -> Vec<String> {
    let stop_words = STOP_WORDS.iter().copied().collect::<BTreeSet<_>>();
    normalize_assessment_roll_owner_name(value)
        .split_whitespace()
        .filter(|token| !stop_words.contains(token))
        .map(ToOwned::to_owned)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoAssessmentRollOwnerMatch {
    NoOwner,
    Exact,
    Token,
    None,
}

pub fn assessment_roll_owner_match(
    owner: &str,
    borrower_party_name_norms: &BTreeSet<String>,
) -> GeoAssessmentRollOwnerMatch {
    let owner_norm = normalize_assessment_roll_owner_name(owner);
    if owner_norm.is_empty() {
        return GeoAssessmentRollOwnerMatch::NoOwner;
    }
    if borrower_party_name_norms.contains(&owner_norm) {
        return GeoAssessmentRollOwnerMatch::Exact;
    }
    let owner_tokens = assessment_roll_owner_tokens(owner)
        .into_iter()
        .collect::<BTreeSet<_>>();
    for borrower in borrower_party_name_norms {
        let borrower_tokens = assessment_roll_owner_tokens(borrower)
            .into_iter()
            .collect::<BTreeSet<_>>();
        if owner_tokens.is_empty() || borrower_tokens.is_empty() {
            continue;
        }
        let shared = owner_tokens.intersection(&borrower_tokens).count();
        if owner_tokens.is_subset(&borrower_tokens)
            || borrower_tokens.is_subset(&owner_tokens)
            || shared >= 2
        {
            return GeoAssessmentRollOwnerMatch::Token;
        }
    }
    GeoAssessmentRollOwnerMatch::None
}

fn exact_contract(request: &GeoAssessmentRollOwnerRequest) -> GeoRhoContract {
    GeoRhoContract {
        id: GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID.to_string(),
        version: OWNER_CONTRACT_VERSION.to_string(),
        source_dataset: request.contract_source.source_dataset.clone(),
        source_release: request.contract_source.source_release.clone(),
        source_lineage_ids: request.contract_source.source_lineage_ids.clone(),
        method_id: EXACT_METHOD_ID.to_string(),
        method_version: OWNER_METHOD_VERSION.to_string(),
        claim_role: GeoEvidenceClaimRole::StableIdentityAnchor,
        basis: GeoRhoBasis::EmpiricalCalibration {
            population_id: request.calibration.population_id.clone(),
            calibration_blake3: request.calibration.calibration_blake3.clone(),
            falsification_rule_id: request.calibration.exact_falsification_rule_id.clone(),
            admissible_hard_band: true,
        },
    }
}

fn affiliate_contract(request: &GeoAssessmentRollOwnerRequest) -> GeoRhoContract {
    GeoRhoContract {
        id: GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID.to_string(),
        version: OWNER_CONTRACT_VERSION.to_string(),
        source_dataset: request.contract_source.source_dataset.clone(),
        source_release: request.contract_source.source_release.clone(),
        source_lineage_ids: request.contract_source.source_lineage_ids.clone(),
        method_id: AFFILIATE_METHOD_ID.to_string(),
        method_version: OWNER_METHOD_VERSION.to_string(),
        claim_role: GeoEvidenceClaimRole::StableIdentityAnchor,
        basis: GeoRhoBasis::EmpiricalCalibration {
            population_id: request.calibration.population_id.clone(),
            calibration_blake3: request.calibration.calibration_blake3.clone(),
            falsification_rule_id: request.calibration.affiliate_falsification_rule_id.clone(),
            admissible_hard_band: false,
        },
    }
}

fn exact_observation(
    case_id: &str,
    parcels: &[String],
    exact_lots: &[String],
    party_rows: &[&GeoAssessmentRollPartyRow],
    roll_by_bbl: &BTreeMap<String, &GeoAssessmentRollLotRow>,
) -> Result<GeoRhoObservation, GeoAssessmentRollOwnerError> {
    let exact = exact_lots.iter().cloned().collect::<BTreeSet<_>>();
    let values = parcels
        .iter()
        .map(|parcel| GeoIntegerMemberValue {
            id: parcel.clone(),
            value: if exact.contains(parcel) { 0 } else { 1 },
        })
        .collect::<Vec<_>>();
    Ok(GeoRhoObservation {
        id: format!("{GEO_ASSESSMENT_ROLL_OWNER_EXACT_OBSERVATION_PREFIX}:{case_id}"),
        contract_id: GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID.to_string(),
        source_records: owner_source_records(party_rows, parcels, roll_by_bbl)?,
        valid_time: None,
        observation: GeoRhoObservationKind::IntegerSumBand {
            level: GeoEntityLevel::Parcel,
            measure: GeoIntegerMeasure {
                semantic_id: OWNER_NOT_EXACT_MEASURE_ID.to_string(),
                unit: OWNER_NOT_EXACT_UNIT.to_string(),
                value_origin: GeoIntegerValueOrigin::SourceAsserted,
            },
            values,
            min: 0,
            max: 0,
        },
    })
}

fn affiliate_observation(
    case_id: &str,
    lot: &str,
    affiliate_lots: &[String],
    party_rows: &[&GeoAssessmentRollPartyRow],
    roll_by_bbl: &BTreeMap<String, &GeoAssessmentRollLotRow>,
) -> Result<GeoRhoObservation, GeoAssessmentRollOwnerError> {
    Ok(GeoRhoObservation {
        id: format!("{GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_OBSERVATION_PREFIX}:{case_id}:{lot}"),
        contract_id: GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID.to_string(),
        source_records: owner_source_records(party_rows, affiliate_lots, roll_by_bbl)?,
        valid_time: None,
        observation: GeoRhoObservationKind::PreferMember {
            member: GeoEntityRef::new(GeoEntityLevel::Parcel, lot),
            cost_if_absent: 1,
        },
    })
}

fn owner_source_records(
    party_rows: &[&GeoAssessmentRollPartyRow],
    roll_bbls: &[String],
    roll_by_bbl: &BTreeMap<String, &GeoAssessmentRollLotRow>,
) -> Result<Vec<GeoEvidenceRecordRef>, GeoAssessmentRollOwnerError> {
    let mut records = BTreeMap::<String, GeoEvidenceRecordRef>::new();
    for party in party_rows {
        insert_source_record(&mut records, party_source_record(party)?)?;
    }
    for bbl in roll_bbls {
        let Some(row) = roll_by_bbl.get(bbl) else {
            continue;
        };
        insert_source_record(&mut records, roll_source_record(row)?)?;
    }
    Ok(records.into_values().collect())
}

fn insert_source_record(
    records: &mut BTreeMap<String, GeoEvidenceRecordRef>,
    record: GeoEvidenceRecordRef,
) -> Result<(), GeoAssessmentRollOwnerError> {
    if let Some(previous) = records.insert(record.source_record_id.clone(), record.clone())
        && previous != record
    {
        return Err(GeoAssessmentRollOwnerError::new(
            GeoAssessmentRollOwnerErrorCode::SourceRecordCollision,
            "Geo assessment-roll owner source records collide by id with different payloads",
            [("source_record_id", record.source_record_id)],
        ));
    }
    Ok(())
}

fn roll_source_record(
    row: &GeoAssessmentRollLotRow,
) -> Result<GeoEvidenceRecordRef, GeoAssessmentRollOwnerError> {
    Ok(GeoEvidenceRecordRef {
        source_record_id: row.source_record_id.clone(),
        source_vintage: row.source_vintage.clone(),
        record_blake3: digest_unprefixed(&roll_row_payload(row))?,
    })
}

fn party_source_record(
    row: &GeoAssessmentRollPartyRow,
) -> Result<GeoEvidenceRecordRef, GeoAssessmentRollOwnerError> {
    Ok(GeoEvidenceRecordRef {
        source_record_id: row.source_record_id.clone(),
        source_vintage: row.source_vintage.clone(),
        record_blake3: digest_unprefixed(&party_row_payload(row))?,
    })
}

fn roll_row_payload(row: &GeoAssessmentRollLotRow) -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        ("bbl", row.bbl.clone()),
        ("condo_number", row.condo_number.clone()),
        ("gross_sqft", row.gross_sqft.clone()),
        ("owner", row.owner.clone()),
        ("units", row.units.clone()),
    ])
}

fn party_row_payload(row: &GeoAssessmentRollPartyRow) -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        ("document_id", row.document_id.clone()),
        ("party_name_norm", row.party_name_norm.clone()),
        ("party_type", row.party_type.clone()),
    ])
}

fn widened_parcel_universe(
    input: &[String],
    roll_by_block: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<String>, GeoAssessmentRollOwnerError> {
    let mut widened = input.iter().cloned().collect::<BTreeSet<_>>();
    for parcel in input {
        validate_bbl("population.cases[].evidence.universe.parcels[]", parcel)?;
        let block = parcel_block(parcel)?;
        if let Some(block_roll_lots) = roll_by_block.get(block) {
            widened.extend(block_roll_lots.iter().cloned());
        }
    }
    Ok(widened.into_iter().collect())
}

fn roll_rows_by_bbl(
    rows: &[GeoAssessmentRollLotRow],
) -> Result<BTreeMap<String, &GeoAssessmentRollLotRow>, GeoAssessmentRollOwnerError> {
    let mut by_bbl = BTreeMap::new();
    for row in rows {
        if by_bbl.insert(row.bbl.clone(), row).is_some() {
            return Err(GeoAssessmentRollOwnerError::invalid(
                "Geo assessment-roll owner rows must be unique by BBL",
                [("bbl", row.bbl.as_str())],
            ));
        }
    }
    Ok(by_bbl)
}

fn roll_rows_by_block(
    rows: &[GeoAssessmentRollLotRow],
) -> Result<BTreeMap<String, Vec<String>>, GeoAssessmentRollOwnerError> {
    let mut by_block = BTreeMap::<String, Vec<String>>::new();
    for row in rows {
        by_block
            .entry(parcel_block(&row.bbl)?.to_string())
            .or_default()
            .push(row.bbl.clone());
    }
    for rows in by_block.values_mut() {
        rows.sort();
        rows.dedup();
    }
    Ok(by_block)
}

fn party_rows_by_document(
    rows: &[GeoAssessmentRollPartyRow],
) -> BTreeMap<&str, Vec<&GeoAssessmentRollPartyRow>> {
    let mut by_document = BTreeMap::<&str, Vec<&GeoAssessmentRollPartyRow>>::new();
    for row in rows {
        by_document
            .entry(row.document_id.as_str())
            .or_default()
            .push(row);
    }
    by_document
}

fn case_documents_by_case(
    request: &GeoAssessmentRollOwnerRequest,
) -> Result<BTreeMap<&str, String>, GeoAssessmentRollOwnerError> {
    let mut by_case = BTreeMap::new();
    for binding in &request.case_documents {
        if by_case
            .insert(binding.case_id.as_str(), binding.document_id.clone())
            .is_some()
        {
            return Err(GeoAssessmentRollOwnerError::invalid(
                "Geo assessment-roll owner request has duplicate case document bindings",
                [("case_id", binding.case_id.as_str())],
            ));
        }
    }
    Ok(by_case)
}

fn ensure_parcel_population_case(
    case: &GeoLabeledCompositionCase,
) -> Result<(), GeoAssessmentRollOwnerError> {
    if case.evidence.version != CANON_GEO_EVIDENCE_REQUEST_VERSION {
        return Err(GeoAssessmentRollOwnerError::invalid_field(
            "population.cases[].evidence.version",
            case.evidence.version.as_str(),
        ));
    }
    if case.evidence.profile.selection_level != GeoEntityLevel::Parcel {
        let selection_level = format!("{:?}", case.evidence.profile.selection_level);
        return Err(GeoAssessmentRollOwnerError::invalid(
            "Geo assessment-roll owner stage supports parcel-selected populations only",
            [
                ("case_id", case.id.as_str()),
                ("selection_level", selection_level.as_str()),
            ],
        ));
    }
    if !case.evidence.universe.buildings.is_empty() || !case.truth.buildings.is_empty() {
        return Err(GeoAssessmentRollOwnerError::invalid(
            "Geo assessment-roll owner stage supports parcel-only universes and truth labels",
            [("case_id", case.id.as_str())],
        ));
    }
    Ok(())
}

fn canonicalize_population(
    population: &GeoPopulationEvaluationRequest,
) -> Result<GeoPopulationEvaluationRequest, GeoAssessmentRollOwnerError> {
    if population.version != super::evaluation::CANON_GEO_POPULATION_REQUEST_VERSION {
        return Err(GeoAssessmentRollOwnerError::invalid_field(
            "population.version",
            population.version.as_str(),
        ));
    }
    if population.max_cases == 0 || population.cases.len() > population.max_cases {
        return Err(GeoAssessmentRollOwnerError::budget(
            "Geo assessment-roll owner population exceeds its declared case budget",
            [
                ("cases", population.cases.len().to_string()),
                ("max_cases", population.max_cases.to_string()),
            ],
        ));
    }
    let mut canonical = population.clone();
    for case in &mut canonical.cases {
        validate_identifier("population.cases[].id", &case.id)?;
        ensure_parcel_population_case(case)?;
        canonicalize_evidence_request(&mut case.evidence)?;
        canonicalize_model(&mut case.truth);
    }
    canonical
        .cases
        .sort_by(|left, right| left.id.cmp(&right.id));
    reject_adjacent_duplicates(
        "population.cases[].id",
        canonical.cases.iter().map(|case| case.id.as_str()),
    )?;
    Ok(canonical)
}

fn canonicalize_evidence_request(
    request: &mut GeoEvidenceCompilationRequest,
) -> Result<(), GeoAssessmentRollOwnerError> {
    if request.profile != GeoCompositionProfile::default() {
        request.profile.version =
            super::composition::CANON_GEO_COMPOSITION_PROFILE_VERSION.to_string();
    }
    request.universe = GeoCompositionUniverse {
        parcels: sorted_unique(
            request.universe.parcels.clone(),
            "evidence.universe.parcels",
        )?,
        buildings: {
            let mut buildings = request.universe.buildings.clone();
            for building in &mut buildings {
                building.parcel_ids = sorted_unique(
                    building.parcel_ids.clone(),
                    "evidence.universe.buildings[].parcel_ids",
                )?;
            }
            buildings.sort_by(|left, right| left.id.cmp(&right.id));
            buildings
        },
    };
    request
        .contracts
        .sort_by(|left, right| left.id.cmp(&right.id));
    request
        .observations
        .sort_by(|left, right| left.id.cmp(&right.id));
    Ok(())
}

fn canonicalize_model(model: &mut GeoCompositionModel) {
    model.parcels.sort();
    model.parcels.dedup();
    model.buildings.sort();
    model.buildings.dedup();
}

fn validate_case_document_coverage(
    request: &GeoAssessmentRollOwnerRequest,
) -> Result<(), GeoAssessmentRollOwnerError> {
    let cases = request
        .population
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let bindings = request
        .case_documents
        .iter()
        .map(|binding| binding.case_id.as_str())
        .collect::<BTreeSet<_>>();
    if cases != bindings {
        let missing = cases
            .difference(&bindings)
            .copied()
            .collect::<Vec<_>>()
            .join(",");
        let extra = bindings
            .difference(&cases)
            .copied()
            .collect::<Vec<_>>()
            .join(",");
        return Err(GeoAssessmentRollOwnerError::invalid(
            "Geo assessment-roll owner case-document bindings must exactly cover population cases",
            [("missing", missing), ("extra", extra)],
        ));
    }
    Ok(())
}

fn sorted_unique(
    mut values: Vec<String>,
    field: &str,
) -> Result<Vec<String>, GeoAssessmentRollOwnerError> {
    for value in &values {
        validate_identifier(field, value)?;
    }
    values.sort();
    reject_adjacent_duplicates(field, values.iter().map(String::as_str))?;
    Ok(values)
}

fn sort_distinct_strings(
    field: &'static str,
    values: &mut [String],
) -> Result<(), GeoAssessmentRollOwnerError> {
    for value in values.iter() {
        validate_identifier(field, value)?;
    }
    values.sort();
    reject_adjacent_duplicates(field, values.iter().map(String::as_str))
}

fn reject_adjacent_duplicates<'a>(
    field: &str,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<(), GeoAssessmentRollOwnerError> {
    let mut previous = None;
    for value in values {
        if previous == Some(value) {
            return Err(GeoAssessmentRollOwnerError::invalid(
                "Geo assessment-roll owner input contains a duplicate value",
                [("field", field.to_string()), ("value", value.to_string())],
            ));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoAssessmentRollOwnerError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoAssessmentRollOwnerError::invalid_field(field, value));
    }
    Ok(())
}

fn validate_bbl(field: &str, value: &str) -> Result<(), GeoAssessmentRollOwnerError> {
    if value.len() != 10 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(GeoAssessmentRollOwnerError::invalid(
            "Geo assessment-roll owner BBL values must be 10 ASCII digits",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn parcel_block(parcel: &str) -> Result<&str, GeoAssessmentRollOwnerError> {
    validate_bbl("bbl", parcel)?;
    Ok(&parcel[..6])
}

fn validate_unprefixed_blake3(field: &str, value: &str) -> Result<(), GeoAssessmentRollOwnerError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoAssessmentRollOwnerError::invalid(
            "Geo assessment-roll owner BLAKE3 digests must be lowercase hexadecimal",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_prefixed_blake3(field: &str, value: &str) -> Result<(), GeoAssessmentRollOwnerError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoAssessmentRollOwnerError::invalid(
            "Geo assessment-roll owner BLAKE3 digests must use blake3:<hex>",
            [("field", field), ("value", value)],
        ));
    };
    validate_unprefixed_blake3(field, hex)
}

fn digest_unprefixed<T: Serialize>(value: &T) -> Result<String, GeoAssessmentRollOwnerError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        GeoAssessmentRollOwnerError::invalid(
            "Geo assessment-roll owner payload could not be serialized for hashing",
            [("error", error.to_string())],
        )
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn digest_prefixed<T: Serialize>(value: &T) -> Result<String, GeoAssessmentRollOwnerError> {
    digest_unprefixed(value).map(|digest| format!("blake3:{digest}"))
}

fn overlay_observation_count(
    overlay: &GeoPopulationEvidenceStackRequest,
) -> Result<usize, GeoAssessmentRollOwnerError> {
    overlay
        .case_overlays
        .iter()
        .try_fold(0usize, |count, case| {
            count
                .checked_add(case.observations.len())
                .ok_or_else(|| GeoAssessmentRollOwnerError::overflow("overlay_observations"))
        })
}

fn checked_inc(value: u64, field: &str) -> Result<u64, GeoAssessmentRollOwnerError> {
    value
        .checked_add(1)
        .ok_or_else(|| GeoAssessmentRollOwnerError::overflow(field))
}

fn checked_add(left: u64, right: u64, field: &str) -> Result<u64, GeoAssessmentRollOwnerError> {
    left.checked_add(right)
        .ok_or_else(|| GeoAssessmentRollOwnerError::overflow(field))
}

fn validate_overlay_observations_compile(
    population: &GeoPopulationEvaluationRequest,
    overlay: &GeoPopulationEvidenceStackRequest,
) -> Result<(), GeoAssessmentRollOwnerError> {
    if overlay.case_overlays.is_empty() {
        return Ok(());
    }
    let stacked = stack_population_evidence(population, overlay).map_err(|error| {
        let mut detail = error.detail;
        detail.insert("stack_code".to_string(), format!("{:?}", error.code));
        GeoAssessmentRollOwnerError {
            code: GeoAssessmentRollOwnerErrorCode::Evidence,
            message: error.message,
            detail,
        }
    })?;
    for case in &stacked.population.cases {
        compile_evidence(&case.evidence).map_err(|error| {
            let mut detail = error.detail;
            detail.insert("evidence_code".to_string(), format!("{:?}", error.code));
            GeoAssessmentRollOwnerError {
                code: GeoAssessmentRollOwnerErrorCode::Evidence,
                message: error.message,
                detail,
            }
        })?;
    }
    Ok(())
}
