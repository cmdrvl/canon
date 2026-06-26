#![forbid(unsafe_code)]

use canon::entity::{
    apply::{
        ApplyCanonicalResolution, ApplyRegistryReference, ApplySafetyCheck, ApplyStreamRequest,
        run_apply_streaming,
    },
    run::{EntityRunRequest, run_entity_workbench},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const SUMMARY_PATH: &str = "tests/fixtures/entity/cmbs/e2e/expected_summary.json";
const APPLY_EXPECTED_PATH: &str = "tests/fixtures/entity/cmbs/e2e/expected_apply.csv";
const SMALL_BOOK_ROOT: &str = "tests/fixtures/entity/cmbs/small_book";
const OBSERVATIONS_PATH: &str = "tests/fixtures/entity/cmbs/small_book/observations.csv";
const STRATEGY_PATH: &str = "tests/fixtures/entity/profiles/cmbs_tenant_label.yaml";

#[test]
fn cmbs_e2e_small_book_operator_summary_matches_artifacts() {
    let expected = expected_summary();
    let run = run_small_book();
    let artifact = run.artifact;
    let artifact_summary = &expected["artifact_summary"];

    assert_eq!(
        expected["schema_version"],
        "canon.entity.cmbs_e2e_summary.v0"
    );
    assert_eq!(expected["profile_id"], "cmbs_tenant_label");
    assert_eq!(artifact.summary.labels["profile_id"], "cmbs_tenant_label");
    assert_eq!(
        artifact.summary.counts["row_count"],
        u64_at(artifact_summary, "row_count")
    );
    assert_eq!(
        raw_unique_names(),
        u64_at(artifact_summary, "raw_unique_names")
    );
    for key in [
        "prepared_surfaces",
        "exact_resolved_surfaces",
        "exact_bucket_count",
        "candidate_pairs",
        "edge_records",
        "solved_entities",
        "review_group_count",
    ] {
        assert_eq!(
            artifact.summary.counts[key],
            u64_at(artifact_summary, key),
            "{key}"
        );
    }

    let solve = read_json(&run.work_dir.join("solve/solve.json"));
    assert_eq!(
        solve["summary"]["counts"]["promotable_new"],
        artifact_summary["promotable_aliases"]
    );
    let index = read_json(&run.work_dir.join("index.json"));
    assert_eq!(
        index["summary"]["labels"]["cache_status"],
        artifact_summary["cache_status"]
    );

    for command in strings(&expected["operator_review_summary"]["next_commands_required"]) {
        assert!(
            next_commands_json(&artifact).contains_key(command.as_str()),
            "summary exposes next command {command}"
        );
    }
}

#[test]
fn review_grouping_summary_exposes_unresolved_tokens_and_antimerge_reasons() {
    let expected = expected_summary();
    let review = review_queue_rows();
    let profile_summary = small_book_profile_summary();
    let operator = &expected["operator_review_summary"];

    assert_eq!(review.len() as u64, u64_at(operator, "review_groups"));
    assert_eq!(
        review
            .iter()
            .map(|row| row["row_count"].parse::<u64>().expect("row_count"))
            .sum::<u64>(),
        u64_at(operator, "review_rows")
    );
    assert_eq!(
        review
            .iter()
            .map(|row| row["deal_count"].parse::<u64>().expect("deal_count"))
            .sum::<u64>(),
        u64_at(operator, "review_deals")
    );
    assert_eq!(
        review
            .iter()
            .filter(|row| row["reason_code"].contains("not_same_tenant_label"))
            .count() as u64,
        u64_at(operator, "anti_merge_groups")
    );
    assert_eq!(
        top_unresolved_tokens(5),
        strings(&operator["top_unresolved_tokens"])
    );
    assert_eq!(
        top_anti_merge_reasons(&profile_summary),
        strings(&operator["top_anti_merge_reasons"])
    );
}

#[test]
fn cmbs_e2e_apply_output_preserves_rows_and_appends_canonical_fields() {
    let expected = expected_summary();
    let apply_summary = &expected["apply_summary"];
    let temp = tempfile::tempdir().expect("tempdir");
    let output = temp.path().join("small-book.canon.csv");
    let rows = repo_path(OBSERVATIONS_PATH);
    let resolutions = apply_resolutions();
    let artifact = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "raw_tenant_name",
        registry: ApplyRegistryReference {
            id: string_at(apply_summary, "registry_id"),
            version: string_at(apply_summary, "registry_version"),
        },
        resolutions: &resolutions,
        safety: ApplySafetyCheck {
            expected_profile_id: Some("cmbs_tenant_label".to_string()),
            actual_profile_id: Some("cmbs_tenant_label".to_string()),
            expected_identity_semantics: Some("canonical_display_label".to_string()),
            actual_identity_semantics: Some("canonical_display_label".to_string()),
            expected_registry_snapshot_hash: Some("blake3:cmbs-e2e-registry".to_string()),
            actual_registry_snapshot_hash: Some("blake3:cmbs-e2e-registry".to_string()),
            ..ApplySafetyCheck::default()
        },
        require_full_resolution: false,
        target_rows_per_chunk: 4,
    })
    .expect("CMBS e2e apply replay succeeds");

    assert_eq!(artifact.summary["rows"], u64_at(apply_summary, "row_count"));
    assert_eq!(
        artifact.summary["resolved"],
        u64_at(apply_summary, "resolved")
    );
    assert_eq!(
        artifact.summary["unresolved"],
        u64_at(apply_summary, "unresolved")
    );
    assert_eq!(
        fs::read_to_string(&output).expect("apply output"),
        fs::read_to_string(repo_path(APPLY_EXPECTED_PATH)).expect("expected apply")
    );
}

struct RunFixture {
    _temp: tempfile::TempDir,
    work_dir: PathBuf,
    artifact: canon::entity::run::EntityRunArtifact,
}

fn run_small_book() -> RunFixture {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    write_cmbs_registry(&registry);
    let result = run_entity_workbench(EntityRunRequest {
        rows: &repo_path(OBSERVATIONS_PATH),
        profile: "cmbs_tenant_label",
        strategy: &repo_path(STRATEGY_PATH),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("CMBS small-book run succeeds");
    RunFixture {
        _temp: temp,
        work_dir,
        artifact: result.artifact,
    }
}

fn write_cmbs_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.25","description":"CMBS e2e test registry","updated":"2026-06-25","entry_count":8}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_vec_pretty(&json!([
            {"input":"Sears","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"SEARS LLC","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"Sears Roebuck & Co.","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 Hour Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HOUR FITNESS USA, INC.","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HR Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 Sand Island Prop","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 SAND ISLAND PROPERTY LLC","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"}
        ]))
        .expect("aliases json"),
    )
    .expect("aliases");
}

fn apply_resolutions() -> BTreeMap<String, ApplyCanonicalResolution> {
    [
        ("Sears", "TNT-SEARS"),
        ("SEARS LLC", "TNT-SEARS"),
        ("Sears Roebuck & Co.", "TNT-SEARS"),
        ("24 Hour Fitness", "TNT-24-HOUR-FITNESS"),
        ("24 HOUR FITNESS USA, INC.", "TNT-24-HOUR-FITNESS"),
        ("24 HR Fitness", "TNT-24-HOUR-FITNESS"),
        ("238 Sand Island Prop", "TNT-238-SAND-ISLAND-PROPERTY"),
        (
            "238 SAND ISLAND PROPERTY LLC",
            "TNT-238-SAND-ISLAND-PROPERTY",
        ),
    ]
    .into_iter()
    .map(|(input, canonical_id)| {
        (
            input.to_string(),
            ApplyCanonicalResolution {
                canonical_id: canonical_id.to_string(),
                canonical_type: "tenant_label".to_string(),
                rule_id: "REGISTRY_EXACT".to_string(),
            },
        )
    })
    .collect()
}

fn raw_unique_names() -> u64 {
    observation_rows()
        .into_iter()
        .map(|row| row["raw_tenant_name"].clone())
        .collect::<BTreeSet<_>>()
        .len() as u64
}

fn top_unresolved_tokens(limit: usize) -> Vec<String> {
    let mut counts = BTreeMap::<String, u64>::new();
    for row in observation_rows()
        .into_iter()
        .filter(|row| row["expected_resolution_status"] != "exact_resolved")
    {
        for token in row["expected_normalized_surface"].split_whitespace() {
            *counts.entry(token.to_string()).or_default() += 1;
        }
    }
    let mut ranked = counts.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(token, _)| token)
        .collect()
}

fn top_anti_merge_reasons(profile_summary: &Value) -> Vec<String> {
    let mut reasons = profile_summary["hard_negative_pairs"]
        .as_array()
        .expect("hard negative pairs")
        .iter()
        .map(|pair| pair["reason_code"].as_str().expect("reason").to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    reasons.sort();
    reasons
}

fn next_commands_json(
    artifact: &canon::entity::run::EntityRunArtifact,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "review_export".to_string(),
            artifact.next_commands.review_export.clone(),
        ),
        ("audit".to_string(), artifact.next_commands.audit.clone()),
        (
            "promote".to_string(),
            artifact.next_commands.promote.clone(),
        ),
        ("apply".to_string(), artifact.next_commands.apply.clone()),
    ])
}

fn observation_rows() -> Vec<BTreeMap<String, String>> {
    read_csv_rows(&repo_path(OBSERVATIONS_PATH))
}

fn review_queue_rows() -> Vec<BTreeMap<String, String>> {
    read_csv_rows(&repo_path(&format!("{SMALL_BOOK_ROOT}/review_queue.csv")))
}

fn read_csv_rows(path: &Path) -> Vec<BTreeMap<String, String>> {
    csv::Reader::from_path(path)
        .expect("csv opens")
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("csv parses")
}

fn expected_summary() -> Value {
    read_json(&repo_path(SUMMARY_PATH))
}

fn small_book_profile_summary() -> Value {
    read_json(&repo_path(&format!(
        "{SMALL_BOOK_ROOT}/expected_summary.json"
    )))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn u64_at(value: &Value, key: &str) -> u64 {
    value[key]
        .as_u64()
        .unwrap_or_else(|| panic!("{key} must be u64"))
}

fn string_at(value: &Value, key: &str) -> String {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} must be string"))
        .to_string()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
