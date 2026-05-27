use super::{DefaultIdScheme, MappingFile, load_registry_definition};
use crate::{Refusal, RefusalCode, RegistryMeta};
use serde::Serialize;
use serde_json::json;
use std::{collections::BTreeSet, path::PathBuf};

const DEFAULT_ZERO_PAD: usize = 3;

#[derive(Debug, Clone)]
pub struct RegistryNextIdRequest {
    pub registry: PathBuf,
    pub prefix: Option<String>,
    pub zero_pad: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryNextIdScheme {
    pub prefix: String,
    pub zero_pad: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryNextIdOutput {
    pub version: String,
    pub registry: RegistryMeta,
    pub scheme: RegistryNextIdScheme,
    pub current_max: Option<String>,
    pub next_id: String,
    pub entry_count_matching_prefix: usize,
    pub warnings: Vec<String>,
}

impl RegistryNextIdOutput {
    pub fn render_plain(&self) -> &str {
        &self.next_id
    }
}

pub fn next_id(request: RegistryNextIdRequest) -> Result<RegistryNextIdOutput, Refusal> {
    let (registry_json, registry_meta, mapping_files) = load_registry_definition(&request.registry)
        .map_err(|error| {
            Refusal::bad_registry(&request.registry.display().to_string(), &error.to_string())
        })?;

    let scheme = resolve_scheme(
        &request.registry,
        request.prefix.as_deref(),
        request.zero_pad,
        registry_json.default_id_scheme.as_ref(),
    )?;
    let allocation = allocate_next(&request.registry, &scheme, &mapping_files)?;

    Ok(RegistryNextIdOutput {
        version: "canon_registry_next_id.v0".to_string(),
        registry: registry_meta,
        scheme,
        current_max: allocation.current_max,
        next_id: allocation.next_id,
        entry_count_matching_prefix: allocation.entry_count_matching_prefix,
        warnings: Vec::new(),
    })
}

fn resolve_scheme(
    registry: &std::path::Path,
    prefix: Option<&str>,
    zero_pad: Option<usize>,
    default_id_scheme: Option<&DefaultIdScheme>,
) -> Result<RegistryNextIdScheme, Refusal> {
    let (prefix, zero_pad) = match prefix {
        Some(prefix) => (prefix.to_string(), zero_pad.unwrap_or(DEFAULT_ZERO_PAD)),
        None => {
            let Some(scheme) = default_id_scheme else {
                return Err(Refusal {
                    code: RefusalCode::EParse,
                    message: "canon registry next-id requires PREFIX when registry.json has no default_id_scheme".to_string(),
                    detail: json!({
                        "registry": registry.display().to_string(),
                    }),
                    next_command: Some(format!(
                        "canon registry next-id PPL --registry {}",
                        registry.display()
                    )),
                });
            };
            (scheme.prefix.clone(), zero_pad.unwrap_or(scheme.zero_pad))
        }
    };

    validate_prefix(registry, &prefix)?;
    if zero_pad == 0 {
        return Err(Refusal {
            code: RefusalCode::EParse,
            message: "--zero-pad must be greater than zero".to_string(),
            detail: json!({
                "registry": registry.display().to_string(),
                "zero_pad": zero_pad,
            }),
            next_command: Some(format!(
                "canon registry next-id {} --registry {} --zero-pad {}",
                prefix,
                registry.display(),
                DEFAULT_ZERO_PAD
            )),
        });
    }

    Ok(RegistryNextIdScheme { prefix, zero_pad })
}

fn validate_prefix(registry: &std::path::Path, prefix: &str) -> Result<(), Refusal> {
    let mut chars = prefix.chars();
    let valid = chars.next().is_some_and(|first| first.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit());

    if valid {
        return Ok(());
    }

    Err(Refusal {
        code: RefusalCode::EParse,
        message: format!("Invalid registry ID prefix '{prefix}'"),
        detail: json!({
            "registry": registry.display().to_string(),
            "prefix": prefix,
            "expected": "^[A-Z][A-Z0-9]*$",
        }),
        next_command: Some(format!(
            "canon registry next-id PPL --registry {}",
            registry.display()
        )),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Allocation {
    current_max: Option<String>,
    next_id: String,
    entry_count_matching_prefix: usize,
}

fn allocate_next(
    registry: &std::path::Path,
    scheme: &RegistryNextIdScheme,
    mapping_files: &[MappingFile],
) -> Result<Allocation, Refusal> {
    let namespace = format!("{}-", scheme.prefix);
    let mut distinct_ids = BTreeSet::new();
    for mapping_file in mapping_files {
        for entry in &mapping_file.entries {
            distinct_ids.insert(entry.canonical_id.clone());
        }
    }

    let mut current_max_number: Option<u128> = None;
    let mut current_max = None;
    let mut entry_count_matching_prefix = 0usize;

    for canonical_id in distinct_ids {
        let Some(suffix) = canonical_id.strip_prefix(&namespace) else {
            continue;
        };

        if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(malformed_namespace_refusal(
                registry,
                &scheme.prefix,
                &canonical_id,
            ));
        }

        let number = suffix
            .parse::<u128>()
            .map_err(|_| malformed_namespace_refusal(registry, &scheme.prefix, &canonical_id))?;
        entry_count_matching_prefix += 1;

        if current_max_number.is_none_or(|max| number > max) {
            current_max_number = Some(number);
            current_max = Some(canonical_id);
        }
    }

    let next_number = match current_max_number {
        Some(number) => number.checked_add(1).ok_or_else(|| Refusal {
            code: RefusalCode::EBadRegistry,
            message: format!(
                "Cannot allocate next ID for prefix '{}' because the namespace is exhausted",
                scheme.prefix
            ),
            detail: json!({
                "registry": registry.display().to_string(),
                "prefix": scheme.prefix,
            }),
            next_command: Some(
                "Choose a different prefix or repair the malformed registry namespace".to_string(),
            ),
        })?,
        None => 1,
    };
    let next_id = format!(
        "{}-{:0width$}",
        scheme.prefix,
        next_number,
        width = scheme.zero_pad
    );

    Ok(Allocation {
        current_max,
        next_id,
        entry_count_matching_prefix,
    })
}

fn malformed_namespace_refusal(
    registry: &std::path::Path,
    prefix: &str,
    canonical_id: &str,
) -> Refusal {
    Refusal {
        code: RefusalCode::EBadRegistry,
        message: format!(
            "Canonical ID '{canonical_id}' is malformed inside the '{prefix}' namespace"
        ),
        detail: json!({
            "registry": registry.display().to_string(),
            "prefix": prefix,
            "canonical_id": canonical_id,
            "expected": format!("{prefix}-<digits>"),
        }),
        next_command: Some(format!(
            "Fix malformed {prefix}-* canonical IDs, then rerun canon registry next-id {prefix} --registry {}",
            registry.display()
        )),
    }
}
