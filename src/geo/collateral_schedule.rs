#![forbid(unsafe_code)]

//! Collateral-set completeness evidence for Geo composition.
//!
//! This adapter turns a source-authored complete member set into the generic
//! `AllOf` plus `ExactCardinality` rho observations. Source-specific code may
//! decide that an ACRIS legal-row document, SEC annex schedule, or county
//! recorder schedule asserts completeness; this module only models the typed
//! relation and preserves the source pins.

use super::{
    composition::{GeoEntityLevel, GeoEntityRef},
    evidence::{
        GeoCollateralCompletenessAssertion, GeoCollateralCompletenessObservationRequest,
        GeoEvidenceClaimRole, GeoEvidenceRecordRef, GeoRhoBasis, GeoRhoContract,
        collateral_completeness_observations,
    },
    stack::{
        CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION, GeoPopulationCaseEvidenceOverlay,
        GeoPopulationEvidenceStackRequest,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, error::Error, fmt};

pub const GEO_ACRIS_DOCUMENT_LEGAL_COMPLETENESS_CONTRACT_ID: &str =
    "rho.collateral.acris_document_legal_complete";
const ACRIS_DOCUMENT_LEGAL_COMPLETENESS_METHOD_ID: &str = "acris-document-legal-completeness";
const ACRIS_DOCUMENT_LEGAL_COMPLETENESS_METHOD_VERSION: &str = "1.0.0";
const ACRIS_DOCUMENT_LEGAL_COMPLETENESS_CONTRACT_VERSION: &str = "1.0.0";
const ACRIS_DOCUMENT_LEGAL_COMPLETENESS_INVARIANT_ID: &str =
    "bound-acris-document-legal-rows-enumerate-complete-instrument-parcel-set";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCollateralCompletenessContractSource {
    pub source_dataset: String,
    pub source_release: String,
    pub source_lineage_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCollateralCompletenessCaseAssertion {
    pub case_id: String,
    pub level: GeoEntityLevel,
    pub members: Vec<GeoEntityRef>,
    pub source_records: Vec<GeoEvidenceRecordRef>,
    pub assertion: GeoCollateralCompletenessAssertion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCollateralCompletenessOverlayRequest {
    pub contract_id: String,
    pub contract_version: String,
    pub contract_source: GeoCollateralCompletenessContractSource,
    pub method_id: String,
    pub method_version: String,
    pub invariant_id: String,
    pub case_assertions: Vec<GeoCollateralCompletenessCaseAssertion>,
    pub max_overlay_cases: usize,
    pub max_overlay_observations: usize,
}

impl GeoCollateralCompletenessOverlayRequest {
    pub fn acris_document_legal(
        contract_source: GeoCollateralCompletenessContractSource,
        case_assertions: Vec<GeoCollateralCompletenessCaseAssertion>,
        max_overlay_cases: usize,
        max_overlay_observations: usize,
    ) -> Self {
        Self {
            contract_id: GEO_ACRIS_DOCUMENT_LEGAL_COMPLETENESS_CONTRACT_ID.to_string(),
            contract_version: ACRIS_DOCUMENT_LEGAL_COMPLETENESS_CONTRACT_VERSION.to_string(),
            contract_source,
            method_id: ACRIS_DOCUMENT_LEGAL_COMPLETENESS_METHOD_ID.to_string(),
            method_version: ACRIS_DOCUMENT_LEGAL_COMPLETENESS_METHOD_VERSION.to_string(),
            invariant_id: ACRIS_DOCUMENT_LEGAL_COMPLETENESS_INVARIANT_ID.to_string(),
            case_assertions,
            max_overlay_cases,
            max_overlay_observations,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCollateralCompletenessOverlaySummary {
    pub requested_cases: u64,
    pub asserted_complete_cases: u64,
    pub abstained_cases: u64,
    pub emitted_observations: u64,
    pub emitted_source_records: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCollateralCompletenessOverlayArtifact {
    pub summary: GeoCollateralCompletenessOverlaySummary,
    pub stack_request: Option<GeoPopulationEvidenceStackRequest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCollateralCompletenessErrorCode {
    InvalidInput,
    BudgetExceeded,
    Evidence,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCollateralCompletenessError {
    pub code: GeoCollateralCompletenessErrorCode,
    pub message: String,
    pub detail: std::collections::BTreeMap<String, String>,
}

impl GeoCollateralCompletenessError {
    fn new(
        code: GeoCollateralCompletenessErrorCode,
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
            GeoCollateralCompletenessErrorCode::InvalidInput,
            message,
            detail,
        )
    }

    fn overflow(field: &str) -> Self {
        Self::new(
            GeoCollateralCompletenessErrorCode::ArithmeticOverflow,
            "Geo collateral completeness accounting overflowed",
            [("field", field)],
        )
    }
}

impl fmt::Display for GeoCollateralCompletenessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoCollateralCompletenessError {}

pub fn build_collateral_completeness_overlay(
    request: &GeoCollateralCompletenessOverlayRequest,
) -> Result<GeoCollateralCompletenessOverlayArtifact, GeoCollateralCompletenessError> {
    validate_overlay_request(request)?;
    let contract = completeness_contract(request);
    let mut case_assertions = request.case_assertions.clone();
    case_assertions.sort_by(|left, right| left.case_id.cmp(&right.case_id));

    let mut case_ids = BTreeSet::new();
    let mut case_overlays = Vec::new();
    let mut asserted_complete_cases = 0_u64;
    let mut abstained_cases = 0_u64;
    let mut emitted_observations = 0_u64;
    let mut emitted_source_records = 0_u64;

    for mut assertion in case_assertions {
        if !case_ids.insert(assertion.case_id.clone()) {
            return Err(GeoCollateralCompletenessError::invalid(
                "Geo collateral completeness assertions must be one per case",
                [("case_id", assertion.case_id.as_str())],
            ));
        }
        assertion.source_records.sort();
        assertion.members.sort_by(|left, right| {
            (left.level, left.id.as_str()).cmp(&(right.level, right.id.as_str()))
        });

        let observations =
            collateral_completeness_observations(&GeoCollateralCompletenessObservationRequest {
                observation_id_prefix: format!("obs.{}:{}", request.contract_id, assertion.case_id),
                contract_id: request.contract_id.clone(),
                source_records: assertion.source_records.clone(),
                level: assertion.level,
                members: assertion.members,
                assertion: assertion.assertion,
            })
            .map_err(|error| {
                GeoCollateralCompletenessError::new(
                    GeoCollateralCompletenessErrorCode::Evidence,
                    error.message,
                    error.detail,
                )
            })?;

        if observations.is_empty() {
            abstained_cases = checked_add(abstained_cases, 1, "abstained_cases")?;
            continue;
        }
        asserted_complete_cases =
            checked_add(asserted_complete_cases, 1, "asserted_complete_cases")?;
        emitted_observations = checked_add(
            emitted_observations,
            to_u64(observations.len(), "emitted_observations")?,
            "emitted_observations",
        )?;
        emitted_source_records = checked_add(
            emitted_source_records,
            observations.iter().try_fold(0_u64, |count, observation| {
                checked_add(
                    count,
                    to_u64(observation.source_records.len(), "emitted_source_records")?,
                    "emitted_source_records",
                )
            })?,
            "emitted_source_records",
        )?;
        case_overlays.push(GeoPopulationCaseEvidenceOverlay {
            case_id: assertion.case_id,
            expected_base_evidence_blake3: None,
            contracts: vec![contract.clone()],
            observations,
        });
    }

    if emitted_observations as usize > request.max_overlay_observations {
        return Err(GeoCollateralCompletenessError::new(
            GeoCollateralCompletenessErrorCode::BudgetExceeded,
            "Geo collateral completeness overlay exceeds its observation budget",
            [
                ("observations", emitted_observations.to_string()),
                (
                    "max_overlay_observations",
                    request.max_overlay_observations.to_string(),
                ),
            ],
        ));
    }

    let stack_request = if case_overlays.is_empty() {
        None
    } else {
        Some(GeoPopulationEvidenceStackRequest {
            version: CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION.to_string(),
            case_overlays,
            max_overlay_cases: request.max_overlay_cases,
            max_overlay_observations: request.max_overlay_observations,
        })
    };

    Ok(GeoCollateralCompletenessOverlayArtifact {
        summary: GeoCollateralCompletenessOverlaySummary {
            requested_cases: to_u64(request.case_assertions.len(), "requested_cases")?,
            asserted_complete_cases,
            abstained_cases,
            emitted_observations,
            emitted_source_records,
        },
        stack_request,
    })
}

fn completeness_contract(request: &GeoCollateralCompletenessOverlayRequest) -> GeoRhoContract {
    let mut source_lineage_ids = request.contract_source.source_lineage_ids.clone();
    source_lineage_ids.sort();
    GeoRhoContract {
        id: request.contract_id.clone(),
        version: request.contract_version.clone(),
        source_dataset: request.contract_source.source_dataset.clone(),
        source_release: request.contract_source.source_release.clone(),
        source_lineage_ids,
        method_id: request.method_id.clone(),
        method_version: request.method_version.clone(),
        claim_role: GeoEvidenceClaimRole::AttributeObservation,
        basis: GeoRhoBasis::LogicalRelaxation {
            invariant_id: request.invariant_id.clone(),
        },
    }
}

fn validate_overlay_request(
    request: &GeoCollateralCompletenessOverlayRequest,
) -> Result<(), GeoCollateralCompletenessError> {
    validate_identifier("contract_id", &request.contract_id)?;
    validate_identifier("contract_version", &request.contract_version)?;
    validate_identifier(
        "contract_source.source_dataset",
        &request.contract_source.source_dataset,
    )?;
    validate_identifier(
        "contract_source.source_release",
        &request.contract_source.source_release,
    )?;
    if request.contract_source.source_lineage_ids.is_empty() {
        return Err(GeoCollateralCompletenessError::invalid(
            "Geo collateral completeness source requires at least one lineage id",
            [("field", "contract_source.source_lineage_ids")],
        ));
    }
    let mut previous_lineage: Option<&str> = None;
    for lineage_id in &request.contract_source.source_lineage_ids {
        validate_identifier("contract_source.source_lineage_ids[]", lineage_id)?;
        if previous_lineage.is_some_and(|previous| previous >= lineage_id.as_str()) {
            return Err(GeoCollateralCompletenessError::invalid(
                "Geo collateral completeness lineage ids must be sorted and distinct",
                [("lineage_id", lineage_id.as_str())],
            ));
        }
        previous_lineage = Some(lineage_id);
    }
    validate_identifier("method_id", &request.method_id)?;
    validate_identifier("method_version", &request.method_version)?;
    validate_identifier("invariant_id", &request.invariant_id)?;
    if request.case_assertions.is_empty() {
        return Err(GeoCollateralCompletenessError::invalid(
            "Geo collateral completeness overlay requires at least one case assertion",
            [("field", "case_assertions")],
        ));
    }
    if request.max_overlay_cases == 0 || request.case_assertions.len() > request.max_overlay_cases {
        return Err(GeoCollateralCompletenessError::new(
            GeoCollateralCompletenessErrorCode::BudgetExceeded,
            "Geo collateral completeness overlay exceeds its case budget",
            [
                ("cases", request.case_assertions.len().to_string()),
                ("max_overlay_cases", request.max_overlay_cases.to_string()),
            ],
        ));
    }
    if request.max_overlay_observations == 0 {
        return Err(GeoCollateralCompletenessError::new(
            GeoCollateralCompletenessErrorCode::BudgetExceeded,
            "Geo collateral completeness overlay requires a positive observation budget",
            [("max_overlay_observations", "0")],
        ));
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoCollateralCompletenessError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoCollateralCompletenessError::invalid(
            "Geo collateral completeness identifiers must be non-empty and canonical",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn to_u64(value: usize, field: &str) -> Result<u64, GeoCollateralCompletenessError> {
    u64::try_from(value).map_err(|_| GeoCollateralCompletenessError::overflow(field))
}

fn checked_add(left: u64, right: u64, field: &str) -> Result<u64, GeoCollateralCompletenessError> {
    left.checked_add(right)
        .ok_or_else(|| GeoCollateralCompletenessError::overflow(field))
}
