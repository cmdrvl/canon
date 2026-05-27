use super::{DefaultIdScheme, MappingFile, add_entry, load_registry_definition};
use crate::{Refusal, RefusalCode, registry::RegistryVersionBump};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct RegistryDefaultIdSchemeRequest {
    pub registry: PathBuf,
    pub prefix: String,
    pub zero_pad: Option<usize>,
    pub strict: bool,
    pub bump: Option<RegistryVersionBump>,
    pub next_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryDefaultIdSchemeRegistry {
    pub id: String,
    pub source: String,
    pub version_before: String,
    pub version_after: String,
    pub entry_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryDefaultIdSchemeWarning {
    pub code: String,
    pub canonical_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryDefaultIdSchemeSummary {
    pub warnings: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryDefaultIdSchemeOutput {
    pub version: String,
    pub registry: RegistryDefaultIdSchemeRegistry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_scheme: Option<DefaultIdScheme>,
    pub new_scheme: DefaultIdScheme,
    pub summary: RegistryDefaultIdSchemeSummary,
    pub warnings: Vec<RegistryDefaultIdSchemeWarning>,
    pub touched_files: Vec<String>,
}

impl RegistryDefaultIdSchemeOutput {
    pub fn render_plain(&self) -> String {
        format!("{}-{}", self.new_scheme.prefix, self.new_scheme.zero_pad)
    }
}

const DEFAULT_ZERO_PAD: usize = 3;
const MAX_ZERO_PAD: usize = 20;

pub fn set_default_id_scheme(
    request: RegistryDefaultIdSchemeRequest,
) -> Result<RegistryDefaultIdSchemeOutput, Refusal> {
    let registry_path = request.registry.join("registry.json");
    let (registry_json, registry_meta, mapping_files) = load_registry_definition(&request.registry)
        .map_err(|error| {
            Refusal::bad_registry(&request.registry.display().to_string(), &error.to_string())
        })?;
    let prefix = validate_prefix(&request.registry, &request.prefix)?;
    let zero_pad = validate_zero_pad(
        &request.registry,
        request.zero_pad.unwrap_or(DEFAULT_ZERO_PAD),
    )?;
    let new_scheme = DefaultIdScheme { prefix, zero_pad };
    let warnings = collect_scheme_warnings(&new_scheme, &mapping_files);
    if request.strict && !warnings.is_empty() {
        return Err(Refusal {
            code: RefusalCode::EBadRegistry,
            message: "Existing canonical IDs do not conform to the requested default_id_scheme"
                .to_string(),
            detail: json!({
                "registry": request.registry.display().to_string(),
                "new_scheme": new_scheme,
                "warnings": warnings,
            }),
            next_command: Some(
                "Repair in-namespace canonical IDs or rerun without --strict to record warnings"
                    .to_string(),
            ),
        });
    }
    let version_after = add_entry::resolve_next_version(
        &request.registry,
        &registry_json.version,
        request.bump,
        request.next_version.as_deref(),
    )?;
    let registry_bytes = build_registry_bytes(
        &request.registry,
        &registry_path,
        &new_scheme,
        &version_after,
    )?;
    let original =
        fs::read(&registry_path).map_err(|error| add_entry::io_refusal(&registry_path, error))?;
    if let Err(error) = add_entry::write_atomic(&registry_path, &registry_bytes) {
        let _ = add_entry::write_atomic(&registry_path, &original);
        return Err(add_entry::io_refusal(&registry_path, error));
    }

    Ok(RegistryDefaultIdSchemeOutput {
        version: "canon_registry_default_id_scheme.v0".to_string(),
        registry: RegistryDefaultIdSchemeRegistry {
            id: registry_meta.id,
            source: registry_meta.source,
            version_before: registry_json.version,
            version_after,
            entry_count: registry_json.entry_count,
        },
        previous_scheme: registry_json.default_id_scheme,
        new_scheme,
        summary: RegistryDefaultIdSchemeSummary {
            warnings: warnings.len(),
        },
        warnings,
        touched_files: vec!["registry.json".to_string()],
    })
}

fn validate_prefix(registry: &Path, prefix: &str) -> Result<String, Refusal> {
    let trimmed = add_entry::ascii_trim(prefix);
    let mut chars = trimmed.chars();
    let valid = chars.next().is_some_and(|first| first.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit());
    if valid && trimmed == prefix {
        return Ok(trimmed.to_string());
    }

    Err(add_entry::parse_refusal(
        registry,
        "Invalid registry ID prefix",
        json!({
            "prefix": prefix,
            "expected": "^[A-Z][A-Z0-9]*$",
        }),
        "canon registry default-id-scheme --prefix PPL --registry <DIR>",
    ))
}

fn validate_zero_pad(registry: &Path, zero_pad: usize) -> Result<usize, Refusal> {
    if (1..=MAX_ZERO_PAD).contains(&zero_pad) {
        return Ok(zero_pad);
    }
    Err(add_entry::parse_refusal(
        registry,
        "--zero-pad must be between 1 and 20",
        json!({
            "zero_pad": zero_pad,
            "min": 1,
            "max": MAX_ZERO_PAD,
        }),
        "canon registry default-id-scheme --zero-pad 3 --registry <DIR>",
    ))
}

fn collect_scheme_warnings(
    scheme: &DefaultIdScheme,
    mapping_files: &[MappingFile],
) -> Vec<RegistryDefaultIdSchemeWarning> {
    let namespace = format!("{}-", scheme.prefix);
    let mut canonical_ids = BTreeSet::new();
    for mapping_file in mapping_files {
        for entry in &mapping_file.entries {
            canonical_ids.insert(entry.canonical_id.clone());
        }
    }

    let mut warnings = Vec::new();
    for canonical_id in canonical_ids {
        let Some(suffix) = canonical_id.strip_prefix(&namespace) else {
            continue;
        };
        let reason = if suffix.is_empty() {
            Some("empty_suffix")
        } else if !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            Some("non_numeric_suffix")
        } else if suffix.len() < scheme.zero_pad {
            Some("suffix_shorter_than_zero_pad")
        } else {
            None
        };
        if let Some(reason) = reason {
            warnings.push(RegistryDefaultIdSchemeWarning {
                code: "canonical_id_out_of_scheme".to_string(),
                canonical_id,
                reason: reason.to_string(),
            });
        }
    }
    warnings
}

fn build_registry_bytes(
    registry: &Path,
    registry_path: &Path,
    new_scheme: &DefaultIdScheme,
    version_after: &str,
) -> Result<Vec<u8>, Refusal> {
    let bytes =
        fs::read(registry_path).map_err(|error| add_entry::io_refusal(registry_path, error))?;
    let mut value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        Refusal::bad_registry(
            &registry.display().to_string(),
            &format!("Failed to parse registry.json: {error}"),
        )
    })?;
    let Some(object) = value.as_object_mut() else {
        return Err(Refusal::bad_registry(
            &registry.display().to_string(),
            "registry.json must be a JSON object",
        ));
    };
    object.insert(
        "version".to_string(),
        Value::String(version_after.to_string()),
    );
    object.insert(
        "default_id_scheme".to_string(),
        json!({
            "prefix": new_scheme.prefix,
            "zero_pad": new_scheme.zero_pad,
        }),
    );
    add_entry::to_pretty_bytes(&value, registry)
}
