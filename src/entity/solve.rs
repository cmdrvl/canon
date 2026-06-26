//! Solve-stage budget contracts.
//!
//! Full signed-graph solving is implemented by later ENT-P07 beads. This module
//! owns the stage-local budget decision for oversized components so callers
//! either get an explicit bounded abstention or a structured refusal.

use crate::Refusal;
use crate::entity::{
    budget::{BudgetEnforcement, BudgetLimit, BudgetStage, find_budget_policy},
    error::EntityRefusalKind,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

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
