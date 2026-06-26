#![forbid(unsafe_code)]

use canon::entity::{
    apply::{
        ApplyCanonicalResolution, ApplyRegistryReference, ApplySafetyCheck, ApplyStreamRequest,
        run_apply_streaming,
    },
    block_artifact::BlockCandidateArtifact,
    prepare::PrepareRunArtifact,
    run::{EntityRunRequest, run_entity_workbench},
    solve::SolveArtifact,
    summary::{
        EntityRunOperatorSummary, EntityRunOperatorSummaryRequest, EntityStageOperatorSummary,
        EntitySummaryRankedItem, build_apply_operator_summary, build_block_operator_summary,
        build_prepare_operator_summary, build_run_operator_summary, build_solve_operator_summary,
    },
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const OBSERVATIONS_PATH: &str = "tests/fixtures/entity/cmbs/small_book/observations.csv";
const STRATEGY_PATH: &str = "tests/fixtures/entity/profiles/cmbs_tenant_label.yaml";
const EXPECTED_ROBOT_JSON: &str =
    include_str!("../fixtures/entity/operator_journey/summary/robot_summary_projection.json");

#[test]
fn entity_robot_json_summaries_are_stable_and_actionable() {
    let fixture = run_fixture();
    let run_summary = build_run_operator_summary(EntityRunOperatorSummaryRequest {
        artifact: &fixture.artifact,
        extra_counts: BTreeMap::from([
            ("deal_count".to_string(), 12),
            ("raw_unique_names".to_string(), 15),
            ("operator_review_groups".to_string(), 2),
            ("anti_merge_groups".to_string(), 2),
        ]),
        cache_status: BTreeMap::from([("index".to_string(), "rebuilt".to_string())]),
        top_unresolved_tokens: vec![
            EntitySummaryRankedItem::new("sears", 2),
            EntitySummaryRankedItem::new("china", 2),
            EntitySummaryRankedItem::new("buffet", 2),
        ],
        top_anti_merge_reasons: vec![
            EntitySummaryRankedItem::new("successor_or_operator_not_display_label", 1),
            EntitySummaryRankedItem::new("related_brand_family_not_same_tenant_label", 3),
        ],
    });
    let prepare = build_prepare_operator_summary(&read_json::<PrepareRunArtifact>(
        &fixture.work_dir.join("prepare/prepare.json"),
    ));
    let block = build_block_operator_summary(&read_json::<BlockCandidateArtifact>(
        &fixture.work_dir.join("block/block.json"),
    ));
    let solve = build_solve_operator_summary(&read_json::<SolveArtifact>(
        &fixture.work_dir.join("solve/solve.json"),
    ));
    let apply = build_apply_operator_summary(&apply_artifact(&fixture));

    assert_eq!(
        robot_projection(&run_summary, [&prepare, &block, &solve, &apply]),
        expected_robot_json()
    );
    assert!(run_summary.next_command.contains("canon entity review export"));
    for stage in [&prepare, &block, &solve, &apply] {
        assert_eq!(stage.version, "canon_entity_operator_summary.v0");
        assert!(
            !stage.human_summary.contains("cmbs-small:001"),
            "{} summary leaked row-level source ids",
            stage.stage
        );
        assert!(
            stage.human_summary.contains("telemetry="),
            "{} summary omits telemetry label",
            stage.stage
        );
    }
}

fn robot_projection<'a>(
    run: &EntityRunOperatorSummary,
    stages: impl IntoIterator<Item = &'a EntityStageOperatorSummary>,
) -> Value {
    json!({
        "version": "canon.entity.operator_journey.summary_robot.v0",
        "run": {
            "summary_version": run.version,
            "profile_id": run.profile_id,
            "registry_id": run.registry.id,
            "registry_version": run.registry.version,
            "counts": {
                "row_count": run.counts["row_count"],
                "raw_unique_names": run.counts["raw_unique_names"],
                "prepared_surfaces": run.counts["prepared_surfaces"],
                "exact_resolved_surfaces": run.counts["exact_resolved_surfaces"],
                "operator_review_groups": run.counts["operator_review_groups"],
                "anti_merge_groups": run.counts["anti_merge_groups"]
            },
            "cache_status": run.cache_status,
            "next_command_key": next_command_key(run.next_command.as_str(), &run.next_commands),
            "next_command_keys": string_keys(&run.next_commands),
            "telemetry_link_keys": string_keys(&run.telemetry_links),
            "top_unresolved_tokens": ranked_keys(&run.top_unresolved_tokens),
            "top_anti_merge_reasons": ranked_keys(&run.top_anti_merge_reasons)
        },
        "stages": stages
            .into_iter()
            .map(stage_projection)
            .collect::<Vec<_>>()
    })
}

fn stage_projection(stage: &EntityStageOperatorSummary) -> Value {
    json!({
        "stage": stage.stage,
        "summary_version": stage.version,
        "artifact_version": stage.artifact_version,
        "next_command_key": next_command_key(stage.next_command.as_str(), &stage.next_commands),
        "telemetry_link_keys": string_keys(&stage.telemetry_links),
        "counts": stage_count_projection(stage)
    })
}

fn stage_count_projection(stage: &EntityStageOperatorSummary) -> BTreeMap<String, u64> {
    let keys = match stage.stage.as_str() {
        "prepare" => ["prepared_observations", "prepared_surfaces", "exact_resolved_surfaces"],
        "block" => ["candidate_pairs", "exact_bucket_count", "exact_bucket_pair_expansion_count"],
        "solve" => ["entity_count", "review_group_count", "promotable_new_count"],
        "apply" => ["rows", "resolved", "unresolved"],
        _ => ["", "", ""],
    };
    keys.into_iter()
        .filter_map(|key| stage.counts.get(key).map(|value| (key.to_string(), *value)))
        .collect()
}

fn next_command_key(command: &str, commands: &BTreeMap<String, String>) -> String {
    commands
        .iter()
        .find(|(_, candidate)| candidate.as_str() == command)
        .map(|(key, _)| key.clone())
        .unwrap_or_else(|| {
            if command.trim().is_empty() {
                "none".to_string()
            } else {
                "custom".to_string()
            }
        })
}

fn string_keys(values: &BTreeMap<String, String>) -> Vec<String> {
    values.keys().cloned().collect::<Vec<_>>()
}

fn ranked_keys(items: &[EntitySummaryRankedItem]) -> Vec<String> {
    items.iter().map(|item| item.key.clone()).collect()
}

struct RunFixture {
    temp: tempfile::TempDir,
    work_dir: PathBuf,
    artifact: canon::entity::run::EntityRunArtifact,
}

fn run_fixture() -> RunFixture {
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
    .expect("run succeeds");
    RunFixture {
        temp,
        work_dir,
        artifact: result.artifact,
    }
}

fn apply_artifact(fixture: &RunFixture) -> canon::entity::apply::ApplyRunArtifact {
    let rows = repo_path(OBSERVATIONS_PATH);
    let output = fixture.temp.path().join("apply.csv");
    let resolutions = apply_resolutions();
    run_apply_streaming(apply_request(&rows, &output, &resolutions)).expect("apply succeeds")
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
            expected_registry_snapshot_hash: Some("blake3:cmbs-robot-registry".to_string()),
            actual_registry_snapshot_hash: Some("blake3:cmbs-robot-registry".to_string()),
            expected_sidecar_artifact_version: Some(
                "canon_entity_promotion_sidecar.v0".to_string(),
            ),
            actual_sidecar_artifact_version: Some("canon_entity_promotion_sidecar.v0".to_string()),
            expected_sidecar_snapshot_hash: Some("blake3:cmbs-robot-sidecars".to_string()),
            actual_sidecar_snapshot_hash: Some("blake3:cmbs-robot-sidecars".to_string()),
        },
        require_full_resolution: false,
        target_rows_per_chunk: 5,
    }
}

fn apply_resolutions() -> BTreeMap<String, ApplyCanonicalResolution> {
    BTreeMap::from([
        ("Sears".to_string(), resolution("TNT-SEARS")),
        ("SEARS LLC".to_string(), resolution("TNT-SEARS")),
        (
            "Sears Roebuck & Co.".to_string(),
            resolution("TNT-SEARS"),
        ),
        (
            "24 Hour Fitness".to_string(),
            resolution("TNT-24-HOUR-FITNESS"),
        ),
        (
            "24 HOUR FITNESS USA, INC.".to_string(),
            resolution("TNT-24-HOUR-FITNESS"),
        ),
        (
            "24 HR Fitness".to_string(),
            resolution("TNT-24-HOUR-FITNESS"),
        ),
        (
            "238 Sand Island Prop".to_string(),
            resolution("TNT-238-SAND-ISLAND-PROPERTY"),
        ),
        (
            "238 SAND ISLAND PROPERTY LLC".to_string(),
            resolution("TNT-238-SAND-ISLAND-PROPERTY"),
        ),
    ])
}

fn resolution(canonical_id: &str) -> ApplyCanonicalResolution {
    ApplyCanonicalResolution {
        canonical_id: canonical_id.to_string(),
        canonical_type: "tenant_label".to_string(),
        rule_id: "CMBS_ALIAS".to_string(),
    }
}

fn write_cmbs_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.26","description":"CMBS robot summary registry","updated":"2026-06-26","entry_count":8}"#,
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

fn expected_robot_json() -> Value {
    serde_json::from_str(EXPECTED_ROBOT_JSON).expect("expected robot json parses")
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
