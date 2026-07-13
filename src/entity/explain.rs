#![forbid(unsafe_code)]

//! V1 proof-trace artifact construction for `canon entity explain`.

#[path = "../evidence/explain.rs"]
pub mod evidence_waterfall;

pub use evidence_waterfall::*;

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_EXPLAIN_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1,
        CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactStageV1,
        error::EntityRefusalKind,
        review::{required_value_string, value_string_or, value_u64_or},
        schema::{
            entity_v1_artifact_reference, entity_v1_lifecycle_metadata_from_source,
            finalize_entity_v1_self_hash, validate_artifact_v1_core_contract,
        },
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityExplainV1Query {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escrow_id: Option<String>,
}

pub fn explain_entity_v1(
    query: EntityExplainV1Query,
    result_artifact: Value,
) -> Result<Value, Refusal> {
    validate_explain_v1_source(&result_artifact)?;
    validate_query(&query)?;
    let source_ref = entity_v1_artifact_reference(&result_artifact)?;
    let metadata = entity_v1_lifecycle_metadata_from_source(
        &result_artifact,
        EntityArtifactStageV1::Explain,
        vec![source_ref],
    )?;
    let selector = selector_json(&query);
    let selected = selected_records(&query, &result_artifact);
    let source_hash = required_value_string(
        &result_artifact,
        &["artifact_content_hash"],
        "artifact_content_hash",
    )?;
    let source_version = required_value_string(&result_artifact, &["version"], "version")?;
    let mut artifact = json!({
        "version": CANON_ENTITY_EXPLAIN_VERSION_V1,
        "artifact_content_hash": "",
        "metadata": metadata,
        "summary": {
            "counts": {
                "selected_records": selected.len() as u64,
                "source_rows": value_u64_or(&result_artifact, &["metadata", "input", "row_count"], 0)
            },
            "labels": {
                "stage": "explain",
                "selector": selector["kind"].as_str().unwrap_or("unknown"),
                "source_version": source_version
            }
        },
        "explanation_path": "explain/evidence.json",
        "query": query,
        "source_result": {
            "version": source_version,
            "content_hash": source_hash
        },
        "result": {
            "selector": selector,
            "registry_snapshot": result_artifact
                .get("metadata")
                .and_then(|metadata| metadata.get("registry_snapshot"))
                .cloned()
                .unwrap_or(Value::Null),
            "records": selected,
            "evidence": matching_array(&result_artifact, "evidence"),
            "review": matching_array(&result_artifact, "review_items"),
            "promotion": matching_array(&result_artifact, "promotions"),
            "next_command": "canon entity apply <PROMOTE.json> --rows <ROWS> --registry <REGISTRY>"
        }
    });
    finalize_entity_v1_self_hash(&mut artifact)?;
    Ok(artifact)
}

pub fn render_explain_v1_summary(artifact: &Value) -> String {
    let selector_kind = value_string_or(artifact, &["result", "selector", "kind"], "<selector>");
    let selector_value = value_string_or(artifact, &["result", "selector", "value"], "<value>");
    let registry = value_string_or(
        artifact,
        &["result", "registry_snapshot", "id"],
        "<registry>",
    );
    let version = value_string_or(
        artifact,
        &["result", "registry_snapshot", "version"],
        "<version>",
    );
    let records = value_u64_or(artifact, &["summary", "counts", "selected_records"], 0);
    format!(
        "{selector_kind} {selector_value} explain v1 registry={registry}@{version} records={records}"
    )
}

fn validate_explain_v1_source(artifact: &Value) -> Result<(), Refusal> {
    let contract = validate_artifact_v1_core_contract(artifact)?;
    if !matches!(
        contract.artifact_version,
        CANON_ENTITY_RUN_VERSION_V1 | CANON_ENTITY_SOLVE_VERSION_V1
    ) {
        return Err(explain_refusal(
            "Explain requires a canon_entity_run.v1 or canon_entity_solve.v1 artifact",
            json!({
                "stage": "explain",
                "field": "version",
                "expected": [CANON_ENTITY_RUN_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1],
                "actual": contract.artifact_version
            }),
        ));
    }
    Ok(())
}

fn validate_query(query: &EntityExplainV1Query) -> Result<(), Refusal> {
    let count = usize::from(query.row_id.is_some())
        + usize::from(query.surface_id.is_some())
        + usize::from(query.canonical_id.is_some())
        + usize::from(query.escrow_id.is_some());
    if count == 1 {
        Ok(())
    } else {
        Err(explain_refusal(
            "Explain query must set exactly one selector",
            json!({
                "stage": "explain",
                "field": "query"
            }),
        ))
    }
}

fn selector_json(query: &EntityExplainV1Query) -> Value {
    if let Some(value) = &query.row_id {
        json!({ "kind": "row_id", "value": value })
    } else if let Some(value) = &query.surface_id {
        json!({ "kind": "surface_id", "value": value })
    } else if let Some(value) = &query.canonical_id {
        json!({ "kind": "canonical_id", "value": value })
    } else {
        json!({ "kind": "escrow_id", "value": query.escrow_id.as_deref().unwrap_or("") })
    }
}

fn selected_records(query: &EntityExplainV1Query, artifact: &Value) -> Vec<Value> {
    let selector = selector_json(query);
    let Some(kind) = selector.get("kind").and_then(Value::as_str) else {
        return Vec::new();
    };
    let Some(value) = selector.get("value").and_then(Value::as_str) else {
        return Vec::new();
    };
    [
        "entities",
        "surfaces",
        "rows",
        "abstentions",
        "contradictions",
    ]
    .into_iter()
    .filter_map(|field| artifact.get(field).and_then(Value::as_array))
    .flat_map(|items| items.iter())
    .filter(|item| record_matches(item, kind, value))
    .cloned()
    .collect()
}

fn record_matches(record: &Value, kind: &str, expected: &str) -> bool {
    let candidate_fields = match kind {
        "row_id" => &["row_id", "source_row_id", "source_id"][..],
        "surface_id" => &["surface_id"][..],
        "canonical_id" => &["canonical_id", "canon_id"][..],
        "escrow_id" => &["escrow_id"][..],
        _ => &[][..],
    };
    candidate_fields
        .iter()
        .any(|field| record.get(*field).and_then(Value::as_str) == Some(expected))
        || candidate_fields.iter().any(|field| {
            record
                .get(*field)
                .and_then(Value::as_array)
                .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(expected)))
        })
}

fn matching_array(artifact: &Value, field: &str) -> Value {
    artifact
        .get(field)
        .and_then(Value::as_array)
        .cloned()
        .map(Value::Array)
        .unwrap_or_else(|| Value::Array(Vec::new()))
}

fn explain_refusal(message: &'static str, detail: Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some("canon entity explain <RESULT.json> --row <ROW_ID>".to_string()),
    )
}
