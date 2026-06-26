#![forbid(unsafe_code)]

use canon::entity::runtime::{
    explain::explain_from_artifact_value,
    types::{EntityState, EvidenceKind, ExplainQuery, PromotionDecision},
};
use predicates::prelude::*;
use serde_json::Value;
use std::{fs, path::Path};

const EXPLAIN_BUNDLE: &str = "operator_journey/explain/reconstruction_bundle.json";

#[test]
fn explain_acceptance_complete_reconstructs_g15_sections_and_next_action() {
    let artifact = explain_from_artifact_value(
        ExplainQuery {
            surface_id: Some("surf:cmbs:sears".to_string()),
            ..ExplainQuery::default()
        },
        fixture_value(EXPLAIN_BUNDLE),
    )
    .expect("explain reconstructs acceptance fixture");

    assert_eq!(artifact.version, "canon_entity_explain.v0");
    assert_eq!(artifact.result.state, EntityState::ResolvedExisting);
    assert_eq!(artifact.result.canonical_id.as_deref(), Some("TNT-SEARS"));
    assert_eq!(
        artifact
            .result
            .registry_snapshot
            .as_ref()
            .map(|snapshot| (snapshot.id.as_str(), snapshot.version.as_str())),
        Some(("cmbs-tenants", "2026.06.25"))
    );
    assert_eq!(
        artifact.result.next_action.as_deref(),
        Some("replay exact apply against the promoted registry snapshot")
    );

    let sears_surface = artifact
        .result
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "surf:cmbs:sears")
        .expect("selected surface is included");
    assert_eq!(
        sears_surface
            .normalized_views
            .get("tenant_label")
            .map(String::as_str),
        Some("sears")
    );
    assert!(
        artifact
            .result
            .surfaces
            .iter()
            .any(|surface| surface.surface_id == "surf:cmbs:auto"),
        "candidate neighbor surface is included for anti-merge context"
    );

    assert_eq!(artifact.result.candidates.len(), 3);
    assert!(artifact.result.candidates.iter().any(|candidate| {
        candidate.left_row_id == "row-sears-001" && candidate.right_row_id == "row-auto-001"
    }));
    assert!(artifact.result.positive_evidence.iter().any(|evidence| {
        evidence.kind == EvidenceKind::Support
            && evidence.namespace == "tenant_name"
            && evidence.operator_id == "exact_view:tenant_label"
    }));
    assert!(artifact.result.positive_evidence.iter().any(|evidence| {
        evidence.namespace == "relation_hint" && evidence.operator_id == "relation_hint:dba_alias"
    }));
    assert!(artifact.result.anti_merge_evidence.iter().any(|evidence| {
        evidence.kind == EvidenceKind::CannotLink
            && evidence.operator_id == "cannot_link:tenant_label_scope"
    }));

    assert_eq!(artifact.result.review_decisions.len(), 1);
    assert_eq!(
        artifact.result.review_decisions[0].review_id,
        "review-sears-alias"
    );
    assert_eq!(
        artifact.result.review_decisions[0].decision,
        "accept_aliases"
    );
    assert!(
        !artifact
            .result
            .review_decisions
            .iter()
            .any(|decision| decision.review_id == "review-unrelated"),
        "explain output does not leak unrelated review packets"
    );

    assert_eq!(artifact.result.promotion_provenance.len(), 1);
    let provenance = &artifact.result.promotion_provenance[0];
    assert_eq!(provenance.decision, PromotionDecision::Promote);
    assert_eq!(
        provenance.registry_version_after.as_deref(),
        Some("2026.06.26")
    );
    assert_eq!(provenance.writes.existing_alias_entries, 1);
}

#[test]
fn explain_acceptance_cli_summary_exposes_registry_and_next_action() {
    let fixture_path = fixture_path(EXPLAIN_BUNDLE);

    assert_cmd::cargo::cargo_bin_cmd!("canon")
        .args([
            "entity",
            "explain",
            fixture_path.to_str().expect("fixture path is utf-8"),
            "--surface-id",
            "surf:cmbs:sears",
            "--emit",
            "summary",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("surface surf:cmbs:sears"))
        .stdout(predicate::str::contains("registry=cmbs-tenants@2026.06.25"))
        .stdout(predicate::str::contains("support=2"))
        .stdout(predicate::str::contains("anti_merge=1"))
        .stdout(predicate::str::contains("review_decisions=1"))
        .stdout(predicate::str::contains("promotions=1"))
        .stdout(predicate::str::contains(
            "next_action=replay exact apply against the promoted registry snapshot",
        ));
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
