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
            validate_entity_v1_self_hash,
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

#[derive(Debug, Clone, PartialEq)]
pub enum EntityExplainV1Source {
    Solve(Value),
    Run {
        run_artifact: Value,
        solve_artifact: Value,
    },
}

pub fn explain_entity_v1(
    query: EntityExplainV1Query,
    source: EntityExplainV1Source,
) -> Result<Value, Refusal> {
    validate_explain_v1_source(&source)?;
    validate_query(&query)?;
    let result_artifact = source.requested_artifact();
    let selection_artifact = source.selection_artifact();
    let mut upstream_artifacts = vec![entity_v1_artifact_reference(result_artifact)?];
    if let Some(solve_artifact) = source.bound_solve_artifact() {
        upstream_artifacts.push(entity_v1_artifact_reference(solve_artifact)?);
    }
    let metadata = entity_v1_lifecycle_metadata_from_source(
        result_artifact,
        EntityArtifactStageV1::Explain,
        upstream_artifacts,
    )?;
    let selector = selector_json(&query);
    let selected = selected_records(&query, selection_artifact);
    let source_hash = required_value_string(
        result_artifact,
        &["artifact_content_hash"],
        "artifact_content_hash",
    )?;
    let source_version = required_value_string(result_artifact, &["version"], "version")?;
    let bound_solve = source
        .bound_solve_artifact()
        .map(|solve| {
            Ok::<Value, Refusal>(json!({
                "version": required_value_string(solve, &["version"], "version")?,
                "content_hash": required_value_string(
                    solve,
                    &["artifact_content_hash"],
                    "artifact_content_hash",
                )?
            }))
        })
        .transpose()?;
    let mut source_result = json!({
        "version": source_version,
        "content_hash": source_hash
    });
    if let Some(bound_solve) = bound_solve
        && let Some(object) = source_result.as_object_mut()
    {
        object.insert("bound_solve".to_string(), bound_solve);
    }
    let mut artifact = json!({
        "version": CANON_ENTITY_EXPLAIN_VERSION_V1,
        "artifact_content_hash": "",
        "metadata": metadata,
        "summary": {
            "counts": {
                "selected_records": selected.len() as u64,
                "source_rows": value_u64_or(result_artifact, &["metadata", "input", "row_count"], 0)
            },
            "labels": {
                "stage": "explain",
                "selector": selector["kind"].as_str().unwrap_or("unknown"),
                "source_version": source_version
            }
        },
        "explanation_path": "explain/evidence.json",
        "query": query,
        "source_result": source_result,
        "result": {
            "selector": selector,
            "registry_snapshot": selection_artifact
                .get("metadata")
                .and_then(|metadata| metadata.get("registry_snapshot"))
                .cloned()
                .unwrap_or(Value::Null),
            "records": selected,
            "evidence": matching_array(selection_artifact, "evidence"),
            "review": matching_array(selection_artifact, "review_items"),
            "promotion": matching_array(selection_artifact, "promotions"),
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

impl EntityExplainV1Source {
    fn requested_artifact(&self) -> &Value {
        match self {
            Self::Solve(artifact) => artifact,
            Self::Run { run_artifact, .. } => run_artifact,
        }
    }

    fn selection_artifact(&self) -> &Value {
        match self {
            Self::Solve(artifact) => artifact,
            Self::Run { solve_artifact, .. } => solve_artifact,
        }
    }

    fn bound_solve_artifact(&self) -> Option<&Value> {
        match self {
            Self::Solve(_) => None,
            Self::Run { solve_artifact, .. } => Some(solve_artifact),
        }
    }
}

fn validate_explain_v1_source(source: &EntityExplainV1Source) -> Result<(), Refusal> {
    match source {
        EntityExplainV1Source::Solve(artifact) => {
            validate_explain_v1_source_artifact(artifact, CANON_ENTITY_SOLVE_VERSION_V1)?;
        }
        EntityExplainV1Source::Run {
            run_artifact,
            solve_artifact,
        } => {
            validate_explain_v1_source_artifact(run_artifact, CANON_ENTITY_RUN_VERSION_V1)?;
            validate_explain_v1_source_artifact(solve_artifact, CANON_ENTITY_SOLVE_VERSION_V1)?;
            validate_run_bound_solve(run_artifact, solve_artifact)?;
        }
    }
    Ok(())
}

fn validate_explain_v1_source_artifact(
    artifact: &Value,
    expected_version: &str,
) -> Result<(), Refusal> {
    let contract = validate_artifact_v1_core_contract(artifact)?;
    if contract.artifact_version != expected_version {
        return Err(explain_refusal(
            "Explain source artifact has the wrong contract version",
            json!({
                "stage": "explain",
                "field": "version",
                "expected": expected_version,
                "actual": contract.artifact_version
            }),
        ));
    }
    validate_entity_v1_self_hash(artifact)?;
    Ok(())
}

fn validate_run_bound_solve(run_artifact: &Value, solve_artifact: &Value) -> Result<(), Refusal> {
    let solve_hash = required_value_string(
        solve_artifact,
        &["artifact_content_hash"],
        "solve.artifact_content_hash",
    )?;
    let solve_path = required_value_string(
        run_artifact,
        &["work_dir", "solve_artifact_path"],
        "work_dir.solve_artifact_path",
    )?;
    if solve_path != "solve/solve.json" {
        return Err(explain_refusal(
            "Run explain requires the canonical bound solve path",
            json!({
                "stage": "explain",
                "field": "work_dir.solve_artifact_path",
                "expected": "solve/solve.json",
                "actual": solve_path
            }),
        ));
    }
    let solve_stage_refs = run_artifact
        .get("stage_artifacts")
        .and_then(Value::as_array)
        .map(|artifacts| {
            artifacts
                .iter()
                .filter(|artifact| {
                    artifact.get("stage").and_then(Value::as_str) == Some("solve")
                        && artifact.get("version").and_then(Value::as_str)
                            == Some(CANON_ENTITY_SOLVE_VERSION_V1)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if solve_stage_refs.len() != 1 {
        return Err(explain_refusal(
            "Run explain requires exactly one solve stage reference",
            json!({
                "stage": "explain",
                "field": "stage_artifacts.solve",
                "expected_version": CANON_ENTITY_SOLVE_VERSION_V1,
                "expected_count": 1,
                "actual_count": solve_stage_refs.len() as u64
            }),
        ));
    }
    let stage_ref = solve_stage_refs[0];
    let stage_hash = required_value_string(
        stage_ref,
        &["artifact_content_hash"],
        "stage_artifacts.solve.artifact_content_hash",
    )?;
    let stage_path = required_value_string(stage_ref, &["path"], "stage_artifacts.solve.path")?;
    if stage_hash != solve_hash || stage_path != solve_path {
        return Err(explain_refusal(
            "Run explain solve stage reference does not match the bound solve artifact",
            json!({
                "stage": "explain",
                "field": "stage_artifacts.solve",
                "expected": {
                    "path": solve_path,
                    "artifact_content_hash": solve_hash
                },
                "actual": {
                    "path": stage_path,
                    "artifact_content_hash": stage_hash
                }
            }),
        ));
    }
    validate_same_json_field(run_artifact, solve_artifact, &["metadata", "profile"])?;
    validate_same_json_field(
        run_artifact,
        solve_artifact,
        &["metadata", "registry_snapshot"],
    )?;
    Ok(())
}

fn validate_same_json_field(left: &Value, right: &Value, path: &[&str]) -> Result<(), Refusal> {
    let left_value = value_at_path(left, path);
    let right_value = value_at_path(right, path);
    if left_value == right_value {
        return Ok(());
    }
    Err(explain_refusal(
        "Run explain source continuity check failed",
        json!({
            "stage": "explain",
            "field": path.join("."),
            "expected": left_value.cloned().unwrap_or(Value::Null),
            "actual": right_value.cloned().unwrap_or(Value::Null)
        }),
    ))
}

fn value_at_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
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
