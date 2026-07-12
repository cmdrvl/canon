use super::{
    cache::{self, ContentCache, ContentCacheError},
    oci::{
        self, ANNOTATION_LAYER_ROLE, ANNOTATION_PACKAGE_DIGEST, ANNOTATION_PACKAGE_ID,
        ANNOTATION_PACKAGE_SCHEMA, ANNOTATION_PACKAGE_VERSION, CANON_LAYER_ROLE_PRIMARY,
        CANON_OCI_CONFIG_MEDIA_TYPE, CanonArtifactClass, CanonOciManifest, CanonPackageBinding,
        OCI_IMAGE_MANIFEST_MEDIA_TYPE, OciDescriptor,
    },
    package,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{error::Error, fmt, io::Read, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OciRemote {
    pub base_url: String,
    pub repository: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OciRemotePolicy {
    pub allow_network: bool,
    pub allow_cache_writes: bool,
}

impl OciRemotePolicy {
    pub const fn online() -> Self {
        Self {
            allow_network: true,
            allow_cache_writes: true,
        }
    }

    pub const fn offline_read_only() -> Self {
        Self {
            allow_network: false,
            allow_cache_writes: false,
        }
    }

    fn require_network(self) -> Result<(), OciRemoteError> {
        if self.allow_network {
            Ok(())
        } else {
            Err(OciRemoteError::new(
                OciRemoteErrorKind::NetworkDisabled,
                "OCI network access is disabled by explicit policy",
            ))
        }
    }

    fn require_cache_writes(self) -> Result<(), OciRemoteError> {
        if self.allow_cache_writes {
            Ok(())
        } else {
            Err(OciRemoteError::new(
                OciRemoteErrorKind::CacheWriteDisabled,
                "OCI cache writes are disabled by explicit policy",
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciPublishReceipt {
    pub repository: String,
    pub manifest_digest: String,
    pub manifest_bytes: u64,
    pub package_archive_digest: String,
    pub package_content_digest: String,
    pub package_id: String,
    pub package_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub pushed_blobs: Vec<String>,
    pub reused_blobs: Vec<String>,
    pub manifest_uploaded: bool,
    pub tag_uploaded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciResolvedReference {
    pub repository: String,
    pub tag: String,
    pub manifest_digest: String,
    pub manifest_bytes: u64,
    pub package_content_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciPullReceipt {
    pub repository: String,
    pub manifest_digest: String,
    pub manifest_bytes: u64,
    pub package_archive_digest: String,
    pub package_content_digest: String,
    pub package_bytes_digest: String,
    pub package_cache_path: String,
    pub cached_blobs: Vec<String>,
    pub verified_files: usize,
    pub verified_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_from_tag: Option<OciResolvedReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OciRemoteErrorKind {
    NetworkDisabled,
    CacheWriteDisabled,
    InvalidReference,
    DigestMismatch,
    MissingPrimaryLayer,
    Http,
    Parse,
    OciContract,
    Package,
    Cache,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OciRemoteError {
    pub kind: OciRemoteErrorKind,
    pub message: String,
}

impl OciRemoteError {
    fn new(kind: OciRemoteErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for OciRemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for OciRemoteError {}

impl From<ContentCacheError> for OciRemoteError {
    fn from(error: ContentCacheError) -> Self {
        Self::new(OciRemoteErrorKind::Cache, error.to_string())
    }
}

pub fn publish_package_by_immutable_digest(
    remote: &OciRemote,
    archive_bytes: &[u8],
    tag: Option<&str>,
    policy: OciRemotePolicy,
) -> Result<OciPublishReceipt, OciRemoteError> {
    policy.require_network()?;
    let prepared = prepare_manifest(archive_bytes)?;
    let agent = agent();

    let mut pushed_blobs = Vec::new();
    let mut reused_blobs = Vec::new();
    for (descriptor, bytes) in [
        (&prepared.manifest.config, prepared.config_bytes.as_slice()),
        (&prepared.manifest.layers[0], archive_bytes),
    ] {
        if blob_exists(&agent, remote, &descriptor.digest)? {
            reused_blobs.push(descriptor.digest.clone());
        } else {
            upload_blob(&agent, remote, &descriptor.digest, bytes)?;
            pushed_blobs.push(descriptor.digest.clone());
        }
    }

    let manifest_uploaded = if manifest_exists(&agent, remote, &prepared.manifest_digest)? {
        false
    } else {
        put_manifest(
            &agent,
            remote,
            &prepared.manifest_digest,
            &prepared.manifest_bytes,
        )?;
        true
    };

    let tag_uploaded = if let Some(tag) = tag {
        let tag = normalized_reference(tag)?;
        put_manifest(&agent, remote, &tag, &prepared.manifest_bytes)?;
        true
    } else {
        false
    };

    Ok(OciPublishReceipt {
        repository: remote.repository.clone(),
        manifest_digest: prepared.manifest_digest,
        manifest_bytes: prepared.manifest_bytes.len() as u64,
        package_archive_digest: prepared.archive_digest,
        package_content_digest: prepared.binding.package_digest,
        package_id: prepared.binding.package_id,
        package_version: prepared.binding.package_version,
        tag: tag.map(ToString::to_string),
        pushed_blobs,
        reused_blobs,
        manifest_uploaded,
        tag_uploaded,
    })
}

pub fn resolve_tag_once(
    remote: &OciRemote,
    tag: &str,
    policy: OciRemotePolicy,
) -> Result<OciResolvedReference, OciRemoteError> {
    policy.require_network()?;
    let tag = normalized_reference(tag)?;
    let (manifest_digest, manifest_bytes) = get_manifest_by_reference(remote, &tag)?;
    let manifest = parse_manifest(&manifest_bytes)?;
    let package_content_digest = manifest_annotation(&manifest, ANNOTATION_PACKAGE_DIGEST)?;
    Ok(OciResolvedReference {
        repository: remote.repository.clone(),
        tag,
        manifest_digest,
        manifest_bytes: manifest_bytes.len() as u64,
        package_content_digest,
    })
}

pub fn pull_package_by_immutable_digest(
    remote: &OciRemote,
    manifest_digest: &str,
    cache: &ContentCache,
    policy: OciRemotePolicy,
) -> Result<OciPullReceipt, OciRemoteError> {
    pull_package_by_digest_inner(remote, manifest_digest, cache, policy, None)
}

pub fn pull_resolved_package(
    remote: &OciRemote,
    resolved: &OciResolvedReference,
    cache: &ContentCache,
    policy: OciRemotePolicy,
) -> Result<OciPullReceipt, OciRemoteError> {
    if resolved.repository != remote.repository {
        return Err(OciRemoteError::new(
            OciRemoteErrorKind::InvalidReference,
            format!(
                "resolved reference repository {} does not match remote {}",
                resolved.repository, remote.repository
            ),
        ));
    }
    pull_package_by_digest_inner(
        remote,
        &resolved.manifest_digest,
        cache,
        policy,
        Some(resolved.clone()),
    )
}

fn pull_package_by_digest_inner(
    remote: &OciRemote,
    manifest_digest: &str,
    cache: &ContentCache,
    policy: OciRemotePolicy,
    resolved_from_tag: Option<OciResolvedReference>,
) -> Result<OciPullReceipt, OciRemoteError> {
    policy.require_network()?;
    policy.require_cache_writes()?;
    cache::parse_digest(manifest_digest).map_err(OciRemoteError::from)?;

    let (reported_digest, manifest_bytes) = get_manifest_by_reference(remote, manifest_digest)?;
    if reported_digest != manifest_digest {
        return Err(OciRemoteError::new(
            OciRemoteErrorKind::DigestMismatch,
            format!(
                "registry returned manifest digest {reported_digest}, expected {manifest_digest}"
            ),
        ));
    }
    cache.put_blob(manifest_digest, &manifest_bytes)?;

    let manifest = parse_manifest(&manifest_bytes)?;
    let binding = binding_from_manifest(&manifest)?;
    oci::validate_manifest(&manifest, &binding).map_err(|error| {
        OciRemoteError::new(
            OciRemoteErrorKind::OciContract,
            format!("pulled OCI manifest failed Canon contract validation: {error}"),
        )
    })?;

    let primary = manifest
        .layers
        .iter()
        .find(|layer| {
            layer
                .annotations
                .get(ANNOTATION_LAYER_ROLE)
                .is_some_and(|role| role == CANON_LAYER_ROLE_PRIMARY)
        })
        .ok_or_else(|| {
            OciRemoteError::new(
                OciRemoteErrorKind::MissingPrimaryLayer,
                "pulled OCI manifest does not contain a primary Canon package layer",
            )
        })?;

    let config_bytes = get_blob(remote, &manifest.config)?;
    let cached_config = cache.put_blob(&manifest.config.digest, &config_bytes)?;
    let package_archive_bytes = get_blob(remote, primary)?;
    let cached_layer = cache.put_blob(&primary.digest, &package_archive_bytes)?;
    let cached_package = cache.materialize_package_archive(&package_archive_bytes)?;

    if cached_package.package_content_digest != binding.package_digest {
        return Err(OciRemoteError::new(
            OciRemoteErrorKind::DigestMismatch,
            format!(
                "pulled package digest {} does not match OCI binding {}",
                cached_package.package_content_digest, binding.package_digest
            ),
        ));
    }

    Ok(OciPullReceipt {
        repository: remote.repository.clone(),
        manifest_digest: manifest_digest.to_string(),
        manifest_bytes: manifest_bytes.len() as u64,
        package_archive_digest: cached_package.archive_digest,
        package_content_digest: cached_package.package_content_digest,
        package_bytes_digest: cached_package.package_bytes_digest,
        package_cache_path: cached_package.path.display().to_string(),
        cached_blobs: vec![cached_config.digest, cached_layer.digest],
        verified_files: cached_package.verified_files,
        verified_bytes: cached_package.verified_bytes,
        resolved_from_tag,
    })
}

struct PreparedManifest {
    binding: CanonPackageBinding,
    archive_digest: String,
    config_bytes: Vec<u8>,
    manifest: CanonOciManifest,
    manifest_bytes: Vec<u8>,
    manifest_digest: String,
}

fn prepare_manifest(archive_bytes: &[u8]) -> Result<PreparedManifest, OciRemoteError> {
    let inspection = package::inspect_local_package(archive_bytes).map_err(|error| {
        OciRemoteError::new(
            OciRemoteErrorKind::Package,
            format!("local package archive inspection failed before publish: {error}"),
        )
    })?;
    let artifact_class = artifact_class_for_schema(&inspection.package.schema_version)?;
    let binding = CanonPackageBinding {
        artifact_class,
        package_schema: inspection.package.schema_version.clone(),
        package_id: inspection.package.package_id.clone(),
        package_version: inspection.package.package_version.clone(),
        package_digest: inspection.package.content_digest.clone(),
    };
    let config_bytes = serde_json::to_vec(&json!({
        "schema_version": "canon.oci.config.v1",
        "package_schema": binding.package_schema,
        "package_id": binding.package_id,
        "package_version": binding.package_version,
        "package_digest": binding.package_digest,
        "archive_digest": inspection.archive_digest,
    }))
    .map_err(|error| {
        OciRemoteError::new(
            OciRemoteErrorKind::Parse,
            format!("failed to serialize OCI config: {error}"),
        )
    })?;
    let config_descriptor = OciDescriptor {
        media_type: CANON_OCI_CONFIG_MEDIA_TYPE.to_string(),
        digest: cache::sha256_digest(&config_bytes),
        size: config_bytes.len() as u64,
        annotations: Default::default(),
    };
    let payload_descriptor = OciDescriptor {
        media_type: oci::payload_media_type(&binding).map_err(|error| {
            OciRemoteError::new(
                OciRemoteErrorKind::OciContract,
                format!("failed to derive Canon OCI payload media type: {error}"),
            )
        })?,
        digest: cache::sha256_digest(archive_bytes),
        size: archive_bytes.len() as u64,
        annotations: Default::default(),
    };
    let manifest = oci::build_manifest(
        &binding,
        config_descriptor,
        payload_descriptor,
        None,
        vec![],
    )
    .map_err(|error| {
        OciRemoteError::new(
            OciRemoteErrorKind::OciContract,
            format!("failed to build Canon OCI manifest: {error}"),
        )
    })?;
    let manifest_bytes = oci::canonical_manifest_bytes(&manifest).map_err(|error| {
        OciRemoteError::new(
            OciRemoteErrorKind::OciContract,
            format!("failed to serialize Canon OCI manifest: {error}"),
        )
    })?;
    let manifest_digest = cache::sha256_digest(&manifest_bytes);
    Ok(PreparedManifest {
        binding,
        archive_digest: inspection.archive_digest,
        config_bytes,
        manifest,
        manifest_bytes,
        manifest_digest,
    })
}

fn get_manifest_by_reference(
    remote: &OciRemote,
    reference: &str,
) -> Result<(String, Vec<u8>), OciRemoteError> {
    let reference = normalized_reference(reference)?;
    let response = agent()
        .get(&remote.manifest_url(&reference)?)
        .set("Accept", OCI_IMAGE_MANIFEST_MEDIA_TYPE)
        .call()
        .map_err(map_http_error)?;
    let reported_digest = response
        .header("Docker-Content-Digest")
        .map(ToString::to_string);
    let bytes = read_response_bytes(response)?;
    let computed_digest = cache::sha256_digest(&bytes);
    let digest = reported_digest.unwrap_or_else(|| computed_digest.clone());
    if digest != computed_digest {
        return Err(OciRemoteError::new(
            OciRemoteErrorKind::DigestMismatch,
            format!("manifest header digest {digest} does not match body digest {computed_digest}"),
        ));
    }
    Ok((digest, bytes))
}

fn get_blob(remote: &OciRemote, descriptor: &OciDescriptor) -> Result<Vec<u8>, OciRemoteError> {
    let response = agent()
        .get(&remote.blob_url(&descriptor.digest)?)
        .call()
        .map_err(map_http_error)?;
    let bytes = read_response_bytes(response)?;
    verify_descriptor_bytes(descriptor, &bytes)?;
    Ok(bytes)
}

fn blob_exists(
    agent: &ureq::Agent,
    remote: &OciRemote,
    digest: &str,
) -> Result<bool, OciRemoteError> {
    match agent.head(&remote.blob_url(digest)?).call() {
        Ok(response) if response.status() < 400 => Ok(true),
        Err(ureq::Error::Status(404, _)) => Ok(false),
        Err(error) => Err(map_http_error(error)),
        Ok(response) => Err(OciRemoteError::new(
            OciRemoteErrorKind::Http,
            format!("unexpected blob HEAD status {}", response.status()),
        )),
    }
}

fn manifest_exists(
    agent: &ureq::Agent,
    remote: &OciRemote,
    digest: &str,
) -> Result<bool, OciRemoteError> {
    match agent
        .head(&remote.manifest_url(digest)?)
        .set("Accept", OCI_IMAGE_MANIFEST_MEDIA_TYPE)
        .call()
    {
        Ok(response) if response.status() < 400 => Ok(true),
        Err(ureq::Error::Status(404, _)) => Ok(false),
        Err(error) => Err(map_http_error(error)),
        Ok(response) => Err(OciRemoteError::new(
            OciRemoteErrorKind::Http,
            format!("unexpected manifest HEAD status {}", response.status()),
        )),
    }
}

fn upload_blob(
    agent: &ureq::Agent,
    remote: &OciRemote,
    digest: &str,
    bytes: &[u8],
) -> Result<(), OciRemoteError> {
    cache::verify_digest(digest, bytes).map_err(OciRemoteError::from)?;
    let response = agent
        .post(&remote.blob_uploads_url()?)
        .call()
        .map_err(map_http_error)?;
    let location = response.header("Location").ok_or_else(|| {
        OciRemoteError::new(
            OciRemoteErrorKind::Http,
            "OCI registry did not return a blob upload Location",
        )
    })?;
    let upload_url = remote.complete_upload_url(location, digest)?;
    let response = agent
        .put(&upload_url)
        .set("Content-Type", "application/octet-stream")
        .send_bytes(bytes)
        .map_err(map_http_error)?;
    if matches!(response.status(), 200..=299) {
        Ok(())
    } else {
        Err(OciRemoteError::new(
            OciRemoteErrorKind::Http,
            format!(
                "unexpected blob upload completion status {}",
                response.status()
            ),
        ))
    }
}

fn put_manifest(
    agent: &ureq::Agent,
    remote: &OciRemote,
    reference: &str,
    manifest_bytes: &[u8],
) -> Result<(), OciRemoteError> {
    let response = agent
        .put(&remote.manifest_url(reference)?)
        .set("Content-Type", OCI_IMAGE_MANIFEST_MEDIA_TYPE)
        .send_bytes(manifest_bytes)
        .map_err(map_http_error)?;
    if matches!(response.status(), 200..=299) {
        Ok(())
    } else {
        Err(OciRemoteError::new(
            OciRemoteErrorKind::Http,
            format!("unexpected manifest PUT status {}", response.status()),
        ))
    }
}

fn parse_manifest(bytes: &[u8]) -> Result<CanonOciManifest, OciRemoteError> {
    serde_json::from_slice(bytes).map_err(|error| {
        OciRemoteError::new(
            OciRemoteErrorKind::Parse,
            format!("failed to parse OCI manifest JSON: {error}"),
        )
    })
}

fn binding_from_manifest(
    manifest: &CanonOciManifest,
) -> Result<CanonPackageBinding, OciRemoteError> {
    let package_schema = manifest_annotation(manifest, ANNOTATION_PACKAGE_SCHEMA)?;
    Ok(CanonPackageBinding {
        artifact_class: artifact_class_for_schema(&package_schema)?,
        package_schema,
        package_id: manifest_annotation(manifest, ANNOTATION_PACKAGE_ID)?,
        package_version: manifest_annotation(manifest, ANNOTATION_PACKAGE_VERSION)?,
        package_digest: manifest_annotation(manifest, ANNOTATION_PACKAGE_DIGEST)?,
    })
}

fn manifest_annotation(manifest: &CanonOciManifest, key: &str) -> Result<String, OciRemoteError> {
    manifest
        .annotations
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| {
            OciRemoteError::new(
                OciRemoteErrorKind::OciContract,
                format!("OCI manifest is missing required annotation {key}"),
            )
        })
}

fn artifact_class_for_schema(schema: &str) -> Result<CanonArtifactClass, OciRemoteError> {
    match schema {
        oci::REGISTRY_PACKAGE_SCHEMA_ID => Ok(CanonArtifactClass::RegistryPackage),
        oci::STRATEGY_PACKAGE_SCHEMA_ID => Ok(CanonArtifactClass::StrategyPackage),
        oci::FACT_PACKAGE_SCHEMA_ID => Ok(CanonArtifactClass::FactPackage),
        oci::REVIEW_ATTESTATION_SCHEMA_ID => Ok(CanonArtifactClass::ReviewAttestation),
        oci::PROMOTION_ATTESTATION_SCHEMA_ID => Ok(CanonArtifactClass::PromotionAttestation),
        _ if schema.starts_with("canon.extension.") => {
            Ok(CanonArtifactClass::DomainExtensionPackage)
        }
        _ if schema.starts_with("canon.export.") => Ok(CanonArtifactClass::ExportProjection),
        _ => Err(OciRemoteError::new(
            OciRemoteErrorKind::OciContract,
            format!("unsupported Canon OCI package schema {schema}"),
        )),
    }
}

fn verify_descriptor_bytes(descriptor: &OciDescriptor, bytes: &[u8]) -> Result<(), OciRemoteError> {
    if descriptor.size != bytes.len() as u64 {
        return Err(OciRemoteError::new(
            OciRemoteErrorKind::DigestMismatch,
            format!(
                "descriptor {} size mismatch: expected {}, found {}",
                descriptor.digest,
                descriptor.size,
                bytes.len()
            ),
        ));
    }
    cache::verify_digest(&descriptor.digest, bytes).map_err(OciRemoteError::from)
}

fn read_response_bytes(response: ureq::Response) -> Result<Vec<u8>, OciRemoteError> {
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|error| {
            OciRemoteError::new(
                OciRemoteErrorKind::Http,
                format!("failed to read OCI response body: {error}"),
            )
        })?;
    Ok(bytes)
}

fn map_http_error(error: ureq::Error) -> OciRemoteError {
    match error {
        ureq::Error::Status(status, response) => {
            let body = response.into_string().unwrap_or_default();
            OciRemoteError::new(
                OciRemoteErrorKind::Http,
                format!("OCI registry returned status {status}: {body}"),
            )
        }
        ureq::Error::Transport(error) => OciRemoteError::new(
            OciRemoteErrorKind::Http,
            format!("OCI registry transport error: {error}"),
        ),
    }
}

fn normalized_reference(reference: &str) -> Result<String, OciRemoteError> {
    let reference = reference.trim();
    if reference.is_empty()
        || reference.starts_with('/')
        || reference.contains("..")
        || reference.contains(char::is_whitespace)
    {
        return Err(OciRemoteError::new(
            OciRemoteErrorKind::InvalidReference,
            format!("invalid OCI reference {reference:?}"),
        ));
    }
    Ok(reference.to_string())
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build()
}

impl OciRemote {
    pub fn new(base_url: impl Into<String>, repository: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            repository: repository.into().trim_matches('/').to_string(),
        }
    }

    fn manifest_url(&self, reference: &str) -> Result<String, OciRemoteError> {
        Ok(format!(
            "{}/v2/{}/manifests/{}",
            self.base_url,
            self.normalized_repository()?,
            normalized_reference(reference)?
        ))
    }

    fn blob_url(&self, digest: &str) -> Result<String, OciRemoteError> {
        cache::parse_digest(digest).map_err(OciRemoteError::from)?;
        Ok(format!(
            "{}/v2/{}/blobs/{}",
            self.base_url,
            self.normalized_repository()?,
            digest
        ))
    }

    fn blob_uploads_url(&self) -> Result<String, OciRemoteError> {
        Ok(format!(
            "{}/v2/{}/blobs/uploads/",
            self.base_url,
            self.normalized_repository()?
        ))
    }

    fn complete_upload_url(&self, location: &str, digest: &str) -> Result<String, OciRemoteError> {
        cache::parse_digest(digest).map_err(OciRemoteError::from)?;
        let mut url = if location.starts_with("http://") || location.starts_with("https://") {
            location.to_string()
        } else if location.starts_with('/') {
            format!("{}{}", self.base_url, location)
        } else {
            format!("{}/{}", self.base_url, location)
        };
        if url.contains('?') {
            url.push_str("&digest=");
        } else {
            url.push_str("?digest=");
        }
        url.push_str(digest);
        Ok(url)
    }

    fn normalized_repository(&self) -> Result<&str, OciRemoteError> {
        if self.repository.trim().is_empty()
            || self.repository.starts_with('/')
            || self.repository.ends_with('/')
            || self.repository.contains("..")
            || self.repository.contains(char::is_whitespace)
        {
            return Err(OciRemoteError::new(
                OciRemoteErrorKind::InvalidReference,
                format!("invalid OCI repository {:?}", self.repository),
            ));
        }
        Ok(&self.repository)
    }
}
