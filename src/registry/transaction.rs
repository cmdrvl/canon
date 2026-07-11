use crate::{
    distribution::backend::{
        PublicationBackend, PublicationConflictReceipt, PublicationError, PublicationErrorKind,
        PublicationReceipt, PublicationRequest, PublishedPackageRef,
    },
    registry::{canonical_package_bytes, compile_registry_package, parse_registry_package},
};
use std::{fmt, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPublicationTransaction {
    pub registry_dir: PathBuf,
    pub channel: String,
    pub expected_base: PublishedPackageRef,
    pub expected_channel_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPublicationOutput {
    pub package: PublishedPackageRef,
    pub receipt: PublicationReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryTransactionErrorKind {
    PackageBuild,
    PackageVerify,
    Publication,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryTransactionError {
    pub kind: RegistryTransactionErrorKind,
    pub message: String,
    pub conflict: Option<Box<PublicationConflictReceipt>>,
}

impl RegistryTransactionError {
    fn new(kind: RegistryTransactionErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            conflict: None,
        }
    }

    fn from_publication(error: PublicationError) -> Self {
        Self {
            kind: RegistryTransactionErrorKind::Publication,
            message: error.message,
            conflict: error.conflict,
        }
    }
}

impl fmt::Display for RegistryTransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RegistryTransactionError {}

pub fn publish_registry_transaction(
    backend: &impl PublicationBackend,
    transaction: RegistryPublicationTransaction,
) -> Result<RegistryPublicationOutput, RegistryTransactionError> {
    let package = compile_registry_package(&transaction.registry_dir).map_err(|error| {
        RegistryTransactionError::new(
            RegistryTransactionErrorKind::PackageBuild,
            format!(
                "failed to build registry package for {}: {error}",
                transaction.registry_dir.display()
            ),
        )
    })?;
    let candidate_bytes = canonical_package_bytes(&package).map_err(|error| {
        RegistryTransactionError::new(
            RegistryTransactionErrorKind::PackageBuild,
            format!("failed to serialize registry package candidate: {error}"),
        )
    })?;

    parse_registry_package(&candidate_bytes).map_err(|error| {
        RegistryTransactionError::new(
            RegistryTransactionErrorKind::PackageVerify,
            format!("candidate registry package failed verification before publish: {error}"),
        )
    })?;

    let receipt = backend
        .publish(PublicationRequest {
            channel: transaction.channel,
            expected_base: transaction.expected_base,
            expected_channel_digest: transaction.expected_channel_digest,
            candidate_package_bytes: candidate_bytes,
        })
        .map_err(|error| {
            if matches!(error.kind, PublicationErrorKind::Conflict) {
                RegistryTransactionError::from_publication(error)
            } else {
                RegistryTransactionError::new(
                    RegistryTransactionErrorKind::Publication,
                    error.to_string(),
                )
            }
        })?;

    Ok(RegistryPublicationOutput {
        package: receipt.package.clone(),
        receipt,
    })
}
