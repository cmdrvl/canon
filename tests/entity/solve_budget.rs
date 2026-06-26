use canon::RefusalCode;
use canon::entity::budget::{BudgetEnforcement, BudgetLimit, BudgetStage, find_budget_policy};
use canon::entity::solve::{
    SolveBudgetAction, SolveBudgetConfig, SolveComponentBudgetInput, SolveOversizedComponentPolicy,
    evaluate_solve_budget, evaluate_solve_component_budget,
};

#[test]
fn solve_budget_abstention() {
    let report = evaluate_solve_budget(
        &[
            SolveComponentBudgetInput::new(
                "component-b",
                vec![
                    "surf-4".to_string(),
                    "surf-2".to_string(),
                    "surf-3".to_string(),
                    "surf-1".to_string(),
                ],
            ),
            SolveComponentBudgetInput::new(
                "component-a",
                vec!["surf-7".to_string(), "surf-6".to_string()],
            ),
        ],
        SolveBudgetConfig::bounded_abstention(3),
    )
    .expect("bounded abstention report");

    assert_eq!(report.policy_id, "solve.max_component_size");
    assert_eq!(report.enforcement, BudgetEnforcement::BoundedAbstention);
    assert_eq!(report.summary["component_count"], 2);
    assert_eq!(report.summary["solved_component_count"], 1);
    assert_eq!(report.summary["abstained_component_count"], 1);
    assert_eq!(report.summary["surface_count"], 6);
    assert_eq!(report.summary["abstained_surface_count"], 4);
    assert_eq!(report.summary["largest_component_size"], 4);

    assert_eq!(report.components[0].component_id, "component-a");
    assert_eq!(report.components[0].action, SolveBudgetAction::Solve);
    assert_eq!(report.components[1].component_id, "component-b");
    assert_eq!(report.components[1].action, SolveBudgetAction::Abstain);
    assert_eq!(report.components[1].observed, 4);
    assert_eq!(report.components[1].configured, 3);
    assert_eq!(
        report.components[1].surface_ids,
        ["surf-1", "surf-2", "surf-3", "surf-4"]
    );
    assert_eq!(report.components[1].reason, "component_size_exceeds_budget");
}

#[test]
fn solver_large_component_refusal() {
    let refusal = evaluate_solve_component_budget(
        SolveComponentBudgetInput::new(
            "component-large",
            vec![
                "surf-z".to_string(),
                "surf-a".to_string(),
                "surf-m".to_string(),
            ],
        ),
        SolveBudgetConfig::refuse(2),
    )
    .expect_err("oversized component refuses when configured");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "solve");
    assert_eq!(refusal.detail["policy_id"], "solve.max_component_size");
    assert_eq!(refusal.detail["component_id"], "component-large");
    assert_eq!(refusal.detail["observed"], 3);
    assert_eq!(refusal.detail["configured"], 2);
    assert_eq!(
        refusal.detail["surface_ids"],
        serde_json::json!(["surf-a", "surf-m", "surf-z"])
    );
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|command| command.contains("larger solve component cap"))
    );
}

#[test]
fn solve_budget_uses_shared_policy_table() {
    let policy = find_budget_policy(BudgetStage::Solve, BudgetLimit::MaxComponentSize)
        .expect("solve policy exists");
    assert_eq!(policy.id, "solve.max_component_size");
    assert_eq!(policy.enforcement, BudgetEnforcement::BoundedAbstention);

    let config = SolveBudgetConfig {
        max_component_size: 2,
        oversized_component_policy: SolveOversizedComponentPolicy::BoundedAbstention,
    };
    let decision = evaluate_solve_component_budget(
        SolveComponentBudgetInput::new("component-ok", vec!["surf-2".into(), "surf-1".into()]),
        config,
    )
    .expect("within-budget component solves");
    assert_eq!(decision.action, SolveBudgetAction::Solve);
    assert_eq!(decision.policy_id, policy.id);
    assert_eq!(decision.reason, "within_component_budget");
    assert_eq!(decision.surface_ids, ["surf-1", "surf-2"]);
}
