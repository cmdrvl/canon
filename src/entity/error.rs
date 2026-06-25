//! Entity-specific refusal helpers.
//!
//! Entity stages use the normal canon refusal envelope and exit semantics.
//! This module only provides a stable typed mapping from entity-stage refusal
//! kinds to the shared `RefusalCode` variants.

use crate::{Refusal, RefusalCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityRefusalKind {
    Profile,
    Strategy,
    InputContract,
    SurfaceIdCollision,
    PatchConflict,
    RegistrySnapshot,
    CacheMismatch,
    IndexLimit,
    CandidateBudget,
    ArtifactContract,
    CannotLinkOverride,
    ReviewImport,
    AuditGate,
    ApplyUnresolved,
    IoBudget,
}

impl EntityRefusalKind {
    pub const fn all() -> &'static [Self] {
        &[
            Self::Profile,
            Self::Strategy,
            Self::InputContract,
            Self::SurfaceIdCollision,
            Self::PatchConflict,
            Self::RegistrySnapshot,
            Self::CacheMismatch,
            Self::IndexLimit,
            Self::CandidateBudget,
            Self::ArtifactContract,
            Self::CannotLinkOverride,
            Self::ReviewImport,
            Self::AuditGate,
            Self::ApplyUnresolved,
            Self::IoBudget,
        ]
    }

    pub const fn refusal_code(self) -> RefusalCode {
        match self {
            Self::Profile => RefusalCode::EEntityProfile,
            Self::Strategy => RefusalCode::EEntityStrategy,
            Self::InputContract => RefusalCode::EEntityInputContract,
            Self::SurfaceIdCollision => RefusalCode::EEntitySurfaceIdCollision,
            Self::PatchConflict => RefusalCode::EEntityPatchConflict,
            Self::RegistrySnapshot => RefusalCode::EEntityRegistrySnapshot,
            Self::CacheMismatch => RefusalCode::EEntityCacheMismatch,
            Self::IndexLimit => RefusalCode::EEntityIndexLimit,
            Self::CandidateBudget => RefusalCode::EEntityCandidateBudget,
            Self::ArtifactContract => RefusalCode::EEntityArtifactContract,
            Self::CannotLinkOverride => RefusalCode::EEntityCannotLinkOverride,
            Self::ReviewImport => RefusalCode::EEntityReviewImport,
            Self::AuditGate => RefusalCode::EEntityAuditGate,
            Self::ApplyUnresolved => RefusalCode::EEntityApplyUnresolved,
            Self::IoBudget => RefusalCode::EEntityIoBudget,
        }
    }

    pub const fn code_str(self) -> &'static str {
        match self {
            Self::Profile => "E_ENTITY_PROFILE",
            Self::Strategy => "E_ENTITY_STRATEGY",
            Self::InputContract => "E_ENTITY_INPUT_CONTRACT",
            Self::SurfaceIdCollision => "E_ENTITY_SURFACE_ID_COLLISION",
            Self::PatchConflict => "E_ENTITY_PATCH_CONFLICT",
            Self::RegistrySnapshot => "E_ENTITY_REGISTRY_SNAPSHOT",
            Self::CacheMismatch => "E_ENTITY_CACHE_MISMATCH",
            Self::IndexLimit => "E_ENTITY_INDEX_LIMIT",
            Self::CandidateBudget => "E_ENTITY_CANDIDATE_BUDGET",
            Self::ArtifactContract => "E_ENTITY_ARTIFACT_CONTRACT",
            Self::CannotLinkOverride => "E_ENTITY_CANNOT_LINK_OVERRIDE",
            Self::ReviewImport => "E_ENTITY_REVIEW_IMPORT",
            Self::AuditGate => "E_ENTITY_AUDIT_GATE",
            Self::ApplyUnresolved => "E_ENTITY_APPLY_UNRESOLVED",
            Self::IoBudget => "E_ENTITY_IO_BUDGET",
        }
    }

    pub const fn stage_hint(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::Strategy => "strategy",
            Self::InputContract => "prepare",
            Self::SurfaceIdCollision => "prepare",
            Self::PatchConflict => "patch",
            Self::RegistrySnapshot => "registry_snapshot",
            Self::CacheMismatch => "cache",
            Self::IndexLimit => "index",
            Self::CandidateBudget => "block",
            Self::ArtifactContract => "artifact",
            Self::CannotLinkOverride => "solve",
            Self::ReviewImport => "review_import",
            Self::AuditGate => "audit",
            Self::ApplyUnresolved => "apply",
            Self::IoBudget => "io_budget",
        }
    }

    pub fn to_refusal(
        self,
        message: impl Into<String>,
        detail: serde_json::Value,
        next_command: Option<String>,
    ) -> Refusal {
        let code = self.refusal_code();
        Refusal {
            code: code.clone(),
            message: message.into(),
            detail,
            next_command: next_command.or_else(|| Some(code.default_next_command().to_string())),
        }
    }
}

pub fn entity_refusal(
    kind: EntityRefusalKind,
    message: impl Into<String>,
    detail: serde_json::Value,
) -> Refusal {
    kind.to_refusal(message, detail, None)
}
