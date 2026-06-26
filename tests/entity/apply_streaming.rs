use canon::entity::apply::{
    ApplyCanonicalResolution, ApplyRegistryReference, ApplyStreamRequest, run_apply_streaming,
};
use canon::entity::stream::{EntityStreamFormat, EntityStreamStage};
use std::collections::BTreeMap;
use std::fs;

#[test]
fn apply_streaming_exact_replay_preserves_raw_csv_bytes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    let output = temp.path().join("rows.canon.csv");
    fs::write(
        &rows,
        concat!(
            "source_row_id,raw_tenant_name,amount\r\n",
            "row-1,\"SEARS, LLC\",10\r\n",
            "row-2,Kmart,20\r\n",
            "row-3,Unknown,30\r\n",
        ),
    )
    .expect("rows");

    let artifact = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "raw_tenant_name",
        registry: registry(),
        resolutions: &resolutions(),
        target_rows_per_chunk: 2,
    })
    .expect("apply streaming");

    let applied = fs::read_to_string(&output).expect("applied output");
    assert_eq!(
        applied,
        concat!(
            "source_row_id,raw_tenant_name,amount,canonical_id,canonical_type,canonical_rule_id\r\n",
            "row-1,\"SEARS, LLC\",10,TNT-SEARS,tenant_label,REGISTRY_EXACT\r\n",
            "row-2,Kmart,20,TNT-KMART,tenant_label,REGISTRY_EXACT\r\n",
            "row-3,Unknown,30,,,\r\n",
        )
    );
    assert_eq!(artifact.version, "canon_entity_apply.v0");
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    assert_eq!(artifact.summary["rows"], 3);
    assert_eq!(artifact.summary["resolved"], 2);
    assert_eq!(artifact.summary["unresolved"], 1);
    assert_eq!(artifact.streaming.input.stage, EntityStreamStage::Apply);
    assert_eq!(artifact.streaming.input.format, EntityStreamFormat::Csv);
    assert_eq!(artifact.streaming.input.row_count, 3);
    assert_eq!(artifact.streaming.chunks.len(), 2);
    assert_eq!(artifact.streaming.telemetry.rows_seen, 3);
    assert_eq!(
        artifact.streaming.provenance_samples[1]
            .source_row_id
            .as_deref(),
        Some("Kmart")
    );
    assert_eq!(artifact.streaming.provenance_samples[2].chunk_index, 1);
}

#[test]
fn apply_batch_size_equivalence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    let output_one = temp.path().join("one.csv");
    let output_many = temp.path().join("many.csv");
    fs::write(
        &rows,
        concat!(
            "source_row_id,raw_tenant_name,amount\n",
            "row-1,Sears,10\n",
            "row-2,Kmart,20\n",
            "row-3,Sears,30\n",
            "row-4,Unknown,40\n",
        ),
    )
    .expect("rows");
    let resolutions = resolutions();

    let one = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output_one,
        lookup_column: "raw_tenant_name",
        registry: registry(),
        resolutions: &resolutions,
        target_rows_per_chunk: 1,
    })
    .expect("apply with one-row chunks");
    let many = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output_many,
        lookup_column: "raw_tenant_name",
        registry: registry(),
        resolutions: &resolutions,
        target_rows_per_chunk: 3,
    })
    .expect("apply with larger chunks");

    assert_eq!(
        fs::read(&output_one).expect("one output"),
        fs::read(&output_many).expect("many output")
    );
    assert_eq!(one.summary, many.summary);
    assert_ne!(one.streaming.chunks, many.streaming.chunks);
}

#[test]
fn apply_streaming_jsonl_appends_fields_without_rewriting_raw_object() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.jsonl");
    let output = temp.path().join("rows.canon.jsonl");
    fs::write(
        &rows,
        concat!(
            "{\"source_row_id\":\"row-1\",\"raw_tenant_name\":\"Sears\",\"amount\":10}\n",
            "{\"source_row_id\":\"row-2\",\"raw_tenant_name\":\"Unknown\",\"amount\":20}\n",
        ),
    )
    .expect("rows");
    let resolutions = resolutions();

    let artifact = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "raw_tenant_name",
        registry: registry(),
        resolutions: &resolutions,
        target_rows_per_chunk: 4,
    })
    .expect("apply jsonl");

    let applied = fs::read_to_string(&output).expect("applied output");
    assert_eq!(
        applied,
        concat!(
            "{\"source_row_id\":\"row-1\",\"raw_tenant_name\":\"Sears\",\"amount\":10,",
            "\"canonical_id\":\"TNT-SEARS\",\"canonical_type\":\"tenant_label\",",
            "\"canonical_rule_id\":\"REGISTRY_EXACT\"}\n",
            "{\"source_row_id\":\"row-2\",\"raw_tenant_name\":\"Unknown\",\"amount\":20,",
            "\"canonical_id\":null,\"canonical_type\":null,\"canonical_rule_id\":null}\n",
        )
    );
    assert_eq!(artifact.streaming.input.format, EntityStreamFormat::Jsonl);
    assert_eq!(artifact.summary["resolved"], 1);
    assert_eq!(artifact.summary["unresolved"], 1);
}

fn registry() -> ApplyRegistryReference {
    ApplyRegistryReference {
        id: "cmbs-tenants".to_string(),
        version: "2026.06.25".to_string(),
    }
}

fn resolutions() -> BTreeMap<String, ApplyCanonicalResolution> {
    BTreeMap::from([
        (
            "SEARS, LLC".to_string(),
            ApplyCanonicalResolution {
                canonical_id: "TNT-SEARS".to_string(),
                canonical_type: "tenant_label".to_string(),
                rule_id: "REGISTRY_EXACT".to_string(),
            },
        ),
        (
            "Sears".to_string(),
            ApplyCanonicalResolution {
                canonical_id: "TNT-SEARS".to_string(),
                canonical_type: "tenant_label".to_string(),
                rule_id: "REGISTRY_EXACT".to_string(),
            },
        ),
        (
            "Kmart".to_string(),
            ApplyCanonicalResolution {
                canonical_id: "TNT-KMART".to_string(),
                canonical_type: "tenant_label".to_string(),
                rule_id: "REGISTRY_EXACT".to_string(),
            },
        ),
    ])
}
