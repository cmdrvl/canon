#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
};

pub const CANON_PROJECT_SCHEMA_VERSION: &str = "canon.project.v1";

const ALLOWED_SECRET_HANDLE_PREFIXES: [&str; 5] =
    ["env:", "keyring:", "op://", "aws-sm://", "gcp-sm://"];

pub fn project_manifest_schema_version() -> &'static str {
    CANON_PROJECT_SCHEMA_VERSION
}

pub type ProjectResult<T> = Result<T, ProjectManifestError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectManifestErrorCode {
    ArtifactContract,
    UnknownField,
    PathPolicy,
    SecretPolicy,
    CompatibilityPolicy,
    EnvironmentInterpolation,
    Parse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectManifestError {
    pub code: ProjectManifestErrorCode,
    pub message: String,
}

impl ProjectManifestError {
    pub fn new(code: ProjectManifestErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ProjectManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ProjectManifestError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProjectPackageKind {
    #[serde(rename = "registry_package")]
    Registry,
    #[serde(rename = "strategy_package")]
    Strategy,
    #[serde(rename = "entity_profile_package")]
    EntityProfile,
    #[serde(rename = "source_mapping_package")]
    SourceMapping,
    #[serde(rename = "extension_package")]
    Extension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSourceFormat {
    Csv,
    Tsv,
    Jsonl,
    Parquet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectModeKind {
    Cluster,
    Link,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectOutputKind {
    SummaryJson,
    ReviewQueueCsv,
    RegistrySnapshotJson,
    ArtifactBundleDir,
    DiagnosticsJsonl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectNetworkPolicy {
    DenyAll,
    AllowDeclaredHosts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPackageBinding {
    pub alias: String,
    pub kind: ProjectPackageKind,
    pub id: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSourceDeclaration {
    pub source_id: String,
    pub path: String,
    pub format: ProjectSourceFormat,
    pub mapping_package: String,
    pub mapping_profile: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectExecutionMode {
    pub mode_id: String,
    pub kind: ProjectModeKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_ids: Vec<String>,
    pub registry_package: String,
    pub strategy_package: String,
    pub profile_package: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectOutputDeclaration {
    pub output_id: String,
    pub kind: ProjectOutputKind,
    pub path: String,
    pub redact_identity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectReviewThresholds {
    pub cannot_link_max_score_basis_points: u64,
    pub review_required_min_score_basis_points: u64,
    pub auto_promote_min_score_basis_points: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectTemporalScope {
    pub valid_at: String,
    pub known_as_of: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectResourceBudgets {
    pub max_input_bytes: u64,
    pub max_rows: u64,
    pub max_candidates: u64,
    pub max_review_items: u64,
    pub max_runtime_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRuntimePolicy {
    pub offline_build_only: bool,
    pub network_policy: ProjectNetworkPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declared_hosts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProjectSecretHandle {
    pub name: String,
    pub handle: String,
    pub purpose: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectExtensionActivation {
    pub extension_id: String,
    pub package: String,
    pub entrypoint: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mode_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub schema_version: String,
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<ProjectPackageBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<ProjectSourceDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modes: Vec<ProjectExecutionMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<ProjectOutputDeclaration>,
    pub review: ProjectReviewThresholds,
    pub temporal: ProjectTemporalScope,
    pub budgets: ProjectResourceBudgets,
    pub runtime: ProjectRuntimePolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<ProjectSecretHandle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ProjectExtensionActivation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectResolvedSource {
    pub source_id: String,
    pub path: String,
    pub format: ProjectSourceFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectResolvedOutput {
    pub output_id: String,
    pub kind: ProjectOutputKind,
    pub path: String,
    pub redact_identity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectResolvedMode {
    pub mode_id: String,
    pub kind: ProjectModeKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_ids: Vec<String>,
    pub registry_package: String,
    pub strategy_package: String,
    pub profile_package: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectResolvedExtension {
    pub extension_id: String,
    pub package: String,
    pub entrypoint: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mode_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProjectRedactedSecretHandle {
    pub name: String,
    pub purpose: String,
    pub handle: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectManifestProjection {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<ProjectResolvedSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<ProjectResolvedOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modes: Vec<ProjectResolvedMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ProjectResolvedExtension>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_secrets: Vec<ProjectRedactedSecretHandle>,
}

pub fn load_project_manifest_toml(input: &str) -> ProjectResult<ProjectManifest> {
    let raw = parse_raw_project_toml(input)?;
    build_project_manifest(raw)
}

pub fn canonical_project_manifest_bytes(manifest: &ProjectManifest) -> ProjectResult<Vec<u8>> {
    let mut canonical = finalize_project_manifest(manifest.clone())?;
    sort_manifest(&mut canonical);
    serde_json::to_vec(&canonical).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize canonical project manifest: {error}"
        ))
    })
}

pub fn project_manifest_digest(manifest: &ProjectManifest) -> ProjectResult<String> {
    let bytes = canonical_project_manifest_bytes(manifest)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn project_manifest_projection(
    manifest: &ProjectManifest,
    manifest_path: &Path,
    env: &BTreeMap<String, String>,
) -> ProjectResult<ProjectManifestProjection> {
    let manifest = finalize_project_manifest(manifest.clone())?;
    let base_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));

    let mut sources = manifest
        .sources
        .iter()
        .map(|source| {
            Ok(ProjectResolvedSource {
                source_id: source.source_id.clone(),
                path: resolve_relative_path(&source.path, "sources.path", base_dir, env)?,
                format: source.format,
            })
        })
        .collect::<ProjectResult<Vec<_>>>()?;
    let mut outputs = manifest
        .outputs
        .iter()
        .map(|output| {
            Ok(ProjectResolvedOutput {
                output_id: output.output_id.clone(),
                kind: output.kind,
                path: resolve_relative_path(&output.path, "outputs.path", base_dir, env)?,
                redact_identity: output.redact_identity,
            })
        })
        .collect::<ProjectResult<Vec<_>>>()?;
    let mut extensions = manifest
        .extensions
        .iter()
        .map(|extension| {
            Ok(ProjectResolvedExtension {
                extension_id: extension.extension_id.clone(),
                package: extension.package.clone(),
                entrypoint: extension.entrypoint.clone(),
                mode_ids: extension.mode_ids.clone(),
                config_path: extension
                    .config_path
                    .as_deref()
                    .map(|path| {
                        resolve_relative_path(path, "extensions.config_path", base_dir, env)
                    })
                    .transpose()?,
            })
        })
        .collect::<ProjectResult<Vec<_>>>()?;
    let mut redacted_secrets = manifest
        .secrets
        .iter()
        .map(|secret| ProjectRedactedSecretHandle {
            name: secret.name.clone(),
            purpose: secret.purpose.clone(),
            handle: redact_secret_handle(&secret.handle),
        })
        .collect::<Vec<_>>();
    let mut modes = manifest
        .modes
        .iter()
        .map(|mode| ProjectResolvedMode {
            mode_id: mode.mode_id.clone(),
            kind: mode.kind,
            source_ids: mode.source_ids.clone(),
            registry_package: mode.registry_package.clone(),
            strategy_package: mode.strategy_package.clone(),
            profile_package: mode.profile_package.clone(),
            output_ids: mode.output_ids.clone(),
        })
        .collect::<Vec<_>>();

    ensure_unique_resolved_paths(
        sources
            .iter()
            .map(|source| (source.source_id.as_str(), source.path.as_str())),
        "source path",
    )?;
    ensure_unique_resolved_paths(
        outputs
            .iter()
            .map(|output| (output.output_id.as_str(), output.path.as_str())),
        "output path",
    )?;
    ensure_unique_optional_paths(
        extensions.iter().filter_map(|extension| {
            extension
                .config_path
                .as_deref()
                .map(|path| (extension.extension_id.as_str(), path))
        }),
        "extension config path",
    )?;

    sources.sort_by(|left, right| left.source_id.cmp(&right.source_id));
    outputs.sort_by(|left, right| left.output_id.cmp(&right.output_id));
    extensions.sort_by(|left, right| left.extension_id.cmp(&right.extension_id));
    redacted_secrets.sort_by(|left, right| left.name.cmp(&right.name));
    modes.sort_by(|left, right| left.mode_id.cmp(&right.mode_id));

    Ok(ProjectManifestProjection {
        project_id: manifest.project_id,
        sources,
        outputs,
        modes,
        extensions,
        redacted_secrets,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SimpleTomlValue {
    String(String),
    Integer(u64),
    Bool(bool),
    Array(Vec<SimpleTomlValue>),
}

impl SimpleTomlValue {
    fn into_string(self, field: &str) -> ProjectResult<String> {
        match self {
            Self::String(value) => Ok(value),
            _ => Err(parse_error(format!("{field} must be a quoted string"))),
        }
    }

    fn into_u64(self, field: &str) -> ProjectResult<u64> {
        match self {
            Self::Integer(value) => Ok(value),
            _ => Err(parse_error(format!("{field} must be an integer"))),
        }
    }

    fn into_bool(self, field: &str) -> ProjectResult<bool> {
        match self {
            Self::Bool(value) => Ok(value),
            _ => Err(parse_error(format!("{field} must be a bool"))),
        }
    }

    fn into_string_vec(self, field: &str) -> ProjectResult<Vec<String>> {
        match self {
            Self::Array(values) => values
                .into_iter()
                .enumerate()
                .map(|(index, value)| value.into_string(&format!("{field}[{index}]")))
                .collect(),
            _ => Err(parse_error(format!(
                "{field} must be an array of quoted strings"
            ))),
        }
    }
}

#[derive(Debug, Default)]
struct RawProjectToml {
    top_level: BTreeMap<String, SimpleTomlValue>,
    tables: BTreeMap<String, BTreeMap<String, SimpleTomlValue>>,
    arrays: BTreeMap<String, Vec<BTreeMap<String, SimpleTomlValue>>>,
}

#[derive(Debug, Clone)]
enum CurrentSection {
    TopLevel,
    Table(String),
    ArrayTable { name: String, index: usize },
}

fn parse_raw_project_toml(input: &str) -> ProjectResult<RawProjectToml> {
    let mut document = RawProjectToml::default();
    let mut current = CurrentSection::TopLevel;

    for (line_index, raw_line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with("[[") {
            if !line.ends_with("]]") {
                return Err(parse_error(format!(
                    "line {line_number} has an unterminated array-of-table header"
                )));
            }
            let name = line[2..line.len() - 2].trim();
            if !matches!(
                name,
                "packages" | "sources" | "modes" | "outputs" | "secrets" | "extensions"
            ) {
                return Err(unknown_field_error(format!(
                    "line {line_number} declares unsupported array section [[{name}]]"
                )));
            }
            let entries = document.arrays.entry(name.to_string()).or_default();
            entries.push(BTreeMap::new());
            current = CurrentSection::ArrayTable {
                name: name.to_string(),
                index: entries.len() - 1,
            };
            continue;
        }

        if line.starts_with('[') {
            if !line.ends_with(']') {
                return Err(parse_error(format!(
                    "line {line_number} has an unterminated table header"
                )));
            }
            let name = line[1..line.len() - 1].trim();
            if !matches!(name, "review" | "temporal" | "budgets" | "runtime") {
                return Err(unknown_field_error(format!(
                    "line {line_number} declares unsupported table [{name}]"
                )));
            }
            document.tables.entry(name.to_string()).or_default();
            current = CurrentSection::Table(name.to_string());
            continue;
        }

        let (key, value) = line.split_once('=').ok_or_else(|| {
            parse_error(format!(
                "line {line_number} must use `key = value` syntax in the supported manifest subset"
            ))
        })?;
        let key = key.trim();
        if key.is_empty() || key.contains('.') {
            return Err(parse_error(format!(
                "line {line_number} uses unsupported key `{key}`"
            )));
        }
        let value = parse_simple_toml_value(value.trim(), line_number)?;
        match &current {
            CurrentSection::TopLevel => {
                insert_unique_key(&mut document.top_level, key, value, line_number)?
            }
            CurrentSection::Table(name) => insert_unique_key(
                document.tables.get_mut(name).expect("table exists"),
                key,
                value,
                line_number,
            )?,
            CurrentSection::ArrayTable { name, index } => insert_unique_key(
                document
                    .arrays
                    .get_mut(name)
                    .and_then(|entries| entries.get_mut(*index))
                    .expect("array table exists"),
                key,
                value,
                line_number,
            )?,
        }
    }

    Ok(document)
}

fn parse_simple_toml_value(value: &str, line_number: usize) -> ProjectResult<SimpleTomlValue> {
    if value.starts_with('"') {
        return Ok(SimpleTomlValue::String(parse_quoted_string(
            value,
            line_number,
        )?));
    }
    if value == "true" {
        return Ok(SimpleTomlValue::Bool(true));
    }
    if value == "false" {
        return Ok(SimpleTomlValue::Bool(false));
    }
    if value.starts_with('[') {
        if !value.ends_with(']') {
            return Err(parse_error(format!(
                "line {line_number} has an unterminated array literal"
            )));
        }
        let inner = value[1..value.len() - 1].trim();
        if inner.is_empty() {
            return Ok(SimpleTomlValue::Array(Vec::new()));
        }
        let items = split_array_items(inner, line_number)?;
        let values = items
            .into_iter()
            .map(|item| parse_simple_toml_value(item.trim(), line_number))
            .collect::<ProjectResult<Vec<_>>>()?;
        return Ok(SimpleTomlValue::Array(values));
    }
    if value.bytes().all(|byte| byte.is_ascii_digit()) {
        return value
            .parse::<u64>()
            .map(SimpleTomlValue::Integer)
            .map_err(|error| {
                parse_error(format!("line {line_number} integer parse failed: {error}"))
            });
    }
    Err(parse_error(format!(
        "line {line_number} uses unsupported TOML value `{value}`"
    )))
}

fn split_array_items(value: &str, line_number: usize) -> ProjectResult<Vec<String>> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escape = false;

    for character in value.chars() {
        if in_string {
            current.push(character);
            if escape {
                escape = false;
            } else if character == '\\' {
                escape = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        match character {
            '"' => {
                in_string = true;
                current.push(character);
            }
            ',' => {
                items.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(character),
        }
    }

    if in_string {
        return Err(parse_error(format!(
            "line {line_number} has an unterminated string inside an array"
        )));
    }
    items.push(current.trim().to_string());
    Ok(items)
}

fn parse_quoted_string(value: &str, line_number: usize) -> ProjectResult<String> {
    if !value.ends_with('"') || value.len() < 2 {
        return Err(parse_error(format!(
            "line {line_number} has an unterminated quoted string"
        )));
    }
    let mut parsed = String::new();
    let mut escape = false;
    for character in value[1..value.len() - 1].chars() {
        if escape {
            let escaped = match character {
                '"' => '"',
                '\\' => '\\',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => {
                    return Err(parse_error(format!(
                        "line {line_number} uses unsupported escape sequence \\{other}"
                    )));
                }
            };
            parsed.push(escaped);
            escape = false;
            continue;
        }
        if character == '\\' {
            escape = true;
        } else {
            parsed.push(character);
        }
    }
    if escape {
        return Err(parse_error(format!(
            "line {line_number} ends with an incomplete escape sequence"
        )));
    }
    Ok(parsed)
}

fn insert_unique_key(
    map: &mut BTreeMap<String, SimpleTomlValue>,
    key: &str,
    value: SimpleTomlValue,
    line_number: usize,
) -> ProjectResult<()> {
    if map.insert(key.to_string(), value).is_some() {
        return Err(parse_error(format!(
            "line {line_number} redeclares key `{key}`"
        )));
    }
    Ok(())
}

fn build_project_manifest(raw: RawProjectToml) -> ProjectResult<ProjectManifest> {
    let RawProjectToml {
        mut top_level,
        mut tables,
        mut arrays,
    } = raw;

    let schema_version = take_required_string(&mut top_level, "schema_version", "top_level")?;
    let project_id = take_required_string(&mut top_level, "project_id", "top_level")?;
    ensure_empty(&top_level, "top-level manifest fields")?;

    let review = build_review_thresholds(
        tables
            .remove("review")
            .ok_or_else(|| artifact_contract_error("manifest must declare a [review] table"))?,
    )?;
    let temporal = build_temporal_scope(
        tables
            .remove("temporal")
            .ok_or_else(|| artifact_contract_error("manifest must declare a [temporal] table"))?,
    )?;
    let budgets = build_resource_budgets(
        tables
            .remove("budgets")
            .ok_or_else(|| artifact_contract_error("manifest must declare a [budgets] table"))?,
    )?;
    let runtime = build_runtime_policy(
        tables
            .remove("runtime")
            .ok_or_else(|| artifact_contract_error("manifest must declare a [runtime] table"))?,
    )?;
    ensure_empty(&tables, "manifest tables")?;

    let packages = arrays
        .remove("packages")
        .ok_or_else(|| {
            artifact_contract_error("manifest must declare at least one [[packages]] entry")
        })?
        .into_iter()
        .map(build_package_binding)
        .collect::<ProjectResult<Vec<_>>>()?;
    let sources = arrays
        .remove("sources")
        .ok_or_else(|| {
            artifact_contract_error("manifest must declare at least one [[sources]] entry")
        })?
        .into_iter()
        .map(build_source_declaration)
        .collect::<ProjectResult<Vec<_>>>()?;
    let modes = arrays
        .remove("modes")
        .ok_or_else(|| {
            artifact_contract_error("manifest must declare at least one [[modes]] entry")
        })?
        .into_iter()
        .map(build_execution_mode)
        .collect::<ProjectResult<Vec<_>>>()?;
    let outputs = arrays
        .remove("outputs")
        .ok_or_else(|| {
            artifact_contract_error("manifest must declare at least one [[outputs]] entry")
        })?
        .into_iter()
        .map(build_output_declaration)
        .collect::<ProjectResult<Vec<_>>>()?;
    let secrets = arrays
        .remove("secrets")
        .unwrap_or_default()
        .into_iter()
        .map(build_secret_handle)
        .collect::<ProjectResult<Vec<_>>>()?;
    let extensions = arrays
        .remove("extensions")
        .unwrap_or_default()
        .into_iter()
        .map(build_extension_activation)
        .collect::<ProjectResult<Vec<_>>>()?;
    ensure_empty(&arrays, "manifest array sections")?;

    finalize_project_manifest(ProjectManifest {
        schema_version,
        project_id,
        packages,
        sources,
        modes,
        outputs,
        review,
        temporal,
        budgets,
        runtime,
        secrets,
        extensions,
    })
}

pub fn finalize_project_manifest(mut manifest: ProjectManifest) -> ProjectResult<ProjectManifest> {
    manifest.schema_version = normalized_non_empty(&manifest.schema_version, "schema_version")?;
    if manifest.schema_version != CANON_PROJECT_SCHEMA_VERSION {
        return Err(compatibility_policy_error(format!(
            "unsupported project schema version: {}",
            manifest.schema_version
        )));
    }
    manifest.project_id = normalized_package_id(&manifest.project_id, "project_id")?;

    if manifest.packages.is_empty() {
        return Err(artifact_contract_error(
            "manifest must declare at least one package alias",
        ));
    }
    if manifest.sources.is_empty() {
        return Err(artifact_contract_error(
            "manifest must declare at least one source",
        ));
    }
    if manifest.modes.is_empty() {
        return Err(artifact_contract_error(
            "manifest must declare at least one execution mode",
        ));
    }
    if manifest.outputs.is_empty() {
        return Err(artifact_contract_error(
            "manifest must declare at least one output",
        ));
    }

    for package in &mut manifest.packages {
        package.alias = normalized_package_id(&package.alias, "packages.alias")?;
        package.id = normalized_package_id(&package.id, "packages.id")?;
        package.version = normalized_semver(&package.version, "packages.version")?;
        package.content_hash = normalized_hash(&package.content_hash, "packages.content_hash")?;
    }
    for source in &mut manifest.sources {
        source.source_id = normalized_package_id(&source.source_id, "sources.source_id")?;
        source.path = normalized_non_empty(&source.path, "sources.path")?;
        source.mapping_package =
            normalized_package_id(&source.mapping_package, "sources.mapping_package")?;
        source.mapping_profile =
            normalized_opaque_ref(&source.mapping_profile, "sources.mapping_profile")?;
    }
    for mode in &mut manifest.modes {
        mode.mode_id = normalized_package_id(&mode.mode_id, "modes.mode_id")?;
        mode.source_ids =
            normalize_unique_package_ids(mode.source_ids.clone(), "modes.source_ids")?;
        mode.registry_package =
            normalized_package_id(&mode.registry_package, "modes.registry_package")?;
        mode.strategy_package =
            normalized_package_id(&mode.strategy_package, "modes.strategy_package")?;
        mode.profile_package =
            normalized_package_id(&mode.profile_package, "modes.profile_package")?;
        mode.output_ids =
            normalize_unique_package_ids(mode.output_ids.clone(), "modes.output_ids")?;
    }
    for output in &mut manifest.outputs {
        output.output_id = normalized_package_id(&output.output_id, "outputs.output_id")?;
        output.path = normalized_non_empty(&output.path, "outputs.path")?;
    }
    for secret in &mut manifest.secrets {
        secret.name = normalized_package_id(&secret.name, "secrets.name")?;
        secret.purpose = normalized_non_empty(&secret.purpose, "secrets.purpose")?;
        secret.handle = normalized_secret_handle(&secret.handle, "secrets.handle")?;
    }
    for extension in &mut manifest.extensions {
        extension.extension_id =
            normalized_package_id(&extension.extension_id, "extensions.extension_id")?;
        extension.package = normalized_package_id(&extension.package, "extensions.package")?;
        extension.entrypoint =
            normalized_opaque_ref(&extension.entrypoint, "extensions.entrypoint")?;
        extension.mode_ids =
            normalize_unique_package_ids(extension.mode_ids.clone(), "extensions.mode_ids")?;
        extension.config_path = extension
            .config_path
            .as_deref()
            .map(|path| normalized_non_empty(path, "extensions.config_path"))
            .transpose()?;
    }

    manifest.temporal.valid_at =
        normalized_non_empty(&manifest.temporal.valid_at, "temporal.valid_at")?;
    manifest.temporal.known_as_of =
        normalized_non_empty(&manifest.temporal.known_as_of, "temporal.known_as_of")?;
    manifest.temporal.scope_ref = manifest
        .temporal
        .scope_ref
        .as_deref()
        .map(|scope| normalized_opaque_ref(scope, "temporal.scope_ref"))
        .transpose()?;

    validate_review_thresholds(&manifest.review)?;
    validate_resource_budgets(&manifest.budgets)?;
    validate_runtime_policy(&mut manifest.runtime)?;

    validate_unique_packages(&manifest.packages)?;
    validate_unique_sources(&manifest.sources)?;
    validate_unique_modes(&manifest.modes)?;
    validate_unique_outputs(&manifest.outputs)?;
    validate_unique_secrets(&manifest.secrets)?;
    validate_unique_extensions(&manifest.extensions)?;
    validate_cross_references(&manifest)?;

    sort_manifest(&mut manifest);
    Ok(manifest)
}

fn build_package_binding(
    mut fields: BTreeMap<String, SimpleTomlValue>,
) -> ProjectResult<ProjectPackageBinding> {
    let alias = take_required_string(&mut fields, "alias", "packages")?;
    let kind = parse_package_kind(take_required_string(&mut fields, "kind", "packages")?)?;
    let id = take_required_string(&mut fields, "id", "packages")?;
    let version = take_required_string(&mut fields, "version", "packages")?;
    let content_hash = take_required_string(&mut fields, "content_hash", "packages")?;
    ensure_empty(&fields, "packages")?;
    Ok(ProjectPackageBinding {
        alias,
        kind,
        id,
        version,
        content_hash,
    })
}

fn build_source_declaration(
    mut fields: BTreeMap<String, SimpleTomlValue>,
) -> ProjectResult<ProjectSourceDeclaration> {
    let source_id = take_required_string(&mut fields, "source_id", "sources")?;
    let path = take_required_string(&mut fields, "path", "sources")?;
    let format = parse_source_format(take_required_string(&mut fields, "format", "sources")?)?;
    let mapping_package = take_required_string(&mut fields, "mapping_package", "sources")?;
    let mapping_profile = take_required_string(&mut fields, "mapping_profile", "sources")?;
    let required = take_required_bool(&mut fields, "required", "sources")?;
    ensure_empty(&fields, "sources")?;
    Ok(ProjectSourceDeclaration {
        source_id,
        path,
        format,
        mapping_package,
        mapping_profile,
        required,
    })
}

fn build_execution_mode(
    mut fields: BTreeMap<String, SimpleTomlValue>,
) -> ProjectResult<ProjectExecutionMode> {
    let mode_id = take_required_string(&mut fields, "mode_id", "modes")?;
    let kind = parse_mode_kind(take_required_string(&mut fields, "kind", "modes")?)?;
    let source_ids = take_required_string_vec(&mut fields, "source_ids", "modes")?;
    let registry_package = take_required_string(&mut fields, "registry_package", "modes")?;
    let strategy_package = take_required_string(&mut fields, "strategy_package", "modes")?;
    let profile_package = take_required_string(&mut fields, "profile_package", "modes")?;
    let output_ids = take_required_string_vec(&mut fields, "output_ids", "modes")?;
    ensure_empty(&fields, "modes")?;
    Ok(ProjectExecutionMode {
        mode_id,
        kind,
        source_ids,
        registry_package,
        strategy_package,
        profile_package,
        output_ids,
    })
}

fn build_output_declaration(
    mut fields: BTreeMap<String, SimpleTomlValue>,
) -> ProjectResult<ProjectOutputDeclaration> {
    let output_id = take_required_string(&mut fields, "output_id", "outputs")?;
    let kind = parse_output_kind(take_required_string(&mut fields, "kind", "outputs")?)?;
    let path = take_required_string(&mut fields, "path", "outputs")?;
    let redact_identity = take_required_bool(&mut fields, "redact_identity", "outputs")?;
    ensure_empty(&fields, "outputs")?;
    Ok(ProjectOutputDeclaration {
        output_id,
        kind,
        path,
        redact_identity,
    })
}

fn build_review_thresholds(
    mut fields: BTreeMap<String, SimpleTomlValue>,
) -> ProjectResult<ProjectReviewThresholds> {
    let cannot_link_max_score_basis_points =
        take_required_u64(&mut fields, "cannot_link_max_score_basis_points", "review")?;
    let review_required_min_score_basis_points = take_required_u64(
        &mut fields,
        "review_required_min_score_basis_points",
        "review",
    )?;
    let auto_promote_min_score_basis_points =
        take_required_u64(&mut fields, "auto_promote_min_score_basis_points", "review")?;
    ensure_empty(&fields, "review")?;
    Ok(ProjectReviewThresholds {
        cannot_link_max_score_basis_points,
        review_required_min_score_basis_points,
        auto_promote_min_score_basis_points,
    })
}

fn build_temporal_scope(
    mut fields: BTreeMap<String, SimpleTomlValue>,
) -> ProjectResult<ProjectTemporalScope> {
    let valid_at = take_required_string(&mut fields, "valid_at", "temporal")?;
    let known_as_of = take_required_string(&mut fields, "known_as_of", "temporal")?;
    let scope_ref = take_optional_string(&mut fields, "scope_ref", "temporal")?;
    ensure_empty(&fields, "temporal")?;
    Ok(ProjectTemporalScope {
        valid_at,
        known_as_of,
        scope_ref,
    })
}

fn build_resource_budgets(
    mut fields: BTreeMap<String, SimpleTomlValue>,
) -> ProjectResult<ProjectResourceBudgets> {
    let max_input_bytes = take_required_u64(&mut fields, "max_input_bytes", "budgets")?;
    let max_rows = take_required_u64(&mut fields, "max_rows", "budgets")?;
    let max_candidates = take_required_u64(&mut fields, "max_candidates", "budgets")?;
    let max_review_items = take_required_u64(&mut fields, "max_review_items", "budgets")?;
    let max_runtime_seconds = take_required_u64(&mut fields, "max_runtime_seconds", "budgets")?;
    ensure_empty(&fields, "budgets")?;
    Ok(ProjectResourceBudgets {
        max_input_bytes,
        max_rows,
        max_candidates,
        max_review_items,
        max_runtime_seconds,
    })
}

fn build_runtime_policy(
    mut fields: BTreeMap<String, SimpleTomlValue>,
) -> ProjectResult<ProjectRuntimePolicy> {
    let offline_build_only = take_required_bool(&mut fields, "offline_build_only", "runtime")?;
    let network_policy = parse_network_policy(take_required_string(
        &mut fields,
        "network_policy",
        "runtime",
    )?)?;
    let declared_hosts = take_required_string_vec(&mut fields, "declared_hosts", "runtime")?;
    ensure_empty(&fields, "runtime")?;
    Ok(ProjectRuntimePolicy {
        offline_build_only,
        network_policy,
        declared_hosts,
    })
}

fn build_secret_handle(
    mut fields: BTreeMap<String, SimpleTomlValue>,
) -> ProjectResult<ProjectSecretHandle> {
    let name = take_required_string(&mut fields, "name", "secrets")?;
    let handle = take_required_string(&mut fields, "handle", "secrets")?;
    let purpose = take_required_string(&mut fields, "purpose", "secrets")?;
    ensure_empty(&fields, "secrets")?;
    Ok(ProjectSecretHandle {
        name,
        handle,
        purpose,
    })
}

fn build_extension_activation(
    mut fields: BTreeMap<String, SimpleTomlValue>,
) -> ProjectResult<ProjectExtensionActivation> {
    let extension_id = take_required_string(&mut fields, "extension_id", "extensions")?;
    let package = take_required_string(&mut fields, "package", "extensions")?;
    let entrypoint = take_required_string(&mut fields, "entrypoint", "extensions")?;
    let mode_ids = take_required_string_vec(&mut fields, "mode_ids", "extensions")?;
    let config_path = take_optional_string(&mut fields, "config_path", "extensions")?;
    ensure_empty(&fields, "extensions")?;
    Ok(ProjectExtensionActivation {
        extension_id,
        package,
        entrypoint,
        mode_ids,
        config_path,
    })
}

fn take_required_string(
    fields: &mut BTreeMap<String, SimpleTomlValue>,
    key: &str,
    context: &str,
) -> ProjectResult<String> {
    fields
        .remove(key)
        .ok_or_else(|| artifact_contract_error(format!("{context} must declare `{key}`")))?
        .into_string(&format!("{context}.{key}"))
}

fn take_optional_string(
    fields: &mut BTreeMap<String, SimpleTomlValue>,
    key: &str,
    context: &str,
) -> ProjectResult<Option<String>> {
    fields
        .remove(key)
        .map(|value| value.into_string(&format!("{context}.{key}")))
        .transpose()
}

fn take_required_bool(
    fields: &mut BTreeMap<String, SimpleTomlValue>,
    key: &str,
    context: &str,
) -> ProjectResult<bool> {
    fields
        .remove(key)
        .ok_or_else(|| artifact_contract_error(format!("{context} must declare `{key}`")))?
        .into_bool(&format!("{context}.{key}"))
}

fn take_required_u64(
    fields: &mut BTreeMap<String, SimpleTomlValue>,
    key: &str,
    context: &str,
) -> ProjectResult<u64> {
    fields
        .remove(key)
        .ok_or_else(|| artifact_contract_error(format!("{context} must declare `{key}`")))?
        .into_u64(&format!("{context}.{key}"))
}

fn take_required_string_vec(
    fields: &mut BTreeMap<String, SimpleTomlValue>,
    key: &str,
    context: &str,
) -> ProjectResult<Vec<String>> {
    fields
        .remove(key)
        .ok_or_else(|| artifact_contract_error(format!("{context} must declare `{key}`")))?
        .into_string_vec(&format!("{context}.{key}"))
}

fn ensure_empty<T>(fields: &BTreeMap<String, T>, context: &str) -> ProjectResult<()> {
    if let Some(first) = fields.keys().next() {
        return Err(unknown_field_error(format!(
            "{context} contains unknown field `{first}`"
        )));
    }
    Ok(())
}

fn parse_package_kind(value: String) -> ProjectResult<ProjectPackageKind> {
    match value.as_str() {
        "registry_package" => Ok(ProjectPackageKind::Registry),
        "strategy_package" => Ok(ProjectPackageKind::Strategy),
        "entity_profile_package" => Ok(ProjectPackageKind::EntityProfile),
        "source_mapping_package" => Ok(ProjectPackageKind::SourceMapping),
        "extension_package" => Ok(ProjectPackageKind::Extension),
        _ => Err(artifact_contract_error(format!(
            "unknown package kind {value}"
        ))),
    }
}

fn parse_source_format(value: String) -> ProjectResult<ProjectSourceFormat> {
    match value.as_str() {
        "csv" => Ok(ProjectSourceFormat::Csv),
        "tsv" => Ok(ProjectSourceFormat::Tsv),
        "jsonl" => Ok(ProjectSourceFormat::Jsonl),
        "parquet" => Ok(ProjectSourceFormat::Parquet),
        _ => Err(artifact_contract_error(format!(
            "unknown source format {value}"
        ))),
    }
}

fn parse_mode_kind(value: String) -> ProjectResult<ProjectModeKind> {
    match value.as_str() {
        "cluster" => Ok(ProjectModeKind::Cluster),
        "link" => Ok(ProjectModeKind::Link),
        _ => Err(artifact_contract_error(format!(
            "unknown execution mode kind {value}"
        ))),
    }
}

fn parse_output_kind(value: String) -> ProjectResult<ProjectOutputKind> {
    match value.as_str() {
        "summary_json" => Ok(ProjectOutputKind::SummaryJson),
        "review_queue_csv" => Ok(ProjectOutputKind::ReviewQueueCsv),
        "registry_snapshot_json" => Ok(ProjectOutputKind::RegistrySnapshotJson),
        "artifact_bundle_dir" => Ok(ProjectOutputKind::ArtifactBundleDir),
        "diagnostics_jsonl" => Ok(ProjectOutputKind::DiagnosticsJsonl),
        _ => Err(artifact_contract_error(format!(
            "unknown output kind {value}"
        ))),
    }
}

fn parse_network_policy(value: String) -> ProjectResult<ProjectNetworkPolicy> {
    match value.as_str() {
        "deny_all" => Ok(ProjectNetworkPolicy::DenyAll),
        "allow_declared_hosts" => Ok(ProjectNetworkPolicy::AllowDeclaredHosts),
        _ => Err(artifact_contract_error(format!(
            "unknown network policy {value}"
        ))),
    }
}

fn validate_review_thresholds(review: &ProjectReviewThresholds) -> ProjectResult<()> {
    for (field, value) in [
        (
            "cannot_link_max_score_basis_points",
            review.cannot_link_max_score_basis_points,
        ),
        (
            "review_required_min_score_basis_points",
            review.review_required_min_score_basis_points,
        ),
        (
            "auto_promote_min_score_basis_points",
            review.auto_promote_min_score_basis_points,
        ),
    ] {
        if value > 10_000 {
            return Err(compatibility_policy_error(format!(
                "review.{field} must be <= 10000 basis points"
            )));
        }
    }
    if review.cannot_link_max_score_basis_points >= review.review_required_min_score_basis_points {
        return Err(compatibility_policy_error(
            "review thresholds must satisfy cannot_link_max < review_required_min",
        ));
    }
    if review.review_required_min_score_basis_points > review.auto_promote_min_score_basis_points {
        return Err(compatibility_policy_error(
            "review thresholds must satisfy review_required_min <= auto_promote_min",
        ));
    }
    Ok(())
}

fn validate_resource_budgets(budgets: &ProjectResourceBudgets) -> ProjectResult<()> {
    for (field, value) in [
        ("max_input_bytes", budgets.max_input_bytes),
        ("max_rows", budgets.max_rows),
        ("max_candidates", budgets.max_candidates),
        ("max_review_items", budgets.max_review_items),
        ("max_runtime_seconds", budgets.max_runtime_seconds),
    ] {
        if value == 0 {
            return Err(artifact_contract_error(format!(
                "budgets.{field} must be greater than zero"
            )));
        }
    }
    Ok(())
}

fn validate_runtime_policy(runtime: &mut ProjectRuntimePolicy) -> ProjectResult<()> {
    runtime.declared_hosts = runtime
        .declared_hosts
        .iter()
        .map(|host| normalized_host(host, "runtime.declared_hosts"))
        .collect::<ProjectResult<Vec<_>>>()?;
    runtime.declared_hosts.sort();
    runtime.declared_hosts.dedup();

    if runtime.offline_build_only && runtime.network_policy != ProjectNetworkPolicy::DenyAll {
        return Err(compatibility_policy_error(
            "offline_build_only projects must use network_policy = deny_all",
        ));
    }
    match runtime.network_policy {
        ProjectNetworkPolicy::DenyAll => {
            if !runtime.declared_hosts.is_empty() {
                return Err(compatibility_policy_error(
                    "network_policy = deny_all cannot declare hosts",
                ));
            }
        }
        ProjectNetworkPolicy::AllowDeclaredHosts => {
            if runtime.declared_hosts.is_empty() {
                return Err(compatibility_policy_error(
                    "network_policy = allow_declared_hosts requires at least one declared host",
                ));
            }
        }
    }
    Ok(())
}

fn validate_unique_packages(packages: &[ProjectPackageBinding]) -> ProjectResult<()> {
    let mut aliases = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for package in packages {
        if !aliases.insert(package.alias.clone()) {
            return Err(compatibility_policy_error(format!(
                "package alias {} is declared more than once",
                package.alias
            )));
        }
        let key = (package.kind, package.id.clone(), package.version.clone());
        if !ids.insert(key) {
            return Err(compatibility_policy_error(format!(
                "package {}@{} of kind {:?} is declared more than once",
                package.id, package.version, package.kind
            )));
        }
    }
    Ok(())
}

fn validate_unique_sources(sources: &[ProjectSourceDeclaration]) -> ProjectResult<()> {
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for source in sources {
        if !ids.insert(source.source_id.clone()) {
            return Err(compatibility_policy_error(format!(
                "source_id {} is declared more than once",
                source.source_id
            )));
        }
        if !paths.insert(source.path.clone()) {
            return Err(compatibility_policy_error(format!(
                "source path {} is declared more than once",
                source.path
            )));
        }
    }
    Ok(())
}

fn validate_unique_modes(modes: &[ProjectExecutionMode]) -> ProjectResult<()> {
    let mut ids = BTreeSet::new();
    for mode in modes {
        if !ids.insert(mode.mode_id.clone()) {
            return Err(compatibility_policy_error(format!(
                "mode_id {} is declared more than once",
                mode.mode_id
            )));
        }
    }
    Ok(())
}

fn validate_unique_outputs(outputs: &[ProjectOutputDeclaration]) -> ProjectResult<()> {
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for output in outputs {
        if !ids.insert(output.output_id.clone()) {
            return Err(compatibility_policy_error(format!(
                "output_id {} is declared more than once",
                output.output_id
            )));
        }
        if !paths.insert(output.path.clone()) {
            return Err(compatibility_policy_error(format!(
                "output path {} is declared more than once",
                output.path
            )));
        }
    }
    Ok(())
}

fn validate_unique_secrets(secrets: &[ProjectSecretHandle]) -> ProjectResult<()> {
    let mut names = BTreeSet::new();
    for secret in secrets {
        if !names.insert(secret.name.clone()) {
            return Err(compatibility_policy_error(format!(
                "secret handle {} is declared more than once",
                secret.name
            )));
        }
    }
    Ok(())
}

fn validate_unique_extensions(extensions: &[ProjectExtensionActivation]) -> ProjectResult<()> {
    let mut ids = BTreeSet::new();
    for extension in extensions {
        if !ids.insert(extension.extension_id.clone()) {
            return Err(compatibility_policy_error(format!(
                "extension_id {} is declared more than once",
                extension.extension_id
            )));
        }
    }
    Ok(())
}

fn validate_cross_references(manifest: &ProjectManifest) -> ProjectResult<()> {
    let package_kinds = manifest
        .packages
        .iter()
        .map(|package| (package.alias.as_str(), package.kind))
        .collect::<BTreeMap<_, _>>();
    let source_ids = manifest
        .sources
        .iter()
        .map(|source| source.source_id.as_str())
        .collect::<BTreeSet<_>>();
    let output_ids = manifest
        .outputs
        .iter()
        .map(|output| output.output_id.as_str())
        .collect::<BTreeSet<_>>();
    let mode_ids = manifest
        .modes
        .iter()
        .map(|mode| mode.mode_id.as_str())
        .collect::<BTreeSet<_>>();

    for source in &manifest.sources {
        expect_package_kind(
            &package_kinds,
            &source.mapping_package,
            ProjectPackageKind::SourceMapping,
            "sources.mapping_package",
        )?;
    }

    for mode in &manifest.modes {
        if mode.source_ids.is_empty() {
            return Err(compatibility_policy_error(format!(
                "mode {} must declare at least one source_id",
                mode.mode_id
            )));
        }
        if mode.kind == ProjectModeKind::Link && mode.source_ids.len() < 2 {
            return Err(compatibility_policy_error(format!(
                "link mode {} must declare at least two source_ids",
                mode.mode_id
            )));
        }
        for source_id in &mode.source_ids {
            if !source_ids.contains(source_id.as_str()) {
                return Err(compatibility_policy_error(format!(
                    "mode {} references unknown source_id {}",
                    mode.mode_id, source_id
                )));
            }
        }
        if mode.output_ids.is_empty() {
            return Err(compatibility_policy_error(format!(
                "mode {} must declare at least one output_id",
                mode.mode_id
            )));
        }
        for output_id in &mode.output_ids {
            if !output_ids.contains(output_id.as_str()) {
                return Err(compatibility_policy_error(format!(
                    "mode {} references unknown output_id {}",
                    mode.mode_id, output_id
                )));
            }
        }
        expect_package_kind(
            &package_kinds,
            &mode.registry_package,
            ProjectPackageKind::Registry,
            "modes.registry_package",
        )?;
        expect_package_kind(
            &package_kinds,
            &mode.strategy_package,
            ProjectPackageKind::Strategy,
            "modes.strategy_package",
        )?;
        expect_package_kind(
            &package_kinds,
            &mode.profile_package,
            ProjectPackageKind::EntityProfile,
            "modes.profile_package",
        )?;
    }

    for extension in &manifest.extensions {
        expect_package_kind(
            &package_kinds,
            &extension.package,
            ProjectPackageKind::Extension,
            "extensions.package",
        )?;
        for mode_id in &extension.mode_ids {
            if !mode_ids.contains(mode_id.as_str()) {
                return Err(compatibility_policy_error(format!(
                    "extension {} references unknown mode_id {}",
                    extension.extension_id, mode_id
                )));
            }
        }
    }
    Ok(())
}

fn expect_package_kind(
    package_kinds: &BTreeMap<&str, ProjectPackageKind>,
    alias: &str,
    expected: ProjectPackageKind,
    field: &str,
) -> ProjectResult<()> {
    let actual = package_kinds.get(alias).ok_or_else(|| {
        compatibility_policy_error(format!("{field} references unknown package alias {alias}"))
    })?;
    if *actual != expected {
        return Err(compatibility_policy_error(format!(
            "{field} alias {alias} must reference a {:?}, found {:?}",
            expected, actual
        )));
    }
    Ok(())
}

fn ensure_unique_resolved_paths<'a, I>(entries: I, label: &str) -> ProjectResult<()>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut seen = BTreeMap::<&str, &str>::new();
    for (id, path) in entries {
        if let Some(previous) = seen.insert(path, id) {
            return Err(path_policy_error(format!(
                "{label} collision: {path} is claimed by both {previous} and {id}"
            )));
        }
    }
    Ok(())
}

fn ensure_unique_optional_paths<'a, I>(entries: I, label: &str) -> ProjectResult<()>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    ensure_unique_resolved_paths(entries, label)
}

fn resolve_relative_path(
    declared_path: &str,
    field: &str,
    base_dir: &Path,
    env: &BTreeMap<String, String>,
) -> ProjectResult<String> {
    let interpolated = interpolate_env(declared_path, field, env)?;
    let normalized = normalized_non_empty(&interpolated, field)?;
    let path = Path::new(&normalized);
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => relative.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(path_policy_error(format!(
                    "{field} must stay relative to the manifest and cannot escape it: {normalized}"
                )));
            }
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(path_policy_error(format!(
            "{field} must resolve to at least one relative segment"
        )));
    }
    Ok(base_dir.join(relative).to_string_lossy().into_owned())
}

fn interpolate_env(
    value: &str,
    field: &str,
    env: &BTreeMap<String, String>,
) -> ProjectResult<String> {
    let mut output = String::with_capacity(value.len());
    let chars = value.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
            let mut end = index + 2;
            while end < chars.len() && chars[end] != '}' {
                end += 1;
            }
            if end == chars.len() {
                return Err(environment_interpolation_error(format!(
                    "{field} contains an unterminated env placeholder"
                )));
            }
            let name = chars[index + 2..end].iter().collect::<String>();
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
            {
                return Err(environment_interpolation_error(format!(
                    "{field} contains invalid env placeholder ${{{name}}}"
                )));
            }
            let replacement = env.get(&name).ok_or_else(|| {
                environment_interpolation_error(format!(
                    "{field} references unset env placeholder ${{{name}}}"
                ))
            })?;
            output.push_str(replacement);
            index = end + 1;
            continue;
        }
        output.push(chars[index]);
        index += 1;
    }
    Ok(output)
}

fn sort_manifest(manifest: &mut ProjectManifest) {
    manifest
        .packages
        .sort_by(|left, right| left.alias.cmp(&right.alias));
    manifest
        .sources
        .sort_by(|left, right| left.source_id.cmp(&right.source_id));
    manifest
        .modes
        .sort_by(|left, right| left.mode_id.cmp(&right.mode_id));
    manifest
        .outputs
        .sort_by(|left, right| left.output_id.cmp(&right.output_id));
    manifest
        .secrets
        .sort_by(|left, right| left.name.cmp(&right.name));
    manifest
        .extensions
        .sort_by(|left, right| left.extension_id.cmp(&right.extension_id));
    manifest.runtime.declared_hosts.sort();
    for mode in &mut manifest.modes {
        mode.source_ids.sort();
        mode.output_ids.sort();
    }
    for extension in &mut manifest.extensions {
        extension.mode_ids.sort();
    }
}

fn normalized_non_empty(value: &str, field: &str) -> ProjectResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must be non-empty"
        )));
    }
    Ok(value)
}

fn normalized_package_id(value: &str, field: &str) -> ProjectResult<String> {
    let value = normalized_non_empty(value, field)?;
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return Err(artifact_contract_error(format!(
            "{field} must be non-empty"
        )));
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(artifact_contract_error(format!(
            "{field} must start with a lowercase letter or digit"
        )));
    }
    if bytes.all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must contain only lowercase letters, digits, '.', '_' or '-'"
    )))
}

fn normalized_semver(value: &str, field: &str) -> ProjectResult<String> {
    let value = normalized_non_empty(value, field)?;
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(artifact_contract_error(format!(
            "{field} must be semver major.minor.patch"
        )));
    }
    if parts
        .iter()
        .all(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must be semver major.minor.patch"
    )))
}

fn normalized_hash(value: &str, field: &str) -> ProjectResult<String> {
    let value = normalized_non_empty(value, field)?;
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must start with blake3:"
        )));
    };
    if hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must be a blake3 digest with 64 lowercase hex characters"
    )))
}

fn normalized_opaque_ref(value: &str, field: &str) -> ProjectResult<String> {
    let value = normalized_non_empty(value, field)?;
    let (namespace, identifier) = value
        .split_once(':')
        .ok_or_else(|| artifact_contract_error(format!("{field} must use namespace:id syntax")))?;
    normalized_package_id(namespace, field)?;
    normalized_package_id(identifier, field)?;
    Ok(value)
}

fn normalize_unique_package_ids(values: Vec<String>, field: &str) -> ProjectResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalized_package_id(&value, field))
        .collect::<ProjectResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must contain at least one item"
        )));
    }
    Ok(normalized)
}

fn normalized_host(value: &str, field: &str) -> ProjectResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.contains("://") || value.contains('/') || value.contains('\\') {
        return Err(artifact_contract_error(format!(
            "{field} must declare hostnames without scheme or path segments"
        )));
    }
    if value.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err(artifact_contract_error(format!(
            "{field} must not contain whitespace"
        )));
    }
    Ok(value)
}

fn normalized_secret_handle(value: &str, field: &str) -> ProjectResult<String> {
    let value = normalized_non_empty(value, field)?;
    if ALLOWED_SECRET_HANDLE_PREFIXES
        .iter()
        .any(|prefix| value.starts_with(prefix))
    {
        return Ok(value);
    }
    Err(secret_policy_error(format!(
        "{field} must use a typed external handle such as env:, keyring:, op://, aws-sm://, or gcp-sm://"
    )))
}

fn redact_secret_handle(handle: &str) -> String {
    for prefix in ALLOWED_SECRET_HANDLE_PREFIXES {
        if handle.starts_with(prefix) {
            return match prefix {
                "op://" => "op://[redacted]".to_string(),
                "aws-sm://" => "aws-sm://[redacted]".to_string(),
                "gcp-sm://" => "gcp-sm://[redacted]".to_string(),
                _ => format!("{prefix}[redacted]"),
            };
        }
    }
    "[redacted]".to_string()
}

fn artifact_contract_error(message: impl Into<String>) -> ProjectManifestError {
    ProjectManifestError::new(ProjectManifestErrorCode::ArtifactContract, message)
}

fn unknown_field_error(message: impl Into<String>) -> ProjectManifestError {
    ProjectManifestError::new(ProjectManifestErrorCode::UnknownField, message)
}

fn path_policy_error(message: impl Into<String>) -> ProjectManifestError {
    ProjectManifestError::new(ProjectManifestErrorCode::PathPolicy, message)
}

fn secret_policy_error(message: impl Into<String>) -> ProjectManifestError {
    ProjectManifestError::new(ProjectManifestErrorCode::SecretPolicy, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> ProjectManifestError {
    ProjectManifestError::new(ProjectManifestErrorCode::CompatibilityPolicy, message)
}

fn environment_interpolation_error(message: impl Into<String>) -> ProjectManifestError {
    ProjectManifestError::new(ProjectManifestErrorCode::EnvironmentInterpolation, message)
}

fn parse_error(message: impl Into<String>) -> ProjectManifestError {
    ProjectManifestError::new(ProjectManifestErrorCode::Parse, message)
}
