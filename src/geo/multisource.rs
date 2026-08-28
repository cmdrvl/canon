#![forbid(unsafe_code)]

//! Geo command adapter for Canon's existing N-source entity materializer.
//!
//! Source roles describe provenance and workflow position. They do not assign
//! evidence weight, and a larger source count is never treated as independent
//! information. The bounded Geo solver admits evidence later through explicit
//! rho contracts.

use crate::{
    Refusal,
    entity::{
        error::EntityRefusalKind,
        run::link::multisource::{
            EntityMultisourceLinkArtifact, EntityMultisourceLinkRequest, EntityNamedSource,
            EntitySourceComparison, EntitySourceRole, complete_comparison_graph,
            materialize_multisource_rows, validate_multisource_link_artifact,
        },
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

pub const CANON_GEO_MULTISOURCE_REQUEST_VERSION: &str = "canon_geo_multisource_request.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoMultisourceRequest {
    pub version: String,
    pub sources: Vec<GeoMultisourceSource>,
    #[serde(default)]
    pub comparison_graph: Vec<EntitySourceComparison>,
    pub default_pair_budget: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoMultisourceSource {
    pub name: String,
    pub role: EntitySourceRole,
    pub rows_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_id_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_id_column: Option<String>,
}

pub fn materialize_geo_multisource(
    request: &GeoMultisourceRequest,
    output_rows: &Path,
) -> Result<EntityMultisourceLinkArtifact, Refusal> {
    validate_geo_request(request)?;
    let sources = request
        .sources
        .iter()
        .map(as_entity_source)
        .collect::<Vec<_>>();
    let comparison_graph = if request.comparison_graph.is_empty() {
        complete_comparison_graph(request.sources.iter().map(|source| source.name.clone()))
    } else {
        request.comparison_graph.clone()
    };
    let artifact = materialize_multisource_rows(EntityMultisourceLinkRequest {
        sources,
        comparison_graph,
        canonical_source: None,
        default_pair_budget: request.default_pair_budget,
        output_rows,
    })?;
    validate_multisource_link_artifact(&artifact)?;
    Ok(artifact)
}

pub fn canonical_multisource_artifact_bytes(
    artifact: &EntityMultisourceLinkArtifact,
) -> Result<Vec<u8>, Refusal> {
    validate_multisource_link_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Geo N-source artifact could not be serialized",
            json!({
                "stage": "link_sources",
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Repair the request and rerun canon geo link-sources".to_string()),
        )
    })
}

fn validate_geo_request(request: &GeoMultisourceRequest) -> Result<(), Refusal> {
    if request.version != CANON_GEO_MULTISOURCE_REQUEST_VERSION {
        return Err(input_refusal(
            "Geo N-source request version mismatch",
            json!({
                "stage": "link_sources",
                "expected": CANON_GEO_MULTISOURCE_REQUEST_VERSION,
                "actual": request.version,
                "writes_performed": false
            }),
        ));
    }
    if request.sources.len() < 3 {
        return Err(input_refusal(
            "Geo N-source materialization requires at least three named sources",
            json!({
                "stage": "link_sources",
                "reason": "too_few_geo_sources",
                "source_count": request.sources.len(),
                "writes_performed": false
            }),
        ));
    }
    let canonical_references = request
        .sources
        .iter()
        .filter(|source| source.role == EntitySourceRole::CanonicalReference)
        .map(|source| source.name.clone())
        .collect::<Vec<_>>();
    if !canonical_references.is_empty() {
        return Err(input_refusal(
            "Geo N-source materialization does not permit a globally canonical input source",
            json!({
                "stage": "link_sources",
                "reason": "canonical_reference_forbidden",
                "sources": canonical_references,
                "role_semantics": "tile and client sources contribute evidence under declared rho contracts; no vendor wins by role",
                "writes_performed": false
            }),
        ));
    }
    let target_count = request
        .sources
        .iter()
        .filter(|source| source.role == EntitySourceRole::Target)
        .count();
    if target_count != 1 {
        return Err(input_refusal(
            "Geo N-source materialization requires exactly one target source",
            json!({
                "stage": "link_sources",
                "reason": "target_cardinality",
                "target_count": target_count,
                "role_semantics": "the target is the client property book being keyed; references and peers supply bounded evidence",
                "writes_performed": false
            }),
        ));
    }
    if !request
        .sources
        .iter()
        .any(|source| source.role == EntitySourceRole::Reference)
    {
        return Err(input_refusal(
            "Geo N-source materialization requires at least one reference source",
            json!({
                "stage": "link_sources",
                "reason": "missing_reference",
                "role_semantics": "a reference supplies identity evidence within its declared coverage; it is not globally canonical",
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn as_entity_source(source: &GeoMultisourceSource) -> EntityNamedSource<'_> {
    EntityNamedSource {
        name: &source.name,
        role: source.role,
        rows_path: &source.rows_path,
        local_id_column: source.local_id_column.as_deref(),
        anchor_namespace: source.anchor_namespace.as_deref(),
        anchor_column: source.anchor_column.as_deref(),
        canonical_id_column: source.canonical_id_column.as_deref(),
    }
}

fn input_refusal(message: &'static str, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::InputContract.to_refusal(
        message,
        detail,
        Some(
            "Repair the request against canon_geo_multisource_request.v0, then rerun canon geo link-sources"
                .to_string(),
        ),
    )
}
