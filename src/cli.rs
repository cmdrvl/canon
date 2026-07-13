use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Emit mode for output
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum EmitMode {
    /// JSON mapping artifact (default)
    #[default]
    Json,
    /// CSV with canonical column appended
    Csv,
}

/// Emit mode for registry reporting subcommands
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum RegistryEmitMode {
    /// Structured registry JSON (default)
    #[default]
    Json,
    /// Human-readable registry summary
    Summary,
}

/// Emit mode for registry commands that default to shell-composable text
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum RegistryPlainJsonEmitMode {
    /// Plain text value for shell composition (default)
    #[default]
    Plain,
    /// Structured registry JSON
    Json,
}

/// Registry version bump mode
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum RegistryVersionBumpMode {
    /// Increment MAJOR.MINOR.PATCH patch component
    Patch,
    /// Increment minor and reset patch
    Minor,
    /// Increment major and reset minor/patch
    Major,
}

/// Emit mode for entity artifact subcommands
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum EntityEmitMode {
    /// Structured entity JSON artifact (default)
    #[default]
    Json,
    /// Human-readable entity summary
    Summary,
}

/// Entity index cache mode for artifact-backed entity runs
#[derive(Debug, Clone, Copy, ValueEnum, Default, PartialEq, Eq)]
pub enum EntityCacheModeArg {
    /// Read and write verified entity index cache artifacts
    #[default]
    Enabled,
    /// Bypass cache reads and emit a disabled-cache receipt
    Disabled,
}

/// Emit mode for project ergonomics subcommands
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum ProjectEmitMode {
    /// Structured project JSON artifact (default)
    #[default]
    Json,
    /// Human-readable project summary
    Summary,
}

/// Emit mode for entity streaming subcommands
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum EntityStreamEmitMode {
    /// Line-delimited entity records (default)
    #[default]
    Jsonl,
    /// Human-readable entity summary
    Summary,
}

/// Emit mode for entity review export
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum EntityReviewExportEmitMode {
    /// Structured review JSON artifact (default)
    #[default]
    Json,
    /// Human-reviewable CSV artifact
    Csv,
    /// Offline native review HTML artifact
    Html,
}

/// Artifact contract for entity review export
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum EntityReviewExportArtifact {
    /// Existing queue/v1/legacy review contracts (default)
    #[default]
    Queue,
    /// Native offline review artifact contract
    #[value(name = "native-review")]
    NativeReview,
}

/// Include selector for entity review export
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum EntityReviewInclude {
    /// Resolved and promotable entities
    Resolved,
    /// Escrowed abstentions
    Escrow,
    /// Contradiction records
    Contradictions,
    /// All reviewable items
    #[default]
    All,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum StrategyGradeArg {
    /// Lightweight operator attestation without verify/assess/airlock artifacts
    #[value(name = "operator-attested")]
    OperatorAttested,
    /// Proof-gated attestation with verify, assess, and airlock artifacts
    #[default]
    #[value(name = "proof-attested")]
    ProofAttested,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StrategyStatusArg {
    /// Active entries participate in resolution
    Active,
    /// Deprecated entries are preserved but ignored by resolution
    Deprecated,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StrategyKeyTypeArg {
    /// Schema/profile keyed entries
    Schema,
    /// Task/intent keyed entries
    Task,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CanonCommand {
    /// Read-only health, capabilities, and robot-oriented diagnostics
    Doctor(DoctorArgs),
    /// Local package archive operations
    Package(PackageCli),
    /// Project bootstrap, validation, and description commands
    Project(ProjectCli),
    /// Unresolved inbox triage, review export, and bounded entity planning
    Inbox(InboxCli),
    /// Registry maintenance and inspection commands
    Registry(RegistryCommand),
    /// Profiled entity workbench commands
    #[command(name = "entity")]
    Entity(EntityCommand),
    /// Frozen script strategy registry commands
    Strategy(StrategyCommand),
}

#[derive(Args, Debug, Clone)]
pub struct PackageCli {
    #[command(subcommand)]
    pub command: PackageSubcommand,
}

#[derive(Args, Debug, Clone)]
pub struct ProjectCli {
    #[command(subcommand)]
    pub command: ProjectSubcommand,
}

#[derive(Args, Debug, Clone)]
pub struct InboxCli {
    #[command(subcommand)]
    pub command: InboxSubcommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PackageSubcommand {
    /// Create a deterministic local package archive from canonical package bytes
    Pack(PackagePackCli),
    /// Extract structured package inventory and metadata without writing files
    Inspect(PackageInspectCli),
    /// Verify package archive digests and semantic package contracts
    Verify(PackageVerifyCli),
    /// Unpack a verified package archive into an existing empty target directory
    Unpack(PackageUnpackCli),
    /// Publish a verified local package archive to an OCI registry by immutable digest
    Push(PackagePushCli),
    /// Pull and verify a package from an OCI registry into an external content cache
    Pull(PackagePullCli),
}

#[derive(Subcommand, Debug, Clone)]
pub enum ProjectSubcommand {
    /// Create a minimal neutral project manifest in an explicit empty directory
    Init(ProjectInitCli),
    /// Validate a project manifest and report deterministic diagnostics
    Validate(ProjectValidateCli),
    /// Describe project capabilities, state flags, side effects, and next commands
    Describe(ProjectDescribeCli),
}

#[derive(Subcommand, Debug, Clone)]
pub enum InboxSubcommand {
    /// List ranked unresolved inbox items with deterministic pagination
    List(InboxListCli),
    /// Show one unresolved inbox item and its next commands
    Show(InboxShowCli),
    /// Explain one item's priority score components and provenance
    Explain(InboxExplainCli),
    /// Summarize inbox counts and ranking coverage
    Stats(InboxStatsCli),
    /// Export a stable review queue for selected inbox items
    #[command(name = "export-review")]
    ExportReview(InboxExportReviewCli),
    /// Apply explicit review decisions into a grouped unresolved artifact
    #[command(name = "apply-review")]
    ApplyReview(InboxApplyReviewCli),
    /// Plan a bounded entity workbench request without deciding identity
    #[command(name = "plan-entity")]
    PlanEntity(InboxPlanEntityCli),
}

#[derive(Args, Debug, Clone)]
pub struct InboxListCli {
    /// Unresolved inbox JSON artifact
    #[arg(long)]
    pub inbox: PathBuf,

    /// Optional priority policy JSON; defaults to a deterministic baseline policy
    #[arg(long)]
    pub policy: Option<PathBuf>,

    /// Maximum items to return
    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    /// Cursor from a previous list/export page
    #[arg(long)]
    pub cursor: Option<String>,

    /// Filter by event kind; repeatable
    #[arg(long = "event-kind")]
    pub event_kind: Vec<String>,

    /// Filter by reason code; repeatable
    #[arg(long = "reason-code")]
    pub reason_code: Vec<String>,

    /// Filter by field role; repeatable
    #[arg(long = "field-role")]
    pub field_role: Vec<String>,

    /// Filter by ranking partition key; repeatable
    #[arg(long)]
    pub partition: Vec<String>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct InboxShowCli {
    /// Unresolved inbox JSON artifact
    #[arg(long)]
    pub inbox: PathBuf,

    /// Event key to inspect
    #[arg(long = "event-key")]
    pub event_key: String,

    /// Optional priority policy JSON; defaults to a deterministic baseline policy
    #[arg(long)]
    pub policy: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct InboxExplainCli {
    /// Unresolved inbox JSON artifact
    #[arg(long)]
    pub inbox: PathBuf,

    /// Event key to explain
    #[arg(long = "event-key")]
    pub event_key: String,

    /// Optional priority policy JSON; defaults to a deterministic baseline policy
    #[arg(long)]
    pub policy: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct InboxStatsCli {
    /// Unresolved inbox JSON artifact
    #[arg(long)]
    pub inbox: PathBuf,

    /// Optional priority policy JSON; defaults to a deterministic baseline policy
    #[arg(long)]
    pub policy: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct InboxExportReviewCli {
    /// Unresolved inbox JSON artifact
    #[arg(long)]
    pub inbox: PathBuf,

    /// Explicit output file for the review artifact; stdout is used when omitted
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Optional priority policy JSON; defaults to a deterministic baseline policy
    #[arg(long)]
    pub policy: Option<PathBuf>,

    /// Maximum items to export
    #[arg(long, default_value_t = 100)]
    pub limit: usize,

    /// Cursor from a previous list/export page
    #[arg(long)]
    pub cursor: Option<String>,

    /// Filter by event kind; repeatable
    #[arg(long = "event-kind")]
    pub event_kind: Vec<String>,

    /// Filter by reason code; repeatable
    #[arg(long = "reason-code")]
    pub reason_code: Vec<String>,

    /// Filter by field role; repeatable
    #[arg(long = "field-role")]
    pub field_role: Vec<String>,

    /// Filter by ranking partition key; repeatable
    #[arg(long)]
    pub partition: Vec<String>,

    /// Output mode for the receipt/stdout artifact
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct InboxApplyReviewCli {
    /// Unresolved inbox JSON artifact
    #[arg(long)]
    pub inbox: PathBuf,

    /// Review decision JSON from canon inbox export-review or an operator-edited equivalent
    #[arg(long)]
    pub review: PathBuf,

    /// Expected inbox artifact hash; refuses stale review application
    #[arg(long = "expected-inbox-hash")]
    pub expected_inbox_hash: String,

    /// Explicit grouped artifact output path
    #[arg(long)]
    pub out: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum InboxEntityPlanMode {
    /// Build a cluster-mode entity request
    #[default]
    Cluster,
    /// Build a link-mode entity request
    Link,
}

#[derive(Args, Debug, Clone)]
pub struct InboxPlanEntityCli {
    /// Unresolved inbox JSON artifact
    #[arg(long)]
    pub inbox: PathBuf,

    /// Expected inbox artifact hash; refuses stale planning inputs
    #[arg(long = "expected-inbox-hash")]
    pub expected_inbox_hash: String,

    /// Explicit entity request output path
    #[arg(long)]
    pub out: PathBuf,

    /// Optional priority policy JSON; defaults to a deterministic baseline policy
    #[arg(long)]
    pub policy: Option<PathBuf>,

    /// Event key to include; repeatable. Defaults to top ranked item.
    #[arg(long = "event-key")]
    pub event_key: Vec<String>,

    /// Maximum ranked items to include when --event-key is omitted
    #[arg(long, default_value_t = 1)]
    pub limit: usize,

    /// Entity workbench mode to request
    #[arg(long, value_enum, default_value = "cluster")]
    pub mode: InboxEntityPlanMode,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct ProjectInitCli {
    /// Empty or missing project directory to initialize
    pub directory: PathBuf,

    /// Neutral project identifier to write into the generated manifest
    #[arg(long = "project-id", default_value = "project.synthetic.alpha")]
    pub project_id: String,

    /// External mapping/profile reference for the sample source declaration
    #[arg(long = "mapping-profile", default_value = "pkg.synthetic:contacts")]
    pub mapping_profile: String,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: ProjectEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct ProjectValidateCli {
    /// Project directory containing the manifest
    pub directory: PathBuf,

    /// Manifest path, relative to the project directory unless absolute
    #[arg(long, default_value = "canon.project.toml")]
    pub manifest: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: ProjectEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct ProjectDescribeCli {
    /// Project directory containing the manifest
    pub directory: PathBuf,

    /// Manifest path, relative to the project directory unless absolute
    #[arg(long, default_value = "canon.project.toml")]
    pub manifest: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: ProjectEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct PackagePackCli {
    /// Package root directory to normalize and archive
    #[arg(long)]
    pub root: PathBuf,

    /// Canonical package JSON bytes to bind into package.json
    #[arg(long)]
    pub package: PathBuf,

    /// Single-file local archive output path
    #[arg(long)]
    pub out: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct PackageInspectCli {
    /// Local package archive to inspect
    pub archive: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct PackageVerifyCli {
    /// Local package archive to verify
    pub archive: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct PackageUnpackCli {
    /// Local package archive to unpack
    pub archive: PathBuf,

    /// Existing empty target directory
    #[arg(long)]
    pub target: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct PackagePushCli {
    /// Local package archive to verify and publish
    #[arg(long)]
    pub archive: PathBuf,

    /// OCI Distribution registry base URL, for example http://127.0.0.1:5000
    #[arg(long)]
    pub registry: String,

    /// OCI repository name, for example canon/registry
    #[arg(long)]
    pub repository: String,

    /// Optional mutable tag to write after uploading the immutable digest
    #[arg(long)]
    pub tag: Option<String>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
#[command(group(ArgGroup::new("reference").required(true).multiple(false).args(["digest", "tag"])))]
pub struct PackagePullCli {
    /// OCI Distribution registry base URL, for example http://127.0.0.1:5000
    #[arg(long)]
    pub registry: String,

    /// OCI repository name, for example canon/registry
    #[arg(long)]
    pub repository: String,

    /// External content cache directory to materialize verified package bytes into
    #[arg(long)]
    pub cache: PathBuf,

    /// Immutable OCI manifest digest to pull, such as sha256:<64 lowercase hex>
    #[arg(long)]
    pub digest: Option<String>,

    /// Mutable tag to resolve exactly once before pulling by the resolved digest
    #[arg(long)]
    pub tag: Option<String>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct DoctorArgs {
    /// Emit compact machine-readable triage JSON
    #[arg(long = "robot-triage")]
    pub robot_triage: bool,

    /// Emit health JSON when no doctor subcommand is provided
    #[arg(long)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<DoctorCommand>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum DoctorCommand {
    /// Report compiled manifest and read-only contract health
    Health(DoctorJsonArgs),
    /// Describe doctor commands, exit codes, side effects, and fixers
    Capabilities(DoctorJsonArgs),
    /// Emit concise machine-oriented usage notes
    #[command(name = "robot-docs")]
    RobotDocs,
}

#[derive(Args, Debug, Clone)]
pub struct DoctorJsonArgs {
    /// Emit JSON instead of concise text
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct RegistryCommand {
    #[command(subcommand)]
    pub command: RegistrySubcommand,
}

#[derive(Args, Debug, Clone)]
pub struct EntityCommand {
    #[command(subcommand)]
    pub command: EntitySubcommand,
}

#[derive(Args, Debug, Clone)]
pub struct EntityProfileCommand {
    #[command(subcommand)]
    pub command: EntityProfileSubcommand,
}

#[derive(Args, Debug, Clone)]
pub struct EntityReviewCommand {
    #[command(subcommand)]
    pub command: EntityReviewSubcommand,
}

#[derive(Args, Debug, Clone)]
pub struct EntityIndexCommand {
    #[command(subcommand)]
    pub command: EntityIndexSubcommand,
}

#[derive(Args, Debug, Clone)]
pub struct StrategyCommand {
    #[command(subcommand)]
    pub command: StrategySubcommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum RegistrySubcommand {
    /// Export a registry as a downstream serving or transform artifact
    Export(RegistryExportCli),
    /// Suggest the next canonical ID for a self-authored registry namespace
    #[command(name = "next-id")]
    NextId(RegistryNextIdCli),
    /// Append one exact alias entry to a self-authored registry
    #[command(name = "add-entry")]
    AddEntry(RegistryAddEntryCli),
    /// Mint one self-authored canonical ID with one or more starting aliases
    Mint(RegistryMintCli),
    /// Persist a registry's default self-authored canonical ID scheme
    #[command(name = "default-id-scheme")]
    DefaultIdScheme(RegistryDefaultIdSchemeCli),
    /// Compare two registry versions and report what changed
    Diff(RegistryDiffCli),
    /// Audit a seed corpus against a registry for authoring workflows
    Audit(RegistryAuditCli),
    /// Materialize a registry from a provider and seed corpus
    Build(RegistryBuildCli),
    /// Check registry health before production use
    Lint(RegistryLintCli),
    /// List registry build providers available for materialization
    Providers(RegistryProvidersCli),
    /// Show the --provider-config option schema for one provider
    #[command(name = "provider-schema")]
    ProviderSchema(RegistryProviderSchemaCli),
}

#[derive(Debug, Clone, ValueEnum)]
pub enum RegistryExportFormatCli {
    /// Deterministic CSV seed for dbt or SQL transform tools
    #[value(name = "dbt-seed")]
    DbtSeed,
    /// Self-describing SQLite search artifact for serving endpoints
    #[value(name = "search-index")]
    SearchIndex,
}

#[derive(Args, Debug, Clone)]
pub struct RegistryExportCli {
    /// Registry directory to export
    #[arg(long)]
    pub registry: PathBuf,

    /// Export format
    #[arg(long, value_enum)]
    pub format: RegistryExportFormatCli,

    /// Output artifact path
    #[arg(long)]
    pub out: PathBuf,

    /// Required context namespace for dbt-seed exports; optional metadata for search-index exports
    #[arg(long)]
    pub namespace: Option<String>,

    /// Include only entries from this root-level mapping file; repeatable
    #[arg(long = "source-file")]
    pub source_files: Vec<String>,

    /// Include only entries with this canonical_type; repeatable
    #[arg(long = "canonical-type")]
    pub canonical_types: Vec<String>,

    /// Include only entries whose rule_id starts with this prefix; repeatable
    #[arg(long = "rule-id-prefix")]
    pub rule_id_prefixes: Vec<String>,

    /// Prefix used to materialize canonical_iri from a bare canonical_id
    #[arg(long = "canonical-iri-prefix", default_value = "cmdrvl:")]
    pub canonical_iri_prefix: String,

    /// Optional companion dbt schema.yml path; only valid with --format dbt-seed
    #[arg(long = "schema-out")]
    pub schema_out: Option<PathBuf>,

    /// Optional dbt singular test SQL path guarding normalized-key collapse
    #[arg(long = "anti-collapse-test-out")]
    pub anti_collapse_test_out: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct RegistryProvidersCli {
    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct RegistryProviderSchemaCli {
    /// Provider id to describe, such as openfigi or mock
    pub provider: String,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct RegistryNextIdCli {
    /// Canonical ID prefix to allocate under, such as PPL or CPTY
    pub prefix: Option<String>,

    /// Registry directory to inspect
    #[arg(long)]
    pub registry: PathBuf,

    /// Zero-padding width for the numeric suffix
    #[arg(long = "zero-pad")]
    pub zero_pad: Option<usize>,

    /// Output mode
    #[arg(long, value_enum, default_value = "plain")]
    pub emit: RegistryPlainJsonEmitMode,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("version_update")
        .multiple(false)
        .args(["bump", "next_version"])
))]
pub struct RegistryAddEntryCli {
    /// Registry directory to update
    #[arg(long)]
    pub registry: PathBuf,

    /// Existing root-level mapping file to append to
    #[arg(long = "alias-file")]
    pub alias_file: String,

    /// Canonical ID to map the input alias to
    #[arg(long = "canonical-id")]
    pub canonical_id: String,

    /// Exact input alias to write
    #[arg(long)]
    pub input: String,

    /// Rule ID for this authored alias
    #[arg(long = "rule-id")]
    pub rule_id: String,

    /// Canonical type; inferred only for an existing canonical ID with one type
    #[arg(long = "canonical-type")]
    pub canonical_type: Option<String>,

    /// Numeric semver bump to apply; defaults to patch when --next-version is absent
    #[arg(long, value_enum)]
    pub bump: Option<RegistryVersionBumpMode>,

    /// Explicit next registry version for non-numeric or calendar versions
    #[arg(long = "next-version")]
    pub next_version: Option<String>,

    /// Skip standard registry lint before accepting the write
    #[arg(long = "no-lint")]
    pub no_lint: bool,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryPlainJsonEmitMode,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("mint_id")
        .multiple(false)
        .args(["canonical_id", "prefix"])
))]
#[command(group(
    ArgGroup::new("version_update")
        .multiple(false)
        .args(["bump", "next_version"])
))]
pub struct RegistryMintCli {
    /// Registry directory to update
    #[arg(long)]
    pub registry: PathBuf,

    /// Explicit canonical ID; omit to allocate with next-id
    #[arg(long = "canonical-id")]
    pub canonical_id: Option<String>,

    /// Prefix override for allocation when --canonical-id is omitted
    #[arg(long)]
    pub prefix: Option<String>,

    /// Canonical type for the minted entity
    #[arg(long = "canonical-type")]
    pub canonical_type: String,

    /// Alias spec, repeatable: FILE=INPUT:RULE_ID
    #[arg(long = "with-alias")]
    pub with_alias: Vec<String>,

    /// Numeric semver bump to apply; defaults to patch when --next-version is absent
    #[arg(long, value_enum)]
    pub bump: Option<RegistryVersionBumpMode>,

    /// Explicit next registry version for non-numeric or calendar versions
    #[arg(long = "next-version")]
    pub next_version: Option<String>,

    /// Skip standard registry lint before accepting the write
    #[arg(long = "no-lint")]
    pub no_lint: bool,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryPlainJsonEmitMode,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("version_update")
        .multiple(false)
        .args(["bump", "next_version"])
))]
pub struct RegistryDefaultIdSchemeCli {
    /// Registry directory to update
    #[arg(long)]
    pub registry: PathBuf,

    /// Canonical ID prefix to store as the default scheme
    #[arg(long)]
    pub prefix: String,

    /// Zero-padding width for the numeric suffix
    #[arg(long = "zero-pad")]
    pub zero_pad: Option<usize>,

    /// Refuse instead of warning when existing in-namespace IDs are out of scheme
    #[arg(long)]
    pub strict: bool,

    /// Numeric semver bump to apply; defaults to patch when --next-version is absent
    #[arg(long, value_enum)]
    pub bump: Option<RegistryVersionBumpMode>,

    /// Explicit next registry version for non-numeric or calendar versions
    #[arg(long = "next-version")]
    pub next_version: Option<String>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryPlainJsonEmitMode,
}

#[derive(Subcommand, Debug, Clone)]
pub enum EntitySubcommand {
    /// Run the full entity orchestration flow
    Run(EntityRunCli),
    /// Validate and project profile-mapped observations for entity preparation
    Prepare(EntityPrepareCli),
    /// Build artifact-backed entity indexes
    Index(EntityIndexCommand),
    /// Generate candidate neighborhoods
    Block(EntityBlockCli),
    /// Evaluate candidate retrieval recall against sealed public gold labels
    #[command(name = "candidate-recall")]
    CandidateRecall(EntityCandidateRecallCli),
    /// Compile a public alias-withholding execution envelope into a report
    #[command(name = "alias-withholding")]
    AliasWithholding(EntityAliasWithholdingCli),
    /// Compile a strict artifact-backed generalization execution envelope into a report
    Generalization(EntityGeneralizationCli),
    /// Score typed evidence for candidate pairs
    Evidence(EntityEvidenceCli),
    /// Solve entity identity assignments from evidence artifacts
    Solve(EntitySolveCli),
    /// Link two row sets through the artifact-backed entity workbench
    Link(EntityLinkCli),
    /// Audit an entity result artifact against a suite
    Audit(EntityAuditCli),
    /// Promote an audited entity result into the registry and escrow sidecars
    Promote(EntityPromoteCli),
    /// Replay accepted entity assignments onto input rows
    Apply(EntityApplyCli),
    /// Explain one entity row, canonical entity, or escrow entity
    Explain(EntityExplainCli),
    /// List and initialize built-in entity profile templates
    Profile(EntityProfileCommand),
    /// Export and import human adjudication review queues
    Review(EntityReviewCommand),
}

#[derive(Subcommand, Debug, Clone)]
pub enum EntityIndexSubcommand {
    /// Build deterministic index artifacts for a work directory
    Build(EntityIndexBuildCli),
}

#[derive(Subcommand, Debug, Clone)]
pub enum StrategySubcommand {
    /// Derive a deterministic schema profile from CSV, TSV, JSONL, or NDJSON input
    Profile(StrategyProfileCli),
    /// Audit a frozen script against a deterministic fixture suite
    Audit(StrategyAuditCli),
    /// Resolve a schema shape and skill hash to a frozen champion script
    Resolve(StrategyResolveCli),
    /// Register a verified frozen champion script for a schema shape and skill hash
    Register(StrategyRegisterCli),
    /// Update an active strategy champion in place with a version bump
    Update(StrategyUpdateCli),
    /// Deprecate a champion so active resolution ignores it without deleting history
    Deprecate(StrategyDeprecateCli),
    /// Promote an operator-attested champion to proof-attested
    Promote(StrategyPromoteCli),
    /// List strategy champions, provenance, grades, and lifecycle status
    List(StrategyListCli),
    /// Explain active and ignored entries for one strategy key
    Explain(StrategyExplainCli),
    /// Compare two frozen-script strategy registry versions
    Diff(StrategyDiffCli),
}

#[derive(Args, Debug, Clone)]
pub struct RegistryDiffCli {
    /// Older registry directory
    #[arg(long)]
    pub old: PathBuf,

    /// Newer registry directory
    #[arg(long)]
    pub new: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct RegistryAuditCli {
    /// Seed CSV or JSONL file to audit (use '-' for stdin with JSONL)
    pub seed: PathBuf,

    /// Registry directory to audit against
    #[arg(long)]
    pub registry: PathBuf,

    /// Column containing seed identifiers
    #[arg(long)]
    pub column: String,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,

    /// Refuse if input exceeds N data rows
    #[arg(long)]
    pub max_rows: Option<usize>,

    /// Refuse if input exceeds N bytes
    #[arg(long)]
    pub max_bytes: Option<u64>,
}

#[derive(Args, Debug, Clone)]
pub struct RegistryBuildCli {
    /// Provider source name
    #[arg(long)]
    pub source: String,

    /// Seed CSV or JSONL file to materialize from
    #[arg(long)]
    pub seed: PathBuf,

    /// Column containing seed identifiers
    #[arg(long)]
    pub seed_column: String,

    /// Output registry directory
    #[arg(long)]
    pub output: PathBuf,

    /// Registry version to write into registry.json
    #[arg(long)]
    pub version: String,

    /// Carry forward existing registry entries and fetch only new identifiers
    #[arg(long)]
    pub incremental: bool,

    /// Refuse if input exceeds N data rows
    #[arg(long)]
    pub max_rows: Option<usize>,

    /// Refuse if input exceeds N bytes
    #[arg(long)]
    pub max_bytes: Option<u64>,

    /// Override provider batch size
    #[arg(long)]
    pub batch_size: Option<usize>,

    /// Override provider rate limit delay in milliseconds
    #[arg(long)]
    pub rate_limit_ms: Option<u64>,

    /// Provider-specific key=value option (repeatable), e.g. OpenFIGI id_type/base_url/api_key/exchCode
    #[arg(long = "provider-config", value_name = "KEY=VALUE")]
    pub provider_config: Vec<String>,
}

#[derive(Debug, Clone, ValueEnum, Default)]
pub enum RegistryLintProfile {
    /// Infer strategy, org, or standard registry linting from sidecars
    #[default]
    Auto,
    /// Standard exact-match mapping registry
    Standard,
    /// Organization identity registry with aliases and sidecars
    Org,
    /// Frozen-script strategy registry
    Strategy,
}

#[derive(Args, Debug, Clone)]
pub struct RegistryLintCli {
    /// Registry directory to lint
    pub registry: PathBuf,

    /// Registry profile to lint
    #[arg(long, value_enum, default_value = "auto")]
    pub profile: RegistryLintProfile,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct StrategyProfileCli {
    /// CSV, TSV, JSONL, or NDJSON input to profile (use '-' for JSONL stdin)
    pub input: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,

    /// Refuse if input exceeds N data rows
    #[arg(long)]
    pub max_rows: Option<usize>,

    /// Refuse if input exceeds N bytes
    #[arg(long)]
    pub max_bytes: Option<u64>,
}

#[derive(Args, Debug, Clone)]
pub struct StrategyAuditCli {
    /// Schema/profile JSON file describing the input shape
    #[arg(long)]
    pub schema: PathBuf,

    /// Frozen script executable to audit
    #[arg(long)]
    pub script: PathBuf,

    /// Deterministic fixture suite directory
    #[arg(long)]
    pub suite: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("skill_identity")
        .required(true)
        .multiple(false)
        .args(["skill", "skill_hash"])
))]
#[command(group(
    ArgGroup::new("strategy_key")
        .required(true)
        .multiple(false)
        .args(["schema", "task"])
))]
pub struct StrategyResolveCli {
    /// Strategy registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Schema/profile JSON file describing the input shape
    #[arg(long)]
    pub schema: Option<PathBuf>,

    /// Exact task/intent key to resolve
    #[arg(long)]
    pub task: Option<String>,

    /// Skill file whose bytes define the authoring context
    #[arg(long)]
    pub skill: Option<PathBuf>,

    /// Precomputed BLAKE3 skill hash
    #[arg(long = "skill-hash")]
    pub skill_hash: Option<String>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("skill_identity")
        .required(true)
        .multiple(false)
        .args(["skill", "skill_hash"])
))]
#[command(group(
    ArgGroup::new("strategy_key")
        .required(true)
        .multiple(false)
        .args(["schema", "task"])
))]
pub struct StrategyRegisterCli {
    /// Strategy registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Schema/profile JSON file describing the input shape
    #[arg(long)]
    pub schema: Option<PathBuf>,

    /// Exact task/intent key to register
    #[arg(long)]
    pub task: Option<String>,

    /// Skill file whose bytes define the authoring context
    #[arg(long)]
    pub skill: Option<PathBuf>,

    /// Precomputed BLAKE3 skill hash
    #[arg(long = "skill-hash")]
    pub skill_hash: Option<String>,

    /// Frozen script file that passed verify, assess, and airlock
    #[arg(long)]
    pub script: PathBuf,

    /// Stable script identifier to store in the registry
    #[arg(long = "script-id")]
    pub script_id: String,

    /// Script language/runtime label
    #[arg(long)]
    pub language: String,

    /// Attestation grade to record
    #[arg(long, value_enum, default_value = "proof-attested")]
    pub grade: StrategyGradeArg,

    /// Operator identity for operator-attested entries and lifecycle receipts
    #[arg(long)]
    pub operator: Option<String>,

    /// Single-line operator reason for operator-attested entries and lifecycle receipts
    #[arg(long)]
    pub reason: Option<String>,

    /// Explicit RFC3339 timestamp for deterministic tests and reproducible receipts
    #[arg(long = "attested-at")]
    pub attested_at: Option<String>,

    /// Verify artifact proving the script passed verification
    #[arg(long)]
    pub verify: Option<PathBuf>,

    /// Assess artifact proving the script should proceed
    #[arg(long)]
    pub assess: Option<PathBuf>,

    /// Airlock artifact proving the script cleared airlock
    #[arg(long)]
    pub airlock: Option<PathBuf>,

    /// Explicit next registry version
    #[arg(long = "next-version")]
    pub next_version: String,

    /// Rule identifier for this registration
    #[arg(long = "rule-id")]
    pub rule_id: Option<String>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,

    /// Suppress witness ledger append for this registry mutation
    #[arg(long)]
    pub no_witness: bool,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("skill_identity")
        .required(true)
        .multiple(false)
        .args(["skill", "skill_hash"])
))]
#[command(group(
    ArgGroup::new("strategy_key")
        .required(true)
        .multiple(false)
        .args(["schema", "task"])
))]
pub struct StrategyUpdateCli {
    /// Strategy registry directory
    #[arg(long)]
    pub registry: PathBuf,
    /// Schema/profile JSON file describing the input shape
    #[arg(long)]
    pub schema: Option<PathBuf>,
    /// Exact task/intent key to update
    #[arg(long)]
    pub task: Option<String>,
    /// Skill file whose bytes define the authoring context
    #[arg(long)]
    pub skill: Option<PathBuf>,
    /// Precomputed BLAKE3 skill hash
    #[arg(long = "skill-hash")]
    pub skill_hash: Option<String>,
    /// Replacement frozen script file
    #[arg(long)]
    pub script: PathBuf,
    /// Stable script identifier to store in the registry
    #[arg(long = "script-id")]
    pub script_id: String,
    /// Script language/runtime label
    #[arg(long)]
    pub language: String,
    /// Operator identity for the update attestation
    #[arg(long)]
    pub operator: Option<String>,
    /// Single-line update reason
    #[arg(long)]
    pub reason: Option<String>,
    /// Explicit RFC3339 timestamp for deterministic tests
    #[arg(long = "attested-at")]
    pub attested_at: Option<String>,
    /// Verify artifact for updating proof-attested entries
    #[arg(long)]
    pub verify: Option<PathBuf>,
    /// Assess artifact for updating proof-attested entries
    #[arg(long)]
    pub assess: Option<PathBuf>,
    /// Airlock artifact for updating proof-attested entries
    #[arg(long)]
    pub airlock: Option<PathBuf>,
    /// Explicit next registry version
    #[arg(long = "next-version")]
    pub next_version: String,
    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
    /// Suppress witness ledger append for this registry mutation
    #[arg(long)]
    pub no_witness: bool,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("skill_identity")
        .required(true)
        .multiple(false)
        .args(["skill", "skill_hash"])
))]
#[command(group(
    ArgGroup::new("strategy_key")
        .required(true)
        .multiple(false)
        .args(["schema", "task"])
))]
pub struct StrategyDeprecateCli {
    #[arg(long)]
    pub registry: PathBuf,
    #[arg(long)]
    pub schema: Option<PathBuf>,
    #[arg(long)]
    pub task: Option<String>,
    #[arg(long)]
    pub skill: Option<PathBuf>,
    #[arg(long = "skill-hash")]
    pub skill_hash: Option<String>,
    #[arg(long)]
    pub operator: String,
    #[arg(long)]
    pub reason: String,
    #[arg(long = "attested-at")]
    pub attested_at: Option<String>,
    #[arg(long = "next-version")]
    pub next_version: String,
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
    #[arg(long)]
    pub no_witness: bool,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("skill_identity")
        .required(true)
        .multiple(false)
        .args(["skill", "skill_hash"])
))]
#[command(group(
    ArgGroup::new("strategy_key")
        .required(true)
        .multiple(false)
        .args(["schema", "task"])
))]
pub struct StrategyPromoteCli {
    #[arg(long)]
    pub registry: PathBuf,
    #[arg(long)]
    pub schema: Option<PathBuf>,
    #[arg(long)]
    pub task: Option<String>,
    #[arg(long)]
    pub skill: Option<PathBuf>,
    #[arg(long = "skill-hash")]
    pub skill_hash: Option<String>,
    #[arg(long)]
    pub verify: PathBuf,
    #[arg(long)]
    pub assess: PathBuf,
    #[arg(long)]
    pub airlock: PathBuf,
    #[arg(long = "next-version")]
    pub next_version: String,
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
    #[arg(long)]
    pub no_witness: bool,
}

#[derive(Args, Debug, Clone)]
pub struct StrategyListCli {
    #[arg(long)]
    pub registry: PathBuf,
    #[arg(long = "key-type", value_enum)]
    pub key_type: Option<StrategyKeyTypeArg>,
    #[arg(long, value_enum)]
    pub grade: Option<StrategyGradeArg>,
    #[arg(long, value_enum)]
    pub status: Option<StrategyStatusArg>,
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("skill_identity")
        .required(true)
        .multiple(false)
        .args(["skill", "skill_hash"])
))]
#[command(group(
    ArgGroup::new("strategy_key")
        .required(true)
        .multiple(false)
        .args(["schema", "task"])
))]
pub struct StrategyExplainCli {
    #[arg(long)]
    pub registry: PathBuf,
    #[arg(long)]
    pub schema: Option<PathBuf>,
    #[arg(long)]
    pub task: Option<String>,
    #[arg(long)]
    pub skill: Option<PathBuf>,
    #[arg(long = "skill-hash")]
    pub skill_hash: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct StrategyDiffCli {
    /// Older strategy registry directory
    #[arg(long)]
    pub old: PathBuf,

    /// Newer strategy registry directory
    #[arg(long)]
    pub new: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntityRunCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Entity profile id or YAML path for artifact-backed runs
    #[arg(long)]
    pub profile: Option<String>,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Work directory for artifact-backed runs
    #[arg(long = "work-dir")]
    pub work_dir: Option<PathBuf>,

    /// Entity index cache mode
    #[arg(long = "cache-mode", value_enum, default_value = "enabled")]
    pub cache_mode: EntityCacheModeArg,

    /// Frozen evaluation suite directory
    #[arg(long)]
    pub suite: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,

    /// Suppress witness ledger append
    #[arg(long)]
    pub no_witness: bool,
}

#[derive(Args, Debug, Clone)]
pub struct EntityPrepareCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Entity profile id or YAML path
    #[arg(long)]
    pub profile: String,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Work directory for prepare artifacts
    #[arg(long = "work-dir")]
    pub work_dir: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct EntityIndexBuildCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Entity profile id or YAML path
    #[arg(long)]
    pub profile: Option<String>,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Work directory for artifact-backed stages
    #[arg(long = "work-dir")]
    pub work_dir: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntityBlockCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Entity profile id or YAML path for artifact-backed dispatch
    #[arg(long)]
    pub profile: Option<String>,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Work directory for artifact-backed stages
    #[arg(long = "work-dir")]
    pub work_dir: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "jsonl")]
    pub emit: EntityStreamEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntityCandidateRecallCli {
    /// Public quality manifest containing sealed must-link labels keyed by observation IDs
    #[arg(long)]
    pub manifest: PathBuf,

    /// JSON array or native JSONL of block candidate records
    #[arg(long)]
    pub candidates: PathBuf,

    /// JSON block candidate generation diagnostics
    #[arg(long)]
    pub diagnostics: PathBuf,

    /// Compact exact buckets emitted alongside candidates
    #[arg(long = "exact-bucket-count")]
    pub exact_bucket_count: u64,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntityAliasWithholdingCli {
    /// Alias-withholding execution envelope JSON
    #[arg(long)]
    pub manifest: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntityGeneralizationCli {
    /// Strict generalization execution envelope JSON
    #[arg(long)]
    pub manifest: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntityEvidenceCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Entity profile id or YAML path for artifact-backed dispatch
    #[arg(long)]
    pub profile: Option<String>,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Candidate block artifact
    #[arg(long)]
    pub candidates: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Work directory for artifact-backed stages
    #[arg(long = "work-dir")]
    pub work_dir: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "jsonl")]
    pub emit: EntityStreamEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntitySolveCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Entity profile id or YAML path for artifact-backed dispatch
    #[arg(long)]
    pub profile: Option<String>,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Evidence artifact
    #[arg(long)]
    pub evidence: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Work directory for artifact-backed stages
    #[arg(long = "work-dir")]
    pub work_dir: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntityLinkCli {
    /// Reference rows
    pub reference: PathBuf,

    /// Target rows
    pub target: PathBuf,

    /// Entity profile id or YAML path; required for successful execution
    #[arg(long)]
    pub profile: Option<String>,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Gold cross-reference JSONL file for link scoring
    #[arg(long)]
    pub gold: Option<PathBuf>,

    /// Write matched ID pairs back into a new registry mapping file
    #[arg(long = "write-back")]
    pub write_back: bool,

    /// Refuse if a target record has more than N surviving candidates
    #[arg(long)]
    pub max_candidates: Option<usize>,

    /// Refuse if either input exceeds N data rows
    #[arg(long)]
    pub max_rows: Option<usize>,

    /// Refuse if either input exceeds N bytes
    #[arg(long)]
    pub max_bytes: Option<u64>,

    /// Work directory for artifact-backed stages; required for successful execution
    #[arg(long = "work-dir")]
    pub work_dir: Option<PathBuf>,

    /// Entity index cache mode
    #[arg(long = "cache-mode", value_enum, default_value = "enabled")]
    pub cache_mode: EntityCacheModeArg,

    /// Run a frozen audit suite and write audit.json under --work-dir
    #[arg(long)]
    pub suite: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,

    /// Suppress witness ledger append
    #[arg(long)]
    pub no_witness: bool,
}

#[derive(Args, Debug, Clone)]
pub struct EntityAuditCli {
    /// Entity solve or run artifact
    pub result: PathBuf,

    /// Frozen evaluation suite directory
    #[arg(long)]
    pub suite: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntityPromoteCli {
    /// Entity solve or run artifact
    pub result: PathBuf,

    /// Audit artifact
    #[arg(long)]
    pub audit: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Explicit next registry version
    #[arg(long)]
    pub next_version: String,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("apply_resolution_policy")
        .args(["require_full_resolution", "allow_partial_output"])
        .multiple(false)
))]
pub struct EntityApplyCli {
    /// Entity solve or run artifact
    pub result: PathBuf,

    /// Input CSV or JSONL rows to replay
    #[arg(long)]
    pub rows: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Input column to exact-match against the promoted registry
    #[arg(long)]
    pub column: Option<String>,

    /// Output path for canonicalized rows
    #[arg(long, alias = "output")]
    pub out: Option<PathBuf>,

    /// Work directory for artifact-backed stages
    #[arg(long = "work-dir")]
    pub work_dir: Option<PathBuf>,

    /// Refuse before writing output if any row remains unresolved (default)
    #[arg(long = "require-full-resolution")]
    pub require_full_resolution: bool,

    /// Permit partial replay output; exits 1 when unresolved rows remain
    #[arg(long = "allow-partial-output")]
    pub allow_partial_output: bool,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,
}

#[derive(Subcommand, Debug, Clone)]
pub enum EntityProfileSubcommand {
    /// List built-in entity profile templates
    List(EntityProfileListCli),
    /// Write a built-in entity profile template to disk
    Init(EntityProfileInitCli),
}

#[derive(Args, Debug, Clone)]
pub struct EntityProfileListCli {
    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntityProfileInitCli {
    /// Built-in profile id
    pub profile: String,

    /// Output YAML path to write
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Subcommand, Debug, Clone)]
pub enum EntityReviewSubcommand {
    /// Export reviewable entity clusters from a solve/run artifact
    Export(EntityReviewExportCli),
    /// Import adjudicated review decisions into a registry version
    Import(EntityReviewImportCli),
}

#[derive(Args, Debug, Clone)]
pub struct EntityReviewExportCli {
    /// Entity solve or run artifact
    pub result: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityReviewExportEmitMode,

    /// Review artifact contract to emit
    #[arg(long, value_enum, default_value = "queue")]
    pub artifact: EntityReviewExportArtifact,

    /// Which reviewable records to include
    #[arg(long, value_enum, default_value = "all")]
    pub include: EntityReviewInclude,
}

#[derive(Args, Debug, Clone)]
pub struct EntityReviewImportCli {
    /// Review JSON or CSV artifact
    pub review: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Explicit next registry version
    #[arg(long = "next-version")]
    pub next_version: String,

    /// Audit artifact required for alias/anchor promotion decisions
    #[arg(long)]
    pub audit: Option<PathBuf>,

    /// Source canon_entity_native_review.v0 artifact for native decision import
    #[arg(long = "source-review")]
    pub source_review: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("query")
        .required(true)
        .multiple(false)
        .args(["row", "surface_id", "canon_id", "escrow_id"])
))]
pub struct EntityExplainCli {
    /// Entity solve or run artifact
    pub result: PathBuf,

    /// Explain a source row by source_row_id
    #[arg(long)]
    pub row: Option<String>,

    /// Explain a prepared surface by surface_id
    #[arg(long = "surface-id")]
    pub surface_id: Option<String>,

    /// Explain a resolved entity by canonical ID
    #[arg(long)]
    pub canon_id: Option<String>,

    /// Explain an escrow entity by escrow ID
    #[arg(long)]
    pub escrow_id: Option<String>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,
}

#[derive(Parser, Debug)]
#[command(name = "canon")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Resolve messy identifiers to canonical IDs using versioned registries")]
#[command(disable_version_flag = true)]
#[command(subcommand_precedence_over_arg = true)]
#[command(subcommand_negates_reqs = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<CanonCommand>,

    /// Input CSV or JSONL file (use '-' for stdin with JSONL)
    pub input: Option<PathBuf>,

    /// Registry directory (versioned)
    #[arg(long, required_unless_present_any = ["version", "describe", "schema"])]
    pub registry: Option<PathBuf>,

    /// Column containing IDs to resolve
    #[arg(long, required_unless_present_any = ["version", "describe", "schema"])]
    pub column: Option<String>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EmitMode,

    /// Name of appended canonical column (CSV mode only)
    #[arg(long)]
    pub canon_column: Option<String>,

    /// Write JSON mapping artifact to file (CSV mode only)
    #[arg(long)]
    pub map_out: Option<PathBuf>,

    /// Refuse if input exceeds N data rows
    #[arg(long)]
    pub max_rows: Option<usize>,

    /// Refuse if input exceeds N bytes
    #[arg(long)]
    pub max_bytes: Option<u64>,

    /// Suppress witness ledger append
    #[arg(long)]
    pub no_witness: bool,

    /// Show entity names and identifiers in JSON output (default: redacted for zero-retention safety)
    #[arg(long)]
    pub explicit: bool,

    /// With --explicit JSON output, emit UTF-8 values without the u8: prefix and include encoding metadata
    #[arg(long = "plain-json-values", requires = "explicit")]
    pub plain_json_values: bool,

    /// Print version and exit
    #[arg(long)]
    pub version: bool,

    /// Emit operator.json to stdout and exit
    #[arg(long)]
    pub describe: bool,

    /// Print JSON Schema for mapping artifact and exit
    #[arg(long)]
    pub schema: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn registry_command(cli: Cli) -> Option<RegistryCommand> {
        let command = cli.command;
        assert!(
            matches!(&command, Some(CanonCommand::Registry(_))),
            "expected registry command"
        );
        match command {
            Some(CanonCommand::Registry(command)) => Some(command),
            _ => None,
        }
    }

    fn strategy_command(cli: Cli) -> Option<StrategyCommand> {
        let command = cli.command;
        assert!(
            matches!(&command, Some(CanonCommand::Strategy(_))),
            "expected strategy command"
        );
        match command {
            Some(CanonCommand::Strategy(command)) => Some(command),
            _ => None,
        }
    }

    fn entity_command(cli: Cli) -> Option<EntityCommand> {
        let command = cli.command;
        assert!(
            matches!(&command, Some(CanonCommand::Entity(_))),
            "expected entity command"
        );
        match command {
            Some(CanonCommand::Entity(command)) => Some(command),
            _ => None,
        }
    }

    #[test]
    fn test_emit_mode_default() {
        assert!(matches!(EmitMode::default(), EmitMode::Json));
    }

    #[test]
    fn test_entity_emit_mode_defaults() {
        assert!(matches!(EntityEmitMode::default(), EntityEmitMode::Json));
        assert!(matches!(
            EntityStreamEmitMode::default(),
            EntityStreamEmitMode::Jsonl
        ));
    }

    #[test]
    fn test_cli_info_commands() {
        // Test version flag
        let args = ["canon", "--version"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.version);

        // Test describe flag
        let args = ["canon", "--describe"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.describe);

        // Test schema flag
        let args = ["canon", "--schema"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.schema);
    }

    #[test]
    fn test_cli_basic_parsing() {
        let args = [
            "canon",
            "input.csv",
            "--registry",
            "registries/test",
            "--column",
            "id",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.input, Some(PathBuf::from("input.csv")));
        assert_eq!(cli.registry, Some(PathBuf::from("registries/test")));
        assert_eq!(cli.column, Some("id".to_string()));
        assert!(matches!(cli.emit, EmitMode::Json));
        assert!(!cli.plain_json_values);
    }

    #[test]
    fn test_cli_plain_json_values_parsing() {
        let args = [
            "canon",
            "input.csv",
            "--registry",
            "registries/test",
            "--column",
            "id",
            "--explicit",
            "--plain-json-values",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        assert!(cli.explicit);
        assert!(cli.plain_json_values);
    }

    #[test]
    fn test_cli_doctor_parsing() {
        let args = ["canon", "doctor", "capabilities", "--json"];
        let cli = Cli::try_parse_from(args).unwrap();

        let command = cli.command;
        assert!(matches!(&command, Some(CanonCommand::Doctor(_))));
        if let Some(CanonCommand::Doctor(doctor)) = command {
            assert!(!doctor.robot_triage);
            assert!(!doctor.json);
            assert!(matches!(
                doctor.command,
                Some(DoctorCommand::Capabilities(DoctorJsonArgs { json: true }))
            ));
        }

        let args = ["canon", "doctor", "--robot-triage"];
        let cli = Cli::try_parse_from(args).unwrap();
        let command = cli.command;
        assert!(matches!(&command, Some(CanonCommand::Doctor(_))));
        if let Some(CanonCommand::Doctor(doctor)) = command {
            assert!(doctor.robot_triage);
            assert!(doctor.command.is_none());
        }
    }

    #[test]
    fn test_cli_registry_diff_parsing() {
        let args = [
            "canon",
            "registry",
            "diff",
            "--old",
            "registries/test-v1",
            "--new",
            "registries/test-v2",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = registry_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, RegistrySubcommand::Diff(_)));
        if let RegistrySubcommand::Diff(diff) = subcommand {
            assert_eq!(diff.old, PathBuf::from("registries/test-v1"));
            assert_eq!(diff.new, PathBuf::from("registries/test-v2"));
            assert!(matches!(diff.emit, RegistryEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_registry_next_id_parsing() {
        let args = [
            "canon",
            "registry",
            "next-id",
            "PPL",
            "--registry",
            "registries/people",
            "--zero-pad",
            "5",
            "--emit",
            "json",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = registry_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, RegistrySubcommand::NextId(_)));
        if let RegistrySubcommand::NextId(next_id) = subcommand {
            assert_eq!(next_id.prefix.as_deref(), Some("PPL"));
            assert_eq!(next_id.registry, PathBuf::from("registries/people"));
            assert_eq!(next_id.zero_pad, Some(5));
            assert!(matches!(next_id.emit, RegistryPlainJsonEmitMode::Json));
        }
    }

    #[test]
    fn test_cli_registry_add_entry_parsing() {
        let args = [
            "canon",
            "registry",
            "add-entry",
            "--registry",
            "registries/people",
            "--alias-file",
            "aliases.json",
            "--canonical-id",
            "PPL-001",
            "--input",
            "Jane Doe",
            "--rule-id",
            "MANUAL",
            "--canonical-type",
            "person",
            "--bump",
            "minor",
            "--emit",
            "plain",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = registry_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, RegistrySubcommand::AddEntry(_)));
        if let RegistrySubcommand::AddEntry(add_entry) = subcommand {
            assert_eq!(add_entry.registry, PathBuf::from("registries/people"));
            assert_eq!(add_entry.alias_file, "aliases.json");
            assert_eq!(add_entry.canonical_id, "PPL-001");
            assert_eq!(add_entry.input, "Jane Doe");
            assert_eq!(add_entry.rule_id, "MANUAL");
            assert_eq!(add_entry.canonical_type.as_deref(), Some("person"));
            assert_eq!(add_entry.bump, Some(RegistryVersionBumpMode::Minor));
            assert!(add_entry.next_version.is_none());
            assert!(!add_entry.no_lint);
            assert!(matches!(add_entry.emit, RegistryPlainJsonEmitMode::Plain));
        }
    }

    #[test]
    fn test_cli_registry_mint_parsing() {
        let args = [
            "canon",
            "registry",
            "mint",
            "--registry",
            "registries/people",
            "--prefix",
            "PPL",
            "--canonical-type",
            "person",
            "--with-alias",
            "aliases.json=Jane Doe:MANUAL",
            "--with-alias",
            "aliases.json=J. Doe:ALIAS",
            "--bump",
            "minor",
            "--emit",
            "plain",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = registry_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, RegistrySubcommand::Mint(_)));
        if let RegistrySubcommand::Mint(mint) = subcommand {
            assert_eq!(mint.registry, PathBuf::from("registries/people"));
            assert_eq!(mint.canonical_id, None);
            assert_eq!(mint.prefix.as_deref(), Some("PPL"));
            assert_eq!(mint.canonical_type, "person");
            assert_eq!(mint.with_alias.len(), 2);
            assert_eq!(mint.bump, Some(RegistryVersionBumpMode::Minor));
            assert!(mint.next_version.is_none());
            assert!(!mint.no_lint);
            assert!(matches!(mint.emit, RegistryPlainJsonEmitMode::Plain));
        }
    }

    #[test]
    fn test_cli_registry_default_id_scheme_parsing() {
        let args = [
            "canon",
            "registry",
            "default-id-scheme",
            "--registry",
            "registries/people",
            "--prefix",
            "PPL",
            "--zero-pad",
            "5",
            "--strict",
            "--bump",
            "major",
            "--emit",
            "plain",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = registry_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(
            &subcommand,
            RegistrySubcommand::DefaultIdScheme(_)
        ));
        if let RegistrySubcommand::DefaultIdScheme(id_scheme) = subcommand {
            assert_eq!(id_scheme.registry, PathBuf::from("registries/people"));
            assert_eq!(id_scheme.prefix, "PPL");
            assert_eq!(id_scheme.zero_pad, Some(5));
            assert!(id_scheme.strict);
            assert_eq!(id_scheme.bump, Some(RegistryVersionBumpMode::Major));
            assert!(id_scheme.next_version.is_none());
            assert!(matches!(id_scheme.emit, RegistryPlainJsonEmitMode::Plain));
        }
    }

    #[test]
    fn test_cli_registry_audit_parsing() {
        let args = [
            "canon",
            "registry",
            "audit",
            "seeds.csv",
            "--registry",
            "registries/test",
            "--column",
            "cusip",
            "--emit",
            "summary",
            "--max-rows",
            "10",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = registry_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, RegistrySubcommand::Audit(_)));
        if let RegistrySubcommand::Audit(audit) = subcommand {
            assert_eq!(audit.seed, PathBuf::from("seeds.csv"));
            assert_eq!(audit.registry, PathBuf::from("registries/test"));
            assert_eq!(audit.column, "cusip");
            assert!(matches!(audit.emit, RegistryEmitMode::Summary));
            assert_eq!(audit.max_rows, Some(10));
        }
    }

    #[test]
    fn test_cli_registry_build_parsing() {
        let args = [
            "canon",
            "registry",
            "build",
            "--source",
            "mock",
            "--seed",
            "seeds.csv",
            "--seed-column",
            "cusip",
            "--output",
            "registries/mock-cusip",
            "--version",
            "2026.03.13",
            "--incremental",
            "--batch-size",
            "25",
            "--rate-limit-ms",
            "100",
            "--provider-config",
            "id_type=ID_CUSIP",
            "--provider-config",
            "base_url=http://127.0.0.1:8080/v3/mapping",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = registry_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, RegistrySubcommand::Build(_)));
        if let RegistrySubcommand::Build(build) = subcommand {
            assert_eq!(build.source, "mock");
            assert_eq!(build.seed, PathBuf::from("seeds.csv"));
            assert_eq!(build.seed_column, "cusip");
            assert_eq!(build.output, PathBuf::from("registries/mock-cusip"));
            assert_eq!(build.version, "2026.03.13");
            assert!(build.incremental);
            assert_eq!(build.batch_size, Some(25));
            assert_eq!(build.rate_limit_ms, Some(100));
            assert_eq!(
                build.provider_config,
                vec![
                    "id_type=ID_CUSIP".to_string(),
                    "base_url=http://127.0.0.1:8080/v3/mapping".to_string(),
                ]
            );
        }
    }

    #[test]
    fn test_cli_registry_lint_parsing() {
        let args = [
            "canon",
            "registry",
            "lint",
            "registries/org",
            "--profile",
            "org",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = registry_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, RegistrySubcommand::Lint(_)));
        if let RegistrySubcommand::Lint(lint) = subcommand {
            assert_eq!(lint.registry, PathBuf::from("registries/org"));
            assert!(matches!(lint.profile, RegistryLintProfile::Org));
            assert!(matches!(lint.emit, RegistryEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_strategy_resolve_parsing() {
        let args = [
            "canon",
            "strategy",
            "resolve",
            "--registry",
            "registries/strategies",
            "--schema",
            "profile.json",
            "--skill",
            "SKILL.md",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = strategy_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, StrategySubcommand::Resolve(_)));
        if let StrategySubcommand::Resolve(resolve) = subcommand {
            assert_eq!(resolve.registry, PathBuf::from("registries/strategies"));
            assert_eq!(resolve.schema, Some(PathBuf::from("profile.json")));
            assert_eq!(resolve.task, None);
            assert_eq!(resolve.skill, Some(PathBuf::from("SKILL.md")));
            assert_eq!(resolve.skill_hash, None);
            assert!(matches!(resolve.emit, RegistryEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_strategy_profile_parsing() {
        let args = [
            "canon",
            "strategy",
            "profile",
            "rows.ndjson",
            "--emit",
            "summary",
            "--max-rows",
            "100",
            "--max-bytes",
            "4096",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = strategy_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, StrategySubcommand::Profile(_)));
        if let StrategySubcommand::Profile(profile) = subcommand {
            assert_eq!(profile.input, PathBuf::from("rows.ndjson"));
            assert!(matches!(profile.emit, RegistryEmitMode::Summary));
            assert_eq!(profile.max_rows, Some(100));
            assert_eq!(profile.max_bytes, Some(4096));
        }
    }

    #[test]
    fn test_cli_strategy_audit_parsing() {
        let args = [
            "canon",
            "strategy",
            "audit",
            "--schema",
            "profile.json",
            "--script",
            "script.sh",
            "--suite",
            "suite",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = strategy_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, StrategySubcommand::Audit(_)));
        if let StrategySubcommand::Audit(audit) = subcommand {
            assert_eq!(audit.schema, PathBuf::from("profile.json"));
            assert_eq!(audit.script, PathBuf::from("script.sh"));
            assert_eq!(audit.suite, PathBuf::from("suite"));
            assert!(matches!(audit.emit, RegistryEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_strategy_register_parsing() {
        let args = [
            "canon",
            "strategy",
            "register",
            "--registry",
            "registries/strategies",
            "--schema",
            "profile.json",
            "--skill-hash",
            "blake3:abc",
            "--script",
            "script.py",
            "--script-id",
            "procurement-total.v1",
            "--language",
            "python",
            "--verify",
            "verify.json",
            "--assess",
            "assess.json",
            "--airlock",
            "airlock.json",
            "--next-version",
            "0.2.0",
            "--rule-id",
            "PROCUREMENT_TOTAL",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = strategy_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, StrategySubcommand::Register(_)));
        if let StrategySubcommand::Register(register) = subcommand {
            assert_eq!(register.registry, PathBuf::from("registries/strategies"));
            assert_eq!(register.schema, Some(PathBuf::from("profile.json")));
            assert_eq!(register.task, None);
            assert_eq!(register.skill, None);
            assert_eq!(register.skill_hash.as_deref(), Some("blake3:abc"));
            assert_eq!(register.script, PathBuf::from("script.py"));
            assert_eq!(register.script_id, "procurement-total.v1");
            assert_eq!(register.language, "python");
            assert_eq!(register.verify, Some(PathBuf::from("verify.json")));
            assert_eq!(register.assess, Some(PathBuf::from("assess.json")));
            assert_eq!(register.airlock, Some(PathBuf::from("airlock.json")));
            assert_eq!(register.next_version, "0.2.0");
            assert_eq!(register.rule_id.as_deref(), Some("PROCUREMENT_TOTAL"));
        }
    }

    #[test]
    fn test_cli_strategy_diff_parsing() {
        let args = [
            "canon",
            "strategy",
            "diff",
            "--old",
            "registries/strategies-v1",
            "--new",
            "registries/strategies-v2",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = strategy_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, StrategySubcommand::Diff(_)));
        if let StrategySubcommand::Diff(diff) = subcommand {
            assert_eq!(diff.old, PathBuf::from("registries/strategies-v1"));
            assert_eq!(diff.new, PathBuf::from("registries/strategies-v2"));
            assert!(matches!(diff.emit, RegistryEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_entity_run_parsing() {
        let args = [
            "canon",
            "entity",
            "run",
            "rows.csv",
            "--strategy",
            "strategy.yaml",
            "--registry",
            "registries/entity",
            "--profile",
            "entity_profile",
            "--work-dir",
            "work/entity",
            "--cache-mode",
            "disabled",
            "--suite",
            "suite",
            "--emit",
            "summary",
            "--no-witness",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Run(_)));
        if let EntitySubcommand::Run(run) = subcommand {
            assert_eq!(run.rows, PathBuf::from("rows.csv"));
            assert_eq!(run.profile.as_deref(), Some("entity_profile"));
            assert_eq!(run.strategy, PathBuf::from("strategy.yaml"));
            assert_eq!(run.registry, PathBuf::from("registries/entity"));
            assert_eq!(run.work_dir, Some(PathBuf::from("work/entity")));
            assert!(matches!(run.cache_mode, EntityCacheModeArg::Disabled));
            assert_eq!(run.suite, Some(PathBuf::from("suite")));
            assert!(matches!(run.emit, EntityEmitMode::Summary));
            assert!(run.no_witness);
        }
    }

    #[test]
    fn test_cli_entity_run_cache_mode_defaults_to_enabled() {
        let args = [
            "canon",
            "entity",
            "run",
            "rows.csv",
            "--strategy",
            "strategy.yaml",
            "--registry",
            "registries/entity",
            "--profile",
            "entity_profile",
            "--work-dir",
            "work/entity",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let EntitySubcommand::Run(run) = command.command else {
            panic!("expected entity run command");
        };
        assert!(matches!(run.cache_mode, EntityCacheModeArg::Enabled));
    }

    #[test]
    fn test_cli_entity_cache_mode_rejects_unknown_value() {
        let args = [
            "canon",
            "entity",
            "run",
            "rows.csv",
            "--strategy",
            "strategy.yaml",
            "--registry",
            "registries/entity",
            "--profile",
            "entity_profile",
            "--work-dir",
            "work/entity",
            "--cache-mode",
            "auto",
        ];
        assert!(Cli::try_parse_from(args).is_err());
    }

    #[test]
    fn test_cli_entity_prepare_parsing() {
        let args = [
            "canon",
            "entity",
            "prepare",
            "rows.csv",
            "--profile",
            "cmbs_tenant_label",
            "--registry",
            "registries/cmbs-tenants",
            "--work-dir",
            "work/entity",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Prepare(_)));
        if let EntitySubcommand::Prepare(prepare) = subcommand {
            assert_eq!(prepare.rows, PathBuf::from("rows.csv"));
            assert_eq!(prepare.profile, "cmbs_tenant_label");
            assert_eq!(prepare.registry, PathBuf::from("registries/cmbs-tenants"));
            assert_eq!(prepare.work_dir, PathBuf::from("work/entity"));
        }
    }

    #[test]
    fn test_cli_entity_index_build_parsing() {
        let args = [
            "canon",
            "entity",
            "index",
            "build",
            "rows.csv",
            "--profile",
            "entity_profile",
            "--strategy",
            "strategy.yaml",
            "--registry",
            "registries/entity",
            "--work-dir",
            "work/entity",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Index(_)));
        if let EntitySubcommand::Index(index) = subcommand {
            let EntityIndexSubcommand::Build(build) = index.command;
            assert_eq!(build.rows, PathBuf::from("rows.csv"));
            assert_eq!(build.profile.as_deref(), Some("entity_profile"));
            assert_eq!(build.strategy, PathBuf::from("strategy.yaml"));
            assert_eq!(build.registry, PathBuf::from("registries/entity"));
            assert_eq!(build.work_dir, Some(PathBuf::from("work/entity")));
            assert!(matches!(build.emit, EntityEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_entity_block_parsing() {
        let args = [
            "canon",
            "entity",
            "block",
            "rows.csv",
            "--profile",
            "entity_profile",
            "--strategy",
            "strategy.yaml",
            "--registry",
            "registries/entity",
            "--work-dir",
            "work/entity",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Block(_)));
        if let EntitySubcommand::Block(block) = subcommand {
            assert_eq!(block.rows, PathBuf::from("rows.csv"));
            assert_eq!(block.profile.as_deref(), Some("entity_profile"));
            assert_eq!(block.strategy, PathBuf::from("strategy.yaml"));
            assert_eq!(block.registry, PathBuf::from("registries/entity"));
            assert_eq!(block.work_dir, Some(PathBuf::from("work/entity")));
            assert!(matches!(block.emit, EntityStreamEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_entity_generalization_parsing() {
        let args = [
            "canon",
            "entity",
            "generalization",
            "--manifest",
            "strict-envelope.json",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Generalization(_)));
        if let EntitySubcommand::Generalization(generalization) = subcommand {
            assert_eq!(
                generalization.manifest,
                PathBuf::from("strict-envelope.json")
            );
            assert!(matches!(generalization.emit, EntityEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_entity_evidence_parsing() {
        let args = [
            "canon",
            "entity",
            "evidence",
            "rows.csv",
            "--profile",
            "entity_profile",
            "--strategy",
            "strategy.yaml",
            "--candidates",
            "block.jsonl",
            "--registry",
            "registries/entity",
            "--work-dir",
            "work/entity",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Evidence(_)));
        if let EntitySubcommand::Evidence(evidence) = subcommand {
            assert_eq!(evidence.rows, PathBuf::from("rows.csv"));
            assert_eq!(evidence.profile.as_deref(), Some("entity_profile"));
            assert_eq!(evidence.strategy, PathBuf::from("strategy.yaml"));
            assert_eq!(evidence.candidates, PathBuf::from("block.jsonl"));
            assert_eq!(evidence.registry, PathBuf::from("registries/entity"));
            assert_eq!(evidence.work_dir, Some(PathBuf::from("work/entity")));
            assert!(matches!(evidence.emit, EntityStreamEmitMode::Jsonl));
        }
    }

    #[test]
    fn test_cli_entity_solve_parsing() {
        let args = [
            "canon",
            "entity",
            "solve",
            "rows.csv",
            "--profile",
            "entity_profile",
            "--strategy",
            "strategy.yaml",
            "--evidence",
            "evidence.jsonl",
            "--registry",
            "registries/entity",
            "--work-dir",
            "work/entity",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Solve(_)));
        if let EntitySubcommand::Solve(solve) = subcommand {
            assert_eq!(solve.rows, PathBuf::from("rows.csv"));
            assert_eq!(solve.profile.as_deref(), Some("entity_profile"));
            assert_eq!(solve.strategy, PathBuf::from("strategy.yaml"));
            assert_eq!(solve.evidence, PathBuf::from("evidence.jsonl"));
            assert_eq!(solve.registry, PathBuf::from("registries/entity"));
            assert_eq!(solve.work_dir, Some(PathBuf::from("work/entity")));
            assert!(matches!(solve.emit, EntityEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_entity_link_parsing() {
        let args = [
            "canon",
            "entity",
            "link",
            "reference.csv",
            "target.csv",
            "--profile",
            "entity_profile",
            "--strategy",
            "strategy.yaml",
            "--registry",
            "registries/entity",
            "--gold",
            "gold/link_matches.jsonl",
            "--write-back",
            "--max-candidates",
            "25",
            "--max-rows",
            "1000",
            "--max-bytes",
            "1048576",
            "--work-dir",
            "work/entity-link",
            "--cache-mode",
            "disabled",
            "--suite",
            "suite",
            "--emit",
            "summary",
            "--no-witness",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Link(_)));
        if let EntitySubcommand::Link(link) = subcommand {
            assert_eq!(link.reference, PathBuf::from("reference.csv"));
            assert_eq!(link.target, PathBuf::from("target.csv"));
            assert_eq!(link.profile.as_deref(), Some("entity_profile"));
            assert_eq!(link.strategy, PathBuf::from("strategy.yaml"));
            assert_eq!(link.registry, PathBuf::from("registries/entity"));
            assert_eq!(link.gold, Some(PathBuf::from("gold/link_matches.jsonl")));
            assert!(link.write_back);
            assert_eq!(link.max_candidates, Some(25));
            assert_eq!(link.max_rows, Some(1000));
            assert_eq!(link.max_bytes, Some(1_048_576));
            assert_eq!(link.work_dir, Some(PathBuf::from("work/entity-link")));
            assert!(matches!(link.cache_mode, EntityCacheModeArg::Disabled));
            assert_eq!(link.suite, Some(PathBuf::from("suite")));
            assert!(matches!(link.emit, EntityEmitMode::Summary));
            assert!(link.no_witness);
        }
    }

    #[test]
    fn test_cli_entity_link_cache_mode_defaults_to_enabled() {
        let args = [
            "canon",
            "entity",
            "link",
            "reference.csv",
            "target.csv",
            "--profile",
            "entity_profile",
            "--strategy",
            "strategy.yaml",
            "--registry",
            "registries/entity",
            "--work-dir",
            "work/entity-link",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let EntitySubcommand::Link(link) = command.command else {
            panic!("expected entity link command");
        };
        assert!(matches!(link.cache_mode, EntityCacheModeArg::Enabled));
    }

    #[test]
    fn test_cli_entity_audit_parsing() {
        let args = [
            "canon",
            "entity",
            "audit",
            "result.json",
            "--suite",
            "suite",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Audit(_)));
        if let EntitySubcommand::Audit(audit) = subcommand {
            assert_eq!(audit.result, PathBuf::from("result.json"));
            assert_eq!(audit.suite, PathBuf::from("suite"));
            assert!(matches!(audit.emit, EntityEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_entity_promote_parsing() {
        let args = [
            "canon",
            "entity",
            "promote",
            "result.json",
            "--audit",
            "audit.json",
            "--registry",
            "registries/org",
            "--next-version",
            "2026.03.23",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Promote(_)));
        if let EntitySubcommand::Promote(promote) = subcommand {
            assert_eq!(promote.result, PathBuf::from("result.json"));
            assert_eq!(promote.audit, PathBuf::from("audit.json"));
            assert_eq!(promote.registry, PathBuf::from("registries/org"));
            assert_eq!(promote.next_version, "2026.03.23");
            assert!(matches!(promote.emit, EntityEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_entity_review_export_parsing() {
        let args = [
            "canon",
            "entity",
            "review",
            "export",
            "result.json",
            "--emit",
            "csv",
            "--include",
            "escrow",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Review(_)));
        if let EntitySubcommand::Review(review) = subcommand {
            let review_subcommand = review.command;
            assert!(matches!(
                &review_subcommand,
                EntityReviewSubcommand::Export(_)
            ));
            if let EntityReviewSubcommand::Export(export) = review_subcommand {
                assert_eq!(export.result, PathBuf::from("result.json"));
                assert!(matches!(export.emit, EntityReviewExportEmitMode::Csv));
                assert!(matches!(export.artifact, EntityReviewExportArtifact::Queue));
                assert!(matches!(export.include, EntityReviewInclude::Escrow));
            }
        }
    }

    #[test]
    fn test_cli_entity_review_native_export_parsing() {
        let args = [
            "canon",
            "entity",
            "review",
            "export",
            "result.json",
            "--artifact",
            "native-review",
            "--emit",
            "html",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        if let EntitySubcommand::Review(review) = command.command
            && let EntityReviewSubcommand::Export(export) = review.command
        {
            assert_eq!(export.result, PathBuf::from("result.json"));
            assert!(matches!(
                export.artifact,
                EntityReviewExportArtifact::NativeReview
            ));
            assert!(matches!(export.emit, EntityReviewExportEmitMode::Html));
            assert!(matches!(export.include, EntityReviewInclude::All));
        }
    }

    #[test]
    fn test_cli_entity_review_import_parsing() {
        let args = [
            "canon",
            "entity",
            "review",
            "import",
            "review.csv",
            "--registry",
            "registries/org",
            "--next-version",
            "2026.05.06",
            "--audit",
            "audit.json",
            "--source-review",
            "native-review.json",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Review(_)));
        if let EntitySubcommand::Review(review) = subcommand {
            let review_subcommand = review.command;
            assert!(matches!(
                &review_subcommand,
                EntityReviewSubcommand::Import(_)
            ));
            if let EntityReviewSubcommand::Import(import) = review_subcommand {
                assert_eq!(import.review, PathBuf::from("review.csv"));
                assert_eq!(import.registry, PathBuf::from("registries/org"));
                assert_eq!(import.next_version, "2026.05.06");
                assert_eq!(import.audit, Some(PathBuf::from("audit.json")));
                assert_eq!(
                    import.source_review,
                    Some(PathBuf::from("native-review.json"))
                );
                assert!(matches!(import.emit, EntityEmitMode::Summary));
            }
        }
    }

    #[test]
    fn test_cli_entity_explain_parsing() {
        let args = [
            "canon",
            "entity",
            "explain",
            "result.json",
            "--canon-id",
            "ORG-0001",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Explain(_)));
        if let EntitySubcommand::Explain(explain) = subcommand {
            assert_eq!(explain.result, PathBuf::from("result.json"));
            assert_eq!(explain.canon_id.as_deref(), Some("ORG-0001"));
            assert_eq!(explain.row, None);
            assert_eq!(explain.surface_id, None);
            assert_eq!(explain.escrow_id, None);
            assert!(matches!(explain.emit, EntityEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_entity_explain_surface_id_parsing() {
        let args = [
            "canon",
            "entity",
            "explain",
            "result.json",
            "--surface-id",
            "surf:cmbs_tenant_label:abc",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Explain(_)));
        if let EntitySubcommand::Explain(explain) = subcommand {
            assert_eq!(explain.result, PathBuf::from("result.json"));
            assert_eq!(
                explain.surface_id.as_deref(),
                Some("surf:cmbs_tenant_label:abc")
            );
            assert_eq!(explain.row, None);
            assert_eq!(explain.canon_id, None);
            assert_eq!(explain.escrow_id, None);
        }
    }

    #[test]
    fn test_cli_entity_explain_requires_exactly_one_selector() {
        let args = [
            "canon",
            "entity",
            "explain",
            "result.json",
            "--row",
            "row-1",
            "--canon-id",
            "ORG-0001",
        ];

        assert!(Cli::try_parse_from(args).is_err());
    }

    #[test]
    fn test_cli_entity_profile_list_parsing() {
        let args = ["canon", "entity", "profile", "list", "--emit", "summary"];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Profile(_)));
        if let EntitySubcommand::Profile(profile) = subcommand {
            let profile_subcommand = profile.command;
            assert!(matches!(
                &profile_subcommand,
                EntityProfileSubcommand::List(_)
            ));
            if let EntityProfileSubcommand::List(list) = profile_subcommand {
                assert!(matches!(list.emit, RegistryEmitMode::Summary));
            }
        }
    }

    #[test]
    fn test_cli_entity_profile_init_parsing() {
        let args = [
            "canon",
            "entity",
            "profile",
            "init",
            "cmbs_tenant_label",
            "--output",
            "strategy.yaml",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Profile(_)));
        if let EntitySubcommand::Profile(profile) = subcommand {
            let profile_subcommand = profile.command;
            assert!(matches!(
                &profile_subcommand,
                EntityProfileSubcommand::Init(_)
            ));
            if let EntityProfileSubcommand::Init(init) = profile_subcommand {
                assert_eq!(init.profile, "cmbs_tenant_label");
                assert_eq!(init.output, PathBuf::from("strategy.yaml"));
            }
        }
    }

    #[test]
    fn test_cli_csv_mode() {
        let args = [
            "canon",
            "input.csv",
            "--registry",
            "registries/test",
            "--column",
            "id",
            "--emit",
            "csv",
            "--canon-column",
            "id_canon",
            "--map-out",
            "mapping.json",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        assert!(matches!(cli.emit, EmitMode::Csv));
        assert_eq!(cli.canon_column, Some("id_canon".to_string()));
        assert_eq!(cli.map_out, Some(PathBuf::from("mapping.json")));
    }

    #[test]
    fn test_cli_package_local_argument_shapes() {
        let pack = Cli::try_parse_from([
            "canon",
            "package",
            "pack",
            "--root",
            "pkg-root",
            "--package",
            "package.json",
            "--out",
            "pkg.canonpkg",
        ])
        .unwrap();
        let Some(CanonCommand::Package(package)) = pack.command else {
            panic!("expected package command");
        };
        assert!(matches!(package.command, PackageSubcommand::Pack(_)));
        if let PackageSubcommand::Pack(args) = package.command {
            assert_eq!(args.root, PathBuf::from("pkg-root"));
            assert_eq!(args.package, PathBuf::from("package.json"));
            assert_eq!(args.out, PathBuf::from("pkg.canonpkg"));
        }

        let unpack = Cli::try_parse_from([
            "canon",
            "package",
            "unpack",
            "pkg.canonpkg",
            "--target",
            "target",
            "--emit",
            "summary",
        ])
        .unwrap();
        let Some(CanonCommand::Package(package)) = unpack.command else {
            panic!("expected package command");
        };
        assert!(matches!(package.command, PackageSubcommand::Unpack(_)));
        if let PackageSubcommand::Unpack(args) = package.command {
            assert_eq!(args.archive, PathBuf::from("pkg.canonpkg"));
            assert_eq!(args.target, PathBuf::from("target"));
            assert!(matches!(args.emit, RegistryEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_package_remote_argument_shapes() {
        let push = Cli::try_parse_from([
            "canon",
            "package",
            "push",
            "--archive",
            "pkg.canonpkg",
            "--registry",
            "http://127.0.0.1:5000",
            "--repository",
            "canon/registry",
            "--tag",
            "latest",
            "--emit",
            "summary",
        ])
        .unwrap();
        let Some(CanonCommand::Package(package)) = push.command else {
            panic!("expected package command");
        };
        assert!(matches!(package.command, PackageSubcommand::Push(_)));
        if let PackageSubcommand::Push(args) = package.command {
            assert_eq!(args.archive, PathBuf::from("pkg.canonpkg"));
            assert_eq!(args.registry, "http://127.0.0.1:5000");
            assert_eq!(args.repository, "canon/registry");
            assert_eq!(args.tag.as_deref(), Some("latest"));
            assert!(matches!(args.emit, RegistryEmitMode::Summary));
        }

        let pull = Cli::try_parse_from([
            "canon",
            "package",
            "pull",
            "--registry",
            "http://127.0.0.1:5000",
            "--repository",
            "canon/registry",
            "--cache",
            "cache",
            "--digest",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ])
        .unwrap();
        let Some(CanonCommand::Package(package)) = pull.command else {
            panic!("expected package command");
        };
        assert!(matches!(package.command, PackageSubcommand::Pull(_)));
        if let PackageSubcommand::Pull(args) = package.command {
            assert_eq!(args.registry, "http://127.0.0.1:5000");
            assert_eq!(args.repository, "canon/registry");
            assert_eq!(args.cache, PathBuf::from("cache"));
            assert_eq!(
                args.digest.as_deref(),
                Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            );
            assert_eq!(args.tag, None);
            assert!(matches!(args.emit, RegistryEmitMode::Json));
        }

        let missing_reference = Cli::try_parse_from([
            "canon",
            "package",
            "pull",
            "--registry",
            "http://127.0.0.1:5000",
            "--repository",
            "canon/registry",
            "--cache",
            "cache",
        ]);
        assert!(missing_reference.is_err());
    }
}
