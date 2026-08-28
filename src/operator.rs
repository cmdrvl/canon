use clap::Command;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MutationClass {
    ReadOnly,
    OwnedOutput,
    CacheOnly,
    RegistryMutation,
    PublicationTransaction,
    ExternalMaterialization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NetworkClass {
    Offline,
    DeniedByDefault,
    ExplicitExternalProvider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConcurrencyClass {
    StatelessRead,
    AtomicOwnedOutput,
    CacheRaceSafe,
    ExclusiveRegistryMutation,
    OptimisticPublicationCas,
    IsolatedRunner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlatformClass {
    PortablePathUtf8,
    SameFilesystemAtomicReplace,
    UnixPermissionBits,
    RejectLinks,
    AdvisoryFileLock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSafetyDeclaration {
    pub command: &'static str,
    pub operator_contract_name: Option<&'static str>,
    pub usage: &'static str,
    pub read_only: bool,
    pub mutation: MutationClass,
    pub network: NetworkClass,
    pub concurrency: ConcurrencyClass,
    pub platforms: &'static [PlatformClass],
    pub owned_temp_fixtures_only: bool,
    pub notes: &'static str,
}

pub const SAFETY_MATRIX_SCHEMA_VERSION: &str = "canon.operator.safety_matrix.v1";

pub const CORE_PLATFORM_CLASSES: &[PlatformClass] = &[
    PlatformClass::PortablePathUtf8,
    PlatformClass::SameFilesystemAtomicReplace,
    PlatformClass::UnixPermissionBits,
    PlatformClass::RejectLinks,
    PlatformClass::AdvisoryFileLock,
];

pub const COMMAND_SAFETY_DECLARATIONS: &[CommandSafetyDeclaration] = &[
    CommandSafetyDeclaration {
        command: "doctor",
        operator_contract_name: Some("doctor"),
        usage: "canon doctor [health --json|capabilities --json|robot-docs|--robot-triage]",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "operator contract inspection only; no fixers or writes",
    },
    CommandSafetyDeclaration {
        command: "lookup",
        operator_contract_name: None,
        usage: "canon <INPUT> --registry <DIR> --column <COL> --no-witness",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "exact lookup reads input and registry; safety harness disables witness and registry index cache",
    },
    CommandSafetyDeclaration {
        command: "package inspect",
        operator_contract_name: Some("package inspect"),
        usage: "canon package inspect <ARCHIVE>",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8, PlatformClass::RejectLinks],
        owned_temp_fixtures_only: true,
        notes: "local archive read only",
    },
    CommandSafetyDeclaration {
        command: "package verify",
        operator_contract_name: Some("package verify"),
        usage: "canon package verify <ARCHIVE>",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8, PlatformClass::RejectLinks],
        owned_temp_fixtures_only: true,
        notes: "local archive digest and contract verification only",
    },
    CommandSafetyDeclaration {
        command: "package pack",
        operator_contract_name: Some("package pack"),
        usage: "canon package pack --root <DIR> --package <package.json> --out <ARCHIVE>",
        read_only: false,
        mutation: MutationClass::OwnedOutput,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
            PlatformClass::UnixPermissionBits,
            PlatformClass::RejectLinks,
        ],
        owned_temp_fixtures_only: true,
        notes: "writes a declared archive path only",
    },
    CommandSafetyDeclaration {
        command: "package unpack",
        operator_contract_name: Some("package unpack"),
        usage: "canon package unpack <ARCHIVE> --target <EMPTY_DIR>",
        read_only: false,
        mutation: MutationClass::OwnedOutput,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[PlatformClass::PortablePathUtf8, PlatformClass::RejectLinks],
        owned_temp_fixtures_only: true,
        notes: "writes only inside an explicit existing empty target directory",
    },
    CommandSafetyDeclaration {
        command: "package push",
        operator_contract_name: Some("package push"),
        usage: "canon package push --archive <ARCHIVE> --registry <OCI_BASE_URL> --repository <REPOSITORY>",
        read_only: false,
        mutation: MutationClass::PublicationTransaction,
        network: NetworkClass::ExplicitExternalProvider,
        concurrency: ConcurrencyClass::OptimisticPublicationCas,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "publishes verified package bytes to an explicit external OCI registry",
    },
    CommandSafetyDeclaration {
        command: "package pull",
        operator_contract_name: Some("package pull"),
        usage: "canon package pull --registry <OCI_BASE_URL> --repository <REPOSITORY> --cache <DIR>",
        read_only: false,
        mutation: MutationClass::OwnedOutput,
        network: NetworkClass::ExplicitExternalProvider,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "pulls verified package bytes from an explicit external OCI registry into an owned cache",
    },
    CommandSafetyDeclaration {
        command: "registry lint",
        operator_contract_name: Some("registry lint"),
        usage: "canon registry lint <DIR> --emit json",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "registry health inspection",
    },
    CommandSafetyDeclaration {
        command: "registry providers",
        operator_contract_name: Some("registry providers"),
        usage: "canon registry providers --emit json",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "provider catalog is generated locally and must not contact providers",
    },
    CommandSafetyDeclaration {
        command: "registry read",
        operator_contract_name: None,
        usage: "canon registry next-id|diff|audit|lint|providers|provider-schema ...",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "registry inspection and reporting read local files and emit stdout only",
    },
    CommandSafetyDeclaration {
        command: "registry index cache",
        operator_contract_name: None,
        usage: "managed registry index cache under ~/.cmdrvl/cache/registry-indexes",
        read_only: false,
        mutation: MutationClass::CacheOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::CacheRaceSafe,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "cache writes are optional, derived, disableable with CANON_REGISTRY_INDEX_MODE=no-cache, and never source of truth",
    },
    CommandSafetyDeclaration {
        command: "registry provider-schema",
        operator_contract_name: Some("registry provider-schema"),
        usage: "canon registry provider-schema <PROVIDER> --emit json",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "provider schema is generated locally and must not contact providers",
    },
    CommandSafetyDeclaration {
        command: "registry export",
        operator_contract_name: Some("registry export"),
        usage: "canon registry export --format dbt-seed --registry <DIR> --out <PATH>",
        read_only: false,
        mutation: MutationClass::OwnedOutput,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "writes declared export artifacts without mutating registry inputs",
    },
    CommandSafetyDeclaration {
        command: "registry build mock",
        operator_contract_name: Some("registry build"),
        usage: "canon registry build --source mock --seed <CSV> --output <DIR>",
        read_only: false,
        mutation: MutationClass::ExternalMaterialization,
        network: NetworkClass::DeniedByDefault,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "mock provider materializes from local seed data and is the only offline build case",
    },
    CommandSafetyDeclaration {
        command: "registry build openfigi",
        operator_contract_name: Some("registry build"),
        usage: "canon registry build --source openfigi --seed <CSV> --output <DIR>",
        read_only: false,
        mutation: MutationClass::ExternalMaterialization,
        network: NetworkClass::ExplicitExternalProvider,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "external provider access is opt-in and must be refused by offline harnesses before spawn",
    },
    CommandSafetyDeclaration {
        command: "registry build external materialization",
        operator_contract_name: Some("registry build"),
        usage: "canon registry build --source <SOURCE> --seed <CSV> --output <DIR>",
        read_only: false,
        mutation: MutationClass::ExternalMaterialization,
        network: NetworkClass::ExplicitExternalProvider,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "worst-case registry materialization may contact an explicit external provider and writes declared output artifacts",
    },
    CommandSafetyDeclaration {
        command: "registry mutation",
        operator_contract_name: Some("registry add-entry"),
        usage: "canon registry add-entry|mint|default-id-scheme --registry <DIR> ...",
        read_only: false,
        mutation: MutationClass::RegistryMutation,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::ExclusiveRegistryMutation,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "registry mutations require explicit registry roots and version movement",
    },
    CommandSafetyDeclaration {
        command: "strategy read",
        operator_contract_name: Some("strategy explain"),
        usage: "canon strategy list|explain|resolve ...",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "strategy resolution and explanation do not execute scripts",
    },
    CommandSafetyDeclaration {
        command: "strategy audit",
        operator_contract_name: Some("strategy audit"),
        usage: "canon strategy audit --schema <PROFILE.json> --script <SCRIPT> --suite <DIR>",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::IsolatedRunner,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::UnixPermissionBits,
        ],
        owned_temp_fixtures_only: true,
        notes: "runs inside an isolated runner with denied network and scratch-only writes",
    },
    CommandSafetyDeclaration {
        command: "strategy mutation",
        operator_contract_name: Some("strategy promote"),
        usage: "canon strategy register|update|deprecate|promote --registry <DIR> ...",
        read_only: false,
        mutation: MutationClass::RegistryMutation,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::ExclusiveRegistryMutation,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "mutates strategy registry entries and may append witness unless disabled",
    },
    CommandSafetyDeclaration {
        command: "entity read",
        operator_contract_name: Some("entity explain"),
        usage: "canon entity explain|review export ...",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "artifact explanation and review export read immutable result artifacts",
    },
    CommandSafetyDeclaration {
        command: "entity workbench",
        operator_contract_name: Some("entity run"),
        usage: "canon entity run|block|evidence|solve|audit --no-witness ...",
        read_only: false,
        mutation: MutationClass::OwnedOutput,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::CacheRaceSafe,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "owned cache/artifact writes only; safety harness disables witness for read checks",
    },
    CommandSafetyDeclaration {
        command: "entity scaffold refusal",
        operator_contract_name: None,
        usage: "canon entity block|evidence|solve ... without required artifact-backed execution inputs",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "legacy scaffold commands currently refuse before writing when artifact-backed inputs are absent",
    },
    CommandSafetyDeclaration {
        command: "entity owned output",
        operator_contract_name: None,
        usage: "canon entity apply|profile init ... --out <PATH>",
        read_only: false,
        mutation: MutationClass::OwnedOutput,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "writes declared entity output artifacts without mutating registries",
    },
    CommandSafetyDeclaration {
        command: "entity link write-back",
        operator_contract_name: Some("entity link"),
        usage: "canon entity link <REFERENCE> <TARGET> ... [--write-back]",
        read_only: false,
        mutation: MutationClass::RegistryMutation,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::ExclusiveRegistryMutation,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "worst-case entity link may write back promoted registry/sidecar updates",
    },
    CommandSafetyDeclaration {
        command: "entity promotion",
        operator_contract_name: Some("entity promote"),
        usage: "canon entity promote|review import --registry <DIR> ...",
        read_only: false,
        mutation: MutationClass::RegistryMutation,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::ExclusiveRegistryMutation,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "promotion writes registry and sidecars only after audit/profile gates",
    },
    CommandSafetyDeclaration {
        command: "project read",
        operator_contract_name: None,
        usage: "project manifest/lock/plan/explain read",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "project metadata readers must not create caches or migration files",
    },
    CommandSafetyDeclaration {
        command: "project init",
        operator_contract_name: Some("project init"),
        usage: "canon project init <DIR> --project-id <ID> --mapping-profile <PROFILE>",
        read_only: false,
        mutation: MutationClass::OwnedOutput,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "creates declared project files inside an explicit project directory",
    },
    CommandSafetyDeclaration {
        command: "geo link-sources",
        operator_contract_name: Some("geo link-sources"),
        usage: "canon geo link-sources --request <REQUEST.json> --rows-out <ROWS.csv>",
        read_only: false,
        mutation: MutationClass::OwnedOutput,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "reads declared local CSV sources and atomically replaces only the explicit merged-row output",
    },
    CommandSafetyDeclaration {
        command: "geo read",
        operator_contract_name: None,
        usage: "geo composition/evidence/population request read",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "geo requests are answered on stdout; no registry, cache, or work-dir writes",
    },
    CommandSafetyDeclaration {
        command: "unresolved inbox read",
        operator_contract_name: None,
        usage: "unresolved inbox artifact merge/export/read",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "inbox artifacts are read or transformed to stdout, not shadow registries",
    },
    CommandSafetyDeclaration {
        command: "inbox read cli",
        operator_contract_name: Some("inbox list"),
        usage: "canon inbox list|show|explain|stats --inbox <INBOX.json>",
        read_only: true,
        mutation: MutationClass::ReadOnly,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::StatelessRead,
        platforms: &[PlatformClass::PortablePathUtf8],
        owned_temp_fixtures_only: true,
        notes: "reads finalized unresolved inbox artifacts and emits ranked JSON/summary only",
    },
    CommandSafetyDeclaration {
        command: "inbox review export",
        operator_contract_name: Some("inbox export-review"),
        usage: "canon inbox export-review --inbox <INBOX.json> [--out <REVIEW.json>]",
        read_only: false,
        mutation: MutationClass::OwnedOutput,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "writes only an explicit review output path when --out is supplied; stdout otherwise",
    },
    CommandSafetyDeclaration {
        command: "inbox review apply",
        operator_contract_name: Some("inbox apply-review"),
        usage: "canon inbox apply-review --inbox <INBOX.json> --review <REVIEW.json> --expected-inbox-hash <HASH> --out <GROUPS.json>",
        read_only: false,
        mutation: MutationClass::OwnedOutput,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "requires explicit inbox hash and writes grouped unresolved artifact to a create-new output path",
    },
    CommandSafetyDeclaration {
        command: "inbox entity plan",
        operator_contract_name: Some("inbox plan-entity"),
        usage: "canon inbox plan-entity --inbox <INBOX.json> --expected-inbox-hash <HASH> --out <REQUEST.json>",
        read_only: false,
        mutation: MutationClass::OwnedOutput,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::AtomicOwnedOutput,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
        ],
        owned_temp_fixtures_only: true,
        notes: "plans a bounded entity workbench request without selecting canonical identity",
    },
    CommandSafetyDeclaration {
        command: "publication backend",
        operator_contract_name: None,
        usage: "filesystem publication backend publish/read/current_head",
        read_only: false,
        mutation: MutationClass::PublicationTransaction,
        network: NetworkClass::Offline,
        concurrency: ConcurrencyClass::OptimisticPublicationCas,
        platforms: &[
            PlatformClass::PortablePathUtf8,
            PlatformClass::SameFilesystemAtomicReplace,
            PlatformClass::AdvisoryFileLock,
        ],
        owned_temp_fixtures_only: true,
        notes: "immutable object create-if-absent followed by channel compare-and-swap",
    },
];

pub fn declaration_for(command: &str) -> Option<&'static CommandSafetyDeclaration> {
    COMMAND_SAFETY_DECLARATIONS
        .iter()
        .find(|declaration| declaration.command == command)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicLeafLongFlags {
    pub command: String,
    pub long_flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorBehaviorMetadata {
    pub command: String,
    pub operator_contract_name: Option<String>,
    pub usage: String,
    pub read_only: bool,
    pub mutation: String,
    pub network: String,
    pub concurrency: String,
    pub platforms: Vec<String>,
    pub owned_temp_fixtures_only: bool,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorCommandFlagDrift {
    pub command: String,
    pub missing_flags: Vec<String>,
    pub extra_flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorMissingSemanticField {
    pub command: String,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorBehaviorMetadataDrift {
    pub command: String,
    pub field: String,
    pub expected: String,
    pub actual: Option<String>,
    pub source_declarations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorManifestValidationReport {
    pub ok: bool,
    pub manifest_digest: Option<String>,
    pub manifest_errors: Vec<String>,
    pub missing_leaf_commands: Vec<String>,
    pub extra_operator_commands: Vec<String>,
    pub flag_drifts: Vec<OperatorCommandFlagDrift>,
    pub missing_required_fields: Vec<OperatorMissingSemanticField>,
    pub behavior_drifts: Vec<OperatorBehaviorMetadataDrift>,
}

pub const REQUIRED_OPERATOR_SUBCOMMAND_FIELDS: &[&str] = &[
    "name",
    "usage",
    "description",
    "output_mode",
    "output_schema",
    "exit_codes",
    "options",
    "status",
    "read_only",
    "side_effects",
    "safety",
    "recovery",
];

pub const REQUIRED_OPERATOR_AGGREGATE_FIELDS: &[&str] = &["name", "usage", "aggregate", "leaves"];

pub fn public_leaf_commands_from(command: &Command) -> Vec<String> {
    public_leaf_long_flags_from(command)
        .into_iter()
        .map(|surface| surface.command)
        .collect()
}

pub fn public_leaf_long_flags_from(command: &Command) -> Vec<PublicLeafLongFlags> {
    let command = built_command(command);
    let mut surfaces = compiled_command_surfaces(&command)
        .into_iter()
        .filter(|surface| surface.leaf)
        .map(|surface| PublicLeafLongFlags {
            command: surface.command,
            long_flags: surface.long_flags.into_iter().collect(),
        })
        .collect::<Vec<_>>();
    surfaces.sort_by(|left, right| left.command.cmp(&right.command));
    surfaces
}

pub fn command_behavior_metadata() -> Vec<OperatorBehaviorMetadata> {
    let mut metadata = COMMAND_SAFETY_DECLARATIONS
        .iter()
        .map(|declaration| OperatorBehaviorMetadata {
            command: declaration.command.to_string(),
            operator_contract_name: declaration.operator_contract_name.map(str::to_string),
            usage: declaration.usage.to_string(),
            read_only: declaration.read_only,
            mutation: mutation_class_contract_name(declaration.mutation).to_string(),
            network: network_class_contract_name(declaration.network).to_string(),
            concurrency: concurrency_class_contract_name(declaration.concurrency).to_string(),
            platforms: declaration
                .platforms
                .iter()
                .map(|platform| platform_class_contract_name(*platform).to_string())
                .collect(),
            owned_temp_fixtures_only: declaration.owned_temp_fixtures_only,
            notes: declaration.notes.to_string(),
        })
        .collect::<Vec<_>>();
    metadata.sort_by(|left, right| {
        left.operator_contract_name
            .cmp(&right.operator_contract_name)
            .then_with(|| left.command.cmp(&right.command))
    });
    metadata
}

pub fn stable_manifest_digest(manifest: &Value) -> String {
    let mut canonical = String::new();
    push_canonical_json(manifest, &mut canonical);
    format!("blake3:{}", blake3::hash(canonical.as_bytes()).to_hex())
}

// Path-included integration tests compile this public lib helper as a private module.
#[allow(dead_code)]
pub fn validate_operator_manifest_json(
    command: &Command,
    manifest_json: &str,
) -> OperatorManifestValidationReport {
    match serde_json::from_str::<Value>(manifest_json) {
        Ok(manifest) => validate_operator_manifest(command, &manifest),
        Err(error) => OperatorManifestValidationReport {
            ok: false,
            manifest_digest: None,
            manifest_errors: vec![format!("operator manifest JSON parse failed: {error}")],
            missing_leaf_commands: public_leaf_commands_from(command),
            extra_operator_commands: Vec::new(),
            flag_drifts: Vec::new(),
            missing_required_fields: Vec::new(),
            behavior_drifts: Vec::new(),
        },
    }
}

pub fn validate_operator_manifest(
    command: &Command,
    manifest: &Value,
) -> OperatorManifestValidationReport {
    let command = built_command(command);
    let manifest_digest = Some(stable_manifest_digest(manifest));
    let actual_all_surfaces = compiled_command_surfaces(&command)
        .into_iter()
        .map(|surface| {
            (
                surface.command,
                CompiledCommandSurface {
                    command: String::new(),
                    long_flags: surface.long_flags,
                    positionals: surface.positionals,
                    leaf: surface.leaf,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let actual_surfaces = actual_all_surfaces
        .iter()
        .filter(|(_, surface)| surface.leaf)
        .map(|(command, surface)| (command.clone(), surface.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut manifest_errors = Vec::new();
    validate_root_contract(&command, manifest, &mut manifest_errors);
    let actual_commands = actual_surfaces.keys().cloned().collect::<BTreeSet<_>>();
    let operator_rows = operator_manifest_rows(manifest, &mut manifest_errors);
    let mut flag_drifts = Vec::new();
    validate_aggregate_rows(
        &operator_rows,
        &actual_commands,
        &actual_all_surfaces,
        &mut manifest_errors,
        &mut flag_drifts,
    );
    let operator_commands = operator_rows
        .iter()
        .filter(|(_, row)| !row.aggregate)
        .map(|(command, _)| command.clone())
        .collect::<BTreeSet<_>>();

    let missing_leaf_commands = actual_commands
        .difference(&operator_commands)
        .cloned()
        .collect::<Vec<_>>();
    let extra_operator_commands = operator_commands
        .difference(&actual_commands)
        .cloned()
        .collect::<Vec<_>>();

    let actual_root_flags = local_long_flags(&command);
    let operator_root_flags = operator_row_long_flags(manifest.get("options"));
    let missing_root_flags = actual_root_flags
        .difference(&operator_root_flags)
        .cloned()
        .collect::<Vec<_>>();
    let extra_root_flags = operator_root_flags
        .difference(&actual_root_flags)
        .cloned()
        .collect::<Vec<_>>();
    if !missing_root_flags.is_empty() || !extra_root_flags.is_empty() {
        flag_drifts.push(OperatorCommandFlagDrift {
            command: "canon".to_string(),
            missing_flags: missing_root_flags,
            extra_flags: extra_root_flags,
        });
    }
    for command_name in actual_commands.intersection(&operator_commands) {
        let actual_surface = actual_surfaces
            .get(command_name)
            .expect("intersection command has actual surface");
        let operator_flags = &operator_rows
            .get(command_name)
            .expect("intersection command has operator row")
            .long_flags;
        let missing_flags = actual_surface
            .long_flags
            .difference(operator_flags)
            .cloned()
            .collect::<Vec<_>>();
        let extra_flags = operator_flags
            .difference(&actual_surface.long_flags)
            .cloned()
            .collect::<Vec<_>>();
        if !missing_flags.is_empty() || !extra_flags.is_empty() {
            flag_drifts.push(OperatorCommandFlagDrift {
                command: command_name.clone(),
                missing_flags,
                extra_flags,
            });
        }
    }

    validate_row_syntax(&operator_rows, &actual_all_surfaces, &mut manifest_errors);

    let mut missing_required_fields = Vec::new();
    for (command_name, row) in &operator_rows {
        let mut required_fields = REQUIRED_OPERATOR_SUBCOMMAND_FIELDS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if row.aggregate {
            required_fields.extend(REQUIRED_OPERATOR_AGGREGATE_FIELDS.iter().copied());
        }
        for field in required_fields {
            if !row.fields.contains(field) {
                missing_required_fields.push(OperatorMissingSemanticField {
                    command: command_name.clone(),
                    field: field.to_string(),
                });
            }
        }
    }

    let behavior_drifts = behavior_metadata_drifts(&operator_rows);
    let ok = manifest_errors.is_empty()
        && missing_leaf_commands.is_empty()
        && extra_operator_commands.is_empty()
        && flag_drifts.is_empty()
        && missing_required_fields.is_empty()
        && behavior_drifts.is_empty();

    OperatorManifestValidationReport {
        ok,
        manifest_digest,
        manifest_errors,
        missing_leaf_commands,
        extra_operator_commands,
        flag_drifts,
        missing_required_fields,
        behavior_drifts,
    }
}

fn built_command(command: &Command) -> Command {
    let mut command = command.clone();
    command.build();
    command
}

fn validate_root_contract(command: &Command, manifest: &Value, manifest_errors: &mut Vec<String>) {
    validate_root_string(
        manifest,
        "schema_version",
        "operator.v0",
        "operator manifest schema_version",
        manifest_errors,
    );
    validate_root_string(
        manifest,
        "name",
        "canon",
        "operator manifest name",
        manifest_errors,
    );
    validate_root_string(
        manifest,
        "version",
        env!("CARGO_PKG_VERSION"),
        "operator manifest version",
        manifest_errors,
    );
    let invocation_output_schema = manifest
        .get("invocation")
        .and_then(Value::as_object)
        .and_then(|invocation| invocation.get("output_schema"))
        .and_then(Value::as_str);
    if invocation_output_schema != Some("canon.v0") {
        manifest_errors.push(format!(
            "operator manifest invocation.output_schema must be canon.v0, got {}",
            invocation_output_schema.unwrap_or("<missing>")
        ));
    }

    let actual_positionals = local_positionals(command)
        .into_iter()
        .map(|positional| OperatorPositionalArg {
            name: positional.name,
            position: positional.position.saturating_sub(1),
        })
        .collect::<Vec<_>>();
    let operator_positionals =
        operator_root_positionals(manifest.get("arguments"), manifest_errors);
    if actual_positionals != operator_positionals {
        manifest_errors.push(format!(
            "operator manifest root arguments must match compiled args; expected {}, got {}",
            render_positionals(&actual_positionals),
            render_positionals(&operator_positionals)
        ));
    }
}

fn validate_root_string(
    manifest: &Value,
    field: &str,
    expected: &str,
    label: &str,
    manifest_errors: &mut Vec<String>,
) {
    let actual = manifest.get(field).and_then(Value::as_str);
    if actual != Some(expected) {
        manifest_errors.push(format!(
            "{label} must be {expected}, got {}",
            actual.unwrap_or("<missing>")
        ));
    }
}

#[derive(Debug)]
struct OperatorManifestRow {
    aggregate: bool,
    leaves: Vec<String>,
    usage: Option<String>,
    output_mode: Option<String>,
    output_schema: Option<String>,
    alternate_output_schemas: Vec<String>,
    exit_codes: Option<Value>,
    fields: BTreeSet<String>,
    long_flags: BTreeSet<String>,
    positionals: Vec<OperatorPositionalArg>,
    read_only: Option<bool>,
    side_effects: Option<Value>,
    safety: Option<Value>,
    safety_declaration_command: Option<String>,
    recovery_next_command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompiledCommandSurface {
    command: String,
    long_flags: BTreeSet<String>,
    positionals: Vec<CompiledPositionalArg>,
    leaf: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompiledPositionalArg {
    name: String,
    position: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OperatorPositionalArg {
    name: String,
    position: usize,
}

fn compiled_command_surfaces(command: &Command) -> Vec<CompiledCommandSurface> {
    let mut surfaces = Vec::new();
    for subcommand in visible_subcommands(command) {
        collect_leaf_surface("", subcommand, &mut surfaces);
    }
    surfaces.sort_by(|left, right| left.command.cmp(&right.command));
    surfaces
}

fn collect_leaf_surface(
    prefix: &str,
    command: &Command,
    surfaces: &mut Vec<CompiledCommandSurface>,
) {
    let command_name = if prefix.is_empty() {
        command.get_name().to_string()
    } else {
        format!("{prefix} {}", command.get_name())
    };
    let flags = local_long_flags(command);
    let positionals = local_positionals(command);
    let subcommands = visible_subcommands(command);
    let leaf = subcommands.is_empty();
    surfaces.push(CompiledCommandSurface {
        command: command_name.clone(),
        long_flags: flags,
        positionals,
        leaf,
    });
    if subcommands.is_empty() {
        return;
    }
    for subcommand in subcommands {
        collect_leaf_surface(&command_name, subcommand, surfaces);
    }
}

fn visible_subcommands(command: &Command) -> Vec<&Command> {
    command
        .get_subcommands()
        .filter(|subcommand| subcommand.get_name() != "help")
        .collect()
}

fn local_long_flags(command: &Command) -> BTreeSet<String> {
    let mut flags = BTreeSet::new();
    for arg in command.get_arguments() {
        if let Some(long) = arg.get_long()
            && long != "help"
        {
            flags.insert(long.to_string());
        }
        if let Some(aliases) = arg.get_all_aliases() {
            flags.extend(
                aliases
                    .into_iter()
                    .filter(|alias| *alias != "help")
                    .map(str::to_string),
            );
        }
    }
    flags
}

fn local_positionals(command: &Command) -> Vec<CompiledPositionalArg> {
    let mut positionals = Vec::new();
    for arg in command.get_arguments() {
        if let Some(position) = arg.get_index() {
            positionals.push(CompiledPositionalArg {
                name: arg.get_id().as_str().to_string(),
                position,
            });
        }
    }
    positionals.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.name.cmp(&right.name))
    });
    positionals
}

fn operator_manifest_rows(
    manifest: &Value,
    manifest_errors: &mut Vec<String>,
) -> BTreeMap<String, OperatorManifestRow> {
    let mut rows = BTreeMap::new();
    let Some(object) = manifest.as_object() else {
        manifest_errors.push("operator manifest must be a JSON object".to_string());
        return rows;
    };
    let Some(subcommands) = object.get("subcommands") else {
        manifest_errors.push("operator manifest missing subcommands array".to_string());
        return rows;
    };
    let Some(subcommands) = subcommands.as_array() else {
        manifest_errors.push("operator manifest subcommands must be an array".to_string());
        return rows;
    };

    for (index, row) in subcommands.iter().enumerate() {
        let Some(row_object) = row.as_object() else {
            manifest_errors.push(format!("subcommands[{index}] must be a JSON object"));
            continue;
        };
        let Some(name) = row_object.get("name").and_then(Value::as_str) else {
            manifest_errors.push(format!("subcommands[{index}] missing string name"));
            continue;
        };
        if rows.contains_key(name) {
            manifest_errors.push(format!("duplicate operator subcommand row '{name}'"));
            continue;
        }
        let aggregate = row_object
            .get("aggregate")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let leaves = if aggregate {
            aggregate_leaves(index, row_object.get("leaves"), manifest_errors)
        } else {
            Vec::new()
        };
        rows.insert(
            name.to_string(),
            OperatorManifestRow {
                aggregate,
                leaves,
                usage: row_object
                    .get("usage")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                output_mode: row_object
                    .get("output_mode")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                output_schema: row_object
                    .get("output_schema")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                alternate_output_schemas: operator_row_string_array(
                    row_object.get("alternate_output_schemas"),
                    index,
                    "alternate_output_schemas",
                    manifest_errors,
                ),
                exit_codes: row_object.get("exit_codes").cloned(),
                fields: row_object.keys().cloned().collect(),
                long_flags: operator_row_long_flags(row_object.get("options")),
                positionals: operator_row_positionals(
                    index,
                    row_object.get("options"),
                    manifest_errors,
                ),
                read_only: row_object.get("read_only").and_then(Value::as_bool),
                side_effects: row_object.get("side_effects").cloned(),
                safety: row_object.get("safety").cloned(),
                safety_declaration_command: operator_row_safety_declaration_command(
                    row_object.get("safety"),
                ),
                recovery_next_command: operator_row_recovery_next_command(row_object),
            },
        );
    }

    rows
}

fn aggregate_leaves(
    index: usize,
    leaves: Option<&Value>,
    manifest_errors: &mut Vec<String>,
) -> Vec<String> {
    let Some(leaves) = leaves else {
        manifest_errors.push(format!(
            "aggregate subcommands[{index}] missing leaves array"
        ));
        return Vec::new();
    };
    let Some(leaves) = leaves.as_array() else {
        manifest_errors.push(format!(
            "aggregate subcommands[{index}] leaves must be an array"
        ));
        return Vec::new();
    };
    if leaves.is_empty() {
        manifest_errors.push(format!(
            "aggregate subcommands[{index}] leaves must not be empty"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut parsed = Vec::new();
    for (leaf_index, leaf) in leaves.iter().enumerate() {
        let Some(leaf) = leaf.as_str() else {
            manifest_errors.push(format!(
                "aggregate subcommands[{index}].leaves[{leaf_index}] must be a string"
            ));
            continue;
        };
        if !seen.insert(leaf.to_string()) {
            manifest_errors.push(format!(
                "aggregate subcommands[{index}] repeats leaf '{leaf}'"
            ));
            continue;
        }
        parsed.push(leaf.to_string());
    }
    parsed
}

fn validate_aggregate_rows(
    operator_rows: &BTreeMap<String, OperatorManifestRow>,
    actual_commands: &BTreeSet<String>,
    actual_all_surfaces: &BTreeMap<String, CompiledCommandSurface>,
    manifest_errors: &mut Vec<String>,
    flag_drifts: &mut Vec<OperatorCommandFlagDrift>,
) {
    for (command_name, row) in operator_rows.iter().filter(|(_, row)| row.aggregate) {
        let Some(actual_parent) = actual_all_surfaces.get(command_name) else {
            manifest_errors.push(format!(
                "aggregate operator row '{command_name}' must resolve to a compiled command path"
            ));
            continue;
        };
        if actual_parent.leaf {
            manifest_errors.push(format!(
                "aggregate operator row '{command_name}' resolved to a leaf command"
            ));
        }
        let missing_flags = actual_parent
            .long_flags
            .difference(&row.long_flags)
            .cloned()
            .collect::<Vec<_>>();
        let extra_flags = row
            .long_flags
            .difference(&actual_parent.long_flags)
            .cloned()
            .collect::<Vec<_>>();
        if !missing_flags.is_empty() || !extra_flags.is_empty() {
            flag_drifts.push(OperatorCommandFlagDrift {
                command: command_name.clone(),
                missing_flags,
                extra_flags,
            });
        }
        for leaf in &row.leaves {
            if !actual_commands.contains(leaf) {
                manifest_errors.push(format!(
                    "aggregate operator row '{command_name}' declares unknown leaf '{leaf}'"
                ));
            }
        }
        validate_known_aggregate_contract(command_name, row, manifest_errors);
    }
}

fn validate_known_aggregate_contract(
    command_name: &str,
    row: &OperatorManifestRow,
    manifest_errors: &mut Vec<String>,
) {
    if command_name != "doctor" {
        return;
    }
    if row.output_schema.as_deref() != Some("canon.doctor.health.v1") {
        manifest_errors.push(format!(
            "operator row '{command_name}' output_schema must be canon.doctor.health.v1"
        ));
    }
    let expected_alternates = [
        "canon.doctor.capabilities.v1",
        "canon.doctor.triage.v1",
        "text/plain",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<BTreeSet<_>>();
    let actual_alternates = row
        .alternate_output_schemas
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if actual_alternates != expected_alternates {
        manifest_errors.push(format!(
            "operator row '{command_name}' alternate_output_schemas must match doctor parent outputs"
        ));
    }
    let Some(exit_codes) = row.exit_codes.as_ref().and_then(Value::as_object) else {
        return;
    };
    for code in ["0", "1", "2"] {
        if !exit_codes.contains_key(code) {
            manifest_errors.push(format!(
                "operator row '{command_name}' exit_codes missing {code}"
            ));
        }
    }
}

fn validate_row_syntax(
    operator_rows: &BTreeMap<String, OperatorManifestRow>,
    actual_surfaces: &BTreeMap<String, CompiledCommandSurface>,
    manifest_errors: &mut Vec<String>,
) {
    for (command_name, row) in operator_rows {
        match row.usage.as_deref() {
            Some(usage) if usage_has_exact_command_prefix(usage, command_name) => {}
            Some(usage) => manifest_errors.push(format!(
                "operator row '{command_name}' usage must begin with exact command path 'canon {command_name}', got '{usage}'"
            )),
            None => {}
        }
        if actual_surfaces.contains_key(command_name) {
            validate_output_exit_metadata(command_name, row, manifest_errors);
            if let Some(actual_surface) = actual_surfaces.get(command_name) {
                validate_row_positionals(command_name, row, actual_surface, manifest_errors);
            }
        }
    }
}

fn validate_output_exit_metadata(
    command_name: &str,
    row: &OperatorManifestRow,
    manifest_errors: &mut Vec<String>,
) {
    match row.output_mode.as_deref().map(str::trim) {
        Some(mode) if !mode.is_empty() => {}
        Some(_) => manifest_errors.push(format!(
            "operator row '{command_name}' output_mode must be a nonempty string"
        )),
        None if row.fields.contains("output_mode") => manifest_errors.push(format!(
            "operator row '{command_name}' output_mode must be a nonempty string"
        )),
        None => {}
    }
    match row.output_schema.as_deref().map(str::trim) {
        Some(schema) if !schema.is_empty() => {}
        Some(_) => manifest_errors.push(format!(
            "operator row '{command_name}' output_schema must be a nonempty string"
        )),
        None if row.fields.contains("output_schema") => manifest_errors.push(format!(
            "operator row '{command_name}' output_schema must be a nonempty string"
        )),
        None if row.alternate_output_schemas.is_empty() => manifest_errors.push(format!(
            "operator row '{command_name}' must declare output_schema or alternate_output_schemas"
        )),
        None => {}
    }
    let exit_codes = row.exit_codes.as_ref().and_then(Value::as_object);
    if exit_codes.is_none_or(serde_json::Map::is_empty) {
        manifest_errors.push(format!(
            "operator row '{command_name}' exit_codes must be a nonempty object"
        ));
    }
}

fn usage_has_exact_command_prefix(usage: &str, command_name: &str) -> bool {
    let expected = format!("canon {command_name}");
    let Some(remainder) = usage.strip_prefix(&expected) else {
        return false;
    };
    remainder.is_empty()
        || remainder
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_whitespace())
}

fn validate_row_positionals(
    command_name: &str,
    row: &OperatorManifestRow,
    actual_surface: &CompiledCommandSurface,
    manifest_errors: &mut Vec<String>,
) {
    let actual = actual_surface
        .positionals
        .iter()
        .map(|positional| OperatorPositionalArg {
            name: positional.name.clone(),
            position: positional.position.saturating_sub(1),
        })
        .collect::<Vec<_>>();
    if actual == row.positionals {
        return;
    }
    manifest_errors.push(format!(
        "operator row '{command_name}' positional args must match compiled args; expected {}, got {}",
        render_positionals(&actual),
        render_positionals(&row.positionals)
    ));
}

fn render_positionals(positionals: &[OperatorPositionalArg]) -> String {
    if positionals.is_empty() {
        return "[]".to_string();
    }
    let mut rendered = String::from("[");
    for (index, positional) in positionals.iter().enumerate() {
        if index > 0 {
            rendered.push_str(", ");
        }
        rendered.push_str(&format!("{}@{}", positional.name, positional.position));
    }
    rendered.push(']');
    rendered
}

fn operator_row_long_flags(options: Option<&Value>) -> BTreeSet<String> {
    let Some(options) = options.and_then(Value::as_array) else {
        return BTreeSet::new();
    };
    options
        .iter()
        .filter_map(|option| option.get("flag").and_then(Value::as_str))
        .filter_map(normalize_long_flag)
        .collect()
}

fn operator_row_string_array(
    value: Option<&Value>,
    row_index: usize,
    field: &str,
    manifest_errors: &mut Vec<String>,
) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Some(values) = value.as_array() else {
        manifest_errors.push(format!("subcommands[{row_index}].{field} must be an array"));
        return Vec::new();
    };
    values
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let Some(value) = value.as_str() else {
                manifest_errors.push(format!(
                    "subcommands[{row_index}].{field}[{index}] must be a string"
                ));
                return None;
            };
            Some(value.to_string())
        })
        .collect()
}

fn operator_row_positionals(
    row_index: usize,
    options: Option<&Value>,
    manifest_errors: &mut Vec<String>,
) -> Vec<OperatorPositionalArg> {
    let Some(options) = options.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut positionals = Vec::new();
    let mut seen = BTreeSet::new();
    for (option_index, option) in options.iter().enumerate() {
        let Some(option_object) = option.as_object() else {
            manifest_errors.push(format!(
                "subcommands[{row_index}].options[{option_index}] must be a JSON object"
            ));
            continue;
        };
        if option_object.get("flag").is_some() {
            continue;
        }
        let Some(position) = option_object.get("position") else {
            continue;
        };
        let Some(position) = position.as_u64() else {
            manifest_errors.push(format!(
                "subcommands[{row_index}].options[{option_index}].position must be an integer"
            ));
            continue;
        };
        let Some(name) = option_object.get("name").and_then(Value::as_str) else {
            manifest_errors.push(format!(
                "subcommands[{row_index}].options[{option_index}] positional missing string name"
            ));
            continue;
        };
        let position = position as usize;
        if !seen.insert((position, name.to_string())) {
            manifest_errors.push(format!(
                "subcommands[{row_index}].options repeats positional '{name}' at position {position}"
            ));
            continue;
        }
        positionals.push(OperatorPositionalArg {
            name: name.to_string(),
            position,
        });
    }
    positionals.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.name.cmp(&right.name))
    });
    positionals
}

fn operator_root_positionals(
    arguments: Option<&Value>,
    manifest_errors: &mut Vec<String>,
) -> Vec<OperatorPositionalArg> {
    let Some(arguments) = arguments else {
        manifest_errors.push("operator manifest missing arguments array".to_string());
        return Vec::new();
    };
    let Some(arguments) = arguments.as_array() else {
        manifest_errors.push("operator manifest arguments must be an array".to_string());
        return Vec::new();
    };
    let mut positionals = Vec::new();
    let mut seen = BTreeSet::new();
    for (argument_index, argument) in arguments.iter().enumerate() {
        let Some(argument_object) = argument.as_object() else {
            manifest_errors.push(format!("arguments[{argument_index}] must be a JSON object"));
            continue;
        };
        let Some(position) = argument_object.get("position") else {
            continue;
        };
        let Some(position) = position.as_u64() else {
            manifest_errors.push(format!(
                "arguments[{argument_index}].position must be an integer"
            ));
            continue;
        };
        let Some(name) = argument_object.get("name").and_then(Value::as_str) else {
            manifest_errors.push(format!(
                "arguments[{argument_index}] positional missing string name"
            ));
            continue;
        };
        let position = position as usize;
        if !seen.insert((position, name.to_string())) {
            manifest_errors.push(format!(
                "arguments repeats positional '{name}' at position {position}"
            ));
            continue;
        }
        positionals.push(OperatorPositionalArg {
            name: name.to_string(),
            position,
        });
    }
    positionals.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.name.cmp(&right.name))
    });
    positionals
}

fn normalize_long_flag(flag: &str) -> Option<String> {
    flag.strip_prefix("--")
        .filter(|flag| !flag.is_empty())
        .map(str::to_string)
}

fn operator_row_recovery_next_command(
    row_object: &serde_json::Map<String, Value>,
) -> Option<String> {
    row_object
        .get("recovery")
        .and_then(Value::as_object)
        .and_then(|recovery| recovery.get("next_command"))
        .and_then(Value::as_str)
        .filter(|command| !command.trim().is_empty())
        .map(str::to_string)
}

fn operator_row_safety_declaration_command(safety: Option<&Value>) -> Option<String> {
    safety
        .and_then(Value::as_object)
        .and_then(|safety| safety.get("declaration_command"))
        .and_then(Value::as_str)
        .filter(|command| !command.trim().is_empty())
        .map(str::to_string)
}

fn behavior_metadata_drifts(
    operator_rows: &BTreeMap<String, OperatorManifestRow>,
) -> Vec<OperatorBehaviorMetadataDrift> {
    let mut drifts = Vec::new();
    for (command_name, row) in operator_rows {
        let Some(binding) = row.safety_declaration_command.as_deref() else {
            drifts.push(OperatorBehaviorMetadataDrift {
                command: command_name.clone(),
                field: "safety.declaration_command".to_string(),
                expected: "known declaration command".to_string(),
                actual: None,
                source_declarations: Vec::new(),
            });
            continue;
        };
        if declaration_for(binding).is_none() {
            drifts.push(OperatorBehaviorMetadataDrift {
                command: command_name.clone(),
                field: "safety.declaration_command".to_string(),
                expected: "known declaration command".to_string(),
                actual: Some(binding.to_string()),
                source_declarations: Vec::new(),
            });
            continue;
        }
        let declarations = declarations_for_binding(binding);
        let sources = declarations
            .iter()
            .map(|declaration| declaration.command.to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let read_only_values = declarations
            .iter()
            .map(|declaration| declaration.read_only.to_string())
            .collect::<BTreeSet<_>>();
        push_scalar_set_drift(
            &mut drifts,
            command_name,
            "read_only",
            &read_only_values,
            row.read_only
                .map(|value| value.to_string())
                .into_iter()
                .collect(),
            &sources,
        );
        push_scalar_set_drift(
            &mut drifts,
            command_name,
            "safety.mutation_class",
            &declarations
                .iter()
                .map(|declaration| mutation_class_contract_name(declaration.mutation).to_string())
                .collect(),
            row.safety_string_set("mutation_class"),
            &sources,
        );
        push_scalar_set_drift(
            &mut drifts,
            command_name,
            "safety.network_class",
            &declarations
                .iter()
                .map(|declaration| network_class_contract_name(declaration.network).to_string())
                .collect(),
            row.safety_string_set("network_class"),
            &sources,
        );
        push_scalar_set_drift(
            &mut drifts,
            command_name,
            "safety.concurrency_class",
            &declarations
                .iter()
                .map(|declaration| {
                    concurrency_class_contract_name(declaration.concurrency).to_string()
                })
                .collect(),
            row.safety_string_set("concurrency_class"),
            &sources,
        );
        push_scalar_set_drift(
            &mut drifts,
            command_name,
            "safety.platform_classes",
            &declarations
                .iter()
                .flat_map(|declaration| {
                    declaration
                        .platforms
                        .iter()
                        .map(|platform| platform_class_contract_name(*platform).to_string())
                })
                .collect(),
            row.safety_string_set("platform_classes"),
            &sources,
        );
        if binding_does_not_match_row(row, &declarations) {
            drifts.push(OperatorBehaviorMetadataDrift {
                command: command_name.clone(),
                field: "safety.declaration_command".to_string(),
                expected: "declaration command whose typed safety matches row safety".to_string(),
                actual: Some(binding.to_string()),
                source_declarations: sources.clone(),
            });
        }
        push_safety_footprint_drifts(&mut drifts, command_name, row, &sources);
        if row
            .recovery_next_command
            .as_deref()
            .is_none_or(str::is_empty)
        {
            drifts.push(OperatorBehaviorMetadataDrift {
                command: command_name.clone(),
                field: "next_command".to_string(),
                expected: "non-empty recovery.next_command".to_string(),
                actual: None,
                source_declarations: sources,
            });
        }
    }
    drifts
}

fn binding_does_not_match_row(
    row: &OperatorManifestRow,
    declarations: &[&CommandSafetyDeclaration],
) -> bool {
    let expected_read_only = declarations
        .iter()
        .map(|declaration| declaration.read_only.to_string())
        .collect::<BTreeSet<_>>();
    let actual_read_only = row
        .read_only
        .map(|value| value.to_string())
        .into_iter()
        .collect::<BTreeSet<_>>();
    expected_read_only != actual_read_only
        || declarations
            .iter()
            .map(|declaration| mutation_class_contract_name(declaration.mutation).to_string())
            .collect::<BTreeSet<_>>()
            != row.safety_string_set("mutation_class")
        || declarations
            .iter()
            .map(|declaration| network_class_contract_name(declaration.network).to_string())
            .collect::<BTreeSet<_>>()
            != row.safety_string_set("network_class")
        || declarations
            .iter()
            .map(|declaration| concurrency_class_contract_name(declaration.concurrency).to_string())
            .collect::<BTreeSet<_>>()
            != row.safety_string_set("concurrency_class")
        || declarations
            .iter()
            .flat_map(|declaration| {
                declaration
                    .platforms
                    .iter()
                    .map(|platform| platform_class_contract_name(*platform).to_string())
            })
            .collect::<BTreeSet<_>>()
            != row.safety_string_set("platform_classes")
}

fn declarations_for_binding(binding: &str) -> Vec<&'static CommandSafetyDeclaration> {
    COMMAND_SAFETY_DECLARATIONS
        .iter()
        .filter(|declaration| declaration.command == binding)
        .collect()
}

impl OperatorManifestRow {
    fn safety_string_set(&self, field: &str) -> BTreeSet<String> {
        let Some(safety) = self.safety.as_ref().and_then(Value::as_object) else {
            return BTreeSet::new();
        };
        let Some(value) = safety.get(field) else {
            return BTreeSet::new();
        };
        match value {
            Value::String(value) => [value.to_string()].into_iter().collect(),
            Value::Array(values) => values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            _ => BTreeSet::new(),
        }
    }

    fn side_effect_bool(&self, field: &str) -> Option<bool> {
        self.side_effects
            .as_ref()
            .and_then(Value::as_object)
            .and_then(|side_effects| side_effects.get(field))
            .and_then(Value::as_bool)
    }

    fn true_write_or_network_effects(&self) -> Vec<String> {
        let Some(side_effects) = self.side_effects.as_ref().and_then(Value::as_object) else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        for (field, value) in side_effects {
            let is_write_or_network = field.starts_with("writes_")
                || field == "uses_network"
                || field == "appends_witness_ledger"
                || field == "creates_witness_directory";
            if is_write_or_network && value.as_bool() == Some(true) {
                effects.push(field.to_string());
            }
        }
        effects.sort();
        effects
    }
}

fn push_safety_footprint_drifts(
    drifts: &mut Vec<OperatorBehaviorMetadataDrift>,
    command: &str,
    row: &OperatorManifestRow,
    source_declarations: &[String],
) {
    if row.safety_string_set("mutation_class").contains("ReadOnly") {
        if row.read_only != Some(true) {
            drifts.push(OperatorBehaviorMetadataDrift {
                command: command.to_string(),
                field: "read_only".to_string(),
                expected: "true because safety.mutation_class is ReadOnly".to_string(),
                actual: row.read_only.map(|value| value.to_string()),
                source_declarations: source_declarations.to_vec(),
            });
        }
        let true_effects = row.true_write_or_network_effects();
        if !true_effects.is_empty() {
            drifts.push(OperatorBehaviorMetadataDrift {
                command: command.to_string(),
                field: "side_effects".to_string(),
                expected:
                    "no write or network booleans true when safety.mutation_class is ReadOnly"
                        .to_string(),
                actual: Some(true_effects.join(",")),
                source_declarations: source_declarations.to_vec(),
            });
        }
    }
    if row.safety_string_set("network_class").contains("Offline")
        && row.side_effect_bool("uses_network") != Some(false)
    {
        drifts.push(OperatorBehaviorMetadataDrift {
            command: command.to_string(),
            field: "side_effects.uses_network".to_string(),
            expected: "false because safety.network_class is Offline".to_string(),
            actual: row
                .side_effect_bool("uses_network")
                .map(|value| value.to_string()),
            source_declarations: source_declarations.to_vec(),
        });
    }
}

fn push_scalar_set_drift(
    drifts: &mut Vec<OperatorBehaviorMetadataDrift>,
    command: &str,
    field: &str,
    expected: &BTreeSet<String>,
    actual: BTreeSet<String>,
    source_declarations: &[String],
) {
    if actual == *expected {
        return;
    }
    drifts.push(OperatorBehaviorMetadataDrift {
        command: command.to_string(),
        field: field.to_string(),
        expected: render_string_set(expected),
        actual: if actual.is_empty() {
            None
        } else {
            Some(render_string_set(&actual))
        },
        source_declarations: source_declarations.to_vec(),
    });
}

fn render_string_set(values: &BTreeSet<String>) -> String {
    if values.len() == 1 {
        return values
            .iter()
            .next()
            .expect("single value exists")
            .to_string();
    }
    values.iter().cloned().collect::<Vec<_>>().join(",")
}

fn push_canonical_json(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => out.push_str(&value.to_string()),
        Value::String(value) => {
            out.push_str(
                &serde_json::to_string(value).expect("string serialization is infallible"),
            );
        }
        Value::Array(values) => {
            out.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                push_canonical_json(value, out);
            }
            out.push(']');
        }
        Value::Object(values) => {
            out.push('{');
            for (index, (key, value)) in
                values.iter().collect::<BTreeMap<_, _>>().iter().enumerate()
            {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("key serialization is infallible"));
                out.push(':');
                push_canonical_json(value, out);
            }
            out.push('}');
        }
    }
}

fn mutation_class_contract_name(value: MutationClass) -> &'static str {
    match value {
        MutationClass::ReadOnly => "ReadOnly",
        MutationClass::OwnedOutput => "OwnedOutput",
        MutationClass::CacheOnly => "CacheOnly",
        MutationClass::RegistryMutation => "RegistryMutation",
        MutationClass::PublicationTransaction => "PublicationTransaction",
        MutationClass::ExternalMaterialization => "ExternalMaterialization",
    }
}

fn network_class_contract_name(value: NetworkClass) -> &'static str {
    match value {
        NetworkClass::Offline => "Offline",
        NetworkClass::DeniedByDefault => "DeniedByDefault",
        NetworkClass::ExplicitExternalProvider => "ExplicitExternalProvider",
    }
}

fn concurrency_class_contract_name(value: ConcurrencyClass) -> &'static str {
    match value {
        ConcurrencyClass::StatelessRead => "StatelessRead",
        ConcurrencyClass::AtomicOwnedOutput => "AtomicOwnedOutput",
        ConcurrencyClass::CacheRaceSafe => "CacheRaceSafe",
        ConcurrencyClass::ExclusiveRegistryMutation => "ExclusiveRegistryMutation",
        ConcurrencyClass::OptimisticPublicationCas => "OptimisticPublicationCas",
        ConcurrencyClass::IsolatedRunner => "IsolatedRunner",
    }
}

fn platform_class_contract_name(value: PlatformClass) -> &'static str {
    match value {
        PlatformClass::PortablePathUtf8 => "PortablePathUtf8",
        PlatformClass::SameFilesystemAtomicReplace => "SameFilesystemAtomicReplace",
        PlatformClass::UnixPermissionBits => "UnixPermissionBits",
        PlatformClass::RejectLinks => "RejectLinks",
        PlatformClass::AdvisoryFileLock => "AdvisoryFileLock",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, Command};
    use serde_json::json;

    fn sample_command() -> Command {
        Command::new("canon")
            .subcommand(
                Command::new("alpha").subcommand(
                    Command::new("run")
                        .arg(Arg::new("input").long("input"))
                        .arg(Arg::new("emit").long("emit")),
                ),
            )
            .subcommand(
                Command::new("beta")
                    .arg(Arg::new("cache").long("cache-mode"))
                    .subcommand(Command::new("show").arg(Arg::new("id").long("id"))),
            )
    }

    fn doctor_command() -> Command {
        Command::new("canon").subcommand(Command::new("doctor"))
    }

    fn doctor_safety() -> Value {
        json!({
            "declaration_command": "doctor",
            "mutation_class": "ReadOnly",
            "network_class": "Offline",
            "concurrency_class": "StatelessRead",
            "platform_classes": ["PortablePathUtf8"]
        })
    }

    fn doctor_side_effects() -> Value {
        json!({
            "uses_network": false
        })
    }

    #[test]
    fn leaf_command_helpers_are_sorted_and_include_only_leaf_flags() {
        let command = sample_command();

        assert_eq!(
            public_leaf_commands_from(&command),
            vec!["alpha run".to_string(), "beta show".to_string()]
        );
        assert_eq!(
            public_leaf_long_flags_from(&command),
            vec![
                PublicLeafLongFlags {
                    command: "alpha run".to_string(),
                    long_flags: vec!["emit".to_string(), "input".to_string()]
                },
                PublicLeafLongFlags {
                    command: "beta show".to_string(),
                    long_flags: vec!["id".to_string()]
                }
            ]
        );
    }

    #[test]
    fn manifest_validation_reports_missing_and_extra_surface_drift() {
        let manifest = json!({
            "subcommands": [
                {
                    "name": "alpha run",
                    "usage": "canon alpha run --input <INPUT>",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "example.v0",
                    "exit_codes": {},
                    "options": [
                        {"flag": "--input"}
                    ]
                },
                {
                    "name": "gamma extra",
                    "usage": "canon gamma extra",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "example.v0",
                    "exit_codes": {},
                    "options": [],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": {}
                }
            ]
        });

        let report = validate_operator_manifest(&sample_command(), &manifest);

        assert!(!report.ok);
        assert_eq!(report.missing_leaf_commands, vec!["beta show".to_string()]);
        assert_eq!(
            report.extra_operator_commands,
            vec!["gamma extra".to_string()]
        );
        assert_eq!(
            report.flag_drifts,
            vec![OperatorCommandFlagDrift {
                command: "alpha run".to_string(),
                missing_flags: vec!["emit".to_string()],
                extra_flags: Vec::new()
            }]
        );
        assert!(
            report
                .missing_required_fields
                .iter()
                .any(|field| { field.command == "alpha run" && field.field == "status" })
        );
    }

    #[test]
    fn aggregate_rows_are_not_executable_leaves_but_their_leaves_are_checked() {
        let manifest = json!({
            "subcommands": [
                {
                    "name": "beta",
                    "usage": "canon beta ...",
                    "aggregate": true,
                    "leaves": ["beta show"]
                },
                {
                    "name": "alpha run",
                    "usage": "canon alpha run --input <INPUT> --emit json",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "example.v0",
                    "exit_codes": {},
                    "options": [
                        {"flag": "--input"},
                        {"flag": "--emit"}
                    ],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": {}
                },
                {
                    "name": "beta show",
                    "usage": "canon beta show --id <ID>",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "example.v0",
                    "exit_codes": {},
                    "options": [
                        {"flag": "--cache-mode"},
                        {"flag": "--id"}
                    ],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": {}
                }
            ]
        });

        let report = validate_operator_manifest(&sample_command(), &manifest);

        assert!(!report.extra_operator_commands.contains(&"beta".to_string()));
        assert!(
            !report
                .manifest_errors
                .iter()
                .any(|error| error.contains("unknown leaf"))
        );
    }

    #[test]
    fn aggregate_rows_refuse_unknown_declared_leaves() {
        let manifest = json!({
            "subcommands": [
                {
                    "name": "beta",
                    "usage": "canon beta ...",
                    "aggregate": true,
                    "leaves": ["beta missing"]
                }
            ]
        });

        let report = validate_operator_manifest(&sample_command(), &manifest);

        assert!(
            report.manifest_errors.iter().any(|error| error
                == "aggregate operator row 'beta' declares unknown leaf 'beta missing'")
        );
    }

    #[test]
    fn behavior_validation_checks_safety_classes_and_recovery() {
        let manifest = json!({
            "subcommands": [
                {
                    "name": "doctor",
                    "usage": "canon doctor health --json",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "canon.doctor.health.v1",
                    "exit_codes": {},
                    "options": [],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": doctor_side_effects(),
                    "safety": doctor_safety(),
                    "next_command": "canon doctor health --json",
                    "recovery": {
                        "next_command": "canon doctor health --json"
                    }
                }
            ]
        });

        let report = validate_operator_manifest(&doctor_command(), &manifest);

        assert!(report.behavior_drifts.is_empty());
    }

    #[test]
    fn behavior_validation_reports_safety_class_and_recovery_drift() {
        let manifest = json!({
            "subcommands": [
                {
                    "name": "doctor",
                    "usage": "canon doctor health --json",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "canon.doctor.health.v1",
                    "exit_codes": {},
                    "options": [],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": doctor_side_effects(),
                    "safety": {
                        "declaration_command": "doctor",
                        "mutation_class": "ReadOnly",
                        "network_class": "ExplicitExternalProvider",
                        "concurrency_class": "StatelessRead",
                        "platform_classes": []
                    }
                }
            ]
        });

        let report = validate_operator_manifest(&doctor_command(), &manifest);

        assert!(report.behavior_drifts.iter().any(|drift| {
            drift.command == "doctor"
                && drift.field == "safety.network_class"
                && drift.expected == "Offline"
                && drift.actual.as_deref() == Some("ExplicitExternalProvider")
        }));
        assert!(report.behavior_drifts.iter().any(|drift| {
            drift.command == "doctor" && drift.field == "safety.platform_classes"
        }));
        assert!(
            report
                .behavior_drifts
                .iter()
                .any(|drift| drift.command == "doctor" && drift.field == "next_command")
        );
    }

    #[test]
    fn behavior_validation_cross_checks_read_only_and_offline_footprint() {
        let manifest = json!({
            "subcommands": [
                {
                    "name": "doctor",
                    "usage": "canon doctor health --json",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "canon.doctor.health.v1",
                    "exit_codes": {},
                    "options": [],
                    "status": "implemented",
                    "read_only": false,
                    "side_effects": {
                        "writes_output_files": true,
                        "uses_network": true
                    },
                    "safety": doctor_safety(),
                    "next_command": "canon doctor health --json",
                    "recovery": {
                        "next_command": "canon doctor health --json"
                    }
                }
            ]
        });

        let report = validate_operator_manifest(&doctor_command(), &manifest);

        assert!(
            report
                .behavior_drifts
                .iter()
                .any(|drift| drift.command == "doctor"
                    && drift.field == "read_only"
                    && drift.expected == "true because safety.mutation_class is ReadOnly")
        );
        assert!(
            report
                .behavior_drifts
                .iter()
                .any(|drift| drift.command == "doctor" && drift.field == "side_effects")
        );
        assert!(report.behavior_drifts.iter().any(|drift| {
            drift.command == "doctor" && drift.field == "side_effects.uses_network"
        }));
    }

    #[test]
    fn root_options_are_validated_separately_from_leaf_flags() {
        let command = Command::new("canon")
            .arg(Arg::new("describe").long("describe"))
            .subcommand(Command::new("doctor"));
        let manifest = json!({
            "options": [
                {"name": "stale", "flag": "--stale"}
            ],
            "subcommands": [
                {
                    "name": "doctor",
                    "usage": "canon doctor",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "canon.doctor.health.v1",
                    "exit_codes": {},
                    "options": [],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": {},
                    "safety": {"declaration_command": "doctor"},
                    "recovery": {}
                }
            ]
        });

        let report = validate_operator_manifest(&command, &manifest);

        assert!(report.flag_drifts.iter().any(|drift| {
            drift.command == "canon"
                && drift.missing_flags == vec!["describe".to_string()]
                && drift.extra_flags == vec!["stale".to_string()]
        }));
    }

    #[test]
    fn root_contract_validates_identity_schema_and_arguments() {
        let command = Command::new("canon")
            .arg(Arg::new("input").index(1))
            .subcommand(Command::new("doctor"));
        let manifest = json!({
            "schema_version": "stale",
            "name": "not-canon",
            "version": "0.0.0",
            "arguments": [
                {"name": "wrong", "position": 0}
            ],
            "invocation": {
                "output_schema": "wrong.v0"
            },
            "subcommands": [
                {
                    "name": "doctor",
                    "usage": "canon doctor",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "canon.doctor.health.v1",
                    "exit_codes": {"0": {"meaning": "ok"}},
                    "options": [],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": doctor_side_effects(),
                    "safety": doctor_safety(),
                    "recovery": {"next_command": "canon doctor health --json"}
                }
            ]
        });

        let report = validate_operator_manifest(&command, &manifest);

        assert!(
            report
                .manifest_errors
                .iter()
                .any(|error| error.contains("schema_version must be operator.v0"))
        );
        assert!(
            report
                .manifest_errors
                .iter()
                .any(|error| error.contains("operator manifest name must be canon"))
        );
        assert!(
            report
                .manifest_errors
                .iter()
                .any(|error| error.contains("invocation.output_schema must be canon.v0"))
        );
        assert!(
            report
                .manifest_errors
                .iter()
                .any(|error| error.contains("root arguments must match compiled args"))
        );
    }

    #[test]
    fn row_usage_and_positionals_are_validated_against_compiled_leaf() {
        let command = Command::new("canon").subcommand(
            Command::new("package").subcommand(
                Command::new("inspect")
                    .arg(Arg::new("archive").index(1))
                    .arg(Arg::new("emit").long("emit")),
            ),
        );
        let manifest = json!({
            "subcommands": [
                {
                    "name": "package inspect",
                    "usage": "canon package stale <ARCHIVE>",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "canon.package.inspect.v1",
                    "exit_codes": {},
                    "options": [
                        {"name": "wrong_archive", "position": 0},
                        {"flag": "--emit"}
                    ],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": {},
                    "safety": {"declaration_command": "package inspect"},
                    "recovery": {}
                }
            ]
        });

        let report = validate_operator_manifest(&command, &manifest);

        assert!(
            report.manifest_errors.iter().any(|error| error.contains(
                "operator row 'package inspect' usage must begin with exact command path"
            ))
        );
        assert!(
            report.manifest_errors.iter().any(|error| error.contains(
                "operator row 'package inspect' positional args must match compiled args"
            ))
        );
    }

    #[test]
    fn safety_binding_requires_known_typed_declaration() {
        let manifest = json!({
            "subcommands": [
                {
                    "name": "doctor",
                    "usage": "canon doctor",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "canon.doctor.health.v1",
                    "exit_codes": {"0": {"meaning": "ok"}},
                    "options": [],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": doctor_side_effects(),
                    "safety": {
                        "mutation_class": "ReadOnly",
                        "network_class": "Offline",
                        "concurrency_class": "StatelessRead",
                        "platform_classes": ["PortablePathUtf8"]
                    },
                    "recovery": {"next_command": "canon doctor health --json"}
                },
                {
                    "name": "ghost",
                    "usage": "canon ghost",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "ghost.v0",
                    "exit_codes": {"0": {"meaning": "ok"}},
                    "options": [],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": doctor_side_effects(),
                    "safety": {
                        "declaration_command": "ghost declaration",
                        "mutation_class": "ReadOnly",
                        "network_class": "Offline",
                        "concurrency_class": "StatelessRead",
                        "platform_classes": ["PortablePathUtf8"]
                    },
                    "recovery": {"next_command": "canon ghost"}
                }
            ]
        });

        let report = validate_operator_manifest(&doctor_command(), &manifest);

        assert!(report.behavior_drifts.iter().any(|drift| {
            drift.command == "doctor" && drift.field == "safety.declaration_command"
        }));
        assert!(report.behavior_drifts.iter().any(|drift| {
            drift.command == "ghost"
                && drift.field == "safety.declaration_command"
                && drift.actual.as_deref() == Some("ghost declaration")
        }));
    }

    #[test]
    fn doctor_aggregate_parent_options_do_not_become_synthetic_leaves() {
        let command = Command::new("canon").subcommand(
            Command::new("doctor")
                .arg(Arg::new("robot_triage").long("robot-triage"))
                .subcommand(Command::new("health").arg(Arg::new("json").long("json")))
                .subcommand(Command::new("capabilities").arg(Arg::new("json").long("json")))
                .subcommand(Command::new("robot-docs")),
        );
        let manifest = json!({
            "subcommands": [
                {
                    "name": "doctor",
                    "usage": "canon doctor [health [--json]|capabilities [--json]|robot-docs|--robot-triage]",
                    "aggregate": true,
                    "leaves": ["doctor health", "doctor capabilities", "doctor robot-docs"],
                    "options": [
                        {"name": "robot_triage", "flag": "--robot-triage"}
                    ],
                    "description": "doctor parent",
                    "output_mode": "mixed",
                    "output_schema": "canon.doctor.health.v1",
                    "exit_codes": {"0": {"meaning": "ok"}},
                    "status": "implemented",
                    "read_only": true,
                    "safety": doctor_safety(),
                    "side_effects": doctor_side_effects(),
                    "recovery": {
                        "next_command": "canon doctor health --json"
                    }
                },
                {
                    "name": "doctor health",
                    "usage": "canon doctor health [--json]",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "canon.doctor.health.v1",
                    "exit_codes": {},
                    "options": [{"name": "json", "flag": "--json"}],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": doctor_side_effects(),
                    "safety": doctor_safety(),
                    "recovery": {"next_command": "canon doctor health --json"}
                },
                {
                    "name": "doctor capabilities",
                    "usage": "canon doctor capabilities [--json]",
                    "description": "example",
                    "output_mode": "json",
                    "output_schema": "canon.doctor.capabilities.v1",
                    "exit_codes": {},
                    "options": [{"name": "json", "flag": "--json"}],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": doctor_side_effects(),
                    "safety": doctor_safety(),
                    "recovery": {"next_command": "canon doctor capabilities --json"}
                },
                {
                    "name": "doctor robot-docs",
                    "usage": "canon doctor robot-docs",
                    "description": "example",
                    "output_mode": "text",
                    "output_schema": "text/plain",
                    "exit_codes": {},
                    "options": [],
                    "status": "implemented",
                    "read_only": true,
                    "side_effects": doctor_side_effects(),
                    "safety": doctor_safety(),
                    "recovery": {"next_command": "canon doctor robot-docs"}
                }
            ]
        });

        let report = validate_operator_manifest(&command, &manifest);

        assert!(
            !report
                .manifest_errors
                .iter()
                .any(|error| error.contains("doctor --robot-triage"))
        );
        assert!(!report.flag_drifts.iter().any(|drift| {
            drift.command.starts_with("doctor")
                && drift.missing_flags.contains(&"robot-triage".to_string())
        }));
    }

    #[test]
    fn stable_manifest_digest_sorts_object_keys() {
        let left = json!({"b": [2, 1], "a": true});
        let right = json!({"a": true, "b": [2, 1]});

        assert_eq!(
            stable_manifest_digest(&left),
            stable_manifest_digest(&right)
        );
        assert!(stable_manifest_digest(&left).starts_with("blake3:"));
    }

    #[test]
    fn behavior_metadata_exposes_existing_safety_declarations() {
        let metadata = command_behavior_metadata();

        assert!(metadata.iter().any(|entry| {
            entry.command == "doctor"
                && entry.operator_contract_name.as_deref() == Some("doctor")
                && entry.read_only
                && entry.network == "Offline"
        }));
    }
}
