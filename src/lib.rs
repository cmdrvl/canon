#![forbid(unsafe_code)]

pub mod cli;
pub mod input;
pub mod lookup;
pub mod org;
pub mod output;
pub mod refusal;
pub mod registry;
pub mod registry_lint;
pub mod resolve;
pub mod strategy_audit;
pub mod strategy_profile;
pub mod strategy_registry;
pub mod witness;

use crate::cli::{
    CanonCommand, Cli, OrgAuditCli, OrgBlockCli, OrgCommand, OrgEdgeCli, OrgEmitMode,
    OrgExplainCli, OrgPromoteCli, OrgReviewCommand, OrgReviewExportCli, OrgReviewExportEmitMode,
    OrgReviewImportCli, OrgReviewInclude, OrgReviewSubcommand, OrgRunCli, OrgSolveCli,
    OrgStreamEmitMode, OrgSubcommand, RegistryAuditCli, RegistryBuildCli, RegistryDiffCli,
    RegistryEmitMode, RegistryLintCli, RegistryLintProfile, RegistrySubcommand, ResolveCli,
    ResolveEmitMode, StrategyAuditCli, StrategyCommand, StrategyDiffCli, StrategyProfileCli,
    StrategyRegisterCli, StrategyResolveCli, StrategySubcommand,
};
use serde::{Deserialize, Serialize, Serializer, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

const ORG_V1_PROFILE: &str = "bdc_issuer";

struct OrgRunExecution {
    artifact: org::SolveRunArtifact,
    candidate_pairs: u64,
}

// Entry point function
pub fn run(cli: Cli) -> Result<u8, Box<dyn Error>> {
    // Step 1: Handle info commands (early return)
    if cli.version {
        return run_display_mode(DisplayMode::Version);
    }

    if cli.describe {
        return run_display_mode(DisplayMode::Describe);
    }

    if cli.schema {
        return run_display_mode(DisplayMode::Schema);
    }

    if let Some(command) = &cli.command {
        return run_command(command);
    }

    // Step 2: Validate required args
    let input_path = cli
        .input
        .as_ref()
        .ok_or("Input path required")?
        .to_path_buf();
    let registry_path = cli
        .registry
        .as_ref()
        .ok_or("Registry path required")?
        .to_path_buf();
    let column = cli
        .column
        .as_deref()
        .ok_or("Column name required")?
        .to_string();

    // Step 3: Warn on stderr if --map-out is set with --emit json
    if matches!(cli.emit, crate::cli::EmitMode::Json) && cli.map_out.is_some() {
        eprintln!("Warning: --map-out ignored in JSON mode (mapping already is stdout)");
    }

    // Handle refusals by converting to CanonOutput and routing appropriately
    let result = run_pipeline(&input_path, &registry_path, &column, &cli);

    match result {
        Ok(exit_code) => Ok(exit_code),
        Err(refusal_output) => {
            match cli.emit {
                crate::cli::EmitMode::Json => {
                    // Refusal JSON to stdout
                    println!("{}", serde_json::to_string(&refusal_output)?);
                }
                crate::cli::EmitMode::Csv => {
                    // Refusal JSON to stderr
                    eprintln!("{}", serde_json::to_string(&refusal_output)?);
                }
            }
            Ok(2) // REFUSAL exit code
        }
    }
}

fn run_command(command: &CanonCommand) -> Result<u8, Box<dyn Error>> {
    match command {
        CanonCommand::Resolve(resolve) => run_resolve_command(resolve),
        CanonCommand::Registry(command) => match &command.command {
            RegistrySubcommand::Diff(diff) => run_registry_diff(diff),
            RegistrySubcommand::Audit(audit) => run_registry_audit(audit),
            RegistrySubcommand::Build(build) => run_registry_build(build),
            RegistrySubcommand::Lint(lint) => run_registry_lint(lint),
        },
        CanonCommand::Org(command) => run_org_command(command),
        CanonCommand::Strategy(command) => run_strategy_command(command),
    }
}

fn run_resolve_command(resolve_cli: &ResolveCli) -> Result<u8, Box<dyn Error>> {
    let request = resolve::ResolveRequest {
        reference_tape: resolve_cli.reference_tape.clone(),
        target_tape: resolve_cli.target_tape.clone(),
        strategy: resolve_cli.strategy.clone(),
        registry: resolve_cli.registry.clone(),
        gold: resolve_cli.gold.clone(),
        write_back: resolve_cli.write_back,
        max_candidates: resolve_cli.max_candidates,
        max_rows: resolve_cli.max_rows,
        max_bytes: resolve_cli.max_bytes,
        no_witness: resolve_cli.no_witness,
    };

    match resolve::run(request) {
        Ok(output) => {
            match resolve_cli.emit {
                ResolveEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                ResolveEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(output.exit_code())
        }
        Err(error) => {
            let output = create_resolve_refusal(error);
            match resolve_cli.emit {
                ResolveEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                ResolveEmitMode::Summary => eprintln!("{}", serde_json::to_string(&output)?),
            }
            Ok(2)
        }
    }
}

fn run_strategy_command(command: &StrategyCommand) -> Result<u8, Box<dyn Error>> {
    match &command.command {
        StrategySubcommand::Profile(profile) => run_strategy_profile_command(profile),
        StrategySubcommand::Audit(audit) => run_strategy_audit_command(audit),
        StrategySubcommand::Resolve(resolve) => run_strategy_resolve_command(resolve),
        StrategySubcommand::Register(register) => run_strategy_register_command(register),
        StrategySubcommand::Diff(diff) => run_strategy_diff_command(diff),
    }
}

fn run_strategy_profile_command(profile: &StrategyProfileCli) -> Result<u8, Box<dyn Error>> {
    match strategy_profile::profile(&profile.input, profile.max_bytes, profile.max_rows) {
        Ok(output) => {
            match profile.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(0)
        }
        Err(refusal) => {
            emit_strategy_refusal(refusal, matches!(profile.emit, RegistryEmitMode::Summary))
        }
    }
}

fn run_strategy_audit_command(audit: &StrategyAuditCli) -> Result<u8, Box<dyn Error>> {
    match strategy_audit::audit(&audit.schema, &audit.script, &audit.suite) {
        Ok(output) => {
            match audit.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(output.exit_code())
        }
        Err(refusal) => {
            emit_strategy_refusal(refusal, matches!(audit.emit, RegistryEmitMode::Summary))
        }
    }
}

fn run_strategy_resolve_command(resolve: &StrategyResolveCli) -> Result<u8, Box<dyn Error>> {
    match strategy_registry::resolve(
        &resolve.registry,
        &resolve.schema,
        resolve.skill.as_deref(),
        resolve.skill_hash.as_deref(),
    ) {
        Ok(output) => {
            match resolve.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(output.exit_code())
        }
        Err(refusal) => {
            emit_strategy_refusal(refusal, matches!(resolve.emit, RegistryEmitMode::Summary))
        }
    }
}

fn run_strategy_register_command(register: &StrategyRegisterCli) -> Result<u8, Box<dyn Error>> {
    let request = strategy_registry::StrategyRegisterRequest {
        registry_dir: &register.registry,
        schema_path: &register.schema,
        skill_path: register.skill.as_deref(),
        skill_hash: register.skill_hash.as_deref(),
        script_path: &register.script,
        script_id: &register.script_id,
        language: &register.language,
        verify_path: &register.verify,
        assess_path: &register.assess,
        airlock_path: &register.airlock,
        next_version: &register.next_version,
        rule_id: register.rule_id.as_deref(),
    };

    match strategy_registry::register(request) {
        Ok(output) => {
            match register.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(0)
        }
        Err(refusal) => {
            emit_strategy_refusal(refusal, matches!(register.emit, RegistryEmitMode::Summary))
        }
    }
}

fn run_strategy_diff_command(diff: &StrategyDiffCli) -> Result<u8, Box<dyn Error>> {
    match strategy_registry::diff(&diff.old, &diff.new) {
        Ok(output) => {
            match diff.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(0)
        }
        Err(refusal) => {
            emit_strategy_refusal(refusal, matches!(diff.emit, RegistryEmitMode::Summary))
        }
    }
}

fn emit_strategy_refusal(refusal: Refusal, emit_summary: bool) -> Result<u8, Box<dyn Error>> {
    let output = refusal.to_canon_output();
    if emit_summary {
        eprintln!("{}", serde_json::to_string(&output)?);
    } else {
        println!("{}", serde_json::to_string(&output)?);
    }
    Ok(2)
}

fn run_org_command(command: &OrgCommand) -> Result<u8, Box<dyn Error>> {
    match &command.command {
        OrgSubcommand::Run(run) => run_org_run_command(run),
        OrgSubcommand::Block(block) => run_org_block_command(block),
        OrgSubcommand::Edge(edge) => run_org_edge_command(edge),
        OrgSubcommand::Solve(solve) => run_org_solve_command(solve),
        OrgSubcommand::Audit(audit) => run_org_audit_command(audit),
        OrgSubcommand::Promote(promote) => run_org_promote_command(promote),
        OrgSubcommand::Explain(explain) => run_org_explain_command(explain),
        OrgSubcommand::Review(review) => run_org_review_command(review),
    }
}

fn run_org_run_command(run: &OrgRunCli) -> Result<u8, Box<dyn Error>> {
    let started = Instant::now();

    match run_org_run_pipeline(run) {
        Ok(execution) => {
            let artifact_bytes = serde_json::to_vec(&execution.artifact)?;
            if let Some(suite_dir) = run.suite.as_deref()
                && let Err(refusal_output) = org::audit::audit(
                    &execution.artifact,
                    &artifact_bytes,
                    org::audit::AuditContext {
                        suite_dir,
                        profile: ORG_V1_PROFILE,
                        budget_usage: org::audit::AuditBudgetUsage {
                            runtime_seconds: started.elapsed().as_secs(),
                            candidate_pairs: execution.candidate_pairs,
                        },
                        baseline: None,
                        promoted_with_prior_escrow_count: 0,
                    },
                )
                .map_err(create_org_refusal)
            {
                return emit_org_refusal(
                    refusal_output,
                    true,
                    matches!(run.emit, OrgEmitMode::Summary),
                );
            }

            let output = match run.emit {
                OrgEmitMode::Json => org::output::emit_run_json(&execution.artifact)?,
                OrgEmitMode::Summary => org::output::render_run_summary(&execution.artifact),
            };
            emit_org_output(&output, matches!(run.emit, OrgEmitMode::Summary));
            append_org_run_witness(
                run,
                &execution.artifact,
                &output,
                started.elapsed().as_secs(),
                run.suite.as_deref().map(|_| execution.candidate_pairs),
            );
            Ok(0)
        }
        Err(refusal_output) => emit_org_refusal(
            refusal_output,
            true,
            matches!(run.emit, OrgEmitMode::Summary),
        ),
    }
}

fn run_org_block_command(block: &OrgBlockCli) -> Result<u8, Box<dyn Error>> {
    match run_org_block_pipeline(block) {
        Ok(records) => {
            let output = match block.emit {
                OrgStreamEmitMode::Jsonl => org::output::emit_block_jsonl(&records)?,
                OrgStreamEmitMode::Summary => org::output::render_block_summary(&records),
            };
            emit_org_output(&output, matches!(block.emit, OrgStreamEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_org_refusal(
            refusal_output,
            true,
            matches!(block.emit, OrgStreamEmitMode::Summary),
        ),
    }
}

fn run_org_edge_command(edge: &OrgEdgeCli) -> Result<u8, Box<dyn Error>> {
    match run_org_edge_pipeline(edge) {
        Ok(records) => {
            let output = match edge.emit {
                OrgStreamEmitMode::Jsonl => org::output::emit_edge_jsonl(&records)?,
                OrgStreamEmitMode::Summary => org::output::render_edge_summary(&records),
            };
            emit_org_output(&output, matches!(edge.emit, OrgStreamEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_org_refusal(
            refusal_output,
            true,
            matches!(edge.emit, OrgStreamEmitMode::Summary),
        ),
    }
}

fn run_org_solve_command(solve: &OrgSolveCli) -> Result<u8, Box<dyn Error>> {
    match run_org_solve_pipeline(solve) {
        Ok(artifact) => {
            let output = match solve.emit {
                OrgEmitMode::Json => org::output::emit_solve_json(&artifact)?,
                OrgEmitMode::Summary => org::output::render_solve_summary(&artifact),
            };
            emit_org_output(&output, matches!(solve.emit, OrgEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_org_refusal(
            refusal_output,
            true,
            matches!(solve.emit, OrgEmitMode::Summary),
        ),
    }
}

fn run_org_audit_command(audit: &OrgAuditCli) -> Result<u8, Box<dyn Error>> {
    match run_org_audit_pipeline(audit) {
        Ok(artifact) => {
            let output = match audit.emit {
                OrgEmitMode::Json => org::output::emit_audit_json(&artifact)?,
                OrgEmitMode::Summary => org::output::render_audit_summary(&artifact),
            };
            emit_org_output(&output, matches!(audit.emit, OrgEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_org_refusal(
            refusal_output,
            true,
            matches!(audit.emit, OrgEmitMode::Summary),
        ),
    }
}

fn run_org_promote_command(promote: &OrgPromoteCli) -> Result<u8, Box<dyn Error>> {
    match run_org_promote_pipeline(promote) {
        Ok(artifact) => {
            let output = match promote.emit {
                OrgEmitMode::Json => org::output::emit_promote_json(&artifact)?,
                OrgEmitMode::Summary => org::output::render_promote_summary(&artifact),
            };
            emit_org_output(&output, matches!(promote.emit, OrgEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_org_refusal(
            refusal_output,
            true,
            matches!(promote.emit, OrgEmitMode::Summary),
        ),
    }
}

fn run_org_review_command(review: &OrgReviewCommand) -> Result<u8, Box<dyn Error>> {
    match &review.command {
        OrgReviewSubcommand::Export(export) => run_org_review_export_command(export),
        OrgReviewSubcommand::Import(import) => run_org_review_import_command(import),
    }
}

fn run_org_review_export_command(export: &OrgReviewExportCli) -> Result<u8, Box<dyn Error>> {
    match run_org_review_export_pipeline(export) {
        Ok(artifact) => {
            let output = match export.emit {
                OrgReviewExportEmitMode::Json => Ok(serde_json::to_string(&artifact)?),
                OrgReviewExportEmitMode::Csv => {
                    org::review::export_csv(&artifact).map_err(create_org_refusal)
                }
            };
            match output {
                Ok(output) => {
                    emit_org_output(&output, false);
                    Ok(0)
                }
                Err(refusal_output) => emit_org_refusal(
                    refusal_output,
                    matches!(export.emit, OrgReviewExportEmitMode::Json),
                    false,
                ),
            }
        }
        Err(refusal_output) => emit_org_refusal(
            refusal_output,
            matches!(export.emit, OrgReviewExportEmitMode::Json),
            false,
        ),
    }
}

fn run_org_review_import_command(import: &OrgReviewImportCli) -> Result<u8, Box<dyn Error>> {
    match run_org_review_import_pipeline(import) {
        Ok(artifact) => {
            let output = match import.emit {
                OrgEmitMode::Json => serde_json::to_string(&artifact)?,
                OrgEmitMode::Summary => artifact.render_summary(),
            };
            emit_org_output(&output, matches!(import.emit, OrgEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_org_refusal(
            refusal_output,
            true,
            matches!(import.emit, OrgEmitMode::Summary),
        ),
    }
}

fn run_org_explain_command(explain: &OrgExplainCli) -> Result<u8, Box<dyn Error>> {
    match run_org_explain_pipeline(explain) {
        Ok(artifact) => {
            let output = match explain.emit {
                OrgEmitMode::Json => org::output::emit_explain_json(&artifact)?,
                OrgEmitMode::Summary => org::output::render_explain_summary(&artifact),
            };
            emit_org_output(&output, matches!(explain.emit, OrgEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_org_refusal(
            refusal_output,
            true,
            matches!(explain.emit, OrgEmitMode::Summary),
        ),
    }
}

fn run_registry_diff(diff: &RegistryDiffCli) -> Result<u8, Box<dyn Error>> {
    let result = registry::diff_registries(&diff.old, &diff.new).map_err(|error| {
        if error.is_mismatched_id {
            create_registry_id_mismatch_refusal(error)
        } else {
            create_registry_refusal(error.source)
        }
    });

    match result {
        Ok(output) => {
            match diff.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(0)
        }
        Err(refusal_output) => {
            match diff.emit {
                RegistryEmitMode::Json => {
                    println!("{}", serde_json::to_string(&refusal_output)?);
                }
                RegistryEmitMode::Summary => {
                    eprintln!("{}", serde_json::to_string(&refusal_output)?);
                }
            }
            Ok(2)
        }
    }
}

fn run_registry_audit(audit: &RegistryAuditCli) -> Result<u8, Box<dyn Error>> {
    let result = run_registry_audit_pipeline(audit);

    match result {
        Ok(output) => {
            match audit.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(0)
        }
        Err(refusal_output) => {
            match audit.emit {
                RegistryEmitMode::Json => {
                    println!("{}", serde_json::to_string(&refusal_output)?);
                }
                RegistryEmitMode::Summary => {
                    eprintln!("{}", serde_json::to_string(&refusal_output)?);
                }
            }
            Ok(2)
        }
    }
}

fn run_registry_build(build: &RegistryBuildCli) -> Result<u8, Box<dyn Error>> {
    let result = run_registry_build_pipeline(build);

    match result {
        Ok(output) => {
            println!("{}", serde_json::to_string(&output)?);
            if !output.failures.is_empty() {
                eprintln!(
                    "Warning: registry build completed with {} provider failure(s); see {}/_build.json for details",
                    output.failures.len(),
                    output.output_path,
                );
            }
            Ok(0)
        }
        Err(refusal_output) => {
            println!("{}", serde_json::to_string(&refusal_output)?);
            Ok(2)
        }
    }
}

fn run_registry_lint(lint: &RegistryLintCli) -> Result<u8, Box<dyn Error>> {
    let profile = match lint.profile {
        RegistryLintProfile::Standard => registry_lint::RegistryLintProfile::Standard,
        RegistryLintProfile::Org => registry_lint::RegistryLintProfile::Org,
        RegistryLintProfile::Strategy => registry_lint::RegistryLintProfile::Strategy,
        RegistryLintProfile::Auto => registry_lint::RegistryLintProfile::Auto,
    };

    match registry_lint::lint(&lint.registry, profile) {
        Ok(output) => {
            match lint.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(0)
        }
        Err(refusal) => {
            let output = refusal.to_canon_output();
            match lint.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => eprintln!("{}", serde_json::to_string(&output)?),
            }
            Ok(2)
        }
    }
}

#[allow(clippy::result_large_err)]
fn run_org_run_pipeline(run: &OrgRunCli) -> Result<OrgRunExecution, CanonOutput> {
    let strategy = org::strategy::load_strategy(&run.strategy).map_err(create_org_refusal)?;
    let incumbent =
        org::incumbent::load_incumbent_memory(&run.registry).map_err(create_org_refusal)?;
    let observations = org::projection::project_input(&run.rows, &strategy, None, None)
        .map_err(create_org_refusal)?;
    let blocks = org::block::build_candidate_blocks(&strategy, &observations, &incumbent)
        .map_err(create_org_refusal)?;
    let edges = org::edge::build_edges(&strategy, &observations, &blocks, &incumbent)
        .map_err(create_org_refusal)?;
    let artifact = org::solve::run(&strategy, &observations, &edges, &incumbent)
        .map_err(create_org_refusal)?;

    Ok(OrgRunExecution {
        artifact,
        candidate_pairs: edges.len() as u64,
    })
}

#[allow(clippy::result_large_err)]
fn run_org_block_pipeline(block: &OrgBlockCli) -> Result<Vec<org::BlockRecord>, CanonOutput> {
    let strategy = org::strategy::load_strategy(&block.strategy).map_err(create_org_refusal)?;
    let incumbent =
        org::incumbent::load_incumbent_memory(&block.registry).map_err(create_org_refusal)?;
    let observations = org::projection::project_input(&block.rows, &strategy, None, None)
        .map_err(create_org_refusal)?;

    org::block::build_candidate_blocks(&strategy, &observations, &incumbent)
        .map_err(create_org_refusal)
}

#[allow(clippy::result_large_err)]
fn run_org_edge_pipeline(edge: &OrgEdgeCli) -> Result<Vec<org::EdgeRecord>, CanonOutput> {
    let strategy = org::strategy::load_strategy(&edge.strategy).map_err(create_org_refusal)?;
    let incumbent =
        org::incumbent::load_incumbent_memory(&edge.registry).map_err(create_org_refusal)?;
    let observations = org::projection::project_input(&edge.rows, &strategy, None, None)
        .map_err(create_org_refusal)?;
    let candidates = read_jsonl_artifact(&edge.candidates, "candidate block artifact")?;

    org::edge::build_edges(&strategy, &observations, &candidates, &incumbent)
        .map_err(create_org_refusal)
}

#[allow(clippy::result_large_err)]
fn run_org_solve_pipeline(solve: &OrgSolveCli) -> Result<org::SolveRunArtifact, CanonOutput> {
    let strategy = org::strategy::load_strategy(&solve.strategy).map_err(create_org_refusal)?;
    let incumbent =
        org::incumbent::load_incumbent_memory(&solve.registry).map_err(create_org_refusal)?;
    let observations = org::projection::project_input(&solve.rows, &strategy, None, None)
        .map_err(create_org_refusal)?;
    let edges = read_jsonl_artifact(&solve.edges, "edge artifact")?;

    org::solve::solve(&strategy, &observations, &edges, &incumbent).map_err(create_org_refusal)
}

#[allow(clippy::result_large_err)]
fn run_org_audit_pipeline(audit: &OrgAuditCli) -> Result<org::AuditArtifact, CanonOutput> {
    let (result, result_bytes): (org::SolveRunArtifact, Vec<u8>) =
        read_json_artifact(&audit.result, "org result artifact")?;

    org::audit::audit(
        &result,
        &result_bytes,
        org::audit::AuditContext {
            suite_dir: &audit.suite,
            profile: ORG_V1_PROFILE,
            budget_usage: org::audit::AuditBudgetUsage {
                runtime_seconds: 0,
                candidate_pairs: 0,
            },
            baseline: None,
            promoted_with_prior_escrow_count: 0,
        },
    )
    .map_err(create_org_refusal)
}

#[allow(clippy::result_large_err)]
fn run_org_promote_pipeline(promote: &OrgPromoteCli) -> Result<org::PromoteArtifact, CanonOutput> {
    let (result, result_bytes): (org::SolveRunArtifact, Vec<u8>) =
        read_json_artifact(&promote.result, "org result artifact")?;
    let (audit, audit_bytes): (org::AuditArtifact, Vec<u8>) =
        read_json_artifact(&promote.audit, "org audit artifact")?;

    org::promote::promote(
        &result,
        &result_bytes,
        &audit,
        &audit_bytes,
        &promote.registry,
        &promote.next_version,
    )
    .map_err(create_org_refusal)
}

#[allow(clippy::result_large_err)]
fn run_org_review_export_pipeline(
    export: &OrgReviewExportCli,
) -> Result<org::review::ReviewExportOutput, CanonOutput> {
    let (result, result_bytes): (org::SolveRunArtifact, Vec<u8>) =
        read_json_artifact(&export.result, "org result artifact")?;

    org::review::export(
        &result,
        &result_bytes,
        map_org_review_include(&export.include),
    )
    .map_err(create_org_refusal)
}

#[allow(clippy::result_large_err)]
fn run_org_review_import_pipeline(
    import: &OrgReviewImportCli,
) -> Result<org::review::ReviewImportOutput, CanonOutput> {
    let review_bytes = read_artifact_bytes(&import.review, "org review artifact")?;
    let audit_data = import
        .audit
        .as_ref()
        .map(|audit_path| {
            read_json_artifact::<org::AuditArtifact>(audit_path, "org audit artifact")
        })
        .transpose()?;
    let audit = audit_data
        .as_ref()
        .map(|(audit, bytes)| (audit, bytes.as_slice()));

    org::review::import(
        &import.review,
        &review_bytes,
        &import.registry,
        &import.next_version,
        audit,
    )
    .map_err(create_org_refusal)
}

fn map_org_review_include(include: &OrgReviewInclude) -> org::review::ReviewInclude {
    match include {
        OrgReviewInclude::Resolved => org::review::ReviewInclude::Resolved,
        OrgReviewInclude::Escrow => org::review::ReviewInclude::Escrow,
        OrgReviewInclude::Contradictions => org::review::ReviewInclude::Contradictions,
        OrgReviewInclude::All => org::review::ReviewInclude::All,
    }
}

#[allow(clippy::result_large_err)]
fn run_org_explain_pipeline(explain: &OrgExplainCli) -> Result<org::ExplainArtifact, CanonOutput> {
    let (result, _result_bytes): (org::SolveRunArtifact, Vec<u8>) =
        read_json_artifact(&explain.result, "org result artifact")?;
    let query = org::ExplainQuery {
        row_id: explain.row.clone(),
        canonical_id: explain.canon_id.clone(),
        escrow_id: explain.escrow_id.clone(),
    };

    org::explain::explain_from_result(query, &result).map_err(create_org_refusal)
}

fn emit_org_output(output: &str, summary_mode: bool) {
    if summary_mode {
        println!("{output}");
    } else {
        print!("{output}");
    }
}

fn emit_org_refusal(
    refusal_output: CanonOutput,
    structured_stdout: bool,
    summary_mode: bool,
) -> Result<u8, Box<dyn Error>> {
    let refusal_json = serde_json::to_string(&refusal_output)?;
    if structured_stdout && !summary_mode {
        println!("{refusal_json}");
    } else {
        eprintln!("{refusal_json}");
    }
    Ok(2)
}

fn append_org_run_witness(
    run: &OrgRunCli,
    artifact: &org::SolveRunArtifact,
    output: &str,
    runtime_seconds: u64,
    candidate_pairs: Option<u64>,
) {
    if run.no_witness {
        return;
    }

    let mut inputs = vec![witness::WitnessInput {
        path: run.rows.display().to_string(),
        hash: hash_input_path(&run.rows),
        bytes: input_size(&run.rows),
    }];
    inputs.push(witness::WitnessInput {
        path: run.strategy.display().to_string(),
        hash: hash_input_path(&run.strategy),
        bytes: input_size(&run.strategy),
    });

    let mut params = serde_json::Map::new();
    params.insert(
        "subcommand".to_string(),
        serde_json::Value::String("org.run".to_string()),
    );
    params.insert(
        "strategy_id".to_string(),
        serde_json::Value::String(artifact.strategy.id.clone()),
    );
    params.insert(
        "strategy_version".to_string(),
        serde_json::Value::String(artifact.strategy.version.clone()),
    );
    params.insert(
        "strategy_path".to_string(),
        serde_json::Value::String(run.strategy.display().to_string()),
    );
    params.insert(
        "registry_id".to_string(),
        serde_json::Value::String(artifact.registry.id.clone()),
    );
    params.insert(
        "registry_version".to_string(),
        serde_json::Value::String(artifact.registry.version.clone()),
    );
    params.insert(
        "registry_path".to_string(),
        serde_json::Value::String(run.registry.display().to_string()),
    );
    params.insert(
        "registry_lookup_snapshot_hash".to_string(),
        serde_json::Value::String(artifact.registry.lookup_snapshot_hash.clone()),
    );
    params.insert(
        "registry_escrow_snapshot_hash".to_string(),
        serde_json::Value::String(artifact.registry.escrow_snapshot_hash.clone()),
    );
    params.insert(
        "emit".to_string(),
        serde_json::Value::String(
            match run.emit {
                OrgEmitMode::Json => "json",
                OrgEmitMode::Summary => "summary",
            }
            .to_string(),
        ),
    );
    params.insert(
        "summary".to_string(),
        serde_json::json!({
            "observations": artifact.summary.observations,
            "resolved_existing": artifact.summary.resolved_existing,
            "promotable_new": artifact.summary.promotable_new,
            "abstain_low_evidence": artifact.summary.abstain_low_evidence,
            "abstain_conflict": artifact.summary.abstain_conflict,
            "contradictions": artifact.contradictions.len(),
        }),
    );
    params.insert(
        "runtime_seconds".to_string(),
        serde_json::Value::from(runtime_seconds),
    );

    if let Some(suite_dir) = &run.suite {
        params.insert(
            "suite_path".to_string(),
            serde_json::Value::String(suite_dir.display().to_string()),
        );
    }
    if let Some(candidate_pairs) = candidate_pairs {
        params.insert(
            "candidate_pairs".to_string(),
            serde_json::Value::from(candidate_pairs),
        );
    }

    let witness_record = witness::WitnessRecord::new(
        inputs,
        params,
        &witness::hash_bytes(output.as_bytes()),
        "RESOLVED",
        0,
    );
    if let Err(error) = witness::append_witness_record(&witness_record, false) {
        eprintln!("Warning: failed to append witness: {}", error);
    }
}

fn hash_input_path(path: &Path) -> Option<String> {
    if path == Path::new("-") {
        None
    } else {
        witness::hash_file(path).ok()
    }
}

#[allow(clippy::result_large_err)]
fn read_json_artifact<T: DeserializeOwned>(
    path: &Path,
    label: &str,
) -> Result<(T, Vec<u8>), CanonOutput> {
    let bytes = read_artifact_bytes(path, label)?;
    let value = serde_json::from_slice(&bytes).map_err(|error| {
        refusal::create_refusal(
            RefusalCode::EParse,
            format!("Failed to parse {} '{}': {}", label, path.display(), error),
            serde_json::json!({
                "path": path.display().to_string(),
                "artifact": label,
                "error": error.to_string(),
            }),
            None,
        )
    })?;
    Ok((value, bytes))
}

#[allow(clippy::result_large_err)]
fn read_jsonl_artifact<T: DeserializeOwned>(
    path: &Path,
    label: &str,
) -> Result<Vec<T>, CanonOutput> {
    let bytes = read_artifact_bytes(path, label)?;
    let text = std::str::from_utf8(&bytes).map_err(|error| {
        refusal::create_refusal(
            RefusalCode::EParse,
            format!(
                "{} '{}' must be valid UTF-8 JSONL: {}",
                label,
                path.display(),
                error
            ),
            serde_json::json!({
                "path": path.display().to_string(),
                "artifact": label,
                "error": error.to_string(),
            }),
            None,
        )
    })?;

    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line).map_err(|error| {
                refusal::create_refusal(
                    RefusalCode::EParse,
                    format!(
                        "Failed to parse {} '{}' as JSONL: {}",
                        label,
                        path.display(),
                        error
                    ),
                    serde_json::json!({
                        "path": path.display().to_string(),
                        "artifact": label,
                        "line": line,
                        "error": error.to_string(),
                    }),
                    None,
                )
            })
        })
        .collect()
}

#[allow(clippy::result_large_err)]
fn read_artifact_bytes(path: &Path, label: &str) -> Result<Vec<u8>, CanonOutput> {
    std::fs::read(path).map_err(|error| {
        refusal::create_refusal(
            RefusalCode::EIo,
            format!("Failed to read {} '{}': {}", label, path.display(), error),
            serde_json::json!({
                "path": path.display().to_string(),
                "artifact": label,
                "error": error.to_string(),
            }),
            None,
        )
    })
}

#[allow(clippy::result_large_err)]
fn run_registry_audit_pipeline(
    audit: &RegistryAuditCli,
) -> Result<RegistryAuditOutput, CanonOutput> {
    let registry = registry::load_registry(&audit.registry).map_err(create_registry_refusal)?;
    let input_values =
        input::parse_input(&audit.seed, &audit.column, audit.max_bytes, audit.max_rows)
            .map_err(create_input_refusal)?;
    let resolve_result =
        lookup::resolve_values(&registry, &input_values).map_err(create_lookup_refusal)?;

    Ok(RegistryAuditOutput::from_resolve_result(
        &audit.seed,
        &audit.column,
        &registry.meta,
        &resolve_result,
    ))
}

#[allow(clippy::result_large_err)]
fn run_registry_build_pipeline(
    build: &RegistryBuildCli,
) -> Result<RegistryBuildOutput, CanonOutput> {
    let provider_options = parse_provider_config(&build.provider_config)?;
    let input_values = input::parse_input(
        &build.seed,
        &build.seed_column,
        build.max_bytes,
        build.max_rows,
    )
    .map_err(create_input_refusal)?;

    let seed_hash = if build.seed == Path::new("-") {
        input_values.source_hash.clone().ok_or_else(|| {
            create_io_refusal(std::io::Error::other(
                "Failed to hash stdin seed bytes during parsing",
            ))
        })?
    } else {
        witness::hash_file(&build.seed).map_err(|error| {
            create_io_refusal(std::io::Error::other(format!(
                "Failed to hash seed file: {}",
                error
            )))
        })?
    };

    let mut identifiers = input_values.values.keys().cloned().collect::<Vec<_>>();
    identifiers.sort();

    let mut special_reasons = input_values
        .special
        .iter()
        .map(|(reason, count)| (reason.to_string(), *count))
        .collect::<Vec<_>>();
    special_reasons.sort_by(|left, right| left.0.cmp(&right.0));

    registry::build_registry(&registry::RegistryBuildRequest {
        source: build.source.clone(),
        seed_path: build.seed.clone(),
        seed_column: build.seed_column.clone(),
        output_dir: build.output.clone(),
        version: build.version.clone(),
        incremental: build.incremental,
        identifiers,
        seed_hash,
        special_reasons,
        batch_size: build.batch_size,
        rate_limit_ms: build.rate_limit_ms,
        provider_options,
    })
    .map_err(create_registry_build_refusal)
}

#[allow(clippy::result_large_err)]
fn parse_provider_config(options: &[String]) -> Result<BTreeMap<String, String>, CanonOutput> {
    let mut parsed = BTreeMap::new();

    for option in options {
        let (raw_key, raw_value) = option.split_once('=').ok_or_else(|| {
            refusal::create_refusal(
                RefusalCode::EParse,
                format!("Invalid --provider-config '{}'; expected KEY=VALUE", option),
                serde_json::json!({ "provider_config": option }),
                None,
            )
        })?;

        let key = raw_key.trim();
        if key.is_empty() {
            return Err(refusal::create_refusal(
                RefusalCode::EParse,
                format!(
                    "Invalid --provider-config '{}'; key cannot be empty",
                    option
                ),
                serde_json::json!({ "provider_config": option }),
                None,
            ));
        }

        if parsed
            .insert(key.to_string(), raw_value.to_string())
            .is_some()
        {
            return Err(refusal::create_refusal(
                RefusalCode::EParse,
                format!("Duplicate --provider-config key '{}'", key),
                serde_json::json!({ "provider_config_key": key }),
                None,
            ));
        }
    }

    Ok(parsed)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayMode {
    Version,
    Describe,
    Schema,
}

pub fn detect_display_mode<I, T>(args: I) -> Option<DisplayMode>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<OsString>>();
    let first_arg = args.get(1).and_then(|arg| arg.to_str());

    match first_arg {
        Some("--version") => Some(DisplayMode::Version),
        Some("--describe") => Some(DisplayMode::Describe),
        Some("--schema") => Some(DisplayMode::Schema),
        _ => None,
    }
}

pub fn run_display_mode(mode: DisplayMode) -> Result<u8, Box<dyn Error>> {
    match mode {
        DisplayMode::Version => {
            println!("canon {}", env!("CARGO_PKG_VERSION"));
        }
        DisplayMode::Describe => {
            const OPERATOR_JSON: &str = include_str!("../operator.json");
            println!("{OPERATOR_JSON}");
        }
        DisplayMode::Schema => {
            let schema = serde_json::json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "$id": "https://canon.v0/schema.json",
                "title": "Canon Output Schema",
                "description": "JSON schema for canon.v0 output format",
                "type": "object",
                "required": ["version", "outcome"],
                "properties": {
                    "version": {
                        "type": "string",
                        "const": "canon.v0"
                    },
                    "outcome": {
                        "type": "string",
                        "enum": ["RESOLVED", "PARTIAL", "UNRESOLVED", "REFUSAL"]
                    },
                    "registry": {
                        "type": ["object", "null"],
                        "properties": {
                            "id": { "type": "string" },
                            "version": { "type": "string" },
                            "source": { "type": "string" }
                        },
                        "required": ["id", "version", "source"]
                    },
                    "summary": {
                        "type": ["object", "null"],
                        "properties": {
                            "total": { "type": "integer", "minimum": 0 },
                            "resolved": { "type": "integer", "minimum": 0 },
                            "unresolved": { "type": "integer", "minimum": 0 }
                        },
                        "required": ["total", "resolved", "unresolved"]
                    },
                    "mappings": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "input": { "type": "string" },
                                "canonical_id": { "type": "string" },
                                "canonical_type": { "type": "string" },
                                "rule_id": { "type": "string" },
                                "confidence": { "type": "string" }
                            },
                            "required": ["input", "canonical_id", "canonical_type", "rule_id", "confidence"]
                        }
                    },
                    "unresolved": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "input": { "type": ["string", "null"] },
                                "reason": { "type": "string" }
                            },
                            "required": ["reason"]
                        }
                    },
                    "refusal": {
                        "type": ["object", "null"],
                        "properties": {
                            "code": { "type": "string" },
                            "message": { "type": "string" },
                            "detail": { "type": "object" },
                            "next_command": { "type": ["string", "null"] }
                        },
                        "required": ["code", "message", "detail"]
                    }
                }
            });
            println!("{}", serde_json::to_string_pretty(&schema)?);
        }
    }

    Ok(0)
}

// Internal pipeline that can return refusals
#[allow(clippy::result_large_err)]
fn run_pipeline(
    input_path: &Path,
    registry_path: &Path,
    column: &str,
    cli: &Cli,
) -> Result<u8, CanonOutput> {
    // Step 4: Load registry
    let registry = registry::load_registry(registry_path).map_err(create_registry_refusal)?;
    let input_path_display = input_path.to_string_lossy().into_owned();

    // Step 5: Parse input
    let input_values = input::parse_input(input_path, column, cli.max_bytes, cli.max_rows)
        .map_err(create_input_refusal)?;

    // Step 6: Hash input bytes for witness
    let (input_hash, input_bytes) = if input_path == Path::new("-") {
        let hash = input_values.source_hash.clone().ok_or_else(|| {
            create_io_refusal(std::io::Error::other(
                "Failed to hash stdin input bytes during parsing",
            ))
        })?;
        (hash, input_values.source_bytes)
    } else {
        let hash = witness::hash_file(input_path).map_err(|e| {
            create_io_refusal(std::io::Error::other(format!(
                "Failed to hash input file: {}",
                e
            )))
        })?;
        (hash, input_size(input_path))
    };

    // Step 7: Validate emit mode
    if matches!(cli.emit, crate::cli::EmitMode::Csv)
        && matches!(input_values.format, InputFormat::Jsonl)
    {
        return Err(refusal::create_refusal(
            RefusalCode::EEmitFormat,
            "--emit csv cannot be used with JSONL input".to_string(),
            serde_json::json!({"input_format": "jsonl", "emit_mode": "csv"}),
            Some("Use --emit json with JSONL input".to_string()),
        ));
    }

    // Step 8: Resolve values
    let resolve_result =
        lookup::resolve_values(&registry, &input_values).map_err(create_lookup_refusal)?;

    // Step 9: Determine outcome
    let outcome = determine_outcome(&resolve_result.summary);

    // Debug assert for safety net
    debug_assert!(
        !(resolve_result.summary.resolved == 0 && resolve_result.summary.unresolved == 0),
        "Empty input should have been caught by input module"
    );

    // Step 10: Emit output
    let output_hash = match cli.emit {
        crate::cli::EmitMode::Json => {
            // JSON mode: emit to stdout with hash
            let json_output =
                output::json::emit_json_explicit(&registry.meta, &resolve_result, cli.explicit)
                    .map_err(create_output_refusal)?;

            print!("{}", json_output);

            // Step 11: Hash output bytes (witness protocol)
            witness::hash_bytes(json_output.as_bytes())
        }
        crate::cli::EmitMode::Csv => {
            // CSV mode: create resolve map and emit with hash
            let resolve_map = build_resolve_map(&resolve_result);
            let default_canonical_column = format!("{}__canon", column);
            let canonical_column = cli
                .canon_column
                .as_deref()
                .unwrap_or(default_canonical_column.as_str());

            let stdout = std::io::stdout();
            let mut stdout_lock = stdout.lock();
            let mut tee_writer = HashingWriter::new(&mut stdout_lock);
            output::csv::emit_csv(
                input_path,
                &resolve_map,
                column,
                canonical_column,
                input_values.delimiter.unwrap_or(b','),
                &mut tee_writer,
            )
            .map_err(create_csv_output_refusal)?;

            tee_writer.flush().map_err(create_io_refusal)?;

            // Write --map-out sidecar if specified
            if let Some(map_out_path) = &cli.map_out {
                let json_output =
                    output::json::emit_json_explicit(&registry.meta, &resolve_result, cli.explicit)
                        .map_err(create_output_refusal)?;
                std::fs::write(map_out_path, json_output).map_err(create_io_refusal)?;
            }

            // Step 11: Hash output bytes (witness protocol)
            tee_writer.finalize_hash()
        }
    };

    // Step 12: Record witness (unless --no-witness)
    let exit_code = match outcome {
        Outcome::Resolved => 0,
        Outcome::Partial | Outcome::Unresolved => 1,
        Outcome::Refusal => 2, // Should not reach here
    };

    let no_witness = cli.no_witness;
    if !no_witness {
        let witness_summary = witness::WitnessSummary {
            total: resolve_result.summary.total,
            resolved: resolve_result.summary.resolved,
            unresolved: resolve_result.summary.unresolved,
        };

        let outcome_str = match outcome {
            Outcome::Resolved => "RESOLVED",
            Outcome::Partial => "PARTIAL",
            Outcome::Unresolved => "UNRESOLVED",
            Outcome::Refusal => "REFUSAL",
        };

        let mut params = serde_json::Map::new();
        params.insert(
            "input_path".to_string(),
            serde_json::Value::String(input_path_display.clone()),
        );
        params.insert(
            "registry_id".to_string(),
            serde_json::Value::String(registry.meta.id.clone()),
        );
        params.insert(
            "registry_version".to_string(),
            serde_json::Value::String(registry.meta.version.clone()),
        );
        params.insert(
            "column".to_string(),
            serde_json::Value::String(column.to_string()),
        );
        params.insert(
            "emit".to_string(),
            serde_json::Value::String(
                match cli.emit {
                    crate::cli::EmitMode::Json => "json",
                    crate::cli::EmitMode::Csv => "csv",
                }
                .to_string(),
            ),
        );
        params.insert(
            "explicit".to_string(),
            serde_json::Value::Bool(cli.explicit),
        );
        params.insert(
            "summary".to_string(),
            serde_json::json!({
                "total": witness_summary.total,
                "resolved": witness_summary.resolved,
                "unresolved": witness_summary.unresolved
            }),
        );
        if let Some(canon_column) = &cli.canon_column {
            params.insert(
                "canon_column".to_string(),
                serde_json::Value::String(canon_column.clone()),
            );
        }
        if let Some(map_out) = &cli.map_out {
            params.insert(
                "map_out".to_string(),
                serde_json::Value::String(map_out.display().to_string()),
            );
        }
        if let Some(max_rows) = cli.max_rows {
            params.insert(
                "max_rows".to_string(),
                serde_json::Value::from(max_rows as u64),
            );
        }
        if let Some(max_bytes) = cli.max_bytes {
            params.insert("max_bytes".to_string(), serde_json::Value::from(max_bytes));
        }

        let witness_record = witness::WitnessRecord::new(
            vec![witness::WitnessInput {
                path: input_path_display.clone(),
                hash: Some(input_hash.clone()),
                bytes: input_bytes,
            }],
            params,
            &output_hash,
            outcome_str,
            exit_code,
        );

        if let Err(error) = witness::append_witness_record(&witness_record, no_witness) {
            eprintln!("Warning: failed to append witness: {}", error);
        }
    }

    // Step 13: Return exit code
    Ok(exit_code)
}

fn determine_outcome(summary: &Summary) -> Outcome {
    match (summary.resolved, summary.unresolved) {
        (resolved, 0) if resolved > 0 => Outcome::Resolved,
        (resolved, unresolved) if resolved > 0 && unresolved > 0 => Outcome::Partial,
        (0, unresolved) if unresolved > 0 => Outcome::Unresolved,
        _ => {
            debug_assert!(false, "Invalid summary state");
            Outcome::Unresolved
        }
    }
}

fn build_resolve_map(
    resolve_result: &ResolveResult,
) -> std::collections::HashMap<String, Option<String>> {
    let mut resolve_map = std::collections::HashMap::new();

    // Add resolved mappings
    for mapping in &resolve_result.mappings {
        resolve_map.insert(mapping.input.clone(), Some(mapping.canonical_id.clone()));
    }

    // Add unresolved entries that have input values
    for unresolved in &resolve_result.unresolved {
        if let Some(input_value) = &unresolved.input {
            resolve_map.insert(input_value.clone(), None);
        }
    }

    resolve_map
}

fn input_size(path: &Path) -> Option<u64> {
    if path == Path::new("-") {
        return None;
    }

    std::fs::metadata(path).ok().map(|metadata| metadata.len())
}

struct HashingWriter<W: Write> {
    writer: W,
    hasher: blake3::Hasher,
}

impl<W: Write> HashingWriter<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            hasher: blake3::Hasher::new(),
        }
    }

    fn finalize_hash(self) -> String {
        format!("blake3:{}", self.hasher.finalize().to_hex())
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let bytes_written = self.writer.write(buf)?;
        self.hasher.update(&buf[..bytes_written]);
        Ok(bytes_written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

// Helper functions to create refusal outputs from errors
fn create_registry_refusal(error: Box<dyn Error>) -> CanonOutput {
    let message = error.to_string();
    let code = if message.contains("Registry directory not found") {
        RefusalCode::EIo
    } else {
        RefusalCode::EBadRegistry
    };
    refusal::create_refusal(code, message, serde_json::json!({}), None)
}

fn create_registry_id_mismatch_refusal(error: registry::RegistryDiffError) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EBadRegistry,
        error.source.to_string(),
        serde_json::json!({
            "old": {
                "path": error.old_path.display().to_string(),
                "id": error.old_id,
            },
            "new": {
                "path": error.new_path.display().to_string(),
                "id": error.new_id,
            }
        }),
        Some("Compare two versions of the same registry id".to_string()),
    )
}

fn create_registry_build_refusal(error: registry::RegistryBuildError) -> CanonOutput {
    let code = match error.kind {
        registry::RegistryBuildErrorKind::Io => RefusalCode::EIo,
        registry::RegistryBuildErrorKind::BadRegistry => RefusalCode::EBadRegistry,
        registry::RegistryBuildErrorKind::Parse => RefusalCode::EParse,
    };
    refusal::create_refusal(code, error.message, error.detail, None)
}

fn create_input_refusal(error: input::InputError) -> CanonOutput {
    match error {
        input::InputError::ColumnNotFound { column, available } => refusal::create_refusal(
            RefusalCode::EColumnNotFound,
            format!("Column '{}' not found in input file", column),
            serde_json::json!({
                "column": column,
                "available_columns": available
            }),
            None,
        ),
        input::InputError::TooLarge {
            limit_type,
            limit,
            actual,
        } => refusal::create_refusal(
            RefusalCode::ETooLarge,
            format!(
                "Input exceeds --{} limit ({} > {})",
                limit_type, actual, limit
            ),
            serde_json::json!({
                "limit_type": limit_type,
                "limit": limit,
                "actual": actual
            }),
            None,
        ),
        input::InputError::Io(message) => {
            refusal::create_refusal(RefusalCode::EIo, message, serde_json::json!({}), None)
        }
        input::InputError::Parse(message) => {
            refusal::create_refusal(RefusalCode::EParse, message, serde_json::json!({}), None)
        }
        input::InputError::CsvParse(message) => {
            refusal::create_refusal(RefusalCode::ECsvParse, message, serde_json::json!({}), None)
        }
        input::InputError::Encoding(message) => {
            refusal::create_refusal(RefusalCode::EEncoding, message, serde_json::json!({}), None)
        }
        input::InputError::EmptyInput => refusal::create_refusal(
            RefusalCode::EEmptyInput,
            "Input has no processable rows".to_string(),
            serde_json::json!({}),
            None,
        ),
    }
}

fn create_lookup_refusal(error: lookup::LookupError) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EIo,
        error.to_string(),
        serde_json::json!({}),
        None,
    )
}

fn create_output_refusal(error: Box<dyn Error>) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EIo,
        error.to_string(),
        serde_json::json!({}),
        None,
    )
}

fn create_csv_output_refusal(error: output::csv::CsvOutputError) -> CanonOutput {
    match error {
        output::csv::CsvOutputError::Io(message) => {
            refusal::create_refusal(RefusalCode::EIo, message, serde_json::json!({}), None)
        }
        output::csv::CsvOutputError::CsvParse(message) => {
            refusal::create_refusal(RefusalCode::ECsvParse, message, serde_json::json!({}), None)
        }
        output::csv::CsvOutputError::ColumnExists { column } => refusal::create_refusal(
            RefusalCode::EColumnExists,
            format!("Canonical column '{}' already exists in CSV header", column),
            serde_json::json!({ "canon_column": column }),
            None,
        ),
        output::csv::CsvOutputError::ColumnNotFound { column, available } => {
            refusal::create_refusal(
                RefusalCode::EColumnNotFound,
                format!("Column '{}' not found", column),
                serde_json::json!({ "column": column, "available_columns": available }),
                None,
            )
        }
    }
}

fn create_io_refusal(error: std::io::Error) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EIo,
        error.to_string(),
        serde_json::json!({}),
        None,
    )
}

fn create_org_refusal(error: org::OrgError) -> CanonOutput {
    let detail = error.detail.unwrap_or_else(|| serde_json::json!({}));
    let message = error.message;

    match error.code {
        org::OrgErrorCode::InputContract => {
            Refusal::org_input_contract(message, detail).to_canon_output()
        }
        org::OrgErrorCode::Strategy => {
            Refusal::org_bad_strategy(message, detail).to_canon_output()
        }
        org::OrgErrorCode::Audit => {
            let lowercase = message.to_ascii_lowercase();
            if lowercase.contains("fixture")
                || lowercase.contains("row catalog")
                || lowercase.contains("source_row_id")
            {
                Refusal::org_fixture_invalid(message, detail).to_canon_output()
            } else {
                Refusal::org_bad_suite(message, detail).to_canon_output()
            }
        }
        org::OrgErrorCode::Promotion => {
            let lowercase = message.to_ascii_lowercase();
            if lowercase.contains("stale") {
                Refusal::org_stale_registry(message, detail).to_canon_output()
            } else if lowercase.contains("next-version")
                || lowercase.contains("next version")
                || lowercase.contains("version bump")
                || lowercase.contains("differ from the current")
            {
                Refusal::org_version_bump_required(message, detail).to_canon_output()
            } else {
                refusal::create_refusal(RefusalCode::EParse, message, detail, None)
            }
        }
        org::OrgErrorCode::Registry => refusal::create_refusal(
            RefusalCode::EBadRegistry,
            message,
            detail,
            Some("Check the org registry sidecars and rerun the canon org command".to_string()),
        ),
        org::OrgErrorCode::ArtifactContract => refusal::create_refusal(
            RefusalCode::EParse,
            message,
            detail,
            Some("Regenerate the referenced org artifact with matching strategy and registry inputs, then rerun".to_string()),
        ),
        org::OrgErrorCode::Explain => refusal::create_refusal(
            RefusalCode::EParse,
            message,
            detail,
            Some("Check the explain selector and result artifact, then rerun canon org explain".to_string()),
        ),
        org::OrgErrorCode::Unimplemented => refusal::create_refusal(
            RefusalCode::EParse,
            message,
            detail,
            Some("Use an implemented canon org subcommand or update the runtime wiring first".to_string()),
        ),
    }
}

fn create_resolve_refusal(error: resolve::ResolveError) -> CanonOutput {
    let detail = error.detail.unwrap_or_else(|| serde_json::json!({}));
    let message = error.message;

    match error.code {
        resolve::ResolveErrorCode::Strategy => refusal::create_refusal(
            RefusalCode::EBadStrategy,
            message,
            detail,
            Some("Fix the strategy YAML and rerun canon resolve with --strategy".to_string()),
        ),
        resolve::ResolveErrorCode::InputContract => refusal::create_refusal(
            RefusalCode::EColumnNotFound,
            message,
            detail,
            Some("Fix strategy field mappings or tape headers, then rerun canon resolve".to_string()),
        ),
        resolve::ResolveErrorCode::Registry => refusal::create_refusal(
            RefusalCode::EBadRegistry,
            message,
            detail,
            Some("Check the resolve registry and rerun canon resolve".to_string()),
        ),
        resolve::ResolveErrorCode::TooManyCandidates => refusal::create_refusal(
            RefusalCode::ETooManyCandidates,
            message,
            detail,
            Some("Tighten candidate_filter or raise --max-candidates, then rerun canon resolve".to_string()),
        ),
        resolve::ResolveErrorCode::EmptyTape => refusal::create_refusal(
            RefusalCode::EEmptyTape,
            message,
            detail,
            Some("Provide reference and target tapes with processable records, then rerun canon resolve".to_string()),
        ),
        resolve::ResolveErrorCode::IncompatibleTapes => refusal::create_refusal(
            RefusalCode::EIncompatibleTapes,
            message,
            detail,
            Some("Fix the strategy so reference and target fields can be compared, then rerun canon resolve".to_string()),
        ),
        resolve::ResolveErrorCode::Gold => refusal::create_refusal(
            RefusalCode::EParse,
            message,
            detail,
            Some("Repair the gold JSONL cross-reference file and rerun canon resolve".to_string()),
        ),
        resolve::ResolveErrorCode::WriteBack => refusal::create_refusal(
            RefusalCode::EParse,
            message,
            detail,
            Some("Resolve registry write-back conflicts before rerunning canon resolve --write-back".to_string()),
        ),
        resolve::ResolveErrorCode::Unimplemented => refusal::create_refusal(
            RefusalCode::EParse,
            message,
            detail,
            Some("Complete the resolve implementation beads before using canon resolve".to_string()),
        ),
    }
}

// Output types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    Resolved,
    Partial,
    Unresolved,
    Refusal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    // NOTE: input and canonical_id are RAW values without u8:/hex: prefix
    // JSON output applies encoding at serialization time
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnresolvedEntry {
    // input is RAW or None for special reasons
    pub input: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryMeta {
    pub id: String,
    pub version: String,
    // source is CLI arg verbatim
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryDiffVersion {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryDiffEntry {
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryDiffValue {
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryDiffChangeType {
    CanonicalIdChange,
    CanonicalTypeChange,
    RuleIdChange,
    MultipleFieldsChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryDiffChangedEntry {
    pub input: String,
    pub old: RegistryDiffValue,
    pub new: RegistryDiffValue,
    pub change_type: RegistryDiffChangeType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryDiffRemovedEntry {
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryDiffSummary {
    pub total_old: usize,
    pub total_new: usize,
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub unchanged: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryDiffOutput {
    pub version: String,
    pub old: RegistryDiffVersion,
    pub new: RegistryDiffVersion,
    pub summary: RegistryDiffSummary,
    pub added: Vec<RegistryDiffEntry>,
    pub removed: Vec<RegistryDiffRemovedEntry>,
    pub changed: Vec<RegistryDiffChangedEntry>,
}

impl RegistryDiffOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "{}: {} -> {} | +{} added, -{} removed, ~{} changed, ={} unchanged",
            self.old.id,
            self.old.version,
            self.new.version,
            self.summary.added,
            self.summary.removed,
            self.summary.changed,
            self.summary.unchanged,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryAuditSeed {
    pub path: String,
    pub column: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryAuditResolvedEntry {
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryAuditUnresolvedEntry {
    pub input: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryAuditCanonicalTarget {
    pub canonical_id: String,
    pub canonical_type: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryAuditRuleHit {
    pub rule_id: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryAuditSummary {
    pub total: usize,
    pub resolved: usize,
    pub unresolved: usize,
    pub distinct_canonical_targets: usize,
    pub distinct_rule_ids: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryAuditOutput {
    pub version: String,
    pub seed: RegistryAuditSeed,
    pub registry: RegistryMeta,
    pub summary: RegistryAuditSummary,
    pub resolved: Vec<RegistryAuditResolvedEntry>,
    pub unresolved: Vec<RegistryAuditUnresolvedEntry>,
    pub canonical_targets: Vec<RegistryAuditCanonicalTarget>,
    pub rule_hits: Vec<RegistryAuditRuleHit>,
}

impl RegistryAuditOutput {
    fn from_resolve_result(
        seed_path: &Path,
        column: &str,
        registry_meta: &RegistryMeta,
        result: &ResolveResult,
    ) -> Self {
        let mut resolved = result
            .mappings
            .iter()
            .map(|mapping| RegistryAuditResolvedEntry {
                input: output::json::encode_identifier(mapping.input.as_bytes()),
                canonical_id: output::json::encode_identifier(mapping.canonical_id.as_bytes()),
                canonical_type: mapping.canonical_type.clone(),
                rule_id: mapping.rule_id.clone(),
            })
            .collect::<Vec<_>>();
        resolved.sort_by(|left, right| left.input.cmp(&right.input));

        let mut unresolved = result
            .unresolved
            .iter()
            .map(|entry| RegistryAuditUnresolvedEntry {
                input: entry
                    .input
                    .as_ref()
                    .map(|value| output::json::encode_identifier(value.as_bytes())),
                reason: entry.reason.clone(),
            })
            .collect::<Vec<_>>();
        unresolved.sort_by(|left, right| match (&left.input, &right.input) {
            (Some(left_input), Some(right_input)) => left_input.cmp(right_input),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, None) => left.reason.cmp(&right.reason),
        });

        let mut canonical_targets = BTreeMap::new();
        let mut rule_hits = BTreeMap::new();
        for mapping in &result.mappings {
            *canonical_targets
                .entry((mapping.canonical_type.clone(), mapping.canonical_id.clone()))
                .or_insert(0usize) += 1;
            *rule_hits.entry(mapping.rule_id.clone()).or_insert(0usize) += 1;
        }

        let canonical_targets = canonical_targets
            .into_iter()
            .map(
                |((canonical_type, canonical_id), count)| RegistryAuditCanonicalTarget {
                    canonical_id: output::json::encode_identifier(canonical_id.as_bytes()),
                    canonical_type,
                    count,
                },
            )
            .collect::<Vec<_>>();
        let rule_hits = rule_hits
            .into_iter()
            .map(|(rule_id, count)| RegistryAuditRuleHit { rule_id, count })
            .collect::<Vec<_>>();

        Self {
            version: "canon_registry_audit.v0".to_string(),
            seed: RegistryAuditSeed {
                path: seed_path.display().to_string(),
                column: column.to_string(),
            },
            registry: registry_meta.clone(),
            summary: RegistryAuditSummary {
                total: result.summary.total,
                resolved: result.summary.resolved,
                unresolved: result.summary.unresolved,
                distinct_canonical_targets: canonical_targets.len(),
                distinct_rule_ids: rule_hits.len(),
            },
            resolved,
            unresolved,
            canonical_targets,
            rule_hits,
        }
    }

    pub fn render_summary(&self) -> String {
        format!(
            "{}@{} audit {}:{} | {} total, {} resolved, {} unresolved, {} targets, {} rules",
            self.registry.id,
            self.registry.version,
            self.seed.path,
            self.seed.column,
            self.summary.total,
            self.summary.resolved,
            self.summary.unresolved,
            self.summary.distinct_canonical_targets,
            self.summary.distinct_rule_ids,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryBuildSpecialReason {
    pub reason: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryBuildFailure {
    pub input: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryBuildUnresolvedEntry {
    pub input: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryBuildSummary {
    pub seed_count: usize,
    pub queried_count: usize,
    pub carried_forward_count: usize,
    pub resolved_count: usize,
    pub unresolved_count: usize,
    pub failure_count: usize,
    pub ambiguous_count: usize,
    pub skipped_special_reason_rows: usize,
    pub mapping_files: usize,
    pub api_calls: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryBuildOutput {
    pub version: String,
    pub source: String,
    pub registry: RegistryMeta,
    pub output_path: String,
    pub summary: RegistryBuildSummary,
    pub files: Vec<String>,
    pub unresolved: Vec<RegistryBuildUnresolvedEntry>,
    pub failures: Vec<RegistryBuildFailure>,
    pub special_reasons: Vec<RegistryBuildSpecialReason>,
    pub incremental: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub total: usize,
    pub resolved: usize,
    pub unresolved: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonOutput {
    pub version: String,
    pub outcome: Outcome,
    pub registry: Option<RegistryMeta>,
    pub summary: Option<Summary>,
    pub mappings: Vec<Mapping>,
    pub unresolved: Vec<UnresolvedEntry>,
    pub refusal: Option<Refusal>,
}

// Refusal types
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum RefusalCode {
    EIo,
    EEncoding,
    ECsvParse,
    EBadRegistry,
    EColumnNotFound,
    EParse,
    EEmptyInput,
    ETooLarge,
    EEmitFormat,
    EColumnExists,
    EOrgInputContract,
    EOrgBadStrategy,
    EOrgBadSuite,
    EOrgFixtureInvalid,
    EOrgVersionBumpRequired,
    EOrgStaleRegistry,
    EStrategyInputContract,
    EStrategyProofInvalid,
    EStrategyVersionBumpRequired,
    EBadStrategy,
    ETooManyCandidates,
    EEmptyTape,
    EIncompatibleTapes,
}

impl Serialize for RefusalCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let code_str = match self {
            RefusalCode::EIo => "E_IO",
            RefusalCode::EEncoding => "E_ENCODING",
            RefusalCode::ECsvParse => "E_CSV_PARSE",
            RefusalCode::EBadRegistry => "E_BAD_REGISTRY",
            RefusalCode::EColumnNotFound => "E_COLUMN_NOT_FOUND",
            RefusalCode::EParse => "E_PARSE",
            RefusalCode::EEmptyInput => "E_EMPTY_INPUT",
            RefusalCode::ETooLarge => "E_TOO_LARGE",
            RefusalCode::EEmitFormat => "E_EMIT_FORMAT",
            RefusalCode::EColumnExists => "E_COLUMN_EXISTS",
            RefusalCode::EOrgInputContract => "E_ORG_INPUT_CONTRACT",
            RefusalCode::EOrgBadStrategy => "E_ORG_BAD_STRATEGY",
            RefusalCode::EOrgBadSuite => "E_ORG_BAD_SUITE",
            RefusalCode::EOrgFixtureInvalid => "E_ORG_FIXTURE_INVALID",
            RefusalCode::EOrgVersionBumpRequired => "E_ORG_VERSION_BUMP_REQUIRED",
            RefusalCode::EOrgStaleRegistry => "E_ORG_STALE_REGISTRY",
            RefusalCode::EStrategyInputContract => "E_STRATEGY_INPUT_CONTRACT",
            RefusalCode::EStrategyProofInvalid => "E_STRATEGY_PROOF_INVALID",
            RefusalCode::EStrategyVersionBumpRequired => "E_STRATEGY_VERSION_BUMP_REQUIRED",
            RefusalCode::EBadStrategy => "E_BAD_STRATEGY",
            RefusalCode::ETooManyCandidates => "E_TOO_MANY_CANDIDATES",
            RefusalCode::EEmptyTape => "E_EMPTY_TAPE",
            RefusalCode::EIncompatibleTapes => "E_INCOMPATIBLE_TAPES",
        };
        serializer.serialize_str(code_str)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refusal {
    pub code: RefusalCode,
    pub message: String,
    pub detail: serde_json::Value,
    pub next_command: Option<String>,
}

// Cross-module types (the 8-agent contract)
#[derive(Debug, Clone, PartialEq)]
pub enum InputFormat {
    Csv,
    Jsonl,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SpecialReason {
    EmptyValue,
    NullValue,
    MissingField,
    NonScalarValue,
}

impl std::fmt::Display for SpecialReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reason_str = match self {
            SpecialReason::EmptyValue => "empty_value",
            SpecialReason::NullValue => "null_value",
            SpecialReason::MissingField => "missing_field",
            SpecialReason::NonScalarValue => "non_scalar_value",
        };
        write!(f, "{}", reason_str)
    }
}

#[derive(Debug, Clone)]
pub struct InputValues {
    pub values: HashMap<String, ()>,
    pub special: HashMap<SpecialReason, usize>,
    pub format: InputFormat,
    pub delimiter: Option<u8>,
    pub source_hash: Option<String>,
    pub source_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Registry {
    pub meta: RegistryMeta,
    pub db_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ResolveResult {
    pub mappings: Vec<Mapping>,
    pub unresolved: Vec<UnresolvedEntry>,
    pub summary: Summary,
}

#[cfg(test)]
mod tests {
    use super::{DisplayMode, detect_display_mode};

    #[test]
    fn detect_display_mode_ignores_subcommand_version_flag() {
        let args = [
            "canon",
            "registry",
            "build",
            "--source",
            "mock",
            "--version",
            "2026.03.13",
        ];

        assert_eq!(detect_display_mode(args), None);
    }

    #[test]
    fn detect_display_mode_short_circuits_top_level_info_flags() {
        assert_eq!(
            detect_display_mode(["canon", "--version", "--emit", "bogus"]),
            Some(DisplayMode::Version)
        );
        assert_eq!(
            detect_display_mode(["canon", "--describe", "--column"]),
            Some(DisplayMode::Describe)
        );
        assert_eq!(
            detect_display_mode(["canon", "--schema", "--max-rows", "nope"]),
            Some(DisplayMode::Schema)
        );
    }
}
