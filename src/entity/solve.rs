//! Solve-stage budget contracts.
//!
//! Full signed-graph solving is implemented by later ENT-P07 beads. This module
//! owns the stage-local budget decision for oversized components so callers
//! either get an explicit bounded abstention or a structured refusal.

use crate::Refusal;
use crate::entity::{
    budget::{BudgetEnforcement, BudgetLimit, BudgetStage, find_budget_policy},
    error::EntityRefusalKind,
    graph::{CannotLinkEvidenceEdge, EntityEvidenceGraph, SignedEvidenceEdge},
    score::ScoreUnits,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveOversizedComponentPolicy {
    BoundedAbstention,
    Refuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveBudgetConfig {
    pub max_component_size: u64,
    pub oversized_component_policy: SolveOversizedComponentPolicy,
}

impl SolveBudgetConfig {
    pub const fn bounded_abstention(max_component_size: u64) -> Self {
        Self {
            max_component_size,
            oversized_component_policy: SolveOversizedComponentPolicy::BoundedAbstention,
        }
    }

    pub const fn refuse(max_component_size: u64) -> Self {
        Self {
            max_component_size,
            oversized_component_policy: SolveOversizedComponentPolicy::Refuse,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveComponentBudgetInput {
    pub component_id: String,
    pub surface_ids: Vec<String>,
}

impl SolveComponentBudgetInput {
    pub fn new(component_id: impl Into<String>, surface_ids: Vec<String>) -> Self {
        Self {
            component_id: component_id.into(),
            surface_ids,
        }
    }

    pub fn observed_size(&self) -> u64 {
        u64::try_from(self.surface_ids.len()).expect("component size fits u64")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveBudgetAction {
    Solve,
    Abstain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveBudgetComponentDecision {
    pub component_id: String,
    pub action: SolveBudgetAction,
    pub policy_id: String,
    pub enforcement: BudgetEnforcement,
    pub observed: u64,
    pub configured: u64,
    pub surface_ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveBudgetReport {
    pub policy_id: String,
    pub enforcement: BudgetEnforcement,
    pub configured: u64,
    pub summary: BTreeMap<String, u64>,
    pub components: Vec<SolveBudgetComponentDecision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveComponentAction {
    AutoMergeCandidate,
    Review,
    Contradiction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveEvidenceCut {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: ScoreUnits,
    pub evidence_count: u64,
    pub evidence_reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveCannotLinkViolation {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: ScoreUnits,
    pub hard_cannot_link: bool,
    pub evidence_count: u64,
    pub evidence_reason_codes: Vec<String>,
    pub evidence_operator_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveComponentConstraintDecision {
    pub component_id: String,
    pub action: SolveComponentAction,
    pub reason: String,
    pub surface_ids: Vec<String>,
    pub support_edge_count: u64,
    pub hard_cannot_link_violations: Vec<SolveCannotLinkViolation>,
    pub soft_anti_merge_warnings: Vec<SolveCannotLinkViolation>,
    pub strongest_positive_cut: Option<SolveEvidenceCut>,
    pub strongest_negative_cut: Option<SolveEvidenceCut>,
    pub raw_support_score_units: ScoreUnits,
    pub adjusted_support_score_units: ScoreUnits,
    pub review_priority_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveComponentConstraintReport {
    pub summary: BTreeMap<String, u64>,
    pub components: Vec<SolveComponentConstraintDecision>,
}

pub fn evaluate_solve_budget(
    components: &[SolveComponentBudgetInput],
    config: SolveBudgetConfig,
) -> Result<SolveBudgetReport, Refusal> {
    let policy = find_budget_policy(BudgetStage::Solve, BudgetLimit::MaxComponentSize)
        .ok_or_else(missing_solve_budget_policy_refusal)?;
    let mut ordered_components = components.to_vec();
    ordered_components.sort_by(|left, right| left.component_id.cmp(&right.component_id));

    let mut decisions = Vec::with_capacity(ordered_components.len());
    for component in &ordered_components {
        let observed = component.observed_size();
        let mut surface_ids = component.surface_ids.clone();
        surface_ids.sort();

        if observed <= config.max_component_size {
            decisions.push(SolveBudgetComponentDecision {
                component_id: component.component_id.clone(),
                action: SolveBudgetAction::Solve,
                policy_id: policy.id.to_string(),
                enforcement: policy.enforcement,
                observed,
                configured: config.max_component_size,
                surface_ids,
                reason: "within_component_budget".to_string(),
            });
            continue;
        }

        match config.oversized_component_policy {
            SolveOversizedComponentPolicy::BoundedAbstention => {
                decisions.push(SolveBudgetComponentDecision {
                    component_id: component.component_id.clone(),
                    action: SolveBudgetAction::Abstain,
                    policy_id: policy.id.to_string(),
                    enforcement: BudgetEnforcement::BoundedAbstention,
                    observed,
                    configured: config.max_component_size,
                    surface_ids,
                    reason: "component_size_exceeds_budget".to_string(),
                });
            }
            SolveOversizedComponentPolicy::Refuse => {
                return Err(solve_component_budget_refusal(
                    &component.component_id,
                    &surface_ids,
                    observed,
                    config.max_component_size,
                    policy.id,
                    policy.next_command,
                ));
            }
        }
    }

    Ok(SolveBudgetReport {
        policy_id: policy.id.to_string(),
        enforcement: policy.enforcement,
        configured: config.max_component_size,
        summary: solve_budget_summary(&decisions),
        components: decisions,
    })
}

pub fn evaluate_solve_component_budget(
    component: SolveComponentBudgetInput,
    config: SolveBudgetConfig,
) -> Result<SolveBudgetComponentDecision, Refusal> {
    let report = evaluate_solve_budget(&[component], config)?;
    Ok(report
        .components
        .into_iter()
        .next()
        .expect("single component report has one decision"))
}

pub fn evaluate_signed_graph_components(
    graph: &EntityEvidenceGraph,
) -> SolveComponentConstraintReport {
    let components = positive_components(graph);
    let mut decisions = components
        .into_iter()
        .map(|component| evaluate_component_constraints(graph, component))
        .collect::<Vec<_>>();
    decisions.sort_by(component_constraint_decision_cmp);

    SolveComponentConstraintReport {
        summary: component_constraint_summary(&decisions),
        components: decisions,
    }
}

fn evaluate_component_constraints(
    graph: &EntityEvidenceGraph,
    component: PositiveSolveComponent,
) -> SolveComponentConstraintDecision {
    let surface_set = component
        .surface_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let support_edges = graph
        .support_edges
        .iter()
        .filter(|edge| edge_inside_component(edge, &surface_set))
        .cloned()
        .collect::<Vec<_>>();
    let cannot_link_edges = graph
        .cannot_link_edges
        .iter()
        .filter(|edge| cannot_link_inside_component(edge, &surface_set))
        .cloned()
        .collect::<Vec<_>>();

    let mut hard_cannot_link_violations = cannot_link_edges
        .iter()
        .filter(|edge| edge.hard_cannot_link)
        .map(cannot_link_violation)
        .collect::<Vec<_>>();
    hard_cannot_link_violations.sort_by(cannot_link_violation_cmp);

    let mut soft_anti_merge_warnings = cannot_link_edges
        .iter()
        .filter(|edge| !edge.hard_cannot_link)
        .map(cannot_link_violation)
        .collect::<Vec<_>>();
    soft_anti_merge_warnings.sort_by(cannot_link_violation_cmp);

    let strongest_positive_cut = support_edges
        .iter()
        .max_by(|left, right| signed_edge_strength_cmp(left, right))
        .map(signed_evidence_cut);
    let strongest_negative_cut = cannot_link_edges
        .iter()
        .max_by(|left, right| cannot_link_strength_cmp(left, right))
        .map(cannot_link_evidence_cut);
    let raw_support_score_units = strongest_positive_cut
        .as_ref()
        .map(|cut| cut.score_units)
        .unwrap_or(ScoreUnits::ZERO);
    let soft_penalty = ScoreUnits::saturating_from_units(
        soft_anti_merge_warnings
            .iter()
            .map(|warning| u64::from(warning.score_units.as_u32()))
            .sum(),
    );
    let adjusted_support_score_units = subtract_score_units(raw_support_score_units, soft_penalty);

    let mut review_priority_reasons = Vec::new();
    let (action, reason) = if !hard_cannot_link_violations.is_empty() {
        review_priority_reasons.push("hard_cannot_link".to_string());
        (
            SolveComponentAction::Contradiction,
            "hard_cannot_link_inside_positive_component",
        )
    } else if !soft_anti_merge_warnings.is_empty() {
        review_priority_reasons.push("soft_anti_merge".to_string());
        (
            SolveComponentAction::Review,
            "soft_anti_merge_inside_positive_component",
        )
    } else {
        (
            SolveComponentAction::AutoMergeCandidate,
            "positive_component_without_cannot_link",
        )
    };

    SolveComponentConstraintDecision {
        component_id: component.component_id,
        action,
        reason: reason.to_string(),
        surface_ids: component.surface_ids,
        support_edge_count: support_edges.len() as u64,
        hard_cannot_link_violations,
        soft_anti_merge_warnings,
        strongest_positive_cut,
        strongest_negative_cut,
        raw_support_score_units,
        adjusted_support_score_units,
        review_priority_reasons,
    }
}

fn positive_components(graph: &EntityEvidenceGraph) -> Vec<PositiveSolveComponent> {
    let mut union_find = SurfaceUnionFind::default();
    for node in &graph.surface_nodes {
        union_find.insert(node.surface_id.clone());
    }
    for edge in &graph.support_edges {
        union_find.union(&edge.left_surface_id, &edge.right_surface_id);
    }
    for hyperedge in &graph.exact_bucket_hyperedges {
        let mut explicit_surface_ids = hyperedge
            .explicit_surface_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        explicit_surface_ids.sort();
        union_all(&mut union_find, &explicit_surface_ids);

        for range in &hyperedge.surface_ranges {
            union_find.union(&range.start_surface_id, &range.end_surface_id);
        }
    }

    union_find.into_components()
}

fn union_all(union_find: &mut SurfaceUnionFind, surface_ids: &[String]) {
    if let Some(first) = surface_ids.first() {
        for surface_id in surface_ids.iter().skip(1) {
            union_find.union(first, surface_id);
        }
    }
}

fn edge_inside_component(edge: &SignedEvidenceEdge, surface_ids: &BTreeSet<String>) -> bool {
    surface_ids.contains(&edge.left_surface_id) && surface_ids.contains(&edge.right_surface_id)
}

fn cannot_link_inside_component(
    edge: &CannotLinkEvidenceEdge,
    surface_ids: &BTreeSet<String>,
) -> bool {
    surface_ids.contains(&edge.left_surface_id) && surface_ids.contains(&edge.right_surface_id)
}

fn signed_evidence_cut(edge: &SignedEvidenceEdge) -> SolveEvidenceCut {
    SolveEvidenceCut {
        left_surface_id: edge.left_surface_id.clone(),
        right_surface_id: edge.right_surface_id.clone(),
        score_units: edge.score_units,
        evidence_count: edge.evidence.len() as u64,
        evidence_reason_codes: evidence_reason_codes(
            edge.evidence.iter().map(|hit| hit.reason_code.as_str()),
        ),
    }
}

fn cannot_link_evidence_cut(edge: &CannotLinkEvidenceEdge) -> SolveEvidenceCut {
    SolveEvidenceCut {
        left_surface_id: edge.left_surface_id.clone(),
        right_surface_id: edge.right_surface_id.clone(),
        score_units: edge.score_units,
        evidence_count: edge.evidence.len() as u64,
        evidence_reason_codes: evidence_reason_codes(
            edge.evidence.iter().map(|hit| hit.reason_code.as_str()),
        ),
    }
}

fn cannot_link_violation(edge: &CannotLinkEvidenceEdge) -> SolveCannotLinkViolation {
    SolveCannotLinkViolation {
        left_surface_id: edge.left_surface_id.clone(),
        right_surface_id: edge.right_surface_id.clone(),
        score_units: edge.score_units,
        hard_cannot_link: edge.hard_cannot_link,
        evidence_count: edge.evidence.len() as u64,
        evidence_reason_codes: evidence_reason_codes(
            edge.evidence.iter().map(|hit| hit.reason_code.as_str()),
        ),
        evidence_operator_ids: evidence_operator_ids(
            edge.evidence.iter().map(|hit| hit.operator_id.as_str()),
        ),
    }
}

fn evidence_reason_codes<'a>(codes: impl Iterator<Item = &'a str>) -> Vec<String> {
    dedup_sorted_strings(codes)
}

fn evidence_operator_ids<'a>(operator_ids: impl Iterator<Item = &'a str>) -> Vec<String> {
    dedup_sorted_strings(operator_ids)
}

fn dedup_sorted_strings<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut values = values.map(str::to_string).collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn subtract_score_units(left: ScoreUnits, right: ScoreUnits) -> ScoreUnits {
    ScoreUnits::from_scaled(left.as_u32().saturating_sub(right.as_u32()))
        .expect("subtracting bounded score units remains inside score scale")
}

fn component_constraint_summary(
    decisions: &[SolveComponentConstraintDecision],
) -> BTreeMap<String, u64> {
    let component_count = decisions.len() as u64;
    let auto_merge_candidate_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveComponentAction::AutoMergeCandidate)
        .count() as u64;
    let review_component_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveComponentAction::Review)
        .count() as u64;
    let contradiction_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveComponentAction::Contradiction)
        .count() as u64;
    let hard_cannot_link_count = decisions
        .iter()
        .map(|decision| decision.hard_cannot_link_violations.len() as u64)
        .sum();
    let soft_anti_merge_warning_count = decisions
        .iter()
        .map(|decision| decision.soft_anti_merge_warnings.len() as u64)
        .sum();

    BTreeMap::from([
        ("component_count".to_string(), component_count),
        (
            "auto_merge_candidate_count".to_string(),
            auto_merge_candidate_count,
        ),
        ("review_component_count".to_string(), review_component_count),
        ("contradiction_count".to_string(), contradiction_count),
        ("hard_cannot_link_count".to_string(), hard_cannot_link_count),
        (
            "soft_anti_merge_warning_count".to_string(),
            soft_anti_merge_warning_count,
        ),
    ])
}

fn component_constraint_decision_cmp(
    left: &SolveComponentConstraintDecision,
    right: &SolveComponentConstraintDecision,
) -> std::cmp::Ordering {
    left.component_id
        .cmp(&right.component_id)
        .then_with(|| left.surface_ids.cmp(&right.surface_ids))
}

fn signed_edge_strength_cmp(
    left: &SignedEvidenceEdge,
    right: &SignedEvidenceEdge,
) -> std::cmp::Ordering {
    left.score_units
        .cmp(&right.score_units)
        .then_with(|| right.left_surface_id.cmp(&left.left_surface_id))
        .then_with(|| right.right_surface_id.cmp(&left.right_surface_id))
}

fn cannot_link_strength_cmp(
    left: &CannotLinkEvidenceEdge,
    right: &CannotLinkEvidenceEdge,
) -> std::cmp::Ordering {
    left.hard_cannot_link
        .cmp(&right.hard_cannot_link)
        .then_with(|| left.score_units.cmp(&right.score_units))
        .then_with(|| right.left_surface_id.cmp(&left.left_surface_id))
        .then_with(|| right.right_surface_id.cmp(&left.right_surface_id))
}

fn cannot_link_violation_cmp(
    left: &SolveCannotLinkViolation,
    right: &SolveCannotLinkViolation,
) -> std::cmp::Ordering {
    left.left_surface_id
        .cmp(&right.left_surface_id)
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
        .then_with(|| right.hard_cannot_link.cmp(&left.hard_cannot_link))
        .then_with(|| right.score_units.cmp(&left.score_units))
}

#[derive(Debug, Clone)]
struct PositiveSolveComponent {
    component_id: String,
    surface_ids: Vec<String>,
}

#[derive(Debug, Default)]
struct SurfaceUnionFind {
    parent: BTreeMap<String, String>,
}

impl SurfaceUnionFind {
    fn insert(&mut self, surface_id: String) {
        self.parent.entry(surface_id.clone()).or_insert(surface_id);
    }

    fn union(&mut self, left_surface_id: &str, right_surface_id: &str) {
        self.insert(left_surface_id.to_string());
        self.insert(right_surface_id.to_string());

        let left_root = self.find(left_surface_id);
        let right_root = self.find(right_surface_id);
        if left_root == right_root {
            return;
        }

        if left_root < right_root {
            self.parent.insert(right_root, left_root);
        } else {
            self.parent.insert(left_root, right_root);
        }
    }

    fn find(&mut self, surface_id: &str) -> String {
        let mut current = self
            .parent
            .get(surface_id)
            .cloned()
            .unwrap_or_else(|| surface_id.to_string());
        loop {
            let parent = self
                .parent
                .get(&current)
                .cloned()
                .unwrap_or_else(|| current.clone());
            if parent == current {
                break;
            }
            current = parent;
        }

        let root = current;
        let mut current = surface_id.to_string();
        while let Some(parent) = self.parent.get(&current).cloned() {
            if parent == root {
                break;
            }
            self.parent.insert(current.clone(), root.clone());
            current = parent;
        }
        root
    }

    fn into_components(mut self) -> Vec<PositiveSolveComponent> {
        let surface_ids = self.parent.keys().cloned().collect::<Vec<_>>();
        let mut by_root = BTreeMap::<String, Vec<String>>::new();
        for surface_id in surface_ids {
            let root = self.find(&surface_id);
            by_root.entry(root).or_default().push(surface_id);
        }

        by_root
            .into_iter()
            .map(|(root, mut surface_ids)| {
                surface_ids.sort();
                PositiveSolveComponent {
                    component_id: format!("component:{root}"),
                    surface_ids,
                }
            })
            .collect()
    }
}

fn solve_budget_summary(decisions: &[SolveBudgetComponentDecision]) -> BTreeMap<String, u64> {
    let component_count = u64::try_from(decisions.len()).expect("component count fits u64");
    let solved_component_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveBudgetAction::Solve)
        .count();
    let abstained_component_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveBudgetAction::Abstain)
        .count();
    let surface_count = decisions
        .iter()
        .map(|decision| decision.observed)
        .sum::<u64>();
    let abstained_surface_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveBudgetAction::Abstain)
        .map(|decision| decision.observed)
        .sum::<u64>();
    let largest_component_size = decisions
        .iter()
        .map(|decision| decision.observed)
        .max()
        .unwrap_or_default();

    BTreeMap::from([
        ("component_count".to_string(), component_count),
        (
            "solved_component_count".to_string(),
            u64::try_from(solved_component_count).expect("solved component count fits u64"),
        ),
        (
            "abstained_component_count".to_string(),
            u64::try_from(abstained_component_count).expect("abstained component count fits u64"),
        ),
        ("surface_count".to_string(), surface_count),
        (
            "abstained_surface_count".to_string(),
            abstained_surface_count,
        ),
        ("largest_component_size".to_string(), largest_component_size),
    ])
}

fn solve_component_budget_refusal(
    component_id: &str,
    surface_ids: &[String],
    observed: u64,
    configured: u64,
    policy_id: &str,
    next_command: &str,
) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Solve component budget exceeded before safe solving",
        json!({
            "stage": "solve",
            "policy_id": policy_id,
            "component_id": component_id,
            "observed": observed,
            "configured": configured,
            "surface_ids": surface_ids,
            "recovery": "Keep the oversized component in escrow or explicitly configure a larger solve component cap"
        }),
        Some(next_command.to_string()),
    )
}

fn missing_solve_budget_policy_refusal() -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Solve component budget policy is missing",
        json!({
            "stage": "solve",
            "policy_id": "solve.max_component_size",
            "recovery": "Restore the shared entity budget policy table before running solve"
        }),
        Some("canon entity solve <ROWS> --strategy <STRATEGY.yaml> --edges <EDGES.jsonl> --registry <REGISTRY_DIR>".to_string()),
    )
}
