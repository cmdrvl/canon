use super::package::{
    LocalPackageDependency, LocalPackageError, LocalPackageInspection, LocalPackageVerification,
    inspect_local_package, unpack_local_package, verify_local_package,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::Path,
};

pub const MIRROR_BUNDLE_SCHEMA_VERSION: &str = "canon.offline.mirror.bundle.v1";
pub const MIRROR_CACHE_LAYOUT_VERSION: &str = "canon-offline-mirror-cache.v1";
pub const MIRROR_CACHE_LAYOUT_FILE: &str = "mirror-layout.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorExportRequest {
    pub roots: Vec<String>,
    pub packages: Vec<MirrorPackageInput>,
    pub base_package_digests: Vec<String>,
    pub attestations: Vec<MirrorAttestationInput>,
    pub trust_roots: Vec<MirrorTrustRootInput>,
}

impl MirrorExportRequest {
    pub fn new(roots: Vec<String>, packages: Vec<MirrorPackageInput>) -> Self {
        Self {
            roots,
            packages,
            base_package_digests: Vec::new(),
            attestations: Vec::new(),
            trust_roots: Vec::new(),
        }
    }

    pub fn incremental_from(mut self, base_package_digests: Vec<String>) -> Self {
        self.base_package_digests = base_package_digests;
        self
    }

    pub fn with_attestations(mut self, attestations: Vec<MirrorAttestationInput>) -> Self {
        self.attestations = attestations;
        self
    }

    pub fn with_trust_roots(mut self, trust_roots: Vec<MirrorTrustRootInput>) -> Self {
        self.trust_roots = trust_roots;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorPackageInput {
    pub archive_bytes: Vec<u8>,
}

impl MirrorPackageInput {
    pub fn new(archive_bytes: Vec<u8>) -> Self {
        Self { archive_bytes }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorAttestationInput {
    pub subject_digest: String,
    pub bytes: Vec<u8>,
}

impl MirrorAttestationInput {
    pub fn new(subject_digest: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            subject_digest: subject_digest.into(),
            bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorTrustRootInput {
    pub id: String,
    pub bytes: Vec<u8>,
}

impl MirrorTrustRootInput {
    pub fn new(id: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            id: id.into(),
            bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorBundle {
    pub schema_version: String,
    pub bundle_digest: String,
    pub roots: Vec<String>,
    pub base_package_digests: Vec<String>,
    pub inventory: Vec<MirrorInventoryEntry>,
    pub blobs: Vec<MirrorBlob>,
    pub attestations: Vec<MirrorAttestationBlob>,
    pub trust_roots: Vec<MirrorTrustRootBlob>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorInventoryEntry {
    pub package_id: String,
    pub package_version: String,
    pub package_digest: String,
    pub archive_digest: String,
    pub package_bytes_digest: String,
    pub dependencies: Vec<LocalPackageDependency>,
    pub attestation_digests: Vec<String>,
    pub included: bool,
    pub external_base: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorBlob {
    pub package_digest: String,
    pub blob_digest: String,
    pub bytes: u64,
    pub data_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorAttestationBlob {
    pub subject_digest: String,
    pub attestation_digest: String,
    pub bytes: u64,
    pub data_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorTrustRootBlob {
    pub id: String,
    pub trust_root_digest: String,
    pub bytes: u64,
    pub data_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorVerification {
    pub bundle_digest: String,
    pub root_count: u64,
    pub included_package_count: u64,
    pub external_base_package_count: u64,
    pub attestation_count: u64,
    pub trust_root_count: u64,
    pub verified_package_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorImportRequest<'a> {
    pub bundle_bytes: &'a [u8],
    pub cache_dir: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorImportReceipt {
    pub bundle_digest: String,
    pub cache_layout: String,
    pub imported: Vec<MirrorImportedPackage>,
    pub reused_existing_count: u64,
    pub external_base_package_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorImportedPackage {
    pub package_digest: String,
    pub archive_digest: String,
    pub path: String,
    pub bytes: u64,
    pub reused_existing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorPackageRestoreRequest<'a> {
    pub bundle_bytes: &'a [u8],
    pub package_digest: &'a str,
    pub target_dir: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorPackageRestoreReceipt {
    pub package_digest: String,
    pub archive_digest: String,
    pub verification: LocalPackageVerification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirrorErrorKind {
    UnsupportedBundleVersion,
    EmptyRoots,
    DuplicatePackageDigest,
    MissingRoot,
    MissingDependencyDigest,
    MissingAncestor,
    MissingBlob,
    MissingAttestation,
    MissingTrustRoot,
    DigestMismatch,
    CorruptInventory,
    ExistingCacheCollision,
    NonCanonicalBundle,
    Io,
    Package,
    Parse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorError {
    pub kind: MirrorErrorKind,
    pub message: String,
}

impl MirrorError {
    fn new(kind: MirrorErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for MirrorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for MirrorError {}

impl From<LocalPackageError> for MirrorError {
    fn from(value: LocalPackageError) -> Self {
        Self::new(MirrorErrorKind::Package, value.to_string())
    }
}

pub fn export_mirror_bundle(request: MirrorExportRequest) -> Result<Vec<u8>, MirrorError> {
    let roots = normalized_digest_set(request.roots)?;
    if roots.is_empty() {
        return Err(MirrorError::new(
            MirrorErrorKind::EmptyRoots,
            "mirror export requires at least one root package digest",
        ));
    }
    let base_package_digests = normalized_digest_set(request.base_package_digests)?;
    let catalog = package_catalog(request.packages)?;
    let attestations = attestation_catalog(request.attestations)?;
    let trust_roots = trust_root_blobs(request.trust_roots)?;
    let closure = dependency_closure(&roots, &base_package_digests, &catalog)?;

    let mut inventory = Vec::new();
    let mut blobs = Vec::new();
    for digest in closure.included {
        let package = catalog.get(&digest).expect("closure package exists");
        let blob_digest = hash_bytes(&package.archive_bytes);
        let attestation_digests = attestations
            .get(&digest)
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| entry.attestation_digest.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        inventory.push(MirrorInventoryEntry {
            package_id: package.inspection.package.package_id.clone(),
            package_version: package.inspection.package.package_version.clone(),
            package_digest: digest.clone(),
            archive_digest: package.inspection.archive_digest.clone(),
            package_bytes_digest: package.inspection.package.canonical_bytes_digest.clone(),
            dependencies: package.inspection.dependencies.clone(),
            attestation_digests,
            included: true,
            external_base: false,
            blob_digest: Some(blob_digest.clone()),
            blob_bytes: Some(package.archive_bytes.len() as u64),
        });
        blobs.push(MirrorBlob {
            package_digest: digest,
            blob_digest,
            bytes: package.archive_bytes.len() as u64,
            data_hex: encode_hex(&package.archive_bytes),
        });
    }
    for digest in closure.external_base {
        inventory.push(MirrorInventoryEntry {
            package_id: String::new(),
            package_version: String::new(),
            package_digest: digest,
            archive_digest: String::new(),
            package_bytes_digest: String::new(),
            dependencies: Vec::new(),
            attestation_digests: Vec::new(),
            included: false,
            external_base: true,
            blob_digest: None,
            blob_bytes: None,
        });
    }

    let attestation_blobs = attestations
        .into_iter()
        .filter(|(subject, _)| {
            inventory
                .iter()
                .any(|entry| entry.package_digest == *subject)
        })
        .flat_map(|(_, entries)| entries)
        .collect::<Vec<_>>();

    let bundle = finalized_bundle(MirrorBundle {
        schema_version: MIRROR_BUNDLE_SCHEMA_VERSION.to_string(),
        bundle_digest: String::new(),
        roots: roots.into_iter().collect(),
        base_package_digests: base_package_digests.into_iter().collect(),
        inventory,
        blobs,
        attestations: attestation_blobs,
        trust_roots,
    })?;
    canonical_bundle_bytes(&bundle)
}

pub fn verify_mirror_bundle(bundle_bytes: &[u8]) -> Result<MirrorVerification, MirrorError> {
    let bundle = parse_mirror_bundle(bundle_bytes)?;
    let blob_by_package = bundle
        .blobs
        .iter()
        .map(|blob| (blob.package_digest.clone(), blob))
        .collect::<BTreeMap<_, _>>();
    let attestation_by_digest = bundle
        .attestations
        .iter()
        .map(|attestation| (attestation.attestation_digest.clone(), attestation))
        .collect::<BTreeMap<_, _>>();
    let inventory_by_digest = inventory_by_digest(&bundle)?;
    let mut verified_package_bytes = 0_u64;

    for root in &bundle.roots {
        let entry = inventory_by_digest.get(root).ok_or_else(|| {
            MirrorError::new(
                MirrorErrorKind::MissingRoot,
                format!("mirror root {root} is absent from inventory"),
            )
        })?;
        if !entry.included {
            return Err(MirrorError::new(
                MirrorErrorKind::MissingRoot,
                format!("mirror root {root} must be included in the bundle"),
            ));
        }
    }

    for entry in &bundle.inventory {
        for dependency in &entry.dependencies {
            let dependency_digest = dependency.content_digest.as_ref().ok_or_else(|| {
                MirrorError::new(
                    MirrorErrorKind::MissingDependencyDigest,
                    format!(
                        "dependency {} of {} lacks content_digest",
                        dependency.id, entry.package_digest
                    ),
                )
            })?;
            validate_digest(dependency_digest)?;
            if !inventory_by_digest.contains_key(dependency_digest) {
                return Err(MirrorError::new(
                    MirrorErrorKind::MissingAncestor,
                    format!(
                        "dependency {} of {} is missing from mirror inventory",
                        dependency_digest, entry.package_digest
                    ),
                ));
            }
        }
        for attestation_digest in &entry.attestation_digests {
            let attestation = attestation_by_digest
                .get(attestation_digest)
                .ok_or_else(|| {
                    MirrorError::new(
                        MirrorErrorKind::MissingAttestation,
                        format!(
                            "attestation {} for {} is missing",
                            attestation_digest, entry.package_digest
                        ),
                    )
                })?;
            if attestation.subject_digest != entry.package_digest {
                return Err(MirrorError::new(
                    MirrorErrorKind::CorruptInventory,
                    format!(
                        "attestation {} subject does not match {}",
                        attestation_digest, entry.package_digest
                    ),
                ));
            }
        }
        if entry.external_base {
            continue;
        }
        let blob = blob_by_package.get(&entry.package_digest).ok_or_else(|| {
            MirrorError::new(
                MirrorErrorKind::MissingBlob,
                format!("package blob {} is missing", entry.package_digest),
            )
        })?;
        verify_blob_against_entry(blob, entry)?;
        verified_package_bytes = verified_package_bytes.saturating_add(blob.bytes);
    }

    verify_attestations(&bundle.attestations)?;
    verify_trust_roots(&bundle.trust_roots)?;

    Ok(MirrorVerification {
        bundle_digest: bundle.bundle_digest,
        root_count: bundle.roots.len() as u64,
        included_package_count: bundle
            .inventory
            .iter()
            .filter(|entry| entry.included)
            .count() as u64,
        external_base_package_count: bundle
            .inventory
            .iter()
            .filter(|entry| entry.external_base)
            .count() as u64,
        attestation_count: bundle.attestations.len() as u64,
        trust_root_count: bundle.trust_roots.len() as u64,
        verified_package_bytes,
    })
}

pub fn import_mirror_bundle(
    request: MirrorImportRequest<'_>,
) -> Result<MirrorImportReceipt, MirrorError> {
    let verification = verify_mirror_bundle(request.bundle_bytes)?;
    let bundle = parse_mirror_bundle(request.bundle_bytes)?;
    fs::create_dir_all(request.cache_dir).map_err(|error| {
        MirrorError::new(
            MirrorErrorKind::Io,
            format!(
                "failed to create mirror cache {}: {error}",
                request.cache_dir.display()
            ),
        )
    })?;
    let layout_path = request.cache_dir.join(MIRROR_CACHE_LAYOUT_FILE);
    write_if_absent_or_identical(&layout_path, MIRROR_CACHE_LAYOUT_VERSION.as_bytes())?;

    let package_dir = request.cache_dir.join("packages");
    fs::create_dir_all(&package_dir).map_err(|error| {
        MirrorError::new(
            MirrorErrorKind::Io,
            format!(
                "failed to create mirror package cache {}: {error}",
                package_dir.display()
            ),
        )
    })?;

    let inventory_by_digest = inventory_by_digest(&bundle)?;
    let mut imported = Vec::new();
    let mut reused_existing_count = 0_u64;
    for blob in &bundle.blobs {
        let entry = inventory_by_digest
            .get(&blob.package_digest)
            .expect("verified blob has inventory");
        let path = package_dir.join(package_cache_filename(&blob.package_digest)?);
        let archive_bytes = decode_hex(&blob.data_hex)?;
        let reused_existing = write_if_absent_or_identical(&path, &archive_bytes)?;
        if reused_existing {
            reused_existing_count = reused_existing_count.saturating_add(1);
        }
        imported.push(MirrorImportedPackage {
            package_digest: blob.package_digest.clone(),
            archive_digest: entry.archive_digest.clone(),
            path: path.display().to_string(),
            bytes: blob.bytes,
            reused_existing,
        });
    }
    imported.sort_by(|left, right| left.package_digest.cmp(&right.package_digest));

    Ok(MirrorImportReceipt {
        bundle_digest: verification.bundle_digest,
        cache_layout: MIRROR_CACHE_LAYOUT_VERSION.to_string(),
        imported,
        reused_existing_count,
        external_base_package_count: verification.external_base_package_count,
    })
}

pub fn restore_mirror_package(
    request: MirrorPackageRestoreRequest<'_>,
) -> Result<MirrorPackageRestoreReceipt, MirrorError> {
    verify_mirror_bundle(request.bundle_bytes)?;
    let bundle = parse_mirror_bundle(request.bundle_bytes)?;
    let entry = bundle
        .inventory
        .iter()
        .find(|entry| entry.package_digest == request.package_digest)
        .ok_or_else(|| {
            MirrorError::new(
                MirrorErrorKind::MissingRoot,
                format!("package {} is absent from mirror", request.package_digest),
            )
        })?;
    if !entry.included {
        return Err(MirrorError::new(
            MirrorErrorKind::MissingBlob,
            format!(
                "package {} is an external base dependency and cannot be restored from this bundle",
                request.package_digest
            ),
        ));
    }
    let blob = bundle
        .blobs
        .iter()
        .find(|blob| blob.package_digest == request.package_digest)
        .ok_or_else(|| {
            MirrorError::new(
                MirrorErrorKind::MissingBlob,
                format!("package blob {} is missing", request.package_digest),
            )
        })?;
    let archive_bytes = decode_hex(&blob.data_hex)?;
    let verification = unpack_local_package(&archive_bytes, request.target_dir)?;
    Ok(MirrorPackageRestoreReceipt {
        package_digest: entry.package_digest.clone(),
        archive_digest: entry.archive_digest.clone(),
        verification,
    })
}

pub fn parse_mirror_bundle(bundle_bytes: &[u8]) -> Result<MirrorBundle, MirrorError> {
    let bundle: MirrorBundle = serde_json::from_slice(bundle_bytes).map_err(|error| {
        MirrorError::new(
            MirrorErrorKind::Parse,
            format!("failed to parse mirror bundle: {error}"),
        )
    })?;
    validate_bundle(&bundle)?;
    finalized_bundle(bundle)
}

fn validate_bundle(bundle: &MirrorBundle) -> Result<(), MirrorError> {
    if bundle.schema_version != MIRROR_BUNDLE_SCHEMA_VERSION {
        return Err(MirrorError::new(
            MirrorErrorKind::UnsupportedBundleVersion,
            format!(
                "unsupported mirror bundle schema_version {}",
                bundle.schema_version
            ),
        ));
    }
    validate_digest(&bundle.bundle_digest)?;
    if bundle.roots.is_empty() {
        return Err(MirrorError::new(
            MirrorErrorKind::EmptyRoots,
            "mirror bundle must contain at least one root",
        ));
    }
    for digest in bundle
        .roots
        .iter()
        .chain(bundle.base_package_digests.iter())
        .chain(bundle.inventory.iter().map(|entry| &entry.package_digest))
        .chain(bundle.blobs.iter().map(|blob| &blob.package_digest))
        .chain(bundle.attestations.iter().map(|item| &item.subject_digest))
    {
        validate_digest(digest)?;
    }
    for entry in &bundle.inventory {
        if entry.included == entry.external_base {
            return Err(MirrorError::new(
                MirrorErrorKind::CorruptInventory,
                format!(
                    "inventory entry {} must be exactly one of included/external_base",
                    entry.package_digest
                ),
            ));
        }
        if entry.included && (entry.blob_digest.is_none() || entry.blob_bytes.is_none()) {
            return Err(MirrorError::new(
                MirrorErrorKind::MissingBlob,
                format!(
                    "included package {} lacks blob metadata",
                    entry.package_digest
                ),
            ));
        }
        if entry.external_base
            && (entry.blob_digest.is_some()
                || entry.blob_bytes.is_some()
                || !entry.dependencies.is_empty())
        {
            return Err(MirrorError::new(
                MirrorErrorKind::CorruptInventory,
                format!(
                    "external base package {} must not carry package payload metadata",
                    entry.package_digest
                ),
            ));
        }
        if let Some(blob_digest) = &entry.blob_digest {
            validate_digest(blob_digest)?;
        }
        for attestation_digest in &entry.attestation_digests {
            validate_digest(attestation_digest)?;
        }
    }
    for blob in &bundle.blobs {
        validate_digest(&blob.blob_digest)?;
    }
    for attestation in &bundle.attestations {
        validate_digest(&attestation.attestation_digest)?;
    }
    for trust_root in &bundle.trust_roots {
        validate_digest(&trust_root.trust_root_digest)?;
        if trust_root.id.trim().is_empty() {
            return Err(MirrorError::new(
                MirrorErrorKind::MissingTrustRoot,
                "trust root id must not be empty",
            ));
        }
    }

    let expected = hash_bundle_without_self(bundle)?;
    if bundle.bundle_digest != expected {
        return Err(MirrorError::new(
            MirrorErrorKind::DigestMismatch,
            format!(
                "mirror bundle digest mismatch: expected {expected}, found {}",
                bundle.bundle_digest
            ),
        ));
    }
    let canonical = finalized_bundle(bundle.clone())?;
    if &canonical != bundle {
        return Err(MirrorError::new(
            MirrorErrorKind::NonCanonicalBundle,
            "mirror bundle is not canonicalized",
        ));
    }
    Ok(())
}

fn package_catalog(
    packages: Vec<MirrorPackageInput>,
) -> Result<BTreeMap<String, CatalogPackage>, MirrorError> {
    let mut catalog = BTreeMap::<String, CatalogPackage>::new();
    for package in packages {
        let inspection = inspect_local_package(&package.archive_bytes)?;
        let verification = verify_local_package(&package.archive_bytes)?;
        if verification.package_content_digest != inspection.package.content_digest {
            return Err(MirrorError::new(
                MirrorErrorKind::CorruptInventory,
                format!(
                    "package {} verification digest does not match inspection digest",
                    inspection.package.content_digest
                ),
            ));
        }
        let digest = inspection.package.content_digest.clone();
        match catalog.get(&digest) {
            Some(existing) if existing.archive_bytes != package.archive_bytes => {
                return Err(MirrorError::new(
                    MirrorErrorKind::DuplicatePackageDigest,
                    format!("multiple package archives claim digest {digest}"),
                ));
            }
            Some(_) => {}
            None => {
                catalog.insert(
                    digest,
                    CatalogPackage {
                        archive_bytes: package.archive_bytes,
                        inspection,
                    },
                );
            }
        }
    }
    Ok(catalog)
}

fn dependency_closure(
    roots: &BTreeSet<String>,
    base_package_digests: &BTreeSet<String>,
    catalog: &BTreeMap<String, CatalogPackage>,
) -> Result<MirrorClosure, MirrorError> {
    let mut included = BTreeSet::new();
    let mut external_base = BTreeSet::new();
    let mut stack = roots.iter().cloned().collect::<Vec<_>>();

    while let Some(digest) = stack.pop() {
        if included.contains(&digest) || external_base.contains(&digest) {
            continue;
        }
        if base_package_digests.contains(&digest) && !roots.contains(&digest) {
            external_base.insert(digest);
            continue;
        }
        let package = catalog.get(&digest).ok_or_else(|| {
            if roots.contains(&digest) {
                MirrorError::new(
                    MirrorErrorKind::MissingRoot,
                    format!("root package {digest} is missing from package inputs"),
                )
            } else {
                MirrorError::new(
                    MirrorErrorKind::MissingAncestor,
                    format!("dependency package {digest} is missing from package inputs"),
                )
            }
        })?;
        included.insert(digest.clone());
        for dependency in &package.inspection.dependencies {
            let dependency_digest = dependency.content_digest.clone().ok_or_else(|| {
                MirrorError::new(
                    MirrorErrorKind::MissingDependencyDigest,
                    format!(
                        "dependency {} of {} lacks content_digest",
                        dependency.id, digest
                    ),
                )
            })?;
            validate_digest(&dependency_digest)?;
            stack.push(dependency_digest);
        }
    }

    Ok(MirrorClosure {
        included,
        external_base,
    })
}

fn attestation_catalog(
    attestations: Vec<MirrorAttestationInput>,
) -> Result<BTreeMap<String, Vec<MirrorAttestationBlob>>, MirrorError> {
    let mut catalog = BTreeMap::<String, Vec<MirrorAttestationBlob>>::new();
    for attestation in attestations {
        validate_digest(&attestation.subject_digest)?;
        let attestation_digest = hash_bytes(&attestation.bytes);
        let blob = MirrorAttestationBlob {
            subject_digest: attestation.subject_digest.clone(),
            attestation_digest,
            bytes: attestation.bytes.len() as u64,
            data_hex: encode_hex(&attestation.bytes),
        };
        catalog
            .entry(attestation.subject_digest)
            .or_default()
            .push(blob);
    }
    for blobs in catalog.values_mut() {
        blobs.sort_by(|left, right| {
            left.attestation_digest
                .cmp(&right.attestation_digest)
                .then_with(|| left.subject_digest.cmp(&right.subject_digest))
        });
        blobs.dedup();
    }
    Ok(catalog)
}

fn trust_root_blobs(
    trust_roots: Vec<MirrorTrustRootInput>,
) -> Result<Vec<MirrorTrustRootBlob>, MirrorError> {
    let mut blobs = Vec::new();
    for trust_root in trust_roots {
        if trust_root.id.trim().is_empty() {
            return Err(MirrorError::new(
                MirrorErrorKind::MissingTrustRoot,
                "trust root id must not be empty",
            ));
        }
        blobs.push(MirrorTrustRootBlob {
            id: trust_root.id,
            trust_root_digest: hash_bytes(&trust_root.bytes),
            bytes: trust_root.bytes.len() as u64,
            data_hex: encode_hex(&trust_root.bytes),
        });
    }
    blobs.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.trust_root_digest.cmp(&right.trust_root_digest))
    });
    blobs.dedup();
    Ok(blobs)
}

fn verify_blob_against_entry(
    blob: &MirrorBlob,
    entry: &MirrorInventoryEntry,
) -> Result<(), MirrorError> {
    if blob.package_digest != entry.package_digest {
        return Err(MirrorError::new(
            MirrorErrorKind::CorruptInventory,
            format!(
                "blob package digest {} does not match inventory {}",
                blob.package_digest, entry.package_digest
            ),
        ));
    }
    let archive_bytes = decode_hex(&blob.data_hex)?;
    let blob_digest = hash_bytes(&archive_bytes);
    if blob.blob_digest != blob_digest {
        return Err(MirrorError::new(
            MirrorErrorKind::DigestMismatch,
            format!(
                "blob digest mismatch for {}: expected {}, found {}",
                blob.package_digest, blob_digest, blob.blob_digest
            ),
        ));
    }
    if entry.blob_digest.as_deref() != Some(blob.blob_digest.as_str()) {
        return Err(MirrorError::new(
            MirrorErrorKind::CorruptInventory,
            format!("inventory blob digest mismatch for {}", blob.package_digest),
        ));
    }
    if entry.blob_bytes != Some(blob.bytes) || blob.bytes != archive_bytes.len() as u64 {
        return Err(MirrorError::new(
            MirrorErrorKind::CorruptInventory,
            format!("blob byte count mismatch for {}", blob.package_digest),
        ));
    }
    let inspection = inspect_local_package(&archive_bytes)?;
    let verification = verify_local_package(&archive_bytes)?;
    if inspection.package.package_id != entry.package_id
        || inspection.package.package_version != entry.package_version
        || inspection.package.content_digest != entry.package_digest
        || inspection.archive_digest != entry.archive_digest
        || inspection.package.canonical_bytes_digest != entry.package_bytes_digest
        || inspection.dependencies != entry.dependencies
        || verification.package_content_digest != entry.package_digest
    {
        return Err(MirrorError::new(
            MirrorErrorKind::CorruptInventory,
            format!(
                "inventory metadata does not match package {}",
                entry.package_digest
            ),
        ));
    }
    Ok(())
}

fn verify_attestations(attestations: &[MirrorAttestationBlob]) -> Result<(), MirrorError> {
    let mut seen = BTreeSet::new();
    for attestation in attestations {
        let bytes = decode_hex(&attestation.data_hex)?;
        if attestation.bytes != bytes.len() as u64 {
            return Err(MirrorError::new(
                MirrorErrorKind::CorruptInventory,
                format!(
                    "attestation {} byte count does not match payload",
                    attestation.attestation_digest
                ),
            ));
        }
        let actual = hash_bytes(&bytes);
        if actual != attestation.attestation_digest {
            return Err(MirrorError::new(
                MirrorErrorKind::DigestMismatch,
                format!(
                    "attestation digest mismatch: expected {actual}, found {}",
                    attestation.attestation_digest
                ),
            ));
        }
        if !seen.insert((
            attestation.subject_digest.clone(),
            attestation.attestation_digest.clone(),
        )) {
            return Err(MirrorError::new(
                MirrorErrorKind::CorruptInventory,
                format!(
                    "duplicate attestation {} for {}",
                    attestation.attestation_digest, attestation.subject_digest
                ),
            ));
        }
    }
    Ok(())
}

fn verify_trust_roots(trust_roots: &[MirrorTrustRootBlob]) -> Result<(), MirrorError> {
    let mut seen = BTreeSet::new();
    for trust_root in trust_roots {
        let bytes = decode_hex(&trust_root.data_hex)?;
        if trust_root.bytes != bytes.len() as u64 {
            return Err(MirrorError::new(
                MirrorErrorKind::CorruptInventory,
                format!(
                    "trust root {} byte count does not match payload",
                    trust_root.id
                ),
            ));
        }
        let actual = hash_bytes(&bytes);
        if actual != trust_root.trust_root_digest {
            return Err(MirrorError::new(
                MirrorErrorKind::DigestMismatch,
                format!(
                    "trust root {} digest mismatch: expected {actual}, found {}",
                    trust_root.id, trust_root.trust_root_digest
                ),
            ));
        }
        if !seen.insert((trust_root.id.clone(), trust_root.trust_root_digest.clone())) {
            return Err(MirrorError::new(
                MirrorErrorKind::CorruptInventory,
                format!("duplicate trust root {}", trust_root.id),
            ));
        }
    }
    Ok(())
}

fn inventory_by_digest(
    bundle: &MirrorBundle,
) -> Result<BTreeMap<String, &MirrorInventoryEntry>, MirrorError> {
    let mut by_digest = BTreeMap::new();
    for entry in &bundle.inventory {
        if by_digest
            .insert(entry.package_digest.clone(), entry)
            .is_some()
        {
            return Err(MirrorError::new(
                MirrorErrorKind::DuplicatePackageDigest,
                format!("duplicate inventory digest {}", entry.package_digest),
            ));
        }
    }
    Ok(by_digest)
}

fn finalized_bundle(mut bundle: MirrorBundle) -> Result<MirrorBundle, MirrorError> {
    bundle.roots = normalized_digest_set(bundle.roots)?.into_iter().collect();
    bundle.base_package_digests = normalized_digest_set(bundle.base_package_digests)?
        .into_iter()
        .collect();
    bundle.inventory.sort_by(|left, right| {
        left.package_digest
            .cmp(&right.package_digest)
            .then_with(|| left.package_id.cmp(&right.package_id))
            .then_with(|| left.package_version.cmp(&right.package_version))
    });
    bundle.blobs.sort_by(|left, right| {
        left.package_digest
            .cmp(&right.package_digest)
            .then_with(|| left.blob_digest.cmp(&right.blob_digest))
    });
    bundle.attestations.sort_by(|left, right| {
        left.subject_digest
            .cmp(&right.subject_digest)
            .then_with(|| left.attestation_digest.cmp(&right.attestation_digest))
    });
    bundle.trust_roots.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.trust_root_digest.cmp(&right.trust_root_digest))
    });
    for entry in &mut bundle.inventory {
        entry.dependencies.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.version.cmp(&right.version))
                .then_with(|| left.content_digest.cmp(&right.content_digest))
        });
        entry.dependencies.dedup();
        entry.attestation_digests =
            normalized_digest_set(std::mem::take(&mut entry.attestation_digests))?
                .into_iter()
                .collect();
    }
    bundle.bundle_digest.clear();
    bundle.bundle_digest = hash_bundle_without_self(&bundle)?;
    Ok(bundle)
}

fn canonical_bundle_bytes(bundle: &MirrorBundle) -> Result<Vec<u8>, MirrorError> {
    let canonical = finalized_bundle(bundle.clone())?;
    serde_json::to_vec(&canonical).map_err(|error| {
        MirrorError::new(
            MirrorErrorKind::Parse,
            format!("failed to serialize mirror bundle: {error}"),
        )
    })
}

fn hash_bundle_without_self(bundle: &MirrorBundle) -> Result<String, MirrorError> {
    let mut hashable = bundle.clone();
    hashable.bundle_digest.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        MirrorError::new(
            MirrorErrorKind::Parse,
            format!("failed to hash mirror bundle: {error}"),
        )
    })?;
    Ok(hash_bytes(&bytes))
}

fn normalized_digest_set(digests: Vec<String>) -> Result<BTreeSet<String>, MirrorError> {
    let mut normalized = BTreeSet::new();
    for digest in digests {
        validate_digest(&digest)?;
        normalized.insert(digest);
    }
    Ok(normalized)
}

fn validate_digest(digest: &str) -> Result<(), MirrorError> {
    let suffix = digest.strip_prefix("blake3:").ok_or_else(|| {
        MirrorError::new(
            MirrorErrorKind::DigestMismatch,
            format!("digest must start with blake3:, got {digest}"),
        )
    })?;
    if suffix.len() != 64 || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MirrorError::new(
            MirrorErrorKind::DigestMismatch,
            format!("digest must be blake3 plus 64 hex characters, got {digest}"),
        ));
    }
    Ok(())
}

fn write_if_absent_or_identical(path: &Path, bytes: &[u8]) -> Result<bool, MirrorError> {
    if path.exists() {
        let existing = fs::read(path).map_err(|error| {
            MirrorError::new(
                MirrorErrorKind::Io,
                format!("failed to read existing {}: {error}", path.display()),
            )
        })?;
        if existing == bytes {
            return Ok(true);
        }
        return Err(MirrorError::new(
            MirrorErrorKind::ExistingCacheCollision,
            format!(
                "existing cache entry {} has different bytes",
                path.display()
            ),
        ));
    }
    fs::write(path, bytes).map_err(|error| {
        MirrorError::new(
            MirrorErrorKind::Io,
            format!("failed to write {}: {error}", path.display()),
        )
    })?;
    Ok(false)
}

fn package_cache_filename(package_digest: &str) -> Result<String, MirrorError> {
    validate_digest(package_digest)?;
    Ok(format!("{}.canonpkg", package_digest.replace(':', "-")))
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(text: &str) -> Result<Vec<u8>, MirrorError> {
    if !text.len().is_multiple_of(2) {
        return Err(MirrorError::new(
            MirrorErrorKind::Parse,
            "hex payload has odd length",
        ));
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    let (chunks, remainder) = text.as_bytes().as_chunks::<2>();
    debug_assert!(remainder.is_empty());
    for [hi, lo] in chunks {
        let hi = hex_value(*hi)?;
        let lo = hex_value(*lo)?;
        bytes.push((hi << 4) | lo);
    }
    Ok(bytes)
}

fn hex_value(byte: u8) -> Result<u8, MirrorError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(MirrorError::new(
            MirrorErrorKind::Parse,
            format!("invalid hex byte {byte}"),
        )),
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CatalogPackage {
    archive_bytes: Vec<u8>,
    inspection: LocalPackageInspection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MirrorClosure {
    included: BTreeSet<String>,
    external_base: BTreeSet<String>,
}
