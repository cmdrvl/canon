#![forbid(unsafe_code)]

use canon::entity::{
    apply::{
        ApplyCanonicalResolution, ApplyRegistryReference, ApplySafetyCheck, ApplyStreamRequest,
        run_apply_streaming,
    },
    block_artifact::BlockCandidateArtifact,
    edge_artifact::EdgeEvidenceArtifact,
    index::EntityIndexArtifact,
    prepare::PrepareRunArtifact,
    run::{EntityRunRequest, run_entity_workbench},
    solve::SolveArtifact,
    summary::{
        EntityRunOperatorSummaryRequest, EntitySummaryRankedItem, build_apply_operator_summary,
        build_block_operator_summary, build_edge_operator_summary, build_index_operator_summary,
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
const EXPECTED_PROJECTION: &str =
    include_str!("../fixtures/entity/summaries/cmbs_operator_summary_projection.json");

#[test]
fn entity_summaries_render_stable_robot_json_and_concise_human_line() {
    let run = run_fixture();
    let summary = build_run_operator_summary(EntityRunOperatorSummaryRequest {
        artifact: &run.artifact,
        extra_counts: BTreeMap::from([
            ("deal_count".to_string(), 12),
            ("raw_unique_names".to_string(), 15),
            ("promotable_aliases".to_string(), 4),
            ("review_groups".to_string(), 2),
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

    assert_eq!(projection(&summary), expected_projection());
    assert!(summary.human_summary.contains("profile=cmbs_tenant_label"));
    assert!(summary.human_summary.contains("deals=12"));
    assert!(summary.human_summary.contains("raw_unique=15"));
    assert!(summary.human_summary.contains("promotable=4"));
    assert!(summary.human_summary.contains("review_groups=2"));
    assert!(summary.human_summary.contains("cache=index:rebuilt"));
    assert!(
        summary
            .human_summary
            .contains("top_unresolved=buffet:2,china:2,sears:2")
    );
    assert!(summary.human_summary.contains(
        "top_anti_merge=related_brand_family_not_same_tenant_label:3,successor_or_operator_not_display_label:1"
    ));
    assert!(
        summary
            .human_summary
            .contains("next=[apply,audit,promote,review_export]")
    );
    assert!(!summary.human_summary.contains("cmbs-small:001"));
    assert!(summary.human_summary.len() < 520);
    assert_stage_summaries(&run);
}

fn projection(summary: &canon::entity::summary::EntityRunOperatorSummary) -> Value {
    json!({
        "version": summary.version,
        "profile_id": summary.profile_id,
        "registry_id": summary.registry.id,
        "registry_version": summary.registry.version,
        "counts": {
            "row_count": summary.counts["row_count"],
            "deal_count": summary.counts["deal_count"],
            "raw_unique_names": summary.counts["raw_unique_names"],
            "prepared_surfaces": summary.counts["prepared_surfaces"],
            "exact_resolved_surfaces": summary.counts["exact_resolved_surfaces"],
            "promotable_aliases": summary.counts["promotable_aliases"],
            "review_groups": summary.counts["review_groups"],
            "anti_merge_groups": summary.counts["anti_merge_groups"],
        },
        "cache_status": summary.cache_status,
        "top_unresolved_tokens": summary.top_unresolved_tokens,
        "top_anti_merge_reasons": summary.top_anti_merge_reasons,
        "next_command_keys": summary
            .next_commands
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
    })
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

fn assert_stage_summaries(run: &RunFixture) {
    let prepare: PrepareRunArtifact = read_json(&run.work_dir.join("prepare/prepare.json"));
    let index: EntityIndexArtifact = read_json(&run.work_dir.join("index/index.json"));
    let block: BlockCandidateArtifact = read_json(&run.work_dir.join("block/block.json"));
    let edge: EdgeEvidenceArtifact = read_json(&run.work_dir.join("evidence/evidence.json"));
    let solve: SolveArtifact = read_json(&run.work_dir.join("solve/solve.json"));

    let prepare_summary = build_prepare_operator_summary(&prepare);
    assert_eq!(prepare_summary.stage, "prepare");
    assert_eq!(prepare_summary.counts, prepare.summary);
    assert!(
        prepare_summary
            .human_summary
            .contains("profile=cmbs_tenant_label")
    );
    assert!(!prepare_summary.human_summary.contains("cmbs-small:001"));

    let index_summary = build_index_operator_summary(&index);
    assert_eq!(index_summary.stage, "index");
    assert_eq!(index_summary.counts, index.summary.counts);
    assert_eq!(index_summary.cache_status["index"], "rebuilt");
    assert!(index_summary.human_summary.contains("cache=index:rebuilt"));

    let block_summary = build_block_operator_summary(&block);
    assert_eq!(block_summary.stage, "block");
    assert_eq!(block_summary.counts, block.summary.counts);
    assert!(block_summary.counts.contains_key("candidate_pairs"));

    let edge_summary = build_edge_operator_summary(&edge);
    assert_eq!(edge_summary.stage, "evidence");
    assert_eq!(edge_summary.counts, edge.summary.counts);
    assert!(edge_summary.counts.contains_key("evidence_records"));

    let solve_summary = build_solve_operator_summary(&solve);
    assert_eq!(solve_summary.stage, "solve");
    assert_eq!(solve_summary.counts, solve.summary.counts);
    assert!(solve_summary.counts.contains_key("entity_count"));

    let rows = repo_path(OBSERVATIONS_PATH);
    let output = run.temp.path().join("apply.csv");
    let resolutions = apply_resolutions();
    let apply = run_apply_streaming(apply_request(&rows, &output, &resolutions))
        .expect("apply replay succeeds");
    let apply_summary = build_apply_operator_summary(&apply);
    assert_eq!(apply_summary.stage, "apply");
    assert_eq!(apply_summary.counts["rows"], 15);
    assert_eq!(apply_summary.counts["resolved"], 8);
    assert_eq!(apply_summary.counts["unresolved"], 7);
    assert!(
        apply_summary
            .human_summary
            .contains("registry=cmbs-tenants")
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
            expected_registry_snapshot_hash: Some("blake3:cmbs-summary-registry".to_string()),
            actual_registry_snapshot_hash: Some("blake3:cmbs-summary-registry".to_string()),
            expected_sidecar_artifact_version: Some(
                "canon_entity_promotion_sidecar.v0".to_string(),
            ),
            actual_sidecar_artifact_version: Some("canon_entity_promotion_sidecar.v0".to_string()),
            expected_sidecar_snapshot_hash: Some("blake3:cmbs-summary-sidecars".to_string()),
            actual_sidecar_snapshot_hash: Some("blake3:cmbs-summary-sidecars".to_string()),
        },
        require_full_resolution: false,
        target_rows_per_chunk: 5,
    }
}

fn apply_resolutions() -> BTreeMap<String, ApplyCanonicalResolution> {
    BTreeMap::from([
        ("Sears".to_string(), resolution("TNT-SEARS")),
        ("SEARS LLC".to_string(), resolution("TNT-SEARS")),
        ("Sears Roebuck & Co.".to_string(), resolution("TNT-SEARS")),
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
        r#"{"id":"cmbs-tenants","version":"2026.06.26","description":"CMBS summary registry","updated":"2026-06-26","entry_count":8}"#,
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

fn expected_projection() -> Value {
    serde_json::from_str(EXPECTED_PROJECTION).expect("expected projection parses")
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
