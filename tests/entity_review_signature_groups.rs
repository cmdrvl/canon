#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        EntityArtifactMetadata, EntityInputReference, EntityPatchNamespaces,
        EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
        review::{
            ReviewProvenanceSample, ReviewQueueArtifact, ReviewQueueItem, ReviewRelationHint,
        },
        review_export::{
            NativeReviewDecisionAction as ExportNativeReviewDecisionAction,
            NativeReviewExportRequest, NativeReviewItem, NativeReviewMode, NativeReviewModeContext,
            build_native_review_artifact, render_native_review_html,
        },
        review_import::{
            NativeReviewDecision, NativeReviewDecisionAction, NativeReviewDecisionContext,
            NativeReviewDecisionMode, NativeReviewGroupDecision,
            expand_native_review_group_decisions, import_native_review_decisions,
            native_review_import_context_from_artifact, parse_native_review_import_json,
            parse_native_review_import_json_with_source,
        },
        score::ScoreUnits,
        solve::{SolveEvidenceCut, SolveReconciliationState},
    },
};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn evidence_signature_groups_are_deterministic_under_review_item_order_shuffle() {
    let forward = native_signature_artifact(false);
    let reversed = native_signature_artifact(true);

    assert!(forward.review_groups.len() <= 10);
    assert_eq!(forward.review_groups.len(), 8);
    assert_eq!(
        forward.summary.counts["evidence_signature_groups"],
        forward.review_groups.len() as u64
    );
    assert_eq!(
        serde_json::to_vec(&forward).expect("forward artifact serializes"),
        serde_json::to_vec(&reversed).expect("reversed artifact serializes")
    );

    let total_members = forward
        .review_groups
        .iter()
        .map(|group| group.member_count)
        .sum::<u64>();
    assert_eq!(total_members, 500);
    for group in &forward.review_groups {
        assert_eq!(group.signature_id, group.signature.signature_id);
        assert_eq!(
            group.sample_review_ids,
            sorted_group_member_ids(&forward, &group.signature_id)
                .into_iter()
                .take(5)
                .collect::<Vec<_>>()
        );
        assert!(
            group.score_stats.min_review_priority_units
                <= group.score_stats.max_review_priority_units
        );
        assert!(
            group.score_stats.min_evidence_score_units
                <= group.score_stats.max_evidence_score_units
        );
    }

    let html = render_native_review_html(&forward).expect("grouped html renders");
    assert!(html.contains("Signature Groups"));
    assert!(html.contains("group_decisions"));
    assert!(html.contains(&forward.review_groups[0].signature_id));
    for forbidden in [
        "http://",
        "https://",
        "fetch(",
        "XMLHttpRequest",
        "WebSocket",
        "sendBeacon",
        "import(",
    ] {
        assert!(
            !html.contains(forbidden),
            "offline review HTML must not include network primitive {forbidden}"
        );
    }
}

#[test]
fn group_decision_import_expands_members_and_member_override_wins() {
    let artifact = native_signature_artifact(false);
    let artifact_value = serde_json::to_value(&artifact).expect("artifact value");
    let group = artifact
        .review_groups
        .iter()
        .find(|group| {
            group.signature.recommended_action == Some(ExportNativeReviewDecisionAction::Alias)
                && group.member_count > 5
        })
        .expect("alias group exists");
    let override_review_id = group.sample_review_ids[0].clone();
    let override_item = artifact
        .review_items
        .iter()
        .find(|item| item.review_id == override_review_id)
        .expect("override item exists");
    let group_decision = NativeReviewGroupDecision {
        evidence_signature_id: group.signature_id.clone(),
        action: NativeReviewDecisionAction::Alias,
        operator_id: "operator:signature-review".to_string(),
        reason_code: "same_signature_alias".to_string(),
        note: "one evidence signature reviewed once".to_string(),
        source_review_artifact_hash: artifact.artifact_content_hash.clone(),
        run_content_hash: artifact.binding.run_content_hash.clone(),
        policy_content_hash: artifact.binding.policy_content_hash.clone(),
        registry_snapshot_hash: artifact.binding.registry_snapshot_hash.clone(),
        target_canonical_id: None,
        relation: None,
    };
    let per_member_override = native_decision_from_item(
        &artifact,
        override_item,
        NativeReviewDecisionAction::Defer,
        "operator:signature-review",
        "defer_one_member",
    );

    let envelope = json!({
        "version": "canon_entity_native_review_decisions.v0",
        "group_decisions": [group_decision.clone()],
        "decisions": [per_member_override.clone()]
    });
    let refusal = parse_native_review_import_json(&envelope.to_string())
        .expect_err("group decisions need source artifact expansion");
    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["field"], "group_decisions");

    let expanded =
        parse_native_review_import_json_with_source(&envelope.to_string(), &artifact_value)
            .expect("group envelope expands with source artifact");
    assert_eq!(expanded.len(), group.member_count as usize);
    let actions_by_review_id = expanded
        .iter()
        .map(|decision| (decision.review_id.clone(), decision.action))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        actions_by_review_id[&override_review_id],
        NativeReviewDecisionAction::Defer
    );
    assert_eq!(
        actions_by_review_id
            .values()
            .filter(|action| **action == NativeReviewDecisionAction::Alias)
            .count(),
        group.member_count as usize - 1
    );

    let context = native_review_import_context_from_artifact(&artifact_value).expect("context");
    let receipt = import_native_review_decisions(context.clone(), expanded.clone())
        .expect("expanded group import accepts");
    assert_eq!(receipt.accepted_decisions, group.member_count);
    assert_eq!(receipt.patches.defer_patches.len(), 1);
    assert_eq!(
        receipt.patches.defer_patches[0].review_id,
        override_review_id
    );
    assert!(
        receipt
            .patches
            .alias_patches
            .iter()
            .all(|patch| patch.review_id != override_review_id)
    );
    let replay = import_native_review_decisions(context.clone(), expanded.clone())
        .expect("expanded group import replays");
    assert_eq!(
        serde_json::to_vec(&receipt).expect("receipt bytes"),
        serde_json::to_vec(&replay).expect("replay bytes")
    );

    let mut tampered = expanded;
    let victim = tampered
        .iter_mut()
        .find(|decision| decision.review_id != override_review_id)
        .expect("group-expanded member exists");
    victim.decision_binding_hash = "blake3:wrong-member-binding".to_string();
    let refusal = import_native_review_decisions(context, tampered)
        .expect_err("one tampered expanded member refuses the batch");
    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["field"], "decision_binding_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn group_decision_rejects_unknown_signature_without_expanding_any_members() {
    let artifact = native_signature_artifact(false);
    let artifact_value = serde_json::to_value(&artifact).expect("artifact value");
    let refusal = expand_native_review_group_decisions(
        &artifact_value,
        vec![NativeReviewGroupDecision {
            evidence_signature_id: "signature:blake3:not-the-exported-signature".to_string(),
            action: NativeReviewDecisionAction::Alias,
            operator_id: "operator:signature-review".to_string(),
            reason_code: "wrong_group".to_string(),
            note: String::new(),
            source_review_artifact_hash: artifact.artifact_content_hash.clone(),
            run_content_hash: artifact.binding.run_content_hash.clone(),
            policy_content_hash: artifact.binding.policy_content_hash.clone(),
            registry_snapshot_hash: artifact.binding.registry_snapshot_hash.clone(),
            target_canonical_id: None,
            relation: None,
        }],
        Vec::new(),
    )
    .expect_err("wrong signature refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["field"], "evidence_signature_id");
    assert_eq!(refusal.detail["writes_performed"], false);
}

fn native_signature_artifact(
    reverse_items: bool,
) -> canon::entity::review_export::NativeReviewArtifact {
    build_native_review_artifact(NativeReviewExportRequest {
        review_queue: synthetic_review_queue(reverse_items),
        run_content_hash: "blake3:run-signature-fixture".to_string(),
        policy_content_hash: "blake3:policy-signature-fixture".to_string(),
    })
    .expect("native grouped artifact builds")
}

fn sorted_group_member_ids(
    artifact: &canon::entity::review_export::NativeReviewArtifact,
    signature_id: &str,
) -> Vec<String> {
    artifact
        .review_items
        .iter()
        .filter(|item| item.evidence_signature.signature_id == signature_id)
        .map(|item| item.review_id.clone())
        .collect()
}

fn native_decision_from_item(
    artifact: &canon::entity::review_export::NativeReviewArtifact,
    item: &NativeReviewItem,
    action: NativeReviewDecisionAction,
    operator_id: &str,
    reason_code: &str,
) -> NativeReviewDecision {
    NativeReviewDecision {
        review_id: item.review_id.clone(),
        mode: import_mode(item.mode),
        action,
        operator_id: operator_id.to_string(),
        reason_code: reason_code.to_string(),
        note: "explicit member override".to_string(),
        source_review_artifact_hash: artifact.artifact_content_hash.clone(),
        decision_binding_hash: item.decision_binding_hash.clone(),
        run_content_hash: artifact.binding.run_content_hash.clone(),
        policy_content_hash: artifact.binding.policy_content_hash.clone(),
        registry_snapshot_hash: artifact.binding.registry_snapshot_hash.clone(),
        mode_context: import_context(&item.mode_context),
        surface_ids: Vec::new(),
        target_canonical_id: None,
        relation: None,
    }
}

fn import_mode(mode: NativeReviewMode) -> NativeReviewDecisionMode {
    match mode {
        NativeReviewMode::Cluster => NativeReviewDecisionMode::Cluster,
        NativeReviewMode::Link => NativeReviewDecisionMode::Link,
    }
}

fn import_context(context: &NativeReviewModeContext) -> NativeReviewDecisionContext {
    serde_json::from_value::<NativeReviewDecisionContext>(
        serde_json::to_value(context).expect("export context serializes"),
    )
    .expect("export context maps to import context")
}

fn synthetic_review_queue(reverse_items: bool) -> ReviewQueueArtifact {
    let mut review_items = (0..500).map(synthetic_review_item).collect::<Vec<_>>();
    if reverse_items {
        review_items.reverse();
    }
    ReviewQueueArtifact {
        version: "canon_entity_review_queue.v0".to_string(),
        artifact_content_hash: "blake3:review-queue-signature-fixture".to_string(),
        metadata: metadata(),
        summary: canon::entity::EntityDeterministicSummary::default(),
        source_solve_hash: "blake3:solve-signature-fixture".to_string(),
        source_link_hash: None,
        review_items,
    }
}

fn synthetic_review_item(index: usize) -> ReviewQueueItem {
    let left = format!("surf:left:{index:03}");
    let right = format!("surf:right:{index:03}");
    let pattern = index % 8;
    let relation_hints = if pattern == 5 {
        vec![ReviewRelationHint {
            left_surface_id: left.clone(),
            right_surface_id: right.clone(),
            relation: "servicer".to_string(),
            reason_code: "relationship.role".to_string(),
        }]
    } else {
        Vec::new()
    };
    ReviewQueueItem {
        review_id: format!("review:item:{index:03}"),
        ambiguity_key: format!("pattern:{pattern}"),
        component_id: format!("component:item:{index:03}"),
        state: SolveReconciliationState::Escrow,
        proposed_action: "review_signature_pattern".to_string(),
        review_priority_units: review_priority_units(pattern),
        priority_reasons: priority_reasons(pattern),
        affected_rows: 1 + (index % 3) as u64,
        affected_deals: 1,
        surface_ids: vec![left.clone(), right.clone()],
        strongest_positive_cut: positive_cut(pattern, &left, &right),
        strongest_negative_cut: negative_cut(pattern, &left, &right),
        relation_hints,
        provenance_samples: vec![
            provenance(&left, index, "left"),
            provenance(&right, index, "right"),
        ],
    }
}

fn review_priority_units(pattern: usize) -> u32 {
    match pattern {
        0 => 6_100,
        1 => 6_300,
        2 => 8_500,
        3 => 7_400,
        4 => 4_800,
        5 => 6_900,
        6 => 7_100,
        _ => 9_000,
    }
}

fn priority_reasons(pattern: usize) -> Vec<String> {
    match pattern {
        2 => vec!["support_and_cannot_link".to_string()],
        3 => vec!["hard_cannot_link".to_string()],
        5 => vec!["related_distinct".to_string()],
        _ => vec!["operator_review_required".to_string()],
    }
}

fn positive_cut(pattern: usize, left: &str, right: &str) -> Option<SolveEvidenceCut> {
    match pattern {
        0 => Some(evidence_cut(left, right, 6_100, "name.normalized")),
        1 => Some(evidence_cut(left, right, 6_300, "address.normalized")),
        2 => Some(evidence_cut(left, right, 8_500, "name.exact")),
        5 => Some(evidence_cut(left, right, 6_900, "relationship.role")),
        6 => Some(evidence_cut(left, right, 7_100, "postal.zip")),
        7 => Some(evidence_cut(left, right, 9_000, "tax_id.exact")),
        _ => None,
    }
}

fn negative_cut(pattern: usize, left: &str, right: &str) -> Option<SolveEvidenceCut> {
    match pattern {
        2 => Some(evidence_cut(left, right, 8_300, "distinct.name_conflict")),
        3 => Some(evidence_cut(
            left,
            right,
            7_400,
            "distinct.address_conflict",
        )),
        _ => None,
    }
}

fn evidence_cut(
    left_surface_id: &str,
    right_surface_id: &str,
    score_units: u64,
    reason_code: &str,
) -> SolveEvidenceCut {
    SolveEvidenceCut {
        left_surface_id: left_surface_id.to_string(),
        right_surface_id: right_surface_id.to_string(),
        score_units: ScoreUnits::saturating_from_units(score_units),
        evidence_count: 1,
        evidence_reason_codes: vec![reason_code.to_string()],
    }
}

fn provenance(surface_id: &str, index: usize, side: &str) -> ReviewProvenanceSample {
    ReviewProvenanceSample {
        surface_id: surface_id.to_string(),
        row_id: format!("row:{side}:{index:03}"),
        source: "synthetic-review.csv".to_string(),
        raw_value: format!("Synthetic {side} {index:03}"),
    }
}

fn metadata() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: "tenant_label".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "tenant_label".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            canonical_type: "tenant_label".to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: "tenant_label.aliases".to_string(),
                distinct: "tenant_label.distinct".to_string(),
                relations: "tenant_label.relations".to_string(),
            },
            content_hash: Some("blake3:profile-signature-fixture".to_string()),
        },
        strategy: EntityStrategyReference {
            id: "tenant_label.signature_fixture".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:strategy-signature-fixture".to_string(),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "tenant-labels".to_string(),
            version: "2026.09.02".to_string(),
            source: "registries/tenant-labels".to_string(),
            lookup_snapshot_hash: "blake3:registry-signature-fixture".to_string(),
            sidecar_snapshot_hash: Some("blake3:sidecar-signature-fixture".to_string()),
        },
        patch_namespace: "tenant_label.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 500,
            content_hash: "blake3:input-signature-fixture".to_string(),
        }),
        upstream_artifacts: Vec::new(),
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}
