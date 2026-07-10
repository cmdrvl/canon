#![forbid(unsafe_code)]

pub mod cli;
pub mod doctor;
pub mod entity;
pub mod inbox;
pub mod input;
pub mod lookup;
pub mod namekit;
pub mod output;
pub mod paths;
pub mod refusal;
pub mod registry;
pub mod registry_lint;
pub mod resolve;
pub mod temporal;
pub mod strategy {
    pub mod types;
}
pub mod strategy_audit;
pub mod strategy_profile;
pub mod strategy_registry;
pub mod witness;

use crate::cli::{
    CanonCommand, Cli, EntityAuditCli, EntityBlockCli, EntityCommand, EntityEdgeCli,
    EntityEmitMode, EntityExplainCli, EntityPrepareCli, EntityProfileCommand, EntityProfileInitCli,
    EntityProfileListCli, EntityProfileSubcommand, EntityPromoteCli, EntityReviewCommand,
    EntityReviewExportCli, EntityReviewExportEmitMode, EntityReviewImportCli, EntityReviewInclude,
    EntityReviewSubcommand, EntityRunCli, EntitySolveCli, EntityStreamEmitMode, EntitySubcommand,
    RegistryAddEntryCli, RegistryAuditCli, RegistryBuildCli, RegistryDefaultIdSchemeCli,
    RegistryDiffCli, RegistryEmitMode, RegistryExportCli, RegistryExportFormatCli, RegistryLintCli,
    RegistryLintProfile, RegistryMintCli, RegistryNextIdCli, RegistryPlainJsonEmitMode,
    RegistryProviderSchemaCli, RegistryProvidersCli, RegistrySubcommand, RegistryVersionBumpMode,
    ResolveCli, ResolveEmitMode, StrategyAuditCli, StrategyCommand, StrategyDeprecateCli,
    StrategyDiffCli, StrategyExplainCli, StrategyGradeArg, StrategyKeyTypeArg, StrategyListCli,
    StrategyProfileCli, StrategyPromoteCli, StrategyRegisterCli, StrategyResolveCli,
    StrategyStatusArg, StrategySubcommand, StrategyUpdateCli,
};
use crate::entity::runtime as entity_runtime;
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
const LEGACY_ENTITY_MAX_CANDIDATE_PAIR_EXPANSIONS: u64 = 25_000_000;

enum EntityRunExecution {
    Legacy {
        artifact: Box<entity_runtime::SolveRunArtifact>,
        candidate_pairs: u64,
    },
    Workbench {
        artifact: Box<entity::run::EntityRunArtifact>,
        candidate_pairs: u64,
    },
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

    // Intent inference: a misspelled subcommand is otherwise swallowed as the
    // optional positional input. If the input does not exist and is one edit
    // away from a known subcommand, refuse with a did-you-mean rather than a
    // bare file-not-found error. Real input files (which exist) never trigger.
    if !input_path.exists()
        && let Some(token) = input_path.to_str()
        && let Some(subcommand) = suggest_subcommand(token)
    {
        let refusal = refusal::create_refusal(
            RefusalCode::EParse,
            format!("'{token}' is not a canon subcommand or a readable input file"),
            serde_json::json!({ "input": token, "suggested_subcommand": subcommand }),
            Some(format!("canon {subcommand} --help")),
        );
        match cli.emit {
            crate::cli::EmitMode::Json => println!("{}", serde_json::to_string(&refusal)?),
            crate::cli::EmitMode::Csv => eprintln!("{}", serde_json::to_string(&refusal)?),
        }
        return Ok(2);
    }

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
        CanonCommand::Doctor(args) => doctor::run(args),
        CanonCommand::Resolve(resolve) => run_resolve_command(resolve),
        CanonCommand::Registry(command) => match &command.command {
            RegistrySubcommand::Export(export) => run_registry_export(export),
            RegistrySubcommand::NextId(next_id) => run_registry_next_id(next_id),
            RegistrySubcommand::AddEntry(add_entry) => run_registry_add_entry(add_entry),
            RegistrySubcommand::Mint(mint) => run_registry_mint(mint),
            RegistrySubcommand::DefaultIdScheme(id_scheme) => {
                run_registry_default_id_scheme(id_scheme)
            }
            RegistrySubcommand::Diff(diff) => run_registry_diff(diff),
            RegistrySubcommand::Audit(audit) => run_registry_audit(audit),
            RegistrySubcommand::Build(build) => run_registry_build(build),
            RegistrySubcommand::Lint(lint) => run_registry_lint(lint),
            RegistrySubcommand::Providers(providers) => run_registry_providers(providers),
            RegistrySubcommand::ProviderSchema(schema) => run_registry_provider_schema(schema),
        },
        CanonCommand::Entity(command) => run_entity_command(command),
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
        StrategySubcommand::Update(update) => run_strategy_update_command(update),
        StrategySubcommand::Deprecate(deprecate) => run_strategy_deprecate_command(deprecate),
        StrategySubcommand::Promote(promote) => run_strategy_promote_command(promote),
        StrategySubcommand::List(list) => run_strategy_list_command(list),
        StrategySubcommand::Explain(explain) => run_strategy_explain_command(explain),
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
    let key = match strategy_key_selector(resolve.schema.as_deref(), resolve.task.as_deref()) {
        Ok(key) => key,
        Err(refusal) => {
            return emit_strategy_refusal(
                refusal,
                matches!(resolve.emit, RegistryEmitMode::Summary),
            );
        }
    };
    match strategy_registry::resolve(strategy_registry::StrategyResolveRequest {
        registry_dir: &resolve.registry,
        key,
        skill_path: resolve.skill.as_deref(),
        skill_hash: resolve.skill_hash.as_deref(),
    }) {
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
    let key = match strategy_key_selector(register.schema.as_deref(), register.task.as_deref()) {
        Ok(key) => key,
        Err(refusal) => {
            return emit_strategy_refusal(
                refusal,
                matches!(register.emit, RegistryEmitMode::Summary),
            );
        }
    };
    let request = strategy_registry::StrategyRegisterRequest {
        registry_dir: &register.registry,
        key,
        skill_path: register.skill.as_deref(),
        skill_hash: register.skill_hash.as_deref(),
        script_path: &register.script,
        script_id: &register.script_id,
        language: &register.language,
        grade: strategy_grade(register.grade),
        operator: register.operator.as_deref(),
        reason: register.reason.as_deref(),
        attested_at: register.attested_at.as_deref(),
        verify_path: register.verify.as_deref(),
        assess_path: register.assess.as_deref(),
        airlock_path: register.airlock.as_deref(),
        next_version: &register.next_version,
        rule_id: register.rule_id.as_deref(),
    };

    match strategy_registry::register(request) {
        Ok(output) => emit_strategy_mutation_output(
            &output,
            output.render_summary(),
            register.emit.clone(),
            register.no_witness,
        ),
        Err(refusal) => {
            emit_strategy_refusal(refusal, matches!(register.emit, RegistryEmitMode::Summary))
        }
    }
}

fn run_strategy_update_command(update: &StrategyUpdateCli) -> Result<u8, Box<dyn Error>> {
    let key = match strategy_key_selector(update.schema.as_deref(), update.task.as_deref()) {
        Ok(key) => key,
        Err(refusal) => {
            return emit_strategy_refusal(
                refusal,
                matches!(update.emit, RegistryEmitMode::Summary),
            );
        }
    };
    let request = strategy_registry::StrategyLifecycleRequest {
        registry_dir: &update.registry,
        key,
        skill_path: update.skill.as_deref(),
        skill_hash: update.skill_hash.as_deref(),
        script_path: Some(&update.script),
        script_id: Some(&update.script_id),
        language: Some(&update.language),
        operator: update.operator.as_deref(),
        reason: update.reason.as_deref(),
        attested_at: update.attested_at.as_deref(),
        verify_path: update.verify.as_deref(),
        assess_path: update.assess.as_deref(),
        airlock_path: update.airlock.as_deref(),
        next_version: &update.next_version,
    };
    match strategy_registry::update(request) {
        Ok(output) => emit_strategy_mutation_output(
            &output,
            output.render_summary(),
            update.emit.clone(),
            update.no_witness,
        ),
        Err(refusal) => {
            emit_strategy_refusal(refusal, matches!(update.emit, RegistryEmitMode::Summary))
        }
    }
}

fn run_strategy_deprecate_command(deprecate: &StrategyDeprecateCli) -> Result<u8, Box<dyn Error>> {
    let key = match strategy_key_selector(deprecate.schema.as_deref(), deprecate.task.as_deref()) {
        Ok(key) => key,
        Err(refusal) => {
            return emit_strategy_refusal(
                refusal,
                matches!(deprecate.emit, RegistryEmitMode::Summary),
            );
        }
    };
    let request = strategy_registry::StrategyLifecycleRequest {
        registry_dir: &deprecate.registry,
        key,
        skill_path: deprecate.skill.as_deref(),
        skill_hash: deprecate.skill_hash.as_deref(),
        script_path: None,
        script_id: None,
        language: None,
        operator: Some(&deprecate.operator),
        reason: Some(&deprecate.reason),
        attested_at: deprecate.attested_at.as_deref(),
        verify_path: None,
        assess_path: None,
        airlock_path: None,
        next_version: &deprecate.next_version,
    };
    match strategy_registry::deprecate(request) {
        Ok(output) => emit_strategy_mutation_output(
            &output,
            output.render_summary(),
            deprecate.emit.clone(),
            deprecate.no_witness,
        ),
        Err(refusal) => {
            emit_strategy_refusal(refusal, matches!(deprecate.emit, RegistryEmitMode::Summary))
        }
    }
}

fn run_strategy_promote_command(promote: &StrategyPromoteCli) -> Result<u8, Box<dyn Error>> {
    let key = match strategy_key_selector(promote.schema.as_deref(), promote.task.as_deref()) {
        Ok(key) => key,
        Err(refusal) => {
            return emit_strategy_refusal(
                refusal,
                matches!(promote.emit, RegistryEmitMode::Summary),
            );
        }
    };
    let request = strategy_registry::StrategyLifecycleRequest {
        registry_dir: &promote.registry,
        key,
        skill_path: promote.skill.as_deref(),
        skill_hash: promote.skill_hash.as_deref(),
        script_path: None,
        script_id: None,
        language: None,
        operator: None,
        reason: None,
        attested_at: None,
        verify_path: Some(&promote.verify),
        assess_path: Some(&promote.assess),
        airlock_path: Some(&promote.airlock),
        next_version: &promote.next_version,
    };
    match strategy_registry::promote(request) {
        Ok(output) => emit_strategy_mutation_output(
            &output,
            output.render_summary(),
            promote.emit.clone(),
            promote.no_witness,
        ),
        Err(refusal) => {
            emit_strategy_refusal(refusal, matches!(promote.emit, RegistryEmitMode::Summary))
        }
    }
}

fn run_strategy_list_command(list: &StrategyListCli) -> Result<u8, Box<dyn Error>> {
    let key_type = list.key_type.map(strategy_key_type);
    match strategy_registry::list(strategy_registry::StrategyCatalogRequest {
        registry_dir: &list.registry,
        key_type,
        grade: list.grade.map(strategy_grade),
        status: list.status.map(strategy_status),
    }) {
        Ok(output) => {
            match list.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(0)
        }
        Err(refusal) => {
            emit_strategy_refusal(refusal, matches!(list.emit, RegistryEmitMode::Summary))
        }
    }
}

fn run_strategy_explain_command(explain: &StrategyExplainCli) -> Result<u8, Box<dyn Error>> {
    let key = match strategy_key_selector(explain.schema.as_deref(), explain.task.as_deref()) {
        Ok(key) => key,
        Err(refusal) => {
            return emit_strategy_refusal(
                refusal,
                matches!(explain.emit, RegistryEmitMode::Summary),
            );
        }
    };
    match strategy_registry::explain(strategy_registry::StrategyExplainRequest {
        registry_dir: &explain.registry,
        key,
        skill_path: explain.skill.as_deref(),
        skill_hash: explain.skill_hash.as_deref(),
    }) {
        Ok(output) => {
            match explain.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(0)
        }
        Err(refusal) => {
            emit_strategy_refusal(refusal, matches!(explain.emit, RegistryEmitMode::Summary))
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

fn strategy_key_selector<'a>(
    schema: Option<&'a Path>,
    task: Option<&'a str>,
) -> Result<strategy_registry::StrategyKeySelector<'a>, Refusal> {
    match (schema, task) {
        (Some(path), None) => Ok(strategy_registry::StrategyKeySelector::Schema(path)),
        (None, Some(task)) => Ok(strategy_registry::StrategyKeySelector::Task(task)),
        _ => Err(Refusal::strategy_input_contract(
            "Exactly one of --schema or --task is required",
            serde_json::json!({
                "has_schema": schema.is_some(),
                "has_task": task.is_some(),
            }),
        )),
    }
}

fn strategy_grade(grade: StrategyGradeArg) -> strategy_registry::StrategyAttestationGrade {
    match grade {
        StrategyGradeArg::OperatorAttested => {
            strategy_registry::StrategyAttestationGrade::OperatorAttested
        }
        StrategyGradeArg::ProofAttested => {
            strategy_registry::StrategyAttestationGrade::ProofAttested
        }
    }
}

fn strategy_status(status: StrategyStatusArg) -> strategy_registry::StrategyEntryStatus {
    match status {
        StrategyStatusArg::Active => strategy_registry::StrategyEntryStatus::Active,
        StrategyStatusArg::Deprecated => strategy_registry::StrategyEntryStatus::Deprecated,
    }
}

fn strategy_key_type(key_type: StrategyKeyTypeArg) -> &'static str {
    match key_type {
        StrategyKeyTypeArg::Schema => "schema",
        StrategyKeyTypeArg::Task => "task",
    }
}

fn emit_strategy_mutation_output<T: Serialize>(
    output: &T,
    summary: String,
    emit: RegistryEmitMode,
    no_witness: bool,
) -> Result<u8, Box<dyn Error>> {
    let json = serde_json::to_string(output)?;
    append_strategy_mutation_witness(&json, no_witness);
    match emit {
        RegistryEmitMode::Json => println!("{json}"),
        RegistryEmitMode::Summary => println!("{summary}"),
    }
    Ok(0)
}

fn append_strategy_mutation_witness(output_json: &str, no_witness: bool) {
    let mut params = serde_json::Map::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(output_json) {
        if let Some(receipt) = value.get("receipt") {
            params.insert("strategy_receipt".to_string(), receipt.clone());
        }
        if let Some(registry) = value.get("registry") {
            params.insert("registry".to_string(), registry.clone());
        }
    }
    let output_hash = witness::hash_bytes(output_json.as_bytes());
    let record = witness::WitnessRecord::new(Vec::new(), params, &output_hash, "RESOLVED", 0);
    if let Err(error) = witness::append_witness_record(&record, no_witness) {
        eprintln!("Warning: witness append skipped: {error}");
    }
}

fn run_entity_command(command: &EntityCommand) -> Result<u8, Box<dyn Error>> {
    match &command.command {
        EntitySubcommand::Run(run) => run_entity_run_command(run),
        EntitySubcommand::Prepare(prepare) => run_entity_prepare_command(prepare),
        EntitySubcommand::Block(block) => run_entity_block_command(block),
        EntitySubcommand::Edge(edge) => run_entity_edge_command(edge),
        EntitySubcommand::Solve(solve) => run_entity_solve_command(solve),
        EntitySubcommand::Audit(audit) => run_entity_audit_command(audit),
        EntitySubcommand::Promote(promote) => run_entity_promote_command(promote),
        EntitySubcommand::Explain(explain) => run_entity_explain_command(explain),
        EntitySubcommand::Profile(profile) => run_entity_profile_command(profile),
        EntitySubcommand::Review(review) => run_entity_review_command(review),
    }
}

fn run_entity_run_command(run: &EntityRunCli) -> Result<u8, Box<dyn Error>> {
    let started = Instant::now();

    match run_entity_run_pipeline(run) {
        Ok(EntityRunExecution::Legacy {
            artifact,
            candidate_pairs,
        }) => {
            let artifact_bytes = serde_json::to_vec(&artifact)?;
            if let Some(suite_dir) = run.suite.as_deref()
                && let Err(refusal_output) = entity_runtime::audit::audit(
                    &artifact,
                    &artifact_bytes,
                    entity_runtime::audit::AuditContext {
                        suite_dir,
                        profile: ORG_V1_PROFILE,
                        budget_usage: entity_runtime::audit::AuditBudgetUsage {
                            runtime_seconds: started.elapsed().as_secs(),
                            candidate_pairs,
                        },
                        baseline: None,
                        promoted_with_prior_escrow_count: 0,
                    },
                )
                .map_err(create_entity_refusal)
            {
                return emit_entity_refusal(
                    refusal_output,
                    true,
                    matches!(run.emit, EntityEmitMode::Summary),
                );
            }

            let output = match run.emit {
                EntityEmitMode::Json => entity_runtime::output::emit_run_json(&artifact)?,
                EntityEmitMode::Summary => entity_runtime::output::render_run_summary(&artifact),
            };
            emit_entity_output(&output, matches!(run.emit, EntityEmitMode::Summary));
            append_entity_run_witness(
                run,
                &artifact,
                &output,
                started.elapsed().as_secs(),
                run.suite.as_deref().map(|_| candidate_pairs),
            );
            Ok(0)
        }
        Ok(EntityRunExecution::Workbench {
            artifact,
            candidate_pairs,
        }) => {
            let output = match run.emit {
                EntityEmitMode::Json => serde_json::to_string(&artifact)?,
                EntityEmitMode::Summary => entity::run::render_run_summary(&artifact),
            };
            emit_entity_output(&output, matches!(run.emit, EntityEmitMode::Summary));
            append_entity_workbench_run_witness(
                run,
                &artifact,
                &output,
                started.elapsed().as_secs(),
                candidate_pairs,
            );
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(
            refusal_output,
            true,
            matches!(run.emit, EntityEmitMode::Summary),
        ),
    }
}

fn run_entity_prepare_command(prepare: &EntityPrepareCli) -> Result<u8, Box<dyn Error>> {
    match run_entity_prepare_pipeline(prepare) {
        Ok(artifact) => {
            let output = serde_json::to_string(&artifact)?;
            emit_entity_output(&output, false);
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(refusal_output, true, false),
    }
}

fn run_entity_block_command(block: &EntityBlockCli) -> Result<u8, Box<dyn Error>> {
    match run_entity_block_pipeline(block) {
        Ok(records) => {
            let output = match block.emit {
                EntityStreamEmitMode::Jsonl => entity_runtime::output::emit_block_jsonl(&records)?,
                EntityStreamEmitMode::Summary => {
                    entity_runtime::output::render_block_summary(&records)
                }
            };
            emit_entity_output(&output, matches!(block.emit, EntityStreamEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(
            refusal_output,
            true,
            matches!(block.emit, EntityStreamEmitMode::Summary),
        ),
    }
}

fn run_entity_edge_command(edge: &EntityEdgeCli) -> Result<u8, Box<dyn Error>> {
    match run_entity_edge_pipeline(edge) {
        Ok(records) => {
            let output = match edge.emit {
                EntityStreamEmitMode::Jsonl => entity_runtime::output::emit_edge_jsonl(&records)?,
                EntityStreamEmitMode::Summary => {
                    entity_runtime::output::render_edge_summary(&records)
                }
            };
            emit_entity_output(&output, matches!(edge.emit, EntityStreamEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(
            refusal_output,
            true,
            matches!(edge.emit, EntityStreamEmitMode::Summary),
        ),
    }
}

fn run_entity_solve_command(solve: &EntitySolveCli) -> Result<u8, Box<dyn Error>> {
    match run_entity_solve_pipeline(solve) {
        Ok(artifact) => {
            let output = match solve.emit {
                EntityEmitMode::Json => entity_runtime::output::emit_solve_json(&artifact)?,
                EntityEmitMode::Summary => entity_runtime::output::render_solve_summary(&artifact),
            };
            emit_entity_output(&output, matches!(solve.emit, EntityEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(
            refusal_output,
            true,
            matches!(solve.emit, EntityEmitMode::Summary),
        ),
    }
}

fn run_entity_audit_command(audit: &EntityAuditCli) -> Result<u8, Box<dyn Error>> {
    match run_entity_audit_pipeline(audit) {
        Ok(artifact) => {
            let output = match audit.emit {
                EntityEmitMode::Json => entity_runtime::output::emit_audit_json(&artifact)?,
                EntityEmitMode::Summary => entity_runtime::output::render_audit_summary(&artifact),
            };
            emit_entity_output(&output, matches!(audit.emit, EntityEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(
            refusal_output,
            true,
            matches!(audit.emit, EntityEmitMode::Summary),
        ),
    }
}

fn run_entity_promote_command(promote: &EntityPromoteCli) -> Result<u8, Box<dyn Error>> {
    match run_entity_promote_pipeline(promote) {
        Ok(artifact) => {
            let output = match promote.emit {
                EntityEmitMode::Json => entity_runtime::output::emit_promote_json(&artifact)?,
                EntityEmitMode::Summary => {
                    entity_runtime::output::render_promote_summary(&artifact)
                }
            };
            emit_entity_output(&output, matches!(promote.emit, EntityEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(
            refusal_output,
            true,
            matches!(promote.emit, EntityEmitMode::Summary),
        ),
    }
}

fn run_entity_profile_command(profile: &EntityProfileCommand) -> Result<u8, Box<dyn Error>> {
    match &profile.command {
        EntityProfileSubcommand::List(list) => run_entity_profile_list_command(list),
        EntityProfileSubcommand::Init(init) => run_entity_profile_init_command(init),
    }
}

fn run_entity_profile_list_command(list: &EntityProfileListCli) -> Result<u8, Box<dyn Error>> {
    match entity::profile_cli::list_profile_templates() {
        Ok(catalog) => {
            match list.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&catalog)?),
                RegistryEmitMode::Summary => println!("{}", catalog.render_summary()),
            }
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(
            refusal_output,
            matches!(list.emit, RegistryEmitMode::Json),
            matches!(list.emit, RegistryEmitMode::Summary),
        ),
    }
}

fn run_entity_profile_init_command(init: &EntityProfileInitCli) -> Result<u8, Box<dyn Error>> {
    match entity::profile_cli::init_profile_template(&init.profile, &init.output) {
        Ok(output) => {
            println!("{}", serde_json::to_string(&output)?);
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(refusal_output, true, false),
    }
}

fn run_entity_review_command(review: &EntityReviewCommand) -> Result<u8, Box<dyn Error>> {
    match &review.command {
        EntityReviewSubcommand::Export(export) => run_entity_review_export_command(export),
        EntityReviewSubcommand::Import(import) => run_entity_review_import_command(import),
    }
}

fn run_entity_review_export_command(export: &EntityReviewExportCli) -> Result<u8, Box<dyn Error>> {
    match run_entity_review_export_pipeline(export) {
        Ok(artifact) => {
            let output = match export.emit {
                EntityReviewExportEmitMode::Json => Ok(serde_json::to_string(&artifact)?),
                EntityReviewExportEmitMode::Csv => {
                    entity_runtime::review::export_csv(&artifact).map_err(create_entity_refusal)
                }
            };
            match output {
                Ok(output) => {
                    emit_entity_output(&output, false);
                    Ok(0)
                }
                Err(refusal_output) => emit_entity_refusal(
                    refusal_output,
                    matches!(export.emit, EntityReviewExportEmitMode::Json),
                    false,
                ),
            }
        }
        Err(refusal_output) => emit_entity_refusal(
            refusal_output,
            matches!(export.emit, EntityReviewExportEmitMode::Json),
            false,
        ),
    }
}

fn run_entity_review_import_command(import: &EntityReviewImportCli) -> Result<u8, Box<dyn Error>> {
    match run_entity_review_import_pipeline(import) {
        Ok(artifact) => {
            let output = match import.emit {
                EntityEmitMode::Json => serde_json::to_string(&artifact)?,
                EntityEmitMode::Summary => artifact.render_summary(),
            };
            emit_entity_output(&output, matches!(import.emit, EntityEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(
            refusal_output,
            true,
            matches!(import.emit, EntityEmitMode::Summary),
        ),
    }
}

fn run_entity_explain_command(explain: &EntityExplainCli) -> Result<u8, Box<dyn Error>> {
    match run_entity_explain_pipeline(explain) {
        Ok(artifact) => {
            let output = match explain.emit {
                EntityEmitMode::Json => entity_runtime::output::emit_explain_json(&artifact)?,
                EntityEmitMode::Summary => {
                    entity_runtime::output::render_explain_summary(&artifact)
                }
            };
            emit_entity_output(&output, matches!(explain.emit, EntityEmitMode::Summary));
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(
            refusal_output,
            true,
            matches!(explain.emit, EntityEmitMode::Summary),
        ),
    }
}

fn run_registry_next_id(next_id: &RegistryNextIdCli) -> Result<u8, Box<dyn Error>> {
    let request = registry::RegistryNextIdRequest {
        registry: next_id.registry.clone(),
        prefix: next_id.prefix.clone(),
        zero_pad: next_id.zero_pad,
    };

    match registry::next_id(request) {
        Ok(output) => {
            match next_id.emit {
                RegistryPlainJsonEmitMode::Plain => println!("{}", output.render_plain()),
                RegistryPlainJsonEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
            }
            Ok(0)
        }
        Err(refusal) => {
            let output = refusal.to_canon_output();
            match next_id.emit {
                RegistryPlainJsonEmitMode::Plain => {
                    eprintln!("{}", serde_json::to_string(&output)?)
                }
                RegistryPlainJsonEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
            }
            Ok(2)
        }
    }
}

fn run_registry_add_entry(add_entry: &RegistryAddEntryCli) -> Result<u8, Box<dyn Error>> {
    let request = registry::RegistryAddEntryRequest {
        registry: add_entry.registry.clone(),
        alias_file: add_entry.alias_file.clone(),
        canonical_id: add_entry.canonical_id.clone(),
        input: add_entry.input.clone(),
        rule_id: add_entry.rule_id.clone(),
        canonical_type: add_entry.canonical_type.clone(),
        bump: add_entry.bump.map(registry_version_bump),
        next_version: add_entry.next_version.clone(),
        no_lint: add_entry.no_lint,
    };

    match registry::add_entry(request) {
        Ok(output) => {
            match add_entry.emit {
                RegistryPlainJsonEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryPlainJsonEmitMode::Plain => println!("{}", output.render_plain()),
            }
            Ok(0)
        }
        Err(refusal) => {
            let output = refusal.to_canon_output();
            match add_entry.emit {
                RegistryPlainJsonEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryPlainJsonEmitMode::Plain => {
                    eprintln!("{}", serde_json::to_string(&output)?)
                }
            }
            Ok(2)
        }
    }
}

fn run_registry_mint(mint: &RegistryMintCli) -> Result<u8, Box<dyn Error>> {
    let request = registry::RegistryMintRequest {
        registry: mint.registry.clone(),
        canonical_id: mint.canonical_id.clone(),
        prefix: mint.prefix.clone(),
        canonical_type: mint.canonical_type.clone(),
        with_alias: mint.with_alias.clone(),
        bump: mint.bump.map(registry_version_bump),
        next_version: mint.next_version.clone(),
        no_lint: mint.no_lint,
    };

    match registry::mint(request) {
        Ok(output) => {
            match mint.emit {
                RegistryPlainJsonEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryPlainJsonEmitMode::Plain => println!("{}", output.render_plain()),
            }
            Ok(0)
        }
        Err(refusal) => {
            let output = refusal.to_canon_output();
            match mint.emit {
                RegistryPlainJsonEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryPlainJsonEmitMode::Plain => {
                    eprintln!("{}", serde_json::to_string(&output)?)
                }
            }
            Ok(2)
        }
    }
}

fn run_registry_default_id_scheme(
    id_scheme: &RegistryDefaultIdSchemeCli,
) -> Result<u8, Box<dyn Error>> {
    let request = registry::RegistryDefaultIdSchemeRequest {
        registry: id_scheme.registry.clone(),
        prefix: id_scheme.prefix.clone(),
        zero_pad: id_scheme.zero_pad,
        strict: id_scheme.strict,
        bump: id_scheme.bump.map(registry_version_bump),
        next_version: id_scheme.next_version.clone(),
    };

    match registry::set_default_id_scheme(request) {
        Ok(output) => {
            match id_scheme.emit {
                RegistryPlainJsonEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryPlainJsonEmitMode::Plain => println!("{}", output.render_plain()),
            }
            Ok(0)
        }
        Err(refusal) => {
            let output = refusal.to_canon_output();
            match id_scheme.emit {
                RegistryPlainJsonEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryPlainJsonEmitMode::Plain => {
                    eprintln!("{}", serde_json::to_string(&output)?)
                }
            }
            Ok(2)
        }
    }
}

fn registry_version_bump(bump: RegistryVersionBumpMode) -> registry::RegistryVersionBump {
    match bump {
        RegistryVersionBumpMode::Patch => registry::RegistryVersionBump::Patch,
        RegistryVersionBumpMode::Minor => registry::RegistryVersionBump::Minor,
        RegistryVersionBumpMode::Major => registry::RegistryVersionBump::Major,
    }
}

fn run_registry_export(export: &RegistryExportCli) -> Result<u8, Box<dyn Error>> {
    let format = match export.format {
        RegistryExportFormatCli::DbtSeed => registry::RegistryExportFormat::DbtSeed,
        RegistryExportFormatCli::SearchIndex => registry::RegistryExportFormat::SearchIndex,
    };
    let request = registry::RegistryExportRequest {
        registry: export.registry.clone(),
        format,
        out: export.out.clone(),
        namespace: export.namespace.clone(),
        source_files: export.source_files.clone(),
        canonical_types: export.canonical_types.clone(),
        rule_id_prefixes: export.rule_id_prefixes.clone(),
        canonical_iri_prefix: export.canonical_iri_prefix.clone(),
        schema_out: export.schema_out.clone(),
        anti_collapse_test_out: export.anti_collapse_test_out.clone(),
    };

    match registry::export_registry(request) {
        Ok(output) => {
            match export.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => println!("{}", output.render_summary()),
            }
            Ok(0)
        }
        Err(refusal) => {
            let output = refusal.to_canon_output();
            match export.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => eprintln!("{}", serde_json::to_string(&output)?),
            }
            Ok(2)
        }
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

#[derive(Serialize)]
struct ProvidersReport {
    version: &'static str,
    providers: Vec<registry::ProviderCatalogEntry>,
}

#[derive(Serialize)]
struct ProviderSchemaReport {
    version: &'static str,
    #[serde(flatten)]
    schema: registry::ProviderSchema,
}

fn run_registry_providers(providers: &RegistryProvidersCli) -> Result<u8, Box<dyn Error>> {
    let catalog = registry::provider_catalog();
    match providers.emit {
        RegistryEmitMode::Json => {
            let report = ProvidersReport {
                version: "canon_registry_providers.v0",
                providers: catalog,
            };
            println!("{}", serde_json::to_string(&report)?);
        }
        RegistryEmitMode::Summary => {
            println!("registry build providers ({}):", catalog.len());
            for entry in &catalog {
                println!("  {} — {}", entry.id, entry.description);
                println!("    seed columns: {}", entry.seed_columns.join(", "));
                println!(
                    "    schema: canon registry provider-schema {} --emit json",
                    entry.id
                );
            }
        }
    }
    Ok(0)
}

fn run_registry_provider_schema(schema: &RegistryProviderSchemaCli) -> Result<u8, Box<dyn Error>> {
    match registry::provider_schema(&schema.provider) {
        Some(provider_schema) => {
            match schema.emit {
                RegistryEmitMode::Json => {
                    let report = ProviderSchemaReport {
                        version: "canon_registry_provider_schema.v0",
                        schema: provider_schema,
                    };
                    println!("{}", serde_json::to_string(&report)?);
                }
                RegistryEmitMode::Summary => {
                    print!("{}", render_provider_schema_summary(&provider_schema));
                }
            }
            Ok(0)
        }
        None => {
            let available = registry::provider_catalog()
                .into_iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            let output = refusal::create_refusal(
                RefusalCode::EParse,
                format!("Unknown registry build provider '{}'", schema.provider),
                serde_json::json!({
                    "provider": schema.provider,
                    "available_providers": available,
                }),
                Some("canon registry providers --emit json".to_string()),
            );
            match schema.emit {
                RegistryEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
                RegistryEmitMode::Summary => eprintln!("{}", serde_json::to_string(&output)?),
            }
            Ok(2)
        }
    }
}

fn render_provider_schema_summary(schema: &registry::ProviderSchema) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} ({}) — {}",
        schema.name, schema.id, schema.description
    );
    let _ = writeln!(out, "seed columns: {}", schema.seed_columns.join(", "));
    if !schema.id_types.is_empty() {
        let _ = writeln!(out, "id types: {}", schema.id_types.join(", "));
    }
    if !schema.options.is_empty() {
        let _ = writeln!(out, "provider-config options:");
        for option in &schema.options {
            let mut tags = vec![option.value_type.clone()];
            if option.required {
                tags.push("required".to_string());
            }
            if option.secret {
                tags.push("secret".to_string());
            }
            if let Some(env) = &option.env_fallback {
                tags.push(format!("env {env}"));
            }
            if let Some(default) = &option.default {
                tags.push(format!("default {default}"));
            }
            let _ = writeln!(
                out,
                "  {} ({}) — {}",
                option.key,
                tags.join(", "),
                option.description
            );
        }
    }
    for pair in &schema.mutual_exclusions {
        let _ = writeln!(out, "mutually exclusive: {}", pair.join(" | "));
    }
    if let Some(rule) = &schema.interval_encoding {
        let _ = writeln!(out, "interval encoding: {rule}");
    }
    if !schema.examples.is_empty() {
        let _ = writeln!(out, "examples:");
        for example in &schema.examples {
            let config = example
                .provider_config
                .iter()
                .map(|item| format!("--provider-config {item}"))
                .collect::<Vec<_>>()
                .join(" ");
            let _ = writeln!(out, "  {}: {}", example.title, config);
        }
    }
    out
}

#[allow(clippy::result_large_err)]
fn run_entity_run_pipeline(run: &EntityRunCli) -> Result<EntityRunExecution, CanonOutput> {
    match (run.profile.as_deref(), run.work_dir.as_deref()) {
        (Some(profile), Some(work_dir)) => {
            let result = entity::run::run_entity_workbench(entity::run::EntityRunRequest {
                rows: &run.rows,
                profile,
                strategy: &run.strategy,
                registry: &run.registry,
                work_dir,
            })
            .map_err(|refusal| refusal.to_canon_output())?;
            return Ok(EntityRunExecution::Workbench {
                artifact: Box::new(result.artifact),
                candidate_pairs: result.candidate_pairs,
            });
        }
        (None, None) => {}
        _ => {
            return Err(refusal::create_refusal(
                RefusalCode::EEntityInputContract,
                "Artifact-backed entity run requires both --profile and --work-dir".to_string(),
                serde_json::json!({
                    "stage": "run",
                    "profile_present": run.profile.is_some(),
                    "work_dir_present": run.work_dir.is_some(),
                    "writes_performed": false
                }),
                Some(format!(
                    "canon entity run {} --profile <PROFILE> --strategy {} --registry {} --work-dir <DIR>",
                    run.rows.display(),
                    run.strategy.display(),
                    run.registry.display()
                )),
            ));
        }
    }

    let strategy =
        entity_runtime::strategy::load_strategy(&run.strategy).map_err(create_entity_refusal)?;
    let incumbent = entity_runtime::incumbent::load_incumbent_memory(&run.registry)
        .map_err(create_entity_refusal)?;
    let observations = entity_runtime::projection::project_input(&run.rows, &strategy, None, None)
        .map_err(create_entity_refusal)?;
    let candidate_estimate =
        entity_runtime::block::estimate_candidate_block_pairs(&strategy, &observations, &incumbent)
            .map_err(create_entity_refusal)?;
    if candidate_estimate.total_pair_expansions > LEGACY_ENTITY_MAX_CANDIDATE_PAIR_EXPANSIONS {
        return Err(refusal::create_refusal(
            RefusalCode::EEntityCandidateBudget,
            "Legacy entity run candidate budget exceeded before candidate emission".to_string(),
            serde_json::json!({
                "stage": "block",
                "reason": "legacy_candidate_pair_budget_exceeded",
                "policy_id": "legacy_entity_run.max_candidate_pair_expansions",
                "observed": candidate_estimate.total_pair_expansions,
                "configured": LEGACY_ENTITY_MAX_CANDIDATE_PAIR_EXPANSIONS,
                "row_count": observations.len(),
                "strategy_id": strategy.id,
                "strategy_version": strategy.version,
                "max_operator_pair_expansions": candidate_estimate.max_operator_pair_expansions,
                "max_bucket": {
                    "operator_id": candidate_estimate.max_bucket_operator_id,
                    "value": candidate_estimate.max_bucket_value,
                    "row_count": candidate_estimate.max_bucket_row_count,
                    "pair_expansions": candidate_estimate.max_bucket_pair_expansions
                },
                "partial_candidate_artifact_written": false,
                "partial_run_artifact_written": false
            }),
            Some(format!(
                "Use canon entity run {} --profile <PROFILE> --strategy {} --registry {} --work-dir <DIR>, or reduce duplicate physical rows before legacy run",
                run.rows.display(),
                run.strategy.display(),
                run.registry.display()
            )),
        ));
    }
    let blocks =
        entity_runtime::block::build_candidate_blocks(&strategy, &observations, &incumbent)
            .map_err(create_entity_refusal)?;
    let edges = entity_runtime::edge::build_edges(&strategy, &observations, &blocks, &incumbent)
        .map_err(create_entity_refusal)?;
    let artifact = entity_runtime::solve::run(&strategy, &observations, &edges, &incumbent)
        .map_err(create_entity_refusal)?;

    Ok(EntityRunExecution::Legacy {
        artifact: Box::new(artifact),
        candidate_pairs: edges.len() as u64,
    })
}

#[allow(clippy::result_large_err)]
fn run_entity_prepare_pipeline(
    prepare: &EntityPrepareCli,
) -> Result<entity::prepare::PrepareRunArtifact, CanonOutput> {
    entity::prepare::run_prepare(entity::prepare::PrepareRunRequest {
        rows: &prepare.rows,
        profile: &prepare.profile,
        registry: &prepare.registry,
        work_dir: &prepare.work_dir,
    })
    .map_err(|refusal| refusal.to_canon_output())
}

#[allow(clippy::result_large_err)]
fn run_entity_block_pipeline(
    block: &EntityBlockCli,
) -> Result<Vec<entity_runtime::BlockRecord>, CanonOutput> {
    let strategy =
        entity_runtime::strategy::load_strategy(&block.strategy).map_err(create_entity_refusal)?;
    let incumbent = entity_runtime::incumbent::load_incumbent_memory(&block.registry)
        .map_err(create_entity_refusal)?;
    let observations =
        entity_runtime::projection::project_input(&block.rows, &strategy, None, None)
            .map_err(create_entity_refusal)?;

    entity_runtime::block::build_candidate_blocks(&strategy, &observations, &incumbent)
        .map_err(create_entity_refusal)
}

#[allow(clippy::result_large_err)]
fn run_entity_edge_pipeline(
    edge: &EntityEdgeCli,
) -> Result<Vec<entity_runtime::EdgeRecord>, CanonOutput> {
    let strategy =
        entity_runtime::strategy::load_strategy(&edge.strategy).map_err(create_entity_refusal)?;
    let incumbent = entity_runtime::incumbent::load_incumbent_memory(&edge.registry)
        .map_err(create_entity_refusal)?;
    let observations = entity_runtime::projection::project_input(&edge.rows, &strategy, None, None)
        .map_err(create_entity_refusal)?;
    let candidates = read_jsonl_artifact(&edge.candidates, "candidate block artifact")?;

    entity_runtime::edge::build_edges(&strategy, &observations, &candidates, &incumbent)
        .map_err(create_entity_refusal)
}

#[allow(clippy::result_large_err)]
fn run_entity_solve_pipeline(
    solve: &EntitySolveCli,
) -> Result<entity_runtime::SolveRunArtifact, CanonOutput> {
    let strategy =
        entity_runtime::strategy::load_strategy(&solve.strategy).map_err(create_entity_refusal)?;
    let incumbent = entity_runtime::incumbent::load_incumbent_memory(&solve.registry)
        .map_err(create_entity_refusal)?;
    let observations =
        entity_runtime::projection::project_input(&solve.rows, &strategy, None, None)
            .map_err(create_entity_refusal)?;
    let edges = read_jsonl_artifact(&solve.edges, "edge artifact")?;

    entity_runtime::solve::solve(&strategy, &observations, &edges, &incumbent)
        .map_err(create_entity_refusal)
}

#[allow(clippy::result_large_err)]
fn run_entity_audit_pipeline(
    audit: &EntityAuditCli,
) -> Result<entity_runtime::AuditArtifact, CanonOutput> {
    let (result, result_bytes): (entity_runtime::SolveRunArtifact, Vec<u8>) =
        read_json_artifact(&audit.result, "org result artifact")?;

    entity_runtime::audit::audit(
        &result,
        &result_bytes,
        entity_runtime::audit::AuditContext {
            suite_dir: &audit.suite,
            profile: ORG_V1_PROFILE,
            budget_usage: entity_runtime::audit::AuditBudgetUsage {
                runtime_seconds: 0,
                candidate_pairs: 0,
            },
            baseline: None,
            promoted_with_prior_escrow_count: 0,
        },
    )
    .map_err(create_entity_refusal)
}

#[allow(clippy::result_large_err)]
fn run_entity_promote_pipeline(
    promote: &EntityPromoteCli,
) -> Result<entity_runtime::PromoteArtifact, CanonOutput> {
    let (result, result_bytes): (entity_runtime::SolveRunArtifact, Vec<u8>) =
        read_json_artifact(&promote.result, "org result artifact")?;
    let (audit, audit_bytes): (entity_runtime::AuditArtifact, Vec<u8>) =
        read_json_artifact(&promote.audit, "org audit artifact")?;

    entity_runtime::promote::promote(
        &result,
        &result_bytes,
        &audit,
        &audit_bytes,
        &promote.registry,
        &promote.next_version,
    )
    .map_err(create_entity_refusal)
}

#[allow(clippy::result_large_err)]
fn run_entity_review_export_pipeline(
    export: &EntityReviewExportCli,
) -> Result<entity_runtime::review::ReviewExportOutput, CanonOutput> {
    let (result, result_bytes): (entity_runtime::SolveRunArtifact, Vec<u8>) =
        read_json_artifact(&export.result, "org result artifact")?;

    entity_runtime::review::export(
        &result,
        &result_bytes,
        map_entity_review_include(&export.include),
    )
    .map_err(create_entity_refusal)
}

#[allow(clippy::result_large_err)]
fn run_entity_review_import_pipeline(
    import: &EntityReviewImportCli,
) -> Result<entity_runtime::review::ReviewImportOutput, CanonOutput> {
    let review_bytes = read_artifact_bytes(&import.review, "org review artifact")?;
    let audit_data = import
        .audit
        .as_ref()
        .map(|audit_path| {
            read_json_artifact::<entity_runtime::AuditArtifact>(audit_path, "org audit artifact")
        })
        .transpose()?;
    let audit = audit_data
        .as_ref()
        .map(|(audit, bytes)| (audit, bytes.as_slice()));

    entity_runtime::review::import(
        &import.review,
        &review_bytes,
        &import.registry,
        &import.next_version,
        audit,
    )
    .map_err(create_entity_refusal)
}

fn map_entity_review_include(
    include: &EntityReviewInclude,
) -> entity_runtime::review::ReviewInclude {
    match include {
        EntityReviewInclude::Resolved => entity_runtime::review::ReviewInclude::Resolved,
        EntityReviewInclude::Escrow => entity_runtime::review::ReviewInclude::Escrow,
        EntityReviewInclude::Contradictions => {
            entity_runtime::review::ReviewInclude::Contradictions
        }
        EntityReviewInclude::All => entity_runtime::review::ReviewInclude::All,
    }
}

#[allow(clippy::result_large_err)]
fn run_entity_explain_pipeline(
    explain: &EntityExplainCli,
) -> Result<entity_runtime::ExplainArtifact, CanonOutput> {
    let (result, _result_bytes): (serde_json::Value, Vec<u8>) =
        read_json_artifact(&explain.result, "org result artifact")?;
    let query = entity_runtime::ExplainQuery {
        row_id: explain.row.clone(),
        surface_id: explain.surface_id.clone(),
        canonical_id: explain.canon_id.clone(),
        escrow_id: explain.escrow_id.clone(),
    };

    entity_runtime::explain::explain_from_artifact_value(query, result)
        .map_err(create_entity_refusal)
}

fn emit_entity_output(output: &str, summary_mode: bool) {
    if summary_mode {
        println!("{output}");
    } else {
        print!("{output}");
    }
}

fn emit_entity_refusal(
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

fn append_entity_run_witness(
    run: &EntityRunCli,
    artifact: &entity_runtime::SolveRunArtifact,
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
        serde_json::Value::String("entity.run".to_string()),
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
                EntityEmitMode::Json => "json",
                EntityEmitMode::Summary => "summary",
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

fn append_entity_workbench_run_witness(
    run: &EntityRunCli,
    artifact: &entity::run::EntityRunArtifact,
    output: &str,
    runtime_seconds: u64,
    candidate_pairs: u64,
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
        serde_json::Value::String("entity.run".to_string()),
    );
    params.insert(
        "profile_id".to_string(),
        serde_json::Value::String(artifact.metadata.profile.id.clone()),
    );
    params.insert(
        "profile_version".to_string(),
        serde_json::Value::String(artifact.metadata.profile.version.clone()),
    );
    params.insert(
        "strategy_id".to_string(),
        serde_json::Value::String(artifact.metadata.strategy.id.clone()),
    );
    params.insert(
        "strategy_version".to_string(),
        serde_json::Value::String(artifact.metadata.strategy.version.clone()),
    );
    params.insert(
        "strategy_path".to_string(),
        serde_json::Value::String(run.strategy.display().to_string()),
    );
    params.insert(
        "registry_id".to_string(),
        serde_json::Value::String(artifact.metadata.registry_snapshot.id.clone()),
    );
    params.insert(
        "registry_version".to_string(),
        serde_json::Value::String(artifact.metadata.registry_snapshot.version.clone()),
    );
    params.insert(
        "registry_path".to_string(),
        serde_json::Value::String(run.registry.display().to_string()),
    );
    params.insert(
        "registry_lookup_snapshot_hash".to_string(),
        serde_json::Value::String(
            artifact
                .metadata
                .registry_snapshot
                .lookup_snapshot_hash
                .clone(),
        ),
    );
    if let Some(work_dir) = &run.work_dir {
        params.insert(
            "work_dir".to_string(),
            serde_json::Value::String(work_dir.display().to_string()),
        );
    }
    params.insert(
        "run_artifact_hash".to_string(),
        serde_json::Value::String(artifact.artifact_content_hash.clone()),
    );
    params.insert(
        "candidate_pairs".to_string(),
        serde_json::Value::from(candidate_pairs),
    );
    params.insert(
        "runtime_seconds".to_string(),
        serde_json::Value::from(runtime_seconds),
    );

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

/// Long flags an agent commonly types on the core resolve command. Used to
/// turn clap's generic "unexpected argument" into a did-you-mean suggestion.
const KNOWN_CORE_FLAGS: [&str; 14] = [
    "registry",
    "column",
    "emit",
    "canon-column",
    "map-out",
    "max-rows",
    "max-bytes",
    "no-witness",
    "explicit",
    "plain-json-values",
    "version",
    "describe",
    "schema",
    "help",
];

/// Top-level subcommands, for disambiguating a misspelled subcommand that clap
/// otherwise swallows as the optional positional input.
const KNOWN_SUBCOMMANDS: [&str; 5] = ["doctor", "resolve", "registry", "entity", "strategy"];

/// Classic dynamic-programming Levenshtein edit distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Nearest known item to `candidate` within `max_distance`, preferring the
/// smallest distance then declaration order. Returns `None` on an exact match
/// (nothing to suggest) or when nothing is close enough.
fn nearest<'a>(candidate: &str, options: &[&'a str], max_distance: usize) -> Option<&'a str> {
    let mut best: Option<(usize, &str)> = None;
    for option in options {
        let distance = levenshtein(candidate, option);
        if distance == 0 {
            return None;
        }
        if distance <= max_distance && best.is_none_or(|(d, _)| distance < d) {
            best = Some((distance, option));
        }
    }
    best.map(|(_, option)| option)
}

/// Suggest the nearest known long flag for an unknown `--flag` token.
pub fn suggest_flag(unknown: &str) -> Option<&'static str> {
    let stripped = unknown.trim_start_matches('-');
    nearest(stripped, &KNOWN_CORE_FLAGS, 1)
}

/// Suggest the nearest known subcommand for a misspelled first token.
fn suggest_subcommand(token: &str) -> Option<&'static str> {
    nearest(token, &KNOWN_SUBCOMMANDS, 1)
}

/// Turn a clap `UnknownArgument` error into an agent-friendly did-you-mean
/// message naming the exact corrected flag, or `None` to defer to clap.
pub fn unknown_flag_suggestion(error: &clap::Error) -> Option<String> {
    use clap::error::{ContextKind, ContextValue};
    if error.kind() != clap::error::ErrorKind::UnknownArgument {
        return None;
    }
    let invalid = match error.get(ContextKind::InvalidArg)? {
        ContextValue::String(value) => value.clone(),
        ContextValue::Strings(values) => values.first()?.clone(),
        _ => return None,
    };
    let suggestion = suggest_flag(&invalid)?;
    Some(format!(
        "error: unknown flag '{invalid}'\n\n  did you mean '--{suggestion}'?\n\nFor more information, try '--help'."
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayMode {
    Version,
    Describe,
    Schema,
    /// Bare `canon` with no arguments: print agent-oriented orientation
    /// instead of a bare clap "required arguments" error.
    Orientation,
}

pub fn detect_display_mode<I, T>(args: I) -> Option<DisplayMode>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<OsString>>();

    // No user arguments at all (just the program name): orient the caller
    // toward the canonical command and the machine-readable surfaces.
    if args.len() <= 1 {
        return Some(DisplayMode::Orientation);
    }

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
        DisplayMode::Orientation => {
            // Diagnostics to stderr (stdout stays clean for pipelines); exit 2
            // signals "no task performed", consistent with a usage error.
            eprintln!(
                "canon: resolve messy identifiers to canonical IDs against versioned registries.\n\
                 \n\
                 No input given. Start with one of:\n\
                 \x20\x20canon <INPUT> --registry <DIR> --column <COLUMN>   resolve a CSV/JSONL column\n\
                 \x20\x20canon --help                                       full usage and flags\n\
                 \x20\x20canon doctor --robot-triage                        machine-readable capabilities + health\n\
                 \x20\x20canon --describe                                   operator.json contract\n\
                 \x20\x20canon --schema                                     canon.v0 output JSON Schema"
            );
            return Ok(2);
        }
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
                    "redacted": {
                        "type": "boolean",
                        "description": "Present on RESOLVED/PARTIAL/UNRESOLVED outputs. true when input and canonical_id values are masked as \"[REDACTED]\" (the zero-retention default); re-run with --explicit to reveal them. Absent on REFUSAL."
                    },
                    "mappings": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "input": { "type": "string" },
                                "input_encoding": {
                                    "type": "string",
                                    "enum": ["utf8", "hex"],
                                    "description": "Present only when --plain-json-values is used with --explicit. Describes how to interpret input."
                                },
                                "canonical_id": { "type": "string" },
                                "canonical_id_encoding": {
                                    "type": "string",
                                    "enum": ["utf8", "hex"],
                                    "description": "Present only when --plain-json-values is used with --explicit. Describes how to interpret canonical_id."
                                },
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
                                "input_encoding": {
                                    "type": "string",
                                    "enum": ["utf8", "hex"],
                                    "description": "Present only when --plain-json-values is used with --explicit and input is not null."
                                },
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
        resolve_result.summary.resolved + resolve_result.summary.unresolved > 0,
        "Empty input should have been caught by input module"
    );

    // Step 10: Emit output
    let output_hash = match cli.emit {
        crate::cli::EmitMode::Json => {
            // JSON mode: emit to stdout with hash
            let json_output = output::json::emit_json_explicit_with_plain_values(
                &registry.meta,
                &resolve_result,
                cli.explicit,
                cli.plain_json_values,
            )
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
                let json_output = output::json::emit_json_explicit_with_plain_values(
                    &registry.meta,
                    &resolve_result,
                    cli.explicit,
                    cli.plain_json_values,
                )
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
            "plain_json_values".to_string(),
            serde_json::Value::Bool(cli.plain_json_values),
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
    // An unknown --source carries available_sources in its detail; point the
    // agent at the discovery command rather than the generic parse hint.
    let next_command = error
        .detail
        .get("available_sources")
        .map(|_| "canon registry providers --emit json".to_string());
    refusal::create_refusal(code, error.message, error.detail, next_command)
}

fn create_input_refusal(error: input::InputError) -> CanonOutput {
    match error {
        // Route through the rich builders: identical message/detail, plus a
        // dynamic recovery path (the column builder suggests an existing column).
        input::InputError::ColumnNotFound { column, available } => {
            Refusal::column_not_found(&column, &available).to_canon_output()
        }
        input::InputError::TooLarge {
            limit_type,
            limit,
            actual,
        } => Refusal::too_large(&limit_type, &limit, &actual).to_canon_output(),
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

fn create_entity_refusal(error: entity_runtime::EntityError) -> CanonOutput {
    let detail = error.detail.unwrap_or_else(|| serde_json::json!({}));
    let message = error.message;

    match error.code {
        entity_runtime::EntityErrorCode::InputContract => {
            Refusal::entity_input_contract(message, detail).to_canon_output()
        }
        entity_runtime::EntityErrorCode::Strategy => {
            Refusal::entity_bad_strategy(message, detail).to_canon_output()
        }
        entity_runtime::EntityErrorCode::Audit => {
            let lowercase = message.to_ascii_lowercase();
            if lowercase.contains("fixture")
                || lowercase.contains("row catalog")
                || lowercase.contains("source_row_id")
            {
                Refusal::entity_fixture_invalid(message, detail).to_canon_output()
            } else {
                Refusal::entity_bad_suite(message, detail).to_canon_output()
            }
        }
        entity_runtime::EntityErrorCode::Promotion => {
            let lowercase = message.to_ascii_lowercase();
            if lowercase.contains("stale") {
                Refusal::entity_stale_registry(message, detail).to_canon_output()
            } else if lowercase.contains("next-version")
                || lowercase.contains("next version")
                || lowercase.contains("version bump")
                || lowercase.contains("differ from the current")
            {
                Refusal::entity_version_bump_required(message, detail).to_canon_output()
            } else {
                refusal::create_refusal(RefusalCode::EEntityArtifactContract, message, detail, None)
            }
        }
        entity_runtime::EntityErrorCode::Registry => refusal::create_refusal(
            RefusalCode::EBadRegistry,
            message,
            detail,
            Some("Check the entity registry sidecars and rerun the canon entity command".to_string()),
        ),
        entity_runtime::EntityErrorCode::ArtifactContract => refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            message,
            detail,
            Some("Regenerate the referenced entity artifact with matching strategy and registry inputs, then rerun".to_string()),
        ),
        entity_runtime::EntityErrorCode::Explain => refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            message,
            detail,
            Some("Check the explain selector and result artifact, then rerun canon entity explain".to_string()),
        ),
        entity_runtime::EntityErrorCode::Unimplemented => refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            message,
            detail,
            Some("Use an implemented canon entity subcommand or update the runtime wiring first".to_string()),
        ),
    }
}

fn create_resolve_refusal(error: resolve::ResolveError) -> CanonOutput {
    let detail = error.detail.unwrap_or_else(|| serde_json::json!({}));
    let message = error.message;

    match error.code {
        resolve::ResolveErrorCode::Io => refusal::create_refusal(
            RefusalCode::EIo,
            message,
            detail,
            Some("Check resolve tape, strategy, and registry paths, then rerun canon resolve".to_string()),
        ),
        resolve::ResolveErrorCode::Parse => refusal::create_refusal(
            RefusalCode::EParse,
            message,
            detail,
            Some("Use supported resolve tape formats (.csv, .tsv, .jsonl, .ndjson) and valid JSON/YAML, then rerun canon resolve".to_string()),
        ),
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
        resolve::ResolveErrorCode::TooLarge => refusal::create_refusal(
            RefusalCode::ETooLarge,
            message,
            detail,
            Some("Increase --max-rows or --max-bytes, or reduce the resolve tapes, then rerun canon resolve".to_string()),
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
    EEntityProfile,
    EEntityStrategy,
    EEntityInputContract,
    EEntitySurfaceIdCollision,
    EEntityPatchConflict,
    EEntityRegistrySnapshot,
    EEntityCacheMismatch,
    EEntityIndexLimit,
    EEntityCandidateBudget,
    EEntityArtifactContract,
    EEntityCannotLinkOverride,
    EEntityReviewImport,
    EEntityAuditGate,
    EEntityApplyUnresolved,
    EEntityIoBudget,
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
            RefusalCode::EEntityProfile => "E_ENTITY_PROFILE",
            RefusalCode::EEntityStrategy => "E_ENTITY_STRATEGY",
            RefusalCode::EEntityInputContract => "E_ENTITY_INPUT_CONTRACT",
            RefusalCode::EEntitySurfaceIdCollision => "E_ENTITY_SURFACE_ID_COLLISION",
            RefusalCode::EEntityPatchConflict => "E_ENTITY_PATCH_CONFLICT",
            RefusalCode::EEntityRegistrySnapshot => "E_ENTITY_REGISTRY_SNAPSHOT",
            RefusalCode::EEntityCacheMismatch => "E_ENTITY_CACHE_MISMATCH",
            RefusalCode::EEntityIndexLimit => "E_ENTITY_INDEX_LIMIT",
            RefusalCode::EEntityCandidateBudget => "E_ENTITY_CANDIDATE_BUDGET",
            RefusalCode::EEntityArtifactContract => "E_ENTITY_ARTIFACT_CONTRACT",
            RefusalCode::EEntityCannotLinkOverride => "E_ENTITY_CANNOT_LINK_OVERRIDE",
            RefusalCode::EEntityReviewImport => "E_ENTITY_REVIEW_IMPORT",
            RefusalCode::EEntityAuditGate => "E_ENTITY_AUDIT_GATE",
            RefusalCode::EEntityApplyUnresolved => "E_ENTITY_APPLY_UNRESOLVED",
            RefusalCode::EEntityIoBudget => "E_ENTITY_IO_BUDGET",
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

impl RefusalCode {
    /// Generic, copy-paste-ready recovery hint for this refusal code.
    ///
    /// Every refusal is an operator handoff, not a dead end: when a refusal is
    /// constructed without a more specific `next_command`, `create_refusal`
    /// falls back to this so the envelope always carries a recovery path. Call
    /// sites that can compute a sharper hint (e.g. suggesting an existing
    /// column) still pass their own `Some(..)` and take precedence.
    pub fn default_next_command(&self) -> &'static str {
        match self {
            RefusalCode::EIo => {
                "Check the input and registry paths and permissions, then rerun the same canon command"
            }
            RefusalCode::EEncoding => {
                "Convert or re-export the input as UTF-8, then rerun the same canon command"
            }
            RefusalCode::ECsvParse => "Re-export the input as standard CSV, then rerun canon",
            RefusalCode::EBadRegistry => {
                "Fix registry.json or the mapping files in the registry directory, then rerun canon"
            }
            RefusalCode::EColumnNotFound => {
                "Pass --column with a column that exists in the input (see detail.available_columns)"
            }
            RefusalCode::EParse => {
                "Use a supported input extension (.csv, .tsv, .jsonl, .ndjson) with valid content, then rerun canon"
            }
            RefusalCode::EEmptyInput => {
                "Provide an input file with data rows in the selected column, then rerun canon"
            }
            RefusalCode::ETooLarge => {
                "Increase --max-rows/--max-bytes or reduce the input size, then rerun canon"
            }
            RefusalCode::EEmitFormat => {
                "Use --emit json for JSONL input, or provide CSV input for --emit csv"
            }
            RefusalCode::EColumnExists => {
                "Choose a --canon-column name that is not already present in the input header"
            }
            RefusalCode::EEntityProfile => {
                "Run canon entity profile list or fix the strategy profile block, then rerun canon entity"
            }
            RefusalCode::EEntityStrategy => {
                "Fix the entity strategy YAML or unsupported operator, then rerun canon entity"
            }
            RefusalCode::EEntityInputContract => {
                "Fix the entity input rows or profile field mapping, then rerun canon entity prepare"
            }
            RefusalCode::EEntitySurfaceIdCollision => {
                "Inspect the collision detail and adjust surface_id derivation before rerunning canon entity prepare"
            }
            RefusalCode::EEntityPatchConflict => {
                "Resolve alias, distinctness, or relation patch conflicts before rerunning canon entity"
            }
            RefusalCode::EEntityRegistrySnapshot => {
                "Re-run the entity stage against the current registry snapshot or use the matching registry"
            }
            RefusalCode::EEntityCacheMismatch => {
                "Rebuild the entity cache or use the work directory matching this input/profile/strategy/registry"
            }
            RefusalCode::EEntityIndexLimit => {
                "Tighten index settings or raise explicit posting/bucket limits, then rerun canon entity index/block"
            }
            RefusalCode::EEntityCandidateBudget => {
                "Adjust blocking operators or candidate caps, then rerun canon entity block"
            }
            RefusalCode::EEntityArtifactContract => {
                "Use the correct upstream entity artifact or re-run the prior stage"
            }
            RefusalCode::EEntityCannotLinkOverride => {
                "Keep the surfaces in review or add explicit operator override evidence before merging"
            }
            RefusalCode::EEntityReviewImport => {
                "Export a fresh review queue or repair the review file before rerunning canon entity review import"
            }
            RefusalCode::EEntityAuditGate => {
                "Re-run entity audit and fix failures before promotion"
            }
            RefusalCode::EEntityApplyUnresolved => {
                "Promote more aliases or rerun canon entity apply with partial output allowed"
            }
            RefusalCode::EEntityIoBudget => {
                "Increase explicit row/byte/artifact limits or process the corpus in physical batches"
            }
            RefusalCode::EStrategyInputContract => {
                "Fix the strategy inputs to match the contract, then rerun canon strategy"
            }
            RefusalCode::EStrategyProofInvalid => {
                "Regenerate passing verify/assess/airlock proof artifacts, then rerun canon strategy register"
            }
            RefusalCode::EStrategyVersionBumpRequired => {
                "Pass --next-version, then rerun canon strategy register"
            }
            RefusalCode::EBadStrategy => "Fix the strategy YAML, then rerun canon with --strategy",
            RefusalCode::ETooManyCandidates => {
                "Tighten candidate_filter or raise --max-candidates, then rerun canon resolve"
            }
            RefusalCode::EEmptyTape => {
                "Provide reference and target tapes with processable records, then rerun canon resolve"
            }
            RefusalCode::EIncompatibleTapes => {
                "Fix strategy field mappings so the tapes share comparable fields, then rerun canon resolve"
            }
        }
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

    #[test]
    fn detect_display_mode_orients_on_no_arguments() {
        assert_eq!(
            detect_display_mode(["canon"]),
            Some(DisplayMode::Orientation)
        );
        // A real invocation (positional input) is not intercepted.
        assert_eq!(detect_display_mode(["canon", "tape.csv"]), None);
        // A subcommand is not intercepted either.
        assert_eq!(detect_display_mode(["canon", "doctor"]), None);
    }

    #[test]
    fn suggest_flag_corrects_one_edit_typos() {
        assert_eq!(super::suggest_flag("--regisry"), Some("registry"));
        assert_eq!(super::suggest_flag("--colum"), Some("column"));
        assert_eq!(super::suggest_flag("--explcit"), Some("explicit"));
        assert_eq!(
            super::suggest_flag("--plain-json-value"),
            Some("plain-json-values")
        );
        // Exact spelling has nothing to suggest.
        assert_eq!(super::suggest_flag("--registry"), None);
        // Nonsense is left to clap (no near match).
        assert_eq!(super::suggest_flag("--zzzzzz"), None);
    }

    #[test]
    fn suggest_subcommand_corrects_one_edit_typos() {
        assert_eq!(super::suggest_subcommand("regstry"), Some("registry"));
        assert_eq!(super::suggest_subcommand("doctr"), Some("doctor"));
        assert_eq!(super::suggest_subcommand("resolv"), Some("resolve"));
        assert_eq!(super::suggest_subcommand("registry"), None);
        // Transpositions are distance 2 under Levenshtein, so not suggested.
        assert_eq!(super::suggest_subcommand("ogr"), None);
        // A real data filename is far from any subcommand.
        assert_eq!(super::suggest_subcommand("positions.csv"), None);
    }
}
