use canon::entity::prepare::{
    PrepareInputContract, PrepareRunRequest, project_prepare_path, run_prepare, stream_prepare_path,
};
use canon::entity::profile::EntityProfileDocument;
use canon::entity::stream::{EntityStreamFormat, EntityStreamStage};
use std::fs;

const CMBS_PROFILE: &str = include_str!("../fixtures/entity/profiles/cmbs_tenant_label.yaml");

#[test]
fn prepare_streaming_equivalence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    write_rows(&rows, 5);
    let contract = cmbs_contract();

    let non_streaming = project_prepare_path(&rows, &contract).expect("non-streaming prepare");
    let streaming = stream_prepare_path(&rows, &contract, 2).expect("streaming prepare");

    assert_eq!(streaming.observations, non_streaming);
    assert_eq!(
        streaming.diagnostics.input.stage,
        EntityStreamStage::Prepare
    );
    assert_eq!(streaming.diagnostics.input.format, EntityStreamFormat::Csv);
    assert_eq!(streaming.diagnostics.input.row_count, 5);
    assert_eq!(streaming.diagnostics.chunks.len(), 3);
    assert_eq!(streaming.diagnostics.chunks[0].first_row_ordinal, 0);
    assert_eq!(streaming.diagnostics.chunks[1].first_row_ordinal, 2);
    assert_eq!(streaming.diagnostics.chunks[2].first_row_ordinal, 4);
    assert_eq!(streaming.diagnostics.telemetry.rows_seen, 5);
    assert_eq!(streaming.diagnostics.telemetry.chunk_count, 3);
    assert_eq!(
        streaming.diagnostics.provenance_samples[3]
            .source_row_id
            .as_deref(),
        Some("row-4")
    );
    assert_eq!(streaming.diagnostics.provenance_samples[3].chunk_index, 1);
}

#[test]
fn prepare_artifact_records_streaming_diagnostics_without_surface_drift() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    write_rows(&rows, 3);
    fs::create_dir_all(&registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.25","description":"Prepare streaming test registry","updated":"2026-06-25","entry_count":0}"#,
    )
    .expect("registry metadata");

    let artifact = run_prepare(PrepareRunRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("prepare run");

    assert_eq!(artifact.input.row_count, 3);
    assert_eq!(artifact.summary["prepared_observations"], 3);
    assert_eq!(artifact.streaming.input.row_count, 3);
    assert_eq!(artifact.streaming.telemetry.rows_seen, 3);
    assert_eq!(artifact.streaming.telemetry.max_chunk_rows, 3);
    assert_eq!(
        artifact.streaming.provenance_samples[0]
            .source_row_id
            .as_deref(),
        Some("row-1")
    );
    assert_eq!(artifact.surfaces_path, "prepare/surfaces.jsonl");
    assert!(work_dir.join("prepare").join("surfaces.jsonl").exists());
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
}

fn cmbs_contract() -> PrepareInputContract {
    let profile = EntityProfileDocument::from_yaml_str(CMBS_PROFILE).expect("valid profile");
    PrepareInputContract::for_builtin_profile(&profile).expect("prepare contract")
}

fn write_rows(path: &std::path::Path, count: usize) {
    let mut rows = String::from(
        "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\n",
    );
    for index in 1..=count {
        rows.push_str(&format!(
            "row-{index},deal-{index},loan-{index},prop-{index},Sears {index},\"[\"\"Sears\"\"]\",[]\n"
        ));
    }
    fs::write(path, rows).expect("rows");
}
