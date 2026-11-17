//! `canon registry scope-policy`: opt a registry into optional scope
//! dimensions (bd-vdcw.54). A listed dimension lets a bare lookup resolve the
//! unscoped entry and lets a scoped lookup fall back to it.

use super::{ScopePolicy, add_entry, load_registry_definition};
use crate::{Refusal, registry::RegistryVersionBump};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Dimensions a registry may declare optional.
const SUPPORTED_DIMENSIONS: &[&str] = &["role"];

#[derive(Debug, Clone)]
pub struct RegistryScopePolicyRequest {
    pub registry: PathBuf,
    pub add_dimensions: Vec<String>,
    pub remove_dimensions: Vec<String>,
    pub bump: Option<RegistryVersionBump>,
    pub next_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryScopePolicyOutput {
    pub version: String,
    pub registry_id: String,
    pub version_before: String,
    pub version_after: String,
    pub previous_policy: ScopePolicy,
    pub new_policy: ScopePolicy,
    pub touched_files: Vec<String>,
}

impl RegistryScopePolicyOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "scope_policy optional_dimensions=[{}] registry={}@{}",
            self.new_policy.optional_dimensions.join(","),
            self.registry_id,
            self.version_after
        )
    }
}

fn check_dimension(registry: &Path, dimension: &str) -> Result<String, Refusal> {
    let trimmed = add_entry::ascii_trim(dimension).to_string();
    if SUPPORTED_DIMENSIONS.contains(&trimmed.as_str()) {
        return Ok(trimmed);
    }
    Err(add_entry::parse_refusal(
        registry,
        "Unsupported scope policy dimension",
        json!({"dimension": dimension, "supported": SUPPORTED_DIMENSIONS}),
        "canon registry scope-policy --registry <DIR> --optional-dimension role --bump patch",
    ))
}

pub fn set_scope_policy(
    request: RegistryScopePolicyRequest,
) -> Result<RegistryScopePolicyOutput, Refusal> {
    let registry_path = request.registry.join("registry.json");
    let (registry_json, registry_meta, _mapping_files) =
        load_registry_definition(&request.registry).map_err(|error| {
            Refusal::bad_registry(&request.registry.display().to_string(), &error.to_string())
        })?;
    if request.add_dimensions.is_empty() && request.remove_dimensions.is_empty() {
        return Err(add_entry::parse_refusal(
            &request.registry,
            "scope-policy needs --optional-dimension or --remove-dimension",
            json!({}),
            "canon registry scope-policy --registry <DIR> --optional-dimension role --bump patch",
        ));
    }
    let previous = registry_json.scope_policy.clone();
    let mut dimensions = previous.optional_dimensions.clone();
    for dimension in &request.add_dimensions {
        let dimension = check_dimension(&request.registry, dimension)?;
        if !dimensions.contains(&dimension) {
            dimensions.push(dimension);
        }
    }
    for dimension in &request.remove_dimensions {
        let dimension = check_dimension(&request.registry, dimension)?;
        dimensions.retain(|existing| existing != &dimension);
    }
    dimensions.sort();
    let new_policy = ScopePolicy {
        optional_dimensions: dimensions,
    };
    let version_after = add_entry::resolve_next_version(
        &request.registry,
        &registry_json.version,
        request.bump,
        request.next_version.as_deref(),
    )?;

    let bytes =
        fs::read(&registry_path).map_err(|error| add_entry::io_refusal(&registry_path, error))?;
    let mut value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        Refusal::bad_registry(
            &request.registry.display().to_string(),
            &format!("Failed to parse registry.json: {error}"),
        )
    })?;
    let Some(object) = value.as_object_mut() else {
        return Err(Refusal::bad_registry(
            &request.registry.display().to_string(),
            "registry.json must be a JSON object",
        ));
    };
    object.insert("version".to_string(), Value::String(version_after.clone()));
    if new_policy.optional_dimensions.is_empty() {
        object.remove("scope_policy");
    } else {
        object.insert(
            "scope_policy".to_string(),
            json!({"optional_dimensions": new_policy.optional_dimensions}),
        );
    }
    add_entry::stamp_updated(object);
    let new_bytes = add_entry::to_pretty_bytes(&value, &request.registry)?;
    if let Err(error) = add_entry::write_atomic(&registry_path, &new_bytes) {
        let _ = add_entry::write_atomic(&registry_path, &bytes);
        return Err(add_entry::io_refusal(&registry_path, error));
    }

    Ok(RegistryScopePolicyOutput {
        version: "canon_registry_scope_policy.v0".to_string(),
        registry_id: registry_meta.id,
        version_before: registry_json.version,
        version_after,
        previous_policy: previous,
        new_policy,
        touched_files: vec!["registry.json".to_string()],
    })
}
