use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::tempdir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

#[test]
fn list_paginates_deterministically_and_read_only() {
    let temp = tempdir().expect("tempdir");
    let inbox = write_inbox_fixture(temp.path());
    let before = fs::read(&inbox).unwrap();

    let first = canon_command()
        .args([
            "inbox",
            "list",
            "--inbox",
            inbox.to_str().unwrap(),
            "--limit",
            "1",
        ])
        .assert()
        .success();
    assert!(first.get_output().stderr.is_empty());
    let first_json: Value = serde_json::from_slice(&first.get_output().stdout).unwrap();
    assert_eq!(first_json["schema_version"], "canon.inbox.list.v1");
    assert_eq!(first_json["identity_status"], "no_identity_decision");
    assert_eq!(first_json["page"]["returned"], 1);
    assert!(first_json["page"]["next_cursor"].as_str().is_some());
    assert!(
        first_json["items"][0]["next_commands"]["plan_entity"]
            .as_str()
            .unwrap()
            .contains("canon inbox plan-entity")
    );

    let repeated = canon_command()
        .args([
            "inbox",
            "list",
            "--inbox",
            inbox.to_str().unwrap(),
            "--limit",
            "1",
        ])
        .assert()
        .success();
    assert_eq!(first.get_output().stdout, repeated.get_output().stdout);

    let cursor = first_json["page"]["next_cursor"].as_str().unwrap();
    let second = canon_command()
        .args([
            "inbox",
            "list",
            "--inbox",
            inbox.to_str().unwrap(),
            "--limit",
            "1",
            "--cursor",
            cursor,
        ])
        .assert()
        .success();
    let second_json: Value = serde_json::from_slice(&second.get_output().stdout).unwrap();
    assert_ne!(
        first_json["items"][0]["event_key"],
        second_json["items"][0]["event_key"]
    );
    assert_eq!(
        fs::read(&inbox).unwrap(),
        before,
        "read command mutated inbox"
    );
}

#[test]
fn show_explain_and_stats_emit_stable_operator_json() {
    let temp = tempdir().expect("tempdir");
    let inbox = write_inbox_fixture(temp.path());
    let list = json_output(&[
        "inbox",
        "list",
        "--inbox",
        inbox.to_str().unwrap(),
        "--limit",
        "2",
        "--reason-code",
        "ambiguous_candidates",
    ]);
    assert_eq!(list["page"]["total_filtered"], 1);
    let key = list["items"][0]["event_key"].as_str().unwrap();

    let show = json_output(&[
        "inbox",
        "show",
        "--inbox",
        inbox.to_str().unwrap(),
        "--event-key",
        key,
    ]);
    assert_eq!(show["schema_version"], "canon.inbox.show.v1");
    assert_eq!(show["event_key"], key);
    assert_eq!(show["item"]["reason_code"], "ambiguous_candidates");
    assert_eq!(show["identity_status"], "no_identity_decision");

    let explain = json_output(&[
        "inbox",
        "explain",
        "--inbox",
        inbox.to_str().unwrap(),
        "--event-key",
        key,
    ]);
    assert_eq!(explain["schema_version"], "canon.inbox.explain.v1");
    assert_eq!(explain["score"]["event_key"], key);
    assert!(explain["score"]["components"].as_array().unwrap().len() > 3);
    assert_eq!(explain["provenance"]["reason_code"], "ambiguous_candidates");

    let stats = json_output(&["inbox", "stats", "--inbox", inbox.to_str().unwrap()]);
    assert_eq!(stats["schema_version"], "canon.inbox.stats.v1");
    assert_eq!(stats["inbox_summary"]["total_items"], 3);
    assert_eq!(stats["ranking_summary"]["total_items"], 3);
    assert!(
        stats["source_inbox_artifact_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
}

#[test]
fn export_apply_and_plan_require_explicit_hashes_and_paths() {
    let temp = tempdir().expect("tempdir");
    let inbox = write_inbox_fixture(temp.path());
    let stats = json_output(&["inbox", "stats", "--inbox", inbox.to_str().unwrap()]);
    let inbox_hash = stats["source_inbox_artifact_hash"].as_str().unwrap();
    let review = temp.path().join("review.json");
    let groups = temp.path().join("groups.json");
    let entity_plan = temp.path().join("entity-plan.json");

    let export = json_output(&[
        "inbox",
        "export-review",
        "--inbox",
        inbox.to_str().unwrap(),
        "--out",
        review.to_str().unwrap(),
        "--limit",
        "2",
    ]);
    assert_eq!(export["schema_version"], "canon.inbox.review_export.v1");
    assert_eq!(export["identity_status"], "no_identity_decision");
    assert_eq!(export["decisions"].as_array().unwrap().len(), 2);
    assert!(review.exists());
    let first_key = export["decisions"][0]["member_event_keys"][0]
        .as_str()
        .unwrap();

    let apply = json_output(&[
        "inbox",
        "apply-review",
        "--inbox",
        inbox.to_str().unwrap(),
        "--review",
        review.to_str().unwrap(),
        "--expected-inbox-hash",
        inbox_hash,
        "--out",
        groups.to_str().unwrap(),
    ]);
    assert_eq!(apply["schema_version"], "canon.inbox.review_apply.v1");
    assert_eq!(apply["identity_status"], "no_identity_decision");
    assert_eq!(apply["applied_decision_count"], 2);
    assert!(groups.exists());

    let plan = json_output(&[
        "inbox",
        "plan-entity",
        "--inbox",
        inbox.to_str().unwrap(),
        "--expected-inbox-hash",
        inbox_hash,
        "--event-key",
        first_key,
        "--out",
        entity_plan.to_str().unwrap(),
    ]);
    assert_eq!(plan["schema_version"], "canon.inbox.entity_plan.v1");
    assert_eq!(plan["identity_status"], "no_identity_decision");
    assert_eq!(plan["bounded_selection"]["selected_count"], 1);
    assert_eq!(plan["request"]["identity_decision"], "no_identity_decision");
    assert!(
        plan["request"]["preview_command"]
            .as_str()
            .unwrap()
            .contains("canon entity run")
    );
    assert!(
        !plan["request"]["preview_command"]
            .as_str()
            .unwrap()
            .contains("resolve")
    );
    assert!(entity_plan.exists());
}

#[test]
fn stale_plan_hash_refuses_without_creating_request() {
    let temp = tempdir().expect("tempdir");
    let inbox = write_inbox_fixture(temp.path());
    let request = temp.path().join("request.json");
    let output = canon_command()
        .args([
            "inbox",
            "plan-entity",
            "--inbox",
            inbox.to_str().unwrap(),
            "--expected-inbox-hash",
            "blake3:0000000000000000000000000000000000000000000000000000000000000000",
            "--out",
            request.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    assert!(output.get_output().stderr.is_empty());
    let refusal: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_PARSE");
    assert!(!request.exists());
}

fn json_output(args: &[&str]) -> Value {
    let output = canon_command().args(args).assert().success();
    assert!(output.get_output().stderr.is_empty());
    serde_json::from_slice(&output.get_output().stdout).unwrap()
}

fn write_inbox_fixture(root: &Path) -> PathBuf {
    let path = root.join("inbox.json");
    let artifact = json!({
        "version": "canon.unresolved.inbox.v1",
        "view": "redacted",
        "artifact_content_hash": "",
        "policy": {
            "policy_id": "policy.fixture.redacted",
            "raw_value_retention": "omit",
            "default_export_mode": "redacted",
            "merge_mode": "strict"
        },
        "summary": {},
        "items": [
            sample_item(ItemSpec {
                label: "alpha",
                event_kind: "exact_lookup",
                reason_code: "no_matching_rule",
                field_name: "issuer_name",
                field_role: "name_field",
                namespace: "registry.issuer",
                privacy_class: "internal",
                occurrence_count: 3,
                candidate_status: "none",
                candidate_count: 0,
            }),
            sample_item(ItemSpec {
                label: "beta",
                event_kind: "cluster_abstention",
                reason_code: "ambiguous_candidates",
                field_name: "borrower_name",
                field_role: "name_field",
                namespace: "registry.borrower",
                privacy_class: "restricted",
                occurrence_count: 2,
                candidate_status: "ambiguous",
                candidate_count: 5,
            }),
            sample_item(ItemSpec {
                label: "gamma",
                event_kind: "link_abstention",
                reason_code: "cannot_link",
                field_name: "counterparty_id",
                field_role: "anchor_field",
                namespace: "registry.counterparty",
                privacy_class: "public",
                occurrence_count: 1,
                candidate_status: "rejected",
                candidate_count: 1,
            })
        ]
    });
    fs::write(&path, serde_json::to_vec(&artifact).unwrap()).unwrap();
    path
}

struct ItemSpec<'a> {
    label: &'a str,
    event_kind: &'a str,
    reason_code: &'a str,
    field_name: &'a str,
    field_role: &'a str,
    namespace: &'a str,
    privacy_class: &'a str,
    occurrence_count: usize,
    candidate_status: &'a str,
    candidate_count: u32,
}

fn sample_item(spec: ItemSpec<'_>) -> Value {
    let occurrences = (0..spec.occurrence_count)
        .map(|index| {
            json!({
                "project_ref": format!("project.{}", index % 2),
                "run_ref": format!("run.{}", index),
                "source_ref": format!("source.{}", spec.label),
                "record_ref": format!("row-{}-{}", spec.label, index),
                "seen_at": format!("2026-07-10T{:02}:00:00Z", index + 1)
            })
        })
        .collect::<Vec<_>>();
    json!({
        "event_key": "",
        "event_kind": spec.event_kind,
        "reason_code": spec.reason_code,
        "field_name": spec.field_name,
        "field_role": spec.field_role,
        "profile_ref": {
            "profile_id": "profile.fixture",
            "profile_version": "1.0.0"
        },
        "surface_fingerprints": [
            {
                "normalizer_id": "fixture.trim.v1",
                "surface_role": spec.field_role,
                "fingerprint": digest(spec.label)
            }
        ],
        "namespace_hints": [
            {
                "namespace": spec.namespace,
                "source": "fixture"
            }
        ],
        "candidate_summary": {
            "status": spec.candidate_status,
            "candidate_count": spec.candidate_count,
            "best_score_band": "medium",
            "rejection_reasons": ["fixture"]
        },
        "first_seen_at": "",
        "last_seen_at": "",
        "occurrence_summary": {},
        "occurrences": occurrences,
        "privacy_class": spec.privacy_class
    })
}

fn digest(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}
