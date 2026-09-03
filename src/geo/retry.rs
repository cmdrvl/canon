#![forbid(unsafe_code)]

//! Retry-population fixtures and bounded Geo reacquisition loop artifacts.

use crate::geo::{
    GeoAcquisitionReceipt, GeoAcquisitionRequest, GeoRun, GeoRunStatus,
    geo_acquisition_request_semantic_hash, validate_geo_acquisition_receipt,
    validate_geo_acquisition_request, validate_geo_run,
};
use h3o::CellIndex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    str::FromStr,
};

pub const CANON_GEO_RETRY_LOOP_VERSION: &str = "canon_geo_retry_loop.v0";
pub const CANON_GEO_POINT_POPULATION_VERSION: &str = "canon_geo_point_population.v0";

const NYC_MIN_LON_E7: i64 = -743_000_000;
const NYC_MAX_LON_E7: i64 = -736_500_000;
const NYC_MIN_LAT_E7: i64 = 404_500_000;
const NYC_MAX_LAT_E7: i64 = 410_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointPopulationReleasePin {
    pub source_dataset: String,
    pub source_release: String,
    pub release_dt: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointPopulationArtifact {
    pub version: String,
    pub population_id: String,
    pub source_dataset: String,
    pub selection_query_sha256: String,
    pub release_pins: Vec<GeoPointPopulationReleasePin>,
    pub points: Vec<GeoPointPopulationPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointPopulationPoint {
    pub point_id: String,
    pub subject_id: String,
    pub loan_key: String,
    pub asserted_address_blake3: String,
    pub landed_geocode: GeoPointPopulationGeocode,
    pub home_cell_r9: String,
    pub refuter_fired: bool,
    pub e1_failure_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pip_lot_bbl: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pad_billing_bbl_candidates: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad_unit_bbl: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_equals_pip: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointPopulationGeocode {
    pub lon_e7: i64,
    pub lat_e7: i64,
    pub accuracy_type: String,
    pub source_attribution: String,
    pub geocode_asof: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRetryPolicy {
    pub max_passes: u8,
    pub regeocode_request_template: GeoAcquisitionRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRetryPass {
    pub index: u8,
    pub plan_blake3: String,
    pub run_blake3: String,
    pub abstention_reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regeocode: Option<GeoAcquisitionRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_blake3: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoRetryTerminal {
    Resolved,
    AbstainedAtCeiling,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRetryLoopArtifact {
    pub version: String,
    pub subject_id: String,
    pub policy: GeoRetryPolicy,
    pub passes: Vec<GeoRetryPass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<GeoRetryTerminal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoRetryErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    RetryPassCeiling,
    RetryPolicyUnbounded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRetryError {
    pub code: GeoRetryErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoRetryError {
    fn new(
        code: GeoRetryErrorCode,
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
        Self::new(GeoRetryErrorCode::InvalidInput, message, detail)
    }

    fn unsupported_version(actual: &str) -> Self {
        Self::new(
            GeoRetryErrorCode::UnsupportedVersion,
            "Geo retry loop artifact declares an unsupported version",
            [
                ("expected", CANON_GEO_RETRY_LOOP_VERSION.to_string()),
                ("actual", actual.to_string()),
            ],
        )
    }

    fn policy_unbounded() -> Self {
        Self::new(
            GeoRetryErrorCode::RetryPolicyUnbounded,
            "Geo retry policy must declare a positive bounded max_passes value",
            [("field", "max_passes")],
        )
    }
}

impl fmt::Display for GeoRetryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoRetryError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPointPopulationErrorCode {
    UnsupportedVersion,
    InvalidInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPointPopulationError {
    pub code: GeoPointPopulationErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoPointPopulationError {
    fn new(
        code: GeoPointPopulationErrorCode,
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
        Self::new(GeoPointPopulationErrorCode::InvalidInput, message, detail)
    }
}

impl fmt::Display for GeoPointPopulationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoPointPopulationError {}

pub fn next_retry_pass(
    loop_state: &GeoRetryLoopArtifact,
    latest_run: &GeoRun,
) -> Result<Option<GeoAcquisitionRequest>, GeoRetryError> {
    validate_retry_loop_artifact(loop_state)?;
    validate_latest_run(latest_run)?;

    if loop_state.terminal.is_some() {
        return Ok(None);
    }
    if loop_state.passes.len() >= usize::from(loop_state.policy.max_passes) {
        return Ok(None);
    }
    match latest_run_disposition(latest_run) {
        RetryRunDisposition::Abstained(_) => {
            Ok(Some(loop_state.policy.regeocode_request_template.clone()))
        }
        RetryRunDisposition::Resolved | RetryRunDisposition::Blocked(_) => Ok(None),
    }
}

pub fn record_pass(
    loop_state: &mut GeoRetryLoopArtifact,
    run: &GeoRun,
    receipt: Option<&GeoAcquisitionReceipt>,
) -> Result<(), GeoRetryError> {
    validate_retry_loop_artifact(loop_state)?;
    validate_latest_run(run)?;
    if loop_state.terminal.is_some() {
        return Err(GeoRetryError::invalid(
            "Geo retry loop is already terminal",
            [("field", "terminal")],
        ));
    }
    if loop_state
        .passes
        .iter()
        .any(|pass| pass.run_blake3 == run.semantic_hash)
    {
        return Err(GeoRetryError::invalid(
            "Geo retry loop cannot record the same run semantic hash twice",
            [
                ("field", "passes[].run_blake3".to_string()),
                ("duplicate_run_blake3", run.semantic_hash.clone()),
            ],
        ));
    }

    let disposition = latest_run_disposition(run);
    if loop_state.passes.len() >= usize::from(loop_state.policy.max_passes) {
        return match disposition {
            RetryRunDisposition::Abstained(_) => {
                loop_state.terminal = Some(GeoRetryTerminal::AbstainedAtCeiling);
                validate_retry_loop_artifact(loop_state)
            }
            RetryRunDisposition::Resolved => {
                loop_state.terminal = Some(GeoRetryTerminal::Resolved);
                validate_retry_loop_artifact(loop_state)
            }
            RetryRunDisposition::Blocked(_) => {
                loop_state.terminal = Some(GeoRetryTerminal::Blocked);
                validate_retry_loop_artifact(loop_state)
            }
        };
    }

    let request = loop_state.policy.regeocode_request_template.clone();
    let receipt_blake3 = match receipt {
        Some(receipt) => Some(validate_receipt_for_retry_request(&request, receipt)?),
        None => None,
    };
    let abstention_reason = match disposition {
        RetryRunDisposition::Abstained(reason) => reason,
        RetryRunDisposition::Resolved => "resolved".to_string(),
        RetryRunDisposition::Blocked(reason) => reason,
    };
    let index = u8::try_from(loop_state.passes.len() + 1).map_err(|_| {
        GeoRetryError::new(
            GeoRetryErrorCode::ArithmeticOverflow,
            "Geo retry pass index exceeded u8 range",
            [("field", "passes[].index")],
        )
    })?;

    loop_state.passes.push(GeoRetryPass {
        index,
        plan_blake3: run.plan_ref.semantic_hash.clone(),
        run_blake3: run.semantic_hash.clone(),
        abstention_reason,
        regeocode: Some(request),
        receipt_blake3,
    });

    match latest_run_disposition(run) {
        RetryRunDisposition::Resolved => loop_state.terminal = Some(GeoRetryTerminal::Resolved),
        RetryRunDisposition::Blocked(_) => loop_state.terminal = Some(GeoRetryTerminal::Blocked),
        RetryRunDisposition::Abstained(_) => {
            if loop_state.passes.len() >= usize::from(loop_state.policy.max_passes) {
                loop_state.terminal = Some(GeoRetryTerminal::AbstainedAtCeiling);
            }
        }
    }

    validate_retry_loop_artifact(loop_state)
}

pub fn validate_retry_loop_artifact(artifact: &GeoRetryLoopArtifact) -> Result<(), GeoRetryError> {
    if artifact.version != CANON_GEO_RETRY_LOOP_VERSION {
        return Err(GeoRetryError::unsupported_version(&artifact.version));
    }
    validate_retry_string("subject_id", &artifact.subject_id)?;
    validate_retry_policy(&artifact.policy)?;
    if artifact.passes.len() > usize::from(artifact.policy.max_passes) {
        return Err(GeoRetryError::invalid(
            "Geo retry loop recorded more passes than its bounded policy allows",
            [
                ("field", "passes".to_string()),
                ("max_passes", artifact.policy.max_passes.to_string()),
                ("passes", artifact.passes.len().to_string()),
            ],
        ));
    }
    validate_retry_passes(artifact)?;
    if artifact.terminal.is_none()
        && artifact.passes.len() >= usize::from(artifact.policy.max_passes)
    {
        return Err(GeoRetryError::new(
            GeoRetryErrorCode::RetryPassCeiling,
            "Geo retry loop reached its pass ceiling without a terminal marker",
            [
                ("field", "terminal".to_string()),
                ("last_abstention_reason", last_abstention_reason(artifact)),
            ],
        ));
    }
    if artifact.terminal == Some(GeoRetryTerminal::AbstainedAtCeiling)
        && artifact.passes.len() != usize::from(artifact.policy.max_passes)
    {
        return Err(GeoRetryError::invalid(
            "Geo retry ceiling terminal must be recorded exactly at max_passes",
            [
                ("field", "terminal".to_string()),
                ("max_passes", artifact.policy.max_passes.to_string()),
                ("passes", artifact.passes.len().to_string()),
            ],
        ));
    }
    Ok(())
}

pub fn canonical_retry_loop_bytes(
    artifact: &GeoRetryLoopArtifact,
) -> Result<Vec<u8>, GeoRetryError> {
    validate_retry_loop_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoRetryError::invalid(
            "Geo retry loop artifact could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn validate_point_population_artifact(
    artifact: &GeoPointPopulationArtifact,
) -> Result<(), GeoPointPopulationError> {
    if artifact.version != CANON_GEO_POINT_POPULATION_VERSION {
        return Err(GeoPointPopulationError::new(
            GeoPointPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo point-population artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_POINT_POPULATION_VERSION),
            ],
        ));
    }
    validate_point_population_string("population_id", &artifact.population_id)?;
    validate_point_population_string("source_dataset", &artifact.source_dataset)?;
    if !artifact.source_dataset.starts_with("fixture.") {
        return Err(point_invalid_field(
            "source_dataset",
            "Geo point-population fixtures must declare a fixture source dataset",
            artifact.source_dataset.as_str(),
        ));
    }
    validate_sha256("selection_query_sha256", &artifact.selection_query_sha256)?;
    validate_release_pins(&artifact.release_pins)?;
    if artifact.points.is_empty() {
        return Err(point_invalid_field(
            "points",
            "Geo point-population artifacts must contain at least one point",
            "0",
        ));
    }

    let mut previous_point_id: Option<&str> = None;
    for point in &artifact.points {
        validate_point(point)?;
        if let Some(previous) = previous_point_id
            && previous >= point.point_id.as_str()
        {
            return Err(GeoPointPopulationError::invalid(
                "Geo point-population points must be strictly sorted by point_id",
                [
                    ("field", "points[].point_id".to_string()),
                    ("previous_point_id", previous.to_string()),
                    ("point_id", point.point_id.clone()),
                ],
            ));
        }
        previous_point_id = Some(point.point_id.as_str());
    }

    Ok(())
}

pub fn canonical_point_population_bytes(
    artifact: &GeoPointPopulationArtifact,
) -> Result<Vec<u8>, GeoPointPopulationError> {
    validate_point_population_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoPointPopulationError::invalid(
            "Geo point-population artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

fn validate_retry_policy(policy: &GeoRetryPolicy) -> Result<(), GeoRetryError> {
    if policy.max_passes == 0 {
        return Err(GeoRetryError::policy_unbounded());
    }
    validate_geo_acquisition_request(&policy.regeocode_request_template).map_err(|error| {
        GeoRetryError::invalid(
            "Geo retry policy contains an invalid acquisition request template",
            [
                ("field", "regeocode_request_template".to_string()),
                ("source_code", format!("{:?}", error.code)),
                ("source_message", error.message),
            ],
        )
    })
}

fn validate_retry_passes(artifact: &GeoRetryLoopArtifact) -> Result<(), GeoRetryError> {
    let mut run_hashes = BTreeSet::new();
    for (position, pass) in artifact.passes.iter().enumerate() {
        let expected_index = u8::try_from(position + 1).map_err(|_| {
            GeoRetryError::new(
                GeoRetryErrorCode::ArithmeticOverflow,
                "Geo retry pass index exceeded u8 range",
                [("field", "passes[].index")],
            )
        })?;
        if pass.index != expected_index {
            return Err(GeoRetryError::invalid(
                "Geo retry passes must be indexed sequentially from one",
                [
                    ("field", "passes[].index".to_string()),
                    ("expected", expected_index.to_string()),
                    ("actual", pass.index.to_string()),
                ],
            ));
        }
        validate_prefixed_blake3("passes[].plan_blake3", &pass.plan_blake3)?;
        validate_prefixed_blake3("passes[].run_blake3", &pass.run_blake3)?;
        validate_retry_string("passes[].abstention_reason", &pass.abstention_reason)?;
        if !run_hashes.insert(pass.run_blake3.as_str()) {
            return Err(GeoRetryError::invalid(
                "Geo retry passes must not repeat a run semantic hash",
                [
                    ("field", "passes[].run_blake3".to_string()),
                    ("duplicate_run_blake3", pass.run_blake3.clone()),
                ],
            ));
        }
        if let Some(request) = &pass.regeocode {
            validate_geo_acquisition_request(request).map_err(|error| {
                GeoRetryError::invalid(
                    "Geo retry pass contains an invalid acquisition request",
                    [
                        ("field", "passes[].regeocode".to_string()),
                        ("source_code", format!("{:?}", error.code)),
                        ("source_message", error.message),
                    ],
                )
            })?;
        }
        if let Some(receipt_blake3) = &pass.receipt_blake3 {
            validate_prefixed_blake3("passes[].receipt_blake3", receipt_blake3)?;
        }
    }
    Ok(())
}

fn validate_latest_run(run: &GeoRun) -> Result<(), GeoRetryError> {
    validate_geo_run(run).map_err(|error| {
        GeoRetryError::invalid(
            "Geo retry loop requires a valid GeoRun",
            [
                ("field", "latest_run".to_string()),
                ("source_code", format!("{:?}", error.code)),
                ("source_message", error.message),
            ],
        )
    })
}

fn validate_receipt_for_retry_request(
    request: &GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
) -> Result<String, GeoRetryError> {
    let receipt_blake3 = prefixed_hash(&serde_json::to_vec(receipt).map_err(|error| {
        GeoRetryError::invalid(
            "Geo retry acquisition receipt could not be serialized",
            [("serde_error", error.to_string())],
        )
    })?);
    let expected_request_hash =
        geo_acquisition_request_semantic_hash(request).map_err(|error| {
            GeoRetryError::invalid(
                "Geo retry acquisition request template could not be hashed",
                [
                    ("field", "regeocode_request_template".to_string()),
                    ("source_code", format!("{:?}", error.code)),
                    ("source_message", error.message),
                ],
            )
        })?;
    if receipt.request_semantic_hash != expected_request_hash {
        return Err(GeoRetryError::invalid(
            "Geo retry receipt request_semantic_hash does not match the emitted acquisition request",
            [
                ("field", "receipt.request_semantic_hash".to_string()),
                ("receipt_blake3", receipt_blake3),
                ("expected", expected_request_hash),
                ("actual", receipt.request_semantic_hash.clone()),
            ],
        ));
    }
    validate_geo_acquisition_receipt(request, receipt).map_err(|error| {
        GeoRetryError::invalid(
            "Geo retry receipt does not satisfy the emitted acquisition request",
            [
                ("field", "receipt".to_string()),
                ("receipt_blake3", receipt_blake3.clone()),
                ("source_code", format!("{:?}", error.code)),
                ("source_message", error.message),
            ],
        )
    })?;
    Ok(receipt_blake3)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RetryRunDisposition {
    Resolved,
    Abstained(String),
    Blocked(String),
}

fn latest_run_disposition(run: &GeoRun) -> RetryRunDisposition {
    match run.status {
        GeoRunStatus::Completed => RetryRunDisposition::Resolved,
        GeoRunStatus::Abstained => RetryRunDisposition::Abstained(
            first_blocker_id(run).unwrap_or_else(|| "abstained:ambiguous_residual".to_string()),
        ),
        GeoRunStatus::WaitingForInput | GeoRunStatus::Partial | GeoRunStatus::UnsupportedGrain => {
            first_blocker_id(run).map_or_else(
                || RetryRunDisposition::Blocked(status_reason(run.status)),
                RetryRunDisposition::Abstained,
            )
        }
        GeoRunStatus::Failed
        | GeoRunStatus::Cancelled
        | GeoRunStatus::BudgetFallback
        | GeoRunStatus::Contradicted => RetryRunDisposition::Blocked(status_reason(run.status)),
    }
}

fn first_blocker_id(run: &GeoRun) -> Option<String> {
    run.blockers
        .first()
        .map(|blocker| blocker.blocker_id.clone())
}

fn status_reason(status: GeoRunStatus) -> String {
    match status {
        GeoRunStatus::Completed => "resolved",
        GeoRunStatus::Partial => "partial",
        GeoRunStatus::WaitingForInput => "waiting_for_input",
        GeoRunStatus::UnsupportedGrain => "unsupported_grain",
        GeoRunStatus::Failed => "failed",
        GeoRunStatus::Cancelled => "cancelled",
        GeoRunStatus::BudgetFallback => "budget_fallback",
        GeoRunStatus::Abstained => "abstained",
        GeoRunStatus::Contradicted => "contradicted",
    }
    .to_string()
}

fn last_abstention_reason(artifact: &GeoRetryLoopArtifact) -> String {
    artifact
        .passes
        .last()
        .map(|pass| pass.abstention_reason.clone())
        .unwrap_or_default()
}

fn validate_retry_string(field: &'static str, value: &str) -> Result<(), GeoRetryError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoRetryError::invalid(
            "Geo retry string fields must be non-empty and canonical-trimmed",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn validate_prefixed_blake3(field: &'static str, value: &str) -> Result<(), GeoRetryError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(invalid_retry_blake3(field, value));
    };
    if hex.len() != 64
        || !hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        || hex.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return Err(invalid_retry_blake3(field, value));
    }
    Ok(())
}

fn invalid_retry_blake3(field: &'static str, value: &str) -> GeoRetryError {
    GeoRetryError::invalid(
        "Geo retry digest fields must be blake3-prefixed lowercase fixed-width hex",
        [("field", field.to_string()), ("value", value.to_string())],
    )
}

fn prefixed_hash(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn validate_release_pins(
    release_pins: &[GeoPointPopulationReleasePin],
) -> Result<(), GeoPointPopulationError> {
    if release_pins.is_empty() {
        return Err(point_invalid_field(
            "release_pins",
            "Geo point-population artifacts must pin at least one source release",
            "0",
        ));
    }
    let mut previous_key: Option<String> = None;
    for pin in release_pins {
        validate_point_population_string("release_pins[].source_dataset", &pin.source_dataset)?;
        validate_point_population_string("release_pins[].source_release", &pin.source_release)?;
        validate_point_population_string("release_pins[].release_dt", &pin.release_dt)?;
        validate_point_population_string("release_pins[].role", &pin.role)?;
        if let Some(variant) = &pin.variant {
            validate_point_population_string("release_pins[].variant", variant)?;
        }
        let key = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            pin.source_dataset,
            pin.source_release,
            pin.release_dt,
            pin.variant.as_deref().unwrap_or("")
        );
        if let Some(previous) = &previous_key
            && previous >= &key
        {
            return Err(GeoPointPopulationError::invalid(
                "Geo point-population release pins must be strictly sorted and unique",
                [
                    ("field", "release_pins".to_string()),
                    ("source_dataset", pin.source_dataset.clone()),
                ],
            ));
        }
        previous_key = Some(key);
    }
    Ok(())
}

fn validate_point(point: &GeoPointPopulationPoint) -> Result<(), GeoPointPopulationError> {
    validate_point_population_string("points[].point_id", &point.point_id)?;
    validate_point_population_string("points[].subject_id", &point.subject_id)?;
    validate_point_population_string("points[].loan_key", &point.loan_key)?;
    validate_blake3(
        "points[].asserted_address_blake3",
        &point.asserted_address_blake3,
    )?;
    validate_point_population_string(
        "points[].landed_geocode.accuracy_type",
        &point.landed_geocode.accuracy_type,
    )?;
    validate_point_population_string(
        "points[].landed_geocode.source_attribution",
        &point.landed_geocode.source_attribution,
    )?;
    validate_point_population_string(
        "points[].landed_geocode.geocode_asof",
        &point.landed_geocode.geocode_asof,
    )?;
    validate_nyc_e7_bbox(point)?;
    validate_home_cell_r9(point)?;

    match point.e1_failure_class.as_str() {
        "gross" => validate_gross_point(point),
        "condo" => validate_condo_point(point),
        _ => Err(GeoPointPopulationError::invalid(
            "Geo point-population E1 failure class is unsupported",
            [
                ("field", "points[].e1_failure_class".to_string()),
                ("point_id", point.point_id.clone()),
                ("value", point.e1_failure_class.clone()),
            ],
        )),
    }
}

fn validate_gross_point(point: &GeoPointPopulationPoint) -> Result<(), GeoPointPopulationError> {
    if point.pip_lot_bbl.is_some()
        || !point.pad_billing_bbl_candidates.is_empty()
        || point.pad_unit_bbl.is_some()
        || point.block.is_some()
        || point.billing_equals_pip.is_some()
    {
        return Err(GeoPointPopulationError::invalid(
            "Gross E1 points must not carry condo crosswalk fields",
            [
                ("field", "points[].condo_fields".to_string()),
                ("point_id", point.point_id.clone()),
            ],
        ));
    }
    Ok(())
}

fn validate_condo_point(point: &GeoPointPopulationPoint) -> Result<(), GeoPointPopulationError> {
    validate_required_option(point, "points[].pip_lot_bbl", point.pip_lot_bbl.as_deref())?;
    validate_required_option(
        point,
        "points[].pad_unit_bbl",
        point.pad_unit_bbl.as_deref(),
    )?;
    validate_required_option(point, "points[].block", point.block.as_deref())?;
    if point.billing_equals_pip.is_none() {
        return Err(GeoPointPopulationError::invalid(
            "Condo E1 points must carry billing_equals_pip",
            [
                ("field", "points[].billing_equals_pip".to_string()),
                ("point_id", point.point_id.clone()),
            ],
        ));
    }
    if point.pad_billing_bbl_candidates.is_empty() {
        return Err(GeoPointPopulationError::invalid(
            "Condo E1 points must carry at least one PAD billing BBL candidate",
            [
                ("field", "points[].pad_billing_bbl_candidates".to_string()),
                ("point_id", point.point_id.clone()),
            ],
        ));
    }
    let mut previous: Option<&str> = None;
    for candidate in &point.pad_billing_bbl_candidates {
        validate_point_population_string("points[].pad_billing_bbl_candidates[]", candidate)?;
        if let Some(previous_candidate) = previous
            && previous_candidate >= candidate.as_str()
        {
            return Err(GeoPointPopulationError::invalid(
                "PAD billing BBL candidates must be strictly sorted and unique",
                [
                    ("field", "points[].pad_billing_bbl_candidates".to_string()),
                    ("point_id", point.point_id.clone()),
                    ("value", candidate.clone()),
                ],
            ));
        }
        previous = Some(candidate.as_str());
    }
    Ok(())
}

fn validate_required_option(
    point: &GeoPointPopulationPoint,
    field: &'static str,
    value: Option<&str>,
) -> Result<(), GeoPointPopulationError> {
    match value {
        Some(value) => validate_point_population_string(field, value),
        None => Err(GeoPointPopulationError::invalid(
            "Condo E1 points must carry the PAD condo crosswalk fields",
            [
                ("field", field.to_string()),
                ("point_id", point.point_id.clone()),
            ],
        )),
    }
}

fn validate_nyc_e7_bbox(point: &GeoPointPopulationPoint) -> Result<(), GeoPointPopulationError> {
    let lon = point.landed_geocode.lon_e7;
    let lat = point.landed_geocode.lat_e7;
    if !(NYC_MIN_LON_E7..=NYC_MAX_LON_E7).contains(&lon)
        || !(NYC_MIN_LAT_E7..=NYC_MAX_LAT_E7).contains(&lat)
    {
        return Err(GeoPointPopulationError::invalid(
            "Geo point-population landed geocode is outside the NYC bounding box",
            [
                ("field", "landed_geocode".to_string()),
                ("point_id", point.point_id.clone()),
                ("lon_e7", lon.to_string()),
                ("lat_e7", lat.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_home_cell_r9(point: &GeoPointPopulationPoint) -> Result<(), GeoPointPopulationError> {
    let cell = CellIndex::from_str(&point.home_cell_r9).map_err(|error| {
        GeoPointPopulationError::invalid(
            "Geo point-population home_cell_r9 is not a valid H3 cell",
            [
                ("field", "home_cell_r9".to_string()),
                ("point_id", point.point_id.clone()),
                ("value", point.home_cell_r9.clone()),
                ("error", error.to_string()),
            ],
        )
    })?;
    if u8::from(cell.resolution()) != 9 {
        return Err(GeoPointPopulationError::invalid(
            "Geo point-population home_cell_r9 must be resolution 9",
            [
                ("field", "home_cell_r9".to_string()),
                ("point_id", point.point_id.clone()),
                ("value", point.home_cell_r9.clone()),
            ],
        ));
    }
    Ok(())
}

fn validate_point_population_string(
    field: &'static str,
    value: &str,
) -> Result<(), GeoPointPopulationError> {
    if value.is_empty() || value.trim() != value {
        return Err(point_invalid_field(
            field,
            "Geo point-population string fields must be non-empty and canonical-trimmed",
            value,
        ));
    }
    Ok(())
}

fn validate_blake3(field: &'static str, value: &str) -> Result<(), GeoPointPopulationError> {
    validate_hex_hash(field, value, 64)
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), GeoPointPopulationError> {
    validate_hex_hash(field, value, 64)
}

fn validate_hex_hash(
    field: &'static str,
    value: &str,
    length: usize,
) -> Result<(), GeoPointPopulationError> {
    if value.len() != length
        || !value.chars().all(|ch| ch.is_ascii_hexdigit())
        || value.chars().any(|ch| ch.is_ascii_uppercase())
    {
        return Err(point_invalid_field(
            field,
            "Geo point-population hash fields must be lowercase fixed-width hex",
            value,
        ));
    }
    Ok(())
}

fn point_invalid_field(
    field: &'static str,
    message: impl Into<String>,
    value: &str,
) -> GeoPointPopulationError {
    GeoPointPopulationError::invalid(
        message,
        [("field", field.to_string()), ("value", value.to_string())],
    )
}
