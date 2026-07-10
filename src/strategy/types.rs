use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, error::Error, fmt};

pub const STRATEGY_SCHEMA_SCOPE: &str = "structural-envelope";
pub const STRATEGY_KIND_DOCTRINE_AUTHORITY: &str =
    "rust::canon::strategy::types::StrategyDefinition::validate";

pub fn strategy_schema_version() -> &'static str {
    concat!("canon.strategy", ".v1")
}

pub type StrategyDoctrineResult<T> = Result<T, StrategyDoctrineError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyDoctrineErrorCode {
    UnknownKind,
    IncompatibleFields,
    AmbiguousMigration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDoctrineError {
    pub code: StrategyDoctrineErrorCode,
    pub message: String,
    pub next_action: String,
}

impl StrategyDoctrineError {
    fn new(
        code: StrategyDoctrineErrorCode,
        message: impl Into<String>,
        next_action: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            next_action: next_action.into(),
        }
    }
}

impl fmt::Display for StrategyDoctrineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for StrategyDoctrineError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyKind {
    IdentityEvidence,
    RecordLinkage,
    SchemaTransform,
    TaskTransform,
}

impl StrategyKind {
    pub const fn all() -> [Self; 4] {
        [
            Self::IdentityEvidence,
            Self::RecordLinkage,
            Self::SchemaTransform,
            Self::TaskTransform,
        ]
    }

    pub const fn doctrine(self) -> StrategyKindDoctrine {
        match self {
            Self::IdentityEvidence => StrategyKindDoctrine {
                kind: Self::IdentityEvidence,
                selection_key_kind: StrategySelectionKeyKind::Profile,
                input_kind: StrategyAllowedInputKind::ProfiledObservations,
                output_kind: StrategyOutputKind::EvidenceBundle,
                compatibility_kind: StrategyCompatibilityKind::ProfileScoped,
                execution_mode: StrategyExecutionMode::WorkbenchExecution,
                promotion_target: StrategyPromotionTarget::RegistryKnowledgePromotion,
            },
            Self::RecordLinkage => StrategyKindDoctrine {
                kind: Self::RecordLinkage,
                selection_key_kind: StrategySelectionKeyKind::LinkageMap,
                input_kind: StrategyAllowedInputKind::TwoTapeRecords,
                output_kind: StrategyOutputKind::LinkageBundle,
                compatibility_kind: StrategyCompatibilityKind::FieldMapScoped,
                execution_mode: StrategyExecutionMode::WorkbenchExecution,
                promotion_target: StrategyPromotionTarget::RegistryKnowledgePromotion,
            },
            Self::SchemaTransform => StrategyKindDoctrine {
                kind: Self::SchemaTransform,
                selection_key_kind: StrategySelectionKeyKind::Schema,
                input_kind: StrategyAllowedInputKind::SchemaProfile,
                output_kind: StrategyOutputKind::FrozenScriptPointer,
                compatibility_kind: StrategyCompatibilityKind::SchemaTiered,
                execution_mode: StrategyExecutionMode::SelectionOnly,
                promotion_target: StrategyPromotionTarget::StrategyRegistryChampion,
            },
            Self::TaskTransform => StrategyKindDoctrine {
                kind: Self::TaskTransform,
                selection_key_kind: StrategySelectionKeyKind::Task,
                input_kind: StrategyAllowedInputKind::ExactTask,
                output_kind: StrategyOutputKind::FrozenScriptPointer,
                compatibility_kind: StrategyCompatibilityKind::TaskExactOnly,
                execution_mode: StrategyExecutionMode::SelectionOnly,
                promotion_target: StrategyPromotionTarget::StrategyRegistryChampion,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrategyKindDoctrine {
    pub kind: StrategyKind,
    pub selection_key_kind: StrategySelectionKeyKind,
    pub input_kind: StrategyAllowedInputKind,
    pub output_kind: StrategyOutputKind,
    pub compatibility_kind: StrategyCompatibilityKind,
    pub execution_mode: StrategyExecutionMode,
    pub promotion_target: StrategyPromotionTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategySelectionKeyKind {
    #[serde(rename = "profile-and-skill")]
    Profile,
    #[serde(rename = "linkage-map-and-skill")]
    LinkageMap,
    #[serde(rename = "schema-and-skill")]
    Schema,
    #[serde(rename = "task-and-skill")]
    Task,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum StrategySelectionKey {
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

impl StrategySelectionKey {
    pub const fn kind(&self) -> StrategySelectionKeyKind {
        match self {
            Self::IdentityEvidence { .. } => StrategySelectionKeyKind::Profile,
            Self::RecordLinkage { .. } => StrategySelectionKeyKind::LinkageMap,
            Self::SchemaTransform { .. } => StrategySelectionKeyKind::Schema,
            Self::TaskTransform { .. } => StrategySelectionKeyKind::Task,
        }
    }

    pub fn skill_hash(&self) -> &str {
        match self {
            Self::IdentityEvidence { skill_hash, .. }
            | Self::RecordLinkage { skill_hash, .. }
            | Self::SchemaTransform { skill_hash, .. }
            | Self::TaskTransform { skill_hash, .. } => skill_hash.as_str(),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::IdentityEvidence { profile_id, .. } => format!("profile={profile_id}"),
            Self::RecordLinkage { linkage_map_id, .. } => {
                format!("linkage_map={linkage_map_id}")
            }
            Self::SchemaTransform {
                schema_fingerprint, ..
            } => {
                format!("schema={schema_fingerprint}")
            }
            Self::TaskTransform { task, .. } => format!("task={task}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyAllowedInputKind {
    ProfiledObservations,
    TwoTapeRecords,
    SchemaProfile,
    ExactTask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum StrategyAllowedInput {
    ProfiledObservations { profile_id: String },
    TwoTapeRecords { linkage_map_id: String },
    SchemaProfile { schema_source: String },
    ExactTask { task: String },
}

impl StrategyAllowedInput {
    pub const fn kind(&self) -> StrategyAllowedInputKind {
        match self {
            Self::ProfiledObservations { .. } => StrategyAllowedInputKind::ProfiledObservations,
            Self::TwoTapeRecords { .. } => StrategyAllowedInputKind::TwoTapeRecords,
            Self::SchemaProfile { .. } => StrategyAllowedInputKind::SchemaProfile,
            Self::ExactTask { .. } => StrategyAllowedInputKind::ExactTask,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyOutputKind {
    EvidenceBundle,
    LinkageBundle,
    FrozenScriptPointer,
    RegistryKnowledgeProposal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyCapabilityRequirement {
    DeterministicLocalExecution,
    NoLiveNetwork,
    PinnedDependencies,
    AuditFixturesRequired,
    ExactLookupBoundary,
    ReviewGateForRegistryMutation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyExecutionMode {
    WorkbenchExecution,
    SelectionOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyExecutionPolicy {
    pub mode: StrategyExecutionMode,
    pub deterministic_replay: bool,
    pub exact_lookup_phase: bool,
    pub permits_live_network: bool,
    pub requires_pinned_dependencies: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyAuditFixtureKind {
    DeterministicStdoutSuite,
    HoldoutPairs,
    HardNegatives,
    ReviewQueue,
    LinkageGold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyCompatibilityKind {
    ProfileScoped,
    FieldMapScoped,
    SchemaTiered,
    TaskExactOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyCompatibility {
    #[serde(rename = "type")]
    pub kind: StrategyCompatibilityKind,
    pub relation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyPromotionTarget {
    StrategyRegistryChampion,
    RegistryKnowledgePromotion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyPromotionSemantics {
    pub target: StrategyPromotionTarget,
    pub requires_version_bump: bool,
    pub requires_audit: bool,
    pub allows_operator_attestation: bool,
    pub requires_review_gate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyDefinition {
    pub version: String,
    pub kind: StrategyKind,
    pub selection_key: StrategySelectionKey,
    pub allowed_inputs: Vec<StrategyAllowedInput>,
    pub declared_outputs: Vec<StrategyOutputKind>,
    pub capability_requirements: Vec<StrategyCapabilityRequirement>,
    pub execution_policy: StrategyExecutionPolicy,
    pub audit_fixtures: Vec<StrategyAuditFixtureKind>,
    pub compatibility: StrategyCompatibility,
    pub promotion: StrategyPromotionSemantics,
}

impl StrategyDefinition {
    pub fn validate(&self) -> StrategyDoctrineResult<()> {
        if self.version != strategy_schema_version() {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "strategy version does not match the canonical doctrine schema version",
                "rewrite the definition using the current strategy schema version",
            ));
        }

        let doctrine = self.kind.doctrine();

        if self.selection_key.kind() != doctrine.selection_key_kind {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "selection key kind does not match strategy kind",
                "use the selection key family that is canonical for this strategy kind",
            ));
        }

        if self.allowed_inputs.is_empty() {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "strategy definitions must declare at least one allowed input contract",
                "add the canonical allowed input for this strategy kind",
            ));
        }

        if self
            .allowed_inputs
            .iter()
            .any(|input| input.kind() != doctrine.input_kind)
        {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "allowed input contract does not match strategy kind",
                "split mixed input contracts into separate typed strategy kinds",
            ));
        }

        let outputs = self
            .declared_outputs
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if !outputs.contains(&doctrine.output_kind) {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "declared outputs are missing the canonical primary output for this strategy kind",
                "add the required primary output or retag the strategy kind",
            ));
        }

        let allowed_outputs = allowed_outputs_for(self.kind);
        if outputs
            .iter()
            .any(|output| !allowed_outputs.contains(output))
        {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "declared outputs mix incompatible strategy vocabularies",
                "separate procedural strategies from identity/linkage knowledge production",
            ));
        }

        let requirements = self
            .capability_requirements
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let required = required_capabilities_for(self.kind);
        if required
            .iter()
            .any(|capability| !requirements.contains(capability))
        {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "capability requirements are missing a canonical doctrine requirement",
                "add deterministic-local, audit, and exact-lookup-boundary requirements",
            ));
        }

        if self.execution_policy.mode != doctrine.execution_mode
            || !self.execution_policy.deterministic_replay
            || self.execution_policy.exact_lookup_phase
            || self.execution_policy.permits_live_network
        {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "execution policy violates canonical strategy doctrine",
                "keep execution local, deterministic, and outside the exact lookup phase",
            ));
        }

        if self.execution_policy.requires_pinned_dependencies
            != matches!(
                doctrine.execution_mode,
                StrategyExecutionMode::SelectionOnly
            )
        {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "pinned dependency policy does not match the selected execution mode",
                "selection-only transform strategies require pinned dependencies; workbench strategies do not declare that requirement here",
            ));
        }

        if self.compatibility.kind != doctrine.compatibility_kind {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "compatibility relation does not match strategy kind",
                "use the canonical compatibility rule for the selected strategy kind",
            ));
        }

        let fixtures = self.audit_fixtures.iter().copied().collect::<BTreeSet<_>>();
        let required_fixtures = required_audit_fixtures_for(self.kind);
        if required_fixtures
            .iter()
            .any(|fixture| !fixtures.contains(fixture))
        {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "audit fixture set is missing a required canonical fixture family",
                "add the required fixture families before promoting the strategy",
            ));
        }

        if self.promotion.target != doctrine.promotion_target
            || !self.promotion.requires_version_bump
            || !self.promotion.requires_audit
        {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "promotion semantics do not match canonical strategy doctrine",
                "use the canonical promotion target with version bump and audit requirements",
            ));
        }

        let champion_target = matches!(
            doctrine.promotion_target,
            StrategyPromotionTarget::StrategyRegistryChampion
        );
        if self.promotion.allows_operator_attestation != champion_target
            || self.promotion.requires_review_gate == champion_target
        {
            return Err(StrategyDoctrineError::new(
                StrategyDoctrineErrorCode::IncompatibleFields,
                "promotion attestation or review-gate policy conflicts with the strategy kind",
                "champion selection strategies may be operator-attested; registry-knowledge promotion requires review-gated mutation",
            ));
        }

        Ok(())
    }

    pub fn selection_summary(&self) -> String {
        format!(
            "select strategy kind={} key={} skill={} mode={} exact_lookup=never",
            serde_json::to_value(self.kind)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string()),
            self.selection_key.describe(),
            self.selection_key.skill_hash(),
            serde_json::to_value(self.execution_policy.mode)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string()),
        )
    }

    pub fn explain_summary(&self) -> String {
        let outputs = self
            .declared_outputs
            .iter()
            .map(enum_label)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "strategy kind={} outputs=[{}] compatibility={} promotion={}",
            enum_label(self.kind),
            outputs,
            enum_label(self.compatibility.kind),
            enum_label(self.promotion.target),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyStrategyFootprint {
    pub schema_key: bool,
    pub task_key: bool,
    pub profile_scope: bool,
    pub linkage_scope: bool,
    pub frozen_script_pointer: bool,
    pub evidence_bundle: bool,
    pub linkage_bundle: bool,
    pub registry_knowledge_proposal: bool,
}

pub fn classify_legacy_footprint(
    footprint: &LegacyStrategyFootprint,
) -> StrategyDoctrineResult<StrategyKind> {
    let mut matches = Vec::new();

    if footprint.profile_scope && footprint.evidence_bundle && footprint.registry_knowledge_proposal
    {
        matches.push(StrategyKind::IdentityEvidence);
    }
    if footprint.linkage_scope && footprint.linkage_bundle && footprint.registry_knowledge_proposal
    {
        matches.push(StrategyKind::RecordLinkage);
    }
    if footprint.schema_key && footprint.frozen_script_pointer {
        matches.push(StrategyKind::SchemaTransform);
    }
    if footprint.task_key && footprint.frozen_script_pointer {
        matches.push(StrategyKind::TaskTransform);
    }

    match matches.as_slice() {
        [kind] => Ok(*kind),
        [] => Err(StrategyDoctrineError::new(
            StrategyDoctrineErrorCode::UnknownKind,
            "legacy strategy footprint does not map to any canonical strategy kind",
            "declare a typed selection key, output family, and execution mode before migrating this strategy",
        )),
        _ => Err(StrategyDoctrineError::new(
            StrategyDoctrineErrorCode::AmbiguousMigration,
            "legacy strategy footprint matches multiple canonical strategy kinds",
            "split mixed schema/task/entity/linkage semantics into separate typed strategy definitions",
        )),
    }
}

fn allowed_outputs_for(kind: StrategyKind) -> BTreeSet<StrategyOutputKind> {
    match kind {
        StrategyKind::IdentityEvidence => [
            StrategyOutputKind::EvidenceBundle,
            StrategyOutputKind::RegistryKnowledgeProposal,
        ]
        .into_iter()
        .collect(),
        StrategyKind::RecordLinkage => [
            StrategyOutputKind::LinkageBundle,
            StrategyOutputKind::RegistryKnowledgeProposal,
        ]
        .into_iter()
        .collect(),
        StrategyKind::SchemaTransform | StrategyKind::TaskTransform => {
            [StrategyOutputKind::FrozenScriptPointer]
                .into_iter()
                .collect()
        }
    }
}

fn required_capabilities_for(kind: StrategyKind) -> BTreeSet<StrategyCapabilityRequirement> {
    let mut required = BTreeSet::from([
        StrategyCapabilityRequirement::DeterministicLocalExecution,
        StrategyCapabilityRequirement::NoLiveNetwork,
        StrategyCapabilityRequirement::AuditFixturesRequired,
        StrategyCapabilityRequirement::ExactLookupBoundary,
    ]);
    if matches!(
        kind,
        StrategyKind::SchemaTransform | StrategyKind::TaskTransform
    ) {
        required.insert(StrategyCapabilityRequirement::PinnedDependencies);
    } else {
        required.insert(StrategyCapabilityRequirement::ReviewGateForRegistryMutation);
    }
    required
}

fn required_audit_fixtures_for(kind: StrategyKind) -> BTreeSet<StrategyAuditFixtureKind> {
    match kind {
        StrategyKind::IdentityEvidence => [
            StrategyAuditFixtureKind::HoldoutPairs,
            StrategyAuditFixtureKind::HardNegatives,
            StrategyAuditFixtureKind::ReviewQueue,
        ]
        .into_iter()
        .collect(),
        StrategyKind::RecordLinkage => [
            StrategyAuditFixtureKind::HoldoutPairs,
            StrategyAuditFixtureKind::LinkageGold,
        ]
        .into_iter()
        .collect(),
        StrategyKind::SchemaTransform | StrategyKind::TaskTransform => {
            [StrategyAuditFixtureKind::DeterministicStdoutSuite]
                .into_iter()
                .collect()
        }
    }
}

fn enum_label<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}
