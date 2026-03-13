use clap::{Args, Parser, Subcommand, ValueEnum};
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

#[derive(Subcommand, Debug, Clone)]
pub enum CanonCommand {
    /// Registry maintenance and inspection commands
    Registry(RegistryCommand),
}

#[derive(Args, Debug, Clone)]
pub struct RegistryCommand {
    #[command(subcommand)]
    pub command: RegistrySubcommand,
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
                }
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
