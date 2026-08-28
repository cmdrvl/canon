#![forbid(unsafe_code)]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::tempdir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn write_sources(root: &Path) -> [String; 3] {
    let parcel = root.join("parcel.csv");
    let property = root.join("property.csv");
    let footprint = root.join("footprint.csv");
    fs::write(
        &parcel,
        "source_row_id,anchor_id,canonical_id,name\np-1,shared,entity:a,Parcel\n",
    )
    .expect("write parcel source");
    fs::write(
        &property,
        "source_row_id,anchor_id,canonical_id,name\nq-1,shared,entity:a,Property\n",
    )
    .expect("write property source");
    fs::write(
        &footprint,
        "source_row_id,anchor_id,canonical_id,name\nf-1,shared,entity:b,Footprint\n",
    )
    .expect("write footprint source");
    [
        parcel.to_string_lossy().to_string(),
        property.to_string_lossy().to_string(),
        footprint.to_string_lossy().to_string(),
    ]
}

fn request(paths: &[String; 3]) -> Value {
    json!({
        "version": "canon_geo_multisource_request.v0",
        "sources": [
            {
                "name": "parcel",
                "role": "reference",
                "rows_path": paths[0],
                "local_id_column": "source_row_id",
                "anchor_namespace": "fixture",
                "anchor_column": "anchor_id",
                "canonical_id_column": "canonical_id"
            },
            {
                "name": "property",
                "role": "target",
                "rows_path": paths[1],
                "local_id_column": "source_row_id",
                "anchor_namespace": "fixture",
                "anchor_column": "anchor_id",
                "canonical_id_column": "canonical_id"
            },
            {
                "name": "footprint",
                "role": "peer",
                "rows_path": paths[2],
                "local_id_column": "source_row_id",
                "anchor_namespace": "fixture",
                "anchor_column": "anchor_id",
                "canonical_id_column": "canonical_id"
            }
        ],
        "comparison_graph": [],
        "default_pair_budget": 8
    })
}

fn write_request(root: &Path, value: &Value) -> std::path::PathBuf {
    let path = root.join("request.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(value).expect("serialize request"),
    )
    .expect("write request");
    path
}

fn run_link(request: &Path, rows_out: &Path) -> std::process::Output {
    canon_command()
        .args([
            "geo",
            "link-sources",
            "--request",
            request.to_str().unwrap(),
            "--rows-out",
            rows_out.to_str().unwrap(),
        ])
        .output()
        .expect("run canon geo link-sources")
}

#[test]
fn geo_link_sources_materializes_one_globally_budgeted_artifact() {
    let temp = tempdir().expect("tempdir");
    let paths = write_sources(temp.path());
    let request_path = write_request(temp.path(), &request(&paths));
    let rows_out = temp.path().join("rows.csv");

    let output = run_link(&request_path, &rows_out);
    assert_eq!(output.status.code(), Some(0), "stderr={:?}", output.stderr);
    assert!(output.stderr.is_empty());
    let artifact: Value = serde_json::from_slice(&output.stdout).expect("artifact JSON");

    assert_eq!(artifact["version"], "canon_entity_multisource_link.v1");
    assert_eq!(artifact["source_count"], 3);
    assert_eq!(artifact["row_count"], 3);
    assert!(artifact["canonical_source"].is_null());
    assert_eq!(artifact["comparison_graph"].as_array().unwrap().len(), 3);
    assert!(
        artifact["artifact_content_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert!(
        artifact["materialized_rows_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert_eq!(
        artifact["consistency"]["anchor_conflicts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        artifact["consistency"]["abstentions"][0]["reason"],
        "anchor_conflict"
    );

    let rows = fs::read_to_string(rows_out).expect("materialized rows");
    assert!(rows.contains(canon::entity::run::link::LINK_SOURCE_NAME_COLUMN));
    assert!(rows.contains("footprint"));
    assert!(rows.contains("parcel"));
    assert!(rows.contains("property"));
}

#[test]
fn geo_link_sources_semantic_hash_is_path_and_enumeration_independent() {
    let first = tempdir().expect("first tempdir");
    let second = tempdir().expect("second tempdir");
    let first_paths = write_sources(first.path());
    let second_paths = write_sources(second.path());
    let first_request = write_request(first.path(), &request(&first_paths));
    let mut permuted = request(&second_paths);
    permuted["sources"].as_array_mut().unwrap().reverse();
    let second_request = write_request(second.path(), &permuted);

    let first_output = run_link(&first_request, &first.path().join("a.csv"));
    let second_output = run_link(&second_request, &second.path().join("b.csv"));
    assert!(first_output.status.success());
    assert!(second_output.status.success());
    let first_artifact: Value = serde_json::from_slice(&first_output.stdout).unwrap();
    let second_artifact: Value = serde_json::from_slice(&second_output.stdout).unwrap();

    assert_eq!(
        first_artifact["artifact_content_hash"], second_artifact["artifact_content_hash"],
        "semantic identity must derive from content, roles, and budgets rather than paths"
    );
    assert_eq!(
        first_artifact["materialized_rows_hash"],
        second_artifact["materialized_rows_hash"]
    );
    assert_eq!(
        fs::read(first.path().join("a.csv")).unwrap(),
        fs::read(second.path().join("b.csv")).unwrap()
    );
}

#[test]
fn geo_link_sources_refuses_canonical_vendor_and_hot_pair_before_write() {
    let temp = tempdir().expect("tempdir");
    let paths = write_sources(temp.path());
    let mut bad_role = request(&paths);
    bad_role["sources"][2]["role"] = json!("canonical_reference");
    let bad_role_path = write_request(temp.path(), &bad_role);
    let role_rows = temp.path().join("role-rows.csv");
    let role_output = run_link(&bad_role_path, &role_rows);
    assert_eq!(role_output.status.code(), Some(2));
    assert!(!role_rows.exists());
    let role_refusal: Value = serde_json::from_slice(&role_output.stdout).unwrap();
    assert_eq!(role_refusal["refusal"]["code"], "E_ENTITY_INPUT_CONTRACT");
    assert_eq!(
        role_refusal["refusal"]["detail"]["reason"],
        "canonical_reference_forbidden"
    );

    let mut over_budget = request(&paths);
    over_budget["comparison_graph"] = json!([{
        "left_source": "parcel",
        "right_source": "property",
        "max_candidate_rows": 0
    }]);
    let budget_path = temp.path().join("budget-request.json");
    fs::write(&budget_path, serde_json::to_vec(&over_budget).unwrap()).unwrap();
    let budget_rows = temp.path().join("budget-rows.csv");
    let budget_output = run_link(&budget_path, &budget_rows);
    assert_eq!(budget_output.status.code(), Some(2));
    assert!(!budget_rows.exists());
    let budget_refusal: Value = serde_json::from_slice(&budget_output.stdout).unwrap();
    assert_eq!(
        budget_refusal["refusal"]["code"],
        "E_ENTITY_CANDIDATE_BUDGET"
    );
    assert_eq!(
        budget_refusal["refusal"]["detail"]["reason"],
        "zero_pair_budget"
    );
}

#[test]
fn geo_link_sources_never_replaces_a_declared_input() {
    let temp = tempdir().expect("tempdir");
    let paths = write_sources(temp.path());
    let request_path = write_request(temp.path(), &request(&paths));
    let parcel_path = Path::new(&paths[0]);
    let before = fs::read(parcel_path).expect("read source before refusal");

    let output = run_link(&request_path, parcel_path);
    assert_eq!(output.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&output.stdout).expect("refusal JSON");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_INPUT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["reason"],
        "input_output_overlap"
    );
    assert_eq!(
        fs::read(parcel_path).expect("read source after refusal"),
        before,
        "an output collision must not mutate the source"
    );
}
