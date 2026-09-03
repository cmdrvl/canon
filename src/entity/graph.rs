#![forbid(unsafe_code)]

//! Compact signed evidence graph helpers for entity solving.
//!
//! Exact-bucket assertions enter solve as hyperedges. The graph stores the
//! bucket membership directly and checks hard cannot-link facts against that
//! membership without expanding the bucket into pairwise edges.

use crate::{
    Refusal,
    entity::{
        block_artifact::{ExactBucketAssertion, SurfaceIdRange},
        contracts::{CANON_ENTITY_EDGE_VERSION, CANON_ENTITY_SOLVE_VERSION},
        edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
        error::EntityRefusalKind,
        score::{ScoreLane, ScoreUnits},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, VecDeque},
};

pub const CANON_ENTITY_CLUSTER_SHAPE_VERSION: &str = "canon_entity_cluster_shape.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityEvidenceGraph {
    pub version: String,
    pub surface_nodes: Vec<SurfaceGraphNode>,
    pub exact_bucket_hyperedges: Vec<ExactBucketHyperedge>,
    pub support_edges: Vec<SignedEvidenceEdge>,
    pub cannot_link_edges: Vec<CannotLinkEvidenceEdge>,
    pub relation_hint_edges: Vec<SignedEvidenceEdge>,
    pub hard_cannot_links: BTreeSet<SurfacePair>,
    pub component_diagnostics_inputs: Vec<ComponentDiagnosticsInput>,
    pub diagnostics: EvidenceGraphDiagnostics,
}

impl EntityEvidenceGraph {
    pub fn from_exact_bucket_assertions(assertions: &[ExactBucketAssertion]) -> Self {
        let mut exact_bucket_hyperedges = assertions
            .iter()
            .map(ExactBucketHyperedge::from_assertion)
            .collect::<Vec<_>>();
        exact_bucket_hyperedges.sort_by(|left, right| left.bucket_id.cmp(&right.bucket_id));

        let mut surface_ids = BTreeSet::new();
        for hyperedge in &exact_bucket_hyperedges {
            surface_ids.extend(hyperedge.explicit_surface_ids.iter().cloned());
            for range in &hyperedge.surface_ranges {
                surface_ids.insert(range.start_surface_id.clone());
                surface_ids.insert(range.end_surface_id.clone());
            }
        }
        let surface_nodes = surface_nodes_from_ids(surface_ids, &BTreeMap::new());
        let component_diagnostics_inputs =
            component_diagnostics_inputs_from_hyperedges(&exact_bucket_hyperedges);
        let diagnostics = evidence_graph_diagnostics(
            surface_nodes.len(),
            &exact_bucket_hyperedges,
            &[],
            &[],
            &[],
        );

        Self {
            version: CANON_ENTITY_SOLVE_VERSION.to_string(),
            surface_nodes,
            exact_bucket_hyperedges,
            support_edges: vec![],
            cannot_link_edges: vec![],
            relation_hint_edges: vec![],
            hard_cannot_links: BTreeSet::new(),
            component_diagnostics_inputs,
            diagnostics,
        }
    }

    pub fn from_signed_evidence(
        edge_records: &[EdgeEvidenceRecord],
        exact_bucket_assertions: &[ExactBucketAssertion],
    ) -> Result<Self, Refusal> {
        build_signed_evidence_graph(SignedEvidenceGraphInput {
            edge_records: edge_records.to_vec(),
            exact_bucket_assertions: exact_bucket_assertions.to_vec(),
            incumbent_ids: vec![],
        })
    }

    pub fn add_hard_cannot_link(
        &mut self,
        left_surface_id: impl Into<String>,
        right_surface_id: impl Into<String>,
    ) {
        if let Some(pair) = SurfacePair::new(left_surface_id, right_surface_id) {
            self.hard_cannot_links.insert(pair);
        }
    }

    pub fn solve_exact_bucket_hyperedges(&self) -> ExactBucketSolveReport {
        let mut decisions = self
            .exact_bucket_hyperedges
            .iter()
            .map(|hyperedge| self.solve_hyperedge(hyperedge))
            .collect::<Vec<_>>();
        decisions.sort_by(|left, right| left.bucket_id.cmp(&right.bucket_id));

        ExactBucketSolveReport {
            hyperedge_count: decisions.len() as u64,
            expanded_pair_count: 0,
            membership_record_count: self
                .exact_bucket_hyperedges
                .iter()
                .map(|hyperedge| hyperedge.membership_record_count)
                .sum(),
            theoretical_pair_count: self
                .exact_bucket_hyperedges
                .iter()
                .map(|hyperedge| hyperedge.theoretical_pair_count)
                .sum(),
            decisions,
        }
    }

    fn solve_hyperedge(&self, hyperedge: &ExactBucketHyperedge) -> ExactBucketSolveDecision {
        let hard_cannot_links = self
            .hard_cannot_links
            .iter()
            .filter(|pair| hyperedge.contains_pair(pair))
            .cloned()
            .collect::<Vec<_>>();
        let hard_cannot_link_count = hard_cannot_links.len() as u64;
        let (action, reason) = if hard_cannot_link_count == 0 {
            (
                ExactBucketSolveAction::MergeCluster,
                "exact_bucket_cluster_evidence",
            )
        } else {
            (
                ExactBucketSolveAction::ReviewContradiction,
                "hard_cannot_link_inside_exact_bucket",
            )
        };

        ExactBucketSolveDecision {
            bucket_id: hyperedge.bucket_id.clone(),
            action,
            reason: reason.to_string(),
            member_count: hyperedge.member_count,
            membership_record_count: hyperedge.membership_record_count,
            expanded_pair_count: 0,
            hard_cannot_link_count,
            hard_cannot_links,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SignedEvidenceGraphInput {
    #[serde(default)]
    pub edge_records: Vec<EdgeEvidenceRecord>,
    #[serde(default)]
    pub exact_bucket_assertions: Vec<ExactBucketAssertion>,
    #[serde(default)]
    pub incumbent_ids: Vec<SurfaceIncumbentId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceGraphNode {
    pub surface_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incumbent_canonical_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceIncumbentId {
    pub surface_id: String,
    pub canonical_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedEvidenceEdge {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: ScoreUnits,
    pub evidence: Vec<EdgeEvidenceHit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CannotLinkEvidenceEdge {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: ScoreUnits,
    pub hard_cannot_link: bool,
    pub evidence: Vec<EdgeEvidenceHit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentDiagnosticsInput {
    pub component_id: String,
    pub source: String,
    pub member_count: u64,
    pub membership_record_count: u64,
    pub explicit_surface_ids: Vec<String>,
    pub surface_ranges: Vec<SurfaceIdRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceGraphDiagnostics {
    pub surface_node_count: u64,
    pub support_edge_count: u64,
    pub cannot_link_edge_count: u64,
    pub hard_cannot_link_edge_count: u64,
    pub soft_cannot_link_edge_count: u64,
    pub relation_hint_edge_count: u64,
    pub exact_bucket_hyperedge_count: u64,
    pub exact_bucket_member_count: u64,
    pub exact_bucket_membership_record_count: u64,
    pub materialized_exact_bucket_pair_count: u64,
    pub theoretical_exact_bucket_pair_count: u64,
}

pub fn build_signed_evidence_graph(
    input: SignedEvidenceGraphInput,
) -> Result<EntityEvidenceGraph, Refusal> {
    let incumbent_ids = validate_incumbent_ids(&input.incumbent_ids)?;

    let mut exact_bucket_hyperedges = Vec::with_capacity(input.exact_bucket_assertions.len());
    for assertion in &input.exact_bucket_assertions {
        assertion.validate().map_err(|error| {
            graph_artifact_contract_refusal(
                "Exact bucket assertion is invalid for signed graph input",
                json!({
                    "stage": "solve",
                    "reason": "invalid_exact_bucket_assertion",
                    "bucket_id": assertion.bucket_id,
                    "error": format!("{error:?}")
                }),
            )
        })?;
        exact_bucket_hyperedges.push(ExactBucketHyperedge::from_assertion(assertion));
    }
    exact_bucket_hyperedges.sort_by(|left, right| left.bucket_id.cmp(&right.bucket_id));

    let mut records = input.edge_records;
    for record in &records {
        validate_edge_record(record)?;
    }
    records.sort_by(edge_record_graph_cmp);

    let mut surface_ids = BTreeSet::new();
    for incumbent in &input.incumbent_ids {
        surface_ids.insert(incumbent.surface_id.clone());
    }
    for hyperedge in &exact_bucket_hyperedges {
        surface_ids.extend(hyperedge.explicit_surface_ids.iter().cloned());
        for range in &hyperedge.surface_ranges {
            surface_ids.insert(range.start_surface_id.clone());
            surface_ids.insert(range.end_surface_id.clone());
        }
    }

    let mut support_edges = Vec::new();
    let mut cannot_link_edges = Vec::new();
    let mut relation_hint_edges = Vec::new();
    let mut hard_cannot_links = BTreeSet::new();

    for record in &records {
        surface_ids.insert(record.left_surface_id.clone());
        surface_ids.insert(record.right_surface_id.clone());

        let support_hits = lane_hits(record, ScoreLane::Support);
        if !support_hits.is_empty() {
            support_edges.push(SignedEvidenceEdge {
                left_surface_id: record.left_surface_id.clone(),
                right_surface_id: record.right_surface_id.clone(),
                score_units: record.pair_score_total,
                evidence: support_hits,
            });
        }

        let cannot_link_hits = lane_hits(record, ScoreLane::AntiMerge);
        if !cannot_link_hits.is_empty() {
            let hard_cannot_link = cannot_link_hits.iter().any(|hit| hit.hard_cannot_link);
            if hard_cannot_link {
                let pair = SurfacePair::new(&record.left_surface_id, &record.right_surface_id)
                    .expect("validated edge record carries a deterministic non-empty surface pair");
                hard_cannot_links.insert(pair);
            }
            cannot_link_edges.push(CannotLinkEvidenceEdge {
                left_surface_id: record.left_surface_id.clone(),
                right_surface_id: record.right_surface_id.clone(),
                score_units: sum_hit_score_units(&cannot_link_hits),
                hard_cannot_link,
                evidence: cannot_link_hits,
            });
        }

        let relation_hint_hits = lane_hits(record, ScoreLane::RelationHint);
        if !relation_hint_hits.is_empty() {
            relation_hint_edges.push(SignedEvidenceEdge {
                left_surface_id: record.left_surface_id.clone(),
                right_surface_id: record.right_surface_id.clone(),
                score_units: sum_hit_score_units(&relation_hint_hits),
                evidence: relation_hint_hits,
            });
        }
    }

    support_edges.sort_by(signed_edge_cmp);
    cannot_link_edges.sort_by(cannot_link_edge_cmp);
    relation_hint_edges.sort_by(signed_edge_cmp);

    let surface_nodes = surface_nodes_from_ids(surface_ids, &incumbent_ids);
    let component_diagnostics_inputs =
        component_diagnostics_inputs_from_hyperedges(&exact_bucket_hyperedges);
    let diagnostics = evidence_graph_diagnostics(
        surface_nodes.len(),
        &exact_bucket_hyperedges,
        &support_edges,
        &cannot_link_edges,
        &relation_hint_edges,
    );

    Ok(EntityEvidenceGraph {
        version: CANON_ENTITY_SOLVE_VERSION.to_string(),
        surface_nodes,
        exact_bucket_hyperedges,
        support_edges,
        cannot_link_edges,
        relation_hint_edges,
        hard_cannot_links,
        component_diagnostics_inputs,
        diagnostics,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketHyperedge {
    pub bucket_id: String,
    pub operator_id: String,
    pub member_count: u64,
    pub membership_record_count: u64,
    pub theoretical_pair_count: u64,
    pub expanded_pair_count: u64,
    pub explicit_surface_ids: BTreeSet<String>,
    pub surface_ranges: Vec<SurfaceIdRange>,
}

impl ExactBucketHyperedge {
    fn from_assertion(assertion: &ExactBucketAssertion) -> Self {
        Self {
            bucket_id: assertion.bucket_id.clone(),
            operator_id: assertion.operator_id.clone(),
            member_count: assertion.membership.member_count(),
            membership_record_count: assertion.artifact_membership_record_count(),
            theoretical_pair_count: assertion.theoretical_pair_count(),
            expanded_pair_count: assertion.expanded_pair_count(),
            explicit_surface_ids: assertion
                .membership
                .surface_ids
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            surface_ranges: assertion.membership.surface_ranges.clone(),
        }
    }

    fn contains_pair(&self, pair: &SurfacePair) -> bool {
        self.contains_surface_id(&pair.left_surface_id)
            && self.contains_surface_id(&pair.right_surface_id)
    }

    fn contains_surface_id(&self, surface_id: &str) -> bool {
        self.explicit_surface_ids.contains(surface_id)
            || self.surface_ranges.iter().any(|range| {
                range.start_surface_id.as_str() <= surface_id
                    && surface_id <= range.end_surface_id.as_str()
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfacePair {
    pub left_surface_id: String,
    pub right_surface_id: String,
}

impl SurfacePair {
    pub fn new(
        left_surface_id: impl Into<String>,
        right_surface_id: impl Into<String>,
    ) -> Option<Self> {
        let left_surface_id = left_surface_id.into();
        let right_surface_id = right_surface_id.into();
        if left_surface_id == right_surface_id {
            return None;
        }
        if left_surface_id < right_surface_id {
            Some(Self {
                left_surface_id,
                right_surface_id,
            })
        } else {
            Some(Self {
                left_surface_id: right_surface_id,
                right_surface_id: left_surface_id,
            })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExactBucketSolveAction {
    MergeCluster,
    ReviewContradiction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketSolveDecision {
    pub bucket_id: String,
    pub action: ExactBucketSolveAction,
    pub reason: String,
    pub member_count: u64,
    pub membership_record_count: u64,
    pub expanded_pair_count: u64,
    pub hard_cannot_link_count: u64,
    pub hard_cannot_links: Vec<SurfacePair>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketSolveReport {
    pub hyperedge_count: u64,
    pub expanded_pair_count: u64,
    pub membership_record_count: u64,
    pub theoretical_pair_count: u64,
    pub decisions: Vec<ExactBucketSolveDecision>,
}

fn validate_incumbent_ids(
    incumbent_ids: &[SurfaceIncumbentId],
) -> Result<BTreeMap<String, String>, Refusal> {
    let mut by_surface = BTreeMap::new();
    for incumbent in incumbent_ids {
        if incumbent.surface_id.trim().is_empty() || incumbent.canonical_id.trim().is_empty() {
            return Err(graph_artifact_contract_refusal(
                "Signed graph incumbent IDs must be non-empty",
                json!({
                    "stage": "solve",
                    "reason": "invalid_incumbent_id",
                    "surface_id": incumbent.surface_id,
                    "canonical_id": incumbent.canonical_id
                }),
            ));
        }
        if by_surface
            .insert(incumbent.surface_id.clone(), incumbent.canonical_id.clone())
            .is_some()
        {
            return Err(graph_artifact_contract_refusal(
                "Signed graph incumbent IDs must be unique by surface",
                json!({
                    "stage": "solve",
                    "reason": "duplicate_incumbent_surface",
                    "surface_id": incumbent.surface_id
                }),
            ));
        }
    }
    Ok(by_surface)
}

fn validate_edge_record(record: &EdgeEvidenceRecord) -> Result<(), Refusal> {
    if record.version != CANON_ENTITY_EDGE_VERSION {
        return Err(graph_artifact_contract_refusal(
            "Edge evidence record has the wrong contract version for signed graph input",
            json!({
                "stage": "solve",
                "reason": "wrong_edge_version",
                "expected": CANON_ENTITY_EDGE_VERSION,
                "actual": record.version
            }),
        ));
    }

    let canonical = build_edge_evidence_record(
        record.left_surface_id.clone(),
        record.right_surface_id.clone(),
        record.hits.clone(),
    )?;
    if canonical != *record {
        return Err(graph_artifact_contract_refusal(
            "Edge evidence record is not canonical for signed graph input",
            json!({
                "stage": "solve",
                "reason": "noncanonical_edge_record",
                "left_surface_id": record.left_surface_id,
                "right_surface_id": record.right_surface_id
            }),
        ));
    }
    Ok(())
}

fn lane_hits(record: &EdgeEvidenceRecord, lane: ScoreLane) -> Vec<EdgeEvidenceHit> {
    record
        .hits
        .iter()
        .filter(|hit| hit.lane == lane)
        .cloned()
        .collect()
}

fn sum_hit_score_units(hits: &[EdgeEvidenceHit]) -> ScoreUnits {
    ScoreUnits::saturating_from_units(
        hits.iter()
            .map(|hit| u64::from(hit.score_units.as_u32()))
            .sum(),
    )
}

fn surface_nodes_from_ids(
    surface_ids: BTreeSet<String>,
    incumbent_ids: &BTreeMap<String, String>,
) -> Vec<SurfaceGraphNode> {
    surface_ids
        .into_iter()
        .map(|surface_id| SurfaceGraphNode {
            incumbent_canonical_id: incumbent_ids.get(&surface_id).cloned(),
            surface_id,
        })
        .collect()
}

fn component_diagnostics_inputs_from_hyperedges(
    hyperedges: &[ExactBucketHyperedge],
) -> Vec<ComponentDiagnosticsInput> {
    hyperedges
        .iter()
        .map(|hyperedge| ComponentDiagnosticsInput {
            component_id: hyperedge.bucket_id.clone(),
            source: "exact_bucket_hyperedge".to_string(),
            member_count: hyperedge.member_count,
            membership_record_count: hyperedge.membership_record_count,
            explicit_surface_ids: hyperedge.explicit_surface_ids.iter().cloned().collect(),
            surface_ranges: hyperedge.surface_ranges.clone(),
        })
        .collect()
}

fn evidence_graph_diagnostics(
    surface_node_count: usize,
    exact_bucket_hyperedges: &[ExactBucketHyperedge],
    support_edges: &[SignedEvidenceEdge],
    cannot_link_edges: &[CannotLinkEvidenceEdge],
    relation_hint_edges: &[SignedEvidenceEdge],
) -> EvidenceGraphDiagnostics {
    EvidenceGraphDiagnostics {
        surface_node_count: surface_node_count as u64,
        support_edge_count: support_edges.len() as u64,
        cannot_link_edge_count: cannot_link_edges.len() as u64,
        hard_cannot_link_edge_count: cannot_link_edges
            .iter()
            .filter(|edge| edge.hard_cannot_link)
            .count() as u64,
        soft_cannot_link_edge_count: cannot_link_edges
            .iter()
            .filter(|edge| !edge.hard_cannot_link)
            .count() as u64,
        relation_hint_edge_count: relation_hint_edges.len() as u64,
        exact_bucket_hyperedge_count: exact_bucket_hyperedges.len() as u64,
        exact_bucket_member_count: exact_bucket_hyperedges
            .iter()
            .map(|hyperedge| hyperedge.member_count)
            .sum(),
        exact_bucket_membership_record_count: exact_bucket_hyperedges
            .iter()
            .map(|hyperedge| hyperedge.membership_record_count)
            .sum(),
        materialized_exact_bucket_pair_count: exact_bucket_hyperedges
            .iter()
            .map(|hyperedge| hyperedge.expanded_pair_count)
            .sum(),
        theoretical_exact_bucket_pair_count: exact_bucket_hyperedges
            .iter()
            .map(|hyperedge| hyperedge.theoretical_pair_count)
            .sum(),
    }
}

fn edge_record_graph_cmp(
    left: &EdgeEvidenceRecord,
    right: &EdgeEvidenceRecord,
) -> std::cmp::Ordering {
    left.left_surface_id
        .cmp(&right.left_surface_id)
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
}

fn signed_edge_cmp(left: &SignedEvidenceEdge, right: &SignedEvidenceEdge) -> std::cmp::Ordering {
    left.left_surface_id
        .cmp(&right.left_surface_id)
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
        .then_with(|| right.score_units.cmp(&left.score_units))
}

fn cannot_link_edge_cmp(
    left: &CannotLinkEvidenceEdge,
    right: &CannotLinkEvidenceEdge,
) -> std::cmp::Ordering {
    left.left_surface_id
        .cmp(&right.left_surface_id)
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
        .then_with(|| right.hard_cannot_link.cmp(&left.hard_cannot_link))
        .then_with(|| right.score_units.cmp(&left.score_units))
}

fn graph_artifact_contract_refusal(message: &'static str, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some(
            "canon entity solve <ROWS> --evidence <EVIDENCE_ARTIFACT.json> --registry <REGISTRY_DIR>"
                .to_string(),
        ),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityClusterShapeInput {
    pub clusters: Vec<EntityClusterShapeClusterInput>,
    pub scored_edges: Vec<EntityClusterShapeEdgeInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityClusterShapeClusterInput {
    pub cluster_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    pub surface_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityClusterShapeEdgeInput {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityClusterShapeReport {
    pub version: String,
    pub summary: EntityClusterShapeSummary,
    pub ranking_policy: EntityClusterShapeRankingPolicy,
    pub clusters: Vec<EntityClusterShapeMetrics>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityClusterShapeSummary {
    pub cluster_count: u64,
    pub surface_count: u64,
    pub scored_edge_count: u64,
    pub bridge_edge_count: u64,
    pub max_diameter: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityClusterShapeRankingPolicy {
    pub order: Vec<String>,
    pub tie_break: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityClusterShapeMetrics {
    pub suspicion_rank: u64,
    pub cluster_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    pub size: u64,
    pub possible_edge_count: u64,
    pub scored_edge_count: u64,
    pub edge_density_basis_points: u32,
    pub bridge_edge_count: u64,
    pub bridge_edges: Vec<EntityClusterShapeBridgeEdge>,
    pub diameter: u64,
    pub disconnected_pair_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_internal_edge_score_units: Option<ScoreUnits>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityClusterShapeBridgeEdge {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: ScoreUnits,
}

impl EntityClusterShapeInput {
    pub fn from_graph(
        clusters: Vec<EntityClusterShapeClusterInput>,
        graph: &EntityEvidenceGraph,
    ) -> Self {
        let scored_edges = graph
            .support_edges
            .iter()
            .map(|edge| EntityClusterShapeEdgeInput {
                left_surface_id: edge.left_surface_id.clone(),
                right_surface_id: edge.right_surface_id.clone(),
                score_units: edge.score_units,
            })
            .collect();
        Self {
            clusters,
            scored_edges,
        }
    }
}

pub fn build_entity_cluster_shape_report(
    input: EntityClusterShapeInput,
) -> EntityClusterShapeReport {
    let mut clusters = input
        .clusters
        .into_iter()
        .map(|cluster| cluster_shape_metrics(&cluster, &input.scored_edges))
        .collect::<Vec<_>>();
    clusters.sort_by(cluster_shape_suspicion_cmp);
    for (index, cluster) in clusters.iter_mut().enumerate() {
        cluster.suspicion_rank = u64::try_from(index + 1).expect("cluster rank fits u64");
    }

    let summary = EntityClusterShapeSummary {
        cluster_count: clusters.len() as u64,
        surface_count: clusters.iter().map(|cluster| cluster.size).sum(),
        scored_edge_count: clusters
            .iter()
            .map(|cluster| cluster.scored_edge_count)
            .sum(),
        bridge_edge_count: clusters
            .iter()
            .map(|cluster| cluster.bridge_edge_count)
            .sum(),
        max_diameter: clusters
            .iter()
            .map(|cluster| cluster.diameter)
            .max()
            .unwrap_or(0),
    };

    EntityClusterShapeReport {
        version: CANON_ENTITY_CLUSTER_SHAPE_VERSION.to_string(),
        summary,
        ranking_policy: EntityClusterShapeRankingPolicy {
            order: vec![
                "edge_density_basis_points_asc".to_string(),
                "bridge_edge_count_desc".to_string(),
                "diameter_desc".to_string(),
                "min_internal_edge_score_units_asc".to_string(),
                "size_desc".to_string(),
            ],
            tie_break: vec!["canonical_id_asc".to_string(), "cluster_id_asc".to_string()],
        },
        clusters,
    }
}

pub fn cluster_shape_suspicion_cmp(
    left: &EntityClusterShapeMetrics,
    right: &EntityClusterShapeMetrics,
) -> Ordering {
    left.edge_density_basis_points
        .cmp(&right.edge_density_basis_points)
        .then_with(|| right.bridge_edge_count.cmp(&left.bridge_edge_count))
        .then_with(|| right.diameter.cmp(&left.diameter))
        .then_with(|| {
            min_score_sort_key(left.min_internal_edge_score_units)
                .cmp(&min_score_sort_key(right.min_internal_edge_score_units))
        })
        .then_with(|| right.size.cmp(&left.size))
        .then_with(|| cluster_shape_tie_break_id(left).cmp(cluster_shape_tie_break_id(right)))
        .then_with(|| left.cluster_id.cmp(&right.cluster_id))
}

fn cluster_shape_metrics(
    cluster: &EntityClusterShapeClusterInput,
    scored_edges: &[EntityClusterShapeEdgeInput],
) -> EntityClusterShapeMetrics {
    let surface_ids = sorted_unique_non_empty(cluster.surface_ids.clone());
    let surface_set = surface_ids.iter().cloned().collect::<BTreeSet<_>>();
    let internal_edges = internal_scored_edges(&surface_set, scored_edges);
    let possible_edge_count = possible_pair_count(surface_ids.len() as u64);
    let scored_edge_count = internal_edges.len() as u64;
    let adjacency = cluster_adjacency(&surface_ids, &internal_edges);
    let bridge_edges = bridge_edges(&surface_ids, &internal_edges, &adjacency);
    let (diameter, disconnected_pair_count) =
        diameter_and_disconnected_pairs(&surface_ids, &adjacency);
    let min_internal_edge_score_units = internal_edges.values().map(|edge| edge.score_units).min();

    EntityClusterShapeMetrics {
        suspicion_rank: 0,
        cluster_id: cluster.cluster_id.clone(),
        canonical_id: cluster.canonical_id.clone(),
        size: surface_ids.len() as u64,
        possible_edge_count,
        scored_edge_count,
        edge_density_basis_points: density_basis_points(scored_edge_count, possible_edge_count),
        bridge_edge_count: bridge_edges.len() as u64,
        bridge_edges,
        diameter,
        disconnected_pair_count,
        min_internal_edge_score_units,
    }
}

fn sorted_unique_non_empty(mut surface_ids: Vec<String>) -> Vec<String> {
    surface_ids.retain(|surface_id| !surface_id.trim().is_empty());
    surface_ids.sort();
    surface_ids.dedup();
    surface_ids
}

fn internal_scored_edges(
    surface_set: &BTreeSet<String>,
    scored_edges: &[EntityClusterShapeEdgeInput],
) -> BTreeMap<SurfacePair, EntityClusterShapeBridgeEdge> {
    let mut internal_edges: BTreeMap<SurfacePair, EntityClusterShapeBridgeEdge> = BTreeMap::new();
    for edge in scored_edges {
        let Some(pair) = SurfacePair::new(&edge.left_surface_id, &edge.right_surface_id) else {
            continue;
        };
        if !surface_set.contains(&pair.left_surface_id)
            || !surface_set.contains(&pair.right_surface_id)
        {
            continue;
        }
        let candidate = EntityClusterShapeBridgeEdge {
            left_surface_id: pair.left_surface_id.clone(),
            right_surface_id: pair.right_surface_id.clone(),
            score_units: edge.score_units,
        };
        match internal_edges.entry(pair) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let current = entry.get_mut();
                if candidate.score_units > current.score_units {
                    *current = candidate;
                }
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(candidate);
            }
        }
    }
    internal_edges
}

fn cluster_adjacency(
    surface_ids: &[String],
    internal_edges: &BTreeMap<SurfacePair, EntityClusterShapeBridgeEdge>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut adjacency = surface_ids
        .iter()
        .map(|surface_id| (surface_id.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for pair in internal_edges.keys() {
        adjacency
            .entry(pair.left_surface_id.clone())
            .or_default()
            .insert(pair.right_surface_id.clone());
        adjacency
            .entry(pair.right_surface_id.clone())
            .or_default()
            .insert(pair.left_surface_id.clone());
    }
    adjacency
}

fn bridge_edges(
    surface_ids: &[String],
    internal_edges: &BTreeMap<SurfacePair, EntityClusterShapeBridgeEdge>,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<EntityClusterShapeBridgeEdge> {
    if surface_ids.len() <= 2 {
        return Vec::new();
    }
    internal_edges
        .iter()
        .filter(|(pair, _)| {
            !reachable_without_edge(
                &pair.left_surface_id,
                &pair.right_surface_id,
                pair,
                adjacency,
            )
        })
        .map(|(_, edge)| edge.clone())
        .collect()
}

fn reachable_without_edge(
    start: &str,
    target: &str,
    removed: &SurfacePair,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([start.to_string()]);
    while let Some(surface_id) = queue.pop_front() {
        if surface_id == target {
            return true;
        }
        if !seen.insert(surface_id.clone()) {
            continue;
        }
        for next in adjacency.get(&surface_id).into_iter().flatten() {
            if edge_matches(&surface_id, next, removed) {
                continue;
            }
            if !seen.contains(next) {
                queue.push_back(next.clone());
            }
        }
    }
    false
}

fn edge_matches(left: &str, right: &str, pair: &SurfacePair) -> bool {
    (left == pair.left_surface_id && right == pair.right_surface_id)
        || (left == pair.right_surface_id && right == pair.left_surface_id)
}

fn diameter_and_disconnected_pairs(
    surface_ids: &[String],
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> (u64, u64) {
    let mut diameter = 0_u64;
    let mut disconnected_pair_count = 0_u64;
    for (left_index, left) in surface_ids.iter().enumerate() {
        let distances = shortest_path_distances(left, adjacency);
        for right in surface_ids.iter().skip(left_index + 1) {
            if let Some(distance) = distances.get(right) {
                diameter = diameter.max(*distance);
            } else {
                disconnected_pair_count += 1;
            }
        }
    }
    (diameter, disconnected_pair_count)
}

fn shortest_path_distances(
    start: &str,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, u64> {
    let mut distances = BTreeMap::from([(start.to_string(), 0)]);
    let mut queue = VecDeque::from([start.to_string()]);
    while let Some(surface_id) = queue.pop_front() {
        let next_distance = distances[&surface_id] + 1;
        for next in adjacency.get(&surface_id).into_iter().flatten() {
            if distances.contains_key(next) {
                continue;
            }
            distances.insert(next.clone(), next_distance);
            queue.push_back(next.clone());
        }
    }
    distances
}

const fn possible_pair_count(size: u64) -> u64 {
    size.saturating_mul(size.saturating_sub(1)) / 2
}

fn density_basis_points(scored_edge_count: u64, possible_edge_count: u64) -> u32 {
    if possible_edge_count == 0 {
        return 0;
    }
    let scaled = (u128::from(scored_edge_count) * 10_000) / u128::from(possible_edge_count);
    u32::try_from(scaled).expect("density basis points fit u32")
}

fn min_score_sort_key(score_units: Option<ScoreUnits>) -> u32 {
    score_units.map(ScoreUnits::as_u32).unwrap_or(u32::MAX)
}

fn cluster_shape_tie_break_id(cluster: &EntityClusterShapeMetrics) -> &str {
    cluster
        .canonical_id
        .as_deref()
        .unwrap_or(cluster.cluster_id.as_str())
}
