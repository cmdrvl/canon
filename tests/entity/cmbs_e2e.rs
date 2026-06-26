#![forbid(unsafe_code)]

use canon::entity::{
    apply::{
        APPLY_CANONICAL_FIELDS, ApplyCanonicalResolution, ApplyRegistryReference, ApplySafetyCheck,
        ApplyStreamRequest, run_apply_streaming,
    },
    run::{EntityRunRequest, render_run_summary, run_entity_workbench},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const OBSERVATIONS_PATH: &str = "tests/fixtures/entity/cmbs/small_book/observations.csv";
const SMALL_BOOK_SUMMARY_PATH: &str = "tests/fixtures/entity/cmbs/small_book/expected_summary.json";
const REVIEW_QUEUE_PATH: &str = "tests/fixtures/entity/cmbs/small_book/review_queue.csv";
const PROMOTION_MANIFEST_PATH: &str = "tests/fixtures/entity/cmbs/promotion_loop/manifest.json";
const E2E_SUMMARY_PATH: &str = "tests/fixtures/entity/cmbs/e2e/operator_summary.json";
const E2E_APPLY_EXPECTED_PATH: &str = "tests/fixtures/entity/cmbs/e2e/expected_apply.csv";
const STRATEGY_PATH: &str = "tests/fixtures/entity/profiles/cmbs_tenant_label.yaml";

#[test]
fn cmbs_e2e_small_book_operator_summary_is_semantic_and_replayable() {
    let expected = json_fixture(E2E_SUMMARY_PATH);
    let small_book = json_fixture(SMALL_BOOK_SUMMARY_PATH);
    assert_eq!(
        expected["schema_version"],
        "canon.entity.cmbs_e2e_operator_summary.v0"
    );
    assert_eq!(expected["profile_id"], "cmbs_tenant_label");
    assert_eq!(expected["identity_semantics"], "canonical_display_label");

    let rows = csv_rows(&fixture(OBSERVATIONS_PATH));
    assert_source_summary(&rows, &expected);
    assert_expected_surfaces(&small_book, &expected);
    assert_operator_rollups(&rows, &small_book, &expected);
    assert_review_groups(&expected);
    assert_promotable_aliases(&expected);

    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    write_cmbs_registry(&registry);

    let run = run_entity_workbench(EntityRunRequest {
        rows: &fixture(OBSERVATIONS_PATH),
        profile: "cmbs_tenant_label",
        strategy: &fixture(STRATEGY_PATH),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("CMBS e2e run succeeds");
    assert_run_summary(&run.artifact.summary_as_json(), &expected);
    assert_stage_artifacts(&work_dir, &expected);

    let summary_line = render_run_summary(&run.artifact);
    for required in [
        "canon_entity_run.v0",
        "profile=cmbs_tenant_label",
        "registry=cmbs-tenants@2026.06.26",
        "review_groups=",
    ] {
        assert!(
            summary_line.contains(required),
            "run summary omits {required}: {summary_line}"
        );
    }
    assert_next_commands(&run.artifact.next_commands_as_json(), &expected);
    assert_apply_replay(&rows, temp.path(), &expected);
}

fn assert_source_summary(rows: &[BTreeMap<String, String>], expected: &Value) {
    assert_eq!(rows.len() as u64, u64_at(&expected["source"], "rows"));
    assert_eq!(
        unique_count(rows, "deal_id"),
        u64_at(&expected["source"], "deals")
    );
    assert_eq!(
        unique_count(rows, "property_id"),
        u64_at(&expected["source"], "properties")
    );
    assert_eq!(
        unique_count(rows, "raw_tenant_name"),
        u64_at(&expected["source"], "raw_unique_names")
    );
}

fn assert_expected_surfaces(small_book: &Value, expected: &Value) {
    assert_eq!(
        small_book["prepare_summary"]["normalized_unique_surfaces"],
        expected["prepared_surfaces"]["normalized_expected_count"]
    );
    assert_eq!(
        small_book["prepare_summary"]["exact_resolved_surface_count"],
        expected["prepared_surfaces"]["exact_resolved_surface_count"]
    );
    assert_eq!(
        small_book["prepare_summary"]["global_surface_scope"],
        expected["prepared_surfaces"]["global_surface_scope"]
    );
    assert_eq!(
        small_book["exact_resolved_surfaces"],
        expected["exact_resolved_surfaces"]
    );
}

fn assert_operator_rollups(
    rows: &[BTreeMap<String, String>],
    small_book: &Value,
    expected: &Value,
) {
    assert_eq!(
        top_unresolved_tokens(rows, 3),
        count_pairs(
            &expected["operator_summary"]["top_unresolved_tokens"],
            "token"
        )
    );
    assert_eq!(
        anti_merge_reason_counts(small_book),
        count_pairs(
            &expected["operator_summary"]["top_anti_merge_reasons"],
            "reason_code"
        )
    );
}

fn assert_review_groups(expected: &Value) {
    let queue = csv_rows(&fixture(REVIEW_QUEUE_PATH));
    let expected_groups = expected["review"]["groups"]
        .as_array()
        .expect("review groups")
        .iter()
        .map(|group| (str_at(group, "id").to_string(), group))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        queue.len() as u64,
        u64_at(&expected["review"], "group_count")
    );
    assert_eq!(
        queue
            .iter()
            .map(|row| parse_u64(row, "row_count"))
            .sum::<u64>(),
        u64_at(&expected["review"], "rows_covered")
    );

    for row in queue {
        let group_id = row.get("review_group_id").expect("review_group_id");
        let expected_group = expected_groups
            .get(group_id)
            .unwrap_or_else(|| panic!("unexpected review group {group_id}"));
        assert_eq!(row["reason_code"], str_at(expected_group, "reason_code"));
        assert_eq!(
            parse_u64(&row, "row_count"),
            u64_at(expected_group, "row_count")
        );
        assert_eq!(
            parse_u64(&row, "deal_count"),
            u64_at(expected_group, "deal_count")
        );
        assert_eq!(
            row["suggested_action"],
            str_at(expected_group, "suggested_action")
        );
    }
}

fn assert_promotable_aliases(expected: &Value) {
    let promotion = json_fixture(PROMOTION_MANIFEST_PATH);
    assert_eq!(
        promotion["expected_aliases"],
        expected["promotion"]["promotable_aliases"]
    );
}

fn assert_run_summary(summary: &Value, expected: &Value) {
    assert_eq!(summary["counts"]["row_count"], expected["source"]["rows"]);
    assert_eq!(
        summary["counts"]["prepared_surfaces"],
        expected["run_artifact"]["prepared_surfaces"]
    );
    assert_eq!(
        summary["counts"]["exact_resolved_surfaces"],
        expected["run_artifact"]["exact_resolved_surfaces"]
    );
    assert_eq!(summary["labels"]["profile_id"], expected["profile_id"]);
    assert_eq!(summary["labels"]["registry_id"], "cmbs-tenants");
    assert_eq!(summary["labels"]["registry_version"], "2026.06.26");
}

fn assert_stage_artifacts(work_dir: &Path, expected: &Value) {
    let surfaces = jsonl_values(&work_dir.join("prepare/surfaces.jsonl"));
    assert_eq!(
        surfaces.len() as u64,
        u64_at(&expected["run_artifact"], "prepared_surfaces")
    );
    let exact_surfaces = surfaces
        .iter()
        .filter(|surface| surface["exact_lookup"]["status"] == "resolved")
        .collect::<Vec<_>>();
    assert_eq!(
        exact_surfaces.len() as u64,
        u64_at(&expected["run_artifact"], "exact_resolved_surfaces")
    );
    let exact_ids = exact_surfaces
        .iter()
        .filter_map(|surface| surface["exact_lookup"]["canonical_id"].as_str())
        .collect::<BTreeSet<_>>();
    for surface in expected["exact_resolved_surfaces"]
        .as_array()
        .expect("exact surfaces")
    {
        assert!(
            exact_ids.contains(str_at(surface, "canonical_id")),
            "missing exact canonical id {}",
            str_at(surface, "canonical_id")
        );
    }

    let index = json_file(&work_dir.join("index.json"));
    assert_eq!(
        index["summary"]["labels"]["cache_status"],
        expected["cache"]["index_status"]
    );
}

fn assert_next_commands(next_commands: &Value, expected: &Value) {
    for command_name in strings(&expected["operator_summary"]["next_commands"]) {
        let command = next_commands
            .get(command_name.as_str())
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("missing next command {command_name}"));
        assert!(
            command.starts_with("canon entity "),
            "{command_name} next command should be operator-runnable: {command}"
        );
    }
}

fn assert_apply_replay(rows: &[BTreeMap<String, String>], temp_root: &Path, expected: &Value) {
    let output = temp_root.join("cmbs-small-book.canon.csv");
    let rows_path = fixture(OBSERVATIONS_PATH);
    let resolutions = apply_resolutions(rows);
    let request = apply_request(&rows_path, &output, &resolutions);
    let first = run_apply_streaming(request).expect("first e2e apply succeeds");
    let first_bytes = fs::read(&output).expect("first apply bytes");
    let second = run_apply_streaming(apply_request(&rows_path, &output, &resolutions))
        .expect("second apply succeeds");
    let second_bytes = fs::read(&output).expect("second apply bytes");

    assert_eq!(first_bytes, second_bytes);
    assert_eq!(first.artifact_content_hash, second.artifact_content_hash);
    assert_eq!(first.summary["rows"], u64_at(&expected["apply"], "rows"));
    assert_eq!(
        first.summary["resolved"],
        u64_at(&expected["apply"], "resolved")
    );
    assert_eq!(
        first.summary["unresolved"],
        u64_at(&expected["apply"], "unresolved")
    );
    assert_eq!(
        fs::read_to_string(&output).expect("apply output"),
        fs::read_to_string(fixture(E2E_APPLY_EXPECTED_PATH)).expect("expected apply output")
    );
    assert_raw_fields_preserved(&fixture(OBSERVATIONS_PATH), &output);
    assert_eq!(
        strings(&expected["apply"]["canonical_fields"]),
        APPLY_CANONICAL_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect::<BTreeSet<_>>()
    );
}

fn apply_request<'a>(
    rows: &'a Path,
    output: &'a Path,
    resolutions: &'a BTreeMap<String, ApplyCanonicalResolution>,
) -> ApplyStreamRequest<'a> {
    ApplyStreamRequest {
        rows,
        output,
        lookup_column: "raw_tenant_name",
        registry: ApplyRegistryReference {
            id: "cmbs-tenants".to_string(),
            version: "2026.06.26".to_string(),
        },
        resolutions,
        safety: ApplySafetyCheck {
            expected_profile_id: Some("cmbs_tenant_label".to_string()),
            actual_profile_id: Some("cmbs_tenant_label".to_string()),
            expected_identity_semantics: Some("canonical_display_label".to_string()),
            actual_identity_semantics: Some("canonical_display_label".to_string()),
            expected_registry_snapshot_hash: Some("blake3:cmbs-e2e-registry".to_string()),
            actual_registry_snapshot_hash: Some("blake3:cmbs-e2e-registry".to_string()),
            expected_sidecar_artifact_version: Some(
                "canon_entity_promotion_sidecar.v0".to_string(),
            ),
            actual_sidecar_artifact_version: Some("canon_entity_promotion_sidecar.v0".to_string()),
            expected_sidecar_snapshot_hash: Some("blake3:cmbs-e2e-sidecars".to_string()),
            actual_sidecar_snapshot_hash: Some("blake3:cmbs-e2e-sidecars".to_string()),
        },
        require_full_resolution: false,
        target_rows_per_chunk: 5,
    }
}

fn apply_resolutions(
    rows: &[BTreeMap<String, String>],
) -> BTreeMap<String, ApplyCanonicalResolution> {
    rows.iter()
        .filter(|row| row["expected_resolution_status"] == "exact_resolved")
        .map(|row| {
            (
                row["raw_tenant_name"].clone(),
                ApplyCanonicalResolution {
                    canonical_id: row["expected_canonical_id"].clone(),
                    canonical_type: "tenant_label".to_string(),
                    rule_id: "CMBS_ALIAS".to_string(),
                },
            )
        })
        .collect()
}

fn assert_raw_fields_preserved(input: &Path, output: &Path) {
    let input_rows = csv_rows(input);
    let output_rows = csv_rows(output);
    assert_eq!(input_rows.len(), output_rows.len());
    for (input, output) in input_rows.iter().zip(output_rows.iter()) {
        for (key, value) in input {
            assert_eq!(output.get(key), Some(value), "raw field {key} changed");
        }
    }
}

fn top_unresolved_tokens(rows: &[BTreeMap<String, String>], limit: usize) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::<String, u64>::new();
    for row in rows
        .iter()
        .filter(|row| row["expected_resolution_status"] != "exact_resolved")
    {
        for token in row["expected_normalized_surface"].split_whitespace() {
            if token.starts_with("placeholder:") {
                continue;
            }
            *counts.entry(token.to_string()).or_default() += 1;
        }
    }
    let mut ordered = counts.into_iter().collect::<Vec<_>>();
    ordered.sort_by(|(left_token, left_count), (right_token, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_token.cmp(right_token))
    });
    ordered.into_iter().take(limit).collect()
}

fn anti_merge_reason_counts(small_book: &Value) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for pair in small_book["hard_negative_pairs"]
        .as_array()
        .expect("hard negative pairs")
    {
        *counts
            .entry(str_at(pair, "reason_code").to_string())
            .or_default() += 1;
    }
    counts
}

fn count_pairs(value: &Value, key: &str) -> BTreeMap<String, u64> {
    value
        .as_array()
        .expect("count pair array")
        .iter()
        .map(|entry| (str_at(entry, key).to_string(), u64_at(entry, "count")))
        .collect()
}

fn csv_rows(path: &Path) -> Vec<BTreeMap<String, String>> {
    let mut reader = csv::Reader::from_path(path).expect("csv opens");
    reader
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("csv parses")
}

fn jsonl_values(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("jsonl reads")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl value parses"))
        .collect()
}

fn json_fixture(relative: &str) -> Value {
    json_file(&fixture(relative))
}

fn json_file(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn unique_count(rows: &[BTreeMap<String, String>], field: &str) -> u64 {
    rows.iter()
        .map(|row| row.get(field).expect("field").as_str())
        .collect::<BTreeSet<_>>()
        .len() as u64
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

fn str_at<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing {key}"))
}

fn u64_at(value: &Value, key: &str) -> u64 {
    value[key]
        .as_u64()
        .unwrap_or_else(|| panic!("missing {key}"))
}

fn parse_u64(row: &BTreeMap<String, String>, key: &str) -> u64 {
    row[key]
        .parse::<u64>()
        .unwrap_or_else(|error| panic!("{key} should be u64: {error}"))
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn write_cmbs_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.26","description":"CMBS e2e test registry","updated":"2026-06-26","entry_count":8}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_string_pretty(&json!([
            {"input":"Sears","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"SEARS LLC","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"Sears Roebuck & Co.","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 Hour Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HOUR FITNESS USA, INC.","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HR Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 Sand Island Prop","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 SAND ISLAND PROPERTY LLC","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"}
        ]))
        .expect("aliases serialize"),
    )
    .expect("aliases");
}

trait EntityRunSummaryJson {
    fn summary_as_json(&self) -> Value;
    fn next_commands_as_json(&self) -> Value;
}

impl EntityRunSummaryJson for canon::entity::run::EntityRunArtifact {
    fn summary_as_json(&self) -> Value {
        serde_json::to_value(&self.summary).expect("summary json")
    }

    fn next_commands_as_json(&self) -> Value {
        serde_json::to_value(&self.next_commands).expect("next commands json")
    }
}
