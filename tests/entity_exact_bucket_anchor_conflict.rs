#![forbid(unsafe_code)]

use canon::entity::run::{EntityRunRequest, run_entity_workbench};
use serde_json::Value;
use std::{fs, path::Path};

const PROFILE: &str = r#"
profile: exact_bucket_anchor_conflict
version: 0.1.0
entity_type: organization
identity_semantics: same_normalized_name
canonical_type: org
required_fields: [source_row_id, name, cik]
normalized_views:
  core:
    operators: [lowercase, normalize_whitespace]
  row_key:
    operators: [ascii_trim_upper]
prepare:
  primary_surface_fields: [name]
  canonical_surface_normalized_view: core
  normalized_view_fields:
    core: name
    row_key: source_row_id
  anchor_fields:
    cik: cik
  surface_key_fields: [row_key]
  nullable_fields: [cik]
  provenance_fields: [source_row_id]
blocking:
  operators:
    - op: exact_identity
      view: core
evidence:
  support:
    - op: exact_view
      view: core
  cannot_link:
    - op: anchor_conflict
      params:
        field: cik
        score_units: "10000"
  relation_hints:
    - op: cross_profile_alignment
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
patch_namespaces:
  aliases: exact_bucket_anchor_conflict.aliases
  distinct: exact_bucket_anchor_conflict.distinct
  relations: exact_bucket_anchor_conflict.relations
block:
  preflight: false
solve:
  hard_cannot_link: contradiction
  standalone_surfaces: include
"#;

struct RunArtifacts {
    block: Value,
    evidence: Vec<Value>,
    solve: Value,
}

#[test]
fn exact_name_bucket_with_conflicting_anchor_emits_hard_cannot_link() {
    let artifacts = run_rows("source_row_id,name,cik\nr1,Apex LLC,333\nr2,Apex LLC,444\n");

    assert!(artifacts.evidence.iter().any(|record| {
        record["hits"].as_array().is_some_and(|hits| {
            hits.iter()
                .any(|hit| hit["lane"] == "anti_merge" && hit["reason_code"] == "anchor_conflict")
        })
    }));
    assert_eq!(
        artifacts.solve["summary"]["counts"]["diagnostics_hard_cannot_link_count"],
        1
    );
    let entity = artifacts.solve["entities"]
        .as_array()
        .expect("solve entities")
        .iter()
        .find(|entity| {
            entity["surface_ids"]
                .as_array()
                .is_some_and(|ids| ids.len() == 2)
        })
        .expect("the exact-name pair remains one contradiction component");
    assert_eq!(entity["state"], "contradiction");
    assert_eq!(entity["hard_cannot_link_count"], 1);
    assert!(
        artifacts.solve["promotable_aliases"]
            .as_array()
            .is_none_or(Vec::is_empty)
    );
}

#[test]
fn exact_name_bucket_with_matching_anchor_has_no_cannot_link() {
    let artifacts = run_rows("source_row_id,name,cik\nr1,Apex LLC,333\nr2,Apex LLC,333\n");

    assert!(artifacts.evidence.iter().all(|record| {
        record["hits"].as_array().is_none_or(|hits| {
            hits.iter()
                .all(|hit| hit["reason_code"] != "anchor_conflict")
        })
    }));
    assert_eq!(
        artifacts.solve["summary"]["counts"]["diagnostics_hard_cannot_link_count"],
        0
    );
    let entity = artifacts.solve["entities"]
        .as_array()
        .expect("solve entities")
        .iter()
        .find(|entity| {
            entity["surface_ids"]
                .as_array()
                .is_some_and(|ids| ids.len() == 2)
        })
        .expect("matching-anchor exact-name pair remains one component");
    assert_ne!(entity["state"], "contradiction");
}

#[test]
fn exact_name_bucket_with_one_missing_anchor_is_not_split() {
    let artifacts = run_rows("source_row_id,name,cik\nr1,Apex LLC,333\nr2,Apex LLC,\n");

    assert!(artifacts.evidence.iter().all(|record| {
        record["hits"].as_array().is_none_or(|hits| {
            hits.iter()
                .all(|hit| hit["reason_code"] != "anchor_conflict")
        })
    }));
    assert_eq!(
        artifacts.solve["summary"]["counts"]["diagnostics_hard_cannot_link_count"],
        0
    );
    let entity = artifacts.solve["entities"]
        .as_array()
        .expect("solve entities")
        .iter()
        .find(|entity| {
            entity["surface_ids"]
                .as_array()
                .is_some_and(|ids| ids.len() == 2)
        })
        .expect("missing-anchor exact-name pair remains one component");
    assert_ne!(entity["state"], "contradiction");
}

#[test]
fn large_exact_name_bucket_uses_bounded_anchor_conflict_candidates() {
    let mut rows = String::from("source_row_id,name,cik\n");
    for ordinal in 0..2_500 {
        rows.push_str(&format!("left-{ordinal},Apex LLC,333\n"));
    }
    for ordinal in 0..2_500 {
        rows.push_str(&format!("right-{ordinal},Apex LLC,444\n"));
    }

    let artifacts = run_rows(&rows);
    assert_eq!(
        artifacts.block["summary"]["counts"]["exact_bucket_count"],
        1
    );
    assert_eq!(artifacts.block["summary"]["counts"]["candidate_pairs"], 1);
    assert_eq!(
        artifacts.solve["summary"]["counts"]["diagnostics_hard_cannot_link_count"],
        1
    );
}

fn run_rows(rows: &str) -> RunArtifacts {
    let temp = tempfile::tempdir().expect("temporary run directory");
    let profile = temp.path().join("profile.yaml");
    let rows_path = temp.path().join("rows.csv");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");

    fs::create_dir_all(&registry).expect("registry directory");
    fs::write(&profile, PROFILE).expect("profile");
    fs::write(&rows_path, rows).expect("rows");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"exact-bucket-test","version":"1.0.0","description":"exact bucket anchor test","updated":"2026-11-15","entry_count":0}"#,
    )
    .expect("registry metadata");
    fs::write(registry.join("aliases.json"), "[]\n").expect("empty aliases");

    run_entity_workbench(EntityRunRequest {
        rows: &rows_path,
        profile: profile.to_str().expect("profile path"),
        strategy: Path::new(&profile),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("entity run succeeds");

    RunArtifacts {
        block: read_json(&work_dir.join("block/block.json")),
        evidence: read_jsonl(&work_dir.join("evidence/evidence.jsonl")),
        solve: read_json(&work_dir.join("solve/solve.json")),
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("JSON artifact")).expect("valid JSON")
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("JSONL artifact")
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid JSONL record"))
        .collect()
}
