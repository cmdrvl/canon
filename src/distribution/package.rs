use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::Path,
};

pub const LOCAL_PACKAGE_ARCHIVE_SCHEMA_VERSION: &str = "canon.local.package.archive.v1";
pub const LOCAL_PACKAGE_DIRECTORY_LAYOUT: &str = "canon-local-package-dir.v1";
pub const LOCAL_PACKAGE_ARCHIVE_LAYOUT: &str = "canon-local-package-archive.v1";
pub const PACKAGE_BYTES_PATH: &str = "package.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPackageSubject {
    pub schema_version: String,
    pub package_id: String,
    pub package_version: String,
    pub content_digest: String,
    pub canonical_bytes_digest: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPackageFile {
    pub path: String,
    pub mode: u32,
    pub content_digest: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPackageBlob {
    pub descriptor: LocalPackageFile,
    pub data_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPackageDependency {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPackageProvenance {
    pub source_schema: String,
    pub package_id: String,
    pub package_version: String,
    pub content_digest: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub declared: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPackageLayouts {
    pub directory_layout: String,
    pub archive_layout: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPackageArchive {
    pub schema_version: String,
    pub archive_digest: String,
    pub package: LocalPackageSubject,
    pub inventory: Vec<LocalPackageFile>,
    pub provenance: LocalPackageProvenance,
    pub licenses: Vec<String>,
    pub dependencies: Vec<LocalPackageDependency>,
    pub capabilities: Vec<String>,
    pub layouts: LocalPackageLayouts,
    pub files: Vec<LocalPackageBlob>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPackageInspection {
    pub archive_digest: String,
    pub package: LocalPackageSubject,
    pub inventory: Vec<LocalPackageFile>,
    pub provenance: LocalPackageProvenance,
    pub licenses: Vec<String>,
    pub dependencies: Vec<LocalPackageDependency>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPackageVerification {
    pub archive_digest: String,
    pub package_content_digest: String,
    pub package_bytes_digest: String,
    pub verified_files: usize,
    pub verified_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalPackageErrorKind {
    UnsupportedArchiveVersion,
    MissingPackageField,
    NonCanonicalPackageBytes,
    SemanticContract,
    PathTraversal,
    DuplicatePath,
    LinkRejected,
    HardLinkRejected,
    InvalidMode,
    InvalidContentDigest,
    NonEmptyTarget,
    MissingTarget,
    TargetNotDirectory,
    Io,
    Parse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPackageError {
    pub kind: LocalPackageErrorKind,
    pub message: String,
}

impl LocalPackageError {
    fn new(kind: LocalPackageErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for LocalPackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LocalPackageError {}

pub fn pack_local_package(
    package_root: &Path,
    package_bytes: &[u8],
) -> Result<Vec<u8>, LocalPackageError> {
    let package_semantics = inspect_package_bytes(package_bytes)?;
    let mut files = collect_package_files(package_root)?;

    match files
        .iter_mut()
        .find(|file| file.descriptor.path == PACKAGE_BYTES_PATH)
    {
        Some(existing) => {
            let existing_bytes = decode_hex(&existing.data_hex)?;
            if existing_bytes != package_bytes {
                return Err(LocalPackageError::new(
                    LocalPackageErrorKind::SemanticContract,
                    "package root package.json must match canonical package bytes",
                ));
            }
            existing.descriptor = file_descriptor(PACKAGE_BYTES_PATH, package_bytes, 0o644)?;
            existing.data_hex = encode_hex(package_bytes);
        }
        None => files.push(LocalPackageBlob {
            descriptor: file_descriptor(PACKAGE_BYTES_PATH, package_bytes, 0o644)?,
            data_hex: encode_hex(package_bytes),
        }),
    }

    let archive = finalized_archive(LocalPackageArchive {
        schema_version: LOCAL_PACKAGE_ARCHIVE_SCHEMA_VERSION.to_string(),
        archive_digest: String::new(),
        package: package_semantics.subject,
        inventory: Vec::new(),
        provenance: package_semantics.provenance,
        licenses: package_semantics.licenses,
        dependencies: package_semantics.dependencies,
        capabilities: package_semantics.capabilities,
        layouts: LocalPackageLayouts {
            directory_layout: LOCAL_PACKAGE_DIRECTORY_LAYOUT.to_string(),
            archive_layout: LOCAL_PACKAGE_ARCHIVE_LAYOUT.to_string(),
        },
        files,
    })?;

    canonical_archive_bytes(&archive)
}

pub fn inspect_local_package(
    archive_bytes: &[u8],
) -> Result<LocalPackageInspection, LocalPackageError> {
    let archive = parse_local_package_archive(archive_bytes)?;
    Ok(LocalPackageInspection {
        archive_digest: archive.archive_digest,
        package: archive.package,
        inventory: archive.inventory,
        provenance: archive.provenance,
        licenses: archive.licenses,
        dependencies: archive.dependencies,
        capabilities: archive.capabilities,
    })
}

pub fn verify_local_package(
    archive_bytes: &[u8],
) -> Result<LocalPackageVerification, LocalPackageError> {
    let archive = parse_local_package_archive(archive_bytes)?;
    verification_from_archive(&archive)
}

pub fn unpack_local_package(
    archive_bytes: &[u8],
    target_dir: &Path,
) -> Result<LocalPackageVerification, LocalPackageError> {
    let archive = parse_local_package_archive(archive_bytes)?;
    validate_unpack_target(target_dir)?;

    for file in &archive.files {
        let relative = normalize_relative_path(&file.descriptor.path)?;
        let target = target_dir.join(Path::new(&relative));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                LocalPackageError::new(
                    LocalPackageErrorKind::Io,
                    format!("failed to create {}: {error}", parent.display()),
                )
            })?;
        }
        let bytes = decode_hex(&file.data_hex)?;
        fs::write(&target, &bytes).map_err(|error| {
            LocalPackageError::new(
                LocalPackageErrorKind::Io,
                format!("failed to write {}: {error}", target.display()),
            )
        })?;
        set_normalized_mode(&target, file.descriptor.mode)?;
    }

    verification_from_archive(&archive)
}

pub fn parse_local_package_archive(
    archive_bytes: &[u8],
) -> Result<LocalPackageArchive, LocalPackageError> {
    let archive: LocalPackageArchive = serde_json::from_slice(archive_bytes).map_err(|error| {
        LocalPackageError::new(
            LocalPackageErrorKind::Parse,
            format!("failed to parse local package archive: {error}"),
        )
    })?;
    validate_local_package_archive(&archive)?;
    canonicalized_archive(&archive, true)
}

pub fn canonical_archive_bytes(
    archive: &LocalPackageArchive,
) -> Result<Vec<u8>, LocalPackageError> {
    let canonical = canonicalized_archive(archive, true)?;
    serde_json::to_vec(&canonical).map_err(|error| {
        LocalPackageError::new(
            LocalPackageErrorKind::Parse,
            format!("failed to serialize local package archive: {error}"),
        )
    })
}

pub fn validate_local_package_archive(
    archive: &LocalPackageArchive,
) -> Result<(), LocalPackageError> {
    let canonical = canonicalized_archive(archive, true)?;
    if canonical.schema_version != LOCAL_PACKAGE_ARCHIVE_SCHEMA_VERSION {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::UnsupportedArchiveVersion,
            format!(
                "unsupported local package archive schema_version {}",
                canonical.schema_version
            ),
        ));
    }
    if canonical.layouts.directory_layout != LOCAL_PACKAGE_DIRECTORY_LAYOUT
        || canonical.layouts.archive_layout != LOCAL_PACKAGE_ARCHIVE_LAYOUT
    {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::SemanticContract,
            "local package archive layout identifiers do not match canon local package v1",
        ));
    }
    validate_digest(&canonical.package.content_digest)?;
    validate_digest(&canonical.package.canonical_bytes_digest)?;
    validate_digest(&canonical.archive_digest)?;

    let package_blob = canonical
        .files
        .iter()
        .find(|file| file.descriptor.path == PACKAGE_BYTES_PATH)
        .ok_or_else(|| {
            LocalPackageError::new(
                LocalPackageErrorKind::MissingPackageField,
                "local package archive must contain package.json",
            )
        })?;
    let package_bytes = decode_hex(&package_blob.data_hex)?;
    let semantics = inspect_package_bytes(&package_bytes)?;
    if semantics.subject != canonical.package {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::SemanticContract,
            "archive package subject does not match package.json semantics",
        ));
    }

    let mut seen = BTreeSet::new();
    let mut inventory = Vec::new();
    for file in &canonical.files {
        validate_file_blob(file)?;
        if !seen.insert(file.descriptor.path.clone()) {
            return Err(LocalPackageError::new(
                LocalPackageErrorKind::DuplicatePath,
                format!("duplicate archive path {}", file.descriptor.path),
            ));
        }
        inventory.push(file.descriptor.clone());
    }
    if inventory != canonical.inventory {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::SemanticContract,
            "archive inventory does not match file descriptors",
        ));
    }

    let expected_digest = archive_digest(&canonical)?;
    if canonical.archive_digest != expected_digest {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::InvalidContentDigest,
            format!(
                "archive digest mismatch: expected {expected_digest}, found {}",
                canonical.archive_digest
            ),
        ));
    }

    Ok(())
}

fn collect_package_files(package_root: &Path) -> Result<Vec<LocalPackageBlob>, LocalPackageError> {
    let metadata = fs::symlink_metadata(package_root).map_err(|error| {
        LocalPackageError::new(
            LocalPackageErrorKind::Io,
            format!(
                "failed to inspect package root {}: {error}",
                package_root.display()
            ),
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::LinkRejected,
            format!(
                "package root must not be a symlink: {}",
                package_root.display()
            ),
        ));
    }
    if !metadata.is_dir() {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::TargetNotDirectory,
            format!(
                "package root must be a directory: {}",
                package_root.display()
            ),
        ));
    }

    let mut files = Vec::new();
    collect_package_files_inner(package_root, package_root, &mut files)?;
    files.sort_by(|left, right| left.descriptor.path.cmp(&right.descriptor.path));

    let mut seen = BTreeSet::new();
    for file in &files {
        if !seen.insert(file.descriptor.path.clone()) {
            return Err(LocalPackageError::new(
                LocalPackageErrorKind::DuplicatePath,
                format!("duplicate normalized package path {}", file.descriptor.path),
            ));
        }
    }
    Ok(files)
}

fn collect_package_files_inner(
    package_root: &Path,
    current: &Path,
    files: &mut Vec<LocalPackageBlob>,
) -> Result<(), LocalPackageError> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| {
            LocalPackageError::new(
                LocalPackageErrorKind::Io,
                format!("failed to read {}: {error}", current.display()),
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            LocalPackageError::new(
                LocalPackageErrorKind::Io,
                format!(
                    "failed to inspect directory entry in {}: {error}",
                    current.display()
                ),
            )
        })?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            LocalPackageError::new(
                LocalPackageErrorKind::Io,
                format!("failed to inspect {}: {error}", path.display()),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(LocalPackageError::new(
                LocalPackageErrorKind::LinkRejected,
                format!("package path must not be a symlink: {}", path.display()),
            ));
        }
        if metadata.is_dir() {
            collect_package_files_inner(package_root, &path, files)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(LocalPackageError::new(
                LocalPackageErrorKind::LinkRejected,
                format!("package path must be a plain file: {}", path.display()),
            ));
        }
        reject_hardlink(&path, &metadata)?;
        let relative = path.strip_prefix(package_root).map_err(|error| {
            LocalPackageError::new(
                LocalPackageErrorKind::PathTraversal,
                format!(
                    "failed to derive relative path for {}: {error}",
                    path.display()
                ),
            )
        })?;
        let relative = path_to_utf8(relative)?;
        let normalized = normalize_relative_path(&relative)?;
        let bytes = fs::read(&path).map_err(|error| {
            LocalPackageError::new(
                LocalPackageErrorKind::Io,
                format!("failed to read {}: {error}", path.display()),
            )
        })?;
        files.push(LocalPackageBlob {
            descriptor: file_descriptor(&normalized, &bytes, normalized_mode(&metadata))?,
            data_hex: encode_hex(&bytes),
        });
    }

    Ok(())
}

fn finalized_archive(
    mut archive: LocalPackageArchive,
) -> Result<LocalPackageArchive, LocalPackageError> {
    archive = canonicalized_archive(&archive, false)?;
    archive.archive_digest = archive_digest(&archive)?;
    canonicalized_archive(&archive, true)
}

fn archive_digest(archive: &LocalPackageArchive) -> Result<String, LocalPackageError> {
    let digest_view = canonicalized_archive(archive, false)?;
    let bytes = serde_json::to_vec(&digest_view).map_err(|error| {
        LocalPackageError::new(
            LocalPackageErrorKind::Parse,
            format!("failed to serialize archive digest view: {error}"),
        )
    })?;
    Ok(hash_bytes(&bytes))
}

fn canonicalized_archive(
    archive: &LocalPackageArchive,
    include_digest: bool,
) -> Result<LocalPackageArchive, LocalPackageError> {
    let mut canonical = archive.clone();
    canonical.files = canonical
        .files
        .into_iter()
        .map(|mut file| {
            file.descriptor.path = normalize_relative_path(&file.descriptor.path)?;
            file.descriptor.content_digest = normalized_digest(&file.descriptor.content_digest)?;
            Ok(file)
        })
        .collect::<Result<Vec<_>, LocalPackageError>>()?;
    canonical
        .files
        .sort_by(|left, right| left.descriptor.path.cmp(&right.descriptor.path));
    canonical.inventory = canonical
        .files
        .iter()
        .map(|file| file.descriptor.clone())
        .collect();
    canonical.licenses.sort();
    canonical.licenses.dedup();
    canonical.dependencies.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.content_digest.cmp(&right.content_digest))
    });
    canonical.dependencies.dedup();
    canonical.capabilities.sort();
    canonical.capabilities.dedup();
    canonical.package.content_digest = normalized_digest(&canonical.package.content_digest)?;
    canonical.package.canonical_bytes_digest =
        normalized_digest(&canonical.package.canonical_bytes_digest)?;
    if !include_digest {
        canonical.archive_digest.clear();
    } else {
        canonical.archive_digest = normalized_digest(&canonical.archive_digest)?;
    }
    Ok(canonical)
}

fn inspect_package_bytes(bytes: &[u8]) -> Result<PackageSemantics, LocalPackageError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        LocalPackageError::new(
            LocalPackageErrorKind::Parse,
            format!("failed to parse canonical package bytes: {error}"),
        )
    })?;
    let canonical_bytes = serde_json::to_vec(&value).map_err(|error| {
        LocalPackageError::new(
            LocalPackageErrorKind::Parse,
            format!("failed to serialize package canonical view: {error}"),
        )
    })?;
    if canonical_bytes != bytes {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::NonCanonicalPackageBytes,
            "package bytes must be canonical compact JSON",
        ));
    }
    let object = value.as_object().ok_or_else(|| {
        LocalPackageError::new(
            LocalPackageErrorKind::SemanticContract,
            "package bytes must be a JSON object",
        )
    })?;
    let schema_version = required_string(&value, "schema_version")?;
    if !schema_version.starts_with("canon.") || !schema_version.ends_with(".v1") {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::SemanticContract,
            format!("unsupported package schema_version {schema_version}"),
        ));
    }
    let content_digest = required_string(&value, "content_digest")?;
    validate_digest(&content_digest)?;
    let expected_digest = semantic_package_digest(&value)?;
    if content_digest != expected_digest {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::InvalidContentDigest,
            format!(
                "package content digest mismatch: expected {expected_digest}, found {content_digest}"
            ),
        ));
    }
    let (package_id, package_version) = package_identity(&value)?;
    let mut provenance = extract_provenance(object);
    provenance.insert("schema_version".to_string(), schema_version.clone());
    provenance.insert("package_id".to_string(), package_id.clone());
    provenance.insert("package_version".to_string(), package_version.clone());
    provenance.insert("content_digest".to_string(), content_digest.clone());

    Ok(PackageSemantics {
        subject: LocalPackageSubject {
            schema_version: schema_version.clone(),
            package_id: package_id.clone(),
            package_version: package_version.clone(),
            content_digest: content_digest.clone(),
            canonical_bytes_digest: hash_bytes(bytes),
            bytes: bytes.len() as u64,
        },
        provenance: LocalPackageProvenance {
            source_schema: schema_version,
            package_id,
            package_version,
            content_digest,
            declared: provenance,
        },
        licenses: extract_licenses(object),
        dependencies: extract_dependencies(object)?,
        capabilities: extract_capabilities(object),
    })
}

fn semantic_package_digest(value: &Value) -> Result<String, LocalPackageError> {
    let mut digest_view = value.clone();
    let Some(object) = digest_view.as_object_mut() else {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::SemanticContract,
            "package bytes must be a JSON object",
        ));
    };
    object.insert("content_digest".to_string(), Value::String(String::new()));
    let bytes = serde_json::to_vec(&digest_view).map_err(|error| {
        LocalPackageError::new(
            LocalPackageErrorKind::Parse,
            format!("failed to serialize package digest view: {error}"),
        )
    })?;
    Ok(hash_bytes(&bytes))
}

fn package_identity(value: &Value) -> Result<(String, String), LocalPackageError> {
    if value.get("package_id").is_some() || value.get("package_version").is_some() {
        return Ok((
            required_string(value, "package_id")?,
            required_string(value, "package_version")?,
        ));
    }
    let registry = value.get("registry").ok_or_else(|| {
        LocalPackageError::new(
            LocalPackageErrorKind::MissingPackageField,
            "package must include package_id/package_version or registry.id/registry.version",
        )
    })?;
    let id = registry.get("id").and_then(Value::as_str).ok_or_else(|| {
        LocalPackageError::new(
            LocalPackageErrorKind::MissingPackageField,
            "registry package must include registry.id",
        )
    })?;
    let version = registry
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            LocalPackageError::new(
                LocalPackageErrorKind::MissingPackageField,
                "registry package must include registry.version",
            )
        })?;
    Ok((id.to_string(), version.to_string()))
}

fn required_string(value: &Value, field: &str) -> Result<String, LocalPackageError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            LocalPackageError::new(
                LocalPackageErrorKind::MissingPackageField,
                format!("package must include non-empty {field}"),
            )
        })
}

fn extract_licenses(object: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut licenses = Vec::new();
    for field in ["license", "license_expression"] {
        if let Some(value) = object.get(field).and_then(Value::as_str)
            && !value.trim().is_empty()
        {
            licenses.push(value.trim().to_string());
        }
    }
    if let Some(values) = object.get("licenses").and_then(Value::as_array) {
        for value in values {
            if let Some(license) = value.as_str()
                && !license.trim().is_empty()
            {
                licenses.push(license.trim().to_string());
            }
        }
    }
    licenses.sort();
    licenses.dedup();
    licenses
}

fn extract_dependencies(
    object: &serde_json::Map<String, Value>,
) -> Result<Vec<LocalPackageDependency>, LocalPackageError> {
    let mut dependencies = Vec::new();
    if let Some(values) = object
        .get("dependency_references")
        .and_then(Value::as_array)
    {
        for value in values {
            dependencies.push(dependency_from_value(value)?);
        }
    }
    if let Some(values) = object.get("dependencies").and_then(Value::as_array) {
        for value in values {
            dependencies.push(dependency_from_value(value)?);
        }
    }
    if let Some(lock) = object.get("dependency_lock")
        && let Some(descriptor) = lock.get("descriptor")
    {
        let digest = descriptor
            .get("content_digest")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        if let Some(digest) = &digest {
            validate_digest(digest)?;
        }
        dependencies.push(LocalPackageDependency {
            id: "dependency_lock".to_string(),
            version: None,
            content_digest: digest,
        });
    }
    dependencies.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.content_digest.cmp(&right.content_digest))
    });
    dependencies.dedup();
    Ok(dependencies)
}

fn dependency_from_value(value: &Value) -> Result<LocalPackageDependency, LocalPackageError> {
    if let Some(id) = value.as_str() {
        return Ok(LocalPackageDependency {
            id: id.to_string(),
            version: None,
            content_digest: None,
        });
    }
    let object = value.as_object().ok_or_else(|| {
        LocalPackageError::new(
            LocalPackageErrorKind::SemanticContract,
            "dependencies must be strings or objects",
        )
    })?;
    let id = object
        .get("id")
        .or_else(|| object.get("package_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            LocalPackageError::new(
                LocalPackageErrorKind::MissingPackageField,
                "dependency object must include id or package_id",
            )
        })?;
    let content_digest = object
        .get("content_digest")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    if let Some(digest) = &content_digest {
        validate_digest(digest)?;
    }
    Ok(LocalPackageDependency {
        id: id.to_string(),
        version: object
            .get("version")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        content_digest,
    })
}

fn extract_capabilities(object: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut capabilities = Vec::new();
    if let Some(values) = object.get("capabilities").and_then(Value::as_array) {
        for value in values {
            if let Some(capability) = value.as_str()
                && !capability.trim().is_empty()
            {
                capabilities.push(capability.trim().to_string());
            }
        }
    }
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

fn extract_provenance(object: &serde_json::Map<String, Value>) -> BTreeMap<String, String> {
    let mut provenance = BTreeMap::new();
    if let Some(values) = object.get("provenance").and_then(Value::as_object) {
        for (key, value) in values {
            if let Some(text) = scalar_string(value) {
                provenance.insert(key.clone(), text);
            }
        }
    }
    provenance
}

fn scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn validate_file_blob(file: &LocalPackageBlob) -> Result<(), LocalPackageError> {
    normalize_relative_path(&file.descriptor.path)?;
    validate_mode(file.descriptor.mode)?;
    validate_digest(&file.descriptor.content_digest)?;
    let bytes = decode_hex(&file.data_hex)?;
    if bytes.len() as u64 != file.descriptor.bytes {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::InvalidContentDigest,
            format!(
                "byte count mismatch for {}: expected {}, found {}",
                file.descriptor.path,
                file.descriptor.bytes,
                bytes.len()
            ),
        ));
    }
    let digest = hash_bytes(&bytes);
    if digest != file.descriptor.content_digest {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::InvalidContentDigest,
            format!(
                "content digest mismatch for {}: expected {}, found {digest}",
                file.descriptor.path, file.descriptor.content_digest
            ),
        ));
    }
    Ok(())
}

fn file_descriptor(
    path: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<LocalPackageFile, LocalPackageError> {
    validate_mode(mode)?;
    Ok(LocalPackageFile {
        path: normalize_relative_path(path)?,
        mode,
        content_digest: hash_bytes(bytes),
        bytes: bytes.len() as u64,
    })
}

fn verification_from_archive(
    archive: &LocalPackageArchive,
) -> Result<LocalPackageVerification, LocalPackageError> {
    let verified_bytes = archive
        .inventory
        .iter()
        .map(|descriptor| descriptor.bytes)
        .sum();
    Ok(LocalPackageVerification {
        archive_digest: archive.archive_digest.clone(),
        package_content_digest: archive.package.content_digest.clone(),
        package_bytes_digest: archive.package.canonical_bytes_digest.clone(),
        verified_files: archive.inventory.len(),
        verified_bytes,
    })
}

fn validate_unpack_target(target_dir: &Path) -> Result<(), LocalPackageError> {
    let metadata = fs::symlink_metadata(target_dir).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            LocalPackageError::new(
                LocalPackageErrorKind::MissingTarget,
                format!("unpack target must already exist: {}", target_dir.display()),
            )
        } else {
            LocalPackageError::new(
                LocalPackageErrorKind::Io,
                format!(
                    "failed to inspect unpack target {}: {error}",
                    target_dir.display()
                ),
            )
        }
    })?;
    if metadata.file_type().is_symlink() {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::LinkRejected,
            format!(
                "unpack target must not be a symlink: {}",
                target_dir.display()
            ),
        ));
    }
    if !metadata.is_dir() {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::TargetNotDirectory,
            format!(
                "unpack target must be a directory: {}",
                target_dir.display()
            ),
        ));
    }
    let mut entries = fs::read_dir(target_dir).map_err(|error| {
        LocalPackageError::new(
            LocalPackageErrorKind::Io,
            format!(
                "failed to read unpack target {}: {error}",
                target_dir.display()
            ),
        )
    })?;
    if entries
        .next()
        .transpose()
        .map_err(|error| {
            LocalPackageError::new(
                LocalPackageErrorKind::Io,
                format!(
                    "failed to inspect unpack target {}: {error}",
                    target_dir.display()
                ),
            )
        })?
        .is_some()
    {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::NonEmptyTarget,
            format!("unpack target must be empty: {}", target_dir.display()),
        ));
    }
    Ok(())
}

fn validate_mode(mode: u32) -> Result<(), LocalPackageError> {
    if matches!(mode, 0o644 | 0o755) {
        Ok(())
    } else {
        Err(LocalPackageError::new(
            LocalPackageErrorKind::InvalidMode,
            format!("unsupported normalized file mode {mode:o}"),
        ))
    }
}

fn normalize_relative_path(path: &str) -> Result<String, LocalPackageError> {
    let normalized = path.replace('\\', "/");
    if normalized.is_empty() {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::PathTraversal,
            "package path must not be empty",
        ));
    }
    if normalized.starts_with('/') || has_windows_drive_prefix(&normalized) {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::PathTraversal,
            format!("package path must be relative: {path}"),
        ));
    }

    let mut segments = Vec::new();
    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains('\0') {
            return Err(LocalPackageError::new(
                LocalPackageErrorKind::PathTraversal,
                format!("package path must not traverse directories: {path}"),
            ));
        }
        segments.push(segment);
    }
    Ok(segments.join("/"))
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[1] == b':' && bytes[2] == b'/' && bytes[0].is_ascii_alphabetic()
}

fn path_to_utf8(path: &Path) -> Result<String, LocalPackageError> {
    path.to_str().map(ToString::to_string).ok_or_else(|| {
        LocalPackageError::new(
            LocalPackageErrorKind::PathTraversal,
            format!("package path is not valid UTF-8: {}", path.display()),
        )
    })
}

fn normalized_digest(digest: &str) -> Result<String, LocalPackageError> {
    validate_digest(digest)?;
    Ok(digest.to_string())
}

fn validate_digest(digest: &str) -> Result<(), LocalPackageError> {
    let Some(hex) = digest.strip_prefix("blake3:") else {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::InvalidContentDigest,
            format!("invalid BLAKE3 digest {digest}"),
        ));
    };
    if hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(LocalPackageError::new(
            LocalPackageErrorKind::InvalidContentDigest,
            format!("invalid BLAKE3 digest {digest}"),
        ))
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, LocalPackageError> {
    if !hex.len().is_multiple_of(2) {
        return Err(LocalPackageError::new(
            LocalPackageErrorKind::Parse,
            "hex payload has odd length",
        ));
    }

    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut offset = 0;
    while offset < hex.len() {
        let text = hex.get(offset..offset + 2).ok_or_else(|| {
            LocalPackageError::new(LocalPackageErrorKind::Parse, "invalid hex payload boundary")
        })?;
        bytes.push(u8::from_str_radix(text, 16).map_err(|error| {
            LocalPackageError::new(
                LocalPackageErrorKind::Parse,
                format!("invalid hex payload: {error}"),
            )
        })?);
        offset += 2;
    }
    Ok(bytes)
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

#[cfg(unix)]
fn normalized_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    if metadata.permissions().mode() & 0o111 != 0 {
        0o755
    } else {
        0o644
    }
}

#[cfg(not(unix))]
fn normalized_mode(_metadata: &fs::Metadata) -> u32 {
    0o644
}

#[cfg(unix)]
fn reject_hardlink(path: &Path, metadata: &fs::Metadata) -> Result<(), LocalPackageError> {
    use std::os::unix::fs::MetadataExt;

    if metadata.nlink() > 1 {
        Err(LocalPackageError::new(
            LocalPackageErrorKind::HardLinkRejected,
            format!("package path must not be hard-linked: {}", path.display()),
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(unix))]
fn reject_hardlink(_path: &Path, _metadata: &fs::Metadata) -> Result<(), LocalPackageError> {
    Ok(())
}

#[cfg(unix)]
fn set_normalized_mode(path: &Path, mode: u32) -> Result<(), LocalPackageError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
        LocalPackageError::new(
            LocalPackageErrorKind::Io,
            format!("failed to set mode on {}: {error}", path.display()),
        )
    })
}

#[cfg(not(unix))]
fn set_normalized_mode(_path: &Path, _mode: u32) -> Result<(), LocalPackageError> {
    Ok(())
}

struct PackageSemantics {
    subject: LocalPackageSubject,
    provenance: LocalPackageProvenance,
    licenses: Vec<String>,
    dependencies: Vec<LocalPackageDependency>,
    capabilities: Vec<String>,
}
