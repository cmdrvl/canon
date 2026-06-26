//! Output/emitter helpers for `canon entity`.

use super::types::{
    AuditArtifact, BlockRecord, CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_EDGE_VERSION,
    CANON_ENTITY_RUN_VERSION, CANON_ENTITY_SOLVE_VERSION, EdgeRecord, ExplainArtifact,
    ExplainQuery, PromoteArtifact, PromotionDecision, SolveRunArtifact,
};
use serde::Serialize;
use std::error::Error;

pub fn emit_block_jsonl(records: &[BlockRecord]) -> Result<String, Box<dyn Error>> {
    emit_json_lines(records)
}

pub fn emit_edge_jsonl(records: &[EdgeRecord]) -> Result<String, Box<dyn Error>> {
    emit_json_lines(records)
}

pub fn emit_run_json(artifact: &SolveRunArtifact) -> Result<String, Box<dyn Error>> {
    emit_json(artifact)
}

pub fn emit_solve_json(artifact: &SolveRunArtifact) -> Result<String, Box<dyn Error>> {
    emit_json(artifact)
}

pub fn emit_audit_json(artifact: &AuditArtifact) -> Result<String, Box<dyn Error>> {
    emit_json(artifact)
}

pub fn emit_promote_json(artifact: &PromoteArtifact) -> Result<String, Box<dyn Error>> {
    emit_json(artifact)
}

pub fn emit_explain_json(artifact: &ExplainArtifact) -> Result<String, Box<dyn Error>> {
    emit_json(artifact)
}

pub fn render_block_summary(records: &[BlockRecord]) -> String {
    let pair_count = records.len();
    let block_hit_count = records
        .iter()
        .map(|record| record.block_hits.len())
        .sum::<usize>();

    match records.first() {
        Some(first) => format!(
            "{}@{} block {}@{} | {} pairs, {} block hits",
            first.strategy.id,
            first.strategy.version,
            first.registry_snapshot.id,
            first.registry_snapshot.version,
            pair_count,
            block_hit_count,
        ),
        None => format!(
            "{} | {} pairs, {} block hits",
            CANON_ENTITY_BLOCK_VERSION, pair_count, block_hit_count
        ),
    }
}

pub fn render_edge_summary(records: &[EdgeRecord]) -> String {
    let pair_count = records.len();
    let hit_count = records
        .iter()
        .map(|record| record.hits.len())
        .sum::<usize>();
    let must_link_pairs = records.iter().filter(|record| record.has_must_link).count();
    let cannot_link_pairs = records
        .iter()
        .filter(|record| record.has_cannot_link)
        .count();
    let total_score = records
        .iter()
        .map(|record| record.pair_score_total)
        .sum::<i64>();

    match records.first() {
        Some(first) => format!(
            "{}@{} edge {}@{} | {} pairs, {} hits, {} must-link, {} cannot-link, total score {}",
            first.strategy.id,
            first.strategy.version,
            first.registry_snapshot.id,
            first.registry_snapshot.version,
            pair_count,
            hit_count,
            must_link_pairs,
            cannot_link_pairs,
            total_score,
        ),
        None => format!(
            "{} | {} pairs, {} hits, {} must-link, {} cannot-link, total score {}",
            CANON_ENTITY_EDGE_VERSION,
            pair_count,
            hit_count,
            must_link_pairs,
            cannot_link_pairs,
            total_score,
        ),
    }
}

pub fn render_run_summary(artifact: &SolveRunArtifact) -> String {
    render_solve_run_summary(artifact)
}

pub fn render_solve_summary(artifact: &SolveRunArtifact) -> String {
    render_solve_run_summary(artifact)
}

pub fn render_audit_summary(artifact: &AuditArtifact) -> String {
    format!(
        "{} audit {} | {}, hard_gates={}, holdout_score={:.3}, anchor_conflicts={}, gate_failures={}",
        artifact.suite.id,
        artifact.result.version,
        promotion_decision_label(artifact.summary.decision),
        artifact.summary.hard_gates_passed,
        artifact.metrics.holdout_score,
        artifact.metrics.anchor_conflicts,
        artifact.gate_failures.len(),
    )
}

pub fn render_promote_summary(artifact: &PromoteArtifact) -> String {
    format!(
        "{}: {} -> {} | {}, {} new entities, {} alias writes, {} pending escrow, {} cannot-link",
        artifact.registry.id,
        artifact.registry.version_before,
        artifact.registry.version_after,
        promotion_decision_label(artifact.decision),
        artifact.writes.new_entity_entries,
        artifact.writes.existing_alias_entries,
        artifact.writes.pending_cluster_entries,
        artifact.writes.cannot_link_entries,
    )
}

pub fn render_explain_summary(artifact: &ExplainArtifact) -> String {
    format!(
        "{} -> {} | canonical_id={}, escrow_id={}, registry={}, backbone_rows={}, attached_rows={}, witnesses={}, surfaces={}, candidates={}, support={}, anti_merge={}, review_decisions={}, promotions={}, next_action={}",
        explain_query_label(&artifact.query),
        entity_state_label(artifact.result.state),
        option_label(artifact.result.canonical_id.as_deref()),
        option_label(artifact.result.escrow_id.as_deref()),
        registry_label(artifact.result.registry_snapshot.as_ref()),
        artifact.result.backbone_rows.len(),
        artifact.result.attached_rows.len(),
        artifact.result.witness_chain.len(),
        artifact.result.surfaces.len(),
        artifact.result.candidates.len(),
        artifact.result.positive_evidence.len(),
        artifact.result.anti_merge_evidence.len(),
        artifact.result.review_decisions.len(),
        artifact.result.promotion_provenance.len(),
        option_label(artifact.result.next_action.as_deref()),
    )
}

fn emit_json<T: Serialize>(artifact: &T) -> Result<String, Box<dyn Error>> {
    Ok(serde_json::to_string(artifact)?)
}

fn emit_json_lines<T: Serialize>(records: &[T]) -> Result<String, Box<dyn Error>> {
    let mut output = String::new();

    for (index, record) in records.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str(&serde_json::to_string(record)?);
    }

    Ok(output)
}

fn render_solve_run_summary(artifact: &SolveRunArtifact) -> String {
    format!(
        "{} {}@{} {}@{} | {} observations, {} resolved_existing, {} promotable_new, {} abstain_low_evidence, {} abstain_conflict, {} contradictions",
        solve_run_label(&artifact.version),
        artifact.strategy.id,
        artifact.strategy.version,
        artifact.registry.id,
        artifact.registry.version,
        artifact.summary.observations,
        artifact.summary.resolved_existing,
        artifact.summary.promotable_new,
        artifact.summary.abstain_low_evidence,
        artifact.summary.abstain_conflict,
        artifact.contradictions.len(),
    )
}

fn solve_run_label(version: &str) -> &str {
    match version {
        CANON_ENTITY_RUN_VERSION => "run",
        CANON_ENTITY_SOLVE_VERSION => "solve",
        _ => version,
    }
}

fn explain_query_label(query: &ExplainQuery) -> String {
    if let Some(row_id) = &query.row_id {
        format!("row {}", row_id)
    } else if let Some(surface_id) = &query.surface_id {
        format!("surface {}", surface_id)
    } else if let Some(canonical_id) = &query.canonical_id {
        format!("canon-id {}", canonical_id)
    } else if let Some(escrow_id) = &query.escrow_id {
        format!("escrow-id {}", escrow_id)
    } else {
        "query <unset>".to_string()
    }
}

fn option_label(value: Option<&str>) -> &str {
    value.unwrap_or("-")
}

fn registry_label(registry: Option<&super::types::RegistrySnapshot>) -> String {
    registry
        .map(|registry| format!("{}@{}", registry.id, registry.version))
        .unwrap_or_else(|| "-".to_string())
}

fn entity_state_label(state: super::types::EntityState) -> &'static str {
    match state {
        super::types::EntityState::ResolvedExisting => "RESOLVED_EXISTING",
        super::types::EntityState::PromotableNew => "PROMOTABLE_NEW",
        super::types::EntityState::AbstainLowEvidence => "ABSTAIN_LOW_EVIDENCE",
        super::types::EntityState::AbstainConflict => "ABSTAIN_CONFLICT",
        super::types::EntityState::Contradiction => "CONTRADICTION",
    }
}

fn promotion_decision_label(decision: PromotionDecision) -> &'static str {
    match decision {
        PromotionDecision::Promote => "PROMOTE",
        PromotionDecision::Reject => "REJECT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_runtime::types::{
        AnchorValue, ExplainResult, InheritanceMode, InheritanceRecord, MergeWitness,
        RegistryPatchSummary, RegistrySnapshot, SolveRunSummary, StrategyReference,
    };
    use std::collections::BTreeMap;

    #[test]
    fn emit_block_jsonl_is_deterministic() {
        let records = vec![
            BlockRecord {
                strategy: StrategyReference {
                    id: "bdc_org_graph.v1".to_string(),
                    version: "0.1.0".to_string(),
                    content_hash: "blake3:strategy".to_string(),
                },
                registry_snapshot: RegistrySnapshot {
                    id: "bdc-issuers".to_string(),
                    version: "2026.03.01".to_string(),
                    source: "registries/bdc-issuers".to_string(),
                    lookup_snapshot_hash: "blake3:lookup".to_string(),
                    escrow_snapshot_hash: "blake3:escrow".to_string(),
                },
                left_row_id: "row-1".to_string(),
                right_row_id: "row-9".to_string(),
                block_hits: vec![super::super::types::BlockHit {
                    operator_id: "exact_view:core_name".to_string(),
                }],
                ..BlockRecord::default()
            },
            BlockRecord {
                strategy: StrategyReference {
                    id: "bdc_org_graph.v1".to_string(),
                    version: "0.1.0".to_string(),
                    content_hash: "blake3:strategy".to_string(),
                },
                registry_snapshot: RegistrySnapshot {
                    id: "bdc-issuers".to_string(),
                    version: "2026.03.01".to_string(),
                    source: "registries/bdc-issuers".to_string(),
                    lookup_snapshot_hash: "blake3:lookup".to_string(),
                    escrow_snapshot_hash: "blake3:escrow".to_string(),
                },
                left_row_id: "row-2".to_string(),
                right_row_id: "row-7".to_string(),
                block_hits: vec![super::super::types::BlockHit {
                    operator_id: "shared_anchor:lei".to_string(),
                }],
                ..BlockRecord::default()
            },
        ];

        let first = emit_block_jsonl(&records).expect("first emit");
        let second = emit_block_jsonl(&records).expect("second emit");

        assert_eq!(first, second);
        assert_eq!(
            first,
            concat!(
                "{\"version\":\"canon_entity_block.v0\",\"strategy\":{\"id\":\"bdc_org_graph.v1\",\"version\":\"0.1.0\",\"content_hash\":\"blake3:strategy\"},\"registry_snapshot\":{\"id\":\"bdc-issuers\",\"version\":\"2026.03.01\",\"source\":\"registries/bdc-issuers\",\"lookup_snapshot_hash\":\"blake3:lookup\",\"escrow_snapshot_hash\":\"blake3:escrow\"},\"left_row_id\":\"row-1\",\"right_row_id\":\"row-9\",\"block_hits\":[{\"operator_id\":\"exact_view:core_name\"}]}\n",
                "{\"version\":\"canon_entity_block.v0\",\"strategy\":{\"id\":\"bdc_org_graph.v1\",\"version\":\"0.1.0\",\"content_hash\":\"blake3:strategy\"},\"registry_snapshot\":{\"id\":\"bdc-issuers\",\"version\":\"2026.03.01\",\"source\":\"registries/bdc-issuers\",\"lookup_snapshot_hash\":\"blake3:lookup\",\"escrow_snapshot_hash\":\"blake3:escrow\"},\"left_row_id\":\"row-2\",\"right_row_id\":\"row-7\",\"block_hits\":[{\"operator_id\":\"shared_anchor:lei\"}]}"
            )
        );
    }

    #[test]
    fn emit_run_json_is_deterministic() {
        let mut pair_score_by_namespace = BTreeMap::new();
        pair_score_by_namespace.insert("name".to_string(), 32);
        pair_score_by_namespace.insert("registry".to_string(), 0);

        let artifact = SolveRunArtifact {
            version: CANON_ENTITY_RUN_VERSION.to_string(),
            strategy: StrategyReference {
                id: "bdc_org_graph.v1".to_string(),
                version: "0.1.0".to_string(),
                content_hash: "blake3:strategy".to_string(),
            },
            registry: RegistrySnapshot {
                id: "bdc-issuers".to_string(),
                version: "2026.03.01".to_string(),
                source: "registries/bdc-issuers/".to_string(),
                lookup_snapshot_hash: "blake3:lookup".to_string(),
                escrow_snapshot_hash: "blake3:escrow".to_string(),
            },
            summary: SolveRunSummary {
                observations: 2,
                resolved_existing: 1,
                promotable_new: 1,
                abstain_low_evidence: 0,
                abstain_conflict: 0,
            },
            entities: vec![super::super::types::SolvedEntity {
                state: super::super::types::EntityState::ResolvedExisting,
                canonical_id: Some("IC-123abc456def".to_string()),
                backbone_rows: vec!["row-1".to_string(), "row-9".to_string()],
                attached_rows: vec![],
                all_rows: vec!["row-1".to_string(), "row-9".to_string()],
                aliases: vec!["ACME Corporation".to_string(), "Acme Corp.".to_string()],
                anchors: vec![AnchorValue {
                    namespace: "lei".to_string(),
                    value: "549300XYZ".to_string(),
                }],
                merge_witnesses: vec![MergeWitness {
                    left_row_id: "row-1".to_string(),
                    right_row_id: "row-9".to_string(),
                    pair_score_total: 32,
                    pair_score_by_namespace,
                    operator_ids: vec!["exact_view:core_name".to_string()],
                }],
                inheritance: InheritanceRecord {
                    mode: InheritanceMode::SingleIncumbentOverlap,
                    incumbent_ids: vec!["IC-123abc456def".to_string()],
                },
                eligible_writeback_aliases: vec!["ACME Corporation".to_string()],
                escrow: None,
            }],
            abstentions: vec![],
            contradictions: vec![],
            proposed_registry_patch: RegistryPatchSummary {
                mapping_files: vec!["org-20260322.json".to_string()],
                new_entity_entries: 1,
                existing_alias_entries: 1,
            },
            proposed_escrow_patch: super::super::types::EscrowPatchSummary {
                pending_cluster_entries: 0,
                cannot_link_entries: 0,
            },
        };

        let first = emit_run_json(&artifact).expect("first emit");
        let second = emit_run_json(&artifact).expect("second emit");

        assert_eq!(first, second);
        assert!(first.contains("\"version\":\"canon_entity_run.v0\""));
        assert!(first.contains("\"pair_score_by_namespace\":{\"name\":32,\"registry\":0}"));
    }

    #[test]
    fn render_summaries_include_key_counts() {
        let run_artifact = SolveRunArtifact {
            version: CANON_ENTITY_SOLVE_VERSION.to_string(),
            strategy: StrategyReference {
                id: "bdc_org_graph.v1".to_string(),
                version: "0.1.0".to_string(),
                content_hash: "blake3:strategy".to_string(),
            },
            registry: RegistrySnapshot {
                id: "bdc-issuers".to_string(),
                version: "2026.03.01".to_string(),
                source: "registries/bdc-issuers/".to_string(),
                lookup_snapshot_hash: "blake3:lookup".to_string(),
                escrow_snapshot_hash: "blake3:escrow".to_string(),
            },
            summary: SolveRunSummary {
                observations: 1200,
                resolved_existing: 830,
                promotable_new: 140,
                abstain_low_evidence: 180,
                abstain_conflict: 50,
            },
            contradictions: vec![super::super::types::ContradictionRecord {
                reason: "anchor_conflict".to_string(),
                row_ids: vec!["row-41".to_string(), "row-58".to_string()],
                left_key: Some("lei:123".to_string()),
                right_key: Some("lei:999".to_string()),
            }],
            ..SolveRunArtifact::default()
        };
        let explain_artifact = ExplainArtifact {
            version: super::super::types::CANON_ENTITY_EXPLAIN_VERSION.to_string(),
            query: ExplainQuery {
                row_id: Some("row-9".to_string()),
                surface_id: None,
                canonical_id: None,
                escrow_id: None,
            },
            result: ExplainResult {
                state: super::super::types::EntityState::ResolvedExisting,
                canonical_id: Some("IC-123abc456def".to_string()),
                escrow_id: None,
                backbone_rows: vec!["row-1".to_string(), "row-9".to_string()],
                attached_rows: vec![],
                inheritance: InheritanceRecord {
                    mode: InheritanceMode::SingleIncumbentOverlap,
                    incumbent_ids: vec!["IC-123abc456def".to_string()],
                },
                witness_chain: vec![MergeWitness {
                    left_row_id: "row-1".to_string(),
                    right_row_id: "row-9".to_string(),
                    pair_score_total: 32,
                    pair_score_by_namespace: BTreeMap::new(),
                    operator_ids: vec!["exact_view:core_name".to_string()],
                }],
                ..ExplainResult::default()
            },
        };
        let audit_artifact = AuditArtifact {
            version: super::super::types::CANON_ENTITY_AUDIT_VERSION.to_string(),
            suite: super::super::types::SuiteReference {
                id: "bdc_org_eval.v1".to_string(),
            },
            result: super::super::types::ResultReference {
                version: CANON_ENTITY_RUN_VERSION.to_string(),
                content_hash: "blake3:run".to_string(),
                strategy_content_hash: "blake3:strategy".to_string(),
                lookup_snapshot_hash: "blake3:lookup".to_string(),
                escrow_snapshot_hash: "blake3:escrow".to_string(),
            },
            summary: super::super::types::AuditSummary {
                decision: PromotionDecision::Promote,
                hard_gates_passed: true,
            },
            metrics: super::super::types::AuditMetrics {
                gold_pair_f1: Some(0.982),
                anchor_consistency: 1.0,
                anchor_conflicts: 0,
                holdout_score: 0.975,
                contradiction_rate: 0.0,
                perturbation_stability: 0.998,
                continuity_gain: 0.071,
                compression_gain: 0.412,
                registry_churn: 0.006,
                escrow_reuse_rate: 0.23,
            },
            gate_failures: vec![],
        };

        assert_eq!(
            render_solve_summary(&run_artifact),
            "solve bdc_org_graph.v1@0.1.0 bdc-issuers@2026.03.01 | 1200 observations, 830 resolved_existing, 140 promotable_new, 180 abstain_low_evidence, 50 abstain_conflict, 1 contradictions"
        );
        assert_eq!(
            render_explain_summary(&explain_artifact),
            "row row-9 -> RESOLVED_EXISTING | canonical_id=IC-123abc456def, escrow_id=-, registry=-, backbone_rows=2, attached_rows=0, witnesses=1, surfaces=0, candidates=0, support=0, anti_merge=0, review_decisions=0, promotions=0, next_action=-"
        );
        assert_eq!(
            render_audit_summary(&audit_artifact),
            "bdc_org_eval.v1 audit canon_entity_run.v0 | PROMOTE, hard_gates=true, holdout_score=0.975, anchor_conflicts=0, gate_failures=0"
        );
    }
}
