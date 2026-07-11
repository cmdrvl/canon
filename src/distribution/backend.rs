use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fmt, fs,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

pub const FILESYSTEM_BACKEND_SCHEMA_VERSION: &str = "canon.distribution.backend.filesystem.v1";
pub const PUBLICATION_RECEIPT_SCHEMA_VERSION: &str = "canon.publication.receipt.v1";
pub const PUBLICATION_CONFLICT_SCHEMA_VERSION: &str = "canon.publication.conflict.v1";
pub const FILESYSTEM_BACKEND_KIND: &str = "filesystem";
pub const PROVIDER_URI_HANDLING_DEFERRED: &str =
    "provider_specific_uri_handling_deferred_until_backend_cas_and_oci_contracts_are_stable";

const OBJECT_ROOT: &str = "objects/blake3";
const PACKAGE_ROOT: &str = "packages";
const TAG_ROOT: &str = "tags";
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(10);
const LOCK_RETRY_ATTEMPTS: usize = 300;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendCapabilities {
    pub schema_version: String,
    pub backend_kind: String,
    pub content_addressed_objects: bool,
    pub read_by_digest: bool,
    pub create_if_absent: bool,
    pub compare_and_swap_tags: bool,
    pub immutable_package_history: bool,
    pub list_declared_ancestry: bool,
    pub deterministic_conflict_receipts: bool,
    pub atomic_replace_requires_same_filesystem: bool,
    pub requires_network: bool,
    pub provider_specific_uri_handling: String,
}

impl BackendCapabilities {
    pub fn filesystem() -> Self {
        Self {
            schema_version: FILESYSTEM_BACKEND_SCHEMA_VERSION.to_string(),
            backend_kind: FILESYSTEM_BACKEND_KIND.to_string(),
            content_addressed_objects: true,
            read_by_digest: true,
            create_if_absent: true,
            compare_and_swap_tags: true,
            immutable_package_history: true,
            list_declared_ancestry: true,
            deterministic_conflict_receipts: true,
            atomic_replace_requires_same_filesystem: true,
            requires_network: false,
            provider_specific_uri_handling: PROVIDER_URI_HANDLING_DEFERRED.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedPackageRef {
    pub package_id: String,
    pub package_version: String,
    pub content_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationCandidate {
    pub package: PublishedPackageRef,
    pub canonical_bytes_digest: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationRequest {
    pub channel: String,
    pub expected_base: PublishedPackageRef,
    pub expected_channel_digest: Option<String>,
    pub candidate_package_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmutableObjectWrite {
    pub digest: String,
    pub created: bool,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelCompareAndSwapRequest {
    pub channel: String,
    pub expected_base: PublishedPackageRef,
    pub expected_channel_digest: Option<String>,
    pub candidate: PublishedPackageRef,
    pub object_path: String,
    pub history_path: String,
    pub candidate_immutably_stored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelCompareAndSwapReceipt {
    pub outcome: PublicationOutcome,
    pub previous_channel_digest: Option<String>,
    pub current_head: PublishedPackageRef,
    pub tag_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationAncestry {
    pub parent: PublishedPackageRef,
    pub child: PublishedPackageRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationOutcome {
    Published,
    AlreadyPublished,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationReceipt {
    pub schema_version: String,
    pub outcome: PublicationOutcome,
    pub channel: String,
    pub package: PublishedPackageRef,
    pub expected_base: PublishedPackageRef,
    pub previous_channel_digest: Option<String>,
    pub current_channel_digest: String,
    pub object_path: String,
    pub history_path: String,
    pub tag_path: String,
    pub ancestry: PublicationAncestry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationConflictReceipt {
    pub schema_version: String,
    pub conflict_kind: String,
    pub channel: String,
    pub expected_base: PublishedPackageRef,
    pub expected_channel_digest: Option<String>,
    pub actual_head: Option<PublishedPackageRef>,
    pub candidate: PublishedPackageRef,
    pub candidate_immutably_stored: bool,
    pub reason: String,
    pub recovery_plan: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicationErrorKind {
    MissingExpectedBase,
    InvalidDigest,
    InvalidPackage,
    UnsafeBackend,
    Conflict,
    Io,
    Parse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationError {
    pub kind: PublicationErrorKind,
    pub message: String,
    pub conflict: Option<Box<PublicationConflictReceipt>>,
}

impl PublicationError {
    fn new(kind: PublicationErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            conflict: None,
        }
    }

    fn conflict(receipt: PublicationConflictReceipt) -> Self {
        Self {
            kind: PublicationErrorKind::Conflict,
            message: receipt.reason.clone(),
            conflict: Some(Box::new(receipt)),
        }
    }
}

impl fmt::Display for PublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PublicationError {}

impl From<io::Error> for PublicationError {
    fn from(error: io::Error) -> Self {
        Self::new(PublicationErrorKind::Io, error.to_string())
    }
}

/// Minimal publication seam for immutable package objects plus mutable channel
/// heads. Implementations must report their safety capabilities up front and
/// refuse publication when they cannot enforce content-addressed create-if-absent
/// object writes and compare-and-swap channel updates.
#[allow(dead_code)]
pub trait PublicationBackend {
    fn capabilities(&self) -> &BackendCapabilities;

    fn publish(&self, request: PublicationRequest) -> Result<PublicationReceipt, PublicationError>;

    fn current_head(&self, channel: &str) -> Result<Option<PublishedPackageRef>, PublicationError>;

    fn read_by_digest(&self, digest: &str) -> Result<Option<Vec<u8>>, PublicationError>;

    fn create_immutable_object_if_absent(
        &self,
        digest: &str,
        bytes: &[u8],
    ) -> Result<ImmutableObjectWrite, PublicationError>;

    fn compare_and_swap_channel(
        &self,
        request: ChannelCompareAndSwapRequest,
    ) -> Result<ChannelCompareAndSwapReceipt, PublicationError>;

    fn list_ancestry(
        &self,
        package: &PublishedPackageRef,
    ) -> Result<Vec<PublicationAncestry>, PublicationError>;
}

#[derive(Debug, Clone)]
pub struct FilesystemPublicationBackend {
    root: PathBuf,
    capabilities: BackendCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PackageHistoryRecord {
    schema_version: String,
    channel: String,
    package: PublishedPackageRef,
    parent: PublishedPackageRef,
    object_path: String,
    package_bytes_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ChannelTag {
    schema_version: String,
    channel: String,
    package: PublishedPackageRef,
    parent: PublishedPackageRef,
    object_path: String,
    history_path: String,
}

struct ReceiptPaths<'a> {
    object_path: &'a Path,
    history_path: &'a Path,
}

struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl FilesystemPublicationBackend {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            capabilities: BackendCapabilities::filesystem(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_capabilities_for_test(
        root: impl Into<PathBuf>,
        capabilities: BackendCapabilities,
    ) -> Self {
        Self {
            root: root.into(),
            capabilities,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn capabilities(&self) -> &BackendCapabilities {
        &self.capabilities
    }

    pub fn publish(
        &self,
        request: PublicationRequest,
    ) -> Result<PublicationReceipt, PublicationError> {
        <Self as PublicationBackend>::publish(self, request)
    }

    pub fn current_head(
        &self,
        channel: &str,
    ) -> Result<Option<PublishedPackageRef>, PublicationError> {
        <Self as PublicationBackend>::current_head(self, channel)
    }

    #[allow(dead_code)]
    pub fn read_by_digest(&self, digest: &str) -> Result<Option<Vec<u8>>, PublicationError> {
        <Self as PublicationBackend>::read_by_digest(self, digest)
    }

    pub fn create_immutable_object_if_absent(
        &self,
        digest: &str,
        bytes: &[u8],
    ) -> Result<ImmutableObjectWrite, PublicationError> {
        <Self as PublicationBackend>::create_immutable_object_if_absent(self, digest, bytes)
    }

    pub fn compare_and_swap_channel(
        &self,
        request: ChannelCompareAndSwapRequest,
    ) -> Result<ChannelCompareAndSwapReceipt, PublicationError> {
        <Self as PublicationBackend>::compare_and_swap_channel(self, request)
    }

    #[allow(dead_code)]
    pub fn list_ancestry(
        &self,
        package: &PublishedPackageRef,
    ) -> Result<Vec<PublicationAncestry>, PublicationError> {
        <Self as PublicationBackend>::list_ancestry(self, package)
    }
}

impl PublicationBackend for FilesystemPublicationBackend {
    fn capabilities(&self) -> &BackendCapabilities {
        &self.capabilities
    }

    fn publish(&self, request: PublicationRequest) -> Result<PublicationReceipt, PublicationError> {
        self.ensure_safe_capabilities()?;
        validate_publication_request(&request)?;
        let candidate = candidate_from_package_bytes(&request.candidate_package_bytes)?;
        fs::create_dir_all(&self.root)?;

        let object_write = self.create_immutable_object_if_absent(
            &candidate.package.content_digest,
            &request.candidate_package_bytes,
        )?;
        let object_path = self.root.join(&object_write.path);

        let history_record = PackageHistoryRecord {
            schema_version: PUBLICATION_RECEIPT_SCHEMA_VERSION.to_string(),
            channel: request.channel.clone(),
            package: candidate.package.clone(),
            parent: request.expected_base.clone(),
            object_path: relative_path_string(&self.root, &object_path)?,
            package_bytes_digest: candidate.canonical_bytes_digest.clone(),
        };
        let history_path = self.history_path(&history_record)?;
        let history_bytes = canonical_json_bytes(&history_record)?;
        let history_created = create_content_addressed_file(&history_path, &history_bytes)?;
        let candidate_immutably_stored =
            object_write.created || history_created || object_path.exists();
        let history_path_string = relative_path_string(&self.root, &history_path)?;

        let cas = self.compare_and_swap_channel(ChannelCompareAndSwapRequest {
            channel: request.channel.clone(),
            expected_base: request.expected_base.clone(),
            expected_channel_digest: request.expected_channel_digest.clone(),
            candidate: candidate.package.clone(),
            object_path: object_write.path,
            history_path: history_path_string,
            candidate_immutably_stored,
        })?;

        self.receipt_from_cas(
            cas,
            &request,
            &candidate.package,
            ReceiptPaths {
                object_path: &object_path,
                history_path: &history_path,
            },
        )
    }

    fn current_head(&self, channel: &str) -> Result<Option<PublishedPackageRef>, PublicationError> {
        Ok(self.read_tag(channel)?.map(|tag| tag.package))
    }

    fn read_by_digest(&self, digest: &str) -> Result<Option<Vec<u8>>, PublicationError> {
        validate_digest(digest)?;
        let path = self.object_path(digest)?;
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(PublicationError::new(
                PublicationErrorKind::Io,
                format!(
                    "failed to read immutable object {}: {error}",
                    path.display()
                ),
            )),
        }
    }

    fn create_immutable_object_if_absent(
        &self,
        digest: &str,
        bytes: &[u8],
    ) -> Result<ImmutableObjectWrite, PublicationError> {
        validate_digest(digest)?;
        let candidate = candidate_from_package_bytes(bytes)?;
        if candidate.package.content_digest != digest {
            return Err(PublicationError::new(
                PublicationErrorKind::InvalidPackage,
                format!(
                    "immutable object digest mismatch: expected {digest}, found {}",
                    candidate.package.content_digest
                ),
            ));
        }
        let path = self.object_path(digest)?;
        let created = create_content_addressed_file(&path, bytes)?;
        Ok(ImmutableObjectWrite {
            digest: digest.to_string(),
            created,
            path: relative_path_string(&self.root, &path)?,
        })
    }

    fn compare_and_swap_channel(
        &self,
        request: ChannelCompareAndSwapRequest,
    ) -> Result<ChannelCompareAndSwapReceipt, PublicationError> {
        self.ensure_safe_capabilities()?;
        validate_channel_preconditions(
            &request.channel,
            &request.expected_base,
            request.expected_channel_digest.as_ref(),
        )?;
        validate_digest(&request.candidate.content_digest)?;

        let tag_path = self.tag_path(&request.channel);
        let _guard = acquire_lock(&self.lock_path(&request.channel))?;
        let current = self.read_tag(&request.channel)?;
        if let Some(current_tag) = &current
            && current_tag.package.content_digest == request.candidate.content_digest
        {
            return Ok(ChannelCompareAndSwapReceipt {
                outcome: PublicationOutcome::AlreadyPublished,
                previous_channel_digest: current
                    .as_ref()
                    .map(|tag| tag.package.content_digest.clone()),
                current_head: request.candidate,
                tag_path: relative_path_string(&self.root, &tag_path)?,
            });
        }

        if let Some(conflict) = channel_precondition_conflict(
            &request.channel,
            &request.expected_base,
            request.expected_channel_digest.as_ref(),
            &request.candidate,
            current.as_ref(),
            request.candidate_immutably_stored,
        ) {
            return Err(PublicationError::conflict(conflict));
        }

        let tag = ChannelTag {
            schema_version: PUBLICATION_RECEIPT_SCHEMA_VERSION.to_string(),
            channel: request.channel.clone(),
            package: request.candidate.clone(),
            parent: request.expected_base,
            object_path: request.object_path,
            history_path: request.history_path,
        };
        atomic_replace(&tag_path, &canonical_json_bytes(&tag)?)?;

        Ok(ChannelCompareAndSwapReceipt {
            outcome: PublicationOutcome::Published,
            previous_channel_digest: current.map(|tag| tag.package.content_digest),
            current_head: request.candidate,
            tag_path: relative_path_string(&self.root, &tag_path)?,
        })
    }

    fn list_ancestry(
        &self,
        package: &PublishedPackageRef,
    ) -> Result<Vec<PublicationAncestry>, PublicationError> {
        let path = self.history_path_for_package(package)?;
        match fs::read(&path) {
            Ok(bytes) => {
                let record: PackageHistoryRecord =
                    serde_json::from_slice(&bytes).map_err(|error| {
                        PublicationError::new(
                            PublicationErrorKind::Parse,
                            format!(
                                "failed to parse package history {}: {error}",
                                path.display()
                            ),
                        )
                    })?;
                Ok(vec![PublicationAncestry {
                    parent: record.parent,
                    child: record.package,
                }])
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(PublicationError::new(
                PublicationErrorKind::Io,
                format!("failed to read package history {}: {error}", path.display()),
            )),
        }
    }
}

impl FilesystemPublicationBackend {
    fn read_tag(&self, channel: &str) -> Result<Option<ChannelTag>, PublicationError> {
        let path = self.tag_path(channel);
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|error| {
                PublicationError::new(
                    PublicationErrorKind::Parse,
                    format!("failed to parse channel tag {}: {error}", path.display()),
                )
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(PublicationError::new(
                PublicationErrorKind::Io,
                format!("failed to read channel tag {}: {error}", path.display()),
            )),
        }
    }

    fn ensure_safe_capabilities(&self) -> Result<(), PublicationError> {
        let capabilities = &self.capabilities;
        if capabilities.content_addressed_objects
            && capabilities.read_by_digest
            && capabilities.create_if_absent
            && capabilities.compare_and_swap_tags
            && capabilities.immutable_package_history
            && capabilities.deterministic_conflict_receipts
            && !capabilities.requires_network
        {
            Ok(())
        } else {
            Err(PublicationError::new(
                PublicationErrorKind::UnsafeBackend,
                "filesystem backend is missing required publication safety capabilities",
            ))
        }
    }

    fn object_path(&self, digest: &str) -> Result<PathBuf, PublicationError> {
        Ok(self
            .root
            .join(OBJECT_ROOT)
            .join(format!("{}.json", digest_hex(digest)?)))
    }

    fn history_path(&self, record: &PackageHistoryRecord) -> Result<PathBuf, PublicationError> {
        self.history_path_for_package(&record.package)
    }

    fn history_path_for_package(
        &self,
        package: &PublishedPackageRef,
    ) -> Result<PathBuf, PublicationError> {
        Ok(self
            .root
            .join(PACKAGE_ROOT)
            .join(sanitize_component(&package.package_id)?)
            .join(sanitize_component(&package.package_version)?)
            .join(format!("{}.json", digest_hex(&package.content_digest)?)))
    }

    fn tag_path(&self, channel: &str) -> PathBuf {
        self.root
            .join(TAG_ROOT)
            .join(format!("{}.json", sanitize_component_lossy(channel)))
    }

    fn lock_path(&self, channel: &str) -> PathBuf {
        self.root
            .join(TAG_ROOT)
            .join(format!("{}.lock", sanitize_component_lossy(channel)))
    }

    fn receipt_from_cas(
        &self,
        cas: ChannelCompareAndSwapReceipt,
        request: &PublicationRequest,
        package: &PublishedPackageRef,
        paths: ReceiptPaths<'_>,
    ) -> Result<PublicationReceipt, PublicationError> {
        Ok(PublicationReceipt {
            schema_version: PUBLICATION_RECEIPT_SCHEMA_VERSION.to_string(),
            outcome: cas.outcome,
            channel: request.channel.clone(),
            package: package.clone(),
            expected_base: request.expected_base.clone(),
            previous_channel_digest: cas.previous_channel_digest,
            current_channel_digest: cas.current_head.content_digest,
            object_path: relative_path_string(&self.root, paths.object_path)?,
            history_path: relative_path_string(&self.root, paths.history_path)?,
            tag_path: cas.tag_path,
            ancestry: PublicationAncestry {
                parent: request.expected_base.clone(),
                child: package.clone(),
            },
        })
    }
}

pub fn candidate_from_package_bytes(
    bytes: &[u8],
) -> Result<PublicationCandidate, PublicationError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        PublicationError::new(
            PublicationErrorKind::Parse,
            format!("failed to parse candidate package bytes: {error}"),
        )
    })?;
    let content_digest = required_string(&value, "content_digest")?;
    validate_digest(&content_digest)?;
    if value.get("package_id").is_some() || value.get("package_version").is_some() {
        let expected_digest = semantic_package_digest(&value)?;
        if content_digest != expected_digest {
            return Err(PublicationError::new(
                PublicationErrorKind::InvalidPackage,
                format!(
                    "candidate package content digest mismatch: expected {expected_digest}, found {content_digest}"
                ),
            ));
        }
    } else if value.get("registry").is_none() {
        return Err(PublicationError::new(
            PublicationErrorKind::InvalidPackage,
            "candidate package must include package_id/package_version or registry.id/registry.version",
        ));
    }

    let (package_id, package_version) = package_identity(&value)?;
    Ok(PublicationCandidate {
        package: PublishedPackageRef {
            package_id,
            package_version,
            content_digest,
        },
        canonical_bytes_digest: hash_bytes(bytes),
        bytes: bytes.len() as u64,
    })
}

fn validate_publication_request(request: &PublicationRequest) -> Result<(), PublicationError> {
    validate_channel_preconditions(
        &request.channel,
        &request.expected_base,
        request.expected_channel_digest.as_ref(),
    )
}

fn validate_channel_preconditions(
    channel: &str,
    expected_base: &PublishedPackageRef,
    expected_channel_digest: Option<&String>,
) -> Result<(), PublicationError> {
    if channel.trim().is_empty() {
        return Err(PublicationError::new(
            PublicationErrorKind::MissingExpectedBase,
            "publication requires a non-empty channel",
        ));
    }
    if expected_base.package_version.trim().is_empty()
        || expected_base.content_digest.trim().is_empty()
    {
        return Err(PublicationError::new(
            PublicationErrorKind::MissingExpectedBase,
            "publication requires expected base digest and version",
        ));
    }
    validate_digest(&expected_base.content_digest)?;
    if let Some(expected) = expected_channel_digest {
        validate_digest(expected)?;
    }
    Ok(())
}

fn channel_precondition_conflict(
    channel: &str,
    expected_base: &PublishedPackageRef,
    expected_channel_digest: Option<&String>,
    candidate: &PublishedPackageRef,
    current: Option<&ChannelTag>,
    candidate_immutably_stored: bool,
) -> Option<PublicationConflictReceipt> {
    let conflict = |kind: &str, reason: String| PublicationConflictReceipt {
        schema_version: PUBLICATION_CONFLICT_SCHEMA_VERSION.to_string(),
        conflict_kind: kind.to_string(),
        channel: channel.to_string(),
        expected_base: expected_base.clone(),
        expected_channel_digest: expected_channel_digest.cloned(),
        actual_head: current.map(|tag| tag.package.clone()),
        candidate: candidate.clone(),
        candidate_immutably_stored,
        reason,
        recovery_plan: recovery_plan(current.map(|tag| &tag.package), candidate),
    };

    match current {
        Some(tag) => {
            if let Some(expected) = expected_channel_digest {
                if expected != &tag.package.content_digest {
                    return Some(conflict(
                        "tag_conflict",
                        format!(
                            "channel {} moved from expected digest {} to actual digest {}",
                            channel, expected, tag.package.content_digest
                        ),
                    ));
                }
            } else {
                return Some(conflict(
                    "tag_exists",
                    format!(
                        "channel {} already points at {} and create-if-absent was requested",
                        channel, tag.package.content_digest
                    ),
                ));
            }

            if tag.package.content_digest != expected_base.content_digest
                || tag.package.package_version != expected_base.package_version
            {
                return Some(conflict(
                    "stale_base",
                    format!(
                        "expected base {}@{} ({}) does not match actual head {}@{} ({})",
                        expected_base.package_id,
                        expected_base.package_version,
                        expected_base.content_digest,
                        tag.package.package_id,
                        tag.package.package_version,
                        tag.package.content_digest
                    ),
                ));
            }
            None
        }
        None => expected_channel_digest.map(|expected| {
            conflict(
                "missing_head",
                format!(
                    "channel {} has no head but caller expected {}",
                    channel, expected
                ),
            )
        }),
    }
}

fn recovery_plan(
    actual_head: Option<&PublishedPackageRef>,
    candidate: &PublishedPackageRef,
) -> Vec<String> {
    match actual_head {
        Some(head) => vec![
            format!(
                "fetch channel head {}@{} ({})",
                head.package_id, head.package_version, head.content_digest
            ),
            format!(
                "rebuild candidate {}@{} on top of the fetched head",
                candidate.package_id, candidate.package_version
            ),
            "retry publication with expected_base and expected_channel_digest set to the fetched head digest".to_string(),
        ],
        None => vec![
            "refresh channel state before retrying".to_string(),
            "retry create-if-absent publication only if the channel is still absent".to_string(),
        ],
    }
}

fn create_content_addressed_file(path: &Path, bytes: &[u8]) -> Result<bool, PublicationError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp = temp_path(path)?;
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| {
                PublicationError::new(
                    PublicationErrorKind::Io,
                    format!("failed to create temp file {}: {error}", temp.display()),
                )
            })?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    match fs::hard_link(&temp, path) {
        Ok(()) => {
            let _ = fs::remove_file(&temp);
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(&temp);
            let existing = fs::read(path)?;
            if existing == bytes {
                Ok(false)
            } else {
                Err(PublicationError::new(
                    PublicationErrorKind::InvalidDigest,
                    format!(
                        "content-addressed path {} already exists with different bytes",
                        path.display()
                    ),
                ))
            }
        }
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(PublicationError::new(
                PublicationErrorKind::Io,
                format!(
                    "failed to create immutable path {}: {error}",
                    path.display()
                ),
            ))
        }
    }
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), PublicationError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = temp_path(path)?;
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        PublicationError::new(
            PublicationErrorKind::Io,
            format!("failed to atomically replace {}: {error}", path.display()),
        )
    })
}

fn acquire_lock(path: &Path) -> Result<LockGuard, PublicationError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    for _ in 0..LOCK_RETRY_ATTEMPTS {
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut file) => {
                file.write_all(b"canon.filesystem-publication-lock.v1\n")?;
                file.sync_all()?;
                return Ok(LockGuard {
                    path: path.to_path_buf(),
                });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                thread::sleep(LOCK_RETRY_DELAY);
            }
            Err(error) => {
                return Err(PublicationError::new(
                    PublicationErrorKind::Io,
                    error.to_string(),
                ));
            }
        }
    }
    Err(PublicationError::new(
        PublicationErrorKind::Conflict,
        format!("publication lock is still held at {}", path.display()),
    ))
}

fn temp_path(path: &Path) -> Result<PathBuf, PublicationError> {
    let parent = path.parent().ok_or_else(|| {
        PublicationError::new(
            PublicationErrorKind::Io,
            format!("path {} has no parent directory", path.display()),
        )
    })?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            PublicationError::new(
                PublicationErrorKind::Io,
                format!("path {} has no UTF-8 file name", path.display()),
            )
        })?;
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    Ok(parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), counter)))
}

fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, PublicationError> {
    serde_json::to_vec(value).map_err(|error| {
        PublicationError::new(
            PublicationErrorKind::Parse,
            format!("failed to serialize publication JSON: {error}"),
        )
    })
}

fn semantic_package_digest(value: &Value) -> Result<String, PublicationError> {
    let mut digest_view = value.clone();
    let object = digest_view.as_object_mut().ok_or_else(|| {
        PublicationError::new(
            PublicationErrorKind::InvalidPackage,
            "candidate package bytes must be a JSON object",
        )
    })?;
    object.insert("content_digest".to_string(), Value::String(String::new()));
    let bytes = serde_json::to_vec(&digest_view).map_err(|error| {
        PublicationError::new(
            PublicationErrorKind::Parse,
            format!("failed to serialize candidate digest view: {error}"),
        )
    })?;
    Ok(hash_bytes(&bytes))
}

fn package_identity(value: &Value) -> Result<(String, String), PublicationError> {
    if value.get("package_id").is_some() || value.get("package_version").is_some() {
        return Ok((
            required_string(value, "package_id")?,
            required_string(value, "package_version")?,
        ));
    }
    let registry = value.get("registry").ok_or_else(|| {
        PublicationError::new(
            PublicationErrorKind::InvalidPackage,
            "candidate package must include package_id/package_version or registry.id/registry.version",
        )
    })?;
    let id = required_nested_string(registry, "registry.id", "id")?;
    let version = required_nested_string(registry, "registry.version", "version")?;
    Ok((id, version))
}

fn required_string(value: &Value, field: &str) -> Result<String, PublicationError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            PublicationError::new(
                PublicationErrorKind::InvalidPackage,
                format!("candidate package must include non-empty {field}"),
            )
        })
}

fn required_nested_string(
    value: &Value,
    label: &str,
    field: &str,
) -> Result<String, PublicationError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            PublicationError::new(
                PublicationErrorKind::InvalidPackage,
                format!("candidate package must include non-empty {label}"),
            )
        })
}

fn validate_digest(digest: &str) -> Result<(), PublicationError> {
    let hex = digest_hex(digest)?;
    if hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(PublicationError::new(
            PublicationErrorKind::InvalidDigest,
            format!("invalid content digest {digest}"),
        ))
    }
}

fn digest_hex(digest: &str) -> Result<&str, PublicationError> {
    digest.strip_prefix("blake3:").ok_or_else(|| {
        PublicationError::new(
            PublicationErrorKind::InvalidDigest,
            format!("invalid content digest {digest}"),
        )
    })
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn relative_path_string(root: &Path, path: &Path) -> Result<String, PublicationError> {
    path.strip_prefix(root)
        .map_err(|error| {
            PublicationError::new(
                PublicationErrorKind::Io,
                format!("failed to derive relative publication path: {error}"),
            )
        })
        .and_then(|relative| {
            relative
                .to_str()
                .map(|value| value.replace('\\', "/"))
                .ok_or_else(|| {
                    PublicationError::new(
                        PublicationErrorKind::Io,
                        format!("publication path {} is not UTF-8", path.display()),
                    )
                })
        })
}

fn sanitize_component(component: &str) -> Result<String, PublicationError> {
    let sanitized = sanitize_component_lossy(component);
    if sanitized.is_empty() {
        Err(PublicationError::new(
            PublicationErrorKind::InvalidPackage,
            "path component must not be empty",
        ))
    } else {
        Ok(sanitized)
    }
}

fn sanitize_component_lossy(component: &str) -> String {
    component
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'_' | b'-' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02x}"),
        })
        .collect()
}
