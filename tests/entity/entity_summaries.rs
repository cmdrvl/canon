#![forbid(unsafe_code)]

use canon::entity::{
    run::{EntityRunRequest, run_entity_workbench},
    summary::{
        EntityRunOperatorSummaryRequest, EntitySummaryRankedItem, build_run_operator_summary,
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

    assert_eq!(projection(&summary), expected_projection());
    assert!(summary.human_summary.contains("profile=cmbs_tenant_label"));
    assert!(summary.human_summary.contains("deals=12"));
    assert!(summary.human_summary.contains("raw_unique=15"));
    assert!(summary.human_summary.contains("cache=index:rebuilt"));
    assert!(
        summary
            .human_summary
            .contains("top_unresolved=buffet,china,sears")
    );
    assert!(summary.human_summary.contains(
        "top_anti_merge=related_brand_family_not_same_tenant_label,successor_or_operator_not_display_label"
    ));
    assert!(
        summary
            .human_summary
            .contains("next=[apply,audit,promote,review_export]")
    );
    assert!(!summary.human_summary.contains("cmbs-small:001"));
    assert!(summary.human_summary.len() < 520);
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
            "operator_review_groups": summary.counts["operator_review_groups"],
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
    _temp: tempfile::TempDir,
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
        _temp: temp,
        artifact: result.artifact,
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

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
