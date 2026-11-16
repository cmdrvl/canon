//! `canon registry check-id`: verify an agent-friendly id before any lookup.

use super::agent_id::{AgentIdRejection, kind_prefix_is_valid, label_matches, parse_agent_id};
use super::{MappingFile, load_registry_definition};
use crate::{Refusal, RefusalCode, session::registries};
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};

pub const REGISTRY_CHECK_ID_VERSION: &str = "canon_registry_check_id.v0";

#[derive(Debug, Clone)]
pub struct RegistryCheckIdRequest {
    /// The id, optionally followed by `~label`.
    pub id: String,
    /// One registry directory to resolve against.
    pub registry: Option<PathBuf>,
    /// A registries map: the kinds' agent prefixes bound the accepted prefixes
    /// and each kind's registry is searched.
    pub registries: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryCheckIdOutput {
    pub version: String,
    pub kind: String,
    pub id: String,
    pub resolves_to: Option<String>,
    /// `match`, `mismatch` or `absent`.
    pub label: String,
}

impl RegistryCheckIdOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "kind={} id={} resolves_to={} label={}",
            self.kind,
            self.id,
            self.resolves_to.as_deref().unwrap_or("<none>"),
            self.label,
        )
    }
}

struct Target {
    kind: String,
    agent_prefix: Option<String>,
    registry: PathBuf,
}

pub fn check_id(request: RegistryCheckIdRequest) -> Result<RegistryCheckIdOutput, Refusal> {
    let targets = targets(&request)?;
    let known_prefixes = known_prefixes(&request.id, &targets);
    let known = known_prefixes
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let parsed =
        parse_agent_id(&request.id, &known).map_err(|rejection| bad_id(&request.id, rejection))?;

    let matching = targets
        .iter()
        .filter(|target| {
            target
                .agent_prefix
                .as_deref()
                .is_none_or(|prefix| prefix == parsed.prefix)
        })
        .collect::<Vec<_>>();
    let kind = matching
        .iter()
        .find(|target| target.agent_prefix.is_some())
        .map(|target| target.kind.clone())
        .unwrap_or_else(|| parsed.prefix.trim_end_matches('_').to_string());

    let mut resolves_to = None;
    let mut aliases = Vec::new();
    for target in matching {
        let files = mapping_files(&target.registry)?;
        let found = files
            .iter()
            .flat_map(|file| file.entries.iter())
            .find(|entry| entry.input == parsed.id)
            .map(|entry| entry.canonical_id.clone())
            .or_else(|| {
                files
                    .iter()
                    .flat_map(|file| file.entries.iter())
                    .any(|entry| entry.canonical_id == parsed.id)
                    .then(|| parsed.id.clone())
            });
        if let Some(canonical_id) = found {
            aliases = files
                .iter()
                .flat_map(|file| file.entries.iter())
                .filter(|entry| entry.canonical_id == canonical_id)
                .map(|entry| entry.input.clone())
                .collect();
            resolves_to = Some(canonical_id);
            break;
        }
    }

    let label = match parsed.label.as_deref() {
        None => "absent",
        Some(label) if label_matches(label, aliases.iter().map(String::as_str)) => "match",
        Some(_) => "mismatch",
    };
    Ok(RegistryCheckIdOutput {
        version: REGISTRY_CHECK_ID_VERSION.to_string(),
        kind,
        id: parsed.id,
        resolves_to,
        label: label.to_string(),
    })
}

fn targets(request: &RegistryCheckIdRequest) -> Result<Vec<Target>, Refusal> {
    let mut targets = Vec::new();
    if let Some(registry) = &request.registry {
        targets.push(Target {
            kind: String::new(),
            agent_prefix: None,
            registry: registry.clone(),
        });
    }
    if let Some(map_path) = &request.registries {
        let text = std::fs::read_to_string(map_path).map_err(|error| Refusal {
            code: RefusalCode::EIo,
            message: format!(
                "Cannot read registries map '{}': {error}",
                map_path.display()
            ),
            detail: json!({"path": map_path.display().to_string()}),
            next_command: Some("Check the --registries path, then rerun check-id".to_string()),
        })?;
        let map = registries::parse_registries_map(&text).map_err(|error| Refusal {
            code: RefusalCode::EParse,
            message: format!(
                "Cannot parse registries map '{}': {error}",
                map_path.display()
            ),
            detail: json!({"yaml_path": error.path, "reason": error.message}),
            next_command: Some("Fix the registries YAML path named in the refusal".to_string()),
        })?;
        for (kind, config) in &map.kinds {
            targets.push(Target {
                kind: kind.clone(),
                agent_prefix: config.agent_prefix(),
                registry: config.registry.clone(),
            });
        }
    }
    Ok(targets)
}

/// With a map, only the kinds' agent prefixes are known. Without one, any
/// well-formed prefix is accepted: nothing declares which kinds exist.
fn known_prefixes(id: &str, targets: &[Target]) -> Vec<String> {
    let mapped = targets
        .iter()
        .filter_map(|target| target.agent_prefix.clone())
        .collect::<Vec<_>>();
    if !mapped.is_empty() {
        return mapped;
    }
    let lowered = id.split('~').next().unwrap_or(id).to_ascii_lowercase();
    match lowered.find('_') {
        Some(index) if kind_prefix_is_valid(&lowered[..=index]) => {
            vec![lowered[..=index].to_string()]
        }
        _ => Vec::new(),
    }
}

fn mapping_files(registry: &Path) -> Result<Vec<MappingFile>, Refusal> {
    load_registry_definition(registry)
        .map(|(_, _, files)| files)
        .map_err(|error| Refusal::bad_registry(&registry.display().to_string(), &error.to_string()))
}

fn bad_id(id: &str, rejection: AgentIdRejection) -> Refusal {
    Refusal {
        code: RefusalCode::EBadId,
        message: format!("'{id}' is not a valid agent id: {}", rejection.reason()),
        detail: json!({"id": id, "reason": rejection.reason()}),
        next_command: Some(
            "Copy the id again from the registry or session receipt, then rerun canon registry check-id"
                .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejection_reasons_are_the_documented_detail_values() {
        for (rejection, reason) in [
            (AgentIdRejection::BadLength, "bad_length"),
            (AgentIdRejection::BadSymbol, "bad_symbol"),
            (AgentIdRejection::UnknownKind, "unknown_kind"),
            (AgentIdRejection::BadCheck, "bad_check"),
        ] {
            let refusal = bad_id("x", rejection);
            assert_eq!(refusal.code, RefusalCode::EBadId);
            assert_eq!(refusal.detail["reason"], reason);
            assert!(refusal.next_command.is_some());
        }
    }

    #[test]
    fn without_a_map_any_well_formed_prefix_is_known() {
        assert_eq!(known_prefixes("PER_abc", &[]), vec!["per_".to_string()]);
        assert!(known_prefixes("abc", &[]).is_empty());
        assert!(known_prefixes("Per9_abc", &[]).is_empty());
    }
}
