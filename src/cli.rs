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

/// Emit mode for org artifact subcommands
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum OrgEmitMode {
    /// Structured org JSON artifact (default)
    #[default]
    Json,
    /// Human-readable org summary
    Summary,
}

/// Emit mode for org streaming subcommands
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum OrgStreamEmitMode {
    /// Line-delimited org records (default)
    #[default]
    Jsonl,
    /// Human-readable org summary
    Summary,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CanonCommand {
    /// Registry maintenance and inspection commands
    Registry(RegistryCommand),
    /// Organization identity commands
    Org(OrgCommand),
    /// Frozen script strategy registry commands
    Strategy(StrategyCommand),
}

#[derive(Args, Debug, Clone)]
pub struct RegistryCommand {
    #[command(subcommand)]
    pub command: RegistrySubcommand,
}

#[derive(Args, Debug, Clone)]
pub struct OrgCommand {
    #[command(subcommand)]
    pub command: OrgSubcommand,
}

#[derive(Args, Debug, Clone)]
pub struct StrategyCommand {
    #[command(subcommand)]
    pub command: StrategySubcommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum RegistrySubcommand {
    /// Compare two registry versions and report what changed
    Diff(RegistryDiffCli),
    /// Audit a seed corpus against a registry for authoring workflows
    Audit(RegistryAuditCli),
    /// Materialize a registry from a provider and seed corpus
    Build(RegistryBuildCli),
}

#[derive(Subcommand, Debug, Clone)]
pub enum OrgSubcommand {
    /// Run the full org orchestration flow
    Run(OrgRunCli),
    /// Generate candidate neighborhoods
    Block(OrgBlockCli),
    /// Generate typed evidence edges for candidate pairs
    Edge(OrgEdgeCli),
    /// Solve org identity assignments from evidence edges
    Solve(OrgSolveCli),
    /// Audit an org result artifact against a suite
    Audit(OrgAuditCli),
    /// Promote an audited org result into the registry and escrow sidecars
    Promote(OrgPromoteCli),
    /// Explain one org row, canonical entity, or escrow entity
    Explain(OrgExplainCli),
}

#[derive(Subcommand, Debug, Clone)]
pub enum StrategySubcommand {
    /// Resolve a schema shape and skill hash to a frozen champion script
    Resolve(StrategyResolveCli),
    /// Register a verified frozen champion script for a schema shape and skill hash
    Register(StrategyRegisterCli),
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

    /// Provider-specific key=value option (repeatable)
    #[arg(long = "provider-config", value_name = "KEY=VALUE")]
    pub provider_config: Vec<String>,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("skill_identity")
        .required(true)
        .multiple(false)
        .args(["skill", "skill_hash"])
))]
pub struct StrategyResolveCli {
    /// Strategy registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Schema/profile JSON file describing the input shape
    #[arg(long)]
    pub schema: PathBuf,

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
pub struct StrategyRegisterCli {
    /// Strategy registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Schema/profile JSON file describing the input shape
    #[arg(long)]
    pub schema: PathBuf,

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

    /// Verify artifact proving the script passed verification
    #[arg(long)]
    pub verify: PathBuf,

    /// Assess artifact proving the script should proceed
    #[arg(long)]
    pub assess: PathBuf,

    /// Airlock artifact proving the script cleared airlock
    #[arg(long)]
    pub airlock: PathBuf,

    /// Explicit next registry version
    #[arg(long = "next-version")]
    pub next_version: String,

    /// Rule identifier for this registration
    #[arg(long = "rule-id")]
    pub rule_id: Option<String>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct OrgRunCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Org registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Frozen evaluation suite directory
    #[arg(long)]
    pub suite: Option<PathBuf>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: OrgEmitMode,

    /// Suppress witness ledger append
    #[arg(long)]
    pub no_witness: bool,
}

#[derive(Args, Debug, Clone)]
pub struct OrgBlockCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Org registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "jsonl")]
    pub emit: OrgStreamEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct OrgEdgeCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Candidate block artifact
    #[arg(long)]
    pub candidates: PathBuf,

    /// Org registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "jsonl")]
    pub emit: OrgStreamEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct OrgSolveCli {
    /// Input CSV or JSONL rows
    pub rows: PathBuf,

    /// Strategy YAML file
    #[arg(long)]
    pub strategy: PathBuf,

    /// Edge artifact
    #[arg(long)]
    pub edges: PathBuf,

    /// Org registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: OrgEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct OrgAuditCli {
    /// Org solve or run artifact
    pub result: PathBuf,

    /// Frozen evaluation suite directory
    #[arg(long)]
    pub suite: PathBuf,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: OrgEmitMode,
}

#[derive(Args, Debug, Clone)]
pub struct OrgPromoteCli {
    /// Org solve or run artifact
    pub result: PathBuf,

    /// Audit artifact
    #[arg(long)]
    pub audit: PathBuf,

    /// Org registry directory
    #[arg(long)]
    pub registry: PathBuf,

    /// Explicit next registry version
    #[arg(long)]
    pub next_version: String,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: OrgEmitMode,
}

#[derive(Args, Debug, Clone)]
#[command(group(
    ArgGroup::new("query")
        .required(true)
        .multiple(false)
        .args(["row", "canon_id", "escrow_id"])
))]
pub struct OrgExplainCli {
    /// Org solve or run artifact
    pub result: PathBuf,

    /// Explain a source row by source_row_id
    #[arg(long)]
    pub row: Option<String>,

    /// Explain a resolved entity by canonical ID
    #[arg(long)]
    pub canon_id: Option<String>,

    /// Explain an escrow entity by escrow ID
    #[arg(long)]
    pub escrow_id: Option<String>,

    /// Output mode
    #[arg(long, value_enum, default_value = "json")]
    pub emit: OrgEmitMode,
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

    #[test]
    fn test_emit_mode_default() {
        assert!(matches!(EmitMode::default(), EmitMode::Json));
    }

    #[test]
    fn test_org_emit_mode_defaults() {
        assert!(matches!(OrgEmitMode::default(), OrgEmitMode::Json));
        assert!(matches!(
            OrgStreamEmitMode::default(),
            OrgStreamEmitMode::Jsonl
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

        match cli.command.unwrap() {
            CanonCommand::Registry(command) => {
                let subcommand = command.command;
                assert!(matches!(subcommand, RegistrySubcommand::Diff(_)));
                if let RegistrySubcommand::Diff(diff) = subcommand {
                    assert_eq!(diff.old, PathBuf::from("registries/test-v1"));
                    assert_eq!(diff.new, PathBuf::from("registries/test-v2"));
                    assert!(matches!(diff.emit, RegistryEmitMode::Summary));
                }
            }
            other => panic!("expected registry command, got {:?}", other),
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

        match cli.command.unwrap() {
            CanonCommand::Registry(command) => {
                let subcommand = command.command;
                assert!(matches!(subcommand, RegistrySubcommand::Audit(_)));
                if let RegistrySubcommand::Audit(audit) = subcommand {
                    assert_eq!(audit.seed, PathBuf::from("seeds.csv"));
                    assert_eq!(audit.registry, PathBuf::from("registries/test"));
                    assert_eq!(audit.column, "cusip");
                    assert!(matches!(audit.emit, RegistryEmitMode::Summary));
                    assert_eq!(audit.max_rows, Some(10));
                }
            }
            other => panic!("expected registry command, got {:?}", other),
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

        match cli.command.unwrap() {
            CanonCommand::Registry(command) => {
                let subcommand = command.command;
                assert!(matches!(subcommand, RegistrySubcommand::Build(_)));
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
            other => panic!("expected registry command, got {:?}", other),
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

        match cli.command.unwrap() {
            CanonCommand::Strategy(command) => match command.command {
                StrategySubcommand::Resolve(resolve) => {
                    assert_eq!(resolve.registry, PathBuf::from("registries/strategies"));
                    assert_eq!(resolve.schema, PathBuf::from("profile.json"));
                    assert_eq!(resolve.skill, Some(PathBuf::from("SKILL.md")));
                    assert_eq!(resolve.skill_hash, None);
                    assert!(matches!(resolve.emit, RegistryEmitMode::Summary));
                }
                other => panic!("expected strategy resolve, got {:?}", other),
            },
            other => panic!("expected strategy command, got {:?}", other),
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

        match cli.command.unwrap() {
            CanonCommand::Strategy(command) => match command.command {
                StrategySubcommand::Register(register) => {
                    assert_eq!(register.registry, PathBuf::from("registries/strategies"));
                    assert_eq!(register.schema, PathBuf::from("profile.json"));
                    assert_eq!(register.skill, None);
                    assert_eq!(register.skill_hash.as_deref(), Some("blake3:abc"));
                    assert_eq!(register.script, PathBuf::from("script.py"));
                    assert_eq!(register.script_id, "procurement-total.v1");
                    assert_eq!(register.language, "python");
                    assert_eq!(register.verify, PathBuf::from("verify.json"));
                    assert_eq!(register.assess, PathBuf::from("assess.json"));
                    assert_eq!(register.airlock, PathBuf::from("airlock.json"));
                    assert_eq!(register.next_version, "0.2.0");
                    assert_eq!(register.rule_id.as_deref(), Some("PROCUREMENT_TOTAL"));
                }
                other => panic!("expected strategy register, got {:?}", other),
            },
            other => panic!("expected strategy command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_org_run_parsing() {
        let args = [
            "canon",
            "org",
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

        match cli.command.unwrap() {
            CanonCommand::Org(command) => match command.command {
                OrgSubcommand::Run(run) => {
                    assert_eq!(run.rows, PathBuf::from("rows.csv"));
                    assert_eq!(run.strategy, PathBuf::from("strategy.yaml"));
                    assert_eq!(run.registry, PathBuf::from("registries/org"));
                    assert_eq!(run.suite, Some(PathBuf::from("suite")));
                    assert!(matches!(run.emit, OrgEmitMode::Summary));
                    assert!(run.no_witness);
                }
                other => panic!("expected org run, got {:?}", other),
            },
            other => panic!("expected org command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_org_block_parsing() {
        let args = [
            "canon",
            "org",
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

        match cli.command.unwrap() {
            CanonCommand::Org(command) => match command.command {
                OrgSubcommand::Block(block) => {
                    assert_eq!(block.rows, PathBuf::from("rows.csv"));
                    assert_eq!(block.strategy, PathBuf::from("strategy.yaml"));
                    assert_eq!(block.registry, PathBuf::from("registries/org"));
                    assert!(matches!(block.emit, OrgStreamEmitMode::Summary));
                }
                other => panic!("expected org block, got {:?}", other),
            },
            other => panic!("expected org command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_org_edge_parsing() {
        let args = [
            "canon",
            "org",
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

        match cli.command.unwrap() {
            CanonCommand::Org(command) => match command.command {
                OrgSubcommand::Edge(edge) => {
                    assert_eq!(edge.rows, PathBuf::from("rows.csv"));
                    assert_eq!(edge.strategy, PathBuf::from("strategy.yaml"));
                    assert_eq!(edge.candidates, PathBuf::from("block.jsonl"));
                    assert_eq!(edge.registry, PathBuf::from("registries/org"));
                    assert!(matches!(edge.emit, OrgStreamEmitMode::Jsonl));
                }
                other => panic!("expected org edge, got {:?}", other),
            },
            other => panic!("expected org command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_org_solve_parsing() {
        let args = [
            "canon",
            "org",
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

        match cli.command.unwrap() {
            CanonCommand::Org(command) => match command.command {
                OrgSubcommand::Solve(solve) => {
                    assert_eq!(solve.rows, PathBuf::from("rows.csv"));
                    assert_eq!(solve.strategy, PathBuf::from("strategy.yaml"));
                    assert_eq!(solve.edges, PathBuf::from("edges.jsonl"));
                    assert_eq!(solve.registry, PathBuf::from("registries/org"));
                    assert!(matches!(solve.emit, OrgEmitMode::Summary));
                }
                other => panic!("expected org solve, got {:?}", other),
            },
            other => panic!("expected org command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_org_audit_parsing() {
        let args = [
            "canon",
            "org",
            "audit",
            "result.json",
            "--suite",
            "suite",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command.unwrap() {
            CanonCommand::Org(command) => match command.command {
                OrgSubcommand::Audit(audit) => {
                    assert_eq!(audit.result, PathBuf::from("result.json"));
                    assert_eq!(audit.suite, PathBuf::from("suite"));
                    assert!(matches!(audit.emit, OrgEmitMode::Summary));
                }
                other => panic!("expected org audit, got {:?}", other),
            },
            other => panic!("expected org command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_org_promote_parsing() {
        let args = [
            "canon",
            "org",
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

        match cli.command.unwrap() {
            CanonCommand::Org(command) => match command.command {
                OrgSubcommand::Promote(promote) => {
                    assert_eq!(promote.result, PathBuf::from("result.json"));
                    assert_eq!(promote.audit, PathBuf::from("audit.json"));
                    assert_eq!(promote.registry, PathBuf::from("registries/org"));
                    assert_eq!(promote.next_version, "2026.03.23");
                    assert!(matches!(promote.emit, OrgEmitMode::Summary));
                }
                other => panic!("expected org promote, got {:?}", other),
            },
            other => panic!("expected org command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_org_explain_parsing() {
        let args = [
            "canon",
            "org",
            "explain",
            "result.json",
            "--canon-id",
            "ORG-0001",
            "--emit",
            "summary",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command.unwrap() {
            CanonCommand::Org(command) => match command.command {
                OrgSubcommand::Explain(explain) => {
                    assert_eq!(explain.result, PathBuf::from("result.json"));
                    assert_eq!(explain.canon_id.as_deref(), Some("ORG-0001"));
                    assert_eq!(explain.row, None);
                    assert_eq!(explain.escrow_id, None);
                    assert!(matches!(explain.emit, OrgEmitMode::Summary));
                }
                other => panic!("expected org explain, got {:?}", other),
            },
            other => panic!("expected org command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_org_explain_requires_exactly_one_selector() {
        let args = [
            "canon",
            "org",
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
