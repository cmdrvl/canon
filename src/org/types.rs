//! Shared org-identity contracts.
//!
//! These types intentionally live in one scaffold-owned file so downstream
//! implementation beads can work in parallel without redefining shared schema.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, error::Error, fmt};

pub const CANON_ORG_PROJECTION_VERSION: &str = "canon_org_projection.v0";
pub const CANON_ORG_BLOCK_VERSION: &str = "canon_org_block.v0";
pub const CANON_ORG_EDGE_VERSION: &str = "canon_org_edge.v0";
pub const CANON_ORG_SOLVE_VERSION: &str = "canon_org_solve.v0";
pub const CANON_ORG_RUN_VERSION: &str = "canon_org_run.v0";
pub const CANON_ORG_EXPLAIN_VERSION: &str = "canon_org_explain.v0";
pub const CANON_ORG_AUDIT_VERSION: &str = "canon_org_audit.v0";
pub const CANON_ORG_PROMOTE_VERSION: &str = "canon_org_promote.v0";

pub type OrgResult<T> = Result<T, OrgError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrgErrorCode {
    Strategy,
    InputContract,
    Registry,
    ArtifactContract,
    Audit,
    Promotion,
    Explain,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrgError {
    pub code: OrgErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

impl OrgError {
    pub fn new(code: OrgErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(code: OrgErrorCode, message: impl Into<String>, detail: Value) -> Self {
        Self {
            code,
            message: message.into(),
            detail: Some(detail),
        }
    }

    pub fn unimplemented(area: &'static str) -> Self {
        Self::new(
            OrgErrorCode::Unimplemented,
            format!("org scaffold placeholder for {}", area),
        )
    }
}

impl fmt::Display for OrgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for OrgError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StrategyReference {
    pub id: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RegistrySnapshot {
    pub id: String,
    pub version: String,
    pub source: String,
    pub lookup_snapshot_hash: String,
    pub escrow_snapshot_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OrgStrategy {
    #[serde(rename = "strategy_id")]
    pub id: String,
    #[serde(rename = "strategy_version")]
    pub version: String,
    pub entity_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub id_prefix: String,
    pub observations: StrategyObservations,
    pub normalize: NormalizeConfig,
    #[serde(default)]
    pub blocking: Vec<StrategyOperator>,
    pub evidence: StrategyEvidence,
    pub solver: StrategySolver,
    pub reconcile: StrategyReconcile,
    pub anchors: StrategyAnchors,
    pub promotion: StrategyPromotion,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content_hash: String,
}

impl OrgStrategy {
    pub fn reference(&self) -> StrategyReference {
        StrategyReference {
            id: self.id.clone(),
            version: self.version.clone(),
            content_hash: self.content_hash.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StrategyObservations {
    #[serde(default)]
    pub name_fields: Vec<String>,
    #[serde(default)]
    pub required_side_fields: Vec<OrgSideField>,
    #[serde(default)]
    pub context_fields: Vec<String>,
    #[serde(default)]
    pub anchor_fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrgSideField {
    #[default]
    AliasSurfacesJson,
    MentionSurfacesJson,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NormalizeConfig {
    #[serde(default)]
    pub views: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StrategyOperator {
    pub op: String,
    #[serde(flatten, default)]
    pub params: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StrategyEvidence {
    #[serde(default)]
    pub must_link: Vec<StrategyOperator>,
    #[serde(default)]
    pub support: Vec<StrategyOperator>,
    #[serde(default)]
    pub cannot_link: Vec<StrategyOperator>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StrategySolver {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub score_mode: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub component_score_mode: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub merge_policy: String,
    #[serde(default)]
    pub backbone_score_min: i64,
    #[serde(default)]
    pub backbone_requires_positive_name: bool,
    #[serde(default)]
    pub attach_score_min: i64,
    #[serde(default)]
    pub abstain_margin: i64,
    #[serde(default)]
    pub max_cluster_diameter: u32,
    #[serde(default)]
    pub require_positive_name_evidence: bool,
    #[serde(default)]
    pub attach_requires_backbone_contact: bool,
    #[serde(default)]
    pub score_against_backbone_only: bool,
    #[serde(default)]
    pub attachments_do_not_chain: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StrategyReconcile {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub single_incumbent_overlap: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub multi_incumbent_overlap: String,
    #[serde(default)]
    pub allow_incumbent_merge: bool,
    #[serde(default)]
    pub allow_alias_writeback_for_resolved_existing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StrategyAnchors {
    #[serde(default)]
    pub precedence: Vec<String>,
    #[serde(default)]
    pub trusted_for_must_link: Vec<String>,
    #[serde(default)]
    pub trusted_for_single_doc_promotion: Vec<String>,
    #[serde(default)]
    pub support_only: Vec<String>,
    #[serde(default)]
    pub require_unique_for_attachment: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StrategyPromotion {
    #[serde(default)]
    pub write_states: Vec<OrgEntityState>,
    #[serde(default)]
    pub require_zero_anchor_conflicts: bool,
    #[serde(default)]
    pub require_holdout_non_regression: bool,
    #[serde(default)]
    pub require_perturbation_stability_gte: f64,
    #[serde(default)]
    pub min_distinct_docs: u32,
    #[serde(default)]
    pub allow_single_doc_if_unique_anchor: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectedObservation {
    pub version: String,
    pub source_row_id: String,
    pub doc_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of_date: Option<String>,
    pub primary_surface: ProjectedSurface,
    #[serde(default)]
    pub alias_surfaces: Vec<ProjectedSurface>,
    #[serde(default)]
    pub mention_surfaces: Vec<ProjectedSurface>,
    #[serde(default)]
    pub anchors: Vec<ProjectedAnchor>,
    #[serde(default)]
    pub context: BTreeMap<String, Value>,
    #[serde(default)]
    pub provenance: BTreeMap<String, Vec<String>>,
}

impl Default for ProjectedObservation {
    fn default() -> Self {
        Self {
            version: CANON_ORG_PROJECTION_VERSION.to_string(),
            source_row_id: String::new(),
            doc_id: String::new(),
            as_of_date: None,
            primary_surface: ProjectedSurface::default(),
            alias_surfaces: Vec::new(),
            mention_surfaces: Vec::new(),
            anchors: Vec::new(),
            context: BTreeMap::new(),
            provenance: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectedSurface {
    pub value: String,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AnchorValue {
    pub namespace: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectedAnchor {
    pub namespace: String,
    pub value: String,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RowPair {
    pub left_row_id: String,
    pub right_row_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AliasMappingEntry {
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TrustedAnchorRecord {
    pub canonical_id: String,
    pub namespace: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PendingClusterRecord {
    pub escrow_id: String,
    pub profile: String,
    #[serde(default)]
    pub doc_ids: Vec<String>,
    #[serde(default)]
    pub surfaces: Vec<String>,
    #[serde(default)]
    pub anchors: Vec<AnchorValue>,
    #[serde(default)]
    pub witness_pairs: Vec<RowPair>,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CannotLinkFact {
    pub left_key: String,
    pub right_key: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IncumbentMemory {
    pub registry: RegistrySnapshot,
    #[serde(default)]
    pub alias_entries: Vec<AliasMappingEntry>,
    #[serde(default)]
    pub trusted_anchors: Vec<TrustedAnchorRecord>,
    #[serde(default)]
    pub pending_clusters: Vec<PendingClusterRecord>,
    #[serde(default)]
    pub cannot_link_facts: Vec<CannotLinkFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BlockHit {
    pub operator_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockRecord {
    pub version: String,
    pub strategy: StrategyReference,
    pub registry_snapshot: RegistrySnapshot,
    pub left_row_id: String,
    pub right_row_id: String,
    #[serde(default)]
    pub block_hits: Vec<BlockHit>,
}

impl Default for BlockRecord {
    fn default() -> Self {
        Self {
            version: CANON_ORG_BLOCK_VERSION.to_string(),
            strategy: StrategyReference::default(),
            registry_snapshot: RegistrySnapshot::default(),
            left_row_id: String::new(),
            right_row_id: String::new(),
            block_hits: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    #[default]
    Support,
    MustLink,
    CannotLink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceHit {
    pub kind: EvidenceKind,
    pub namespace: String,
    pub operator_id: String,
    pub score: i64,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeRecord {
    pub version: String,
    pub strategy: StrategyReference,
    pub registry_snapshot: RegistrySnapshot,
    pub left_row_id: String,
    pub right_row_id: String,
    #[serde(default)]
    pub hits: Vec<EvidenceHit>,
    #[serde(default)]
    pub pair_score_by_namespace: BTreeMap<String, i64>,
    pub pair_score_total: i64,
    pub has_must_link: bool,
    pub has_cannot_link: bool,
}

impl Default for EdgeRecord {
    fn default() -> Self {
        Self {
            version: CANON_ORG_EDGE_VERSION.to_string(),
            strategy: StrategyReference::default(),
            registry_snapshot: RegistrySnapshot::default(),
            left_row_id: String::new(),
            right_row_id: String::new(),
            hits: Vec::new(),
            pair_score_by_namespace: BTreeMap::new(),
            pair_score_total: 0,
            has_must_link: false,
            has_cannot_link: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrgEntityState {
    ResolvedExisting,
    PromotableNew,
    AbstainLowEvidence,
    AbstainConflict,
    #[default]
    Contradiction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InheritanceMode {
    #[default]
    NoIncumbentOverlap,
    SingleIncumbentOverlap,
    MultiIncumbentOverlap,
    TrustedAnchorConflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InheritanceRecord {
    pub mode: InheritanceMode,
    #[serde(default)]
    pub incumbent_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EscrowActionKind {
    #[default]
    UpsertPending,
    EmitCannotLink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EscrowActionRecord {
    pub action: EscrowActionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escrow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cannot_link: Option<CannotLinkFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MergeWitness {
    pub left_row_id: String,
    pub right_row_id: String,
    pub pair_score_total: i64,
    #[serde(default)]
    pub pair_score_by_namespace: BTreeMap<String, i64>,
    #[serde(default)]
    pub operator_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SolvedEntity {
    pub state: OrgEntityState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    #[serde(default)]
    pub backbone_rows: Vec<String>,
    #[serde(default)]
    pub attached_rows: Vec<String>,
    #[serde(default)]
    pub all_rows: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub anchors: Vec<AnchorValue>,
    #[serde(default)]
    pub merge_witnesses: Vec<MergeWitness>,
    pub inheritance: InheritanceRecord,
    #[serde(default)]
    pub eligible_writeback_aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escrow: Option<EscrowActionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AbstentionRecord {
    pub state: OrgEntityState,
    #[serde(default)]
    pub all_rows: Vec<String>,
    pub reason: String,
    #[serde(default)]
    pub incumbent_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escrow: Option<EscrowActionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContradictionRecord {
    pub reason: String,
    #[serde(default)]
    pub row_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RegistryPatchSummary {
    #[serde(default)]
    pub mapping_files: Vec<String>,
    pub new_entity_entries: u64,
    pub existing_alias_entries: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EscrowPatchSummary {
    pub pending_cluster_entries: u64,
    pub cannot_link_entries: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SolveRunSummary {
    pub observations: u64,
    pub resolved_existing: u64,
    pub promotable_new: u64,
    pub abstain_low_evidence: u64,
    pub abstain_conflict: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SolveRunArtifact {
    pub version: String,
    pub strategy: StrategyReference,
    pub registry: RegistrySnapshot,
    pub summary: SolveRunSummary,
    #[serde(default)]
    pub entities: Vec<SolvedEntity>,
    #[serde(default)]
    pub abstentions: Vec<AbstentionRecord>,
    #[serde(default)]
    pub contradictions: Vec<ContradictionRecord>,
    pub proposed_registry_patch: RegistryPatchSummary,
    pub proposed_escrow_patch: EscrowPatchSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExplainQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escrow_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExplainResult {
    pub state: OrgEntityState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escrow_id: Option<String>,
    #[serde(default)]
    pub backbone_rows: Vec<String>,
    #[serde(default)]
    pub attached_rows: Vec<String>,
    pub inheritance: InheritanceRecord,
    #[serde(default)]
    pub witness_chain: Vec<MergeWitness>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExplainArtifact {
    pub version: String,
    pub query: ExplainQuery,
    pub result: ExplainResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResultReference {
    pub version: String,
    pub content_hash: String,
    pub strategy_content_hash: String,
    pub lookup_snapshot_hash: String,
    pub escrow_snapshot_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SuiteReference {
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PromotionDecision {
    #[default]
    Promote,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AuditSummary {
    pub decision: PromotionDecision,
    pub hard_gates_passed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AuditMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gold_pair_f1: Option<f64>,
    pub anchor_consistency: f64,
    pub anchor_conflicts: u64,
    pub holdout_score: f64,
    pub contradiction_rate: f64,
    pub perturbation_stability: f64,
    pub continuity_gain: f64,
    pub compression_gain: f64,
    pub registry_churn: f64,
    pub escrow_reuse_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AuditArtifact {
    pub version: String,
    pub result: ResultReference,
    pub suite: SuiteReference,
    pub summary: AuditSummary,
    pub metrics: AuditMetrics,
    #[serde(default)]
    pub gate_failures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContentAddressedArtifact {
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PromotionRegistrySummary {
    pub id: String,
    pub version_before: String,
    pub version_after: String,
    pub source: String,
    pub lookup_snapshot_hash_before: String,
    pub escrow_snapshot_hash_before: String,
    pub lookup_snapshot_hash_after: String,
    pub escrow_snapshot_hash_after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PromotionWrites {
    #[serde(default)]
    pub mapping_files: Vec<String>,
    pub new_entity_entries: u64,
    pub existing_alias_entries: u64,
    pub pending_cluster_entries: u64,
    pub cannot_link_entries: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PromoteArtifact {
    pub version: String,
    pub result: ContentAddressedArtifact,
    pub audit: ContentAddressedArtifact,
    pub registry: PromotionRegistrySummary,
    pub decision: PromotionDecision,
    pub writes: PromotionWrites,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_state_serializes_as_expected() {
        let state =
            serde_json::to_string(&OrgEntityState::ResolvedExisting).expect("state to serialize");
        assert_eq!(state, "\"RESOLVED_EXISTING\"");
    }

    #[test]
    fn required_side_field_serializes_as_expected() {
        let field =
            serde_json::to_string(&OrgSideField::MentionSurfacesJson).expect("field to serialize");
        assert_eq!(field, "\"mention_surfaces_json\"");
    }

    #[test]
    fn strategy_reference_uses_runtime_content_hash() {
        let strategy = OrgStrategy {
            id: "bdc_org_graph.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:abc".to_string(),
            ..OrgStrategy::default()
        };

        assert_eq!(
            strategy.reference(),
            StrategyReference {
                id: "bdc_org_graph.v1".to_string(),
                version: "0.1.0".to_string(),
                content_hash: "blake3:abc".to_string(),
            }
        );
    }
}
