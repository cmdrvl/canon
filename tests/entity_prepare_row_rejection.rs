#![forbid(unsafe_code)]

use canon::entity::{
    prepare::{PrepareRunRequest, run_prepare},
    run::{EntityRunRequest, render_run_summary, run_entity_workbench},
};
use serde_json::{Value, json};
use std::{fs, path::Path};

const REGISTRY: &str = "tests/fixtures/registries/empty";
const PROFILE: &str = "cmbs_tenant_label";
const STRATEGY: &str = "tests/fixtures/entity/profiles/cmbs_tenant_label.yaml";

#[test]
fn prepare_csv_rejects_one_unnormalisable_row_and_keeps_other_surface_ids_stable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let clean_rows = temp.path().join("clean.csv");
    let rejected_rows = temp.path().join("rejected.csv");
    write_cmbs_csv(&clean_rows, false);
    write_cmbs_csv(&rejected_rows, true);

    let clean_work = temp.path().join("clean-work");
    let rejected_work = temp.path().join("rejected-work");
    let clean = run_prepare(PrepareRunRequest {
        rows: &clean_rows,
        profile: PROFILE,
        registry: Path::new(REGISTRY),
        work_dir: &clean_work,
    })
    .expect("clean prepare");
    let rejected = run_prepare(PrepareRunRequest {
        rows: &rejected_rows,
        profile: PROFILE,
        registry: Path::new(REGISTRY),
        work_dir: &rejected_work,
    })
    .expect("row rejection does not refuse prepare");

    assert_eq!(rejected.input.row_count, 3);
    assert_eq!(rejected.summary["prepared_observations"], 2);
    assert_eq!(rejected.summary["rejected_row_count"], 1);
    assert_eq!(rejected.rejected_rows.len(), 1);
    assert_eq!(rejected.rejected_rows[0].row_number, 2);
    assert_eq!(
        rejected.rejected_rows[0].source_row_id.as_deref(),
        Some("bad")
    );
    assert_eq!(rejected.rejected_rows[0].code, "E_ENTITY_ARTIFACT_CONTRACT");
    assert!(
        rejected.rejected_rows[0]
            .message
            .contains("canonical normalized view")
    );

    assert_eq!(
        clean.summary["prepared_surfaces"],
        rejected.summary["prepared_surfaces"]
    );
    assert_eq!(
        fs::read_to_string(clean_work.join(&clean.surfaces_path)).expect("clean surfaces"),
        fs::read_to_string(rejected_work.join(&rejected.surfaces_path)).expect("rejected surfaces")
    );
    let artifact: Value = read_json(&rejected_work.join("prepare/prepare.json"));
    assert_eq!(artifact["rejected_rows"], json!(rejected.rejected_rows));
}

#[test]
fn prepare_jsonl_rejects_malformed_line_and_continues() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.jsonl");
    fs::write(
        &rows,
        format!(
            "{}\nnot-json\n{}\n",
            serde_json::to_string(&json!({
                "source_row_id": "row-1",
                "deal_id": "deal-1",
                "loan_id": "loan-1",
                "property_id": "property-1",
                "raw_tenant_name": "Sears"
            }))
            .expect("row 1"),
            serde_json::to_string(&json!({
                "source_row_id": "row-3",
                "deal_id": "deal-3",
                "loan_id": "loan-3",
                "property_id": "property-3",
                "raw_tenant_name": "Macy"
            }))
            .expect("row 3")
        ),
    )
    .expect("jsonl rows");

    let work_dir = temp.path().join("work");
    let artifact = run_prepare(PrepareRunRequest {
        rows: &rows,
        profile: PROFILE,
        registry: Path::new(REGISTRY),
        work_dir: &work_dir,
    })
    .expect("malformed JSONL row is reported, not fatal");

    assert_eq!(artifact.input.row_count, 3);
    assert_eq!(artifact.summary["prepared_observations"], 2);
    assert_eq!(artifact.summary["rejected_row_count"], 1);
    assert_eq!(artifact.rejected_rows[0].row_number, 2);
    assert_eq!(artifact.rejected_rows[0].source_row_id, None);
    assert_eq!(artifact.rejected_rows[0].code, "E_ENTITY_INPUT_CONTRACT");
    assert!(work_dir.join("prepare/prepare.json").exists());
    assert!(work_dir.join("prepare/surfaces.jsonl").exists());
}

#[test]
fn prepare_missing_mapped_column_remains_a_whole_run_refusal() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("missing-column.csv");
    fs::write(
        &rows,
        "source_row_id,deal_id,loan_id,property_id\nrow-1,deal-1,loan-1,property-1\n",
    )
    .expect("missing-column rows");
    let work_dir = temp.path().join("work");

    let refusal = run_prepare(PrepareRunRequest {
        rows: &rows,
        profile: PROFILE,
        registry: Path::new(REGISTRY),
        work_dir: &work_dir,
    })
    .expect_err("missing mapped column refuses the whole run");

    assert_eq!(
        serde_json::to_value(refusal.code).expect("refusal code"),
        "E_ENTITY_INPUT_CONTRACT"
    );
    assert_eq!(refusal.detail["field"], "raw_tenant_name");
    assert!(!work_dir.join("prepare/prepare.json").exists());
}

#[test]
fn entity_run_carries_rejected_row_count_and_summary_guidance() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("run.csv");
    let fixture = Path::new("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let mut contents = fs::read_to_string(fixture).expect("CMBS fixture");
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents.push_str("bad,deal-bad,loan-bad,property-bad,`,[],[]\n");
    fs::write(&rows, contents).expect("run rows");

    let work_dir = temp.path().join("run-work");
    let result = run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: PROFILE,
        strategy: Path::new(STRATEGY),
        registry: Path::new(REGISTRY),
        work_dir: &work_dir,
    })
    .expect("entity run continues after one rejected row");

    assert_eq!(result.artifact.summary.counts["rejected_row_count"], 1);
    assert!(render_run_summary(&result.artifact).contains("rejected_rows=1"));
    let prepare: Value = read_json(&work_dir.join("prepare/prepare.json"));
    assert_eq!(prepare["summary"]["rejected_row_count"], 1);
    assert_eq!(prepare["rejected_rows"][0]["source_row_id"], "bad");
}

fn write_cmbs_csv(path: &Path, include_rejected: bool) {
    let mut contents = String::from(
        "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\n",
    );
    contents.push_str("row-1,deal-1,loan-1,property-1,Sears,[],[]\n");
    if include_rejected {
        contents.push_str("bad,deal-bad,loan-bad,property-bad,`,[],[]\n");
    }
    contents.push_str("row-3,deal-3,loan-3,property-3,Macy,[],[]\n");
    fs::write(path, contents).expect("csv rows");
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json artifact")
}
