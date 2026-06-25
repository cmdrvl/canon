use canon::entity::stream::{
    CANON_ENTITY_STREAM_VERSION, EntityStreamFormat, EntityStreamInput, EntityStreamRowProvenance,
    EntityStreamStage, EntityStreamTelemetryHook, deterministic_chunk_metadata,
    required_stream_telemetry_hooks, stream_input_contract_refusal, stream_io_budget_refusal,
    stream_telemetry,
};
use serde_json::json;

#[test]
fn entity_stream_contract_defines_prepare_apply_shared_metadata() {
    let input = EntityStreamInput::new(
        EntityStreamStage::Prepare,
        EntityStreamFormat::Jsonl,
        "tenants.jsonl",
        "blake3:rows",
        7,
        22,
    );

    let chunks = deterministic_chunk_metadata(&input, 3).expect("chunking succeeds");
    assert_eq!(input.version, CANON_ENTITY_STREAM_VERSION);
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].first_row_ordinal, 0);
    assert_eq!(chunks[1].first_row_ordinal, 3);
    assert_eq!(chunks[2].first_row_ordinal, 6);
    assert_eq!(chunks[2].row_count, 1);

    let telemetry = stream_telemetry(&input, &chunks);
    assert_eq!(telemetry.version, CANON_ENTITY_STREAM_VERSION);
    assert_eq!(telemetry.chunk_count, 3);
    assert_eq!(telemetry.rows_seen, 7);
    assert_eq!(telemetry.bytes_seen, 22);
    assert_eq!(telemetry.max_chunk_rows, 3);
    assert_eq!(
        telemetry.hooks,
        [
            EntityStreamTelemetryHook::BeforeChunk,
            EntityStreamTelemetryHook::AfterChunk,
            EntityStreamTelemetryHook::OnRefusal
        ]
    );
    assert_eq!(required_stream_telemetry_hooks(), telemetry.hooks);
}

#[test]
fn stream_chunk_metadata_deterministic() {
    let input = EntityStreamInput::new(
        EntityStreamStage::Apply,
        EntityStreamFormat::Csv,
        "apply.csv",
        "blake3:apply",
        10,
        101,
    );

    let first = deterministic_chunk_metadata(&input, 4).expect("first chunking succeeds");
    let second = deterministic_chunk_metadata(&input, 4).expect("second chunking succeeds");
    assert_eq!(first, second);

    let row_ranges = first
        .iter()
        .map(|chunk| (chunk.first_row_ordinal, chunk.row_end_exclusive()))
        .collect::<Vec<_>>();
    assert_eq!(row_ranges, [(0, 4), (4, 8), (8, 10)]);

    let byte_ranges = first
        .iter()
        .map(|chunk| (chunk.byte_start, chunk.byte_end_exclusive()))
        .collect::<Vec<_>>();
    assert_eq!(byte_ranges, [(0, 34), (34, 68), (68, 101)]);
}

#[test]
fn entity_stream_contract_refusals_are_stage_specific_and_actionable() {
    let input = EntityStreamInput::new(
        EntityStreamStage::Prepare,
        EntityStreamFormat::Jsonl,
        "bad.jsonl",
        "blake3:bad",
        5,
        80,
    );
    let budget = deterministic_chunk_metadata(&input, 0).expect_err("zero chunk size refuses");
    let budget_payload = budget.to_canon_output().refusal.expect("budget refusal");
    assert_eq!(
        serde_json::to_value(budget_payload.code).unwrap(),
        json!("E_ENTITY_IO_BUDGET")
    );
    assert_eq!(budget_payload.detail["stage"], "prepare");
    assert_eq!(budget_payload.detail["limit"], "target_rows_per_chunk");
    assert!(
        budget_payload
            .next_command
            .unwrap()
            .contains("entity prepare")
    );

    let input_contract = stream_input_contract_refusal(
        EntityStreamStage::Apply,
        42,
        "alias_surfaces_json",
        "Malformed side-field JSON",
    );
    let input_payload = input_contract
        .to_canon_output()
        .refusal
        .expect("input refusal");
    assert_eq!(
        serde_json::to_value(input_payload.code).unwrap(),
        json!("E_ENTITY_INPUT_CONTRACT")
    );
    assert_eq!(input_payload.detail["stage"], "apply");
    assert_eq!(input_payload.detail["row_ordinal"], 42);
    assert_eq!(input_payload.detail["field"], "alias_surfaces_json");
    assert!(input_payload.next_command.unwrap().contains("entity apply"));

    let direct_budget = stream_io_budget_refusal(EntityStreamStage::Apply, "max_bytes", 1_001, 100);
    assert_eq!(
        direct_budget.to_canon_output().refusal.unwrap().detail["configured"],
        100
    );
}

#[test]
fn entity_stream_row_provenance_keeps_source_row_id_out_of_identity() {
    let row = EntityStreamRowProvenance::new(
        EntityStreamStage::Prepare,
        2,
        17,
        Some("source-row-17"),
        900,
        45,
    );

    let payload = serde_json::to_value(row).expect("row provenance serializes");
    assert_eq!(payload["version"], CANON_ENTITY_STREAM_VERSION);
    assert_eq!(payload["stage"], "prepare");
    assert_eq!(payload["chunk_index"], 2);
    assert_eq!(payload["row_ordinal"], 17);
    assert_eq!(payload["source_row_id"], "source-row-17");
    assert_eq!(payload["byte_start"], 900);
    assert_eq!(payload["byte_len"], 45);
}
