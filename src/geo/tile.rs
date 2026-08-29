#![forbid(unsafe_code)]

//! Deterministic H3 work-unit ownership and cross-boundary reconciliation.
//!
//! H3 is a blocking and ownership index here, never a geometric truth
//! predicate. Upstream ingest supplies one declared home cell per feature.
//! Canon validates the cell encoding and work-unit consistency, not the
//! coordinate-to-cell truth, then builds a bounded center-plus-halo work unit.
//! A decision emits only from the minimum member home cell. Adjacent work units
//! may observe the same decision, but reconciliation either produces one owned
//! decision or refuses an orphan/non-confluent boundary result.

use h3o::CellIndex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    str::FromStr,
};

pub const CANON_GEO_TILE_WORK_REQUEST_VERSION: &str = "canon_geo_tile_work_request.v0";
pub const CANON_GEO_TILE_WORK_UNIT_VERSION: &str = "canon_geo_tile_work_unit.v0";
pub const CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION: &str =
    "canon_geo_tile_reconciliation_request.v0";
pub const CANON_GEO_TILE_RECONCILIATION_VERSION: &str = "canon_geo_tile_reconciliation.v0";

const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_FEATURES_PER_WORK_UNIT: u64 = 1_000_000;
const MAX_WORK_CELLS: u64 = 100_000;
const MAX_RECONCILIATION_BATCHES: u64 = 100_000;
const MAX_RECONCILIATION_PROPOSALS: u64 = 1_000_000;
const MAX_MEMBERS_PER_DECISION: u64 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileFeatureRef {
    pub source_name: String,
    pub feature_id: String,
    /// Canonical H3 cell containing the feature's declared representative
    /// point. Cell computation belongs to ingest; this contract makes the
    /// ownership input explicit and auditable.
    pub home_cell: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileWorkRequest {
    pub version: String,
    pub center_cell: String,
    pub halo_k: u32,
    pub features: Vec<GeoTileFeatureRef>,
    pub max_features: u64,
    /// Maximum total cells in the center-plus-halo disk, including the center.
    pub max_work_cells: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoTilePlacement {
    Center,
    Halo,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileFeatureMembership {
    pub source_name: String,
    pub feature_id: String,
    pub home_cell: String,
    pub placement: GeoTilePlacement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileWorkUnitArtifact {
    pub version: String,
    pub request_version: String,
    pub center_cell: String,
    pub h3_resolution: u8,
    pub halo_k: u32,
    /// Deterministically sorted center-plus-halo cells, including center_cell.
    pub work_cells: Vec<String>,
    pub features: Vec<GeoTileFeatureMembership>,
    pub center_feature_count: u64,
    pub halo_feature_count: u64,
    pub max_features: u64,
    pub max_work_cells: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileDecisionMember {
    pub source_name: String,
    pub feature_id: String,
    pub home_cell: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileDecisionProposal {
    /// Digest of the complete decision payload produced by the local solver.
    /// Reconciliation does not interpret or merge payloads.
    pub payload_blake3: String,
    pub members: Vec<GeoTileDecisionMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileDecisionBatch {
    /// Exact bounded work unit supplied to the local solver that produced the
    /// proposals below.
    pub work_unit: GeoTileWorkUnitArtifact,
    pub proposals: Vec<GeoTileDecisionProposal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileReconciliationRequest {
    pub version: String,
    pub halo_k: u32,
    pub batches: Vec<GeoTileDecisionBatch>,
    pub max_batches: u64,
    pub max_proposals: u64,
    pub max_members_per_decision: u64,
    pub max_features_per_batch: u64,
    /// Maximum total cells in each center-plus-halo disk, including its center.
    pub max_work_cells_per_batch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileBatchReceipt {
    pub center_cell: String,
    pub work_unit_blake3: String,
    pub proposal_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoReconciledTileDecision {
    pub decision_id: String,
    pub owner_cell: String,
    pub payload_blake3: String,
    pub members: Vec<GeoTileDecisionMember>,
    /// Number of independently executed center cells that observed the same
    /// canonical member set and payload.
    pub proposal_copies: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileReconciliationArtifact {
    pub version: String,
    pub request_version: String,
    pub h3_resolution: u8,
    pub halo_k: u32,
    pub batches: u64,
    pub input_proposals: u64,
    pub owned_decisions: u64,
    pub discarded_halo_proposals: u64,
    pub batch_receipts: Vec<GeoTileBatchReceipt>,
    pub decisions: Vec<GeoReconciledTileDecision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoTileErrorCode {
    UnsupportedVersion,
    InvalidInput,
    InvalidH3Cell,
    ResolutionMismatch,
    HaloBudgetExceeded,
    FeatureBudgetExceeded,
    ReconciliationBudgetExceeded,
    FeatureOutsideHalo,
    DuplicateFeature,
    DuplicateCenter,
    InvalidWorkUnit,
    InvalidDecision,
    NonConfluentDecision,
    MissingOwnerWorkUnit,
    OrphanedDecision,
    ArithmeticOverflow,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoTileError {
    pub code: GeoTileErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoTileError {
    fn new(
        code: GeoTileErrorCode,
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

    fn overflow(field: &str) -> Self {
        Self::new(
            GeoTileErrorCode::ArithmeticOverflow,
            "Geo tile arithmetic exceeded checked integer bounds",
            [("field", field)],
        )
    }
}

impl fmt::Display for GeoTileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoTileError {}

#[derive(Debug)]
struct DecisionAccumulator {
    membership_blake3: String,
    payload_blake3: String,
    owner_cell: CellIndex,
    members: Vec<GeoTileDecisionMember>,
    proposal_centers: BTreeSet<CellIndex>,
}

type FeatureHomeCells = BTreeMap<(String, String), CellIndex>;

#[derive(Debug)]
struct ValidatedWorkUnit {
    center: CellIndex,
    feature_home_cells: FeatureHomeCells,
    blake3: String,
}

/// Build one exact center-plus-halo feature work unit.
///
/// Supplied features outside the declared disk refuse instead of being silently
/// dropped. This makes request-local reach defects visible, but cannot prove
/// that the upstream candidate generator supplied every relevant feature.
pub fn materialize_tile_work_unit(
    request: &GeoTileWorkRequest,
) -> Result<GeoTileWorkUnitArtifact, GeoTileError> {
    if request.version != CANON_GEO_TILE_WORK_REQUEST_VERSION {
        return Err(GeoTileError::new(
            GeoTileErrorCode::UnsupportedVersion,
            "Unsupported Geo tile-work request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_TILE_WORK_REQUEST_VERSION),
            ],
        ));
    }
    validate_budget(
        "max_features",
        request.max_features,
        MAX_FEATURES_PER_WORK_UNIT,
    )?;
    validate_budget("max_work_cells", request.max_work_cells, MAX_WORK_CELLS)?;
    let feature_count = usize_to_u64(request.features.len(), "features.len")?;
    if feature_count > request.max_features {
        return Err(GeoTileError::new(
            GeoTileErrorCode::FeatureBudgetExceeded,
            "Geo tile work unit exceeds the declared feature budget",
            [
                ("observed", feature_count.to_string()),
                ("configured", request.max_features.to_string()),
            ],
        ));
    }

    let center = parse_cell(&request.center_cell, "center_cell")?;
    let disk = bounded_grid_disk(center, request.halo_k, request.max_work_cells)?;
    let center_cell = center.to_string();
    let resolution = center.resolution();
    let mut seen = BTreeSet::new();
    let mut features = Vec::with_capacity(request.features.len());
    let mut center_feature_count = 0_u64;
    let mut halo_feature_count = 0_u64;

    for feature in &request.features {
        validate_identifier("source_name", &feature.source_name)?;
        validate_identifier("feature_id", &feature.feature_id)?;
        let key = (feature.source_name.clone(), feature.feature_id.clone());
        if !seen.insert(key) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::DuplicateFeature,
                "Geo tile work unit contains a duplicate source feature",
                [
                    ("source_name", feature.source_name.as_str()),
                    ("feature_id", feature.feature_id.as_str()),
                ],
            ));
        }
        let home = parse_cell(&feature.home_cell, "features.home_cell")?;
        require_resolution(center, home, &feature.home_cell)?;
        if !disk.contains(&home) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::FeatureOutsideHalo,
                "Geo feature lies outside the declared center-plus-halo work unit",
                [
                    ("source_name", feature.source_name.clone()),
                    ("feature_id", feature.feature_id.clone()),
                    ("home_cell", home.to_string()),
                    ("center_cell", center_cell.clone()),
                    ("halo_k", request.halo_k.to_string()),
                ],
            ));
        }
        let placement = if home == center {
            center_feature_count = checked_add(center_feature_count, 1, "center_feature_count")?;
            GeoTilePlacement::Center
        } else {
            halo_feature_count = checked_add(halo_feature_count, 1, "halo_feature_count")?;
            GeoTilePlacement::Halo
        };
        features.push(GeoTileFeatureMembership {
            source_name: feature.source_name.clone(),
            feature_id: feature.feature_id.clone(),
            home_cell: home.to_string(),
            placement,
        });
    }
    features.sort();

    Ok(GeoTileWorkUnitArtifact {
        version: CANON_GEO_TILE_WORK_UNIT_VERSION.to_string(),
        request_version: request.version.clone(),
        center_cell,
        h3_resolution: u8::from(resolution),
        halo_k: request.halo_k,
        work_cells: disk.into_iter().map(|cell| cell.to_string()).collect(),
        features,
        center_feature_count,
        halo_feature_count,
        max_features: request.max_features,
        max_work_cells: request.max_work_cells,
    })
}

/// Reconcile independently executed tile decisions into one owned result per
/// canonical member set.
///
/// The owner is the numerically smallest H3 home cell among the decision's
/// members. That rule is independent of source name, iteration order, and which
/// neighboring work unit happened to finish first.
pub fn reconcile_tile_decisions(
    request: &GeoTileReconciliationRequest,
) -> Result<GeoTileReconciliationArtifact, GeoTileError> {
    if request.version != CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION {
        return Err(GeoTileError::new(
            GeoTileErrorCode::UnsupportedVersion,
            "Unsupported Geo tile-reconciliation request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION),
            ],
        ));
    }
    validate_budget(
        "max_batches",
        request.max_batches,
        MAX_RECONCILIATION_BATCHES,
    )?;
    validate_budget(
        "max_proposals",
        request.max_proposals,
        MAX_RECONCILIATION_PROPOSALS,
    )?;
    validate_budget(
        "max_members_per_decision",
        request.max_members_per_decision,
        MAX_MEMBERS_PER_DECISION,
    )?;
    validate_budget(
        "max_features_per_batch",
        request.max_features_per_batch,
        MAX_FEATURES_PER_WORK_UNIT,
    )?;
    validate_budget(
        "max_work_cells_per_batch",
        request.max_work_cells_per_batch,
        MAX_WORK_CELLS,
    )?;
    let batch_count = usize_to_u64(request.batches.len(), "batches.len")?;
    if batch_count == 0 || batch_count > request.max_batches {
        return Err(GeoTileError::new(
            GeoTileErrorCode::ReconciliationBudgetExceeded,
            "Geo tile reconciliation batch count is empty or over budget",
            [
                ("observed", batch_count.to_string()),
                ("configured", request.max_batches.to_string()),
            ],
        ));
    }

    let mut batch_centers = BTreeSet::new();
    let mut expected_resolution = None;
    let mut input_proposals = 0_u64;
    let mut decisions: BTreeMap<String, DecisionAccumulator> = BTreeMap::new();
    let mut batch_receipts = Vec::with_capacity(request.batches.len());

    for batch in &request.batches {
        let feature_count = usize_to_u64(batch.work_unit.features.len(), "work_unit.features.len")?;
        if feature_count > request.max_features_per_batch {
            return Err(GeoTileError::new(
                GeoTileErrorCode::ReconciliationBudgetExceeded,
                "Geo tile reconciliation work unit exceeds the per-batch feature budget",
                [
                    ("observed", feature_count.to_string()),
                    ("configured", request.max_features_per_batch.to_string()),
                ],
            ));
        }
        let work_cell_count =
            usize_to_u64(batch.work_unit.work_cells.len(), "work_unit.work_cells.len")?;
        if work_cell_count > request.max_work_cells_per_batch {
            return Err(GeoTileError::new(
                GeoTileErrorCode::ReconciliationBudgetExceeded,
                "Geo tile reconciliation work unit exceeds the per-batch cell budget",
                [
                    ("observed", work_cell_count.to_string()),
                    ("configured", request.max_work_cells_per_batch.to_string()),
                ],
            ));
        }
        if batch.work_unit.halo_k != request.halo_k {
            return Err(GeoTileError::new(
                GeoTileErrorCode::InvalidWorkUnit,
                "Geo tile reconciliation work unit uses a different halo radius",
                [
                    ("expected_halo_k", request.halo_k.to_string()),
                    ("actual_halo_k", batch.work_unit.halo_k.to_string()),
                    ("center_cell", batch.work_unit.center_cell.clone()),
                ],
            ));
        }
        let validated_work_unit = validate_work_unit_artifact(&batch.work_unit)?;
        let center = validated_work_unit.center;
        if let Some(resolution) = expected_resolution {
            if center.resolution() != resolution {
                return Err(resolution_error(
                    resolution,
                    center,
                    &batch.work_unit.center_cell,
                ));
            }
        } else {
            expected_resolution = Some(center.resolution());
        }
        if !batch_centers.insert(center) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::DuplicateCenter,
                "Geo tile reconciliation contains duplicate center batches",
                [("center_cell", center.to_string())],
            ));
        }
        let mut batch_memberships = BTreeSet::new();
        batch_receipts.push(GeoTileBatchReceipt {
            center_cell: center.to_string(),
            work_unit_blake3: validated_work_unit.blake3,
            proposal_count: usize_to_u64(batch.proposals.len(), "batch.proposals.len")?,
        });

        for proposal in &batch.proposals {
            input_proposals = checked_add(input_proposals, 1, "input_proposals")?;
            if input_proposals > request.max_proposals {
                return Err(GeoTileError::new(
                    GeoTileErrorCode::ReconciliationBudgetExceeded,
                    "Geo tile reconciliation exceeds the proposal budget",
                    [
                        ("observed", input_proposals.to_string()),
                        ("configured", request.max_proposals.to_string()),
                    ],
                ));
            }
            validate_blake3(&proposal.payload_blake3)?;
            let members = normalize_members(
                &proposal.members,
                center,
                &validated_work_unit.feature_home_cells,
                request.max_members_per_decision,
            )?;
            let member_bytes = serde_json::to_vec(&members).map_err(|error| {
                GeoTileError::new(
                    GeoTileErrorCode::Serialization,
                    "Geo tile decision members could not be serialized",
                    [("error", error.to_string())],
                )
            })?;
            let membership_blake3 = blake3::hash(&member_bytes).to_hex().to_string();
            if !batch_memberships.insert(membership_blake3.clone()) {
                return Err(GeoTileError::new(
                    GeoTileErrorCode::InvalidDecision,
                    "A center batch proposed the same decision membership more than once",
                    [
                        ("center_cell", center.to_string()),
                        ("membership_blake3", membership_blake3.clone()),
                    ],
                ));
            }
            let owner_cell = members
                .iter()
                .map(|member| {
                    parse_cell(&member.home_cell, "proposals.members.home_cell")
                        .expect("normalized members contain validated cells")
                })
                .min()
                .expect("normalized members are non-empty");

            match decisions.get_mut(&membership_blake3) {
                Some(existing) => {
                    if existing.payload_blake3 != proposal.payload_blake3 {
                        return Err(GeoTileError::new(
                            GeoTileErrorCode::NonConfluentDecision,
                            "Adjacent tile work units produced different payloads for the same members",
                            [
                                ("membership_blake3", membership_blake3.clone()),
                                ("first_payload", existing.payload_blake3.clone()),
                                ("second_payload", proposal.payload_blake3.clone()),
                                ("second_center", center.to_string()),
                            ],
                        ));
                    }
                    existing.proposal_centers.insert(center);
                }
                None => {
                    decisions.insert(
                        membership_blake3.clone(),
                        DecisionAccumulator {
                            membership_blake3,
                            payload_blake3: proposal.payload_blake3.clone(),
                            owner_cell,
                            members,
                            proposal_centers: BTreeSet::from([center]),
                        },
                    );
                }
            }
        }
    }

    let resolution = expected_resolution.expect("non-empty batches establish resolution");
    let mut discarded_halo_proposals = 0_u64;
    let mut reconciled = Vec::with_capacity(decisions.len());
    for (_, decision) in decisions {
        if !batch_centers.contains(&decision.owner_cell) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::MissingOwnerWorkUnit,
                "Geo tile reconciliation is missing the decision owner work unit",
                [
                    ("owner_cell", decision.owner_cell.to_string()),
                    ("membership_blake3", decision.membership_blake3.clone()),
                ],
            ));
        }
        if !decision.proposal_centers.contains(&decision.owner_cell) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::OrphanedDecision,
                "A boundary decision was observed only in halo work units",
                [
                    ("owner_cell", decision.owner_cell.to_string()),
                    ("membership_blake3", decision.membership_blake3.clone()),
                ],
            ));
        }
        let proposal_copies = usize_to_u64(
            decision.proposal_centers.len(),
            "decision.proposal_centers.len",
        )?;
        discarded_halo_proposals = checked_add(
            discarded_halo_proposals,
            proposal_copies.saturating_sub(1),
            "discarded_halo_proposals",
        )?;
        reconciled.push(GeoReconciledTileDecision {
            decision_id: decision_id(&decision.membership_blake3, &decision.payload_blake3),
            owner_cell: decision.owner_cell.to_string(),
            payload_blake3: decision.payload_blake3,
            members: decision.members,
            proposal_copies,
        });
    }
    reconciled.sort_by(|left, right| left.decision_id.cmp(&right.decision_id));
    batch_receipts.sort();
    let owned_decisions = usize_to_u64(reconciled.len(), "decisions.len")?;

    Ok(GeoTileReconciliationArtifact {
        version: CANON_GEO_TILE_RECONCILIATION_VERSION.to_string(),
        request_version: request.version.clone(),
        h3_resolution: u8::from(resolution),
        halo_k: request.halo_k,
        batches: batch_count,
        input_proposals,
        owned_decisions,
        discarded_halo_proposals,
        batch_receipts,
        decisions: reconciled,
    })
}

pub fn canonical_tile_work_unit_bytes(
    artifact: &GeoTileWorkUnitArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

pub fn canonical_tile_reconciliation_bytes(
    artifact: &GeoTileReconciliationArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

fn normalize_members(
    members: &[GeoTileDecisionMember],
    center: CellIndex,
    available_features: &FeatureHomeCells,
    max_members: u64,
) -> Result<Vec<GeoTileDecisionMember>, GeoTileError> {
    let count = usize_to_u64(members.len(), "proposal.members.len")?;
    if count == 0 || count > max_members {
        return Err(GeoTileError::new(
            GeoTileErrorCode::ReconciliationBudgetExceeded,
            "Geo tile decision member count is empty or over budget",
            [
                ("observed", count.to_string()),
                ("configured", max_members.to_string()),
            ],
        ));
    }
    let mut normalized = Vec::with_capacity(members.len());
    let mut seen = BTreeSet::new();
    for member in members {
        validate_identifier("source_name", &member.source_name)?;
        validate_identifier("feature_id", &member.feature_id)?;
        let key = (member.source_name.clone(), member.feature_id.clone());
        if !seen.insert(key) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::InvalidDecision,
                "Geo tile decision contains a duplicate source feature",
                [
                    ("source_name", member.source_name.as_str()),
                    ("feature_id", member.feature_id.as_str()),
                ],
            ));
        }
        let home = parse_cell(&member.home_cell, "proposals.members.home_cell")?;
        require_resolution(center, home, &member.home_cell)?;
        let expected_home =
            available_features.get(&(member.source_name.clone(), member.feature_id.clone()));
        if expected_home != Some(&home) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::InvalidDecision,
                "Geo tile decision references a member absent from its producing work unit",
                [
                    ("center_cell", center.to_string()),
                    ("source_name", member.source_name.clone()),
                    ("feature_id", member.feature_id.clone()),
                    ("home_cell", home.to_string()),
                ],
            ));
        }
        normalized.push(GeoTileDecisionMember {
            source_name: member.source_name.clone(),
            feature_id: member.feature_id.clone(),
            home_cell: home.to_string(),
        });
    }
    normalized.sort();
    Ok(normalized)
}

fn validate_work_unit_artifact(
    artifact: &GeoTileWorkUnitArtifact,
) -> Result<ValidatedWorkUnit, GeoTileError> {
    let request = GeoTileWorkRequest {
        version: artifact.request_version.clone(),
        center_cell: artifact.center_cell.clone(),
        halo_k: artifact.halo_k,
        features: artifact
            .features
            .iter()
            .map(|feature| GeoTileFeatureRef {
                source_name: feature.source_name.clone(),
                feature_id: feature.feature_id.clone(),
                home_cell: feature.home_cell.clone(),
            })
            .collect(),
        max_features: artifact.max_features,
        max_work_cells: artifact.max_work_cells,
    };
    let expected = materialize_tile_work_unit(&request).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::InvalidWorkUnit,
            "Geo tile reconciliation contains an invalid work-unit artifact",
            [
                ("center_cell", artifact.center_cell.clone()),
                ("cause", geo_tile_error_code_name(error.code)),
                ("cause_message", error.message),
            ],
        )
    })?;
    if &expected != artifact {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidWorkUnit,
            "Geo tile reconciliation work-unit artifact is not canonical",
            [("center_cell", artifact.center_cell.clone())],
        ));
    }
    let bytes = canonical_tile_work_unit_bytes(artifact).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::Serialization,
            "Geo tile work-unit receipt could not be serialized",
            [("error", error.to_string())],
        )
    })?;
    let mut available_features = BTreeMap::new();
    for feature in &artifact.features {
        let home = parse_cell(&feature.home_cell, "work_unit.features.home_cell")?;
        available_features.insert(
            (feature.source_name.clone(), feature.feature_id.clone()),
            home,
        );
    }
    let center = parse_cell(&artifact.center_cell, "work_unit.center_cell")?;
    let digest = format!("blake3:{}", blake3::hash(&bytes).to_hex());
    Ok(ValidatedWorkUnit {
        center,
        feature_home_cells: available_features,
        blake3: digest,
    })
}

fn geo_tile_error_code_name(code: GeoTileErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn parse_cell(value: &str, field: &str) -> Result<CellIndex, GeoTileError> {
    let cell = CellIndex::from_str(value).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::InvalidH3Cell,
            "Geo tile contract contains an invalid H3 cell",
            [
                ("field", field.to_string()),
                ("value", value.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    if cell.to_string() != value {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidH3Cell,
            "Geo tile contract contains a non-canonical H3 cell encoding",
            [
                ("field", field.to_string()),
                ("value", value.to_string()),
                ("canonical", cell.to_string()),
            ],
        ));
    }
    Ok(cell)
}

fn require_resolution(
    expected: CellIndex,
    actual: CellIndex,
    actual_text: &str,
) -> Result<(), GeoTileError> {
    if expected.resolution() != actual.resolution() {
        return Err(resolution_error(expected.resolution(), actual, actual_text));
    }
    Ok(())
}

fn resolution_error(
    expected: h3o::Resolution,
    actual: CellIndex,
    actual_text: &str,
) -> GeoTileError {
    GeoTileError::new(
        GeoTileErrorCode::ResolutionMismatch,
        "Geo tile cells use different H3 resolutions",
        [
            ("expected_resolution", u8::from(expected).to_string()),
            (
                "actual_resolution",
                u8::from(actual.resolution()).to_string(),
            ),
            ("actual_cell", actual_text.to_string()),
        ],
    )
}

fn bounded_grid_disk(
    center: CellIndex,
    halo_k: u32,
    max_work_cells: u64,
) -> Result<BTreeSet<CellIndex>, GeoTileError> {
    let k = u64::from(halo_k);
    let theoretical = k
        .checked_add(1)
        .and_then(|next| k.checked_mul(next))
        .and_then(|product| product.checked_mul(3))
        .and_then(|product| product.checked_add(1))
        .ok_or_else(|| GeoTileError::overflow("halo_cell_upper_bound"))?;
    if theoretical > max_work_cells {
        return Err(GeoTileError::new(
            GeoTileErrorCode::HaloBudgetExceeded,
            "Geo tile halo exceeds the declared cell budget before enumeration",
            [
                ("halo_k", halo_k.to_string()),
                ("upper_bound", theoretical.to_string()),
                ("configured", max_work_cells.to_string()),
            ],
        ));
    }
    let disk = center.grid_disk_safe(halo_k).collect::<BTreeSet<_>>();
    let observed = usize_to_u64(disk.len(), "work_cells.len")?;
    if observed > max_work_cells {
        return Err(GeoTileError::new(
            GeoTileErrorCode::HaloBudgetExceeded,
            "Geo tile halo exceeds the declared cell budget",
            [
                ("observed", observed.to_string()),
                ("configured", max_work_cells.to_string()),
            ],
        ));
    }
    Ok(disk)
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoTileError> {
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInput,
            "Geo tile identifier is empty or exceeds its byte budget",
            [
                ("field", field.to_string()),
                ("bytes", value.len().to_string()),
                ("max_bytes", MAX_IDENTIFIER_BYTES.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_blake3(value: &str) -> Result<(), GeoTileError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(invalid_digest(value));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_digest(value));
    }
    Ok(())
}

fn invalid_digest(value: &str) -> GeoTileError {
    GeoTileError::new(
        GeoTileErrorCode::InvalidDecision,
        "Geo tile decision payload digest must be canonical lowercase BLAKE3",
        [("payload_blake3", value)],
    )
}

fn decision_id(membership_blake3: &str, payload_blake3: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"canon_geo_tile_decision.v0\0");
    hasher.update(membership_blake3.as_bytes());
    hasher.update(b"\0");
    hasher.update(payload_blake3.as_bytes());
    format!("geo-decision:{}", hasher.finalize().to_hex())
}

fn validate_budget(field: &str, value: u64, hard_max: u64) -> Result<(), GeoTileError> {
    if value == 0 || value > hard_max {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInput,
            "Geo tile budget must be positive and within the kernel ceiling",
            [
                ("field", field.to_string()),
                ("configured", value.to_string()),
                ("hard_max", hard_max.to_string()),
            ],
        ));
    }
    Ok(())
}

fn checked_add(left: u64, right: u64, field: &str) -> Result<u64, GeoTileError> {
    left.checked_add(right)
        .ok_or_else(|| GeoTileError::overflow(field))
}

fn usize_to_u64(value: usize, field: &str) -> Result<u64, GeoTileError> {
    u64::try_from(value).map_err(|_| GeoTileError::overflow(field))
}
