use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, fs, path::Path};

pub fn strategy_package_schema_version() -> &'static str {
    concat!("canon.strategy.package", ".v1")
}

const MANIFEST_KIND: &str = "package-manifest";
const RUNTIME_LOCK_KIND: &str = "runtime-lock";
const DEPENDENCY_LOCK_KIND: &str = "dependency-lock";
const ENTRYPOINT_KIND: &str = "entrypoint";
const FIXTURE_KIND: &str = "fixture";
const DEFAULT_DIRECTORY_LAYOUT: &str = "strategy-package-dir.v1";
const DEFAULT_ARCHIVE_LAYOUT: &str = "strategy-package-archive.v1";
const DEFAULT_HASH_ALGORITHM: &str = "blake3";
const DEFAULT_DESCRIPTOR_ORDERING: &str = "normalized_path_lexicographic";
const DEFAULT_ROOT_POLICY: &str = "reject_absolute_and_parent_segments";
const DEFAULT_UNDECLARED_FILE_POLICY: &str = "reject_undeclared_files";
const DEFAULT_LINK_POLICY: &str = "reject_symlink_and_hardlink_descriptors";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackage {
    pub schema_version: String,
    pub package_id: String,
    pub package_version: String,
    pub content_digest: String,
    pub strategy: StrategyPackageStrategy,
    pub manifest: StrategyPackageDescriptor,
    pub runtime: StrategyPackageRuntime,
    pub dependency_lock: StrategyPackageDependencyLock,
    pub entrypoints: Vec<StrategyPackageEntrypoint>,
    pub fixtures: Vec<StrategyPackageFixture>,
    pub provenance: StrategyPackageProvenance,
    pub capabilities: Vec<StrategyPackageCapability>,
    pub audit_contract: StrategyPackageAuditContract,
    pub license_expression: String,
    pub signature_references: Vec<StrategyPackageSignatureReference>,
    pub identity: StrategyPackageIdentityRules,
    pub layouts: StrategyPackageLayouts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageBuildInput {
    pub package_id: String,
    pub package_version: String,
    pub strategy: StrategyPackageStrategy,
    pub manifest_path: String,
    pub runtime: StrategyPackageRuntimeBuildInput,
    pub dependency_lock: StrategyPackageDependencyLockBuildInput,
    pub entrypoints: Vec<StrategyPackageEntrypointBuildInput>,
    pub fixtures: Vec<StrategyPackageFixtureBuildInput>,
    pub provenance: StrategyPackageProvenance,
    pub capabilities: Vec<StrategyPackageCapability>,
    pub audit_metrics: Vec<StrategyPackageAuditMetric>,
    pub license_expression: String,
    #[serde(default)]
    pub signature_references: Vec<StrategyPackageSignatureReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyPackageKind {
    IdentityEvidence,
    RecordLinkage,
    SchemaTransform,
    TaskTransform,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageStrategy {
    pub kind: StrategyPackageKind,
    pub selection: StrategyPackageSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum StrategyPackageSelection {
    IdentityEvidence {
        profile_id: String,
        skill_hash: String,
    },
    RecordLinkage {
        linkage_map_id: String,
        skill_hash: String,
    },
    SchemaTransform {
        schema_fingerprint: String,
        skill_hash: String,
    },
    TaskTransform {
        task: String,
        skill_hash: String,
    },
}

impl StrategyPackageSelection {
    pub const fn kind(&self) -> StrategyPackageKind {
        match self {
            Self::IdentityEvidence { .. } => StrategyPackageKind::IdentityEvidence,
            Self::RecordLinkage { .. } => StrategyPackageKind::RecordLinkage,
            Self::SchemaTransform { .. } => StrategyPackageKind::SchemaTransform,
            Self::TaskTransform { .. } => StrategyPackageKind::TaskTransform,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageDescriptor {
    pub path: String,
    pub kind: String,
    pub content_digest: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageRuntimeBuildInput {
    pub runtime: String,
    pub version: String,
    pub interface: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageRuntime {
    pub runtime: String,
    pub version: String,
    pub interface: String,
    pub descriptor: StrategyPackageDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageDependencyLockBuildInput {
    pub ecosystem: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageDependencyLock {
    pub ecosystem: String,
    pub descriptor: StrategyPackageDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageEntrypointBuildInput {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageEntrypoint {
    pub name: String,
    pub argv: Vec<String>,
    pub descriptor: StrategyPackageDescriptor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyPackageFixtureRole {
    Input,
    ExpectedStdout,
    ExpectedExitCode,
    GoldenSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageFixtureBuildInput {
    pub suite_id: String,
    pub role: StrategyPackageFixtureRole,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageFixture {
    pub suite_id: String,
    pub role: StrategyPackageFixtureRole,
    pub descriptor: StrategyPackageDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageProvenance {
    pub source_ref: String,
    pub project_ref: String,
    pub run_ref: String,
    pub builder_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyPackageCapability {
    DeterministicLocalExecution,
    NoLiveNetwork,
    PinnedDependencies,
    AuditFixturesRequired,
    ReadOnlyVerify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyPackageVerificationMode {
    ReadOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageAuditMetric {
    pub name: String,
    pub expected: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageAuditContract {
    pub verification_mode: StrategyPackageVerificationMode,
    pub fixture_corpus_digest: String,
    pub expected_metrics: Vec<StrategyPackageAuditMetric>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageSignatureReference {
    pub kind: String,
    pub reference: String,
    pub content_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageIdentityRules {
    pub hash_algorithm: String,
    pub descriptor_ordering: String,
    pub root_escape_policy: String,
    pub undeclared_file_policy: String,
    pub link_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageLayouts {
    pub directory_layout: String,
    pub archive_layout: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPackageVerification {
    pub verified_paths: usize,
    pub content_digest: String,
    pub fixture_corpus_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyPackageErrorKind {
    UnsupportedSchemaVersion,
    SelectionKindMismatch,
    MissingEntrypoint,
    MissingFixture,
    DuplicatePath,
    PathTraversalDescriptor,
    InvalidContentDigest,
    InvalidPackageDigest,
    FixtureCorpusDigestMismatch,
    MissingDeclaredFile,
    UndeclaredFile,
    NonFileDescriptor,
    SymlinkDescriptor,
    HardLinkDescriptor,
    Io,
    Parse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyPackageError {
    pub kind: StrategyPackageErrorKind,
    pub message: String,
}

impl StrategyPackageError {
    fn new(kind: StrategyPackageErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for StrategyPackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for StrategyPackageError {}

pub fn compile_strategy_package(
    package_root: &Path,
    build: &StrategyPackageBuildInput,
) -> Result<StrategyPackage, StrategyPackageError> {
    let manifest = descriptor_from_path(package_root, &build.manifest_path, MANIFEST_KIND)?;
    let runtime_descriptor =
        descriptor_from_path(package_root, &build.runtime.path, RUNTIME_LOCK_KIND)?;
    let dependency_descriptor = descriptor_from_path(
        package_root,
        &build.dependency_lock.path,
        DEPENDENCY_LOCK_KIND,
    )?;
    let entrypoints = build
        .entrypoints
        .iter()
        .map(|entrypoint| {
            Ok(StrategyPackageEntrypoint {
                name: entrypoint.name.clone(),
                argv: entrypoint.argv.clone(),
                descriptor: descriptor_from_path(package_root, &entrypoint.path, ENTRYPOINT_KIND)?,
            })
        })
        .collect::<Result<Vec<_>, StrategyPackageError>>()?;
    let fixtures = build
        .fixtures
        .iter()
        .map(|fixture| {
            Ok(StrategyPackageFixture {
                suite_id: fixture.suite_id.clone(),
                role: fixture.role,
                descriptor: descriptor_from_path(package_root, &fixture.path, FIXTURE_KIND)?,
            })
        })
        .collect::<Result<Vec<_>, StrategyPackageError>>()?;

    let declared_paths = declared_paths(
        &manifest,
        &runtime_descriptor,
        &dependency_descriptor,
        &entrypoints,
        &fixtures,
    )?;
    validate_package_root(package_root, &declared_paths)?;

    let package = StrategyPackage {
        schema_version: strategy_package_schema_version().to_string(),
        package_id: build.package_id.clone(),
        package_version: build.package_version.clone(),
        content_digest: String::new(),
        strategy: build.strategy.clone(),
        manifest,
        runtime: StrategyPackageRuntime {
            runtime: build.runtime.runtime.clone(),
            version: build.runtime.version.clone(),
            interface: build.runtime.interface.clone(),
            descriptor: runtime_descriptor,
        },
        dependency_lock: StrategyPackageDependencyLock {
            ecosystem: build.dependency_lock.ecosystem.clone(),
            descriptor: dependency_descriptor,
        },
        entrypoints,
        fixtures,
        provenance: build.provenance.clone(),
        capabilities: build.capabilities.clone(),
        audit_contract: StrategyPackageAuditContract {
            verification_mode: StrategyPackageVerificationMode::ReadOnly,
            fixture_corpus_digest: String::new(),
            expected_metrics: build.audit_metrics.clone(),
        },
        license_expression: build.license_expression.clone(),
        signature_references: build.signature_references.clone(),
        identity: StrategyPackageIdentityRules {
            hash_algorithm: DEFAULT_HASH_ALGORITHM.to_string(),
            descriptor_ordering: DEFAULT_DESCRIPTOR_ORDERING.to_string(),
            root_escape_policy: DEFAULT_ROOT_POLICY.to_string(),
            undeclared_file_policy: DEFAULT_UNDECLARED_FILE_POLICY.to_string(),
            link_policy: DEFAULT_LINK_POLICY.to_string(),
        },
        layouts: StrategyPackageLayouts {
            directory_layout: DEFAULT_DIRECTORY_LAYOUT.to_string(),
            archive_layout: DEFAULT_ARCHIVE_LAYOUT.to_string(),
        },
    };

    let package = finalized_package(package)?;
    validate_strategy_package(&package)?;
    Ok(package)
}

pub fn canonical_package_bytes(package: &StrategyPackage) -> Result<Vec<u8>, StrategyPackageError> {
    let canonical = canonicalized_package(package, true)?;
    serde_json::to_vec(&canonical).map_err(|error| {
        StrategyPackageError::new(
            StrategyPackageErrorKind::Parse,
            format!("failed to serialize canonical strategy package bytes: {error}"),
        )
    })
}

pub fn inspect_strategy_package(bytes: &[u8]) -> Result<StrategyPackage, StrategyPackageError> {
    parse_strategy_package(bytes)
}

pub fn parse_strategy_package(bytes: &[u8]) -> Result<StrategyPackage, StrategyPackageError> {
    let package: StrategyPackage = serde_json::from_slice(bytes).map_err(|error| {
        StrategyPackageError::new(
            StrategyPackageErrorKind::Parse,
            format!("failed to parse strategy package: {error}"),
        )
    })?;
    validate_strategy_package(&package)?;
    canonicalized_package(&package, true)
}

pub fn verify_strategy_package(
    package_root: &Path,
    package: &StrategyPackage,
) -> Result<StrategyPackageVerification, StrategyPackageError> {
    let canonical = canonicalized_package(package, true)?;
    validate_strategy_package(&canonical)?;
    let declared_paths = declared_paths(
        &canonical.manifest,
        &canonical.runtime.descriptor,
        &canonical.dependency_lock.descriptor,
        &canonical.entrypoints,
        &canonical.fixtures,
    )?;
    validate_package_root(package_root, &declared_paths)?;

    let mut verified_paths = 0usize;
    verify_descriptor(package_root, &canonical.manifest)?;
    verified_paths += 1;
    verify_descriptor(package_root, &canonical.runtime.descriptor)?;
    verified_paths += 1;
    verify_descriptor(package_root, &canonical.dependency_lock.descriptor)?;
    verified_paths += 1;
    for entrypoint in &canonical.entrypoints {
        verify_descriptor(package_root, &entrypoint.descriptor)?;
        verified_paths += 1;
    }
    for fixture in &canonical.fixtures {
        verify_descriptor(package_root, &fixture.descriptor)?;
        verified_paths += 1;
    }

    Ok(StrategyPackageVerification {
        verified_paths,
        content_digest: canonical.content_digest,
        fixture_corpus_digest: canonical.audit_contract.fixture_corpus_digest,
    })
}

pub fn validate_strategy_package(package: &StrategyPackage) -> Result<(), StrategyPackageError> {
    let canonical = canonicalized_package(package, true)?;

    if canonical.schema_version != strategy_package_schema_version() {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::UnsupportedSchemaVersion,
            format!(
                "unsupported strategy package schema_version {}",
                canonical.schema_version
            ),
        ));
    }

    if canonical.strategy.kind != canonical.strategy.selection.kind() {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::SelectionKindMismatch,
            "strategy kind must match the typed selection payload",
        ));
    }

    if canonical.entrypoints.is_empty() {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::MissingEntrypoint,
            "strategy package must include at least one entrypoint",
        ));
    }
    if canonical.fixtures.is_empty() {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::MissingFixture,
            "strategy package must include at least one fixture",
        ));
    }

    validate_descriptor_kind(&canonical.manifest.kind, MANIFEST_KIND)?;
    validate_descriptor_kind(&canonical.runtime.descriptor.kind, RUNTIME_LOCK_KIND)?;
    validate_descriptor_kind(
        &canonical.dependency_lock.descriptor.kind,
        DEPENDENCY_LOCK_KIND,
    )?;
    validate_descriptor(&canonical.manifest)?;
    validate_descriptor(&canonical.runtime.descriptor)?;
    validate_descriptor(&canonical.dependency_lock.descriptor)?;

    let mut seen_paths = BTreeSet::new();
    for path in [
        canonical.manifest.path.as_str(),
        canonical.runtime.descriptor.path.as_str(),
        canonical.dependency_lock.descriptor.path.as_str(),
    ] {
        if !seen_paths.insert(path.to_string()) {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::DuplicatePath,
                format!("duplicate package path {path}"),
            ));
        }
    }

    for entrypoint in &canonical.entrypoints {
        validate_descriptor_kind(&entrypoint.descriptor.kind, ENTRYPOINT_KIND)?;
        validate_descriptor(&entrypoint.descriptor)?;
        if !seen_paths.insert(entrypoint.descriptor.path.clone()) {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::DuplicatePath,
                format!("duplicate package path {}", entrypoint.descriptor.path),
            ));
        }
    }

    for fixture in &canonical.fixtures {
        validate_descriptor_kind(&fixture.descriptor.kind, FIXTURE_KIND)?;
        validate_descriptor(&fixture.descriptor)?;
        if !seen_paths.insert(fixture.descriptor.path.clone()) {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::DuplicatePath,
                format!("duplicate package path {}", fixture.descriptor.path),
            ));
        }
    }

    let expected_fixture_digest = fixture_corpus_digest(&canonical.fixtures)?;
    if canonical.audit_contract.fixture_corpus_digest != expected_fixture_digest {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::FixtureCorpusDigestMismatch,
            format!(
                "fixture corpus digest mismatch: expected {expected_fixture_digest}, found {}",
                canonical.audit_contract.fixture_corpus_digest
            ),
        ));
    }

    for signature in &canonical.signature_references {
        if signature.kind.trim().is_empty() || signature.reference.trim().is_empty() {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::Parse,
                "signature references must include kind and reference",
            ));
        }
        validate_digest(&signature.content_digest)?;
    }

    let mut metric_names = BTreeSet::new();
    for metric in &canonical.audit_contract.expected_metrics {
        if metric.name.trim().is_empty() {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::Parse,
                "audit metric names must not be empty",
            ));
        }
        if !metric_names.insert(metric.name.clone()) {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::Parse,
                format!("duplicate audit metric {}", metric.name),
            ));
        }
    }

    let expected_digest = package_digest(&canonical)?;
    if canonical.content_digest != expected_digest {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::InvalidPackageDigest,
            format!(
                "package digest mismatch: expected {expected_digest}, found {}",
                canonical.content_digest
            ),
        ));
    }

    Ok(())
}

fn finalized_package(package: StrategyPackage) -> Result<StrategyPackage, StrategyPackageError> {
    let mut finalized = canonicalized_package(&package, false)?;
    finalized.audit_contract.fixture_corpus_digest = fixture_corpus_digest(&finalized.fixtures)?;
    finalized.content_digest = package_digest(&finalized)?;
    canonicalized_package(&finalized, true)
}

fn package_digest(package: &StrategyPackage) -> Result<String, StrategyPackageError> {
    let mut digest_view = canonicalized_package(package, false)?;
    digest_view.audit_contract.fixture_corpus_digest =
        fixture_corpus_digest(&digest_view.fixtures)?;
    let bytes = serde_json::to_vec(&digest_view).map_err(|error| {
        StrategyPackageError::new(
            StrategyPackageErrorKind::Parse,
            format!("failed to serialize strategy package digest view: {error}"),
        )
    })?;
    Ok(hash_bytes(&bytes))
}

fn fixture_corpus_digest(
    fixtures: &[StrategyPackageFixture],
) -> Result<String, StrategyPackageError> {
    #[derive(Serialize)]
    struct FixtureDigestView<'a> {
        suite_id: &'a str,
        role: StrategyPackageFixtureRole,
        path: &'a str,
        content_digest: &'a str,
    }

    let mut fixtures = fixtures.to_vec();
    fixtures.sort_by(|left, right| {
        left.suite_id
            .cmp(&right.suite_id)
            .then_with(|| left.role.cmp(&right.role))
            .then_with(|| left.descriptor.path.cmp(&right.descriptor.path))
    });
    let digest_view = fixtures
        .iter()
        .map(|fixture| FixtureDigestView {
            suite_id: fixture.suite_id.as_str(),
            role: fixture.role,
            path: fixture.descriptor.path.as_str(),
            content_digest: fixture.descriptor.content_digest.as_str(),
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&digest_view).map_err(|error| {
        StrategyPackageError::new(
            StrategyPackageErrorKind::Parse,
            format!("failed to serialize fixture digest view: {error}"),
        )
    })?;
    Ok(hash_bytes(&bytes))
}

fn canonicalized_package(
    package: &StrategyPackage,
    include_digest: bool,
) -> Result<StrategyPackage, StrategyPackageError> {
    let mut canonical = package.clone();
    canonical.manifest.path = normalize_descriptor_path(&canonical.manifest.path)?;
    canonical.runtime.descriptor.path =
        normalize_descriptor_path(&canonical.runtime.descriptor.path)?;
    canonical.dependency_lock.descriptor.path =
        normalize_descriptor_path(&canonical.dependency_lock.descriptor.path)?;
    canonical.entrypoints = canonical
        .entrypoints
        .into_iter()
        .map(|mut entrypoint| {
            entrypoint.descriptor.path = normalize_descriptor_path(&entrypoint.descriptor.path)?;
            Ok(entrypoint)
        })
        .collect::<Result<Vec<_>, StrategyPackageError>>()?;
    canonical.entrypoints.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.descriptor.path.cmp(&right.descriptor.path))
    });
    canonical.fixtures = canonical
        .fixtures
        .into_iter()
        .map(|mut fixture| {
            fixture.descriptor.path = normalize_descriptor_path(&fixture.descriptor.path)?;
            Ok(fixture)
        })
        .collect::<Result<Vec<_>, StrategyPackageError>>()?;
    canonical.fixtures.sort_by(|left, right| {
        left.suite_id
            .cmp(&right.suite_id)
            .then_with(|| left.role.cmp(&right.role))
            .then_with(|| left.descriptor.path.cmp(&right.descriptor.path))
    });
    canonical.capabilities.sort_by(|left, right| {
        capability_rank(*left)
            .cmp(&capability_rank(*right))
            .then_with(|| (*left as usize).cmp(&(*right as usize)))
    });
    canonical
        .audit_contract
        .expected_metrics
        .sort_by(|left, right| left.name.cmp(&right.name));
    canonical.signature_references.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.reference.cmp(&right.reference))
    });
    if !include_digest {
        canonical.content_digest.clear();
    }
    Ok(canonical)
}

fn validate_descriptor(descriptor: &StrategyPackageDescriptor) -> Result<(), StrategyPackageError> {
    normalize_descriptor_path(&descriptor.path)?;
    validate_digest(&descriptor.content_digest)
}

fn validate_descriptor_kind(actual: &str, expected: &str) -> Result<(), StrategyPackageError> {
    if actual == expected {
        Ok(())
    } else {
        Err(StrategyPackageError::new(
            StrategyPackageErrorKind::Parse,
            format!("descriptor kind must be {expected}, found {actual}"),
        ))
    }
}

fn validate_digest(digest: &str) -> Result<(), StrategyPackageError> {
    if digest.starts_with("blake3:") && digest.len() > "blake3:".len() {
        Ok(())
    } else {
        Err(StrategyPackageError::new(
            StrategyPackageErrorKind::InvalidContentDigest,
            format!("invalid content digest {digest}"),
        ))
    }
}

fn descriptor_from_path(
    package_root: &Path,
    declared_path: &str,
    kind: &str,
) -> Result<StrategyPackageDescriptor, StrategyPackageError> {
    let normalized = normalize_descriptor_path(declared_path)?;
    let absolute = package_root.join(Path::new(&normalized));
    ensure_plain_file(&absolute)?;
    let bytes = fs::read(&absolute).map_err(|error| {
        StrategyPackageError::new(
            StrategyPackageErrorKind::Io,
            format!("failed to read {}: {error}", absolute.display()),
        )
    })?;
    Ok(StrategyPackageDescriptor {
        path: normalized,
        kind: kind.to_string(),
        content_digest: hash_bytes(&bytes),
        bytes: bytes.len() as u64,
    })
}

fn verify_descriptor(
    package_root: &Path,
    descriptor: &StrategyPackageDescriptor,
) -> Result<(), StrategyPackageError> {
    let absolute = package_root.join(Path::new(&descriptor.path));
    ensure_plain_file(&absolute)?;
    let bytes = fs::read(&absolute).map_err(|error| {
        StrategyPackageError::new(
            StrategyPackageErrorKind::Io,
            format!("failed to read {}: {error}", absolute.display()),
        )
    })?;
    let digest = hash_bytes(&bytes);
    if digest != descriptor.content_digest {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::InvalidContentDigest,
            format!(
                "content digest mismatch for {}: expected {}, found {}",
                descriptor.path, descriptor.content_digest, digest
            ),
        ));
    }
    if bytes.len() as u64 != descriptor.bytes {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::InvalidContentDigest,
            format!(
                "byte length mismatch for {}: expected {}, found {}",
                descriptor.path,
                descriptor.bytes,
                bytes.len()
            ),
        ));
    }
    Ok(())
}

fn declared_paths(
    manifest: &StrategyPackageDescriptor,
    runtime: &StrategyPackageDescriptor,
    dependency_lock: &StrategyPackageDescriptor,
    entrypoints: &[StrategyPackageEntrypoint],
    fixtures: &[StrategyPackageFixture],
) -> Result<BTreeSet<String>, StrategyPackageError> {
    let mut declared = BTreeSet::new();
    for path in [
        manifest.path.as_str(),
        runtime.path.as_str(),
        dependency_lock.path.as_str(),
    ] {
        if !declared.insert(path.to_string()) {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::DuplicatePath,
                format!("duplicate package path {path}"),
            ));
        }
    }
    for entrypoint in entrypoints {
        if !declared.insert(entrypoint.descriptor.path.clone()) {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::DuplicatePath,
                format!("duplicate package path {}", entrypoint.descriptor.path),
            ));
        }
    }
    for fixture in fixtures {
        if !declared.insert(fixture.descriptor.path.clone()) {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::DuplicatePath,
                format!("duplicate package path {}", fixture.descriptor.path),
            ));
        }
    }
    Ok(declared)
}

fn validate_package_root(
    package_root: &Path,
    declared_paths: &BTreeSet<String>,
) -> Result<(), StrategyPackageError> {
    let mut seen = BTreeSet::new();
    walk_package_root(package_root, package_root, declared_paths, &mut seen)?;
    let missing = declared_paths
        .difference(&seen)
        .cloned()
        .collect::<Vec<_>>();
    if let Some(first) = missing.first() {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::MissingDeclaredFile,
            format!("declared package path missing from root: {first}"),
        ));
    }
    Ok(())
}

fn walk_package_root(
    package_root: &Path,
    current: &Path,
    declared_paths: &BTreeSet<String>,
    seen: &mut BTreeSet<String>,
) -> Result<(), StrategyPackageError> {
    for entry in fs::read_dir(current).map_err(|error| {
        StrategyPackageError::new(
            StrategyPackageErrorKind::Io,
            format!(
                "failed to read package directory {}: {error}",
                current.display()
            ),
        )
    })? {
        let entry = entry.map_err(|error| {
            StrategyPackageError::new(
                StrategyPackageErrorKind::Io,
                format!(
                    "failed to read package entry {}: {error}",
                    current.display()
                ),
            )
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            StrategyPackageError::new(
                StrategyPackageErrorKind::Io,
                format!("failed to stat {}: {error}", path.display()),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::SymlinkDescriptor,
                format!("package path {} must not be a symlink", path.display()),
            ));
        }
        if metadata.is_dir() {
            walk_package_root(package_root, &path, declared_paths, seen)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::NonFileDescriptor,
                format!("package path {} must be a regular file", path.display()),
            ));
        }
        ensure_not_hard_link(&path, &metadata)?;
        let relative = relative_path_from_root(package_root, &path)?;
        if !declared_paths.contains(&relative) {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::UndeclaredFile,
                format!("undeclared package file {relative}"),
            ));
        }
        seen.insert(relative);
    }
    Ok(())
}

fn ensure_plain_file(path: &Path) -> Result<(), StrategyPackageError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        let kind = if error.kind() == std::io::ErrorKind::NotFound {
            StrategyPackageErrorKind::MissingDeclaredFile
        } else {
            StrategyPackageErrorKind::Io
        };
        StrategyPackageError::new(kind, format!("failed to stat {}: {error}", path.display()))
    })?;
    if metadata.file_type().is_symlink() {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::SymlinkDescriptor,
            format!("descriptor path {} must not be a symlink", path.display()),
        ));
    }
    if !metadata.is_file() {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::NonFileDescriptor,
            format!(
                "descriptor path {} must point to a regular file",
                path.display()
            ),
        ));
    }
    ensure_not_hard_link(path, &metadata)
}

fn ensure_not_hard_link(path: &Path, metadata: &fs::Metadata) -> Result<(), StrategyPackageError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() > 1 {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::HardLinkDescriptor,
                format!("descriptor path {} must not be a hard link", path.display()),
            ));
        }
    }
    let _ = path;
    Ok(())
}

fn relative_path_from_root(
    package_root: &Path,
    path: &Path,
) -> Result<String, StrategyPackageError> {
    let relative = path.strip_prefix(package_root).map_err(|error| {
        StrategyPackageError::new(
            StrategyPackageErrorKind::PathTraversalDescriptor,
            format!(
                "failed to derive package-relative path for {}: {error}",
                path.display()
            ),
        )
    })?;
    let parts = relative
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            StrategyPackageError::new(
                StrategyPackageErrorKind::Parse,
                format!("package path {} is not valid UTF-8", path.display()),
            )
        })?;
    normalize_descriptor_path(&parts.join("/"))
}

fn capability_rank(capability: StrategyPackageCapability) -> usize {
    match capability {
        StrategyPackageCapability::DeterministicLocalExecution => 0,
        StrategyPackageCapability::NoLiveNetwork => 1,
        StrategyPackageCapability::PinnedDependencies => 2,
        StrategyPackageCapability::AuditFixturesRequired => 3,
        StrategyPackageCapability::ReadOnlyVerify => 4,
    }
}

fn normalize_descriptor_path(path: &str) -> Result<String, StrategyPackageError> {
    let normalized = path.replace('\\', "/");
    if normalized.is_empty() {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::PathTraversalDescriptor,
            "descriptor path must not be empty",
        ));
    }
    if normalized.starts_with('/') || has_windows_drive_prefix(&normalized) {
        return Err(StrategyPackageError::new(
            StrategyPackageErrorKind::PathTraversalDescriptor,
            format!("descriptor path must be relative: {path}"),
        ));
    }

    let mut segments = Vec::new();
    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(StrategyPackageError::new(
                StrategyPackageErrorKind::PathTraversalDescriptor,
                format!("descriptor path must not traverse directories: {path}"),
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

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}
