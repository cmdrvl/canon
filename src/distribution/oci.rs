#![forbid(unsafe_code)]

//! OCI artifact media-type contract for Canon packages and attestations.
//!
//! OCI is transport and discovery, not the source of truth for registry or
//! strategy semantics. Canon package digests remain the semantic identity; OCI
//! manifests bind those semantic digests to transport descriptors and subjects.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt};

pub const OCI_IMAGE_MANIFEST_MEDIA_TYPE: &str = "application/vnd.oci.image.manifest.v1+json";
pub const OCI_IMAGE_INDEX_MEDIA_TYPE: &str = "application/vnd.oci.image.index.v1+json";
pub const CANON_OCI_CONFIG_MEDIA_TYPE: &str = "application/vnd.cmdrvl.canon.oci.config.v1+json";
pub const CANON_OCI_LAYOUT_VERSION: &str = "1.0.0";
pub const CANON_LAYER_ROLE_PRIMARY: &str = "primary";
pub const CANON_LAYER_ROLE_EXTENSION: &str = "extension";
pub const CANON_VERIFY_EXTENSION_POLICY: &str = "preserve-but-ignore-for-semantic-verify";
pub const ANNOTATION_PACKAGE_SCHEMA: &str = "io.cmdrvl.canon.package.schema";
pub const ANNOTATION_PACKAGE_DIGEST: &str = "io.cmdrvl.canon.package.digest";
pub const ANNOTATION_LAYER_ROLE: &str = "io.cmdrvl.canon.layer.role";
pub const ANNOTATION_PACKAGE_ID: &str = "io.cmdrvl.canon.package.id";
pub const ANNOTATION_PACKAGE_VERSION: &str = "org.opencontainers.image.version";
pub const ANNOTATION_EXTENSION_POLICY: &str = "io.cmdrvl.canon.verify.extension-policy";
pub const ANNOTATION_REF_NAME: &str = "org.opencontainers.image.ref.name";

pub const REGISTRY_PACKAGE_SCHEMA_ID: &str = "canon.registry.package.v1";
pub const STRATEGY_PACKAGE_SCHEMA_ID: &str = "canon.strategy.package.v1";
pub const FACT_PACKAGE_SCHEMA_ID: &str = "canon.identity.fact.package.v1";
pub const REVIEW_ATTESTATION_SCHEMA_ID: &str = "canon.review.attestation.v1";
pub const PROMOTION_ATTESTATION_SCHEMA_ID: &str = "canon.promotion.attestation.v1";

pub type OciResult<T> = Result<T, OciContractError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OciContractErrorCode {
    ArtifactContract,
    CompatibilityPolicy,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OciContractError {
    pub code: OciContractErrorCode,
    pub message: String,
}

impl OciContractError {
    fn new(code: OciContractErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for OciContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for OciContractError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonArtifactClass {
    RegistryPackage,
    StrategyPackage,
    FactPackage,
    DomainExtensionPackage,
    ReviewAttestation,
    PromotionAttestation,
    ExportProjection,
}

impl CanonArtifactClass {
    pub const fn requires_subject(self) -> bool {
        matches!(
            self,
            Self::ReviewAttestation | Self::PromotionAttestation | Self::ExportProjection
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonPackageBinding {
    pub artifact_class: CanonArtifactClass,
    pub package_schema: String,
    pub package_id: String,
    pub package_version: String,
    pub package_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OciDescriptor {
    pub media_type: String,
    pub digest: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonOciManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    #[serde(rename = "artifactType")]
    pub artifact_type: String,
    pub config: OciDescriptor,
    pub layers: Vec<OciDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<OciDescriptor>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonOciIndex {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub manifests: Vec<OciDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonOciLayout {
    pub image_layout_version: String,
    pub index: CanonOciIndex,
}

pub fn payload_media_type(binding: &CanonPackageBinding) -> OciResult<String> {
    validate_binding(binding)?;
    Ok(schema_id_to_media_type(&binding.package_schema))
}

pub fn build_manifest(
    binding: &CanonPackageBinding,
    config_descriptor: OciDescriptor,
    primary_payload: OciDescriptor,
    subject: Option<OciDescriptor>,
    extension_layers: Vec<OciDescriptor>,
) -> OciResult<CanonOciManifest> {
    let binding = finalize_binding(binding.clone())?;
    validate_transport_descriptor(&config_descriptor, "config")?;
    if config_descriptor.media_type != CANON_OCI_CONFIG_MEDIA_TYPE {
        return Err(artifact_contract_error(format!(
            "config descriptor must use {}",
            CANON_OCI_CONFIG_MEDIA_TYPE
        )));
    }

    let artifact_type = schema_id_to_media_type(&binding.package_schema);
    let mut primary_payload = finalize_primary_layer(primary_payload, &binding, &artifact_type)?;
    let mut extension_layers = extension_layers
        .into_iter()
        .map(finalize_extension_layer)
        .collect::<OciResult<Vec<_>>>()?;
    extension_layers.sort_by(|left, right| {
        left.media_type
            .cmp(&right.media_type)
            .then_with(|| left.digest.cmp(&right.digest))
    });

    let subject = subject
        .map(|descriptor| {
            validate_transport_descriptor(&descriptor, "subject")?;
            Ok(descriptor)
        })
        .transpose()?;

    let mut annotations = BTreeMap::new();
    annotations.insert(
        ANNOTATION_PACKAGE_SCHEMA.to_string(),
        binding.package_schema.clone(),
    );
    annotations.insert(
        ANNOTATION_PACKAGE_DIGEST.to_string(),
        binding.package_digest.clone(),
    );
    annotations.insert(
        ANNOTATION_PACKAGE_ID.to_string(),
        binding.package_id.clone(),
    );
    annotations.insert(
        ANNOTATION_PACKAGE_VERSION.to_string(),
        binding.package_version.clone(),
    );
    annotations.insert(
        ANNOTATION_EXTENSION_POLICY.to_string(),
        CANON_VERIFY_EXTENSION_POLICY.to_string(),
    );

    primary_payload.annotations.insert(
        ANNOTATION_PACKAGE_SCHEMA.to_string(),
        binding.package_schema.clone(),
    );
    primary_payload.annotations.insert(
        ANNOTATION_PACKAGE_DIGEST.to_string(),
        binding.package_digest.clone(),
    );

    let manifest = CanonOciManifest {
        schema_version: 2,
        media_type: OCI_IMAGE_MANIFEST_MEDIA_TYPE.to_string(),
        artifact_type,
        config: config_descriptor,
        layers: std::iter::once(primary_payload)
            .chain(extension_layers)
            .collect(),
        subject,
        annotations,
    };
    validate_manifest(&manifest, &binding)?;
    Ok(manifest)
}

pub fn build_local_layout(
    ref_name: &str,
    manifest_descriptor: OciDescriptor,
) -> OciResult<CanonOciLayout> {
    let ref_name = normalized_non_empty(ref_name, "ref_name")?;
    validate_transport_descriptor(&manifest_descriptor, "manifest descriptor")?;
    if manifest_descriptor.media_type != OCI_IMAGE_MANIFEST_MEDIA_TYPE {
        return Err(artifact_contract_error(format!(
            "local OCI layout manifests must point to {}",
            OCI_IMAGE_MANIFEST_MEDIA_TYPE
        )));
    }

    let mut descriptor = manifest_descriptor;
    descriptor
        .annotations
        .insert(ANNOTATION_REF_NAME.to_string(), ref_name);
    Ok(CanonOciLayout {
        image_layout_version: CANON_OCI_LAYOUT_VERSION.to_string(),
        index: CanonOciIndex {
            schema_version: 2,
            media_type: OCI_IMAGE_INDEX_MEDIA_TYPE.to_string(),
            manifests: vec![descriptor],
        },
    })
}

pub fn canonical_manifest_bytes(manifest: &CanonOciManifest) -> OciResult<Vec<u8>> {
    let manifest = canonicalized_manifest(manifest)?;
    serde_json::to_vec(&manifest)
        .map_err(|error| artifact_contract_error(format!("failed to serialize manifest: {error}")))
}

pub fn validate_binding(binding: &CanonPackageBinding) -> OciResult<()> {
    let _ = finalize_binding(binding.clone())?;
    Ok(())
}

pub fn validate_manifest(
    manifest: &CanonOciManifest,
    expected_binding: &CanonPackageBinding,
) -> OciResult<()> {
    let binding = finalize_binding(expected_binding.clone())?;
    let manifest = canonicalized_manifest(manifest)?;
    let expected_artifact_type = schema_id_to_media_type(&binding.package_schema);

    if manifest.schema_version != 2 {
        return Err(artifact_contract_error(format!(
            "manifest schemaVersion must be 2, got {}",
            manifest.schema_version
        )));
    }
    if manifest.media_type != OCI_IMAGE_MANIFEST_MEDIA_TYPE {
        return Err(artifact_contract_error(format!(
            "manifest mediaType must be {}",
            OCI_IMAGE_MANIFEST_MEDIA_TYPE
        )));
    }
    if manifest.artifact_type != expected_artifact_type {
        return Err(compatibility_policy_error(format!(
            "manifest artifactType {} does not match expected {}",
            manifest.artifact_type, expected_artifact_type
        )));
    }

    validate_transport_descriptor(&manifest.config, "config")?;
    if manifest.config.media_type != CANON_OCI_CONFIG_MEDIA_TYPE {
        return Err(artifact_contract_error(format!(
            "config descriptor must use {}",
            CANON_OCI_CONFIG_MEDIA_TYPE
        )));
    }

    let schema_annotation = manifest
        .annotations
        .get(ANNOTATION_PACKAGE_SCHEMA)
        .ok_or_else(|| artifact_contract_error("manifest is missing package schema annotation"))?;
    if schema_annotation != &binding.package_schema {
        return Err(compatibility_policy_error(format!(
            "manifest package schema {} does not match expected {}",
            schema_annotation, binding.package_schema
        )));
    }
    let digest_annotation = manifest
        .annotations
        .get(ANNOTATION_PACKAGE_DIGEST)
        .ok_or_else(|| artifact_contract_error("manifest is missing package digest annotation"))?;
    if digest_annotation != &binding.package_digest {
        return Err(compatibility_policy_error(format!(
            "manifest package digest {} does not match expected {}",
            digest_annotation, binding.package_digest
        )));
    }
    if manifest.annotations.get(ANNOTATION_PACKAGE_ID) != Some(&binding.package_id) {
        return Err(compatibility_policy_error(
            "manifest package id annotation does not match expected package id",
        ));
    }
    if manifest.annotations.get(ANNOTATION_PACKAGE_VERSION) != Some(&binding.package_version) {
        return Err(compatibility_policy_error(
            "manifest package version annotation does not match expected package version",
        ));
    }
    if manifest.annotations.get(ANNOTATION_EXTENSION_POLICY)
        != Some(&CANON_VERIFY_EXTENSION_POLICY.to_string())
    {
        return Err(artifact_contract_error(
            "manifest must declare the extension-layer verify policy",
        ));
    }

    match (&manifest.subject, binding.artifact_class.requires_subject()) {
        (Some(subject), true) => validate_transport_descriptor(subject, "subject")?,
        (None, true) => {
            return Err(artifact_contract_error(
                "attestations and export projections require an immutable subject descriptor",
            ));
        }
        (Some(_), false) => {
            return Err(artifact_contract_error(
                "primary package artifacts must not carry a subject descriptor",
            ));
        }
        (None, false) => {}
    }

    if manifest.layers.is_empty() {
        return Err(artifact_contract_error(
            "manifest must include exactly one primary payload layer",
        ));
    }

    let mut saw_primary = false;
    for (index, layer) in manifest.layers.iter().enumerate() {
        validate_transport_descriptor(layer, "layer")?;
        let role = layer
            .annotations
            .get(ANNOTATION_LAYER_ROLE)
            .ok_or_else(|| artifact_contract_error("every layer must declare a role"))?;

        if role == CANON_LAYER_ROLE_PRIMARY {
            if index != 0 {
                return Err(artifact_contract_error(
                    "primary payload layer must be the first manifest layer",
                ));
            }
            if saw_primary {
                return Err(artifact_contract_error(
                    "manifest must not contain multiple primary payload layers",
                ));
            }
            if layer.media_type != expected_artifact_type {
                return Err(compatibility_policy_error(format!(
                    "primary payload media type {} does not match expected {}",
                    layer.media_type, expected_artifact_type
                )));
            }
            if layer.annotations.get(ANNOTATION_PACKAGE_DIGEST) != Some(&binding.package_digest) {
                return Err(compatibility_policy_error(
                    "primary payload layer must repeat the canonical package digest",
                ));
            }
            if layer.annotations.get(ANNOTATION_PACKAGE_SCHEMA) != Some(&binding.package_schema) {
                return Err(compatibility_policy_error(
                    "primary payload layer must repeat the package schema annotation",
                ));
            }
            saw_primary = true;
        } else if role == CANON_LAYER_ROLE_EXTENSION {
            if layer.media_type == expected_artifact_type {
                return Err(artifact_contract_error(
                    "extension layers must not reuse the primary payload media type",
                ));
            }
        } else {
            return Err(artifact_contract_error(format!(
                "unknown OCI layer role {}",
                role
            )));
        }
    }

    if !saw_primary {
        return Err(artifact_contract_error(
            "manifest must include exactly one primary payload layer",
        ));
    }

    Ok(())
}

fn canonicalized_manifest(manifest: &CanonOciManifest) -> OciResult<CanonOciManifest> {
    let mut canonical = manifest.clone();
    let mut primary = Vec::new();
    let mut extensions = Vec::new();
    for layer in canonical.layers {
        let role = layer
            .annotations
            .get(ANNOTATION_LAYER_ROLE)
            .cloned()
            .unwrap_or_default();
        if role == CANON_LAYER_ROLE_PRIMARY {
            primary.push(layer);
        } else {
            extensions.push(layer);
        }
    }
    extensions.sort_by(|left, right| {
        left.media_type
            .cmp(&right.media_type)
            .then_with(|| left.digest.cmp(&right.digest))
    });
    canonical.layers = primary.into_iter().chain(extensions).collect();
    Ok(canonical)
}

fn finalize_binding(mut binding: CanonPackageBinding) -> OciResult<CanonPackageBinding> {
    binding.package_schema = normalized_non_empty(&binding.package_schema, "package_schema")?;
    binding.package_id = normalized_non_empty(&binding.package_id, "package_id")?;
    binding.package_version = normalized_non_empty(&binding.package_version, "package_version")?;
    binding.package_digest = normalized_blake3_digest(&binding.package_digest, "package_digest")?;

    match binding.artifact_class {
        CanonArtifactClass::RegistryPackage => {
            require_exact_schema(&binding.package_schema, REGISTRY_PACKAGE_SCHEMA_ID)?
        }
        CanonArtifactClass::StrategyPackage => {
            require_exact_schema(&binding.package_schema, STRATEGY_PACKAGE_SCHEMA_ID)?
        }
        CanonArtifactClass::FactPackage => {
            require_exact_schema(&binding.package_schema, FACT_PACKAGE_SCHEMA_ID)?
        }
        CanonArtifactClass::ReviewAttestation => {
            require_exact_schema(&binding.package_schema, REVIEW_ATTESTATION_SCHEMA_ID)?
        }
        CanonArtifactClass::PromotionAttestation => {
            require_exact_schema(&binding.package_schema, PROMOTION_ATTESTATION_SCHEMA_ID)?
        }
        CanonArtifactClass::DomainExtensionPackage => {
            if !binding.package_schema.starts_with("canon.extension.") {
                return Err(compatibility_policy_error(format!(
                    "domain extension schema {} must start with canon.extension.",
                    binding.package_schema
                )));
            }
        }
        CanonArtifactClass::ExportProjection => {
            if !binding.package_schema.starts_with("canon.export.") {
                return Err(compatibility_policy_error(format!(
                    "export projection schema {} must start with canon.export.",
                    binding.package_schema
                )));
            }
        }
    }

    Ok(binding)
}

fn finalize_primary_layer(
    mut layer: OciDescriptor,
    binding: &CanonPackageBinding,
    expected_media_type: &str,
) -> OciResult<OciDescriptor> {
    validate_transport_descriptor(&layer, "primary payload layer")?;
    if layer.media_type != expected_media_type {
        return Err(compatibility_policy_error(format!(
            "primary payload media type {} does not match schema-derived {}",
            layer.media_type, expected_media_type
        )));
    }
    layer.annotations.insert(
        ANNOTATION_LAYER_ROLE.to_string(),
        CANON_LAYER_ROLE_PRIMARY.to_string(),
    );
    layer.annotations.insert(
        ANNOTATION_PACKAGE_SCHEMA.to_string(),
        binding.package_schema.clone(),
    );
    layer.annotations.insert(
        ANNOTATION_PACKAGE_DIGEST.to_string(),
        binding.package_digest.clone(),
    );
    Ok(layer)
}

fn finalize_extension_layer(mut layer: OciDescriptor) -> OciResult<OciDescriptor> {
    validate_transport_descriptor(&layer, "extension layer")?;
    layer.annotations.insert(
        ANNOTATION_LAYER_ROLE.to_string(),
        CANON_LAYER_ROLE_EXTENSION.to_string(),
    );
    Ok(layer)
}

fn validate_transport_descriptor(descriptor: &OciDescriptor, field: &str) -> OciResult<()> {
    normalized_non_empty(&descriptor.media_type, &format!("{field}.media_type"))?;
    normalized_sha256_digest(&descriptor.digest, &format!("{field}.digest"))?;
    Ok(())
}

fn schema_id_to_media_type(schema_id: &str) -> String {
    format!("application/vnd.cmdrvl.{schema_id}+json")
}

fn require_exact_schema(actual: &str, expected: &str) -> OciResult<()> {
    if actual != expected {
        return Err(compatibility_policy_error(format!(
            "artifact class requires schema {}, got {}",
            expected, actual
        )));
    }
    Ok(())
}

fn normalized_sha256_digest(value: &str, field: &str) -> OciResult<String> {
    let value = normalized_non_empty(value, field)?;
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(artifact_contract_error(format!(
            "{field} must start with sha256:"
        )));
    };
    if hex.len() != 64
        || !hex
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    {
        return Err(artifact_contract_error(format!(
            "{field} must contain 64 lowercase hex characters"
        )));
    }
    Ok(value)
}

fn normalized_blake3_digest(value: &str, field: &str) -> OciResult<String> {
    let value = normalized_non_empty(value, field)?;
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must start with blake3:"
        )));
    };
    if hex.len() != 64
        || !hex
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    {
        return Err(artifact_contract_error(format!(
            "{field} must contain 64 lowercase hex characters"
        )));
    }
    Ok(value)
}

fn normalized_non_empty(value: &str, field: &str) -> OciResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must not be empty"
        )));
    }
    Ok(value.to_string())
}

fn artifact_contract_error(message: impl Into<String>) -> OciContractError {
    OciContractError::new(OciContractErrorCode::ArtifactContract, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> OciContractError {
    OciContractError::new(OciContractErrorCode::CompatibilityPolicy, message)
}
