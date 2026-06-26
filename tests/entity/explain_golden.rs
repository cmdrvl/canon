#![forbid(unsafe_code)]

use canon::entity::runtime::{
    explain::explain_from_artifact_value,
    types::{
        ExplainArtifact, ExplainCandidateRecord, ExplainEvidenceRecord,
        ExplainPromotionProvenanceRecord, ExplainQuery,
    },
};
use serde_json::{Value, json};
use std::{fs, path::Path};

const EXPLAIN_BUNDLE: &str = "operator_journey/explain/reconstruction_bundle.json";
const EXPECTED_PROJECTION: &str =
    include_str!("../fixtures/entity/ergonomics/explain_golden_projection.json");

#[test]
fn entity_explain_golden_reconstructs_rows_surfaces_and_canonical_ids() {
    let bundle = fixture_value(EXPLAIN_BUNDLE);
    let surface = explain_from_artifact_value(
        ExplainQuery {
            surface_id: Some("surf:cmbs:sears".to_string()),
            ..ExplainQuery::default()
        },
        bundle.clone(),
    )
    .expect("surface explain reconstructs");
    let row = explain_from_artifact_value(
        ExplainQuery {
            row_id: Some("row-sears-dba".to_string()),
            ..ExplainQuery::default()
        },
        bundle.clone(),
    )
    .expect("row explain reconstructs");
    let canon = explain_from_artifact_value(
        ExplainQuery {
            canonical_id: Some("TNT-SEARS".to_string()),
            ..ExplainQuery::default()
        },
        bundle,
    )
    .expect("canonical explain reconstructs");

    let projection = json!({
        "version": "canon.entity.explain_golden_projection.v0",
        "selectors": [
            explain_projection("surface_id", "surf:cmbs:sears", &surface),
            explain_projection("row_id", "row-sears-dba", &row),
            explain_projection("canonical_id", "TNT-SEARS", &canon),
        ]
    });
    assert_eq!(projection, expected_projection());
}

fn explain_projection(
    selector_kind: &str,
    selector_value: &str,
    artifact: &ExplainArtifact,
) -> Value {
    let registry = artifact
        .result
        .registry_snapshot
        .as_ref()
        .expect("registry snapshot");
    json!({
        "selector": {
            "kind": selector_kind,
            "value": selector_value
        },
        "artifact_version": artifact.version,
        "state": serde_json::to_value(artifact.result.state).expect("state json"),
        "canonical_id": artifact.result.canonical_id,
        "registry": format!("{}@{}", registry.id, registry.version),
        "next_action": artifact.result.next_action,
        "rows": {
            "backbone": artifact.result.backbone_rows,
            "attached": artifact.result.attached_rows
        },
        "surface_ids": artifact
            .result
            .surfaces
            .iter()
            .map(|surface| surface.surface_id.clone())
            .collect::<Vec<_>>(),
        "candidate_pairs": artifact
            .result
            .candidates
            .iter()
            .map(candidate_projection)
            .collect::<Vec<_>>(),
        "support": artifact
            .result
            .positive_evidence
            .iter()
            .map(evidence_projection)
            .collect::<Vec<_>>(),
        "anti_merge": artifact
            .result
            .anti_merge_evidence
            .iter()
            .map(evidence_projection)
            .collect::<Vec<_>>(),
        "review_ids": artifact
            .result
            .review_decisions
            .iter()
            .map(|decision| decision.review_id.clone())
            .collect::<Vec<_>>(),
        "promotion": artifact
            .result
            .promotion_provenance
            .iter()
            .map(promotion_projection)
            .collect::<Vec<_>>()
    })
}

fn candidate_projection(candidate: &ExplainCandidateRecord) -> Value {
    json!({
        "pair": [candidate.left_row_id, candidate.right_row_id],
        "score": candidate.pair_score_total,
        "operators": candidate.operator_ids
    })
}

fn evidence_projection(evidence: &ExplainEvidenceRecord) -> Value {
    json!({
        "kind": serde_json::to_value(evidence.kind).expect("evidence kind json"),
        "namespace": evidence.namespace,
        "operator_id": evidence.operator_id,
        "score": evidence.score,
        "pair": [evidence.left_row_id, evidence.right_row_id]
    })
}

fn promotion_projection(promotion: &ExplainPromotionProvenanceRecord) -> Value {
    json!({
        "decision": serde_json::to_value(promotion.decision).expect("promotion decision json"),
        "canonical_id": promotion.canonical_id,
        "registry_version_after": promotion.registry_version_after,
        "writes": promotion.writes
    })
}

fn expected_projection() -> Value {
    serde_json::from_str(EXPECTED_PROJECTION).expect("expected projection parses")
}

fn fixture_value(relative_path: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(fixture_path(relative_path)).expect("fixture opens"))
        .expect("fixture parses")
}

fn fixture_path(relative_path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity")
        .join(relative_path)
}
