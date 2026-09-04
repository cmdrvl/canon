#![forbid(unsafe_code)]

//! Physical collateral ledger rows and per-truth-plane deal rollups.
//!
//! Ledger rows are receipt surfaces over already-produced composition and
//! evidence artifacts. They do not solve, rescore, or infer missing collateral.

use crate::geo::{
    GeoCandidateReachStatus, GeoClaimClass, GeoCompositionArtifact, GeoCompositionModel,
    GeoCompositionStatus, GeoEvidenceCompilationArtifact, GeoTruthPlane, GeoValidTimeInterval,
    canonical_composition_bytes, canonical_evidence_compilation_bytes,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_COLLATERAL_LEDGER_VERSION: &str = "canon_geo_collateral_ledger.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCollateralLedgerProofClass {
    Fixture,
    RetainedArtifact,
    ObservedWarehouseSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSourceReleasePin {
    pub source_dataset: String,
    pub source_release: String,
    pub blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoLedgerLoanRef {
    pub accession: String,
    pub deal_id: String,
    pub loan_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deed_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoLedgerPropertyRef {
    pub property_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parcel_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub building_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoLedgerRow {
    pub version: String,
    pub accession: String,
    pub deal_id: String,
    pub loan_id: String,
    pub reach: GeoCandidateReachStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach_none_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parcel_set: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub building_set: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deed_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truth_plane: Option<GeoTruthPlane>,
    pub claim_class: GeoClaimClass,
    pub residual_model_count: u64,
    pub count_exact: bool,
    pub backbone_complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_observed_present: Option<GeoValidTimeInterval>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_release_pins: Vec<GeoSourceReleasePin>,
    pub composition_blake3: String,
    pub evidence_blake3: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ambiguous_parcel_set: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ambiguous_building_set: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub property_refs: Vec<GeoLedgerPropertyRef>,
    pub composition_status: GeoCompositionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDealRollupPlaneCounts {
    pub resolved: u64,
    pub ambiguous: u64,
    pub conflict: u64,
    #[serde(default)]
    pub reach_none: u64,
    #[serde(default)]
    pub budget_fallback: u64,
}

impl GeoDealRollupPlaneCounts {
    fn increment(&mut self, row: &GeoLedgerRow) -> Result<(), GeoLedgerError> {
        if row.reach == GeoCandidateReachStatus::None {
            self.reach_none = checked_inc(self.reach_none, "rollup.reach_none")?;
            return Ok(());
        }
        match row.composition_status {
            GeoCompositionStatus::Resolved => {
                self.resolved = checked_inc(self.resolved, "rollup.resolved")?;
            }
            GeoCompositionStatus::Ambiguous => {
                self.ambiguous = checked_inc(self.ambiguous, "rollup.ambiguous")?;
            }
            GeoCompositionStatus::Conflict => {
                self.conflict = checked_inc(self.conflict, "rollup.conflict")?;
            }
            GeoCompositionStatus::BudgetFallback => {
                self.budget_fallback = checked_inc(self.budget_fallback, "rollup.budget_fallback")?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDealRollup {
    pub deal_id: String,
    pub accession: String,
    pub rows: u64,
    pub truth_planes: BTreeMap<GeoTruthPlane, GeoDealRollupPlaneCounts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCollateralLedger {
    pub version: String,
    pub proof_class: GeoCollateralLedgerProofClass,
    pub rows: Vec<GeoLedgerRow>,
    pub rollups: Vec<GeoDealRollup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoLedgerErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    LedgerTruthPlanePooled,
    LedgerReachNone,
    LedgerSetsWithoutArtifacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoLedgerError {
    pub code: GeoLedgerErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoLedgerError {
    fn new(
        code: GeoLedgerErrorCode,
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
        Self::new(GeoLedgerErrorCode::InvalidInput, message, detail)
    }

    fn invalid_field(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self::invalid(
            "Geo collateral ledger input contains an invalid field",
            [("field", field.into()), ("value", value.into())],
        )
    }

    fn overflow(field: impl Into<String>) -> Self {
        Self::new(
            GeoLedgerErrorCode::ArithmeticOverflow,
            "Geo collateral ledger accounting overflowed",
            [("field", field.into())],
        )
    }
}

impl fmt::Display for GeoLedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoLedgerError {}

pub fn build_ledger_row(
    loan: &GeoLedgerLoanRef,
    reach: GeoCandidateReachStatus,
    reach_none_reason: Option<String>,
    composition: Option<&GeoCompositionArtifact>,
    evidence: Option<&GeoEvidenceCompilationArtifact>,
    truth_plane: Option<GeoTruthPlane>,
    pins: &[GeoSourceReleasePin],
) -> Result<GeoLedgerRow, GeoLedgerError> {
    validate_loan_ref(loan)?;
    validate_nonempty_source_release_pins(pins, &loan.loan_id)?;
    validate_source_release_pins(pins)?;

    if reach == GeoCandidateReachStatus::None {
        let reason = normalize_required_reason(reach_none_reason.as_deref())?;
        let (composition_status, residual_model_count, count_exact, backbone_complete) =
            composition.map(composition_summary_fields).unwrap_or((
                GeoCompositionStatus::BudgetFallback,
                0,
                false,
                false,
            ));
        let composition_blake3 = composition
            .map(composition_digest)
            .transpose()?
            .unwrap_or_default();
        let evidence_blake3 = evidence
            .map(evidence_digest)
            .transpose()?
            .unwrap_or_default();
        return Ok(GeoLedgerRow {
            version: CANON_GEO_COLLATERAL_LEDGER_VERSION.to_string(),
            accession: loan.accession.clone(),
            deal_id: loan.deal_id.clone(),
            loan_id: loan.loan_id.clone(),
            reach,
            reach_none_reason: Some(reason),
            parcel_set: None,
            building_set: None,
            deed_ids: sorted_unique(loan.deed_ids.clone()),
            truth_plane,
            claim_class: GeoClaimClass::CollateralComposition,
            residual_model_count,
            count_exact,
            backbone_complete,
            last_observed_present: None,
            source_release_pins: sorted_unique(pins.to_vec()),
            composition_blake3,
            evidence_blake3,
            ambiguous_parcel_set: Vec::new(),
            ambiguous_building_set: Vec::new(),
            property_refs: Vec::new(),
            composition_status,
        });
    }

    if reach_none_reason
        .as_deref()
        .is_some_and(|reason| !reason.trim().is_empty())
    {
        return Err(GeoLedgerError::invalid(
            "Geo collateral ledger reach_none_reason is only valid when candidate reach is none",
            [
                ("field", "reach_none_reason"),
                ("loan_id", loan.loan_id.as_str()),
            ],
        ));
    }
    let composition = composition.ok_or_else(|| {
        GeoLedgerError::new(
            GeoLedgerErrorCode::LedgerSetsWithoutArtifacts,
            "Geo collateral ledger row with candidate reach requires a composition artifact",
            [("field", "composition"), ("loan_id", loan.loan_id.as_str())],
        )
    })?;
    let evidence = evidence.ok_or_else(|| {
        GeoLedgerError::new(
            GeoLedgerErrorCode::LedgerSetsWithoutArtifacts,
            "Geo collateral ledger row with candidate reach requires an evidence artifact",
            [("field", "evidence"), ("loan_id", loan.loan_id.as_str())],
        )
    })?;

    let (composition_status, residual_model_count, count_exact, backbone_complete) =
        composition_summary_fields(composition);
    let parcel_set = sorted_unique(composition.hard_forced.parcels.clone());
    let building_set = sorted_unique(composition.hard_forced.buildings.clone());
    let (ambiguous_parcel_set, ambiguous_building_set) = ambiguous_members(composition);

    Ok(GeoLedgerRow {
        version: CANON_GEO_COLLATERAL_LEDGER_VERSION.to_string(),
        accession: loan.accession.clone(),
        deal_id: loan.deal_id.clone(),
        loan_id: loan.loan_id.clone(),
        reach,
        reach_none_reason: None,
        parcel_set: Some(parcel_set),
        building_set: Some(building_set),
        deed_ids: sorted_unique(loan.deed_ids.clone()),
        truth_plane,
        claim_class: GeoClaimClass::CollateralComposition,
        residual_model_count,
        count_exact,
        backbone_complete,
        last_observed_present: None,
        source_release_pins: sorted_unique(pins.to_vec()),
        composition_blake3: composition_digest(composition)?,
        evidence_blake3: evidence_digest(evidence)?,
        ambiguous_parcel_set,
        ambiguous_building_set,
        property_refs: Vec::new(),
        composition_status,
    })
}

pub fn build_collateral_ledger(
    rows: impl IntoIterator<Item = GeoLedgerRow>,
    proof_class: GeoCollateralLedgerProofClass,
) -> Result<GeoCollateralLedger, GeoLedgerError> {
    let mut rows: Vec<_> = rows.into_iter().collect();
    rows.sort_by_key(row_sort_key);
    let rollups = rollups_for_rows(&rows)?;
    let ledger = GeoCollateralLedger {
        version: CANON_GEO_COLLATERAL_LEDGER_VERSION.to_string(),
        proof_class,
        rows,
        rollups,
    };
    validate_ledger(&ledger)?;
    Ok(ledger)
}

pub fn roll_up_deal(rows: &[GeoLedgerRow]) -> Result<GeoDealRollup, GeoLedgerError> {
    let first = rows.first().ok_or_else(|| {
        GeoLedgerError::invalid(
            "Geo collateral ledger rollup requires at least one row",
            [("field", "rows")],
        )
    })?;
    validate_text("deal_id", &first.deal_id)?;
    validate_text("accession", &first.accession)?;

    let mut truth_planes = BTreeMap::<GeoTruthPlane, GeoDealRollupPlaneCounts>::new();
    for row in rows {
        if row.deal_id != first.deal_id {
            return Err(GeoLedgerError::invalid(
                "Geo collateral ledger rollup received rows from more than one deal",
                [
                    ("field", "deal_id"),
                    ("expected", first.deal_id.as_str()),
                    ("actual", row.deal_id.as_str()),
                    ("loan_id", row.loan_id.as_str()),
                ],
            ));
        }
        if row.accession != first.accession {
            return Err(GeoLedgerError::invalid(
                "Geo collateral ledger rollup received rows from more than one accession",
                [
                    ("field", "accession"),
                    ("expected", first.accession.as_str()),
                    ("actual", row.accession.as_str()),
                    ("loan_id", row.loan_id.as_str()),
                ],
            ));
        }
        let truth_plane = row.truth_plane.ok_or_else(|| {
            GeoLedgerError::new(
                GeoLedgerErrorCode::LedgerTruthPlanePooled,
                "Geo collateral ledger rollup cannot pool rows without per-truth-plane labels",
                [("field", "truth_plane"), ("loan_id", row.loan_id.as_str())],
            )
        })?;
        truth_planes
            .entry(truth_plane)
            .or_insert(GeoDealRollupPlaneCounts {
                resolved: 0,
                ambiguous: 0,
                conflict: 0,
                reach_none: 0,
                budget_fallback: 0,
            })
            .increment(row)?;
    }
    Ok(GeoDealRollup {
        deal_id: first.deal_id.clone(),
        accession: first.accession.clone(),
        rows: usize_to_u64(rows.len(), "rollup.rows")?,
        truth_planes,
    })
}

pub fn validate_ledger(ledger: &GeoCollateralLedger) -> Result<(), GeoLedgerError> {
    if ledger.version != CANON_GEO_COLLATERAL_LEDGER_VERSION {
        return Err(GeoLedgerError::new(
            GeoLedgerErrorCode::UnsupportedVersion,
            "Unsupported Geo collateral ledger artifact version",
            [
                ("actual", ledger.version.as_str()),
                ("expected", CANON_GEO_COLLATERAL_LEDGER_VERSION),
            ],
        ));
    }
    if ledger.rows.is_empty() {
        return Err(GeoLedgerError::invalid(
            "Geo collateral ledger requires at least one row",
            [("field", "rows")],
        ));
    }
    validate_rows_sorted(&ledger.rows)?;
    for row in &ledger.rows {
        validate_ledger_row(row, ledger.proof_class)?;
    }
    let expected_rollups = rollups_for_rows(&ledger.rows)?;
    if ledger.rollups != expected_rollups {
        return Err(GeoLedgerError::invalid(
            "Geo collateral ledger rollups do not match recomputed row rollups",
            [("field", "rollups")],
        ));
    }
    Ok(())
}

pub fn validate_collateral_ledger_artifact(
    ledger: &GeoCollateralLedger,
) -> Result<(), GeoLedgerError> {
    validate_ledger(ledger)
}

pub fn canonical_collateral_ledger_bytes(
    ledger: &GeoCollateralLedger,
) -> Result<Vec<u8>, GeoLedgerError> {
    validate_ledger(ledger)?;
    serde_json::to_vec(ledger).map_err(|error| {
        GeoLedgerError::invalid(
            "Geo collateral ledger artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

fn validate_loan_ref(loan: &GeoLedgerLoanRef) -> Result<(), GeoLedgerError> {
    validate_text("accession", &loan.accession)?;
    validate_text("deal_id", &loan.deal_id)?;
    validate_text("loan_id", &loan.loan_id)?;
    validate_sorted_unique("deed_ids", &sorted_unique(loan.deed_ids.clone()))?;
    for deed_id in &loan.deed_ids {
        validate_text("deed_ids[]", deed_id)?;
    }
    Ok(())
}

fn validate_ledger_row(
    row: &GeoLedgerRow,
    proof_class: GeoCollateralLedgerProofClass,
) -> Result<(), GeoLedgerError> {
    if row.version != CANON_GEO_COLLATERAL_LEDGER_VERSION {
        return Err(GeoLedgerError::new(
            GeoLedgerErrorCode::UnsupportedVersion,
            "Unsupported Geo collateral ledger row version",
            [
                ("actual", row.version.as_str()),
                ("expected", CANON_GEO_COLLATERAL_LEDGER_VERSION),
                ("loan_id", row.loan_id.as_str()),
            ],
        ));
    }
    validate_text("accession", &row.accession)?;
    validate_text("deal_id", &row.deal_id)?;
    validate_text("loan_id", &row.loan_id)?;
    if row.claim_class != GeoClaimClass::CollateralComposition {
        return Err(GeoLedgerError::invalid(
            "Geo collateral ledger row has the wrong claim class",
            [("field", "claim_class"), ("loan_id", row.loan_id.as_str())],
        ));
    }
    validate_sorted_unique("deed_ids", &row.deed_ids)?;
    validate_source_release_pins_for_proof_class(
        &row.source_release_pins,
        proof_class,
        &row.loan_id,
    )?;
    validate_prefixed_blake3_or_empty_for_reach_none(
        "composition_blake3",
        &row.composition_blake3,
        row.reach,
    )?;
    validate_prefixed_blake3_or_empty_for_reach_none(
        "evidence_blake3",
        &row.evidence_blake3,
        row.reach,
    )?;
    validate_sorted_unique("ambiguous_parcel_set", &row.ambiguous_parcel_set)?;
    validate_sorted_unique("ambiguous_building_set", &row.ambiguous_building_set)?;
    validate_property_refs(&row.property_refs)?;
    if let Some(interval) = row.last_observed_present {
        validate_interval("last_observed_present", interval)?;
    }

    match row.reach {
        GeoCandidateReachStatus::None => {
            normalize_required_reason(row.reach_none_reason.as_deref())?;
            if row.parcel_set.is_some() || row.building_set.is_some() {
                return Err(GeoLedgerError::new(
                    GeoLedgerErrorCode::LedgerReachNone,
                    "Geo collateral ledger row with no candidate reach cannot carry fabricated sets",
                    [("field", "parcel_set"), ("loan_id", row.loan_id.as_str())],
                ));
            }
        }
        GeoCandidateReachStatus::Full | GeoCandidateReachStatus::Partial => {
            if row
                .reach_none_reason
                .as_deref()
                .is_some_and(|reason| !reason.trim().is_empty())
            {
                return Err(GeoLedgerError::invalid(
                    "Geo collateral ledger reach_none_reason is only valid when candidate reach is none",
                    [
                        ("field", "reach_none_reason"),
                        ("loan_id", row.loan_id.as_str()),
                    ],
                ));
            }
            let parcel_set = row.parcel_set.as_ref().ok_or_else(|| {
                GeoLedgerError::invalid(
                    "Geo collateral ledger row with candidate reach requires a parcel set",
                    [("field", "parcel_set"), ("loan_id", row.loan_id.as_str())],
                )
            })?;
            let building_set = row.building_set.as_ref().ok_or_else(|| {
                GeoLedgerError::invalid(
                    "Geo collateral ledger row with candidate reach requires a building set",
                    [("field", "building_set"), ("loan_id", row.loan_id.as_str())],
                )
            })?;
            validate_sorted_unique("parcel_set", parcel_set)?;
            validate_sorted_unique("building_set", building_set)?;
            validate_prefixed_blake3("composition_blake3", &row.composition_blake3)?;
            validate_prefixed_blake3("evidence_blake3", &row.evidence_blake3)?;
        }
    }
    if row.truth_plane.is_none() {
        return Err(GeoLedgerError::new(
            GeoLedgerErrorCode::LedgerTruthPlanePooled,
            "Geo collateral ledger row is missing its truth-plane label",
            [("field", "truth_plane"), ("loan_id", row.loan_id.as_str())],
        ));
    }
    Ok(())
}

fn validate_property_refs(refs: &[GeoLedgerPropertyRef]) -> Result<(), GeoLedgerError> {
    let mut previous: Option<&GeoLedgerPropertyRef> = None;
    for property_ref in refs {
        validate_text("property_refs[].property_id", &property_ref.property_id)?;
        validate_sorted_unique("property_refs[].parcel_ids", &property_ref.parcel_ids)?;
        validate_sorted_unique("property_refs[].building_ids", &property_ref.building_ids)?;
        if let Some(previous) = previous
            && previous >= property_ref
        {
            return Err(GeoLedgerError::invalid(
                "Geo collateral ledger property refs must be strictly sorted and unique",
                [
                    ("field", "property_refs"),
                    ("property_id", property_ref.property_id.as_str()),
                ],
            ));
        }
        previous = Some(property_ref);
    }
    Ok(())
}

fn validate_source_release_pins(pins: &[GeoSourceReleasePin]) -> Result<(), GeoLedgerError> {
    validate_sorted_unique("source_release_pins", pins)?;
    for pin in pins {
        validate_text("source_release_pins[].source_dataset", &pin.source_dataset)?;
        validate_text("source_release_pins[].source_release", &pin.source_release)?;
        validate_prefixed_blake3("source_release_pins[].blake3", &pin.blake3)?;
    }
    Ok(())
}

fn validate_source_release_pins_for_proof_class(
    pins: &[GeoSourceReleasePin],
    proof_class: GeoCollateralLedgerProofClass,
    loan_id: &str,
) -> Result<(), GeoLedgerError> {
    validate_nonempty_source_release_pins(pins, loan_id)?;
    validate_source_release_pins(pins)?;
    let mut saw_fixture = false;
    let mut saw_live = false;
    for pin in pins {
        if pin.source_dataset.starts_with("fixture.") {
            saw_fixture = true;
        } else {
            saw_live = true;
        }
        match proof_class {
            GeoCollateralLedgerProofClass::Fixture
                if !pin.source_dataset.starts_with("fixture.") =>
            {
                return Err(source_pin_proof_error(loan_id, &pin.source_dataset));
            }
            GeoCollateralLedgerProofClass::ObservedWarehouseSnapshot
            | GeoCollateralLedgerProofClass::RetainedArtifact
                if pin.source_dataset.starts_with("fixture.") =>
            {
                return Err(source_pin_proof_error(loan_id, &pin.source_dataset));
            }
            GeoCollateralLedgerProofClass::Fixture
            | GeoCollateralLedgerProofClass::ObservedWarehouseSnapshot
            | GeoCollateralLedgerProofClass::RetainedArtifact => {}
        }
    }
    if saw_fixture && saw_live {
        return Err(source_pin_proof_error(loan_id, "mixed fixture/live pins"));
    }
    Ok(())
}

fn validate_nonempty_source_release_pins(
    pins: &[GeoSourceReleasePin],
    loan_id: &str,
) -> Result<(), GeoLedgerError> {
    if pins.is_empty() {
        return Err(GeoLedgerError::invalid(
            "Geo collateral ledger row requires at least one source release pin",
            [("field", "source_release_pins"), ("loan_id", loan_id)],
        ));
    }
    Ok(())
}

fn source_pin_proof_error(loan_id: &str, source_dataset: &str) -> GeoLedgerError {
    GeoLedgerError::invalid(
        "Geo collateral ledger source release pins do not match the artifact proof class",
        [
            ("field", "source_release_pins"),
            ("loan_id", loan_id),
            ("source_dataset", source_dataset),
        ],
    )
}

fn rollups_for_rows(rows: &[GeoLedgerRow]) -> Result<Vec<GeoDealRollup>, GeoLedgerError> {
    let mut grouped = BTreeMap::<(String, String), Vec<GeoLedgerRow>>::new();
    for row in rows {
        grouped
            .entry((row.accession.clone(), row.deal_id.clone()))
            .or_default()
            .push(row.clone());
    }
    grouped
        .values()
        .map(|deal_rows| roll_up_deal(deal_rows))
        .collect()
}

fn validate_rows_sorted(rows: &[GeoLedgerRow]) -> Result<(), GeoLedgerError> {
    let mut previous: Option<(&str, &str, &str)> = None;
    for row in rows {
        let key = (
            row.accession.as_str(),
            row.deal_id.as_str(),
            row.loan_id.as_str(),
        );
        if previous.is_some_and(|previous| previous >= key) {
            return Err(GeoLedgerError::invalid(
                "Geo collateral ledger rows must be strictly sorted by accession, deal_id, loan_id",
                [
                    ("field", "rows"),
                    ("accession", row.accession.as_str()),
                    ("deal_id", row.deal_id.as_str()),
                    ("loan_id", row.loan_id.as_str()),
                ],
            ));
        }
        previous = Some(key);
    }
    Ok(())
}

fn row_sort_key(row: &GeoLedgerRow) -> (String, String, String) {
    (
        row.accession.clone(),
        row.deal_id.clone(),
        row.loan_id.clone(),
    )
}

fn composition_summary_fields(
    composition: &GeoCompositionArtifact,
) -> (GeoCompositionStatus, u64, bool, bool) {
    (
        composition.status,
        composition.summary.residual_model_count,
        composition.summary.residual_model_count_complete
            && !composition.summary.residual_model_count_saturated,
        composition.backbone_complete,
    )
}

fn ambiguous_members(composition: &GeoCompositionArtifact) -> (Vec<String>, Vec<String>) {
    let hard_parcels = composition
        .hard_forced
        .parcels
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let hard_buildings = composition
        .hard_forced
        .buildings
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    (
        model_members(&composition.residual_models, |model| &model.parcels)
            .difference(&hard_parcels)
            .cloned()
            .collect(),
        model_members(&composition.residual_models, |model| &model.buildings)
            .difference(&hard_buildings)
            .cloned()
            .collect(),
    )
}

fn model_members<'a>(
    models: &'a [GeoCompositionModel],
    member_slice: impl Fn(&'a GeoCompositionModel) -> &'a [String],
) -> BTreeSet<String> {
    models
        .iter()
        .flat_map(|model| member_slice(model).iter().cloned())
        .collect()
}

fn composition_digest(composition: &GeoCompositionArtifact) -> Result<String, GeoLedgerError> {
    let bytes = canonical_composition_bytes(composition).map_err(|error| {
        GeoLedgerError::invalid(
            "Geo composition artifact could not be serialized for ledger digest",
            [("error", error.to_string())],
        )
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn evidence_digest(evidence: &GeoEvidenceCompilationArtifact) -> Result<String, GeoLedgerError> {
    let bytes = canonical_evidence_compilation_bytes(evidence).map_err(|error| {
        GeoLedgerError::invalid(
            "Geo evidence artifact could not be serialized for ledger digest",
            [("error", error.to_string())],
        )
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn validate_text(field: &str, value: &str) -> Result<(), GeoLedgerError> {
    if value.trim().is_empty() || value != value.trim() {
        return Err(GeoLedgerError::invalid_field(field, value));
    }
    Ok(())
}

fn normalize_required_reason(reason: Option<&str>) -> Result<String, GeoLedgerError> {
    let Some(reason) = reason else {
        return Err(GeoLedgerError::invalid(
            "Geo collateral ledger reach-none rows require a reason",
            [("field", "reach_none_reason")],
        ));
    };
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(GeoLedgerError::invalid(
            "Geo collateral ledger reach-none rows require a reason",
            [("field", "reach_none_reason")],
        ));
    }
    Ok(reason.to_string())
}

fn validate_interval(field: &str, interval: GeoValidTimeInterval) -> Result<(), GeoLedgerError> {
    if interval.start_day > interval.end_day {
        let start_day = interval.start_day.to_string();
        let end_day = interval.end_day.to_string();
        return Err(GeoLedgerError::invalid(
            "Geo collateral ledger valid-time interval is inverted",
            [
                ("field", field),
                ("start_day", start_day.as_str()),
                ("end_day", end_day.as_str()),
            ],
        ));
    }
    Ok(())
}

fn validate_sorted_unique<T: Ord + fmt::Debug>(
    field: &str,
    values: &[T],
) -> Result<(), GeoLedgerError> {
    for pair in values.windows(2) {
        if pair[0] >= pair[1] {
            let value = format!("{:?}", pair[1]);
            return Err(GeoLedgerError::invalid(
                "Geo collateral ledger vectors must be strictly sorted and unique",
                [("field", field), ("value", value.as_str())],
            ));
        }
    }
    Ok(())
}

fn sorted_unique<T: Ord>(values: Vec<T>) -> Vec<T> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn checked_inc(value: u64, field: impl Into<String>) -> Result<u64, GeoLedgerError> {
    value
        .checked_add(1)
        .ok_or_else(|| GeoLedgerError::overflow(field))
}

fn usize_to_u64(value: usize, field: impl Into<String>) -> Result<u64, GeoLedgerError> {
    u64::try_from(value).map_err(|_| GeoLedgerError::overflow(field))
}

fn validate_prefixed_blake3(field: &str, value: &str) -> Result<(), GeoLedgerError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoLedgerError::invalid(
            "Geo collateral ledger digests must use blake3:<hex>",
            [("field", field), ("value", value)],
        ));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoLedgerError::invalid(
            "Geo collateral ledger digests must use lowercase blake3:<hex>",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_prefixed_blake3_or_empty_for_reach_none(
    field: &str,
    value: &str,
    reach: GeoCandidateReachStatus,
) -> Result<(), GeoLedgerError> {
    if reach == GeoCandidateReachStatus::None && value.is_empty() {
        Ok(())
    } else {
        validate_prefixed_blake3(field, value)
    }
}
