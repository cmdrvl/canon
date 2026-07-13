#![forbid(unsafe_code)]

pub use canon::{Refusal, entity};

#[path = "../src/entity/review_export.rs"]
mod review_export;

use canon::{
    RefusalCode,
    entity::{
        EntityArtifactMetadata, EntityInputReference, EntityPatchNamespaces,
        EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
        review::{
            ReviewProvenanceSample, ReviewQueueArtifact, ReviewQueueItem, ReviewRelationHint,
        },
        review_import::{
            NativeReviewDecision, NativeReviewDecisionAction, NativeReviewDecisionContext,
            NativeReviewDecisionMode, NativeReviewExpectedDecision, NativeReviewImportContext,
            import_native_review_decisions, native_review_import_context_from_artifact,
            parse_native_review_import_csv, parse_native_review_import_json,
        },
        score::ScoreUnits,
        solve::{SolveEvidenceCut, SolveReconciliationState},
    },
};
use review_export::{
    NativeReviewExportRequest, build_native_review_artifact, render_native_review_csv,
    render_native_review_html, render_native_review_json,
};
use serde_json::Value;

#[test]
fn native_review_export_projects_cluster_and_link_json_csv_html() {
    let artifact = native_artifact();

    assert_eq!(artifact.version, "canon_entity_native_review.v0");
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    assert_eq!(
        review_export::native_review_artifact_hash(&artifact).expect("hash recomputes"),
        artifact.artifact_content_hash
    );
    assert_eq!(artifact.binding.run_content_hash, "blake3:run");
    assert_eq!(artifact.binding.policy_content_hash, "blake3:policy");
    assert_eq!(artifact.binding.registry_snapshot_hash, "blake3:registry");
    assert_eq!(artifact.summary.counts["review_items"], 5);
    assert_eq!(artifact.summary.counts["candidate_clusters"], 4);
    assert_eq!(artifact.summary.counts["candidate_links"], 1);
    assert!(
        artifact
            .review_items
            .iter()
            .any(|item| !item.observations.is_empty()
                && !item.evidence_waterfall_refs.is_empty()
                && !item.conflicts.is_empty()
                && !item.related_distinct_cues.is_empty())
    );
    assert!(
        artifact
            .review_items
            .iter()
            .any(|item| !item.candidate_links.is_empty())
    );

    let json = render_native_review_json(&artifact).expect("json renders");
    assert!(json.contains("canon_entity_native_review.v0"));
    assert!(json.contains("decision_binding_hash"));

    let csv = render_native_review_csv(&artifact).expect("csv renders");
    let mut reader = csv::Reader::from_reader(csv.as_bytes());
    let headers = reader.headers().expect("headers").clone();
    assert!(headers.iter().any(|header| header == "mode_context_json"));
    assert!(
        headers
            .iter()
            .any(|header| header == "evidence_waterfall_refs_json")
    );
    assert_eq!(reader.records().count(), 5);

    let html = render_native_review_html(&artifact).expect("html renders");
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("Canon Entity Review"));
    assert!(html.contains("application/json"));
    assert!(!html.contains("http://"));
    assert!(!html.contains("https://"));
}

#[test]
fn native_review_import_derives_all_patch_types_and_refuses_bad_batches() {
    let artifact = native_artifact();
    let artifact_value = serde_json::to_value(&artifact).expect("artifact value");
    let context = native_review_import_context_from_artifact(&artifact_value).expect("context");
    let decisions = native_decisions_from_artifact(&artifact_value);

    let json = serde_json::to_string(&serde_json::json!({ "decisions": decisions }))
        .expect("decision json");
    let parsed_json = parse_native_review_import_json(&json).expect("json decisions parse");
    assert_eq!(parsed_json.len(), 5);

    let parsed_csv = parse_native_review_import_csv(&native_decision_csv(&parsed_json))
        .expect("csv decisions parse");
    assert_eq!(parsed_csv, parsed_json);

    let receipt = import_native_review_decisions(context.clone(), parsed_csv.clone())
        .expect("native import accepts");
    assert_eq!(receipt.accepted_decisions, 5);
    assert_eq!(receipt.patches.alias_patches.len(), 1);
    assert_eq!(receipt.patches.cannot_link_patches.len(), 1);
    assert_eq!(receipt.patches.relation_patches.len(), 1);
    assert_eq!(receipt.patches.assignment_patches.len(), 1);
    assert_eq!(receipt.patches.defer_patches.len(), 1);
    assert_eq!(
        receipt.patches.assignment_patches[0].canonical_id,
        "TENANT-001"
    );
    assert_eq!(receipt.patches.relation_patches[0].relation, "servicer");
    let replay_receipt =
        import_native_review_decisions(context.clone(), parsed_csv).expect("native replay accepts");
    assert_eq!(
        serde_json::to_vec(&receipt).expect("receipt bytes"),
        serde_json::to_vec(&replay_receipt).expect("replay receipt bytes")
    );

    let mut duplicate = parsed_json.clone();
    duplicate.push(parsed_json[0].clone());
    let refusal = import_native_review_decisions(context.clone(), duplicate)
        .expect_err("duplicate decision refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["writes_performed"], false);

    let mut tampered = parsed_json.clone();
    tampered[0].decision_binding_hash = "blake3:tampered".to_string();
    let refusal =
        import_native_review_decisions(context.clone(), tampered).expect_err("tamper refuses");
    assert_eq!(refusal.detail["field"], "decision_binding_hash");
    assert_eq!(refusal.detail["writes_performed"], false);

    let mut stale_source = parsed_json.clone();
    stale_source[0].source_review_artifact_hash = "blake3:old-review".to_string();
    let refusal = import_native_review_decisions(context.clone(), stale_source)
        .expect_err("stale source refuses");
    assert_eq!(refusal.detail["field"], "source_review_artifact_hash");
    assert_eq!(refusal.detail["writes_performed"], false);

    let mut stale = parsed_json.clone();
    stale[0].run_content_hash = "blake3:old-run".to_string();
    let refusal =
        import_native_review_decisions(context.clone(), stale).expect_err("stale refuses");
    assert_eq!(refusal.detail["field"], "run_content_hash");
    assert_eq!(refusal.detail["writes_performed"], false);

    let mut stale_policy = parsed_json.clone();
    stale_policy[0].policy_content_hash = "blake3:old-policy".to_string();
    let refusal = import_native_review_decisions(context.clone(), stale_policy)
        .expect_err("stale policy refuses");
    assert_eq!(refusal.detail["field"], "policy_content_hash");
    assert_eq!(refusal.detail["writes_performed"], false);

    let mut stale_registry = parsed_json;
    stale_registry[0].registry_snapshot_hash = "blake3:old-registry".to_string();
    let refusal = import_native_review_decisions(context, stale_registry)
        .expect_err("stale registry refuses");
    assert_eq!(refusal.detail["field"], "registry_snapshot_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn native_review_import_emits_singleton_alias_receipt_for_prepared_surface_collapse() {
    let mut item = cluster_item(
        "review:collapse",
        "surf:collapsed",
        "surf:discarded",
        false,
        false,
    );
    item.surface_ids = vec!["surf:collapsed".to_string()];
    item.strongest_positive_cut = None;
    item.strongest_negative_cut = None;
    item.provenance_samples = vec![provenance(
        "surf:collapsed",
        "row:collapse",
        "collapse.csv",
        "Collapsed Alias",
    )];
    let mut queue = review_queue();
    queue.review_items = vec![item];
    let artifact = build_native_review_artifact(NativeReviewExportRequest {
        review_queue: queue,
        run_content_hash: "blake3:run".to_string(),
        policy_content_hash: "blake3:policy".to_string(),
    })
    .expect("singleton native review artifact builds");
    let artifact_value = serde_json::to_value(&artifact).expect("artifact value");
    let context = native_review_import_context_from_artifact(&artifact_value).expect("context");
    let item = &artifact_value["review_items"][0];
    let binding = &artifact_value["binding"];
    let decision = NativeReviewDecision {
        review_id: "review:collapse".to_string(),
        mode: NativeReviewDecisionMode::Cluster,
        action: NativeReviewDecisionAction::Alias,
        operator_id: "operator:collapse-review".to_string(),
        reason_code: "confirmed_same_entity".to_string(),
        note: "derivation-proven prepared surface collapse".to_string(),
        source_review_artifact_hash: artifact.artifact_content_hash.clone(),
        decision_binding_hash: item["decision_binding_hash"]
            .as_str()
            .expect("decision hash")
            .to_string(),
        run_content_hash: binding["run_content_hash"]
            .as_str()
            .expect("run hash")
            .to_string(),
        policy_content_hash: binding["policy_content_hash"]
            .as_str()
            .expect("policy hash")
            .to_string(),
        registry_snapshot_hash: binding["registry_snapshot_hash"]
            .as_str()
            .expect("registry hash")
            .to_string(),
        mode_context: serde_json::from_value(item["mode_context"].clone())
            .expect("cluster context"),
        surface_ids: Vec::new(),
        target_canonical_id: Some("ORG-001".to_string()),
        relation: None,
    };

    let receipt = import_native_review_decisions(context.clone(), vec![decision.clone()])
        .expect("singleton alias decision imports");
    assert_eq!(receipt.accepted_decisions, 1);
    assert_eq!(receipt.patches.alias_patches.len(), 1);
    assert_eq!(
        receipt.patches.alias_patches[0].surface_ids,
        vec!["surf:collapsed".to_string()]
    );
    assert_eq!(receipt.patches.alias_patches[0].canonical_hint, "ORG-001");

    let mut missing_canonical = decision;
    missing_canonical.target_canonical_id = None;
    let refusal = import_native_review_decisions(context, vec![missing_canonical])
        .expect_err("singleton alias without canonical hint refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["field"], "mode_context");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn native_review_import_verifies_artifact_hash_and_canonical_shape() {
    let artifact = native_artifact();
    let mut tampered_value = serde_json::to_value(&artifact).expect("artifact value");
    tampered_value["review_items"][0]["recommended_action"] = serde_json::json!("defer");
    let refusal = native_review_import_context_from_artifact(&tampered_value)
        .expect_err("content hash tamper refuses");
    assert_eq!(refusal.detail["field"], "artifact_content_hash");
    assert_eq!(refusal.detail["writes_performed"], false);

    let mut noncanonical_value = serde_json::to_value(&artifact).expect("artifact value");
    noncanonical_value["unexpected_top_level"] = serde_json::json!(true);
    let refusal = native_review_import_context_from_artifact(&noncanonical_value)
        .expect_err("noncanonical artifact refuses");
    assert_eq!(refusal.detail["field"], "artifact");
    assert_eq!(refusal.detail["writes_performed"], false);

    let mut resealed_invalid = native_artifact();
    let link_item = resealed_invalid
        .review_items
        .iter_mut()
        .find(|item| item.review_id == "review:relation")
        .expect("link review item");
    link_item.mode_context = review_export::NativeReviewModeContext::Cluster {
        cluster_id: "cluster:wrong_mode".to_string(),
        surface_ids: vec![
            "surf:charlie".to_string(),
            "surf:charlie_servicer".to_string(),
        ],
    };
    reseal_native_artifact(&mut resealed_invalid);
    let resealed_invalid_value =
        serde_json::to_value(&resealed_invalid).expect("resealed invalid value");
    let refusal = native_review_import_context_from_artifact(&resealed_invalid_value)
        .expect_err("resealed invalid mode context refuses");
    assert_eq!(refusal.detail["field"], "mode_context");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn native_review_import_requires_exact_exported_mode_context() {
    let artifact = native_artifact();
    let artifact_value = serde_json::to_value(&artifact).expect("artifact value");
    let context = native_review_import_context_from_artifact(&artifact_value).expect("context");
    let decisions = native_decisions_from_artifact(&artifact_value);

    let mut swapped_link = decisions.clone();
    let link_decision = swapped_link
        .iter_mut()
        .find(|decision| decision.review_id == "review:relation")
        .expect("link decision");
    let NativeReviewDecisionContext::Link {
        left_surface_id,
        right_surface_id,
        ..
    } = &mut link_decision.mode_context
    else {
        panic!("link context");
    };
    let original_left = left_surface_id.clone();
    let original_right = right_surface_id.clone().expect("candidate-backed right");
    *left_surface_id = original_right;
    *right_surface_id = Some(original_left);
    let refusal = import_native_review_decisions(context.clone(), swapped_link)
        .expect_err("swapped link direction refuses");
    assert_eq!(refusal.detail["field"], "mode_context");
    assert_eq!(refusal.detail["writes_performed"], false);

    let mut tampered_hint = decisions.clone();
    let link_decision = tampered_hint
        .iter_mut()
        .find(|decision| decision.review_id == "review:relation")
        .expect("link decision");
    let NativeReviewDecisionContext::Link { relation_hints, .. } = &mut link_decision.mode_context
    else {
        panic!("link context");
    };
    relation_hints[0].relation = "borrower".to_string();
    let refusal = import_native_review_decisions(context.clone(), tampered_hint)
        .expect_err("tampered relation hint refuses");
    assert_eq!(refusal.detail["field"], "mode_context");
    assert_eq!(refusal.detail["writes_performed"], false);

    let mut tampered_cluster = decisions;
    let cluster_decision = tampered_cluster
        .iter_mut()
        .find(|decision| decision.review_id == "review:alias")
        .expect("cluster decision");
    let NativeReviewDecisionContext::Cluster { cluster_id, .. } =
        &mut cluster_decision.mode_context
    else {
        panic!("cluster context");
    };
    *cluster_id = "cluster:tampered".to_string();
    let refusal = import_native_review_decisions(context, tampered_cluster)
        .expect_err("tampered cluster context refuses");
    assert_eq!(refusal.detail["field"], "mode_context");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn native_review_import_accepts_candidate_free_link_defer() {
    let (context, decision) = candidate_free_link_decision(
        NativeReviewDecisionAction::Defer,
        &[NativeReviewDecisionAction::Defer],
    );

    let receipt = import_native_review_decisions(context, vec![decision])
        .expect("candidate-free defer imports");
    assert_eq!(receipt.accepted_decisions, 1);
    assert!(receipt.patches.alias_patches.is_empty());
    assert!(receipt.patches.cannot_link_patches.is_empty());
    assert!(receipt.patches.relation_patches.is_empty());
    assert!(receipt.patches.assignment_patches.is_empty());
    assert_eq!(receipt.patches.defer_patches.len(), 1);
    assert_eq!(
        receipt.patches.defer_patches[0].surface_ids,
        vec!["surf:unmatched_target".to_string()]
    );
}

#[test]
fn native_review_import_refuses_candidate_free_link_relation_and_cannot_link() {
    for action in [
        NativeReviewDecisionAction::Relation,
        NativeReviewDecisionAction::CannotLink,
    ] {
        let (context, decision) = candidate_free_link_decision(action, &[action]);

        let refusal = import_native_review_decisions(context, vec![decision])
            .expect_err("candidate-free action requiring a right side refuses");
        assert_eq!(refusal.detail["field"], "mode_context");
        assert_eq!(refusal.detail["action"], action.as_str());
        assert_eq!(refusal.detail["writes_performed"], false);
    }
}

#[test]
fn native_review_import_refuses_contradictory_identity_batch_atomically() {
    let mut queue = review_queue();
    queue.review_items = vec![
        cluster_item(
            "review:alias",
            "surf:conflict_left",
            "surf:conflict_right",
            true,
            false,
        ),
        cluster_item(
            "review:cannot",
            "surf:conflict_left",
            "surf:conflict_right",
            true,
            true,
        ),
    ];
    let artifact = build_native_review_artifact(NativeReviewExportRequest {
        review_queue: queue,
        run_content_hash: "blake3:run".to_string(),
        policy_content_hash: "blake3:policy".to_string(),
    })
    .expect("contradictory native review artifact builds");
    let artifact_value = serde_json::to_value(&artifact).expect("artifact value");
    let context = native_review_import_context_from_artifact(&artifact_value).expect("context");
    let decisions = native_decisions_from_artifact(&artifact_value);

    let refusal = import_native_review_decisions(context, decisions)
        .expect_err("contradictory identity decisions refuse");
    assert_eq!(refusal.detail["reason"], "identity_cannot_link_conflict");
    assert_eq!(refusal.detail["writes_performed"], false);
}

fn native_artifact() -> review_export::NativeReviewArtifact {
    build_native_review_artifact(NativeReviewExportRequest {
        review_queue: review_queue(),
        run_content_hash: "blake3:run".to_string(),
        policy_content_hash: "blake3:policy".to_string(),
    })
    .expect("native review artifact builds")
}

fn candidate_free_link_decision(
    action: NativeReviewDecisionAction,
    allowed_actions: &[NativeReviewDecisionAction],
) -> (NativeReviewImportContext, NativeReviewDecision) {
    let artifact = native_artifact();
    let artifact_value = serde_json::to_value(&artifact).expect("artifact value");
    let mut context = native_review_import_context_from_artifact(&artifact_value).expect("context");
    let review_id = format!("review:candidate_free_{}", action.as_str());
    let decision_binding_hash = format!("blake3:candidate-free-{}", action.as_str());
    let mode_context = NativeReviewDecisionContext::Link {
        left_surface_id: "surf:unmatched_target".to_string(),
        right_surface_id: None,
        relation_hints: Vec::new(),
        relation: None,
    };
    context.expected_decisions.clear();
    context.expected_decisions.insert(
        review_id.clone(),
        NativeReviewExpectedDecision {
            mode: NativeReviewDecisionMode::Link,
            decision_binding_hash: decision_binding_hash.clone(),
            mode_context: mode_context.clone(),
            surface_ids: vec!["surf:unmatched_target".to_string()],
            allowed_actions: allowed_actions.iter().copied().collect(),
        },
    );
    let decision = NativeReviewDecision {
        review_id,
        mode: NativeReviewDecisionMode::Link,
        action,
        operator_id: "operator:native-review".to_string(),
        reason_code: action.as_str().to_string(),
        note: "candidate-free unmatched target".to_string(),
        source_review_artifact_hash: context.source_review_artifact_hash.clone(),
        decision_binding_hash,
        run_content_hash: context.run_content_hash.clone(),
        policy_content_hash: context.policy_content_hash.clone(),
        registry_snapshot_hash: context.registry_snapshot_hash.clone(),
        mode_context,
        surface_ids: Vec::new(),
        target_canonical_id: None,
        relation: (action == NativeReviewDecisionAction::Relation).then(|| "servicer".to_string()),
    };
    (context, decision)
}

fn reseal_native_artifact(artifact: &mut review_export::NativeReviewArtifact) {
    let hash = review_export::native_review_artifact_hash(artifact).expect("hash recomputes");
    artifact.artifact_content_hash = hash.clone();
    artifact.metadata.artifact_content_hash = hash;
}

fn native_decisions_from_artifact(artifact: &Value) -> Vec<NativeReviewDecision> {
    let source_hash = artifact["artifact_content_hash"]
        .as_str()
        .expect("artifact hash")
        .to_string();
    let binding = &artifact["binding"];
    artifact["review_items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| {
            let review_id = item["review_id"].as_str().expect("review id").to_string();
            let mode = match item["mode"].as_str().expect("mode") {
                "cluster" => NativeReviewDecisionMode::Cluster,
                "link" => NativeReviewDecisionMode::Link,
                other => panic!("unexpected mode {other}"),
            };
            let action = match review_id.as_str() {
                "review:alias" => NativeReviewDecisionAction::Alias,
                "review:cannot" => NativeReviewDecisionAction::CannotLink,
                "review:relation" => NativeReviewDecisionAction::Relation,
                "review:assignment" => NativeReviewDecisionAction::Assignment,
                "review:defer" => NativeReviewDecisionAction::Defer,
                other => panic!("unexpected review id {other}"),
            };
            NativeReviewDecision {
                review_id,
                mode,
                action,
                operator_id: "operator:native-review".to_string(),
                reason_code: action.as_str().to_string(),
                note: "offline native review fixture".to_string(),
                source_review_artifact_hash: source_hash.clone(),
                decision_binding_hash: item["decision_binding_hash"]
                    .as_str()
                    .expect("decision hash")
                    .to_string(),
                run_content_hash: binding["run_content_hash"]
                    .as_str()
                    .expect("run")
                    .to_string(),
                policy_content_hash: binding["policy_content_hash"]
                    .as_str()
                    .expect("policy")
                    .to_string(),
                registry_snapshot_hash: binding["registry_snapshot_hash"]
                    .as_str()
                    .expect("registry")
                    .to_string(),
                mode_context: serde_json::from_value(item["mode_context"].clone())
                    .expect("mode context"),
                surface_ids: Vec::new(),
                target_canonical_id: (action == NativeReviewDecisionAction::Assignment)
                    .then(|| "TENANT-001".to_string()),
                relation: (action == NativeReviewDecisionAction::Relation)
                    .then(|| "servicer".to_string()),
            }
        })
        .collect()
}

fn native_decision_csv(decisions: &[NativeReviewDecision]) -> String {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record([
            "review_id",
            "mode",
            "action",
            "operator_id",
            "reason_code",
            "note",
            "source_review_artifact_hash",
            "decision_binding_hash",
            "run_content_hash",
            "policy_content_hash",
            "registry_snapshot_hash",
            "mode_context_json",
            "surface_ids_json",
            "target_canonical_id",
            "relation",
        ])
        .expect("csv header");
    for decision in decisions {
        writer
            .write_record([
                decision.review_id.clone(),
                mode_str(decision.mode).to_string(),
                decision.action.as_str().to_string(),
                decision.operator_id.clone(),
                decision.reason_code.clone(),
                decision.note.clone(),
                decision.source_review_artifact_hash.clone(),
                decision.decision_binding_hash.clone(),
                decision.run_content_hash.clone(),
                decision.policy_content_hash.clone(),
                decision.registry_snapshot_hash.clone(),
                serde_json::to_string(&decision.mode_context).expect("context json"),
                serde_json::to_string(&decision.surface_ids).expect("surfaces json"),
                decision.target_canonical_id.clone().unwrap_or_default(),
                decision.relation.clone().unwrap_or_default(),
            ])
            .expect("csv row");
    }
    String::from_utf8(writer.into_inner().expect("csv bytes")).expect("csv utf8")
}

fn mode_str(mode: NativeReviewDecisionMode) -> &'static str {
    match mode {
        NativeReviewDecisionMode::Cluster => "cluster",
        NativeReviewDecisionMode::Link => "link",
    }
}

fn review_queue() -> ReviewQueueArtifact {
    ReviewQueueArtifact {
        version: "canon_entity_review_queue.v0".to_string(),
        artifact_content_hash: "blake3:review-queue".to_string(),
        metadata: metadata(),
        summary: canon::entity::EntityDeterministicSummary::default(),
        source_solve_hash: "blake3:solve".to_string(),
        source_link_hash: None,
        review_items: vec![
            cluster_item("review:alias", "surf:alpha", "surf:alpha_llc", true, false),
            cluster_item("review:cannot", "surf:bravo", "surf:bravo_alt", true, true),
            link_item(),
            cluster_item(
                "review:assignment",
                "surf:delta",
                "surf:delta_inc",
                false,
                false,
            ),
            cluster_item(
                "review:defer",
                "surf:echo",
                "surf:echo_partners",
                false,
                false,
            ),
        ],
    }
}

fn cluster_item(
    review_id: &str,
    left: &str,
    right: &str,
    positive: bool,
    negative: bool,
) -> ReviewQueueItem {
    ReviewQueueItem {
        review_id: review_id.to_string(),
        ambiguity_key: review_id.to_string(),
        component_id: format!("component:{}", review_id.replace(':', "_")),
        state: SolveReconciliationState::Escrow,
        proposed_action: "confirm_merge_distinct_or_relation".to_string(),
        review_priority_units: 7000,
        priority_reasons: if negative {
            vec!["support_and_cannot_link".to_string()]
        } else {
            vec!["operator_review_required".to_string()]
        },
        affected_rows: 10,
        affected_deals: 2,
        surface_ids: vec![left.to_string(), right.to_string()],
        strongest_positive_cut: positive.then(|| evidence_cut(left, right, 8600, "name_match")),
        strongest_negative_cut: negative.then(|| evidence_cut(left, right, 8300, "distinct_cue")),
        relation_hints: Vec::new(),
        provenance_samples: vec![
            provenance(left, "row:1", "tape.csv", "Alpha LLC"),
            provenance(right, "row:2", "servicer.csv", "Alpha"),
        ],
    }
}

fn link_item() -> ReviewQueueItem {
    ReviewQueueItem {
        review_id: "review:relation".to_string(),
        ambiguity_key: "relation".to_string(),
        component_id: "component:relation".to_string(),
        state: SolveReconciliationState::Escrow,
        proposed_action: "confirm_merge_distinct_or_relation".to_string(),
        review_priority_units: 6500,
        priority_reasons: vec!["related_distinct".to_string()],
        affected_rows: 7,
        affected_deals: 1,
        surface_ids: vec![
            "surf:charlie".to_string(),
            "surf:charlie_servicer".to_string(),
        ],
        strongest_positive_cut: Some(evidence_cut(
            "surf:charlie",
            "surf:charlie_servicer",
            6400,
            "relationship_hint",
        )),
        strongest_negative_cut: None,
        relation_hints: vec![ReviewRelationHint {
            left_surface_id: "surf:charlie".to_string(),
            right_surface_id: "surf:charlie_servicer".to_string(),
            relation: "servicer".to_string(),
            reason_code: "related_distinct".to_string(),
        }],
        provenance_samples: vec![
            provenance("surf:charlie", "row:3", "deal.csv", "Charlie"),
            provenance(
                "surf:charlie_servicer",
                "row:4",
                "deal.csv",
                "Charlie Servicer",
            ),
        ],
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

fn provenance(
    surface_id: &str,
    row_id: &str,
    source: &str,
    raw_value: &str,
) -> ReviewProvenanceSample {
    ReviewProvenanceSample {
        surface_id: surface_id.to_string(),
        row_id: row_id.to_string(),
        source: source.to_string(),
        raw_value: raw_value.to_string(),
    }
}

fn metadata() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: "cmbs_tenant_label".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "tenant_label".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            canonical_type: "tenant_label".to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: "cmbs_tenant_label.aliases".to_string(),
                distinct: "cmbs_tenant_label.distinct".to_string(),
                relations: "cmbs_tenant_label.relations".to_string(),
            },
            content_hash: Some("blake3:profile".to_string()),
        },
        strategy: EntityStrategyReference {
            id: "cmbs_tenant_label.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:strategy".to_string(),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "cmbs-tenants".to_string(),
            version: "2026.06.25".to_string(),
            source: "registries/cmbs-tenants".to_string(),
            lookup_snapshot_hash: "blake3:registry".to_string(),
            sidecar_snapshot_hash: Some("blake3:sidecars".to_string()),
        },
        patch_namespace: "cmbs_tenant_label.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 100,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: Vec::new(),
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}
