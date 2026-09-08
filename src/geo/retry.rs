#![forbid(unsafe_code)]

//! Retry-population fixtures and bounded Geo reacquisition loop artifacts.

use crate::geo::{
    CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION, GeoAcquisitionProofClass, GeoAcquisitionReceipt,
    GeoAcquisitionRequest, GeoDigestAlgorithm, GeoRun, GeoRunStatus,
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
pub const CANON_GEO_RETRY_RECOVERY_VERSION: &str = "canon_geo_retry_recovery.v0";

const NYC_MIN_LON_E7: i64 = -743_000_000;
const NYC_MAX_LON_E7: i64 = -736_500_000;
const NYC_MIN_LAT_E7: i64 = 404_500_000;
const NYC_MAX_LAT_E7: i64 = 410_000_000;
const G4_RETRY_RECOVERY_DENOMINATOR: usize = 40;
const PROVIDER_RESPONSE_BYTES_DIGEST_ID: &str = "provider_response_bytes";
const GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID: &str = "geocode_candidate_rows";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRetryRecovery {
    pub version: String,
    pub population_blake3: String,
    pub policy: GeoRetryPolicy,
    pub denominator: u64,
    pub per_point: Vec<GeoRetryRecoveryPoint>,
    pub recovered: u64,
    pub abstained_at_ceiling: u64,
    pub blocked: u64,
    pub by_provider: BTreeMap<String, u64>,
    pub receipts_with_provider_request_id: u64,
    pub receipts_without_provider_request_id: u64,
    pub precision_claim: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRetryRecoveryPoint {
    pub point_id: String,
    pub passes: u8,
    pub terminal: GeoRetryTerminal,
    pub recovered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_recovering_pass: Option<u8>,
    pub receipt_semantic_hashes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_home_cell: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landed_home_cell: Option<String>,
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
    RetryRecoveryDenominatorMismatch,
    RetryReceiptUnbound,
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

pub fn measure_recovery(
    population: &GeoPointPopulationArtifact,
    loops: &[GeoRetryLoopArtifact],
    runs: &BTreeMap<String, GeoRun>,
    receipts: &BTreeMap<String, GeoAcquisitionReceipt>,
) -> Result<GeoRetryRecovery, GeoRetryError> {
    let population_blake3 = retry_population_blake3(population)?;
    if population.points.len() != G4_RETRY_RECOVERY_DENOMINATOR {
        return Err(recovery_denominator_mismatch(
            "Geo retry recovery G4 measurement requires the frozen 40-point gross-class denominator",
            G4_RETRY_RECOVERY_DENOMINATOR,
            population.points.len(),
            None,
            None,
        ));
    }

    let mut population_by_subject = BTreeMap::<String, &GeoPointPopulationPoint>::new();
    for point in &population.points {
        if population_by_subject
            .insert(point.subject_id.clone(), point)
            .is_some()
        {
            return Err(recovery_denominator_mismatch(
                "Geo retry recovery population subject ids must be unique",
                population.points.len(),
                population_by_subject.len(),
                Some(point.subject_id.as_str()),
                Some(point.point_id.as_str()),
            ));
        }
    }

    let mut policy: Option<GeoRetryPolicy> = None;
    let mut loops_by_subject = BTreeMap::<String, &GeoRetryLoopArtifact>::new();
    for loop_state in loops {
        validate_retry_loop_artifact(loop_state)?;
        if !population_by_subject.contains_key(&loop_state.subject_id) {
            return Err(recovery_denominator_mismatch(
                "Geo retry recovery loop subject is outside the frozen population",
                population.points.len(),
                loops.len(),
                Some(loop_state.subject_id.as_str()),
                None,
            ));
        }
        if loops_by_subject
            .insert(loop_state.subject_id.clone(), loop_state)
            .is_some()
        {
            return Err(recovery_denominator_mismatch(
                "Geo retry recovery loops must be unique by subject id",
                population.points.len(),
                loops.len(),
                Some(loop_state.subject_id.as_str()),
                None,
            ));
        }
        match &policy {
            Some(expected) if expected.max_passes != loop_state.policy.max_passes => {
                return Err(GeoRetryError::invalid(
                    "Geo retry recovery loops must share one bounded pass policy",
                    [
                        ("subject_id".to_string(), loop_state.subject_id.clone()),
                        (
                            "expected_max_passes".to_string(),
                            expected.max_passes.to_string(),
                        ),
                        (
                            "actual_max_passes".to_string(),
                            loop_state.policy.max_passes.to_string(),
                        ),
                    ],
                ));
            }
            Some(_) => {}
            None => policy = Some(loop_state.policy.clone()),
        }
    }

    if loops_by_subject.len() != population.points.len() {
        let missing = population
            .points
            .iter()
            .find(|point| !loops_by_subject.contains_key(&point.subject_id))
            .expect("loop count mismatch implies at least one missing subject");
        return Err(recovery_denominator_mismatch(
            "Geo retry recovery requires exactly one terminal loop per population point",
            population.points.len(),
            loops_by_subject.len(),
            Some(missing.subject_id.as_str()),
            Some(missing.point_id.as_str()),
        ));
    }
    let policy = policy.ok_or_else(|| {
        recovery_denominator_mismatch(
            "Geo retry recovery requires a non-empty loop set",
            population.points.len(),
            loops_by_subject.len(),
            None,
            None,
        )
    })?;

    let retained_or_live_receipts_present = receipts
        .values()
        .any(|receipt| receipt.proof_class != GeoAcquisitionProofClass::Fixture);
    let mut per_point = Vec::with_capacity(population.points.len());
    let mut recovered_count = 0_u64;
    let mut abstained_count = 0_u64;
    let mut blocked_count = 0_u64;
    let mut by_provider = BTreeMap::<String, u64>::new();
    let mut receipts_with_provider_request_id = 0_u64;
    let mut receipts_without_provider_request_id = 0_u64;

    for point in &population.points {
        let loop_state = loops_by_subject
            .get(&point.subject_id)
            .expect("loop presence checked above");
        let terminal = loop_state.terminal.ok_or_else(|| {
            GeoRetryError::invalid(
                "Geo retry recovery requires terminal retry loops",
                [
                    ("subject_id", loop_state.subject_id.as_str()),
                    ("point_id", point.point_id.as_str()),
                    ("field", "terminal"),
                ],
            )
        })?;
        let passes = u8::try_from(loop_state.passes.len()).map_err(|_| {
            GeoRetryError::new(
                GeoRetryErrorCode::ArithmeticOverflow,
                "Geo retry recovery pass count exceeded u8 range",
                [("field", "passes")],
            )
        })?;
        let mut receipt_semantic_hashes = Vec::new();
        let mut first_recovering_pass = None;
        let mut final_run: Option<&GeoRun> = None;

        for pass in &loop_state.passes {
            let run = runs.get(&pass.run_blake3).ok_or_else(|| {
                GeoRetryError::invalid(
                    "Geo retry recovery requires every pass run_blake3 to be supplied",
                    [
                        ("point_id".to_string(), point.point_id.clone()),
                        ("pass".to_string(), pass.index.to_string()),
                        ("run_blake3".to_string(), pass.run_blake3.clone()),
                    ],
                )
            })?;
            validate_latest_run(run)?;
            final_run = Some(run);
            if let Some(request) = &pass.regeocode {
                let request_hash = retry_acquisition_request_hash(request, point, pass)?;
                let receipt = receipts.get(&request_hash).ok_or_else(|| {
                    receipt_unbound(
                        point.point_id.as_str(),
                        pass.index,
                        "Geo retry recovery pass has no receipt keyed by the emitted acquisition request",
                        [
                            ("expected", request_hash.clone()),
                            ("actual", "missing".to_string()),
                        ],
                    )
                })?;
                validate_recovery_receipt_binding(
                    point,
                    pass,
                    request,
                    receipt,
                    retained_or_live_receipts_present,
                )?;
                receipt_semantic_hashes.push(request_hash);
                if let Some(executor) = &receipt.executor {
                    increment_count(
                        by_provider.entry(executor.executor_id.clone()).or_insert(0),
                        "by_provider",
                    )?;
                    increment_count(
                        &mut receipts_with_provider_request_id,
                        "receipts_with_provider_request_id",
                    )?;
                } else {
                    increment_count(
                        &mut receipts_without_provider_request_id,
                        "receipts_without_provider_request_id",
                    )?;
                }
                if first_recovering_pass.is_none()
                    && latest_run_disposition(run) == RetryRunDisposition::Resolved
                    && recovery_condition(point, run, final_home_cell_from_run(run).as_deref())
                {
                    first_recovering_pass = Some(pass.index);
                }
            }
        }

        let final_home_cell = final_run.and_then(final_home_cell_from_run);
        let recovered = terminal == GeoRetryTerminal::Resolved
            && final_run
                .is_some_and(|run| recovery_condition(point, run, final_home_cell.as_deref()));
        match (terminal, recovered) {
            (GeoRetryTerminal::Resolved, true) => {
                increment_count(&mut recovered_count, "recovered")?;
            }
            (GeoRetryTerminal::AbstainedAtCeiling, _) => {
                increment_count(&mut abstained_count, "abstained_at_ceiling")?;
            }
            _ => {
                increment_count(&mut blocked_count, "blocked")?;
            }
        }
        per_point.push(GeoRetryRecoveryPoint {
            point_id: point.point_id.clone(),
            passes,
            terminal,
            recovered,
            first_recovering_pass,
            receipt_semantic_hashes,
            final_home_cell,
            landed_home_cell: Some(point.home_cell_r9.clone()),
        });
    }

    let denominator = usize_to_u64(population.points.len(), "denominator")?;
    let recovery = GeoRetryRecovery {
        version: CANON_GEO_RETRY_RECOVERY_VERSION.to_string(),
        population_blake3,
        policy,
        denominator,
        per_point,
        recovered: recovered_count,
        abstained_at_ceiling: abstained_count,
        blocked: blocked_count,
        by_provider,
        receipts_with_provider_request_id,
        receipts_without_provider_request_id,
        precision_claim: false,
    };
    validate_retry_recovery_artifact(&recovery)?;
    Ok(recovery)
}

pub fn validate_retry_recovery_artifact(artifact: &GeoRetryRecovery) -> Result<(), GeoRetryError> {
    if artifact.version != CANON_GEO_RETRY_RECOVERY_VERSION {
        return Err(GeoRetryError::new(
            GeoRetryErrorCode::UnsupportedVersion,
            "Geo retry recovery artifact declares an unsupported version",
            [
                ("expected", CANON_GEO_RETRY_RECOVERY_VERSION),
                ("actual", artifact.version.as_str()),
            ],
        ));
    }
    validate_prefixed_blake3("population_blake3", &artifact.population_blake3)?;
    validate_retry_policy(&artifact.policy)?;
    if artifact.denominator != G4_RETRY_RECOVERY_DENOMINATOR as u64
        || artifact.per_point.len() != G4_RETRY_RECOVERY_DENOMINATOR
    {
        return Err(recovery_denominator_mismatch(
            "Geo retry recovery artifact must preserve the frozen 40-point denominator",
            G4_RETRY_RECOVERY_DENOMINATOR,
            artifact.per_point.len(),
            None,
            None,
        ));
    }
    if artifact.denominator != usize_to_u64(artifact.per_point.len(), "per_point.len")? {
        return Err(recovery_denominator_mismatch(
            "Geo retry recovery denominator must equal per_point.len()",
            artifact.denominator as usize,
            artifact.per_point.len(),
            None,
            None,
        ));
    }
    if artifact.precision_claim {
        return Err(GeoRetryError::invalid(
            "Geo retry recovery is a reach measurement and must not claim precision",
            [("field", "precision_claim")],
        ));
    }

    let mut previous_point_id: Option<&str> = None;
    let mut recovered = 0_u64;
    let mut abstained = 0_u64;
    let mut blocked = 0_u64;
    let mut receipt_refs = 0_u64;
    for point in &artifact.per_point {
        validate_retry_string("per_point[].point_id", &point.point_id)?;
        if let Some(previous) = previous_point_id
            && previous >= point.point_id.as_str()
        {
            return Err(GeoRetryError::invalid(
                "Geo retry recovery points must be strictly sorted by point_id",
                [
                    ("field", "per_point[].point_id".to_string()),
                    ("previous_point_id", previous.to_string()),
                    ("point_id", point.point_id.clone()),
                ],
            ));
        }
        previous_point_id = Some(point.point_id.as_str());
        if point.passes == 0 || point.passes > artifact.policy.max_passes {
            return Err(GeoRetryError::invalid(
                "Geo retry recovery point pass counts must be bounded by policy",
                [
                    ("field", "per_point[].passes".to_string()),
                    ("point_id", point.point_id.clone()),
                    ("passes", point.passes.to_string()),
                    ("max_passes", artifact.policy.max_passes.to_string()),
                ],
            ));
        }
        if point.receipt_semantic_hashes.len() != usize::from(point.passes) {
            return Err(GeoRetryError::invalid(
                "Geo retry recovery points must bind one receipt semantic hash per retry pass",
                [
                    ("field", "per_point[].receipt_semantic_hashes".to_string()),
                    ("point_id", point.point_id.clone()),
                    ("passes", point.passes.to_string()),
                    (
                        "receipt_semantic_hashes",
                        point.receipt_semantic_hashes.len().to_string(),
                    ),
                ],
            ));
        }
        receipt_refs = receipt_refs
            .checked_add(usize_to_u64(
                point.receipt_semantic_hashes.len(),
                "receipt_semantic_hashes.len",
            )?)
            .ok_or_else(|| {
                GeoRetryError::new(
                    GeoRetryErrorCode::ArithmeticOverflow,
                    "Geo retry recovery receipt-reference count overflowed",
                    [("field", "receipt_semantic_hashes")],
                )
            })?;
        for receipt_hash in &point.receipt_semantic_hashes {
            validate_prefixed_blake3("per_point[].receipt_semantic_hashes[]", receipt_hash)?;
        }
        if let Some(first_recovering_pass) = point.first_recovering_pass
            && (!point.recovered
                || first_recovering_pass == 0
                || first_recovering_pass > point.passes)
        {
            return Err(GeoRetryError::invalid(
                "Geo retry recovery first_recovering_pass must point at a recovering pass",
                [
                    ("field", "per_point[].first_recovering_pass".to_string()),
                    ("point_id", point.point_id.clone()),
                    ("first_recovering_pass", first_recovering_pass.to_string()),
                ],
            ));
        }
        if point.recovered && point.terminal != GeoRetryTerminal::Resolved {
            return Err(GeoRetryError::invalid(
                "Geo retry recovery points can recover only from a resolved retry terminal",
                [
                    ("field", "per_point[].recovered".to_string()),
                    ("point_id", point.point_id.clone()),
                ],
            ));
        }
        if let Some(home_cell) = &point.final_home_cell {
            validate_home_cell_string("per_point[].final_home_cell", &point.point_id, home_cell)?;
        }
        if let Some(home_cell) = &point.landed_home_cell {
            validate_home_cell_string("per_point[].landed_home_cell", &point.point_id, home_cell)?;
        }

        match (point.terminal, point.recovered) {
            (GeoRetryTerminal::Resolved, true) => increment_count(&mut recovered, "recovered")?,
            (GeoRetryTerminal::AbstainedAtCeiling, _) => {
                increment_count(&mut abstained, "abstained_at_ceiling")?;
            }
            _ => increment_count(&mut blocked, "blocked")?,
        }
    }
    if artifact.recovered != recovered
        || artifact.abstained_at_ceiling != abstained
        || artifact.blocked != blocked
    {
        return Err(GeoRetryError::invalid(
            "Geo retry recovery summary counts must replay from per_point rows",
            [
                ("expected_recovered", recovered.to_string()),
                ("actual_recovered", artifact.recovered.to_string()),
                ("expected_abstained_at_ceiling", abstained.to_string()),
                (
                    "actual_abstained_at_ceiling",
                    artifact.abstained_at_ceiling.to_string(),
                ),
                ("expected_blocked", blocked.to_string()),
                ("actual_blocked", artifact.blocked.to_string()),
            ],
        ));
    }
    let sum = artifact
        .recovered
        .checked_add(artifact.abstained_at_ceiling)
        .and_then(|value| value.checked_add(artifact.blocked))
        .ok_or_else(|| {
            GeoRetryError::new(
                GeoRetryErrorCode::ArithmeticOverflow,
                "Geo retry recovery summary counts overflowed",
                [("field", "summary")],
            )
        })?;
    if sum != artifact.denominator {
        return Err(recovery_denominator_mismatch(
            "Geo retry recovery summary counts must add to denominator",
            artifact.denominator as usize,
            sum as usize,
            None,
            None,
        ));
    }
    for (provider, count) in &artifact.by_provider {
        validate_retry_string("by_provider.key", provider)?;
        if *count == 0 {
            return Err(GeoRetryError::invalid(
                "Geo retry recovery by_provider counts must be positive",
                [("provider", provider.as_str())],
            ));
        }
    }
    let receipt_counter_sum = artifact
        .receipts_with_provider_request_id
        .checked_add(artifact.receipts_without_provider_request_id)
        .ok_or_else(|| {
            GeoRetryError::new(
                GeoRetryErrorCode::ArithmeticOverflow,
                "Geo retry recovery receipt counter sum overflowed",
                [("field", "receipt counters")],
            )
        })?;
    if receipt_counter_sum != receipt_refs {
        return Err(GeoRetryError::invalid(
            "Geo retry recovery receipt counters must equal bound receipt references",
            [
                ("expected", receipt_refs.to_string()),
                ("actual", receipt_counter_sum.to_string()),
            ],
        ));
    }
    Ok(())
}

pub fn canonical_retry_recovery_bytes(
    artifact: &GeoRetryRecovery,
) -> Result<Vec<u8>, GeoRetryError> {
    validate_retry_recovery_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoRetryError::invalid(
            "Geo retry recovery artifact could not be serialized",
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

fn validate_recovery_receipt_binding(
    point: &GeoPointPopulationPoint,
    pass: &GeoRetryPass,
    request: &GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
    retained_or_live_receipts_present: bool,
) -> Result<String, GeoRetryError> {
    let receipt_blake3 = retry_receipt_blake3(receipt, point, pass)?;
    let expected_request_hash = retry_acquisition_request_hash(request, point, pass)?;
    if receipt.request_semantic_hash != expected_request_hash {
        return Err(receipt_unbound(
            point.point_id.as_str(),
            pass.index,
            "Geo retry recovery receipt request_semantic_hash does not match the emitted acquisition request",
            [
                ("field", "receipt.request_semantic_hash".to_string()),
                ("receipt_blake3", receipt_blake3),
                ("expected", expected_request_hash),
                ("actual", receipt.request_semantic_hash.clone()),
            ],
        ));
    }
    if let Some(expected_receipt_blake3) = &pass.receipt_blake3 {
        if expected_receipt_blake3 != &receipt_blake3 {
            return Err(receipt_unbound(
                point.point_id.as_str(),
                pass.index,
                "Geo retry recovery pass receipt_blake3 does not match the supplied receipt bytes",
                [
                    ("field", "passes[].receipt_blake3".to_string()),
                    ("expected", expected_receipt_blake3.clone()),
                    ("actual", receipt_blake3.clone()),
                ],
            ));
        }
    } else {
        return Err(receipt_unbound(
            point.point_id.as_str(),
            pass.index,
            "Geo retry recovery pass has an acquisition request without a receipt digest",
            [("field", "passes[].receipt_blake3".to_string())],
        ));
    }
    validate_geo_acquisition_receipt(request, receipt).map_err(|error| {
        receipt_unbound(
            point.point_id.as_str(),
            pass.index,
            "Geo retry recovery receipt does not satisfy the emitted acquisition request",
            [
                ("field", "receipt".to_string()),
                ("receipt_blake3", receipt_blake3.clone()),
                ("source_code", format!("{:?}", error.code)),
                ("source_message", error.message),
            ],
        )
    })?;
    if retained_or_live_receipts_present && receipt.proof_class == GeoAcquisitionProofClass::Fixture
    {
        return Err(receipt_unbound(
            point.point_id.as_str(),
            pass.index,
            "Geo retry recovery cannot mix fixture acquisition receipts into retained or live recovery counts",
            [("proof_class", "fixture".to_string())],
        ));
    }
    validate_provider_response_bytes_pin(point, pass, receipt)?;
    validate_candidate_rows_pin(point, pass, receipt)?;
    Ok(receipt_blake3)
}

fn retry_acquisition_request_hash(
    request: &GeoAcquisitionRequest,
    point: &GeoPointPopulationPoint,
    pass: &GeoRetryPass,
) -> Result<String, GeoRetryError> {
    geo_acquisition_request_semantic_hash(request).map_err(|error| {
        receipt_unbound(
            point.point_id.as_str(),
            pass.index,
            "Geo retry recovery could not recompute the emitted acquisition request hash",
            [
                ("field", "passes[].regeocode".to_string()),
                ("source_code", format!("{:?}", error.code)),
                ("source_message", error.message),
            ],
        )
    })
}

fn retry_receipt_blake3(
    receipt: &GeoAcquisitionReceipt,
    point: &GeoPointPopulationPoint,
    pass: &GeoRetryPass,
) -> Result<String, GeoRetryError> {
    serde_json::to_vec(receipt)
        .map(|bytes| prefixed_hash(&bytes))
        .map_err(|error| {
            receipt_unbound(
                point.point_id.as_str(),
                pass.index,
                "Geo retry recovery acquisition receipt could not be serialized",
                [("serde_error", error.to_string())],
            )
        })
}

fn validate_provider_response_bytes_pin(
    point: &GeoPointPopulationPoint,
    pass: &GeoRetryPass,
    receipt: &GeoAcquisitionReceipt,
) -> Result<(), GeoRetryError> {
    let Some(result_digest) = receipt
        .result_digests
        .iter()
        .find(|digest| digest.digest_id == PROVIDER_RESPONSE_BYTES_DIGEST_ID)
    else {
        return Err(receipt_unbound(
            point.point_id.as_str(),
            pass.index,
            "Geo retry recovery receipts must pin retained provider response bytes",
            [("digest_id", PROVIDER_RESPONSE_BYTES_DIGEST_ID.to_string())],
        ));
    };
    if result_digest.algorithm != GeoDigestAlgorithm::Blake3 {
        return Err(receipt_unbound(
            point.point_id.as_str(),
            pass.index,
            "Geo retry recovery provider response bytes must be pinned with Blake3",
            [
                ("digest_id", PROVIDER_RESPONSE_BYTES_DIGEST_ID.to_string()),
                ("algorithm", format!("{:?}", result_digest.algorithm)),
            ],
        ));
    }
    let Some(local_artifact) = receipt
        .local_artifacts
        .iter()
        .find(|artifact| artifact.artifact_id == PROVIDER_RESPONSE_BYTES_DIGEST_ID)
    else {
        return Err(receipt_unbound(
            point.point_id.as_str(),
            pass.index,
            "Geo retry recovery receipts must retain the provider response bytes artifact",
            [("artifact_id", PROVIDER_RESPONSE_BYTES_DIGEST_ID.to_string())],
        ));
    };
    if local_artifact.digest.algorithm != GeoDigestAlgorithm::Blake3
        || local_artifact.digest.hex_digest != result_digest.hex_digest
    {
        return Err(receipt_unbound(
            point.point_id.as_str(),
            pass.index,
            "Geo retry recovery provider response bytes digest is stale relative to the retained artifact pin",
            [
                ("digest_id", PROVIDER_RESPONSE_BYTES_DIGEST_ID.to_string()),
                ("expected", result_digest.hex_digest.clone()),
                ("actual", local_artifact.digest.hex_digest.clone()),
            ],
        ));
    }
    Ok(())
}

fn validate_candidate_rows_pin(
    point: &GeoPointPopulationPoint,
    pass: &GeoRetryPass,
    receipt: &GeoAcquisitionReceipt,
) -> Result<(), GeoRetryError> {
    if receipt
        .local_artifacts
        .iter()
        .any(|artifact| artifact.artifact_id == GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID)
    {
        return Ok(());
    }
    Err(receipt_unbound(
        point.point_id.as_str(),
        pass.index,
        "Geo retry recovery receipts must retain per-candidate geocode rows",
        [(
            "artifact_id",
            GEOCODE_CANDIDATE_ROWS_ARTIFACT_ID.to_string(),
        )],
    ))
}

fn retry_population_blake3(
    population: &GeoPointPopulationArtifact,
) -> Result<String, GeoRetryError> {
    canonical_point_population_bytes(population)
        .map(|bytes| prefixed_hash(&bytes))
        .map_err(|error| {
            GeoRetryError::invalid(
                "Geo retry recovery requires a valid point-population artifact",
                [
                    ("field", "population".to_string()),
                    ("source_code", format!("{:?}", error.code)),
                    ("source_message", error.message),
                ],
            )
        })
}

fn recovery_condition(
    point: &GeoPointPopulationPoint,
    run: &GeoRun,
    final_home_cell: Option<&str>,
) -> bool {
    let home_cell_changed = final_home_cell.is_some_and(|cell| cell != point.home_cell_r9);
    let refuter_silent = point.refuter_fired
        && latest_run_disposition(run) == RetryRunDisposition::Resolved
        && run.blockers.is_empty();
    home_cell_changed || refuter_silent
}

fn final_home_cell_from_run(run: &GeoRun) -> Option<String> {
    run.output_refs
        .iter()
        .filter(|output| output.contract_version == CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION)
        .find_map(|output| {
            if is_h3_r9(&output.output_id) {
                return Some(output.output_id.clone());
            }
            let suffix = output.artifact_id.rsplit('/').next()?;
            if is_h3_r9(suffix) {
                return Some(suffix.to_string());
            }
            None
        })
}

fn validate_home_cell_string(
    field: &'static str,
    point_id: &str,
    value: &str,
) -> Result<(), GeoRetryError> {
    if is_h3_r9(value) {
        return Ok(());
    }
    Err(GeoRetryError::invalid(
        "Geo retry recovery home-cell fields must be valid H3 resolution-9 cells",
        [
            ("field", field.to_string()),
            ("point_id", point_id.to_string()),
            ("value", value.to_string()),
        ],
    ))
}

fn is_h3_r9(value: &str) -> bool {
    CellIndex::from_str(value)
        .map(|cell| u8::from(cell.resolution()) == 9)
        .unwrap_or(false)
}

fn recovery_denominator_mismatch(
    message: &'static str,
    expected: usize,
    actual: usize,
    subject_id: Option<&str>,
    point_id: Option<&str>,
) -> GeoRetryError {
    let mut detail = BTreeMap::from([
        ("expected".to_string(), expected.to_string()),
        ("actual".to_string(), actual.to_string()),
    ]);
    if let Some(subject_id) = subject_id {
        detail.insert("subject_id".to_string(), subject_id.to_string());
    }
    if let Some(point_id) = point_id {
        detail.insert("point_id".to_string(), point_id.to_string());
    }
    GeoRetryError {
        code: GeoRetryErrorCode::RetryRecoveryDenominatorMismatch,
        message: message.to_string(),
        detail,
    }
}

fn receipt_unbound(
    point_id: &str,
    pass: u8,
    message: &'static str,
    detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
) -> GeoRetryError {
    let mut mapped = BTreeMap::from([
        ("point_id".to_string(), point_id.to_string()),
        ("pass".to_string(), pass.to_string()),
    ]);
    mapped.extend(
        detail
            .into_iter()
            .map(|(key, value)| (key.into(), value.into())),
    );
    GeoRetryError {
        code: GeoRetryErrorCode::RetryReceiptUnbound,
        message: message.to_string(),
        detail: mapped,
    }
}

fn increment_count(value: &mut u64, field: &'static str) -> Result<(), GeoRetryError> {
    *value = value.checked_add(1).ok_or_else(|| {
        GeoRetryError::new(
            GeoRetryErrorCode::ArithmeticOverflow,
            "Geo retry recovery count overflowed",
            [("field", field)],
        )
    })?;
    Ok(())
}

fn usize_to_u64(value: usize, field: &'static str) -> Result<u64, GeoRetryError> {
    u64::try_from(value).map_err(|_| {
        GeoRetryError::new(
            GeoRetryErrorCode::ArithmeticOverflow,
            "Geo retry recovery count exceeded u64 range",
            [("field", field)],
        )
    })
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
