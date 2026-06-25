//! Shared streaming IO contract for entity prepare/apply stages.
//!
//! This module intentionally defines stage-neutral metadata and refusal helpers
//! only. Prepare/apply own their projection and output logic; both can depend on
//! these chunk, row provenance, budget, and telemetry contracts.

use crate::Refusal;
use crate::entity::error::EntityRefusalKind;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const CANON_ENTITY_STREAM_VERSION: &str = "canon_entity_stream.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityStreamStage {
    Prepare,
    Apply,
}

impl EntityStreamStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::Apply => "apply",
        }
    }

    pub const fn command(self) -> &'static str {
        match self {
            Self::Prepare => "canon entity prepare <ROWS> --profile <PROFILE>",
            Self::Apply => "canon entity apply <ROWS> --run <RUN_ARTIFACT>",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityStreamFormat {
    Csv,
    Jsonl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityStreamTelemetryHook {
    BeforeChunk,
    AfterChunk,
    OnRefusal,
}

pub const REQUIRED_STREAM_TELEMETRY_HOOKS: &[EntityStreamTelemetryHook] = &[
    EntityStreamTelemetryHook::BeforeChunk,
    EntityStreamTelemetryHook::AfterChunk,
    EntityStreamTelemetryHook::OnRefusal,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityStreamInput {
    pub version: String,
    pub stage: EntityStreamStage,
    pub format: EntityStreamFormat,
    pub source: String,
    pub content_hash: String,
    pub row_count: u64,
    pub byte_count: u64,
}

impl EntityStreamInput {
    pub fn new(
        stage: EntityStreamStage,
        format: EntityStreamFormat,
        source: impl Into<String>,
        content_hash: impl Into<String>,
        row_count: u64,
        byte_count: u64,
    ) -> Self {
        Self {
            version: CANON_ENTITY_STREAM_VERSION.to_string(),
            stage,
            format,
            source: source.into(),
            content_hash: content_hash.into(),
            row_count,
            byte_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityStreamChunkMetadata {
    pub version: String,
    pub stage: EntityStreamStage,
    pub source: String,
    pub input_hash: String,
    pub chunk_index: u64,
    pub first_row_ordinal: u64,
    pub row_count: u64,
    pub byte_start: u64,
    pub byte_len: u64,
}

impl EntityStreamChunkMetadata {
    pub fn row_end_exclusive(&self) -> u64 {
        self.first_row_ordinal + self.row_count
    }

    pub fn byte_end_exclusive(&self) -> u64 {
        self.byte_start + self.byte_len
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityStreamRowProvenance {
    pub version: String,
    pub stage: EntityStreamStage,
    pub chunk_index: u64,
    pub row_ordinal: u64,
    pub source_row_id: Option<String>,
    pub byte_start: u64,
    pub byte_len: u64,
}

impl EntityStreamRowProvenance {
    pub fn new(
        stage: EntityStreamStage,
        chunk_index: u64,
        row_ordinal: u64,
        source_row_id: Option<impl Into<String>>,
        byte_start: u64,
        byte_len: u64,
    ) -> Self {
        Self {
            version: CANON_ENTITY_STREAM_VERSION.to_string(),
            stage,
            chunk_index,
            row_ordinal,
            source_row_id: source_row_id.map(Into::into),
            byte_start,
            byte_len,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityStreamTelemetry {
    pub version: String,
    pub stage: EntityStreamStage,
    pub source: String,
    pub input_hash: String,
    pub chunk_count: u64,
    pub rows_seen: u64,
    pub bytes_seen: u64,
    pub max_chunk_rows: u64,
    pub max_chunk_bytes: u64,
    pub hooks: Vec<EntityStreamTelemetryHook>,
}

pub fn required_stream_telemetry_hooks() -> &'static [EntityStreamTelemetryHook] {
    REQUIRED_STREAM_TELEMETRY_HOOKS
}

pub fn deterministic_chunk_metadata(
    input: &EntityStreamInput,
    target_rows_per_chunk: u64,
) -> Result<Vec<EntityStreamChunkMetadata>, Refusal> {
    if target_rows_per_chunk == 0 {
        return Err(stream_io_budget_refusal(
            input.stage,
            "target_rows_per_chunk",
            input.row_count,
            target_rows_per_chunk,
        ));
    }
    if input.row_count == 0 {
        return Ok(Vec::new());
    }

    let chunk_count = input.row_count.div_ceil(target_rows_per_chunk);
    let byte_ranges = deterministic_byte_ranges(input.byte_count, chunk_count);
    let mut chunks = Vec::with_capacity(usize::try_from(chunk_count).unwrap_or(0));

    for chunk_index in 0..chunk_count {
        let first_row_ordinal = chunk_index * target_rows_per_chunk;
        let remaining_rows = input.row_count - first_row_ordinal;
        let row_count = remaining_rows.min(target_rows_per_chunk);
        let (byte_start, byte_len) =
            byte_ranges[usize::try_from(chunk_index).expect("chunk index fits usize")];
        chunks.push(EntityStreamChunkMetadata {
            version: CANON_ENTITY_STREAM_VERSION.to_string(),
            stage: input.stage,
            source: input.source.clone(),
            input_hash: input.content_hash.clone(),
            chunk_index,
            first_row_ordinal,
            row_count,
            byte_start,
            byte_len,
        });
    }

    Ok(chunks)
}

pub fn stream_telemetry(
    input: &EntityStreamInput,
    chunks: &[EntityStreamChunkMetadata],
) -> EntityStreamTelemetry {
    EntityStreamTelemetry {
        version: CANON_ENTITY_STREAM_VERSION.to_string(),
        stage: input.stage,
        source: input.source.clone(),
        input_hash: input.content_hash.clone(),
        chunk_count: u64::try_from(chunks.len()).expect("chunk count fits u64"),
        rows_seen: chunks.iter().map(|chunk| chunk.row_count).sum(),
        bytes_seen: chunks.iter().map(|chunk| chunk.byte_len).sum(),
        max_chunk_rows: chunks
            .iter()
            .map(|chunk| chunk.row_count)
            .max()
            .unwrap_or_default(),
        max_chunk_bytes: chunks
            .iter()
            .map(|chunk| chunk.byte_len)
            .max()
            .unwrap_or_default(),
        hooks: REQUIRED_STREAM_TELEMETRY_HOOKS.to_vec(),
    }
}

pub fn stream_input_contract_refusal(
    stage: EntityStreamStage,
    row_ordinal: u64,
    field: impl Into<String>,
    message: impl Into<String>,
) -> Refusal {
    let field = field.into();
    EntityRefusalKind::InputContract.to_refusal(
        message,
        json!({
            "stage": stage.as_str(),
            "row_ordinal": row_ordinal,
            "field": field,
            "recovery": "Fix extraction or provide a profile mapping before rerunning the same input stream"
        }),
        Some(stage.command().to_string()),
    )
}

pub fn stream_io_budget_refusal(
    stage: EntityStreamStage,
    limit: impl Into<String>,
    observed: u64,
    configured: u64,
) -> Refusal {
    EntityRefusalKind::IoBudget.to_refusal(
        "Entity stream IO budget exceeded before stage emission",
        json!({
            "stage": stage.as_str(),
            "limit": limit.into(),
            "observed": observed,
            "configured": configured,
            "recovery": "Increase the explicit IO budget or split the physical input while preserving one global prepared/index view"
        }),
        Some(stage.command().to_string()),
    )
}

fn deterministic_byte_ranges(total_bytes: u64, chunk_count: u64) -> Vec<(u64, u64)> {
    if chunk_count == 0 {
        return Vec::new();
    }

    let base_len = total_bytes / chunk_count;
    let remainder = total_bytes % chunk_count;
    let mut offset = 0;
    let mut ranges = Vec::with_capacity(usize::try_from(chunk_count).unwrap_or(0));

    for chunk_index in 0..chunk_count {
        let byte_len = base_len + u64::from(chunk_index < remainder);
        ranges.push((offset, byte_len));
        offset += byte_len;
    }

    ranges
}
