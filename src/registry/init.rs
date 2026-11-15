use super::{DefaultIdScheme, acquire_registry_mutation_guard};
use crate::{Refusal, RefusalCode, RegistryMeta, registry_lint};
use serde::Serialize;
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const DEFAULT_ZERO_PAD: usize = 3;
const MAX_ZERO_PAD: usize = 20;

#[derive(Debug, Clone)]
pub struct RegistryInitRequest {
    pub registry: PathBuf,
    pub registry_id: String,
    pub prefix: String,
    pub zero_pad: Option<usize>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryInitOutput {
    pub version: String,
    pub status: String,
    pub registry: RegistryMeta,
    pub description: String,
    pub default_id_scheme: DefaultIdScheme,
    pub entry_count: usize,
    pub files: Vec<String>,
    pub lint: RegistryInitLint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryInitLint {
    pub profile: String,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
}

pub fn init(request: RegistryInitRequest) -> Result<RegistryInitOutput, Refusal> {
    let registry = request.registry;
    let registry_id = validate_registry_id(&registry, &request.registry_id)?;
    let prefix = validate_prefix(&registry, &request.prefix)?;
    let zero_pad = validate_zero_pad(&registry, request.zero_pad.unwrap_or(DEFAULT_ZERO_PAD))?;
    let description = request
        .description
        .unwrap_or_else(|| format!("Self-authored {registry_id} registry"));

    let registry_path = registry.join("registry.json");
    let aliases_path = registry.join("aliases.json");

    let registry_was_missing = match fs::symlink_metadata(&registry) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(path_refusal(
                &registry,
                "Registry init refuses a symlinked registry directory",
                "Replace the symlink with a regular directory, then rerun",
            ));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(path_refusal(
                &registry,
                "Registry init requires a directory path",
                "Choose a directory for --registry, then rerun",
            ));
        }
        Ok(_) => false,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(&registry).map_err(|error| io_refusal(&registry, error))?;
            true
        }
        Err(error) => return Err(io_refusal(&registry, error)),
    };

    let _guard =
        acquire_registry_mutation_guard(&registry).map_err(|error| io_refusal(&registry, error))?;

    if fs::symlink_metadata(&registry_path).is_ok() {
        return Err(already_exists_refusal(&registry, "registry.json"));
    }
    if fs::symlink_metadata(&aliases_path).is_ok() {
        return Err(already_exists_refusal(&registry, "aliases.json"));
    }

    let default_id_scheme = DefaultIdScheme { prefix, zero_pad };
    let registry_json = json!({
        "default_id_scheme": default_id_scheme.clone(),
        "description": description.clone(),
        "entry_count": 0,
        "id": registry_id.clone(),
        "updated": chrono::Utc::now().format("%Y-%m-%d").to_string(),
        "version": "0.0.0"
    });
    let registry_bytes = pretty_json_bytes(&registry_json, &registry)?;
    let aliases_bytes = b"[]\n".to_vec();

    if let Err(error) = write_atomic(&registry_path, &registry_bytes) {
        if registry_was_missing {
            let _ = fs::remove_dir(&registry);
        }
        return Err(io_refusal(&registry_path, error));
    }
    if let Err(error) = write_atomic(&aliases_path, &aliases_bytes) {
        let _ = fs::remove_file(&registry_path);
        if registry_was_missing {
            let _ = fs::remove_dir(&registry);
        }
        return Err(io_refusal(&aliases_path, error));
    }

    let lint = match registry_lint::lint(&registry, registry_lint::RegistryLintProfile::Standard) {
        Ok(lint) if lint.summary.errors == 0 => RegistryInitLint {
            profile: lint.profile,
            errors: lint.summary.errors,
            warnings: lint.summary.warnings,
            info: lint.summary.info,
        },
        Ok(lint) => {
            let _ = fs::remove_file(&registry_path);
            let _ = fs::remove_file(&aliases_path);
            if registry_was_missing {
                let _ = fs::remove_dir(&registry);
            }
            return Err(Refusal {
                code: RefusalCode::EBadRegistry,
                message: "Registry init produced a registry that failed lint".to_string(),
                detail: json!({
                    "registry": registry.display().to_string(),
                    "lint": lint,
                    "writes_performed": false,
                }),
                next_command: Some(
                    "Fix the registry initialization inputs, then rerun canon registry init"
                        .to_string(),
                ),
            });
        }
        Err(refusal) => {
            let _ = fs::remove_file(&registry_path);
            let _ = fs::remove_file(&aliases_path);
            if registry_was_missing {
                let _ = fs::remove_dir(&registry);
            }
            return Err(refusal);
        }
    };

    Ok(RegistryInitOutput {
        version: "canon_registry_init.v0".to_string(),
        status: "initialized".to_string(),
        registry: RegistryMeta {
            id: registry_id,
            version: "0.0.0".to_string(),
            source: registry.display().to_string(),
        },
        description,
        default_id_scheme,
        entry_count: 0,
        files: vec!["registry.json".to_string(), "aliases.json".to_string()],
        lint,
    })
}

fn validate_registry_id(registry: &Path, value: &str) -> Result<String, Refusal> {
    let trimmed = value.trim();
    if !trimmed.is_empty()
        && trimmed == value
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Ok(trimmed.to_string());
    }
    Err(parse_refusal(
        registry,
        "Invalid --registry-id",
        json!({"registry_id": value, "expected": "a non-empty safe identifier"}),
        "Use --registry-id <ID> with letters, digits, '.', '_' or '-'",
    ))
}

fn validate_prefix(registry: &Path, value: &str) -> Result<String, Refusal> {
    let trimmed = value.trim();
    let mut chars = trimmed.chars();
    let valid = chars.next().is_some_and(|first| first.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit());
    if valid && trimmed == value {
        return Ok(trimmed.to_string());
    }
    Err(parse_refusal(
        registry,
        "Invalid --prefix",
        json!({"prefix": value, "expected": "^[A-Z][A-Z0-9]*$"}),
        "Use --prefix ORG with uppercase letters and digits",
    ))
}

fn validate_zero_pad(registry: &Path, value: usize) -> Result<usize, Refusal> {
    if (1..=MAX_ZERO_PAD).contains(&value) {
        return Ok(value);
    }
    Err(parse_refusal(
        registry,
        "--zero-pad must be between 1 and 20",
        json!({"zero_pad": value, "min": 1, "max": MAX_ZERO_PAD}),
        "Use --zero-pad 3, then rerun canon registry init",
    ))
}

fn already_exists_refusal(registry: &Path, file: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EParse,
        message: format!("Registry init refuses to overwrite existing {file}"),
        detail: json!({
            "registry": registry.display().to_string(),
            "path": registry.join(file).display().to_string(),
            "writes_performed": false,
        }),
        next_command: Some(
            "Use the existing registry or choose an empty --registry directory".to_string(),
        ),
    }
}

fn parse_refusal(registry: &Path, message: &str, detail: serde_json::Value, next: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EParse,
        message: message.to_string(),
        detail: json!({
            "registry": registry.display().to_string(),
            "cause": detail,
            "writes_performed": false,
        }),
        next_command: Some(next.to_string()),
    }
}

fn path_refusal(registry: &Path, message: &str, next: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EParse,
        message: message.to_string(),
        detail: json!({
            "registry": registry.display().to_string(),
            "writes_performed": false,
        }),
        next_command: Some(next.to_string()),
    }
}

fn io_refusal(path: &Path, error: std::io::Error) -> Refusal {
    Refusal {
        code: RefusalCode::EIo,
        message: format!(
            "Unable to initialize registry path '{}': {error}",
            path.display()
        ),
        detail: json!({
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false,
        }),
        next_command: Some(
            "Check paths and permissions, then rerun canon registry init".to_string(),
        ),
    }
}

fn pretty_json_bytes(value: &serde_json::Value, registry: &Path) -> Result<Vec<u8>, Refusal> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| Refusal {
        code: RefusalCode::EBadRegistry,
        message: "Failed to serialize registry metadata".to_string(),
        detail: json!({"registry": registry.display().to_string(), "error": error.to_string()}),
        next_command: Some("Fix registry initialization inputs, then rerun".to_string()),
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_path = path.with_file_name(format!(
        "{}.canon-init-{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
        unique
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    drop(file);
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tempfile::TempDir;

    fn request(registry: &Path) -> RegistryInitRequest {
        RegistryInitRequest {
            registry: registry.to_path_buf(),
            registry_id: "organisations".to_string(),
            prefix: "ORG".to_string(),
            zero_pad: Some(4),
            description: Some("Organisation registry".to_string()),
        }
    }

    #[test]
    fn init_writes_empty_registry_and_passes_standard_lint() {
        let temp = TempDir::new().expect("temp directory");
        let registry = temp.path().join("registry");

        let output = init(request(&registry)).expect("registry init");

        assert_eq!(output.registry.id, "organisations");
        assert_eq!(output.registry.version, "0.0.0");
        assert_eq!(output.default_id_scheme.prefix, "ORG");
        assert_eq!(output.default_id_scheme.zero_pad, 4);
        assert_eq!(output.entry_count, 0);
        assert_eq!(fs::read(registry.join("aliases.json")).unwrap(), b"[]\n");

        let registry_json: Value =
            serde_json::from_slice(&fs::read(registry.join("registry.json")).unwrap())
                .expect("registry JSON");
        assert_eq!(registry_json["entry_count"], 0);
        assert_eq!(registry_json["version"], "0.0.0");
        assert_eq!(registry_json["default_id_scheme"]["prefix"], "ORG");
        assert_eq!(registry_json["default_id_scheme"]["zero_pad"], 4);

        let lint = registry_lint::lint(&registry, registry_lint::RegistryLintProfile::Standard)
            .expect("standard lint");
        assert_eq!(lint.summary.errors, 0);
    }

    #[test]
    fn init_refuses_existing_registry_without_writing() {
        let temp = TempDir::new().expect("temp directory");
        let registry = temp.path();
        fs::write(
            registry.join("registry.json"),
            br#"{"id":"organisations","version":"1.2.3","description":"existing","updated":"2026-11-15","entry_count":3}"#,
        )
        .expect("existing registry JSON");
        fs::write(registry.join("aliases.json"), b"[]\n").expect("existing aliases");
        let before_registry = fs::read(registry.join("registry.json")).unwrap();
        let before_aliases = fs::read(registry.join("aliases.json")).unwrap();

        let refusal = init(request(registry)).expect_err("existing registry must refuse");

        assert_eq!(refusal.code, RefusalCode::EParse);
        assert_eq!(
            fs::read(registry.join("registry.json")).unwrap(),
            before_registry
        );
        assert_eq!(
            fs::read(registry.join("aliases.json")).unwrap(),
            before_aliases
        );
    }
}
