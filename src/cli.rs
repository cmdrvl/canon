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

/// Emit mode for org artifact subcommands
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum EntityEmitMode {
    /// Structured org JSON artifact (default)
    #[default]
    Json,
    /// Human-readable org summary
    Summary,
}

/// Emit mode for cross-tape resolve artifacts
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum ResolveEmitMode {
    /// Structured resolve JSON artifact (default)
    #[default]
    Json,
    /// Human-readable resolve summary
    Summary,
}

/// Emit mode for org streaming subcommands
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum EntityStreamEmitMode {
    /// Line-delimited org records (default)
    #[default]
    Jsonl,
    /// Human-readable org summary
    Summary,
}

/// Emit mode for org review export
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum EntityReviewExportEmitMode {
    /// Structured review JSON artifact (default)
    #[default]
    Json,
    /// Human-reviewable CSV artifact
    Csv,
}

/// Include selector for org review export
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
    /// Cross-tape structural resolution workbench
    Resolve(ResolveCli),
    /// Registry maintenance and inspection commands
    Registry(RegistryCommand),
    /// Profiled entity workbench commands
    #[command(name = "entity")]
    Entity(EntityCommand),
    /// Frozen script strategy registry commands
    Strategy(StrategyCommand),
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
pub struct StrategyCommand {
    #[command(subcommand)]
    pub command: StrategySubcommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum RegistrySubcommand {
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
    /// Generate candidate neighborhoods
    Block(EntityBlockCli),
    /// Generate typed evidence edges for candidate pairs
    Edge(EntityEdgeCli),
    /// Solve entity identity assignments from evidence edges
    Solve(EntitySolveCli),
    /// Audit an entity result artifact against a suite
    Audit(EntityAuditCli),
    /// Promote an audited entity result into the registry and escrow sidecars
    Promote(EntityPromoteCli),
    /// Explain one entity row, canonical entity, or escrow entity
    Explain(EntityExplainCli),
    /// List and initialize built-in entity profile templates
    Profile(EntityProfileCommand),
    /// Export and import human adjudication review queues
    Review(EntityReviewCommand),
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
pub struct ResolveCli {
    /// Authoritative/reference tape
    pub reference_tape: PathBuf,

    /// Target tape to match against the reference
    pub target_tape: PathBuf,

    /// Matching strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Registry directory used for canon_match lookups and optional write-back
    #[arg(long)]
    pub registry: PathBuf,

    /// Gold cross-reference JSONL file for scoring
    #[arg(long)]
    pub gold: Option<PathBuf>,

    /// Write matched ID pairs back into a new registry mapping file
    #[arg(long)]
    pub write_back: bool,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: ResolveEmitMode,

    /// Refuse if a target record has more than N surviving candidates
    #[arg(long)]
    pub max_candidates: Option<usize>,

    /// Refuse if either tape exceeds N data rows
    #[arg(long)]
    pub max_rows: Option<usize>,

    /// Refuse if either tape exceeds N bytes
    #[arg(long)]
    pub max_bytes: Option<u64>,

    /// Suppress witness ledger append
    #[arg(long)]
    pub no_witness: bool,
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
pub struct EntityBlockCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "jsonl")]
    pub emit: EntityStreamEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntityEdgeCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Candidate block artifact
    #[arg(long)]
    pub candidates: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "jsonl")]
    pub emit: EntityStreamEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct EntitySolveCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Edge artifact
    #[arg(long)]
    pub edges: PathBuf,

    /// Entity registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: EntityEmitMode,
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
    /// Built-in profile id, such as cmbs_tenant_label or regab_firm_identity
    pub profile: String,

    /// Output YAML path to write
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Subcommand, Debug, Clone)]
pub enum EntityReviewSubcommand {
    /// Export reviewable org identity clusters from a solve/run artifact
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
    fn test_cli_resolve_parsing() {
        let args = [
            "canon",
            "resolve",
            "trustee.csv",
            "servicer.csv",
            "--strategy",
            "strategies/cmbs.yaml",
            "--registry",
            "registries/cmbs-loan",
            "--gold",
            "gold/loan_matches.jsonl",
            "--write-back",
            "--emit",
            "summary",
            "--max-candidates",
            "25",
            "--max-rows",
            "1000",
            "--max-bytes",
            "1048576",
            "--no-witness",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let command = cli.command;
        assert!(matches!(&command, Some(CanonCommand::Resolve(_))));
        if let Some(CanonCommand::Resolve(resolve)) = command {
            assert_eq!(resolve.reference_tape, PathBuf::from("trustee.csv"));
            assert_eq!(resolve.target_tape, PathBuf::from("servicer.csv"));
            assert_eq!(resolve.strategy, PathBuf::from("strategies/cmbs.yaml"));
            assert_eq!(resolve.registry, PathBuf::from("registries/cmbs-loan"));
            assert_eq!(resolve.gold, Some(PathBuf::from("gold/loan_matches.jsonl")));
            assert!(resolve.write_back);
            assert!(matches!(resolve.emit, ResolveEmitMode::Summary));
            assert_eq!(resolve.max_candidates, Some(25));
            assert_eq!(resolve.max_rows, Some(1000));
            assert_eq!(resolve.max_bytes, Some(1_048_576));
            assert!(resolve.no_witness);
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
            "registries/org",
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
            assert_eq!(run.strategy, PathBuf::from("strategy.yaml"));
            assert_eq!(run.registry, PathBuf::from("registries/org"));
            assert_eq!(run.suite, Some(PathBuf::from("suite")));
            assert!(matches!(run.emit, EntityEmitMode::Summary));
            assert!(run.no_witness);
        }
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
    fn test_cli_entity_block_parsing() {
        let args = [
            "canon",
            "entity",
            "block",
            "rows.csv",
            "--strategy",
            "strategy.yaml",
            "--registry",
            "registries/org",
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
            assert_eq!(block.strategy, PathBuf::from("strategy.yaml"));
            assert_eq!(block.registry, PathBuf::from("registries/org"));
            assert!(matches!(block.emit, EntityStreamEmitMode::Summary));
        }
    }

    #[test]
    fn test_cli_entity_edge_parsing() {
        let args = [
            "canon",
            "entity",
            "edge",
            "rows.csv",
            "--strategy",
            "strategy.yaml",
            "--candidates",
            "block.jsonl",
            "--registry",
            "registries/org",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        let Some(command) = entity_command(cli) else {
            return;
        };
        let subcommand = command.command;
        assert!(matches!(&subcommand, EntitySubcommand::Edge(_)));
        if let EntitySubcommand::Edge(edge) = subcommand {
            assert_eq!(edge.rows, PathBuf::from("rows.csv"));
            assert_eq!(edge.strategy, PathBuf::from("strategy.yaml"));
            assert_eq!(edge.candidates, PathBuf::from("block.jsonl"));
            assert_eq!(edge.registry, PathBuf::from("registries/org"));
            assert!(matches!(edge.emit, EntityStreamEmitMode::Jsonl));
        }
    }

    #[test]
    fn test_cli_entity_solve_parsing() {
        let args = [
            "canon",
            "entity",
            "solve",
            "rows.csv",
            "--strategy",
            "strategy.yaml",
            "--edges",
            "edges.jsonl",
            "--registry",
            "registries/org",
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
            assert_eq!(solve.strategy, PathBuf::from("strategy.yaml"));
            assert_eq!(solve.edges, PathBuf::from("edges.jsonl"));
            assert_eq!(solve.registry, PathBuf::from("registries/org"));
            assert!(matches!(solve.emit, EntityEmitMode::Summary));
        }
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
                assert!(matches!(export.include, EntityReviewInclude::Escrow));
            }
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
}
