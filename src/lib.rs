#![forbid(unsafe_code)]

pub mod cli;
pub mod distribution {
    pub mod backend;
    pub mod cache;
    pub mod mirror;
    pub mod oci;
    pub mod package;
    pub mod remote;
    pub mod trust;
}
pub mod doctor;
pub mod entity;
pub mod evaluation;
pub mod extensions;
mod fs_safety;
pub mod geo;
pub mod identity_scope;
pub mod inbox;
pub mod input;
pub mod lookup;
pub mod namekit;
pub mod operator;
pub mod output;
pub mod paths;
pub mod project;
pub mod refusal;
pub mod registry;
pub mod registry_lint;
#[doc(hidden)]
pub mod resolve;
pub mod sdk;
pub mod temporal;
pub mod strategy {
    pub mod tournament;
    pub mod types;
}
pub mod strategy_audit;
pub mod strategy_profile;
pub mod strategy_registry;
pub mod witness;

use crate::cli::{
    CanonCommand, Cli, EntityAliasWithholdingCli, EntityApplyCli, EntityAuditCli, EntityBlockCli,
    EntityBlockPreflightCli, EntityBlockSubcommand, EntityCacheModeArg, EntityCalibrateCommand,
    EntityCalibrateSubcommand, EntityCalibrateSweepCli, EntityCandidateRecallCli, EntityCommand,
    EntityEmitMode, EntityEvidenceCli, EntityExplainCli, EntityGeneralizationCli,
    EntityIndexBuildCli, EntityIndexCommand, EntityIndexSubcommand, EntityLinkCli,
    EntityPrepareCli, EntityProfileCommand, EntityProfileInitCli, EntityProfileListCli,
    EntityProfileSubcommand, EntityPromoteCli, EntityReviewCommand, EntityReviewExportArtifact,
    EntityReviewExportCli, EntityReviewExportEmitMode, EntityReviewGroupBy, EntityReviewImportCli,
    EntityReviewInclude, EntityReviewSubcommand, EntityRunCli, EntitySolveCli,
    EntityStreamEmitMode, EntitySubcommand, PackageCli, PackagePackCli, PackageSubcommand,
    RegistryAddEntryCli, RegistryAuditCli, RegistryBuildCli, RegistryDefaultIdSchemeCli,
    RegistryDiffCli, RegistryEmitMode, RegistryExportCli, RegistryExportFormatCli, RegistryLintCli,
    RegistryLintProfile, RegistryMintCli, RegistryNextIdCli, RegistryPlainJsonEmitMode,
    RegistryProviderSchemaCli, RegistryProvidersCli, RegistrySubcommand, RegistryVersionBumpMode,
    StrategyAuditCli, StrategyCommand, StrategyDeprecateCli, StrategyDiffCli, StrategyExplainCli,
    StrategyGradeArg, StrategyKeyTypeArg, StrategyListCli, StrategyProfileCli, StrategyPromoteCli,
    StrategyRegisterCli, StrategyResolveCli, StrategyStatusArg, StrategySubcommand,
    StrategyUpdateCli,
};
use crate::entity::runtime as entity_runtime;
use serde::{Deserialize, Serialize, Serializer, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    ffi::OsString,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    time::Instant,
};

const ORG_V1_PROFILE: &str = "bdc_issuer";

struct EntityRunExecution {
    artifact: Box<entity::run::EntityRunArtifact>,
    artifact_value: serde_json::Value,
    candidate_pairs: u64,
}

fn entity_index_cache_mode(mode: EntityCacheModeArg) -> entity::index::EntityIndexCacheMode {
    match mode {
        EntityCacheModeArg::Enabled => entity::index::EntityIndexCacheMode::Enabled,
        EntityCacheModeArg::Disabled => entity::index::EntityIndexCacheMode::Disabled,
    }
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
        CanonCommand::Package(package) => run_package_command(package),
        CanonCommand::Project(project) => project::cli::run(project),
        CanonCommand::Geo(geo) => geo::cli::run(geo),
        CanonCommand::Inbox(inbox) => inbox::cli::run(inbox),
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

fn run_package_command(package: &PackageCli) -> Result<u8, Box<dyn Error>> {
    match &package.command {
        PackageSubcommand::Pack(args) => run_package_pack_command(args),
        PackageSubcommand::Inspect(args) => {
            let archive_bytes = fs::read(&args.archive)?;
            let inspection = distribution::package::inspect_local_package(&archive_bytes)?;
            emit_package_inspection(&inspection, &args.emit)?;
            Ok(0)
        }
        PackageSubcommand::Verify(args) => {
            let archive_bytes = fs::read(&args.archive)?;
            let verification = distribution::package::verify_local_package(&archive_bytes)?;
            emit_package_verification(&verification, &args.emit)?;
            Ok(0)
        }
        PackageSubcommand::Unpack(args) => {
            let archive_bytes = fs::read(&args.archive)?;
            let verification =
                distribution::package::unpack_local_package(&archive_bytes, &args.target)?;
            emit_package_verification(&verification, &args.emit)?;
            Ok(0)
        }
        PackageSubcommand::Push(args) => {
            let archive_bytes = fs::read(&args.archive)?;
            let remote = distribution::remote::OciRemote::new(&args.registry, &args.repository);
            let receipt = distribution::remote::publish_package_by_immutable_digest(
                &remote,
                &archive_bytes,
                args.tag.as_deref(),
                distribution::remote::OciRemotePolicy::online(),
            )?;
            emit_package_publish_receipt(&receipt, &args.emit)?;
            Ok(0)
        }
        PackageSubcommand::Pull(args) => {
            let remote = distribution::remote::OciRemote::new(&args.registry, &args.repository);
            let cache = distribution::cache::ContentCache::new(&args.cache);
            let receipt = if let Some(digest) = &args.digest {
                distribution::remote::pull_package_by_immutable_digest(
                    &remote,
                    digest,
                    &cache,
                    distribution::remote::OciRemotePolicy::online(),
                )?
            } else if let Some(tag) = &args.tag {
                let resolved = distribution::remote::resolve_tag_once(
                    &remote,
                    tag,
                    distribution::remote::OciRemotePolicy::online(),
                )?;
                distribution::remote::pull_resolved_package(
                    &remote,
                    &resolved,
                    &cache,
                    distribution::remote::OciRemotePolicy::online(),
                )?
            } else {
                return Err("package pull requires --digest or --tag".into());
            };
            emit_package_pull_receipt(&receipt, &args.emit)?;
            Ok(0)
        }
    }
}

fn run_package_pack_command(args: &PackagePackCli) -> Result<u8, Box<dyn Error>> {
    let package_bytes = match fs::read(&args.package) {
        Ok(bytes) => bytes,
        Err(error) => {
            return emit_package_refusal(package_pack_io_refusal(
                "read_package",
                &args.package,
                error,
                args,
            ));
        }
    };
    let archive_bytes = match distribution::package::pack_local_package(&args.root, &package_bytes)
    {
        Ok(bytes) => bytes,
        Err(error) => return emit_package_refusal(package_pack_contract_refusal(error, args)),
    };
    if let Err(error) = fs::write(&args.out, archive_bytes) {
        return emit_package_refusal(package_pack_io_refusal(
            "write_archive",
            &args.out,
            error,
            args,
        ));
    }
    Ok(0)
}

fn emit_package_refusal(refusal_output: CanonOutput) -> Result<u8, Box<dyn Error>> {
    eprintln!("{}", serde_json::to_string(&refusal_output)?);
    Ok(2)
}

fn package_pack_io_refusal(
    action: &str,
    path: &Path,
    error: std::io::Error,
    args: &PackagePackCli,
) -> CanonOutput {
    let path = path.display().to_string();
    refusal::create_refusal(
        RefusalCode::EIo,
        format!("package pack failed during {action}: {error}"),
        serde_json::json!({
            "command": "canon package pack",
            "action": action,
            "path": path,
            "root": args.root.display().to_string(),
            "package": args.package.display().to_string(),
            "out": args.out.display().to_string(),
            "error": error.to_string()
        }),
        Some(format!(
            "Check package pack paths and permissions, then {}",
            package_pack_command(args, &args.package)
        )),
    )
}

fn package_pack_contract_refusal(
    error: distribution::package::LocalPackageError,
    args: &PackagePackCli,
) -> CanonOutput {
    let error_kind = error.kind;
    let error_kind_name = package_error_kind_name(&error_kind);
    let code = package_error_refusal_code(&error_kind);
    let next_command = package_error_next_command(&error_kind, args);
    refusal::create_refusal(
        code,
        format!("package pack refused: {}", error.message),
        serde_json::json!({
            "command": "canon package pack",
            "root": args.root.display().to_string(),
            "package": args.package.display().to_string(),
            "out": args.out.display().to_string(),
            "package_error_kind": error_kind_name,
            "reason": error.message
        }),
        Some(next_command),
    )
}

fn package_error_refusal_code(kind: &distribution::package::LocalPackageErrorKind) -> RefusalCode {
    match kind {
        distribution::package::LocalPackageErrorKind::Io => RefusalCode::EIo,
        distribution::package::LocalPackageErrorKind::Parse => RefusalCode::EParse,
        distribution::package::LocalPackageErrorKind::NonCanonicalPackageBytes => {
            RefusalCode::EPackageNonCanonical
        }
        _ => RefusalCode::EPackageContract,
    }
}

fn package_error_next_command(
    kind: &distribution::package::LocalPackageErrorKind,
    args: &PackagePackCli,
) -> String {
    match kind {
        distribution::package::LocalPackageErrorKind::NonCanonicalPackageBytes => {
            package_canonicalization_next_command(args)
        }
        distribution::package::LocalPackageErrorKind::Parse => format!(
            "Fix {} as valid package JSON, then {}",
            shell_path(&args.package),
            package_pack_command(args, &args.package)
        ),
        distribution::package::LocalPackageErrorKind::Io => format!(
            "Check package root files and permissions, then {}",
            package_pack_command(args, &args.package)
        ),
        _ => format!(
            "Fix package fields, digests, paths, and archive constraints in {}, then {}",
            shell_path(&args.package),
            package_pack_command(args, &args.package)
        ),
    }
}

fn package_canonicalization_next_command(args: &PackagePackCli) -> String {
    let canonical_package = args.package.with_extension("canonical.json");
    format!(
        "python3 -c 'import json,sys; data=json.load(open(sys.argv[1],encoding=\"utf-8\")); sys.stdout.write(json.dumps(data, ensure_ascii=False, sort_keys=True, separators=(\",\",\":\")))' {} > {} && {}",
        shell_path(&args.package),
        shell_path(&canonical_package),
        package_pack_command(args, &canonical_package)
    )
}

fn package_pack_command(args: &PackagePackCli, package: &Path) -> String {
    format!(
        "canon package pack --root {} --package {} --out {}",
        shell_path(&args.root),
        shell_path(package),
        shell_path(&args.out)
    )
}

fn shell_path(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\"'\"'"))
}

fn package_error_kind_name(kind: &distribution::package::LocalPackageErrorKind) -> &'static str {
    match kind {
        distribution::package::LocalPackageErrorKind::UnsupportedArchiveVersion => {
            "unsupported_archive_version"
        }
        distribution::package::LocalPackageErrorKind::MissingPackageField => {
            "missing_package_field"
        }
        distribution::package::LocalPackageErrorKind::NonCanonicalPackageBytes => {
            "non_canonical_package_bytes"
        }
        distribution::package::LocalPackageErrorKind::SemanticContract => "semantic_contract",
        distribution::package::LocalPackageErrorKind::PathTraversal => "path_traversal",
        distribution::package::LocalPackageErrorKind::DuplicatePath => "duplicate_path",
        distribution::package::LocalPackageErrorKind::LinkRejected => "link_rejected",
        distribution::package::LocalPackageErrorKind::HardLinkRejected => "hard_link_rejected",
        distribution::package::LocalPackageErrorKind::InvalidMode => "invalid_mode",
        distribution::package::LocalPackageErrorKind::InvalidContentDigest => {
            "invalid_content_digest"
        }
        distribution::package::LocalPackageErrorKind::NonEmptyTarget => "non_empty_target",
        distribution::package::LocalPackageErrorKind::MissingTarget => "missing_target",
        distribution::package::LocalPackageErrorKind::TargetNotDirectory => "target_not_directory",
        distribution::package::LocalPackageErrorKind::Io => "io",
        distribution::package::LocalPackageErrorKind::Parse => "parse",
    }
}

fn emit_package_inspection(
    inspection: &distribution::package::LocalPackageInspection,
    emit: &RegistryEmitMode,
) -> Result<(), Box<dyn Error>> {
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(inspection)?),
        RegistryEmitMode::Summary => {
            let bytes = inspection
                .inventory
                .iter()
                .map(|file| file.bytes)
                .sum::<u64>();
            println!(
                "{}@{} archive {} | {} files, {} bytes, {} dependencies, {} capabilities",
                inspection.package.package_id,
                inspection.package.package_version,
                inspection.archive_digest,
                inspection.inventory.len(),
                bytes,
                inspection.dependencies.len(),
                inspection.capabilities.len()
            );
        }
    }
    Ok(())
}

fn emit_package_verification(
    verification: &distribution::package::LocalPackageVerification,
    emit: &RegistryEmitMode,
) -> Result<(), Box<dyn Error>> {
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(verification)?),
        RegistryEmitMode::Summary => {
            println!(
                "{} verified | {} files, {} bytes, package {}, bytes {}",
                verification.archive_digest,
                verification.verified_files,
                verification.verified_bytes,
                verification.package_content_digest,
                verification.package_bytes_digest
            );
        }
    }
    Ok(())
}

fn emit_package_publish_receipt(
    receipt: &distribution::remote::OciPublishReceipt,
    emit: &RegistryEmitMode,
) -> Result<(), Box<dyn Error>> {
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(receipt)?),
        RegistryEmitMode::Summary => {
            let tag = receipt.tag.as_deref().unwrap_or("-");
            println!(
                "{}@{} published | archive {}, package {}, tag {}, pushed {} blobs, reused {} blobs",
                receipt.repository,
                receipt.manifest_digest,
                receipt.package_archive_digest,
                receipt.package_content_digest,
                tag,
                receipt.pushed_blobs.len(),
                receipt.reused_blobs.len()
            );
        }
    }
    Ok(())
}

fn emit_package_pull_receipt(
    receipt: &distribution::remote::OciPullReceipt,
    emit: &RegistryEmitMode,
) -> Result<(), Box<dyn Error>> {
    match emit {
        RegistryEmitMode::Json => println!("{}", serde_json::to_string(receipt)?),
        RegistryEmitMode::Summary => {
            let resolved = receipt
                .resolved_from_tag
                .as_ref()
                .map(|reference| reference.tag.as_str())
                .unwrap_or("-");
            println!(
                "{}@{} pulled | archive {}, package {}, cached {}, files {}, bytes {}, resolved-tag {}",
                receipt.repository,
                receipt.manifest_digest,
                receipt.package_archive_digest,
                receipt.package_content_digest,
                receipt.package_cache_path,
                receipt.verified_files,
                receipt.verified_bytes,
                resolved
            );
        }
    }
    Ok(())
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
        EntitySubcommand::Index(index) => run_entity_index_command(index),
        EntitySubcommand::Block(block) => run_entity_block_command(block),
        EntitySubcommand::CandidateRecall(recall) => run_entity_candidate_recall_command(recall),
        EntitySubcommand::AliasWithholding(alias_withholding) => {
            run_entity_alias_withholding_command(alias_withholding)
        }
        EntitySubcommand::Generalization(generalization) => {
            run_entity_generalization_command(generalization)
        }
        EntitySubcommand::Calibrate(calibrate) => run_entity_calibrate_command(calibrate),
        EntitySubcommand::Evidence(evidence) => run_entity_evidence_command(evidence),
        EntitySubcommand::Solve(solve) => run_entity_solve_command(solve),
        EntitySubcommand::Link(link) => run_entity_link_command(link),
        EntitySubcommand::Audit(audit) => run_entity_audit_command(audit),
        EntitySubcommand::Promote(promote) => run_entity_promote_command(promote),
        EntitySubcommand::Apply(apply) => run_entity_apply_command(apply),
        EntitySubcommand::Explain(explain) => run_entity_explain_command(explain),
        EntitySubcommand::Profile(profile) => run_entity_profile_command(profile),
        EntitySubcommand::Review(review) => run_entity_review_command(review),
    }
}

fn run_entity_run_command(run: &EntityRunCli) -> Result<u8, Box<dyn Error>> {
    let started = Instant::now();

    match run_entity_run_pipeline(run) {
        Ok(EntityRunExecution {
            artifact,
            artifact_value,
            candidate_pairs,
        }) => {
            let output = match run.emit {
                EntityEmitMode::Json => serde_json::to_string(&artifact_value)?,
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

fn run_entity_index_command(index: &EntityIndexCommand) -> Result<u8, Box<dyn Error>> {
    match &index.command {
        EntityIndexSubcommand::Build(build) => run_entity_index_build_command(build),
    }
}

fn run_entity_index_build_command(build: &EntityIndexBuildCli) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(build.emit, EntityEmitMode::Summary);
    let (Some(profile), Some(work_dir)) = (build.profile.as_ref(), build.work_dir.as_ref()) else {
        return emit_entity_refusal(
            entity_missing_v1_context_refusal(
                entity::EntityArtifactStageV1::Index,
                &build.rows,
                &build.profile,
                &build.strategy,
                &build.registry,
                &build.work_dir,
                None,
            ),
            true,
            summary_mode,
        );
    };

    match entity::index::run_index_build_v1(entity::index::EntityIndexBuildRequest {
        rows: &build.rows,
        profile,
        strategy: &build.strategy,
        registry: &build.registry,
        work_dir,
        max_artifact_bytes: None,
    }) {
        Ok(result) => {
            let output = match build.emit {
                EntityEmitMode::Json => {
                    serde_json::to_string(&entity::index::index_build_v1_report(&result))?
                }
                EntityEmitMode::Summary => render_entity_index_build_v1_summary(&result),
            };
            emit_entity_output(&output, summary_mode);
            Ok(0)
        }
        Err(refusal) => emit_entity_refusal(refusal.to_canon_output(), true, summary_mode),
    }
}

fn run_entity_block_command(block: &EntityBlockCli) -> Result<u8, Box<dyn Error>> {
    if let Some(EntityBlockSubcommand::Preflight(preflight)) = &block.command {
        return run_entity_block_preflight_command(preflight);
    }
    let summary_mode = matches!(block.emit, EntityStreamEmitMode::Summary);
    let (Some(rows), Some(strategy), Some(registry)) = (
        block.rows.as_ref(),
        block.strategy.as_ref(),
        block.registry.as_ref(),
    ) else {
        return emit_entity_refusal(
            entity_missing_block_stage_args_refusal(block),
            true,
            summary_mode,
        );
    };
    let (Some(profile), Some(work_dir)) = (block.profile.as_ref(), block.work_dir.as_ref()) else {
        return emit_entity_refusal(
            entity_missing_v1_context_refusal(
                entity::EntityArtifactStageV1::Block,
                rows,
                &block.profile,
                strategy,
                registry,
                &block.work_dir,
                None,
            ),
            true,
            summary_mode,
        );
    };

    match entity::run::run_entity_block_stage(entity::block::EntityBlockStageRequest {
        rows,
        profile,
        strategy,
        registry,
        work_dir,
    }) {
        Ok(output) => {
            let rendered = match block.emit {
                EntityStreamEmitMode::Jsonl => render_jsonl_records(&output.candidates)?,
                EntityStreamEmitMode::Summary => render_entity_block_stage_summary(&output),
            };
            emit_entity_output(&rendered, summary_mode);
            Ok(0)
        }
        Err(refusal) => emit_entity_refusal(refusal.to_canon_output(), true, summary_mode),
    }
}

fn run_entity_block_preflight_command(
    preflight: &EntityBlockPreflightCli,
) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(preflight.emit, EntityEmitMode::Summary);
    match entity::block_preflight::run_block_preflight(
        entity::block_preflight::EntityBlockPreflightRequest {
            rows: &preflight.rows,
            profile: &preflight.profile,
            strategy: &preflight.strategy,
            sample_pct: preflight.sample_pct,
            work_dir: preflight.work_dir.as_deref(),
        },
    ) {
        Ok(report) => {
            let output = match preflight.emit {
                EntityEmitMode::Json => serde_json::to_string(&report)?,
                EntityEmitMode::Summary => {
                    entity::block_preflight::render_block_preflight_summary(&report)
                }
            };
            emit_entity_output(&output, summary_mode);
            Ok(0)
        }
        Err(refusal) => emit_entity_refusal(refusal.to_canon_output(), true, summary_mode),
    }
}

fn run_entity_candidate_recall_command(
    recall: &EntityCandidateRecallCli,
) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(recall.emit, EntityEmitMode::Summary);
    match run_entity_candidate_recall_pipeline(recall) {
        Ok(report) => {
            let output = match recall.emit {
                EntityEmitMode::Json => serde_json::to_string(&report)?,
                EntityEmitMode::Summary => entity_candidate_recall_summary(&report),
            };
            emit_entity_output(&output, summary_mode);
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(refusal_output, true, summary_mode),
    }
}

fn run_entity_alias_withholding_command(
    alias_withholding: &EntityAliasWithholdingCli,
) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(alias_withholding.emit, EntityEmitMode::Summary);
    match run_entity_alias_withholding_pipeline(alias_withholding) {
        Ok(report) => {
            let output = match alias_withholding.emit {
                EntityEmitMode::Json => {
                    serde_json::to_string(&alias_withholding_cli_report_json(&report)?)?
                }
                EntityEmitMode::Summary => render_alias_withholding_report_summary(&report),
            };
            emit_entity_output(&output, summary_mode);
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(refusal_output, true, summary_mode),
    }
}

fn run_entity_generalization_command(
    generalization: &EntityGeneralizationCli,
) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(generalization.emit, EntityEmitMode::Summary);
    match run_entity_generalization_pipeline(generalization) {
        Ok(report) => {
            let output = match generalization.emit {
                EntityEmitMode::Json => {
                    serde_json::to_string(&generalization_cli_report_json(&report)?)?
                }
                EntityEmitMode::Summary => render_generalization_report_summary(&report),
            };
            emit_entity_output(&output, summary_mode);
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(refusal_output, false, summary_mode),
    }
}

fn run_entity_calibrate_command(calibrate: &EntityCalibrateCommand) -> Result<u8, Box<dyn Error>> {
    match &calibrate.command {
        EntityCalibrateSubcommand::Sweep(sweep) => run_entity_calibrate_sweep_command(sweep),
    }
}

fn run_entity_calibrate_sweep_command(
    sweep: &EntityCalibrateSweepCli,
) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(sweep.emit, EntityEmitMode::Summary);
    match entity::calibrate::run_calibrate_sweep(entity::calibrate::CalibrateSweepRequest {
        result: &sweep.result,
        gold: &sweep.gold,
        strategy: &sweep.strategy,
    }) {
        Ok(report) => {
            let output = match sweep.emit {
                EntityEmitMode::Json => serde_json::to_string(&report)?,
                EntityEmitMode::Summary => {
                    entity::calibrate::render_calibrate_sweep_summary(&report)
                }
            };
            emit_entity_output(&output, summary_mode);
            Ok(0)
        }
        Err(refusal) => emit_entity_refusal(refusal.to_canon_output(), true, summary_mode),
    }
}

fn run_entity_evidence_command(evidence: &EntityEvidenceCli) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(evidence.emit, EntityStreamEmitMode::Summary);
    if let Err(refusal_output) = validate_entity_v1_input_artifact(
        &evidence.candidates,
        "candidate block artifact",
        entity::EntityArtifactStageV1::Block,
    ) {
        return emit_entity_refusal(refusal_output, true, summary_mode);
    }

    let (Some(profile), Some(work_dir)) = (evidence.profile.as_ref(), evidence.work_dir.as_ref())
    else {
        return emit_entity_refusal(
            entity_missing_v1_context_refusal(
                entity::EntityArtifactStageV1::Evidence,
                &evidence.rows,
                &evidence.profile,
                &evidence.strategy,
                &evidence.registry,
                &evidence.work_dir,
                None,
            ),
            true,
            summary_mode,
        );
    };

    match entity::run::run_entity_evidence_stage(entity::edge::EntityEvidenceStageRequest {
        rows: &evidence.rows,
        profile,
        strategy: &evidence.strategy,
        candidates: &evidence.candidates,
        registry: &evidence.registry,
        work_dir,
    }) {
        Ok(output) => {
            let rendered = match evidence.emit {
                EntityStreamEmitMode::Jsonl => render_jsonl_records(&output.records)?,
                EntityStreamEmitMode::Summary => render_entity_evidence_stage_summary(&output),
            };
            emit_entity_output(&rendered, summary_mode);
            Ok(0)
        }
        Err(refusal) => emit_entity_refusal(refusal.to_canon_output(), true, summary_mode),
    }
}

fn run_entity_solve_command(solve: &EntitySolveCli) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(solve.emit, EntityEmitMode::Summary);
    if let Err(refusal_output) = validate_entity_v1_input_artifact(
        &solve.evidence,
        "evidence artifact",
        entity::EntityArtifactStageV1::Evidence,
    ) {
        return emit_entity_refusal(refusal_output, true, summary_mode);
    }

    let (Some(profile), Some(work_dir)) = (solve.profile.as_ref(), solve.work_dir.as_ref()) else {
        return emit_entity_refusal(
            entity_missing_v1_context_refusal(
                entity::EntityArtifactStageV1::Solve,
                &solve.rows,
                &solve.profile,
                &solve.strategy,
                &solve.registry,
                &solve.work_dir,
                None,
            ),
            true,
            summary_mode,
        );
    };

    match entity::run::run_entity_solve_stage(entity::solve::EntitySolveStageRequest {
        rows: &solve.rows,
        profile,
        strategy: &solve.strategy,
        evidence: &solve.evidence,
        registry: &solve.registry,
        work_dir,
    }) {
        Ok(output) => {
            let rendered = match solve.emit {
                EntityEmitMode::Json => {
                    let artifact =
                        read_entity_stage_artifact_json(&work_dir.join("solve/solve.json"))?;
                    serde_json::to_string(&artifact)?
                }
                EntityEmitMode::Summary => render_entity_solve_stage_summary(&output),
            };
            emit_entity_output(&rendered, summary_mode);
            Ok(0)
        }
        Err(refusal) => emit_entity_refusal(refusal.to_canon_output(), true, summary_mode),
    }
}

fn run_entity_link_command(link: &EntityLinkCli) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(link.emit, EntityEmitMode::Summary);
    let (Some(profile), Some(work_dir)) = (link.profile.as_ref(), link.work_dir.as_ref()) else {
        return emit_entity_refusal(
            entity_missing_link_context_refusal(link),
            true,
            summary_mode,
        );
    };
    if let Err(refusal_output) = validate_entity_link_input_rows(link) {
        return emit_entity_refusal(refusal_output, true, summary_mode);
    }

    let strategy = match resolve::load_strategy(&link.strategy) {
        Ok(strategy) => strategy,
        Err(error) => {
            return emit_entity_refusal(create_resolve_refusal(error), true, summary_mode);
        }
    };
    let tapes = match resolve::load_tapes(
        &link.reference,
        &link.target,
        &strategy,
        resolve::TapeLoadOptions {
            max_rows: link.max_rows,
            max_bytes: link.max_bytes,
        },
    ) {
        Ok(tapes) => tapes,
        Err(error) => {
            return emit_entity_refusal(create_resolve_refusal(error), true, summary_mode);
        }
    };
    if let Err(refusal_output) =
        validate_entity_link_candidate_budget_preflight(link, &strategy, &tapes)
    {
        return emit_entity_refusal(refusal_output, true, summary_mode);
    }
    if link.write_back {
        return emit_entity_refusal(
            entity_link_v1_write_back_refusal(link, work_dir),
            true,
            summary_mode,
        );
    }
    if let Err(refusal_output) = validate_entity_link_profile_strategy_contract(profile, &strategy)
    {
        return emit_entity_refusal(refusal_output, true, summary_mode);
    }

    let audit_suite = match link.suite.as_ref() {
        Some(suite_dir) => match load_entity_audit_suite(suite_dir, "link") {
            Ok(suite) => Some(suite),
            Err(refusal_output) => {
                return emit_entity_refusal(refusal_output, true, summary_mode);
            }
        },
        None => None,
    };

    match entity::run::link::run_entity_link_with_cache_mode(
        entity::run::link::EntityLinkRequest {
            reference_rows: &link.reference,
            target_rows: &link.target,
            profile,
            strategy: &link.strategy,
            registry: &link.registry,
            work_dir,
        },
        entity_index_cache_mode(link.cache_mode),
    ) {
        Ok(result) => {
            let audit_receipt = if let Some(suite) = audit_suite.as_ref() {
                match run_entity_link_suite_audit(&result.run.artifact, suite, work_dir) {
                    Ok(receipt) => Some(receipt),
                    Err(refusal_output) => {
                        return emit_entity_refusal(refusal_output, true, summary_mode);
                    }
                }
            } else {
                None
            };

            let decisions = match entity_link_v1_decision_artifact(
                link, work_dir, &result, &strategy, &tapes,
            ) {
                Ok(artifact) => artifact,
                Err(refusal_output) => {
                    return emit_entity_refusal(refusal_output, true, summary_mode);
                }
            };
            let link_artifact = match entity::run::link::finalize_entity_link_artifact(
                entity::run::link::EntityLinkFinalizeRequest {
                    artifact: result.artifact,
                    run_artifact: &result.run.artifact,
                    decisions: &decisions,
                    work_dir,
                },
            ) {
                Ok(artifact) => artifact,
                Err(refusal) => {
                    return emit_entity_refusal(refusal.to_canon_output(), true, summary_mode);
                }
            };
            let output = match link.emit {
                EntityEmitMode::Json => serde_json::to_string(&entity_link_output_value(
                    &link_artifact,
                    audit_receipt.as_ref(),
                )?)?,
                EntityEmitMode::Summary => {
                    entity_link_summary(&link_artifact, audit_receipt.as_ref())
                }
            };
            let exit_code = decisions.exit_code();
            append_entity_link_witness(
                link,
                &decisions,
                audit_receipt.as_ref(),
                &output,
                exit_code,
            );
            emit_entity_output(&output, summary_mode);
            Ok(exit_code)
        }
        Err(refusal) => emit_entity_refusal(refusal.to_canon_output(), true, summary_mode),
    }
}

#[allow(clippy::result_large_err)]
fn validate_entity_link_input_rows(link: &EntityLinkCli) -> Result<(), CanonOutput> {
    validate_entity_link_input_path("reference", &link.reference)?;
    validate_entity_link_input_path("target", &link.target)
}

#[allow(clippy::result_large_err)]
fn validate_entity_link_input_path(role: &str, path: &Path) -> Result<(), CanonOutput> {
    fs::File::open(path).map(|_| ()).map_err(|error| {
        refusal::create_refusal(
            RefusalCode::EEntityInputContract,
            format!(
                "Cannot read entity link {role} rows '{}': {}",
                path.display(),
                error
            ),
            serde_json::json!({
                "stage": "link",
                "role": role,
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(format!(
                "Check the {role} row path and rerun canon entity link"
            )),
        )
    })
}

#[allow(clippy::result_large_err)]
fn validate_entity_link_candidate_budget_preflight(
    link: &EntityLinkCli,
    strategy: &resolve::ResolveStrategy,
    tapes: &resolve::LoadedTapes,
) -> Result<(), CanonOutput> {
    let Some(limit) = link.max_candidates.or(strategy.max_candidates) else {
        return Ok(());
    };
    if limit != 0 {
        return Ok(());
    }

    let mut counting_strategy = strategy.clone();
    counting_strategy.max_candidates = None;
    let target = resolve::select_candidates(tapes, &counting_strategy, None, None)
        .map_err(create_resolve_refusal)?
        .targets
        .into_iter()
        .next();
    let Some(target) = target else {
        return Ok(());
    };
    let target_id = target.target_id;
    let candidate_count = target.candidates.len();

    Err(create_resolve_refusal(resolve::ResolveError::with_detail(
        resolve::ResolveErrorCode::TooManyCandidates,
        format!(
            "Entity link max_candidates=0 is invalid for target '{}' with {} candidates after filtering",
            target_id, candidate_count
        ),
        serde_json::json!({
            "target_id": target_id,
            "candidate_count": candidate_count,
            "max_candidates": limit,
            "filter_count": strategy.candidate_filter.len(),
            "writes_performed": false
        }),
    )))
}

#[allow(clippy::result_large_err)]
fn validate_entity_link_profile_strategy_contract(
    profile_source: &str,
    strategy: &resolve::ResolveStrategy,
) -> Result<(), CanonOutput> {
    let loaded_profile = entity::prepare::load_prepare_profile_with_hash(profile_source)
        .map_err(|refusal| refusal.to_canon_output())?;
    if loaded_profile.document.entity_type == strategy.entity_type {
        return Ok(());
    }

    Err(refusal::create_refusal(
        RefusalCode::EEntityInputContract,
        "Entity link profile entity_type does not match strategy entity_type".to_string(),
        serde_json::json!({
            "stage": "link",
            "field": "profile.entity_type",
            "profile_source": profile_source,
            "expected": {
                "strategy_entity_type": strategy.entity_type.as_str(),
                "strategy_id": strategy.id.as_str(),
                "strategy_version": strategy.version.as_str(),
                "strategy_content_hash": strategy.content_hash.as_str()
            },
            "actual": {
                "profile_entity_type": loaded_profile.document.entity_type.as_str(),
                "profile_id": loaded_profile.document.profile.as_str(),
                "profile_version": loaded_profile.document.version.as_str(),
                "profile_content_hash": loaded_profile.content_hash.as_str()
            },
            "writes_performed": false
        }),
        Some(format!(
            "Use an entity profile with entity_type '{}' or a strategy matching profile '{}', then rerun canon entity link",
            strategy.entity_type.as_str(),
            profile_source
        )),
    ))
}

#[derive(Debug, Clone, Deserialize)]
struct EntityAuditSuiteManifest {
    #[serde(alias = "suite_id")]
    id: String,
    #[serde(default = "default_entity_audit_suite_version")]
    version: String,
    #[serde(default)]
    gates: Vec<entity::audit::EntityAuditGateCheck>,
}

fn default_entity_audit_suite_version() -> String {
    "v1".to_string()
}

#[allow(clippy::result_large_err)]
fn load_entity_audit_suite(
    suite_dir: &Path,
    stage: &str,
) -> Result<entity::audit::EntityAuditSuite, CanonOutput> {
    if !suite_dir.is_dir() {
        return Err(Refusal::entity_bad_suite(
            "Audit suite directory does not exist",
            serde_json::json!({
                "stage": stage,
                "field": "suite",
                "path": suite_dir.display().to_string(),
                "writes_performed": false
            }),
        )
        .to_canon_output());
    }

    let manifest_path = suite_dir.join("manifest.json");
    let bytes = std::fs::read(&manifest_path).map_err(|error| {
        Refusal::entity_bad_suite(
            "Failed to read audit suite manifest",
            serde_json::json!({
                "stage": stage,
                "field": "suite",
                "path": manifest_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
        .to_canon_output()
    })?;
    let manifest = serde_json::from_slice::<EntityAuditSuiteManifest>(&bytes).map_err(|error| {
        Refusal::entity_bad_suite(
            "Audit suite manifest is malformed",
            serde_json::json!({
                "stage": stage,
                "field": "suite",
                "path": manifest_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
        .to_canon_output()
    })?;

    preflight_entity_audit_gates(&manifest, stage)?;

    Ok(entity::audit::EntityAuditSuite {
        id: manifest.id,
        version: manifest.version,
        gates: manifest.gates,
    })
}

#[allow(clippy::result_large_err)]
fn preflight_entity_audit_gates(
    manifest: &EntityAuditSuiteManifest,
    stage: &str,
) -> Result<(), CanonOutput> {
    if manifest.id.trim().is_empty() || manifest.version.trim().is_empty() {
        return Err(Refusal::entity_bad_suite(
            "Audit suite id and version must be non-empty",
            serde_json::json!({
                "stage": stage,
                "field": "suite",
                "suite_id": manifest.id.as_str(),
                "suite_version": manifest.version.as_str(),
                "writes_performed": false
            }),
        )
        .to_canon_output());
    }
    if manifest.gates.is_empty() {
        return Err(Refusal::entity_bad_suite(
            "Audit suite must contain at least one gate",
            serde_json::json!({
                "stage": stage,
                "field": "gates",
                "suite_id": manifest.id.as_str(),
                "writes_performed": false
            }),
        )
        .to_canon_output());
    }

    let known_gates = entity::ENTITY_GATE_IDS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for gate in &manifest.gates {
        if gate.gate_id.trim().is_empty() || gate.label.trim().is_empty() {
            return Err(Refusal::entity_bad_suite(
                "Audit gate IDs and labels must be non-empty",
                serde_json::json!({
                    "stage": stage,
                    "field": "gate_id",
                    "suite_id": manifest.id.as_str(),
                    "gate_id": gate.gate_id.as_str(),
                    "writes_performed": false
                }),
            )
            .to_canon_output());
        }
        if !known_gates.contains(gate.gate_id.as_str()) {
            return Err(Refusal::entity_bad_suite(
                "Audit suite references an unknown gate",
                serde_json::json!({
                    "stage": stage,
                    "field": "gate_id",
                    "suite_id": manifest.id.as_str(),
                    "gate_id": gate.gate_id.as_str(),
                    "writes_performed": false
                }),
            )
            .to_canon_output());
        }
        if !seen.insert(gate.gate_id.as_str()) {
            return Err(Refusal::entity_bad_suite(
                "Audit suite contains duplicate gates",
                serde_json::json!({
                    "stage": stage,
                    "field": "gate_id",
                    "suite_id": manifest.id.as_str(),
                    "gate_id": gate.gate_id.as_str(),
                    "writes_performed": false
                }),
            )
            .to_canon_output());
        }
        if !gate.passed {
            return Err(Refusal::entity_bad_suite(
                "Audit suite gate did not pass",
                serde_json::json!({
                    "stage": stage,
                    "field": "gate_id",
                    "suite_id": manifest.id.as_str(),
                    "gate_id": gate.gate_id.as_str(),
                    "expected": gate.expected.as_str(),
                    "actual": gate.actual.as_str(),
                    "writes_performed": false
                }),
            )
            .to_canon_output());
        }
    }

    Ok(())
}

#[allow(clippy::result_large_err)]
fn run_entity_link_suite_audit(
    run_artifact: &entity::run::EntityRunArtifact,
    suite: &entity::audit::EntityAuditSuite,
    work_dir: &Path,
) -> Result<serde_json::Value, CanonOutput> {
    let result = entity::EntityArtifactHeader {
        version: run_artifact.version.clone(),
        metadata: run_artifact.metadata.clone(),
        summary: run_artifact.summary.clone(),
    };
    let mut certified_artifacts = run_artifact
        .stage_artifacts
        .iter()
        .map(|artifact| entity::EntityArtifactReference {
            version: artifact.version.clone(),
            content_hash: artifact.artifact_content_hash.clone(),
        })
        .collect::<Vec<_>>();
    certified_artifacts.push(entity::EntityArtifactReference {
        version: run_artifact.version.clone(),
        content_hash: run_artifact.artifact_content_hash.clone(),
    });
    let expected = entity::artifact_chain::EntityArtifactChainExpectation::from_link(
        entity::artifact_chain::EntityChainStage::Audit,
        &entity::artifact_chain::EntityArtifactChainLink::from_header(&result),
    );
    let audit = entity::audit::run_entity_audit(entity::audit::EntityAuditRequest {
        result,
        expected,
        certified_artifacts,
        suite: suite.clone(),
    })
    .map_err(|refusal| refusal.to_canon_output())?;

    let audit_path = work_dir.join("audit.json");
    let bytes = serde_json::to_vec_pretty(&audit).map_err(|error| {
        refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            "Failed to serialize entity link audit artifact".to_string(),
            serde_json::json!({
                "stage": "link",
                "path": audit_path.display().to_string(),
                "error": error.to_string(),
                "registry_write_back_performed": false
            }),
            None,
        )
    })?;
    std::fs::write(&audit_path, bytes).map_err(|error| {
        refusal::create_refusal(
            RefusalCode::EIo,
            format!(
                "Failed to write entity link audit artifact '{}': {}",
                audit_path.display(),
                error
            ),
            serde_json::json!({
                "stage": "link",
                "path": audit_path.display().to_string(),
                "error": error.to_string(),
                "registry_write_back_performed": false
            }),
            None,
        )
    })?;

    Ok(entity_link_audit_receipt(&audit_path, &audit))
}

fn entity_link_audit_receipt(
    audit_path: &Path,
    audit: &entity::audit::EntityAuditArtifact,
) -> serde_json::Value {
    serde_json::json!({
        "path": audit_path.display().to_string(),
        "version": audit.version.as_str(),
        "artifact_content_hash": audit.artifact_content_hash.as_str(),
        "suite": {
            "id": audit.suite_id.as_str(),
            "version": audit.suite_version.as_str()
        },
        "audited_artifact": &audit.audited_artifact,
        "gate_count": audit.gates.len(),
        "status": audit.summary.labels.get("status").cloned().unwrap_or_else(|| "passed".to_string())
    })
}

fn entity_link_v1_decision_artifact(
    link: &EntityLinkCli,
    work_dir: &Path,
    result: &entity::run::link::EntityLinkResult,
    strategy: &resolve::ResolveStrategy,
    tapes: &resolve::LoadedTapes,
) -> Result<resolve::ResolveArtifact, CanonOutput> {
    validate_entity_link_v1_run_artifact(result)?;
    let solve = load_entity_link_v1_solve_artifact(work_dir, &result.run.artifact)?;
    let bindings =
        derive_entity_link_decision_bindings(link, work_dir, &result.run.artifact, strategy)?;
    validate_entity_link_run_strategy_continuity(&result.run.artifact, strategy, "link")?;
    let decision_records = entity_link_decision_records_from_solve(&solve, &bindings, strategy)?;
    enforce_entity_link_candidate_limit(link, strategy, &decision_records)?;
    let gold_score = link
        .gold
        .as_deref()
        .map(|gold| resolve::score_gold_file(gold, &decision_records))
        .transpose()
        .map_err(create_resolve_refusal)?;
    entity_link_resolve_artifact_from_v1(
        link,
        &result.run.artifact,
        strategy,
        tapes,
        decision_records,
        gold_score,
    )
}

fn validate_entity_link_v1_run_artifact(
    result: &entity::run::link::EntityLinkResult,
) -> Result<(), CanonOutput> {
    entity::schema::validate_artifact_v1_core_contract(&result.run.artifact_value)
        .map_err(|refusal| refusal.to_canon_output())?;
    entity::schema::validate_entity_v1_self_hash(&result.run.artifact_value)
        .map_err(|refusal| refusal.to_canon_output())?;
    let actual_hash = entity_link_value_string(
        &result.run.artifact_value,
        &["artifact_content_hash"],
        "run.artifact_content_hash",
    )?;
    if actual_hash.as_str() != result.run.artifact.artifact_content_hash.as_str() {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link run artifact hash changed before decision derivation",
            serde_json::json!({
                "stage": "link",
                "field": "run.artifact_content_hash",
                "expected": result.run.artifact.artifact_content_hash.as_str(),
                "actual": actual_hash.as_str(),
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn validate_entity_link_run_strategy_continuity(
    run: &entity::run::EntityRunArtifact,
    strategy: &resolve::ResolveStrategy,
    stage: &str,
) -> Result<(), CanonOutput> {
    if run.metadata.strategy.id.as_str() != strategy.id.as_str()
        || run.metadata.strategy.version.as_str() != strategy.version.as_str()
        || run.metadata.strategy.content_hash.as_str() != strategy.content_hash.as_str()
    {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link strategy source does not match the v1 run metadata",
            serde_json::json!({
                "stage": stage,
                "field": "metadata.strategy",
                "expected": {
                    "id": run.metadata.strategy.id.as_str(),
                    "version": run.metadata.strategy.version.as_str(),
                    "content_hash": run.metadata.strategy.content_hash.as_str()
                },
                "actual": {
                    "id": strategy.id.as_str(),
                    "version": strategy.version.as_str(),
                    "content_hash": strategy.content_hash.as_str()
                },
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn load_entity_link_v1_solve_artifact(
    work_dir: &Path,
    run: &entity::run::EntityRunArtifact,
) -> Result<entity::solve::SolveArtifact, CanonOutput> {
    let solve_path = work_dir.join(&run.work_dir.solve_artifact_path);
    let (solve_value, _bytes) =
        read_entity_lifecycle_json_artifact(&solve_path, "entity solve artifact")?;
    entity::schema::validate_artifact_v1_core_contract(&solve_value)
        .map_err(|refusal| refusal.to_canon_output())?;
    entity::schema::validate_entity_v1_self_hash(&solve_value)
        .map_err(|refusal| refusal.to_canon_output())?;
    let solve_hash = entity_link_value_string(
        &solve_value,
        &["artifact_content_hash"],
        "solve.artifact_hash",
    )?;
    let solve_stage = run
        .stage_artifacts
        .iter()
        .find(|stage| {
            stage.stage == entity::EntityArtifactStageV1::Solve.as_str()
                && stage.version == entity::CANON_ENTITY_SOLVE_VERSION_V1
        })
        .ok_or_else(|| {
            entity_link_v1_artifact_refusal(
                "Entity link run artifact does not bind a solve.v1 stage",
                serde_json::json!({
                    "stage": "link",
                    "field": "run.stage_artifacts",
                    "expected_version": entity::CANON_ENTITY_SOLVE_VERSION_V1,
                    "writes_performed": false
                }),
            )
        })?;
    if solve_stage.artifact_content_hash.as_str() != solve_hash.as_str() {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link solve artifact hash does not match the run stage reference",
            serde_json::json!({
                "stage": "link",
                "field": "run.stage_artifacts.solve.artifact_content_hash",
                "expected": solve_stage.artifact_content_hash.as_str(),
                "actual": solve_hash.as_str(),
                "path": solve_path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }
    serde_json::from_value::<entity::solve::SolveArtifact>(solve_value).map_err(|error| {
        entity_link_v1_artifact_refusal(
            "Entity link solve artifact failed typed v1 deserialization",
            serde_json::json!({
                "stage": "link",
                "path": solve_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntityLinkDecisionBinding {
    side: entity::run::link::EntityLinkRole,
    link_id: String,
    surface_id: String,
}

fn derive_entity_link_decision_bindings(
    link: &EntityLinkCli,
    work_dir: &Path,
    run: &entity::run::EntityRunArtifact,
    strategy: &resolve::ResolveStrategy,
) -> Result<Vec<EntityLinkDecisionBinding>, CanonOutput> {
    let materialized_path = entity::run::link::materialized_rows_path(work_dir);
    let materialized_rows =
        read_entity_link_materialized_decision_rows(&materialized_path, strategy)?;
    let profile = link.profile.as_deref().ok_or_else(|| {
        entity_link_v1_artifact_refusal(
            "Entity link decision derivation requires the CLI profile source",
            serde_json::json!({
                "stage": "link",
                "field": "profile",
                "writes_performed": false
            }),
        )
    })?;
    let contract = entity_link_prepare_contract_for_cli_profile(profile, run)?;
    let observations = entity::prepare::project_prepare_path(&materialized_path, &contract)
        .map_err(|refusal| refusal.to_canon_output())?;
    let surfaces = read_entity_link_prepared_surfaces(work_dir, run)?;
    if materialized_rows.len() != observations.len() {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link materialized rows do not match prepared observations",
            serde_json::json!({
                "stage": "link",
                "field": "materialized_rows",
                "materialized_rows": materialized_rows.len(),
                "prepared_observations": observations.len(),
                "writes_performed": false
            }),
        ));
    }
    let mut bindings = Vec::with_capacity(materialized_rows.len());
    for (row, observation) in materialized_rows.iter().zip(observations.iter()) {
        let surface = prepared_surface_for_observation(observation, &surfaces)?;
        bindings.push(EntityLinkDecisionBinding {
            side: row.side,
            link_id: row.link_id.clone(),
            surface_id: surface.surface_id.clone(),
        });
    }
    bindings.sort_by(|left, right| {
        left.surface_id
            .cmp(&right.surface_id)
            .then_with(|| {
                entity_link_role_order(left.side).cmp(&entity_link_role_order(right.side))
            })
            .then_with(|| left.link_id.cmp(&right.link_id))
    });
    Ok(bindings)
}

fn entity_link_prepare_contract_for_cli_profile(
    profile: &str,
    run: &entity::run::EntityRunArtifact,
) -> Result<entity::prepare::PrepareInputContract, CanonOutput> {
    let loaded_profile = entity::prepare::load_prepare_profile_with_hash(profile)
        .map_err(|refusal| refusal.to_canon_output())?;
    let mut contract = if let Some(mapping) = loaded_profile.prepare_mapping.clone() {
        entity::prepare::PrepareInputContract::new(&loaded_profile.document, mapping)
            .map_err(|refusal| refusal.to_canon_output())?
    } else {
        entity::prepare::PrepareInputContract::for_builtin_profile(&loaded_profile.document)
            .map_err(|refusal| refusal.to_canon_output())?
    };
    contract.profile.content_hash = Some(loaded_profile.content_hash.clone());
    validate_entity_link_profile_continuity(&contract.profile, run)?;
    Ok(contract)
}

fn validate_entity_link_profile_continuity(
    profile: &entity::EntityProfileReference,
    run: &entity::run::EntityRunArtifact,
) -> Result<(), CanonOutput> {
    let run_profile = &run.metadata.profile;
    let run_profile_hash = run_profile.content_hash.as_deref().unwrap_or_default();
    let profile_hash = profile.content_hash.as_deref().unwrap_or_default();
    if profile.id.as_str() != run_profile.id.as_str()
        || profile.version.as_str() != run_profile.version.as_str()
        || profile_hash != run_profile_hash
    {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link CLI profile source does not match the v1 run profile",
            serde_json::json!({
                "stage": "link",
                "field": "profile",
                "expected": {
                    "id": run_profile.id.as_str(),
                    "version": run_profile.version.as_str(),
                    "content_hash": run_profile.content_hash.as_deref()
                },
                "actual": {
                    "id": profile.id.as_str(),
                    "version": profile.version.as_str(),
                    "content_hash": profile.content_hash.as_deref()
                },
                "writes_performed": false
            }),
        ));
    }
    let firewall = &run.orchestration.profile_firewall;
    if firewall.profile_id.as_str() != run_profile.id.as_str()
        || firewall.profile_version.as_str() != run_profile.version.as_str()
    {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link run profile firewall does not match metadata profile",
            serde_json::json!({
                "stage": "link",
                "field": "run.orchestration.profile_firewall",
                "metadata_profile": {
                    "id": run_profile.id.as_str(),
                    "version": run_profile.version.as_str()
                },
                "firewall": {
                    "id": firewall.profile_id.as_str(),
                    "version": firewall.profile_version.as_str()
                },
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn read_entity_link_prepared_surfaces(
    work_dir: &Path,
    run: &entity::run::EntityRunArtifact,
) -> Result<Vec<entity::prepare::PreparedSurfaceRecord>, CanonOutput> {
    let surfaces_path = work_dir.join(&run.work_dir.surfaces_path);
    let bytes = fs::read_to_string(&surfaces_path).map_err(|error| {
        refusal::create_refusal(
            RefusalCode::EIo,
            format!(
                "Failed to read entity link prepared surfaces '{}': {}",
                surfaces_path.display(),
                error
            ),
            serde_json::json!({
                "stage": "link",
                "path": surfaces_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Rerun canon entity link to regenerate the v1 prepare surfaces".to_string()),
        )
    })?;
    let mut surfaces = Vec::new();
    for (line_index, line) in bytes.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let surface = serde_json::from_str::<entity::prepare::PreparedSurfaceRecord>(line)
            .map_err(|error| {
                entity_link_v1_artifact_refusal(
                    "Failed to parse entity link prepared surface row",
                    serde_json::json!({
                        "stage": "link",
                        "path": surfaces_path.display().to_string(),
                        "line": line_index + 1,
                        "error": error.to_string(),
                        "writes_performed": false
                    }),
                )
            })?;
        surfaces.push(surface);
    }
    if surfaces.is_empty() {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link prepared surfaces must not be empty",
            serde_json::json!({
                "stage": "link",
                "path": surfaces_path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }
    let profile_id = run.metadata.profile.id.as_str();
    if surfaces
        .iter()
        .any(|surface| surface.profile_id.as_str() != profile_id)
    {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link prepared surfaces contain a profile outside the v1 run profile",
            serde_json::json!({
                "stage": "link",
                "path": surfaces_path.display().to_string(),
                "profile_id": profile_id,
                "writes_performed": false
            }),
        ));
    }
    Ok(surfaces)
}

fn entity_link_role_order(role: entity::run::link::EntityLinkRole) -> u8 {
    match role {
        entity::run::link::EntityLinkRole::Reference => 0,
        entity::run::link::EntityLinkRole::Target => 1,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntityLinkMaterializedDecisionRow {
    side: entity::run::link::EntityLinkRole,
    link_id: String,
}

fn read_entity_link_materialized_decision_rows(
    path: &Path,
    strategy: &resolve::ResolveStrategy,
) -> Result<Vec<EntityLinkMaterializedDecisionRow>, CanonOutput> {
    let file = fs::File::open(path).map_err(|error| {
        refusal::create_refusal(
            RefusalCode::EIo,
            format!(
                "Failed to open entity link materialized rows '{}': {}",
                path.display(),
                error
            ),
            serde_json::json!({
                "stage": "link",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Rerun canon entity link with matching --work-dir inputs".to_string()),
        )
    })?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file);
    let headers = reader
        .headers()
        .map_err(|error| {
            entity_link_v1_artifact_refusal(
                "Failed to read entity link materialized row headers",
                serde_json::json!({
                    "stage": "link",
                    "path": path.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(|error| {
            entity_link_v1_artifact_refusal(
                "Failed to parse entity link materialized row",
                serde_json::json!({
                    "stage": "link",
                    "path": path.display().to_string(),
                    "row_number": index + 1,
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        let values = headers
            .iter()
            .enumerate()
            .map(|(field_index, header)| {
                (
                    header.clone(),
                    record.get(field_index).unwrap_or_default().to_string(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let row_number = index + 1;
        let side = entity_link_materialized_side(&values, row_number)?;
        let id_columns = match side {
            entity::run::link::EntityLinkRole::Reference => &strategy.identity.reference.id_columns,
            entity::run::link::EntityLinkRole::Target => &strategy.identity.target.id_columns,
        };
        rows.push(EntityLinkMaterializedDecisionRow {
            side,
            link_id: entity_link_materialized_id(&values, id_columns, row_number)?,
        });
    }
    Ok(rows)
}

fn entity_link_materialized_side(
    values: &BTreeMap<String, String>,
    row_number: usize,
) -> Result<entity::run::link::EntityLinkRole, CanonOutput> {
    match entity_link_required_materialized_value(
        values,
        entity::run::link::LINK_SOURCE_NAME_COLUMN,
        row_number,
    )?
    .as_str()
    {
        "reference" => Ok(entity::run::link::EntityLinkRole::Reference),
        "target" => Ok(entity::run::link::EntityLinkRole::Target),
        actual => Err(entity_link_v1_artifact_refusal(
            "Entity link materialized row has an invalid source side",
            serde_json::json!({
                "stage": "link",
                "row_number": row_number,
                "field": entity::run::link::LINK_SOURCE_NAME_COLUMN,
                "actual": actual,
                "writes_performed": false
            }),
        )),
    }
}

fn entity_link_materialized_id(
    values: &BTreeMap<String, String>,
    id_columns: &[String],
    row_number: usize,
) -> Result<String, CanonOutput> {
    const LINK_COMPOSITE_ID_SEPARATOR: &str = "|";
    let mut parts = Vec::with_capacity(id_columns.len());
    for column in id_columns {
        let value = entity_link_required_materialized_value(values, column, row_number)?;
        if value.contains(LINK_COMPOSITE_ID_SEPARATOR) {
            return Err(entity_link_v1_artifact_refusal(
                "Entity link identity value contains the reserved composite separator",
                serde_json::json!({
                    "stage": "link",
                    "row_number": row_number,
                    "field": column,
                    "separator": LINK_COMPOSITE_ID_SEPARATOR,
                    "writes_performed": false
                }),
            ));
        }
        parts.push(value);
    }
    Ok(parts.join(LINK_COMPOSITE_ID_SEPARATOR))
}

fn entity_link_required_materialized_value(
    values: &BTreeMap<String, String>,
    field: &str,
    row_number: usize,
) -> Result<String, CanonOutput> {
    let value = values.get(field).ok_or_else(|| {
        entity_link_v1_artifact_refusal(
            "Entity link materialized row is missing a required field",
            serde_json::json!({
                "stage": "link",
                "row_number": row_number,
                "field": field,
                "writes_performed": false
            }),
        )
    })?;
    let value = value.trim();
    if value.is_empty() {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link materialized row has an empty required field",
            serde_json::json!({
                "stage": "link",
                "row_number": row_number,
                "field": field,
                "writes_performed": false
            }),
        ));
    }
    Ok(value.to_string())
}

fn prepared_surface_for_observation<'a>(
    observation: &entity::prepare::PreparedInputObservation,
    surfaces: &'a [entity::prepare::PreparedSurfaceRecord],
) -> Result<&'a entity::prepare::PreparedSurfaceRecord, CanonOutput> {
    let matches = surfaces
        .iter()
        .filter(|surface| {
            surface.profile_id.as_str() == observation.profile_id.as_str()
                && surface
                    .raw_variants
                    .iter()
                    .any(|variant| variant == &observation.primary_surface.value)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [surface] => Ok(*surface),
        [] => Err(entity_link_v1_artifact_refusal(
            "Entity link decision derivation could not find a prepared surface for a row",
            serde_json::json!({
                "stage": "link",
                "field": "surface_id",
                "row_number": observation.row_number,
                "writes_performed": false
            }),
        )),
        _ => Err(entity_link_v1_artifact_refusal(
            "Entity link decision derivation found multiple prepared surfaces for a row",
            serde_json::json!({
                "stage": "link",
                "field": "surface_id",
                "row_number": observation.row_number,
                "writes_performed": false
            }),
        )),
    }
}

fn entity_link_decision_records_from_solve(
    solve: &entity::solve::SolveArtifact,
    bindings: &[EntityLinkDecisionBinding],
    strategy: &resolve::ResolveStrategy,
) -> Result<resolve::MatchDecisions, CanonOutput> {
    let threshold_units = entity::score::ScoreUnits::from_f64_ratio(strategy.match_threshold);
    let mut by_surface = BTreeMap::<String, Vec<&EntityLinkDecisionBinding>>::new();
    let mut all_targets = BTreeSet::<String>::new();
    for binding in bindings {
        by_surface
            .entry(binding.surface_id.clone())
            .or_default()
            .push(binding);
        if binding.side == entity::run::link::EntityLinkRole::Target
            && !all_targets.insert(binding.link_id.clone())
        {
            return Err(entity_link_v1_artifact_refusal(
                "Entity link decision derivation found a duplicate target link id",
                serde_json::json!({
                    "stage": "link",
                    "field": "target_id",
                    "target_id": binding.link_id.as_str(),
                    "writes_performed": false
                }),
            ));
        }
    }

    let mut matches = Vec::new();
    let mut unmatched = Vec::new();
    let mut ambiguous = Vec::new();
    let mut classified_targets = BTreeSet::<String>::new();

    for entity in &solve.entities {
        let mut reference_ids = BTreeSet::<String>::new();
        let mut target_ids = BTreeSet::<String>::new();
        let mut reference_surfaces = BTreeMap::<String, BTreeSet<String>>::new();
        let mut target_surfaces = BTreeMap::<String, BTreeSet<String>>::new();
        for surface_id in &entity.surface_ids {
            if let Some(surface_bindings) = by_surface.get(surface_id) {
                for binding in surface_bindings {
                    match binding.side {
                        entity::run::link::EntityLinkRole::Reference => {
                            reference_ids.insert(binding.link_id.clone());
                            reference_surfaces
                                .entry(binding.link_id.clone())
                                .or_default()
                                .insert(surface_id.clone());
                        }
                        entity::run::link::EntityLinkRole::Target => {
                            target_ids.insert(binding.link_id.clone());
                            target_surfaces
                                .entry(binding.link_id.clone())
                                .or_default()
                                .insert(surface_id.clone());
                        }
                    }
                }
            }
        }
        if target_ids.is_empty() {
            continue;
        }
        let score = entity_link_score(entity.adjusted_support_score_units);
        let hard_blocked = entity.hard_cannot_link_count > 0
            || matches!(
                entity.state,
                entity::solve::SolveReconciliationState::Contradiction
                    | entity::solve::SolveReconciliationState::Conflict
            );
        for target_id in target_ids {
            if !classified_targets.insert(target_id.clone()) {
                return Err(entity_link_v1_artifact_refusal(
                    "Entity link decision derivation classified a target more than once",
                    serde_json::json!({
                        "stage": "link",
                        "field": "target_id",
                        "target_id": target_id.as_str(),
                        "component_id": entity.component_id.as_str(),
                        "writes_performed": false
                    }),
                ));
            }
            if hard_blocked {
                unmatched.push(resolve::UnmatchedRecord {
                    target_id,
                    reason: "solve_hard_cannot_link_or_conflict".to_string(),
                    best_candidate: entity_link_best_candidate(&reference_ids, score),
                });
            } else if entity_link_directional_match_allowed(entity, &reference_ids, threshold_units)
            {
                let reference_id = reference_ids
                    .iter()
                    .next()
                    .expect("one reference id")
                    .clone();
                matches.push(resolve::MatchRecord {
                    reference_id: reference_id.clone(),
                    target_id,
                    canonical_id: reference_id,
                    score,
                    assertions: Vec::new(),
                    runner_up: None,
                });
            } else if let Some(reference_id) = entity_link_prepared_surface_collapse_reference(
                entity,
                &reference_ids,
                &reference_surfaces,
                target_surfaces.get(&target_id),
            ) {
                matches.push(resolve::MatchRecord {
                    reference_id: reference_id.clone(),
                    target_id,
                    canonical_id: reference_id,
                    score: 0.0,
                    assertions: vec![entity_link_prepared_surface_collapse_assertion()],
                    runner_up: None,
                });
            } else if reference_ids.len() > 1 {
                ambiguous.push(resolve::AmbiguousRecord {
                    target_id,
                    candidates: entity_link_candidate_scores(&reference_ids, score),
                    gap: 0.0,
                    reason: "multiple_reference_surfaces_in_solve_component".to_string(),
                });
            } else {
                unmatched.push(resolve::UnmatchedRecord {
                    target_id,
                    reason: "no_resolved_reference_surface_in_solve_component".to_string(),
                    best_candidate: entity_link_best_candidate(&reference_ids, score),
                });
            }
        }
    }

    for target_id in all_targets.difference(&classified_targets) {
        unmatched.push(resolve::UnmatchedRecord {
            target_id: target_id.clone(),
            reason: "missing_solve_component".to_string(),
            best_candidate: None,
        });
    }

    matches.sort_by(|left, right| {
        left.target_id
            .cmp(&right.target_id)
            .then_with(|| left.reference_id.cmp(&right.reference_id))
    });
    unmatched.sort_by(|left, right| left.target_id.cmp(&right.target_id));
    ambiguous.sort_by(|left, right| left.target_id.cmp(&right.target_id));
    let conflict_warnings = entity_link_conflict_warnings(&matches);
    Ok(resolve::MatchDecisions {
        matches,
        unmatched,
        ambiguous,
        conflict_warnings,
    })
}

fn entity_link_directional_match_allowed(
    entity: &entity::solve::SolveEntityRecord,
    reference_ids: &BTreeSet<String>,
    threshold_units: entity::score::ScoreUnits,
) -> bool {
    reference_ids.len() == 1
        && entity.hard_cannot_link_count == 0
        && !matches!(
            entity.state,
            entity::solve::SolveReconciliationState::Contradiction
                | entity::solve::SolveReconciliationState::Conflict
        )
        && entity.soft_anti_merge_warning_count == 0
        && entity.adjusted_support_score_units > entity::score::ScoreUnits::ZERO
        && entity.adjusted_support_score_units >= threshold_units
}

fn entity_link_prepared_surface_collapse_reference(
    entity: &entity::solve::SolveEntityRecord,
    reference_ids: &BTreeSet<String>,
    reference_surfaces: &BTreeMap<String, BTreeSet<String>>,
    target_surfaces: Option<&BTreeSet<String>>,
) -> Option<String> {
    if reference_ids.len() != 1
        || entity.state != entity::solve::SolveReconciliationState::ResolvedExisting
        || entity.hard_cannot_link_count != 0
        || entity.soft_anti_merge_warning_count != 0
        || entity.adjusted_support_score_units != entity::score::ScoreUnits::ZERO
    {
        return None;
    }
    let reference_id = reference_ids.iter().next()?;
    let reference_surface_ids = reference_surfaces.get(reference_id)?;
    let target_surface_ids = target_surfaces?;
    if target_surface_ids
        .iter()
        .any(|surface_id| reference_surface_ids.contains(surface_id))
    {
        Some(reference_id.clone())
    } else {
        None
    }
}

fn entity_link_prepared_surface_collapse_assertion() -> resolve::AssertionResult {
    let mut detail = BTreeMap::new();
    detail.insert(
        "candidate_credit".to_string(),
        serde_json::Value::Bool(false),
    );
    detail.insert(
        "surface_equality".to_string(),
        serde_json::Value::String("exact_prepared_surface".to_string()),
    );
    resolve::AssertionResult {
        field_ref: "prepared_surface_id".to_string(),
        field_tgt: "prepared_surface_id".to_string(),
        op: "prepared_surface_collapse".to_string(),
        passed: true,
        score: 0.0,
        weight: 0.0,
        required: true,
        detail,
    }
}

fn entity_link_score(score_units: entity::score::ScoreUnits) -> f64 {
    f64::from(score_units.as_u32()) / f64::from(entity::score::ENTITY_SCORE_SCALE)
}

fn entity_link_best_candidate(
    reference_ids: &BTreeSet<String>,
    score: f64,
) -> Option<resolve::CandidateScore> {
    reference_ids
        .iter()
        .next()
        .map(|reference_id| resolve::CandidateScore {
            reference_id: reference_id.clone(),
            score,
            gap: None,
            assertions: Vec::new(),
        })
}

fn entity_link_candidate_scores(
    reference_ids: &BTreeSet<String>,
    score: f64,
) -> Vec<resolve::CandidateScore> {
    reference_ids
        .iter()
        .map(|reference_id| resolve::CandidateScore {
            reference_id: reference_id.clone(),
            score,
            gap: Some(0.0),
            assertions: Vec::new(),
        })
        .collect()
}

fn entity_link_conflict_warnings(matches: &[resolve::MatchRecord]) -> Vec<String> {
    let mut by_reference = BTreeMap::<String, Vec<String>>::new();
    for record in matches {
        by_reference
            .entry(record.reference_id.clone())
            .or_default()
            .push(record.target_id.clone());
    }
    by_reference
        .into_iter()
        .filter_map(|(reference_id, target_ids)| {
            (target_ids.len() > 1).then(|| {
                format!(
                    "one_to_many_conflict: reference_id '{}' matched target_ids [{}]",
                    reference_id,
                    target_ids.join(", ")
                )
            })
        })
        .collect()
}

fn enforce_entity_link_candidate_limit(
    link: &EntityLinkCli,
    strategy: &resolve::ResolveStrategy,
    decisions: &resolve::MatchDecisions,
) -> Result<(), CanonOutput> {
    let Some(limit) = link.max_candidates.or(strategy.max_candidates) else {
        return Ok(());
    };
    for record in &decisions.ambiguous {
        if record.candidates.len() > limit {
            return Err(create_resolve_refusal(resolve::ResolveError::with_detail(
                resolve::ResolveErrorCode::TooManyCandidates,
                format!(
                    "Target '{}' has {} derived link candidates, above limit {}",
                    record.target_id,
                    record.candidates.len(),
                    limit
                ),
                serde_json::json!({
                    "target_id": record.target_id.as_str(),
                    "candidate_count": record.candidates.len(),
                    "max_candidates": limit,
                    "writes_performed": false
                }),
            )));
        }
    }
    Ok(())
}

fn entity_link_resolve_artifact_from_v1(
    link: &EntityLinkCli,
    run: &entity::run::EntityRunArtifact,
    strategy: &resolve::ResolveStrategy,
    tapes: &resolve::LoadedTapes,
    decisions: resolve::MatchDecisions,
    gold_score: Option<resolve::GoldScore>,
) -> Result<resolve::ResolveArtifact, CanonOutput> {
    let registry = entity_link_registry_snapshot_from_v1_run(link, run)?;
    let summary = resolve::build_summary(
        tapes.target.records.len(),
        &decisions.matches,
        &decisions.unmatched,
        &decisions.ambiguous,
    );
    Ok(resolve::ResolveArtifact {
        version: resolve::CANON_RESOLVE_VERSION.to_string(),
        strategy: strategy.reference(),
        registry,
        reference_tape: tapes.reference.summary(),
        target_tape: tapes.target.summary(),
        summary,
        matches: decisions.matches,
        unmatched: decisions.unmatched,
        ambiguous: decisions.ambiguous,
        conflict_warnings: decisions.conflict_warnings,
        gold_score,
        write_back: None,
    })
}

fn entity_link_registry_snapshot_from_v1_run(
    link: &EntityLinkCli,
    run: &entity::run::EntityRunArtifact,
) -> Result<resolve::ResolveRegistrySnapshot, CanonOutput> {
    let snapshot = &run.metadata.registry_snapshot;
    if snapshot.id.trim().is_empty()
        || snapshot.version.trim().is_empty()
        || snapshot.source.trim().is_empty()
        || snapshot.lookup_snapshot_hash.trim().is_empty()
    {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link v1 run registry snapshot is incomplete",
            serde_json::json!({
                "stage": "link",
                "field": "run.metadata.registry_snapshot",
                "registry_id": snapshot.id.as_str(),
                "registry_version": snapshot.version.as_str(),
                "registry_source": snapshot.source.as_str(),
                "lookup_snapshot_hash": snapshot.lookup_snapshot_hash.as_str(),
                "writes_performed": false
            }),
        ));
    }
    let cli_source = link.registry.display().to_string();
    if snapshot.source.as_str() != cli_source.as_str() {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link v1 run registry source does not match the CLI registry",
            serde_json::json!({
                "stage": "link",
                "field": "run.metadata.registry_snapshot.source",
                "expected": cli_source.as_str(),
                "actual": snapshot.source.as_str(),
                "writes_performed": false
            }),
        ));
    }
    let firewall = &run.orchestration.profile_firewall;
    if firewall.registry_id.as_str() != snapshot.id.as_str()
        || firewall.registry_version.as_str() != snapshot.version.as_str()
        || firewall.registry_snapshot_hash.as_str() != snapshot.lookup_snapshot_hash.as_str()
    {
        return Err(entity_link_v1_artifact_refusal(
            "Entity link v1 run registry firewall does not match metadata snapshot",
            serde_json::json!({
                "stage": "link",
                "field": "run.orchestration.profile_firewall.registry",
                "metadata": {
                    "id": snapshot.id.as_str(),
                    "version": snapshot.version.as_str(),
                    "lookup_snapshot_hash": snapshot.lookup_snapshot_hash.as_str()
                },
                "firewall": {
                    "id": firewall.registry_id.as_str(),
                    "version": firewall.registry_version.as_str(),
                    "lookup_snapshot_hash": firewall.registry_snapshot_hash.as_str()
                },
                "writes_performed": false
            }),
        ));
    }
    Ok(resolve::ResolveRegistrySnapshot {
        id: snapshot.id.clone(),
        version: snapshot.version.clone(),
        source: snapshot.source.clone(),
    })
}

fn entity_link_value_string(
    value: &serde_json::Value,
    path: &[&str],
    field: &str,
) -> Result<String, CanonOutput> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment).ok_or_else(|| {
            entity_link_v1_artifact_refusal(
                "Entity link v1 artifact is missing a required field",
                serde_json::json!({
                    "stage": "link",
                    "field": field,
                    "writes_performed": false
                }),
            )
        })?;
    }
    current.as_str().map(str::to_string).ok_or_else(|| {
        entity_link_v1_artifact_refusal(
            "Entity link v1 artifact field must be a string",
            serde_json::json!({
                "stage": "link",
                "field": field,
                "writes_performed": false
            }),
        )
    })
}

fn entity_link_v1_artifact_refusal(
    message: impl Into<String>,
    detail: serde_json::Value,
) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EEntityArtifactContract,
        message.into(),
        detail,
        Some("Regenerate canon entity link artifacts from the current v1 run chain".to_string()),
    )
}

fn entity_link_output_value(
    artifact: &entity::run::link::EntityLinkArtifact,
    audit_receipt: Option<&serde_json::Value>,
) -> Result<serde_json::Value, serde_json::Error> {
    let mut output = serde_json::to_value(artifact)?;
    if let serde_json::Value::Object(object) = &mut output
        && let Some(receipt) = audit_receipt
    {
        object.insert("audit_artifact".to_string(), receipt.clone());
    }
    Ok(output)
}

fn run_entity_audit_command(audit: &EntityAuditCli) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(audit.emit, EntityEmitMode::Summary);
    let (result_probe, result_bytes) =
        match read_entity_lifecycle_json_artifact(&audit.result, "entity result artifact") {
            Ok((value, bytes)) => (value, bytes),
            Err(refusal_output) => return emit_entity_refusal(refusal_output, true, summary_mode),
        };
    if entity_artifact_value_is_v1(&result_probe) {
        match entity::audit::run_entity_audit_v1(entity::audit::EntityAuditV1Request {
            result_artifact: result_probe,
            suite_dir: &audit.suite,
        }) {
            Ok(artifact) => {
                let output = match audit.emit {
                    EntityEmitMode::Json => serde_json::to_string(&artifact)?,
                    EntityEmitMode::Summary => {
                        entity::audit::render_entity_audit_v1_summary(&artifact)
                    }
                };
                emit_entity_output(&output, summary_mode);
                return Ok(0);
            }
            Err(refusal) => {
                return emit_entity_refusal(refusal.to_canon_output(), true, summary_mode);
            }
        }
    }

    if entity_artifact_value_looks_like_native_run_v0(&result_probe) {
        match run_entity_native_run_audit(audit, &result_bytes) {
            Ok(artifact) => {
                let output = match audit.emit {
                    EntityEmitMode::Json => serde_json::to_string(&artifact)?,
                    EntityEmitMode::Summary => render_entity_native_audit_summary(&artifact),
                };
                emit_entity_output(&output, summary_mode);
                return Ok(0);
            }
            Err(refusal_output) => return emit_entity_refusal(refusal_output, true, summary_mode),
        }
    }

    if entity_artifact_value_looks_like_native_solve_v0(&result_probe) {
        match run_entity_native_solve_audit(audit, &result_bytes) {
            Ok(artifact) => {
                let output = match audit.emit {
                    EntityEmitMode::Json => serde_json::to_string(&artifact)?,
                    EntityEmitMode::Summary => render_entity_native_audit_summary(&artifact),
                };
                emit_entity_output(&output, summary_mode);
                return Ok(0);
            }
            Err(refusal_output) => return emit_entity_refusal(refusal_output, true, summary_mode),
        }
    }

    match run_entity_audit_pipeline(audit) {
        Ok(artifact) => {
            let output = match audit.emit {
                EntityEmitMode::Json => entity_runtime::output::emit_audit_json(&artifact)?,
                EntityEmitMode::Summary => entity_runtime::output::render_audit_summary(&artifact),
            };
            emit_entity_output(&output, summary_mode);
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(refusal_output, true, summary_mode),
    }
}

fn run_entity_promote_command(promote: &EntityPromoteCli) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(promote.emit, EntityEmitMode::Summary);
    let result_probe =
        match read_entity_lifecycle_json_artifact(&promote.result, "entity result artifact") {
            Ok((value, _)) => value,
            Err(refusal_output) => return emit_entity_refusal(refusal_output, true, summary_mode),
        };
    let audit_probe =
        match read_json_artifact::<serde_json::Value>(&promote.audit, "entity audit artifact") {
            Ok((value, _)) => value,
            Err(refusal_output) => return emit_entity_refusal(refusal_output, true, summary_mode),
        };
    if entity_artifact_value_is_v1(&result_probe) || entity_artifact_value_is_v1(&audit_probe) {
        match entity::promote::promote_entity_v1(entity::promote::EntityPromoteV1Request {
            result_path: promote.result.clone(),
            result_artifact: result_probe,
            audit_artifact: audit_probe,
            registry: promote.registry.clone(),
            next_version: promote.next_version.clone(),
        }) {
            Ok(artifact) => {
                let output = match promote.emit {
                    EntityEmitMode::Json => serde_json::to_string(&artifact)?,
                    EntityEmitMode::Summary => {
                        entity::promote::render_promote_v1_summary(&artifact)
                    }
                };
                emit_entity_output(&output, summary_mode);
                return Ok(0);
            }
            Err(refusal) => {
                return emit_entity_refusal(refusal.to_canon_output(), true, summary_mode);
            }
        }
    }

    match run_entity_promote_pipeline(promote) {
        Ok(artifact) => {
            let output = match promote.emit {
                EntityEmitMode::Json => entity_runtime::output::emit_promote_json(&artifact)?,
                EntityEmitMode::Summary => {
                    entity_runtime::output::render_promote_summary(&artifact)
                }
            };
            emit_entity_output(&output, summary_mode);
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(refusal_output, true, summary_mode),
    }
}

fn run_entity_apply_command(apply: &EntityApplyCli) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(apply.emit, EntityEmitMode::Summary);
    let result = match read_entity_lifecycle_json_artifact(&apply.result, "entity result artifact")
    {
        Ok((value, _)) => value,
        Err(refusal_output) => return emit_entity_refusal(refusal_output, true, summary_mode),
    };
    if let Err(refusal_output) = validate_entity_apply_result_artifact(&apply.result, &result) {
        return emit_entity_refusal(refusal_output, true, summary_mode);
    }

    let Some(lookup_column) = entity_apply_lookup_column(apply, &result) else {
        return emit_entity_refusal(
            entity_apply_missing_lookup_column_refusal(apply, &result),
            true,
            summary_mode,
        );
    };
    let output_path = entity_apply_output_path(apply, &result);
    let require_full_resolution = entity_apply_require_full_resolution(apply);
    match entity::apply::run_apply_v1_from_registry(entity::apply::ApplyV1ArtifactRequest {
        source_artifact: &result,
        rows: &apply.rows,
        output: &output_path,
        lookup_column: &lookup_column,
        registry_dir: &apply.registry,
        require_full_resolution,
        target_rows_per_chunk: entity::apply::DEFAULT_APPLY_ROWS_PER_CHUNK,
    }) {
        Ok(artifact) => {
            let exit_code = entity_apply_v1_exit_code(&artifact);
            let output = match apply.emit {
                EntityEmitMode::Json => serde_json::to_string(&artifact)?,
                EntityEmitMode::Summary => render_entity_apply_v1_summary(&artifact),
            };
            emit_entity_output(&output, summary_mode);
            Ok(exit_code)
        }
        Err(refusal) => emit_entity_refusal(refusal.to_canon_output(), true, summary_mode),
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
    if matches!(export.emit, EntityReviewExportEmitMode::Html)
        && !matches!(export.artifact, EntityReviewExportArtifact::NativeReview)
    {
        return emit_entity_refusal(entity_review_export_html_refusal(), false, false);
    }
    if matches!(export.group_by, Some(EntityReviewGroupBy::Signature))
        && !matches!(export.artifact, EntityReviewExportArtifact::NativeReview)
    {
        return emit_entity_refusal(
            entity_review_export_group_by_signature_refusal(),
            matches!(export.emit, EntityReviewExportEmitMode::Json),
            false,
        );
    }

    let (result_probe, result_bytes) =
        match read_entity_lifecycle_json_artifact(&export.result, "entity result artifact") {
            Ok((value, bytes)) => (value, bytes),
            Err(refusal_output) => {
                return emit_entity_refusal(
                    refusal_output,
                    matches!(export.emit, EntityReviewExportEmitMode::Json),
                    false,
                );
            }
        };

    if matches!(export.artifact, EntityReviewExportArtifact::NativeReview) {
        return run_entity_native_review_artifact_export_command(
            export,
            &result_probe,
            &result_bytes,
        );
    }

    if entity_artifact_value_looks_like_native_link_v1(&result_probe) {
        match run_entity_native_link_review_export(export, &result_probe, &result_bytes) {
            Ok(artifact) => {
                let output = render_entity_native_review_export(export, &artifact);
                match output {
                    Ok(output) => {
                        emit_entity_output(&output, false);
                        return Ok(0);
                    }
                    Err(refusal_output) => {
                        return emit_entity_refusal(
                            refusal_output,
                            matches!(export.emit, EntityReviewExportEmitMode::Json),
                            false,
                        );
                    }
                }
            }
            Err(refusal_output) => {
                return emit_entity_refusal(
                    refusal_output,
                    matches!(export.emit, EntityReviewExportEmitMode::Json),
                    false,
                );
            }
        }
    }

    if entity_artifact_value_is_v1(&result_probe) {
        let artifact =
            match entity::review::build_review_v1_artifact(entity::review::ReviewV1ExportRequest {
                result_artifact: result_probe,
                include: map_entity_review_include_v1(&export.include),
            }) {
                Ok(artifact) => artifact,
                Err(refusal) => {
                    return emit_entity_refusal(
                        refusal.to_canon_output(),
                        matches!(export.emit, EntityReviewExportEmitMode::Json),
                        false,
                    );
                }
            };
        let output = match export.emit {
            EntityReviewExportEmitMode::Json => serde_json::to_string(&artifact)?,
            EntityReviewExportEmitMode::Csv => {
                match entity::review::render_review_v1_csv(&artifact) {
                    Ok(output) => output,
                    Err(refusal) => {
                        return emit_entity_refusal(
                            refusal.to_canon_output(),
                            matches!(export.emit, EntityReviewExportEmitMode::Json),
                            false,
                        );
                    }
                }
            }
            EntityReviewExportEmitMode::Html => {
                return emit_entity_refusal(entity_review_export_html_refusal(), false, false);
            }
        };
        emit_entity_output(&output, false);
        return Ok(0);
    }

    if entity_artifact_value_looks_like_native_solve_v0(&result_probe) {
        match run_entity_native_solve_review_export(export, &result_bytes) {
            Ok(artifact) => {
                let output = render_entity_native_review_export(export, &artifact);
                match output {
                    Ok(output) => {
                        emit_entity_output(&output, false);
                        return Ok(0);
                    }
                    Err(refusal_output) => {
                        return emit_entity_refusal(
                            refusal_output,
                            matches!(export.emit, EntityReviewExportEmitMode::Json),
                            false,
                        );
                    }
                }
            }
            Err(refusal_output) => {
                return emit_entity_refusal(
                    refusal_output,
                    matches!(export.emit, EntityReviewExportEmitMode::Json),
                    false,
                );
            }
        }
    }

    if entity_artifact_value_looks_like_native_run_v0(&result_probe) {
        match run_entity_native_run_review_export(export, &result_bytes) {
            Ok(artifact) => {
                let output = render_entity_native_review_export(export, &artifact);
                match output {
                    Ok(output) => {
                        emit_entity_output(&output, false);
                        return Ok(0);
                    }
                    Err(refusal_output) => {
                        return emit_entity_refusal(
                            refusal_output,
                            matches!(export.emit, EntityReviewExportEmitMode::Json),
                            false,
                        );
                    }
                }
            }
            Err(refusal_output) => {
                return emit_entity_refusal(
                    refusal_output,
                    matches!(export.emit, EntityReviewExportEmitMode::Json),
                    false,
                );
            }
        }
    }

    match run_entity_review_export_pipeline(export) {
        Ok(artifact) => {
            let output = match export.emit {
                EntityReviewExportEmitMode::Json => Ok(serde_json::to_string(&artifact)?),
                EntityReviewExportEmitMode::Csv => {
                    entity_runtime::review::export_csv(&artifact).map_err(create_entity_refusal)
                }
                EntityReviewExportEmitMode::Html => Err(entity_review_export_html_refusal()),
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
    let summary_mode = matches!(import.emit, EntityEmitMode::Summary);
    let review_bytes = match read_artifact_bytes(&import.review, "entity review artifact") {
        Ok(bytes) => bytes,
        Err(refusal_output) => return emit_entity_refusal(refusal_output, true, summary_mode),
    };
    if let Some(source_review) = import.source_review.as_ref() {
        return run_entity_native_review_import_command(
            import,
            source_review,
            &review_bytes,
            summary_mode,
        );
    }

    if entity::review_import::review_import_input_looks_v1(&review_bytes) {
        let audit_data = if let Some(audit_path) = import.audit.as_ref() {
            match read_json_artifact::<serde_json::Value>(audit_path, "entity audit artifact") {
                Ok(data) => Some(data),
                Err(refusal_output) => {
                    return emit_entity_refusal(refusal_output, true, summary_mode);
                }
            }
        } else {
            None
        };
        let audit = audit_data
            .as_ref()
            .map(|(audit, bytes)| (audit, bytes.as_slice()));
        match entity::review_import::import_review_v1(
            entity::review_import::ReviewImportV1Request {
                review_path: &import.review,
                review_bytes: &review_bytes,
                registry: &import.registry,
                next_version: &import.next_version,
                audit,
            },
        ) {
            Ok(artifact) => {
                let output = match import.emit {
                    EntityEmitMode::Json => serde_json::to_string(&artifact)?,
                    EntityEmitMode::Summary => {
                        entity::review_import::render_review_import_v1_summary(&artifact)
                    }
                };
                emit_entity_output(&output, summary_mode);
                return Ok(0);
            }
            Err(refusal) => {
                return emit_entity_refusal(refusal.to_canon_output(), true, summary_mode);
            }
        }
    }

    match run_entity_review_import_pipeline(import) {
        Ok(artifact) => {
            let output = match import.emit {
                EntityEmitMode::Json => serde_json::to_string(&artifact)?,
                EntityEmitMode::Summary => artifact.render_summary(),
            };
            emit_entity_output(&output, summary_mode);
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(refusal_output, true, summary_mode),
    }
}

fn run_entity_native_review_import_command(
    import: &EntityReviewImportCli,
    source_review: &Path,
    review_bytes: &[u8],
    summary_mode: bool,
) -> Result<u8, Box<dyn Error>> {
    let (source_review_artifact, _source_review_bytes) = match read_json_artifact::<serde_json::Value>(
        source_review,
        "entity native review artifact",
    ) {
        Ok(data) => data,
        Err(refusal_output) => return emit_entity_refusal(refusal_output, true, summary_mode),
    };
    let context = match entity::review_import::native_review_import_context_from_artifact(
        &source_review_artifact,
    ) {
        Ok(context) => context,
        Err(refusal) => return emit_entity_refusal(refusal.to_canon_output(), true, summary_mode),
    };
    let decisions = match parse_native_review_decisions(
        &import.review,
        review_bytes,
        Some(&source_review_artifact),
    ) {
        Ok(decisions) => decisions,
        Err(refusal_output) => return emit_entity_refusal(refusal_output, true, summary_mode),
    };

    match entity::review_import::import_native_review_decisions(context, decisions) {
        Ok(receipt) => {
            let output = match import.emit {
                EntityEmitMode::Json => serde_json::to_string(&receipt)?,
                EntityEmitMode::Summary => render_native_review_import_summary(&receipt),
            };
            emit_entity_output(&output, summary_mode);
            Ok(0)
        }
        Err(refusal) => emit_entity_refusal(refusal.to_canon_output(), true, summary_mode),
    }
}

#[allow(clippy::result_large_err)]
fn parse_native_review_decisions(
    path: &Path,
    bytes: &[u8],
    source_review_artifact: Option<&serde_json::Value>,
) -> Result<Vec<entity::review_import::NativeReviewDecision>, CanonOutput> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        native_review_import_refusal(
            "Native review decisions must be UTF-8 JSON or CSV",
            serde_json::json!({
                "stage": "native_review_import",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let result = if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
    {
        entity::review_import::parse_native_review_import_csv(text)
    } else if let Some(source_review_artifact) = source_review_artifact {
        entity::review_import::parse_native_review_import_json_with_source(
            text,
            source_review_artifact,
        )
    } else {
        entity::review_import::parse_native_review_import_json(text)
    };
    result.map_err(|refusal| refusal.to_canon_output())
}

fn render_native_review_import_summary(
    receipt: &entity::review_import::NativeReviewImportReceipt,
) -> String {
    format!(
        "{} decisions={} aliases={} cannot_link={} relations={} assignments={} defer={}",
        receipt.version,
        receipt.accepted_decisions,
        receipt.patches.alias_patches.len(),
        receipt.patches.cannot_link_patches.len(),
        receipt.patches.relation_patches.len(),
        receipt.patches.assignment_patches.len(),
        receipt.patches.defer_patches.len()
    )
}

fn run_entity_explain_command(explain: &EntityExplainCli) -> Result<u8, Box<dyn Error>> {
    let summary_mode = matches!(explain.emit, EntityEmitMode::Summary);
    let result_probe =
        match read_entity_lifecycle_json_artifact(&explain.result, "entity result artifact") {
            Ok((value, _)) => value,
            Err(refusal_output) => return emit_entity_refusal(refusal_output, true, summary_mode),
        };
    if entity_artifact_value_is_v1(&result_probe) {
        let explain_source =
            match entity_explain_v1_source_from_result(&explain.result, result_probe) {
                Ok(source) => source,
                Err(refusal_output) => {
                    return emit_entity_refusal(refusal_output, true, summary_mode);
                }
            };
        match entity::explain::explain_entity_v1(
            entity::explain::EntityExplainV1Query {
                row_id: explain.row.clone(),
                surface_id: explain.surface_id.clone(),
                canonical_id: explain.canon_id.clone(),
                escrow_id: explain.escrow_id.clone(),
            },
            explain_source,
        ) {
            Ok(artifact) => {
                let output = match explain.emit {
                    EntityEmitMode::Json => serde_json::to_string(&artifact)?,
                    EntityEmitMode::Summary => {
                        entity::explain::render_explain_v1_summary(&artifact)
                    }
                };
                emit_entity_output(&output, summary_mode);
                return Ok(0);
            }
            Err(refusal) => {
                return emit_entity_refusal(refusal.to_canon_output(), true, summary_mode);
            }
        }
    }

    match run_entity_explain_pipeline(explain) {
        Ok(artifact) => {
            let output = match explain.emit {
                EntityEmitMode::Json => entity_runtime::output::emit_explain_json(&artifact)?,
                EntityEmitMode::Summary => {
                    entity_runtime::output::render_explain_summary(&artifact)
                }
            };
            emit_entity_output(&output, summary_mode);
            Ok(0)
        }
        Err(refusal_output) => emit_entity_refusal(refusal_output, true, summary_mode),
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
            let result = entity::run::run_entity_workbench_with_cache_mode(
                entity::run::EntityRunRequest {
                    rows: &run.rows,
                    profile,
                    strategy: &run.strategy,
                    registry: &run.registry,
                    work_dir,
                },
                entity_index_cache_mode(run.cache_mode),
            )
            .map_err(|refusal| refusal.to_canon_output())?;
            Ok(EntityRunExecution {
                artifact: Box::new(result.artifact),
                artifact_value: result.artifact_value,
                candidate_pairs: result.candidate_pairs,
            })
        }
        _ => Err(entity_missing_v1_context_refusal(
            entity::EntityArtifactStageV1::Run,
            &run.rows,
            &run.profile,
            &run.strategy,
            &run.registry,
            &run.work_dir,
            None,
        )),
    }
}

#[allow(clippy::result_large_err)]
fn run_entity_prepare_pipeline(
    prepare: &EntityPrepareCli,
) -> Result<serde_json::Value, CanonOutput> {
    entity::prepare::run_prepare_v1(entity::prepare::PrepareRunRequest {
        rows: &prepare.rows,
        profile: &prepare.profile,
        registry: &prepare.registry,
        work_dir: &prepare.work_dir,
    })
    .map_err(|refusal| refusal.to_canon_output())
}

#[allow(clippy::result_large_err)]
fn run_entity_alias_withholding_pipeline(
    alias_withholding: &EntityAliasWithholdingCli,
) -> Result<evaluation::alias_withholding::AliasWithholdingReport, CanonOutput> {
    let envelope = read_alias_withholding_execution_envelope(&alias_withholding.manifest)?;
    let base_dir = alias_withholding_manifest_base_dir(&alias_withholding.manifest);
    evaluation::alias_withholding::compile_alias_withholding_execution_envelope(envelope, &base_dir)
        .map_err(|error| alias_withholding_refusal(&alias_withholding.manifest, error))
}

#[allow(clippy::result_large_err)]
fn read_alias_withholding_execution_envelope(
    path: &Path,
) -> Result<evaluation::alias_withholding::AliasWithholdingExecutionEnvelope, CanonOutput> {
    let bytes = fs::read(path).map_err(|error| {
        alias_withholding_manifest_refusal(
            path,
            "manifest_read_failed",
            "Failed to read alias-withholding execution envelope".to_string(),
            serde_json::json!({
                "io_error_kind": format!("{:?}", error.kind()),
            }),
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        alias_withholding_manifest_refusal(
            path,
            "manifest_parse_failed",
            "Failed to parse alias-withholding execution envelope".to_string(),
            serde_json::json!({
                "line": error.line(),
                "column": error.column(),
            }),
        )
    })
}

fn alias_withholding_manifest_base_dir(manifest: &Path) -> PathBuf {
    manifest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn render_alias_withholding_report_summary(
    report: &evaluation::alias_withholding::AliasWithholdingReport,
) -> String {
    format!(
        "{} trials={} clean_base_snapshots={} credited={} abstain={} reject={} unsupported_guess={} report_digest={}",
        alias_withholding_public_fingerprint(report.benchmark_id.as_bytes()),
        report.aggregate.trial_count,
        report.aggregate.clean_base_snapshot_count,
        report.aggregate.credited_attachment_count,
        report.aggregate.abstain_count,
        report.aggregate.reject_count,
        report.aggregate.unsupported_guess_count,
        report.report_digest
    )
}

fn alias_withholding_cli_report_json(
    report: &evaluation::alias_withholding::AliasWithholdingReport,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut value = serde_json::to_value(report)?;
    alias_withholding_redact_identifier_fields(&mut value);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "identifier_redaction".to_string(),
            serde_json::Value::String("blake3".to_string()),
        );
    }
    Ok(value)
}

fn alias_withholding_redact_identifier_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                let identifier_scalar = key.ends_with("_id")
                    || key.ends_with("_path")
                    || matches!(key.as_str(), "source_ref" | "canonical_hint");
                if identifier_scalar && let Some(raw) = child.as_str() {
                    *child = serde_json::Value::String(alias_withholding_public_fingerprint(
                        raw.as_bytes(),
                    ));
                    continue;
                }
                let identifier_array = key.ends_with("_ids") || key == "surface_ids";
                if identifier_array && let Some(items) = child.as_array_mut() {
                    for item in items {
                        if let Some(raw) = item.as_str() {
                            *item = serde_json::Value::String(
                                alias_withholding_public_fingerprint(raw.as_bytes()),
                            );
                        }
                    }
                    continue;
                }
                alias_withholding_redact_identifier_fields(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                alias_withholding_redact_identifier_fields(item);
            }
        }
        _ => {}
    }
}

fn alias_withholding_public_fingerprint(bytes: &[u8]) -> String {
    witness::hash_bytes(bytes)
}

fn alias_withholding_path_fingerprint(path: &Path) -> String {
    alias_withholding_public_fingerprint(path.to_string_lossy().as_bytes())
}

fn alias_withholding_public_reason(
    code: evaluation::alias_withholding::AliasWithholdingErrorCode,
) -> &'static str {
    match code {
        evaluation::alias_withholding::AliasWithholdingErrorCode::ArtifactContract => {
            "artifact_contract"
        }
        evaluation::alias_withholding::AliasWithholdingErrorCode::MissingReference => {
            "missing_reference"
        }
        evaluation::alias_withholding::AliasWithholdingErrorCode::DuplicateRecord => {
            "duplicate_record"
        }
        evaluation::alias_withholding::AliasWithholdingErrorCode::IneligibleAlias => {
            "ineligible_alias"
        }
        evaluation::alias_withholding::AliasWithholdingErrorCode::ExactLookupLeak => {
            "exact_lookup_leak"
        }
        evaluation::alias_withholding::AliasWithholdingErrorCode::SideChannelLeak => {
            "side_channel_leak"
        }
        evaluation::alias_withholding::AliasWithholdingErrorCode::ReplayMismatch => {
            "replay_mismatch"
        }
        evaluation::alias_withholding::AliasWithholdingErrorCode::Unimplemented => "unimplemented",
    }
}

fn alias_withholding_refusal(
    manifest: &Path,
    error: evaluation::alias_withholding::AliasWithholdingError,
) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EEntityArtifactContract,
        "Alias-withholding execution envelope failed validation".to_string(),
        serde_json::json!({
            "stage": "alias_withholding",
            "manifest_fingerprint": alias_withholding_path_fingerprint(manifest),
            "alias_withholding_code": error.code,
            "public_reason": alias_withholding_public_reason(error.code),
            "message_fingerprint": alias_withholding_public_fingerprint(error.message.as_bytes()),
            "writes_performed": false,
        }),
        Some(
            "Fix the execution envelope or referenced artifacts, then rerun canon entity alias-withholding --manifest <EXECUTION_ENVELOPE.json>"
                .to_string(),
        ),
    )
}

fn alias_withholding_manifest_refusal(
    manifest: &Path,
    reason: &str,
    message: String,
    detail: serde_json::Value,
) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EEntityArtifactContract,
        message,
        serde_json::json!({
            "stage": "alias_withholding",
            "reason": reason,
            "manifest_fingerprint": alias_withholding_path_fingerprint(manifest),
            "detail": detail,
            "writes_performed": false,
        }),
        Some(
            "Fix the execution envelope path or JSON, then rerun canon entity alias-withholding --manifest <EXECUTION_ENVELOPE.json>"
                .to_string(),
        ),
    )
}

#[allow(clippy::result_large_err)]
fn run_entity_generalization_pipeline(
    generalization: &EntityGeneralizationCli,
) -> Result<evaluation::generalization::GeneralizationReport, CanonOutput> {
    evaluation::generalization::compile_strict_generalization_manifest(&generalization.manifest)
        .map_err(|error| generalization_refusal(&generalization.manifest, error))
}

fn render_generalization_report_summary(
    report: &evaluation::generalization::GeneralizationReport,
) -> String {
    let (failed_gate_count, not_applicable_gate_count) =
        generalization_quality_gate_status_counts(report);
    format!(
        "{} corpus={} visibility={} release_claim_status={} failed_gate_count={} not_applicable_gate_count={} entity_disjoint_trials={} time_forward_trials={} results={} correct={} abstain={} critical_false_merge={} directional_cross_source={} head={} tail={} easy={} hard={} report_digest={}",
        alias_withholding_public_fingerprint(report.benchmark_id.as_bytes()),
        alias_withholding_public_fingerprint(report.corpus_ref.as_bytes()),
        generalization_corpus_visibility_label(report.corpus_visibility),
        generalization_release_claim_status_label(&report.quality.release_claim_status),
        failed_gate_count,
        not_applicable_gate_count,
        report.aggregate.entity_disjoint_trial_count,
        report.aggregate.time_forward_trial_count,
        report.aggregate.result_count,
        report.aggregate.correct_count,
        report.aggregate.abstain_count,
        report.aggregate.critical_false_merge_count,
        report.aggregate.directional_cross_source_count,
        report.aggregate.head_result_count,
        report.aggregate.tail_result_count,
        report.aggregate.easy_result_count,
        report.aggregate.hard_result_count,
        report.report_digest
    )
}

fn generalization_release_claim_status_label(
    status: &evaluation::generalization::GeneralizationReleaseClaimStatus,
) -> &'static str {
    match status {
        evaluation::generalization::GeneralizationReleaseClaimStatus::Eligible => "eligible",
        evaluation::generalization::GeneralizationReleaseClaimStatus::Blocked => "blocked",
    }
}

fn generalization_quality_gate_status_counts(
    report: &evaluation::generalization::GeneralizationReport,
) -> (usize, usize) {
    let mut failed_gate_count = 0;
    let mut not_applicable_gate_count = 0;
    for gate in &report.quality.gates {
        match &gate.status {
            evaluation::generalization::GeneralizationQualityGateStatus::Fail => {
                failed_gate_count += 1;
            }
            evaluation::generalization::GeneralizationQualityGateStatus::NotApplicable => {
                not_applicable_gate_count += 1;
            }
            evaluation::generalization::GeneralizationQualityGateStatus::Pass => {}
        }
    }
    (failed_gate_count, not_applicable_gate_count)
}

fn generalization_cli_report_json(
    report: &evaluation::generalization::GeneralizationReport,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut value = serde_json::to_value(report)?;
    generalization_redact_identifier_fields(&mut value);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "identifier_redaction".to_string(),
            serde_json::Value::String("blake3".to_string()),
        );
    }
    Ok(value)
}

fn generalization_redact_identifier_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                if generalization_identifier_scalar_key(key)
                    && let Some(raw) = child.as_str()
                {
                    *child = serde_json::Value::String(alias_withholding_public_fingerprint(
                        raw.as_bytes(),
                    ));
                    continue;
                }
                if generalization_identifier_array_key(key)
                    && let Some(items) = child.as_array_mut()
                {
                    for item in items {
                        if let Some(raw) = item.as_str() {
                            *item = serde_json::Value::String(
                                alias_withholding_public_fingerprint(raw.as_bytes()),
                            );
                        }
                    }
                    continue;
                }
                generalization_redact_identifier_fields(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                generalization_redact_identifier_fields(item);
            }
        }
        _ => {}
    }
}

fn generalization_identifier_scalar_key(key: &str) -> bool {
    if matches!(key, "gate_id" | "metric_id") {
        return false;
    }
    key.ends_with("_id")
        || key.ends_with("_path")
        || key.ends_with("_ref")
        || matches!(
            key,
            "benchmark_id"
                | "corpus_ref"
                | "cutoff"
                | "path"
                | "root"
                | "locator"
                | "canonical_hint"
        )
}

fn generalization_identifier_array_key(key: &str) -> bool {
    key.ends_with("_ids") || matches!(key, "paths" | "locators" | "source_refs")
}

fn generalization_corpus_visibility_label(
    visibility: evaluation::generalization::CorpusVisibility,
) -> &'static str {
    match visibility {
        evaluation::generalization::CorpusVisibility::PublicFixture => "public_fixture",
        evaluation::generalization::CorpusVisibility::PrivateCorpusRef => "private_corpus_ref",
    }
}

fn generalization_public_reason(
    code: evaluation::generalization::GeneralizationErrorCode,
) -> &'static str {
    match code {
        evaluation::generalization::GeneralizationErrorCode::ArtifactContract => {
            "artifact_contract"
        }
        evaluation::generalization::GeneralizationErrorCode::MissingReference => {
            "missing_reference"
        }
        evaluation::generalization::GeneralizationErrorCode::DuplicateRecord => "duplicate_record",
        evaluation::generalization::GeneralizationErrorCode::EntityDisjointLeak => {
            "entity_disjoint_leak"
        }
        evaluation::generalization::GeneralizationErrorCode::FutureLeakage => "future_leakage",
        evaluation::generalization::GeneralizationErrorCode::TemporalReversal => {
            "temporal_reversal"
        }
        evaluation::generalization::GeneralizationErrorCode::CriticalFalseMerge => {
            "critical_false_merge"
        }
        evaluation::generalization::GeneralizationErrorCode::DirectionalLinkContract => {
            "directional_link_contract"
        }
        evaluation::generalization::GeneralizationErrorCode::Unimplemented => "unimplemented",
    }
}

fn generalization_refusal(
    manifest: &Path,
    error: evaluation::generalization::GeneralizationError,
) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EEntityArtifactContract,
        "Generalization strict execution envelope failed validation".to_string(),
        serde_json::json!({
            "stage": "generalization",
            "manifest_fingerprint": alias_withholding_path_fingerprint(manifest),
            "generalization_code": error.code,
            "public_reason": generalization_public_reason(error.code),
            "message_fingerprint": alias_withholding_public_fingerprint(error.message.as_bytes()),
            "writes_performed": false,
        }),
        Some(
            "Fix the strict execution envelope or referenced artifacts, then rerun canon entity generalization --manifest <STRICT_ENVELOPE.json>"
                .to_string(),
        ),
    )
}

#[allow(clippy::result_large_err)]
fn run_entity_candidate_recall_pipeline(
    recall: &EntityCandidateRecallCli,
) -> Result<entity::telemetry::EntityCandidateRecallReport, CanonOutput> {
    let (manifest, _): (CandidateRecallManifest, Vec<u8>) =
        read_json_artifact(&recall.manifest, "entity candidate recall manifest")?;
    let candidate_records = read_block_candidate_records_artifact(&recall.candidates)?;
    let (diagnostics, _): (entity::block::BlockCandidateGenerationDiagnostics, Vec<u8>) =
        read_json_artifact(&recall.diagnostics, "entity block candidate diagnostics")?;
    let (surface_ids, gold_pairs) = candidate_recall_manifest_gold(&manifest)?;
    let report =
        entity::block::evaluate_candidate_recall(entity::block::CandidateRecallEvaluationRequest {
            candidate_records: &candidate_records,
            diagnostics: &diagnostics,
            gold_pairs: &gold_pairs,
            surface_ids: &surface_ids,
            exact_bucket_count: recall.exact_bucket_count,
        });
    report
        .validate()
        .map_err(candidate_recall_validation_refusal)?;
    Ok(report)
}

#[derive(Debug, Clone, Deserialize)]
struct CandidateRecallManifest {
    #[serde(default)]
    observations: Vec<CandidateRecallManifestObservation>,
    quality_harness: CandidateRecallManifestHarness,
}

#[derive(Debug, Clone, Deserialize)]
struct CandidateRecallManifestObservation {
    observation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CandidateRecallManifestHarness {
    #[serde(default)]
    cases: Vec<CandidateRecallManifestCase>,
}

#[derive(Debug, Clone, Deserialize)]
struct CandidateRecallManifestCase {
    case_id: String,
    left_observation_id: String,
    right_observation_id: String,
    stratum: String,
    label_disposition: String,
}

#[allow(clippy::result_large_err)]
fn candidate_recall_manifest_gold(
    manifest: &CandidateRecallManifest,
) -> Result<(Vec<String>, Vec<entity::telemetry::CandidateRecallGoldPair>), CanonOutput> {
    let mut surface_ids = manifest
        .observations
        .iter()
        .map(|observation| observation.observation_id.trim())
        .filter(|observation_id| !observation_id.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    surface_ids.sort();
    surface_ids.dedup();

    let mut gold_pairs = Vec::new();
    for case in &manifest.quality_harness.cases {
        if case.label_disposition != "same_entity" {
            continue;
        }
        let stratum = candidate_recall_manifest_stratum(&case.stratum)?;
        gold_pairs.push(entity::telemetry::CandidateRecallGoldPair::new(
            &case.case_id,
            &case.left_observation_id,
            &case.right_observation_id,
            stratum,
        ));
    }
    gold_pairs.sort_by(|left, right| left.gold_pair_id.cmp(&right.gold_pair_id));
    Ok((surface_ids, gold_pairs))
}

#[allow(clippy::result_large_err)]
fn candidate_recall_manifest_stratum(
    stratum: &str,
) -> Result<entity::telemetry::CandidateRecallStratum, CanonOutput> {
    match stratum {
        "exact_known" | "exact_known_replay" => {
            Ok(entity::telemetry::CandidateRecallStratum::ExactKnown)
        }
        "withheld_alias" | "withheld_alias_incumbent" => {
            Ok(entity::telemetry::CandidateRecallStratum::WithheldAlias)
        }
        "novel_cluster" | "novel_multi_observation" => {
            Ok(entity::telemetry::CandidateRecallStratum::NovelCluster)
        }
        "directional_link" | "directional_cross_dataset_link" => {
            Ok(entity::telemetry::CandidateRecallStratum::DirectionalLink)
        }
        _ => Err(refusal::create_refusal(
            RefusalCode::EParse,
            format!("Unsupported candidate recall stratum '{stratum}'"),
            serde_json::json!({
                "reason": "unsupported_candidate_recall_stratum",
                "stratum": stratum,
                "supported_strata": [
                    "exact_known",
                    "exact_known_replay",
                    "withheld_alias",
                    "withheld_alias_incumbent",
                    "novel_cluster",
                    "novel_multi_observation",
                    "directional_link",
                    "directional_cross_dataset_link"
                ],
                "writes_performed": false
            }),
            Some("Use a canon quality manifest with supported candidate recall strata".to_string()),
        )),
    }
}

fn candidate_recall_validation_refusal(
    error: entity::telemetry::EntityTelemetryValidationError,
) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EEntityArtifactContract,
        "Entity candidate recall report failed validation".to_string(),
        serde_json::json!({
            "reason": "candidate_recall_report_invalid",
            "field": error.field,
            "message": error.message,
            "writes_performed": false
        }),
        Some("Regenerate candidate recall inputs from matching block artifacts".to_string()),
    )
}

fn entity_candidate_recall_summary(
    report: &entity::telemetry::EntityCandidateRecallReport,
) -> String {
    let recall_50 = report
        .union_recall_at_k
        .iter()
        .find(|metric| metric.k == 50)
        .map_or(0.0, |metric| metric.recall);
    format!(
        "{} gold_pairs={} recall@50={:.3} misses_at_50={} operators={}",
        report.version,
        report.total_gold_pairs,
        recall_50,
        report.misses_at_50.len(),
        report.operators.len()
    )
}

fn entity_missing_v1_context_refusal(
    stage: entity::EntityArtifactStageV1,
    rows: &Path,
    profile: &Option<String>,
    strategy: &Path,
    registry: &Path,
    work_dir: &Option<PathBuf>,
    suite: Option<&PathBuf>,
) -> CanonOutput {
    let contract = entity_runtime::entity_v1_contract_for_stage(stage);
    refusal::create_refusal(
        RefusalCode::EEntityInputContract,
        format!(
            "Artifact-backed entity {} requires explicit --profile and --work-dir",
            stage.as_str()
        ),
        serde_json::json!({
            "reason": "legacy_dispatch_removed",
            "stage": stage.as_str(),
            "command": contract.command,
            "artifact_version": contract.artifact_version,
            "rows": rows.display().to_string(),
            "profile_present": profile.is_some(),
            "strategy": strategy.display().to_string(),
            "registry": registry.display().to_string(),
            "work_dir_present": work_dir.is_some(),
            "suite": suite.map(|path| path.display().to_string()),
            "writes_performed": false,
            "legacy_dispatch_allowed": false
        }),
        Some(format!(
            "{} {} --profile <PROFILE> --strategy {} --registry {} --work-dir <DIR>",
            contract.command,
            rows.display(),
            strategy.display(),
            registry.display()
        )),
    )
}

fn entity_missing_block_stage_args_refusal(block: &EntityBlockCli) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EEntityInputContract,
        "Entity block requires rows, --strategy, and --registry unless using a block subcommand"
            .to_string(),
        serde_json::json!({
            "reason": "missing_entity_block_stage_args",
            "stage": "block",
            "command": "canon entity block",
            "rows_present": block.rows.is_some(),
            "profile_present": block.profile.is_some(),
            "strategy_present": block.strategy.is_some(),
            "registry_present": block.registry.is_some(),
            "work_dir_present": block.work_dir.is_some(),
            "subcommand_present": block.command.is_some(),
            "writes_performed": false
        }),
        Some(
            "canon entity block <ROWS> --profile <PROFILE> --strategy <STRATEGY.yaml> --registry <REGISTRY_DIR> --work-dir <DIR>"
                .to_string(),
        ),
    )
}

fn entity_missing_link_context_refusal(link: &EntityLinkCli) -> CanonOutput {
    let stage = entity::EntityArtifactStageV1::Run;
    let contract = entity_runtime::entity_v1_contract_for_stage(stage);
    refusal::create_refusal(
        RefusalCode::EEntityInputContract,
        "Artifact-backed entity link requires explicit --profile and --work-dir".to_string(),
        serde_json::json!({
            "reason": "legacy_dispatch_removed",
            "stage": "link",
            "compiled_stage": stage.as_str(),
            "command": "canon entity link",
            "compiled_command": contract.command,
            "artifact_version": contract.artifact_version,
            "reference": link.reference.display().to_string(),
            "target": link.target.display().to_string(),
            "profile_present": link.profile.is_some(),
            "strategy": link.strategy.display().to_string(),
            "registry": link.registry.display().to_string(),
            "work_dir_present": link.work_dir.is_some(),
            "suite": link.suite.as_ref().map(|path| path.display().to_string()),
            "writes_performed": false,
            "legacy_dispatch_allowed": false
        }),
        Some(format!(
            "canon entity link {} {} --profile <PROFILE> --strategy {} --registry {} --work-dir <DIR>",
            link.reference.display(),
            link.target.display(),
            link.strategy.display(),
            link.registry.display()
        )),
    )
}

fn entity_link_v1_write_back_refusal(link: &EntityLinkCli, work_dir: &Path) -> CanonOutput {
    let link_artifact = work_dir.join(entity::run::link::LINK_ARTIFACT_PATH);
    refusal::create_refusal(
        RefusalCode::EEntityArtifactContract,
        "canon entity link --write-back is disabled on the v1 public path until transactional registry publication is available".to_string(),
        serde_json::json!({
            "reason": "transactional_publication_required",
            "stage": "link",
            "flag": "--write-back",
            "registry": link.registry.display().to_string(),
            "link_artifact": link_artifact.display().to_string(),
            "writes_performed": false,
            "registry_write_back_performed": false,
            "legacy_dispatch_allowed": false
        }),
        Some(format!(
            "Run canon entity link without --write-back, then review/promote/apply the v1 artifact: canon entity review export {} --include escrow --emit csv",
            link_artifact.display()
        )),
    )
}

fn entity_link_summary(
    artifact: &entity::run::link::EntityLinkArtifact,
    audit_receipt: Option<&serde_json::Value>,
) -> String {
    let audit = audit_receipt
        .and_then(|receipt| receipt.get("path").and_then(serde_json::Value::as_str))
        .map(|path| format!(" audit_artifact={path}"))
        .unwrap_or_default();
    format!(
        "{} mode={:?} reference_records={} target_records={} matched={} unmatched={} ambiguous={} match_rate={:.3} materialized_rows={} shared_run={}@{}",
        artifact.version,
        artifact.mode,
        artifact.reference.row_count,
        artifact.target.row_count,
        artifact.summary.matched,
        artifact.summary.unmatched,
        artifact.summary.ambiguous,
        artifact.summary.match_rate,
        artifact.materialized_rows_path,
        artifact.shared_run_artifact.version,
        artifact.shared_run_artifact.content_hash,
    ) + &audit
}

fn append_entity_link_witness(
    link: &EntityLinkCli,
    decisions: &resolve::ResolveArtifact,
    audit_receipt: Option<&serde_json::Value>,
    output: &str,
    exit_code: u8,
) {
    if link.no_witness {
        return;
    }

    let mut inputs = vec![
        witness::WitnessInput {
            path: link.reference.display().to_string(),
            hash: hash_input_path(&link.reference),
            bytes: input_size(&link.reference),
        },
        witness::WitnessInput {
            path: link.target.display().to_string(),
            hash: hash_input_path(&link.target),
            bytes: input_size(&link.target),
        },
        witness::WitnessInput {
            path: link.strategy.display().to_string(),
            hash: hash_input_path(&link.strategy),
            bytes: input_size(&link.strategy),
        },
        witness::WitnessInput {
            path: link.registry.display().to_string(),
            hash: None,
            bytes: None,
        },
    ];
    if let Some(gold) = &link.gold {
        inputs.push(witness::WitnessInput {
            path: gold.display().to_string(),
            hash: hash_input_path(gold),
            bytes: input_size(gold),
        });
    }
    if let Some(suite) = &link.suite {
        inputs.push(witness::WitnessInput {
            path: suite.display().to_string(),
            hash: None,
            bytes: None,
        });
    }

    let mut params = serde_json::Map::new();
    params.insert(
        "command".to_string(),
        serde_json::Value::String("entity.link".to_string()),
    );
    params.insert(
        "registry_id".to_string(),
        serde_json::Value::String(decisions.registry.id.clone()),
    );
    params.insert(
        "registry_version".to_string(),
        serde_json::Value::String(decisions.registry.version.clone()),
    );
    params.insert(
        "strategy_id".to_string(),
        serde_json::Value::String(decisions.strategy.id.clone()),
    );
    params.insert(
        "strategy_version".to_string(),
        serde_json::Value::String(decisions.strategy.version.clone()),
    );
    params.insert(
        "strategy_hash".to_string(),
        serde_json::Value::String(decisions.strategy.content_hash.clone()),
    );
    params.insert(
        "write_back".to_string(),
        serde_json::Value::Bool(link.write_back),
    );
    params.insert(
        "cache_mode".to_string(),
        serde_json::Value::String(
            entity_index_cache_mode(link.cache_mode)
                .as_str()
                .to_string(),
        ),
    );
    params.insert(
        "summary".to_string(),
        serde_json::json!({
            "target_records": decisions.summary.target_records,
            "matched": decisions.summary.matched,
            "unmatched": decisions.summary.unmatched,
            "ambiguous": decisions.summary.ambiguous,
            "match_rate": decisions.summary.match_rate
        }),
    );
    if let Some(max_candidates) = link.max_candidates {
        params.insert(
            "max_candidates".to_string(),
            serde_json::Value::from(max_candidates as u64),
        );
    }
    if let Some(max_rows) = link.max_rows {
        params.insert(
            "max_rows".to_string(),
            serde_json::Value::from(max_rows as u64),
        );
    }
    if let Some(max_bytes) = link.max_bytes {
        params.insert("max_bytes".to_string(), serde_json::Value::from(max_bytes));
    }
    if let Some(suite) = &link.suite {
        params.insert(
            "suite".to_string(),
            serde_json::Value::String(suite.display().to_string()),
        );
    }
    if let Some(receipt) = audit_receipt {
        params.insert("audit_artifact".to_string(), receipt.clone());
    }

    let output_hash = witness::hash_bytes(output.as_bytes());
    let outcome = if exit_code == 0 {
        "RESOLVED"
    } else {
        "PARTIAL"
    };
    let record = witness::WitnessRecord::new(inputs, params, &output_hash, outcome, exit_code);
    if let Err(error) = witness::append_witness_record(&record, false) {
        eprintln!("Warning: failed to append entity link witness: {}", error);
    }
}

#[allow(clippy::result_large_err)]
fn validate_entity_v1_input_artifact(
    path: &Path,
    label: &str,
    expected_stage: entity::EntityArtifactStageV1,
) -> Result<(), CanonOutput> {
    let (artifact, _bytes): (serde_json::Value, Vec<u8>) = read_json_artifact(path, label)?;
    let Some(version) = artifact.get("version").and_then(|version| version.as_str()) else {
        return Err(refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            format!(
                "{} '{}' is missing an entity artifact version",
                label,
                path.display()
            ),
            serde_json::json!({
                "reason": "missing_artifact_version",
                "path": path.display().to_string(),
                "artifact": label,
                "expected_stage": expected_stage.as_str(),
                "writes_performed": false
            }),
            Some(format!("{} --help", expected_stage.command())),
        ));
    };

    if let Some(replacement) = entity::entity_artifact_v1_contract_for_legacy_version(version) {
        return Err(refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            format!(
                "Legacy entity artifact '{}' is not accepted by v1 dispatch",
                version
            ),
            serde_json::json!({
                "reason": "legacy_entity_artifact_refused",
                "path": path.display().to_string(),
                "artifact": label,
                "actual_version": version,
                "replacement_stage": replacement.stage.as_str(),
                "replacement_command": replacement.command,
                "expected_stage": expected_stage.as_str(),
                "expected_version": entity_runtime::entity_v1_contract_for_stage(expected_stage).artifact_version,
                "writes_performed": false,
                "legacy_artifacts_allowed": false
            }),
            Some(format!("{} --help", replacement.command)),
        ));
    }

    let Some(contract) = entity::entity_artifact_v1_contract_for_version(version) else {
        return Err(refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            format!("Unknown entity artifact version '{}'", version),
            serde_json::json!({
                "reason": "unknown_entity_artifact_version",
                "path": path.display().to_string(),
                "artifact": label,
                "actual_version": version,
                "expected_stage": expected_stage.as_str(),
                "expected_version": entity_runtime::entity_v1_contract_for_stage(expected_stage).artifact_version,
                "writes_performed": false
            }),
            Some(format!("{} --help", expected_stage.command())),
        ));
    };

    if contract.stage != expected_stage {
        return Err(refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            format!(
                "{} '{}' has stage '{}' but '{}' is required",
                label,
                path.display(),
                contract.stage.as_str(),
                expected_stage.as_str()
            ),
            serde_json::json!({
                "reason": "wrong_entity_artifact_stage",
                "path": path.display().to_string(),
                "artifact": label,
                "actual_stage": contract.stage.as_str(),
                "actual_version": version,
                "expected_stage": expected_stage.as_str(),
                "expected_version": entity_runtime::entity_v1_contract_for_stage(expected_stage).artifact_version,
                "writes_performed": false
            }),
            Some(format!("{} --help", expected_stage.command())),
        ));
    }

    entity::schema::validate_artifact_v1_core_contract(&artifact).map_err(|refusal| {
        entity_v1_input_preflight_refusal(
            path,
            label,
            expected_stage,
            "invalid_v1_core_contract",
            refusal,
        )
    })?;
    entity::schema::validate_entity_v1_self_hash(&artifact).map_err(|refusal| {
        entity_v1_input_preflight_refusal(
            path,
            label,
            expected_stage,
            "invalid_v1_self_hash",
            refusal,
        )
    })?;

    Ok(())
}

fn entity_v1_input_preflight_refusal(
    path: &Path,
    label: &str,
    expected_stage: entity::EntityArtifactStageV1,
    reason: &str,
    refusal: Refusal,
) -> CanonOutput {
    let Refusal {
        code,
        message,
        detail: source_detail,
        next_command: _,
    } = refusal;
    let mut detail = match source_detail {
        serde_json::Value::Object(map) => map,
        other => serde_json::Map::from_iter([("source_detail".to_string(), other)]),
    };
    detail.insert(
        "reason".to_string(),
        serde_json::Value::String(reason.to_string()),
    );
    detail.insert(
        "path".to_string(),
        serde_json::Value::String(path.display().to_string()),
    );
    detail.insert(
        "artifact".to_string(),
        serde_json::Value::String(label.to_string()),
    );
    detail.insert(
        "expected_stage".to_string(),
        serde_json::Value::String(expected_stage.as_str().to_string()),
    );
    detail.insert(
        "expected_version".to_string(),
        serde_json::Value::String(
            entity_runtime::entity_v1_contract_for_stage(expected_stage)
                .artifact_version
                .to_string(),
        ),
    );
    detail.insert(
        "writes_performed".to_string(),
        serde_json::Value::Bool(false),
    );
    refusal::create_refusal(
        code,
        message,
        serde_json::Value::Object(detail),
        Some(format!("{} --help", expected_stage.command())),
    )
}

#[allow(clippy::result_large_err)]
fn run_entity_native_run_audit(
    audit: &EntityAuditCli,
    result_bytes: &[u8],
) -> Result<entity::audit::EntityAuditArtifact, CanonOutput> {
    let run: entity::run::EntityRunArtifact = deserialize_native_entity_artifact(
        result_bytes,
        &audit.result,
        "entity result artifact",
        "audit",
        entity::CANON_ENTITY_RUN_VERSION,
    )?;
    validate_native_run_artifact_contract(&run, &audit.result, "audit")?;
    let mut certified_artifacts = run
        .stage_artifacts
        .iter()
        .map(|artifact| entity::EntityArtifactReference {
            version: artifact.version.clone(),
            content_hash: artifact.artifact_content_hash.clone(),
        })
        .collect::<Vec<_>>();
    certified_artifacts.push(entity::EntityArtifactReference {
        version: run.version.clone(),
        content_hash: run.artifact_content_hash.clone(),
    });
    run_entity_native_audit_for_header(
        audit,
        entity::EntityArtifactHeader {
            version: run.version,
            metadata: run.metadata,
            summary: run.summary,
        },
        certified_artifacts,
    )
}

#[allow(clippy::result_large_err)]
fn run_entity_native_solve_audit(
    audit: &EntityAuditCli,
    result_bytes: &[u8],
) -> Result<entity::audit::EntityAuditArtifact, CanonOutput> {
    let solve: entity::solve::SolveArtifact = deserialize_native_entity_artifact(
        result_bytes,
        &audit.result,
        "entity result artifact",
        "audit",
        entity::CANON_ENTITY_SOLVE_VERSION,
    )?;
    entity::solve::validate_solve_artifact_contract(&solve)
        .map_err(|refusal| refusal.to_canon_output())?;
    let mut certified_artifacts = solve.upstream_artifacts.clone();
    certified_artifacts.push(entity::EntityArtifactReference {
        version: solve.version.clone(),
        content_hash: solve.artifact_content_hash.clone(),
    });
    run_entity_native_audit_for_header(
        audit,
        entity::EntityArtifactHeader {
            version: solve.version,
            metadata: solve.metadata,
            summary: solve.summary,
        },
        certified_artifacts,
    )
}

#[allow(clippy::result_large_err)]
fn run_entity_native_audit_for_header(
    audit: &EntityAuditCli,
    result: entity::EntityArtifactHeader,
    certified_artifacts: Vec<entity::EntityArtifactReference>,
) -> Result<entity::audit::EntityAuditArtifact, CanonOutput> {
    let suite = load_entity_audit_suite(&audit.suite, "audit")?;
    let expected = entity::artifact_chain::EntityArtifactChainExpectation::from_link(
        entity::artifact_chain::EntityChainStage::Audit,
        &entity::artifact_chain::EntityArtifactChainLink::from_header(&result),
    );
    entity::audit::run_entity_audit(entity::audit::EntityAuditRequest {
        result,
        expected,
        certified_artifacts,
        suite,
    })
    .map_err(|refusal| refusal.to_canon_output())
}

fn render_entity_native_audit_summary(audit: &entity::audit::EntityAuditArtifact) -> String {
    let status = audit
        .summary
        .labels
        .get("status")
        .map(String::as_str)
        .unwrap_or("passed");
    format!(
        "{} audit suite={} gates={} status={}",
        audit.audited_artifact.version,
        audit.suite_id,
        audit.gates.len(),
        status
    )
}

#[allow(clippy::result_large_err)]
fn run_entity_native_review_artifact_export_command(
    export: &EntityReviewExportCli,
    result_probe: &serde_json::Value,
    result_bytes: &[u8],
) -> Result<u8, Box<dyn Error>> {
    let artifact = if entity_artifact_value_looks_like_native_solve_v0(result_probe) {
        run_entity_native_solve_review_artifact_export(export, result_bytes)
    } else if entity_artifact_value_looks_like_native_run_v0(result_probe) {
        run_entity_native_run_review_artifact_export(export, result_bytes)
    } else if entity_artifact_value_looks_like_native_link_v1(result_probe) {
        run_entity_native_link_review_artifact_export(export, result_probe, result_bytes)
    } else {
        Err(native_entity_artifact_contract_refusal(
            "Native review export requires a native solve, run, or link artifact",
            serde_json::json!({
                "stage": "native_review_export",
                "field": "version",
                "expected": [
                    entity::CANON_ENTITY_SOLVE_VERSION,
                    entity::CANON_ENTITY_RUN_VERSION,
                    entity::run::link::ENTITY_LINK_VERSION
                ],
                "actual": entity_artifact_version(result_probe).unwrap_or("<missing>"),
                "writes_performed": false
            }),
        ))
    };

    match artifact
        .and_then(|artifact| render_entity_native_review_artifact_export(export, &artifact))
    {
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

#[allow(clippy::result_large_err)]
fn run_entity_native_solve_review_artifact_export(
    export: &EntityReviewExportCli,
    result_bytes: &[u8],
) -> Result<entity::review_export::NativeReviewArtifact, CanonOutput> {
    let solve: entity::solve::SolveArtifact = deserialize_native_entity_artifact(
        result_bytes,
        &export.result,
        "entity result artifact",
        "native_review_export",
        entity::CANON_ENTITY_SOLVE_VERSION,
    )?;
    let review_queue = build_entity_native_review_queue(export, solve)?;
    let source_execution_hash = review_queue.source_solve_hash.clone();
    build_entity_native_review_artifact(review_queue, source_execution_hash)
}

#[allow(clippy::result_large_err)]
fn run_entity_native_run_review_artifact_export(
    export: &EntityReviewExportCli,
    result_bytes: &[u8],
) -> Result<entity::review_export::NativeReviewArtifact, CanonOutput> {
    let run: entity::run::EntityRunArtifact = deserialize_native_entity_artifact(
        result_bytes,
        &export.result,
        "entity result artifact",
        "native_review_export",
        entity::CANON_ENTITY_RUN_VERSION,
    )?;
    validate_native_run_artifact_contract(&run, &export.result, "native_review_export")?;
    let source_execution_hash = run.artifact_content_hash.clone();
    let solve_path = resolve_native_run_solve_artifact_path(&export.result, &run)?;
    let solve = read_hash_bound_native_solve_artifact(&solve_path, &run)?;
    let review_queue = build_entity_native_review_queue(export, solve)?;
    build_entity_native_review_artifact(review_queue, source_execution_hash)
}

#[allow(clippy::result_large_err)]
fn run_entity_native_link_review_artifact_export(
    export: &EntityReviewExportCli,
    result_probe: &serde_json::Value,
    result_bytes: &[u8],
) -> Result<entity::review_export::NativeReviewArtifact, CanonOutput> {
    entity::run::link::validate_entity_link_artifact_raw_shape(result_probe)
        .map_err(|refusal| refusal.to_canon_output())?;
    let link: entity::run::link::EntityLinkArtifact = deserialize_native_entity_artifact(
        result_bytes,
        &export.result,
        "entity link artifact",
        "native_review_export",
        entity::run::link::ENTITY_LINK_VERSION,
    )?;
    entity::run::link::validate_entity_link_artifact_at_path(&link, &export.result)
        .map_err(|refusal| refusal.to_canon_output())?;
    validate_entity_link_review_export_decision_derivation(
        &export.result,
        &link,
        "native_review_export",
    )?;
    let source_execution_hash = link.shared_run_artifact.content_hash.clone();
    let review_queue =
        entity::review::build_link_review_queue_artifact(entity::review::LinkReviewQueueRequest {
            link_artifact: link,
            include: map_entity_review_include_v1(&export.include),
        })
        .map_err(|refusal| refusal.to_canon_output())?;
    build_entity_native_review_artifact(review_queue, source_execution_hash)
}

#[allow(clippy::result_large_err)]
fn build_entity_native_review_artifact(
    review_queue: entity::review::ReviewQueueArtifact,
    source_execution_hash: String,
) -> Result<entity::review_export::NativeReviewArtifact, CanonOutput> {
    let policy_content_hash = review_queue.metadata.strategy.content_hash.clone();
    entity::review_export::build_native_review_artifact(
        entity::review_export::NativeReviewExportRequest {
            review_queue,
            run_content_hash: source_execution_hash,
            policy_content_hash,
        },
    )
    .map_err(|refusal| refusal.to_canon_output())
}

#[allow(clippy::result_large_err)]
fn render_entity_native_review_artifact_export(
    export: &EntityReviewExportCli,
    artifact: &entity::review_export::NativeReviewArtifact,
) -> Result<String, CanonOutput> {
    match export.emit {
        EntityReviewExportEmitMode::Json => {
            entity::review_export::render_native_review_json(artifact)
        }
        EntityReviewExportEmitMode::Csv => {
            entity::review_export::render_native_review_csv(artifact)
        }
        EntityReviewExportEmitMode::Html => {
            entity::review_export::render_native_review_html(artifact)
        }
    }
    .map_err(|refusal| refusal.to_canon_output())
}

#[allow(clippy::result_large_err)]
fn run_entity_native_solve_review_export(
    export: &EntityReviewExportCli,
    result_bytes: &[u8],
) -> Result<entity::review::ReviewQueueArtifact, CanonOutput> {
    let solve: entity::solve::SolveArtifact = deserialize_native_entity_artifact(
        result_bytes,
        &export.result,
        "entity result artifact",
        "review_export",
        entity::CANON_ENTITY_SOLVE_VERSION,
    )?;
    build_entity_native_review_queue(export, solve)
}

#[allow(clippy::result_large_err)]
fn run_entity_native_run_review_export(
    export: &EntityReviewExportCli,
    result_bytes: &[u8],
) -> Result<entity::review::ReviewQueueArtifact, CanonOutput> {
    let run: entity::run::EntityRunArtifact = deserialize_native_entity_artifact(
        result_bytes,
        &export.result,
        "entity result artifact",
        "review_export",
        entity::CANON_ENTITY_RUN_VERSION,
    )?;
    validate_native_run_artifact_contract(&run, &export.result, "review_export")?;
    let solve_path = resolve_native_run_solve_artifact_path(&export.result, &run)?;
    let solve = read_hash_bound_native_solve_artifact(&solve_path, &run)?;
    build_entity_native_review_queue(export, solve)
}

#[allow(clippy::result_large_err)]
fn run_entity_native_link_review_export(
    export: &EntityReviewExportCli,
    result_probe: &serde_json::Value,
    result_bytes: &[u8],
) -> Result<entity::review::ReviewQueueArtifact, CanonOutput> {
    entity::run::link::validate_entity_link_artifact_raw_shape(result_probe)
        .map_err(|refusal| refusal.to_canon_output())?;
    let link: entity::run::link::EntityLinkArtifact = deserialize_native_entity_artifact(
        result_bytes,
        &export.result,
        "entity link artifact",
        "review_export",
        entity::run::link::ENTITY_LINK_VERSION,
    )?;
    entity::run::link::validate_entity_link_artifact_at_path(&link, &export.result)
        .map_err(|refusal| refusal.to_canon_output())?;
    validate_entity_link_review_export_decision_derivation(&export.result, &link, "review_export")?;
    entity::review::build_link_review_queue_artifact(entity::review::LinkReviewQueueRequest {
        link_artifact: link,
        include: map_entity_review_include_v1(&export.include),
    })
    .map_err(|refusal| refusal.to_canon_output())
}

fn validate_entity_link_review_export_decision_derivation(
    link_path: &Path,
    link: &entity::run::link::EntityLinkArtifact,
    stage: &str,
) -> Result<(), CanonOutput> {
    let work_dir = entity_link_review_export_work_dir(link_path, stage)?;
    let run = read_hash_bound_entity_link_review_run_artifact(&work_dir, link, stage)?;
    let strategy =
        load_entity_link_review_export_strategy(&run, link, link_path, &work_dir, stage)?;
    let solve = load_entity_link_v1_solve_artifact(&work_dir, &run)?;
    validate_entity_link_review_export_solve_binding(link, &solve, stage)?;
    let bindings =
        entity::run::link::read_derivation_validated_entity_link_observation_surface_bindings_at_path(
            link, link_path, &run,
        )
        .map_err(|refusal| refusal.to_canon_output())?;
    let decision_bindings = bindings
        .into_iter()
        .map(|binding| EntityLinkDecisionBinding {
            side: binding.side,
            link_id: binding.link_id,
            surface_id: binding.surface_id,
        })
        .collect::<Vec<_>>();
    let derived = entity_link_decision_records_from_solve(&solve, &decision_bindings, &strategy)?;
    validate_entity_link_review_export_decision_parity(link, &derived, stage)?;
    Ok(())
}

fn entity_link_review_export_work_dir(
    link_path: &Path,
    stage: &str,
) -> Result<PathBuf, CanonOutput> {
    let link_dir = link_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            native_entity_artifact_contract_refusal(
                "Entity link review export requires a link artifact inside a link work directory",
                serde_json::json!({
                    "stage": stage,
                    "field": "path",
                    "path": link_path.display().to_string(),
                    "expected": "<WORK_DIR>/link/<LINK_ARTIFACT.json>",
                    "writes_performed": false
                }),
            )
        })?;
    let work_dir = link_dir
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            native_entity_artifact_contract_refusal(
                "Entity link review export could not locate the bound run work directory",
                serde_json::json!({
                    "stage": stage,
                    "field": "path",
                    "path": link_path.display().to_string(),
                    "expected": "<WORK_DIR>/link/<LINK_ARTIFACT.json>",
                    "writes_performed": false
                }),
            )
        })?;
    Ok(work_dir.to_path_buf())
}

fn read_hash_bound_entity_link_review_run_artifact(
    work_dir: &Path,
    link: &entity::run::link::EntityLinkArtifact,
    stage: &str,
) -> Result<entity::run::EntityRunArtifact, CanonOutput> {
    let run_path = work_dir.join("run/run.json");
    let (run_value, _bytes) =
        read_entity_lifecycle_json_artifact(&run_path, "entity run artifact")?;
    entity::schema::validate_artifact_v1_core_contract(&run_value)
        .map_err(|refusal| refusal.to_canon_output())?;
    entity::schema::validate_entity_v1_self_hash(&run_value)
        .map_err(|refusal| refusal.to_canon_output())?;
    let run_hash = entity_link_value_string(
        &run_value,
        &["artifact_content_hash"],
        "run.artifact_content_hash",
    )?;
    let run =
        serde_json::from_value::<entity::run::EntityRunArtifact>(run_value).map_err(|error| {
            native_entity_artifact_contract_refusal(
                "Entity link review export failed to deserialize the bound run artifact",
                serde_json::json!({
                    "stage": stage,
                    "path": run_path.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
    if run.version.as_str() != entity::CANON_ENTITY_RUN_VERSION_V1 {
        return Err(native_entity_artifact_contract_refusal(
            "Entity link review export loaded the wrong run artifact version",
            serde_json::json!({
                "stage": stage,
                "field": "shared_run_artifact.version",
                "path": run_path.display().to_string(),
                "expected": entity::CANON_ENTITY_RUN_VERSION_V1,
                "actual": run.version.as_str(),
                "writes_performed": false
            }),
        ));
    }
    if link.shared_run_artifact.version.as_str() != run.version.as_str()
        || link.shared_run_artifact.content_hash.as_str() != run_hash.as_str()
        || run.artifact_content_hash.as_str() != run_hash.as_str()
    {
        return Err(native_entity_artifact_contract_refusal(
            "Entity link review export run artifact does not match the link binding",
            serde_json::json!({
                "stage": stage,
                "field": "shared_run_artifact",
                "path": run_path.display().to_string(),
                "expected": {
                    "version": link.shared_run_artifact.version.as_str(),
                    "content_hash": link.shared_run_artifact.content_hash.as_str()
                },
                "actual": {
                    "version": run.version.as_str(),
                    "content_hash": run_hash.as_str()
                },
                "writes_performed": false
            }),
        ));
    }
    Ok(run)
}

fn validate_entity_link_review_export_solve_binding(
    link: &entity::run::link::EntityLinkArtifact,
    solve: &entity::solve::SolveArtifact,
    stage: &str,
) -> Result<(), CanonOutput> {
    if link.shared_solve_artifact.version.as_str() != solve.version.as_str()
        || link.shared_solve_artifact.content_hash.as_str() != solve.artifact_content_hash.as_str()
    {
        return Err(native_entity_artifact_contract_refusal(
            "Entity link review export solve artifact does not match the link binding",
            serde_json::json!({
                "stage": stage,
                "field": "shared_solve_artifact",
                "expected": {
                    "version": link.shared_solve_artifact.version.as_str(),
                    "content_hash": link.shared_solve_artifact.content_hash.as_str()
                },
                "actual": {
                    "version": solve.version.as_str(),
                    "content_hash": solve.artifact_content_hash.as_str()
                },
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn load_entity_link_review_export_strategy(
    run: &entity::run::EntityRunArtifact,
    link: &entity::run::link::EntityLinkArtifact,
    link_path: &Path,
    work_dir: &Path,
    stage: &str,
) -> Result<resolve::ResolveStrategy, CanonOutput> {
    let strategy_source = run
        .summary
        .labels
        .get("strategy_source")
        .filter(|source| !source.trim().is_empty())
        .ok_or_else(|| {
            native_entity_artifact_contract_refusal(
                "Entity link review export run artifact does not record a strategy source",
                serde_json::json!({
                    "stage": stage,
                    "field": "run.summary.labels.strategy_source",
                    "writes_performed": false
                }),
            )
        })?;
    let strategy = load_entity_link_review_export_strategy_from_context(
        strategy_source,
        link_path,
        work_dir,
        &run.metadata.strategy.content_hash,
        stage,
    )?;
    if strategy.id.as_str() != run.metadata.strategy.id.as_str()
        || strategy.version.as_str() != run.metadata.strategy.version.as_str()
        || strategy.content_hash.as_str() != run.metadata.strategy.content_hash.as_str()
    {
        return Err(native_entity_artifact_contract_refusal(
            "Entity link review export strategy source does not match run metadata",
            serde_json::json!({
                "stage": stage,
                "field": "run.metadata.strategy",
                "source": strategy_source,
                "expected": {
                    "id": run.metadata.strategy.id.as_str(),
                    "version": run.metadata.strategy.version.as_str(),
                    "content_hash": run.metadata.strategy.content_hash.as_str()
                },
                "actual": {
                    "id": strategy.id.as_str(),
                    "version": strategy.version.as_str(),
                    "content_hash": strategy.content_hash.as_str()
                },
                "writes_performed": false
            }),
        ));
    }
    if strategy.id.as_str() != link.decision_artifact.strategy.id.as_str()
        || strategy.version.as_str() != link.decision_artifact.strategy.version.as_str()
        || strategy.content_hash.as_str() != link.decision_artifact.strategy.content_hash.as_str()
    {
        return Err(native_entity_artifact_contract_refusal(
            "Entity link review export strategy source does not match link decisions",
            serde_json::json!({
                "stage": stage,
                "field": "decision_artifact.strategy",
                "source": strategy_source,
                "expected": {
                    "id": link.decision_artifact.strategy.id.as_str(),
                    "version": link.decision_artifact.strategy.version.as_str(),
                    "content_hash": link.decision_artifact.strategy.content_hash.as_str()
                },
                "actual": {
                    "id": strategy.id.as_str(),
                    "version": strategy.version.as_str(),
                    "content_hash": strategy.content_hash.as_str()
                },
                "writes_performed": false
            }),
        ));
    }
    Ok(strategy)
}

fn load_entity_link_review_export_strategy_from_context(
    strategy_source: &str,
    link_path: &Path,
    work_dir: &Path,
    expected_content_hash: &str,
    stage: &str,
) -> Result<resolve::ResolveStrategy, CanonOutput> {
    let source = Path::new(strategy_source);
    if source.is_absolute() {
        return resolve::load_strategy(source).map_err(create_resolve_refusal);
    }

    let candidates = entity_link_review_export_strategy_candidates(source, link_path, work_dir);
    let candidate_labels = candidates
        .iter()
        .map(|candidate| candidate.display().to_string())
        .collect::<Vec<_>>();
    let mut matching = Vec::new();
    let mut mismatches = Vec::new();
    let mut load_failures = Vec::new();

    for candidate in &candidates {
        let bytes = match fs::read(candidate) {
            Ok(bytes) => bytes,
            Err(error) => {
                load_failures.push(serde_json::json!({
                    "resolved_source": candidate.display().to_string(),
                    "error": error.to_string()
                }));
                continue;
            }
        };
        let actual_hash = witness::hash_bytes(&bytes);
        if actual_hash.as_str() != expected_content_hash {
            mismatches.push(serde_json::json!({
                "resolved_source": candidate.display().to_string(),
                "actual": actual_hash
            }));
            continue;
        }
        let strategy = resolve::parse_strategy_bytes(&bytes).map_err(|error| {
            native_entity_artifact_contract_refusal(
                "Entity link review export failed to parse the hash-bound strategy source",
                serde_json::json!({
                    "stage": stage,
                    "field": "run.summary.labels.strategy_source",
                    "source": strategy_source,
                    "resolved_source": candidate.display().to_string(),
                    "error_code": format!("{:?}", error.code),
                    "error": error.message,
                    "writes_performed": false
                }),
            )
        })?;
        matching.push((candidate.display().to_string(), strategy));
    }

    match matching.len() {
        1 => Ok(matching.remove(0).1),
        count if count > 1 => Err(native_entity_artifact_contract_refusal(
            "Entity link review export found multiple hash-bound strategy sources in artifact context",
            serde_json::json!({
                "stage": stage,
                "field": "run.summary.labels.strategy_source",
                "source": strategy_source,
                "expected": expected_content_hash,
                "attempted_sources": candidate_labels,
                "matching_sources": matching
                    .iter()
                    .map(|(source, _)| source.clone())
                    .collect::<Vec<_>>(),
                "writes_performed": false
            }),
        )),
        _ if !mismatches.is_empty() => Err(native_entity_artifact_contract_refusal(
            "Entity link review export strategy hash does not match artifact context source",
            serde_json::json!({
                "stage": stage,
                "field": "run.metadata.strategy.content_hash",
                "source": strategy_source,
                "expected": expected_content_hash,
                "attempted_sources": candidate_labels,
                "mismatches": mismatches,
                "load_failures": load_failures,
                "writes_performed": false
            }),
        )),
        _ => Err(native_entity_artifact_contract_refusal(
            "Entity link review export could not read a strategy source from artifact context",
            serde_json::json!({
                "stage": stage,
                "field": "run.summary.labels.strategy_source",
                "source": strategy_source,
                "attempted_sources": candidate_labels,
                "load_failures": load_failures,
                "writes_performed": false
            }),
        )),
    }
}

fn entity_link_review_export_strategy_candidates(
    strategy_source: &Path,
    link_path: &Path,
    work_dir: &Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for dir in entity_link_review_export_strategy_context_dirs(link_path, work_dir) {
        let candidate = dir.join(strategy_source);
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates
}

fn entity_link_review_export_strategy_context_dirs(
    link_path: &Path,
    work_dir: &Path,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(link_dir) = link_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        push_unique_path(&mut dirs, link_dir);
    }
    push_unique_path(&mut dirs, work_dir);
    let mut current = work_dir
        .parent()
        .filter(|path| !path.as_os_str().is_empty());
    while let Some(dir) = current {
        push_unique_path(&mut dirs, dir);
        current = dir.parent().filter(|parent| !parent.as_os_str().is_empty());
    }
    dirs
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: &Path) {
    let candidate = path.to_path_buf();
    if !paths.contains(&candidate) {
        paths.push(candidate);
    }
}

fn validate_entity_link_review_export_decision_parity(
    link: &entity::run::link::EntityLinkArtifact,
    derived: &resolve::MatchDecisions,
    stage: &str,
) -> Result<(), CanonOutput> {
    let actual = &link.decision_artifact;
    if actual.matches != derived.matches {
        return Err(entity_link_review_export_decision_mismatch_refusal(
            stage,
            "decision_artifact.matches",
            actual.matches.len(),
            derived.matches.len(),
        ));
    }
    if actual.unmatched != derived.unmatched {
        return Err(entity_link_review_export_decision_mismatch_refusal(
            stage,
            "decision_artifact.unmatched",
            actual.unmatched.len(),
            derived.unmatched.len(),
        ));
    }
    if actual.ambiguous != derived.ambiguous {
        return Err(entity_link_review_export_decision_mismatch_refusal(
            stage,
            "decision_artifact.ambiguous",
            actual.ambiguous.len(),
            derived.ambiguous.len(),
        ));
    }
    if actual.conflict_warnings != derived.conflict_warnings {
        return Err(entity_link_review_export_decision_mismatch_refusal(
            stage,
            "decision_artifact.conflict_warnings",
            actual.conflict_warnings.len(),
            derived.conflict_warnings.len(),
        ));
    }
    let expected_summary = resolve::build_summary(
        link.target.row_count as usize,
        &derived.matches,
        &derived.unmatched,
        &derived.ambiguous,
    );
    let expected_summary =
        canonicalize_entity_link_review_export_expected_summary(&expected_summary, stage)?;
    if actual.summary != expected_summary || link.summary != expected_summary {
        return Err(native_entity_artifact_contract_refusal(
            "Entity link review export decision summary does not match deterministic derivation",
            serde_json::json!({
                "stage": stage,
                "field": "decision_artifact.summary",
                "actual": actual.summary,
                "expected": expected_summary,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn canonicalize_entity_link_review_export_expected_summary(
    summary: &resolve::ResolveSummary,
    stage: &str,
) -> Result<resolve::ResolveSummary, CanonOutput> {
    let bytes = serde_json::to_vec(summary).map_err(|error| {
        native_entity_artifact_contract_refusal(
            "Entity link review export failed to canonicalize derived decision summary",
            serde_json::json!({
                "stage": stage,
                "field": "decision_artifact.summary",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        native_entity_artifact_contract_refusal(
            "Entity link review export failed to canonicalize derived decision summary",
            serde_json::json!({
                "stage": stage,
                "field": "decision_artifact.summary",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })
}

fn entity_link_review_export_decision_mismatch_refusal(
    stage: &str,
    field: &str,
    actual_count: usize,
    expected_count: usize,
) -> CanonOutput {
    native_entity_artifact_contract_refusal(
        "Entity link review export decisions do not match deterministic derivation",
        serde_json::json!({
            "stage": stage,
            "field": field,
            "actual_count": actual_count,
            "expected_count": expected_count,
            "writes_performed": false
        }),
    )
}

#[allow(clippy::result_large_err)]
fn build_entity_native_review_queue(
    export: &EntityReviewExportCli,
    solve: entity::solve::SolveArtifact,
) -> Result<entity::review::ReviewQueueArtifact, CanonOutput> {
    entity::review::build_review_queue_artifact(entity::review::ReviewQueueRequest {
        solve_artifact: solve,
        include: map_entity_review_include_v1(&export.include),
        provenance_samples: Vec::new(),
        relation_hints: Vec::new(),
    })
    .map_err(|refusal| refusal.to_canon_output())
}

#[allow(clippy::result_large_err)]
fn render_entity_native_review_export(
    export: &EntityReviewExportCli,
    artifact: &entity::review::ReviewQueueArtifact,
) -> Result<String, CanonOutput> {
    match export.emit {
        EntityReviewExportEmitMode::Json => serde_json::to_string(artifact).map_err(|error| {
            native_entity_artifact_contract_refusal(
                "Failed to serialize native entity review export",
                serde_json::json!({
                    "stage": "review_export",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        }),
        EntityReviewExportEmitMode::Csv => entity::review::render_review_queue_csv(artifact)
            .map_err(|refusal| refusal.to_canon_output()),
        EntityReviewExportEmitMode::Html => Err(entity_review_export_html_refusal()),
    }
}

#[allow(clippy::result_large_err)]
fn read_hash_bound_native_solve_artifact(
    solve_path: &Path,
    run: &entity::run::EntityRunArtifact,
) -> Result<entity::solve::SolveArtifact, CanonOutput> {
    let (solve_probe, bytes) =
        read_entity_lifecycle_json_artifact(solve_path, "entity solve artifact")?;
    if !entity_artifact_value_looks_like_native_solve_v0(&solve_probe) {
        return Err(native_entity_artifact_contract_refusal(
            "Run artifact solve handoff did not resolve to a native solve artifact",
            serde_json::json!({
                "stage": "review_export",
                "path": solve_path.display().to_string(),
                "artifact": "entity solve artifact",
                "expected_version": entity::CANON_ENTITY_SOLVE_VERSION,
                "actual_version": entity_artifact_version(&solve_probe).unwrap_or("<missing>"),
                "writes_performed": false
            }),
        ));
    }
    let solve: entity::solve::SolveArtifact = deserialize_native_entity_artifact(
        &bytes,
        solve_path,
        "entity solve artifact",
        "review_export",
        entity::CANON_ENTITY_SOLVE_VERSION,
    )?;
    entity::solve::validate_solve_artifact_contract(&solve)
        .map_err(|refusal| refusal.to_canon_output())?;
    let expected = run
        .stage_artifacts
        .iter()
        .find(|artifact| {
            artifact.stage == "solve" && artifact.version == entity::CANON_ENTITY_SOLVE_VERSION
        })
        .ok_or_else(|| {
            native_entity_artifact_contract_refusal(
                "Run artifact is missing its native solve stage reference",
                serde_json::json!({
                    "stage": "review_export",
                    "field": "stage_artifacts.solve",
                    "path": solve_path.display().to_string(),
                    "writes_performed": false
                }),
            )
        })?;
    if expected.artifact_content_hash != solve.artifact_content_hash {
        return Err(native_entity_artifact_contract_refusal(
            "Run artifact solve handoff hash does not match the loaded solve artifact",
            serde_json::json!({
                "stage": "review_export",
                "field": "stage_artifacts.solve.artifact_content_hash",
                "path": solve_path.display().to_string(),
                "expected": expected.artifact_content_hash.as_str(),
                "actual": solve.artifact_content_hash.as_str(),
                "writes_performed": false
            }),
        ));
    }
    Ok(solve)
}

#[allow(clippy::result_large_err)]
fn resolve_native_run_solve_artifact_path(
    run_path: &Path,
    run: &entity::run::EntityRunArtifact,
) -> Result<PathBuf, CanonOutput> {
    let raw_path = run.work_dir.solve_artifact_path.trim();
    let relative_path = Path::new(raw_path);
    if raw_path.is_empty()
        || relative_path.is_absolute()
        || !relative_path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(native_entity_artifact_contract_refusal(
            "Run artifact solve handoff path must be a safe relative path",
            serde_json::json!({
                "stage": "review_export",
                "field": "work_dir.solve_artifact_path",
                "path": raw_path,
                "expected": "non_empty_relative_path_with_normal_components",
                "writes_performed": false
            }),
        ));
    }
    let base_dir = run_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(base_dir.join(relative_path))
}

#[allow(clippy::result_large_err)]
fn deserialize_native_entity_artifact<T: DeserializeOwned>(
    bytes: &[u8],
    path: &Path,
    label: &str,
    stage: &str,
    expected_version: &str,
) -> Result<T, CanonOutput> {
    serde_json::from_slice(bytes).map_err(|error| {
        native_entity_artifact_contract_refusal(
            format!("Malformed native {} '{}': {}", label, path.display(), error),
            serde_json::json!({
                "stage": stage,
                "path": path.display().to_string(),
                "artifact": label,
                "expected_version": expected_version,
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })
}

#[allow(clippy::result_large_err)]
fn validate_native_run_artifact_contract(
    run: &entity::run::EntityRunArtifact,
    path: &Path,
    stage: &str,
) -> Result<(), CanonOutput> {
    if run.version != entity::CANON_ENTITY_RUN_VERSION {
        return Err(native_entity_artifact_contract_refusal(
            "Run artifact has the wrong native contract version",
            serde_json::json!({
                "stage": stage,
                "path": path.display().to_string(),
                "field": "version",
                "expected": entity::CANON_ENTITY_RUN_VERSION,
                "actual": run.version.as_str(),
                "writes_performed": false
            }),
        ));
    }
    if run.artifact_content_hash.trim().is_empty() {
        return Err(native_entity_artifact_contract_refusal(
            "Run artifact must carry a content hash",
            serde_json::json!({
                "stage": stage,
                "path": path.display().to_string(),
                "field": "artifact_content_hash",
                "expected": "non_empty_hash",
                "actual": run.artifact_content_hash.as_str(),
                "writes_performed": false
            }),
        ));
    }
    if run.metadata.artifact_content_hash != run.artifact_content_hash {
        return Err(native_entity_artifact_contract_refusal(
            "Run artifact metadata hash does not match artifact hash",
            serde_json::json!({
                "stage": stage,
                "path": path.display().to_string(),
                "field": "metadata.artifact_content_hash",
                "expected": run.artifact_content_hash.as_str(),
                "actual": run.metadata.artifact_content_hash.as_str(),
                "writes_performed": false
            }),
        ));
    }
    let expected = hash_native_run_artifact_without_self(run, path, stage)?;
    if run.artifact_content_hash != expected {
        return Err(native_entity_artifact_contract_refusal(
            "Run artifact content hash does not match its payload",
            serde_json::json!({
                "stage": stage,
                "path": path.display().to_string(),
                "field": "artifact_content_hash",
                "expected": expected,
                "actual": run.artifact_content_hash.as_str(),
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
fn hash_native_run_artifact_without_self(
    run: &entity::run::EntityRunArtifact,
    path: &Path,
    stage: &str,
) -> Result<String, CanonOutput> {
    let mut hashable = run.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        native_entity_artifact_contract_refusal(
            "Failed to hash native run artifact",
            serde_json::json!({
                "stage": stage,
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn entity_artifact_value_looks_like_native_run_v0(value: &serde_json::Value) -> bool {
    entity_artifact_version(value) == Some(entity::CANON_ENTITY_RUN_VERSION)
        && entity_artifact_has_metadata_object(value)
        && entity_artifact_has_any_marker(
            value,
            &[
                "stage_artifacts",
                "work_dir",
                "next_commands",
                "orchestration",
            ],
        )
}

fn entity_artifact_value_looks_like_native_solve_v0(value: &serde_json::Value) -> bool {
    entity_artifact_version(value) == Some(entity::CANON_ENTITY_SOLVE_VERSION)
        && entity_artifact_has_metadata_object(value)
        && entity_artifact_has_any_marker(
            value,
            &[
                "upstream_artifacts",
                "diagnostics",
                "decision_ledger_path",
                "review_groups",
            ],
        )
}

fn entity_artifact_value_looks_like_native_link_v1(value: &serde_json::Value) -> bool {
    entity_artifact_version(value) == Some(entity::run::link::ENTITY_LINK_VERSION)
}

fn entity_artifact_has_metadata_object(value: &serde_json::Value) -> bool {
    value
        .get("metadata")
        .is_some_and(serde_json::Value::is_object)
}

fn entity_artifact_has_any_marker(value: &serde_json::Value, markers: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|object| markers.iter().any(|marker| object.contains_key(*marker)))
}

fn entity_artifact_version(value: &serde_json::Value) -> Option<&str> {
    value.get("version").and_then(serde_json::Value::as_str)
}

fn native_entity_artifact_contract_refusal(
    message: impl Into<String>,
    detail: serde_json::Value,
) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EEntityArtifactContract,
        message.into(),
        detail,
        Some("Use the matching native entity artifact handoff and rerun the command".to_string()),
    )
}

fn entity_review_export_html_refusal() -> CanonOutput {
    native_entity_artifact_contract_refusal(
        "Entity review HTML export requires --artifact native-review",
        serde_json::json!({
            "stage": "review_export",
            "field": "emit",
            "actual": "html",
            "expected": "--artifact native-review",
            "writes_performed": false
        }),
    )
}

fn entity_review_export_group_by_signature_refusal() -> CanonOutput {
    native_entity_artifact_contract_refusal(
        "Entity review signature grouping requires --artifact native-review",
        serde_json::json!({
            "stage": "review_export",
            "field": "group_by",
            "actual": "signature",
            "expected": "--artifact native-review",
            "writes_performed": false
        }),
    )
}

fn native_review_import_refusal(
    message: impl Into<String>,
    detail: serde_json::Value,
) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EEntityReviewImport,
        message.into(),
        detail,
        Some(
            "Provide native review decisions plus --source-review <canon_entity_native_review.v0 JSON>"
                .to_string(),
        ),
    )
}

#[allow(clippy::result_large_err)]
fn run_entity_audit_pipeline(
    audit: &EntityAuditCli,
) -> Result<entity_runtime::AuditArtifact, CanonOutput> {
    let (result, result_bytes): (entity_runtime::SolveRunArtifact, Vec<u8>) =
        read_json_artifact(&audit.result, "entity result artifact")?;

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
        read_json_artifact(&promote.result, "entity result artifact")?;
    let (audit, audit_bytes): (entity_runtime::AuditArtifact, Vec<u8>) =
        read_json_artifact(&promote.audit, "entity audit artifact")?;

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
        read_json_artifact(&export.result, "entity result artifact")?;

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
    let review_bytes = read_artifact_bytes(&import.review, "entity review artifact")?;
    let audit_data = import
        .audit
        .as_ref()
        .map(|audit_path| {
            read_json_artifact::<entity_runtime::AuditArtifact>(audit_path, "entity audit artifact")
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

fn map_entity_review_include_v1(
    include: &EntityReviewInclude,
) -> entity::review::ReviewExportInclude {
    match include {
        EntityReviewInclude::Resolved => entity::review::ReviewExportInclude::Resolved,
        EntityReviewInclude::Escrow => entity::review::ReviewExportInclude::Escrow,
        EntityReviewInclude::Contradictions => entity::review::ReviewExportInclude::Contradictions,
        EntityReviewInclude::All => entity::review::ReviewExportInclude::All,
    }
}

fn entity_artifact_value_is_v1(value: &serde_json::Value) -> bool {
    value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|version| version.starts_with("canon_entity_") && version.ends_with(".v1"))
}

#[allow(clippy::result_large_err)]
fn validate_entity_apply_result_artifact(
    path: &Path,
    artifact: &serde_json::Value,
) -> Result<(), CanonOutput> {
    let Some(version) = artifact.get("version").and_then(serde_json::Value::as_str) else {
        return Err(refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            format!(
                "Entity apply result artifact '{}' is missing an entity artifact version",
                path.display()
            ),
            serde_json::json!({
                "reason": "missing_artifact_version",
                "path": path.display().to_string(),
                "artifact": "entity result artifact",
                "expected_stages": ["solve", "run"],
                "writes_performed": false
            }),
            Some("canon entity solve --help".to_string()),
        ));
    };

    if let Some(contract) = entity::entity_artifact_v1_contract_for_legacy_version(version) {
        return Err(refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            format!(
                "Entity apply refuses legacy result artifact version '{}'",
                version
            ),
            serde_json::json!({
                "reason": "legacy_entity_result_version",
                "path": path.display().to_string(),
                "artifact": "entity result artifact",
                "actual_version": version,
                "legacy_stage": contract.stage.as_str(),
                "legacy_versions": contract.legacy_versions,
                "expected_versions": [
                    entity::CANON_ENTITY_SOLVE_VERSION_V1,
                    entity::CANON_ENTITY_RUN_VERSION_V1
                ],
                "writes_performed": false
            }),
            Some("Re-run the entity pipeline to produce solve.v1 or run.v1".to_string()),
        ));
    }

    let contract = entity::entity_artifact_v1_contract_for_version(version).ok_or_else(|| {
        refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            format!("Unknown entity artifact version '{}'", version),
            serde_json::json!({
                "reason": "unknown_entity_artifact_version",
                "path": path.display().to_string(),
                "artifact": "entity result artifact",
                "actual_version": version,
                "expected_versions": [
                    entity::CANON_ENTITY_SOLVE_VERSION_V1,
                    entity::CANON_ENTITY_RUN_VERSION_V1
                ],
                "writes_performed": false
            }),
            Some(
                "Use a self-hashed canon_entity_solve.v1 or canon_entity_run.v1 artifact"
                    .to_string(),
            ),
        )
    })?;

    if matches!(
        contract.stage,
        entity::EntityArtifactStageV1::Solve | entity::EntityArtifactStageV1::Run
    ) {
        entity::schema::validate_entity_v1_self_hash(artifact)
            .map(|_| ())
            .map_err(|refusal| refusal.to_canon_output())
    } else {
        Err(refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            format!(
                "Entity apply result artifact '{}' has stage '{}' but solve or run is required",
                path.display(),
                contract.stage.as_str()
            ),
            serde_json::json!({
                "reason": "wrong_entity_artifact_stage",
                "path": path.display().to_string(),
                "artifact": "entity result artifact",
                "actual_stage": contract.stage.as_str(),
                "actual_version": version,
                "expected_versions": [
                    entity::CANON_ENTITY_SOLVE_VERSION_V1,
                    entity::CANON_ENTITY_RUN_VERSION_V1
                ],
                "writes_performed": false
            }),
            Some(
                "Use a self-hashed canon_entity_solve.v1 or canon_entity_run.v1 artifact"
                    .to_string(),
            ),
        ))
    }
}

fn entity_apply_lookup_column(
    apply: &EntityApplyCli,
    artifact: &serde_json::Value,
) -> Option<String> {
    if let Some(column) = apply
        .column
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(column.to_string());
    }

    let field_paths: &[&[&str]] = &[
        &["lookup_column"],
        &["column"],
        &["apply", "lookup_column"],
        &["apply", "column"],
        &["metadata", "input", "lookup_column"],
        &["metadata", "input", "column"],
        &["metadata", "apply", "lookup_column"],
        &["metadata", "apply", "column"],
    ];
    for path in field_paths {
        if let Some(value) = entity_json_string_at_path(artifact, path) {
            return Some(value);
        }
    }

    entity_apply_command_flag_value(artifact, "--column").or_else(|| {
        entity_json_string_at_path(artifact, &["metadata", "profile", "id"])
            .and_then(|profile| entity_apply_builtin_lookup_column(&profile).map(str::to_string))
    })
}

fn entity_apply_builtin_lookup_column(profile: &str) -> Option<&'static str> {
    match profile {
        "cmbs_tenant_label" => Some("raw_tenant_name"),
        "regab_firm_identity" => Some("org_name"),
        _ => None,
    }
}

fn entity_apply_output_path(apply: &EntityApplyCli, artifact: &serde_json::Value) -> PathBuf {
    if let Some(path) = apply.out.as_ref() {
        return path.clone();
    }

    if let Some(path) = entity_apply_command_flag_value(artifact, "--out")
        .or_else(|| entity_apply_command_flag_value(artifact, "--output"))
    {
        return PathBuf::from(path);
    }

    let default_output = entity::apply::default_apply_output_path(&apply.rows);
    let Some(work_dir) = apply.work_dir.as_ref() else {
        return default_output;
    };
    let file_name = default_output
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("apply.canon"));
    work_dir.join("apply").join(file_name)
}

fn entity_apply_require_full_resolution(apply: &EntityApplyCli) -> bool {
    apply.require_full_resolution || !apply.allow_partial_output
}

#[allow(clippy::result_large_err)]
fn entity_explain_v1_source_from_result(
    result_path: &Path,
    result_artifact: serde_json::Value,
) -> Result<entity::explain::EntityExplainV1Source, CanonOutput> {
    match entity_artifact_version(&result_artifact) {
        Some(entity::CANON_ENTITY_RUN_VERSION_V1) => {
            let solve_artifact =
                read_entity_lifecycle_bound_solve_artifact(result_path, &result_artifact)?;
            Ok(entity::explain::EntityExplainV1Source::Run {
                run_artifact: result_artifact,
                solve_artifact,
            })
        }
        _ => Ok(entity::explain::EntityExplainV1Source::Solve(
            result_artifact,
        )),
    }
}

fn entity_apply_v1_exit_code(artifact: &serde_json::Value) -> u8 {
    if entity_json_u64_at_path(artifact, &["summary", "counts", "unresolved"]).unwrap_or(0) > 0 {
        1
    } else {
        0
    }
}

fn render_entity_index_build_v1_summary(
    result: &entity::index::EntityIndexBuildV1Result,
) -> String {
    let artifact = &result.artifact;
    let profile = entity_json_string_at_path(artifact, &["metadata", "profile", "id"])
        .unwrap_or_else(|| "<profile>".to_string());
    let registry = entity_json_string_at_path(artifact, &["metadata", "registry_snapshot", "id"])
        .unwrap_or_else(|| "<registry>".to_string());
    let registry_version =
        entity_json_string_at_path(artifact, &["metadata", "registry_snapshot", "version"])
            .unwrap_or_else(|| "<version>".to_string());
    let surfaces =
        entity_json_u64_at_path(artifact, &["summary", "counts", "surface_count"]).unwrap_or(0);
    let tokens =
        entity_json_u64_at_path(artifact, &["summary", "counts", "token_count"]).unwrap_or(0);
    let ngrams =
        entity_json_u64_at_path(artifact, &["summary", "counts", "ngram_count"]).unwrap_or(0);
    format!(
        "{profile} index build v1 registry={registry}@{registry_version} surfaces={surfaces} tokens={tokens} ngrams={ngrams} cache={}",
        result.cache_status.as_str()
    )
}

fn render_jsonl_records<T: serde::Serialize>(records: &[T]) -> Result<String, Box<dyn Error>> {
    let mut rendered = String::new();
    for record in records {
        rendered.push_str(&serde_json::to_string(record)?);
        rendered.push('\n');
    }
    Ok(rendered)
}

fn read_entity_stage_artifact_json(path: &Path) -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn render_entity_block_stage_summary(output: &entity::block::EntityBlockStageOutput) -> String {
    format!(
        "{} candidate_pairs={} exact_buckets={} artifact={}",
        output.artifact.version,
        summary_count(&output.artifact.summary, "candidate_pairs"),
        summary_count(&output.artifact.summary, "exact_bucket_count"),
        output.artifact.artifact_content_hash
    )
}

fn render_entity_evidence_stage_summary(
    output: &entity::edge::EntityEvidenceStageOutput,
) -> String {
    format!(
        "{} evidence_records={} support_hits={} hard_cannot_link={} artifact={}",
        output.artifact.version,
        summary_count(&output.artifact.summary, "evidence_records"),
        summary_count(&output.artifact.summary, "support_hit_count"),
        summary_count(&output.artifact.summary, "hard_cannot_link_count"),
        output.artifact.artifact_content_hash
    )
}

fn render_entity_solve_stage_summary(output: &entity::solve::EntitySolveStageOutput) -> String {
    format!(
        "{} entities={} review_groups={} artifact={}",
        output.artifact.version,
        summary_count(&output.artifact.summary, "entity_count"),
        summary_count(&output.artifact.summary, "review_group_count"),
        output.artifact.artifact_content_hash
    )
}

fn summary_count(summary: &entity::EntityDeterministicSummary, key: &str) -> u64 {
    summary.counts.get(key).copied().unwrap_or_default()
}

fn render_entity_apply_v1_summary(artifact: &serde_json::Value) -> String {
    let registry = entity_json_string_at_path(artifact, &["registry", "id"])
        .unwrap_or_else(|| "<registry>".to_string());
    let registry_version = entity_json_string_at_path(artifact, &["registry", "version"])
        .unwrap_or_else(|| "<version>".to_string());
    let rows = entity_json_u64_at_path(artifact, &["summary", "counts", "rows"]).unwrap_or(0);
    let resolved =
        entity_json_u64_at_path(artifact, &["summary", "counts", "resolved"]).unwrap_or(0);
    let unresolved =
        entity_json_u64_at_path(artifact, &["summary", "counts", "unresolved"]).unwrap_or(0);
    let output_hash = entity_json_string_at_path(artifact, &["output_content_hash"])
        .unwrap_or_else(|| "<output_hash>".to_string());
    format!(
        "apply v1 registry={registry}@{registry_version} rows={rows} resolved={resolved} unresolved={unresolved} output_hash={output_hash}"
    )
}

fn entity_apply_command_flag_value(artifact: &serde_json::Value, flag: &str) -> Option<String> {
    entity_json_string_at_path(artifact, &["next_commands", "apply"])
        .and_then(|command| entity_cli_flag_value(&command, flag))
        .or_else(|| {
            entity_apply_handoff_command(artifact)
                .and_then(|command| entity_cli_flag_value(&command, flag))
        })
}

fn entity_apply_handoff_command(artifact: &serde_json::Value) -> Option<String> {
    let steps = artifact
        .get("orchestration")
        .and_then(|orchestration| orchestration.get("handoff_steps"))
        .or_else(|| artifact.get("handoff_steps"))
        .and_then(serde_json::Value::as_array)?;

    steps.iter().find_map(|step| {
        (step.get("stage").and_then(serde_json::Value::as_str) == Some("apply"))
            .then(|| step.get("command").and_then(serde_json::Value::as_str))
            .flatten()
            .and_then(entity_concrete_cli_value)
    })
}

fn entity_cli_flag_value(command: &str, flag: &str) -> Option<String> {
    let mut parts = command.split_whitespace();
    while let Some(part) = parts.next() {
        if part == flag {
            return parts.next().and_then(entity_concrete_cli_value);
        }
    }
    None
}

fn entity_concrete_cli_value(value: &str) -> Option<String> {
    let trimmed = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string();
    if trimmed.is_empty()
        || (trimmed.starts_with('<') && trimmed.ends_with('>'))
        || trimmed.contains("<COLUMN>")
        || trimmed.contains("<OUT>")
    {
        None
    } else {
        Some(trimmed)
    }
}

fn entity_json_string_at_path(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn entity_json_u64_at_path(value: &serde_json::Value, path: &[&str]) -> Option<u64> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_u64()
}

fn entity_apply_missing_lookup_column_refusal(
    apply: &EntityApplyCli,
    artifact: &serde_json::Value,
) -> CanonOutput {
    refusal::create_refusal(
        RefusalCode::EEntityArtifactContract,
        "Entity apply could not infer the input lookup column from the result artifact".to_string(),
        serde_json::json!({
            "reason": "missing_apply_lookup_column",
            "stage": "apply",
            "result": apply.result.display().to_string(),
            "rows": apply.rows.display().to_string(),
            "registry": apply.registry.display().to_string(),
            "profile": entity_json_string_at_path(artifact, &["metadata", "profile", "id"]),
            "supported_builtin_profiles": ["cmbs_tenant_label", "regab_firm_identity"],
            "writes_performed": false,
            "recovery_flag": "--column"
        }),
        Some("Rerun with canon entity apply <RESULT> --column <COLUMN>".to_string()),
    )
}

#[allow(clippy::result_large_err)]
fn run_entity_explain_pipeline(
    explain: &EntityExplainCli,
) -> Result<entity_runtime::ExplainArtifact, CanonOutput> {
    let (result, _result_bytes): (serde_json::Value, Vec<u8>) =
        read_json_artifact(&explain.result, "entity result artifact")?;
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
    params.insert(
        "cache_mode".to_string(),
        serde_json::Value::String(entity_index_cache_mode(run.cache_mode).as_str().to_string()),
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

struct EntityLifecyclePublicationSelector {
    work_dir: PathBuf,
    logical_path: String,
    from_stable_path: bool,
}

#[allow(clippy::result_large_err)]
fn read_entity_lifecycle_json_artifact(
    path: &Path,
    label: &str,
) -> Result<(serde_json::Value, Vec<u8>), CanonOutput> {
    if let Some(selector) = entity_lifecycle_stable_path_selector(path)
        && let Some(committed_bytes) = read_entity_lifecycle_committed_bytes(&selector)?
    {
        let committed_value = parse_entity_lifecycle_json_bytes(
            &committed_bytes,
            path,
            label,
            &selector.logical_path,
        )?;
        validate_entity_lifecycle_committed_value(&committed_value, &selector.logical_path)?;
        return Ok((committed_value, committed_bytes));
    }
    let (probe, direct_bytes) = read_json_artifact::<serde_json::Value>(path, label)?;
    let Some(selector) = entity_lifecycle_publication_selector(path, &probe)? else {
        return Ok((probe, direct_bytes));
    };
    let Some(committed_bytes) = read_entity_lifecycle_committed_bytes(&selector)? else {
        return Ok((probe, direct_bytes));
    };
    let committed_value =
        parse_entity_lifecycle_json_bytes(&committed_bytes, path, label, &selector.logical_path)?;
    validate_entity_lifecycle_committed_value(&committed_value, &selector.logical_path)?;
    if !selector.from_stable_path {
        validate_entity_lifecycle_copy_matches_committed(
            path,
            label,
            &probe,
            &committed_value,
            &selector,
        )?;
    }
    Ok((committed_value, committed_bytes))
}

#[allow(clippy::result_large_err)]
fn read_entity_lifecycle_committed_bytes(
    selector: &EntityLifecyclePublicationSelector,
) -> Result<Option<Vec<u8>>, CanonOutput> {
    entity::run::read_entity_run_committed_publication_logical_bytes(
        &selector.work_dir,
        &selector.logical_path,
    )
    .map_err(|refusal| refusal.to_canon_output())
}

#[allow(clippy::result_large_err)]
fn read_entity_lifecycle_bound_solve_artifact(
    run_path: &Path,
    run_artifact: &serde_json::Value,
) -> Result<serde_json::Value, CanonOutput> {
    let solve_path = entity_json_string_at_path(run_artifact, &["work_dir", "solve_artifact_path"])
        .ok_or_else(|| {
            refusal::create_refusal(
                RefusalCode::EEntityArtifactContract,
                "Run artifact is missing its bound solve path".to_string(),
                serde_json::json!({
                    "stage": "explain",
                    "field": "work_dir.solve_artifact_path",
                    "writes_performed": false
                }),
                Some("canon entity run --help".to_string()),
            )
        })?;
    let relative = Path::new(&solve_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            "Run artifact solve path must be safe and workdir-relative".to_string(),
            serde_json::json!({
                "stage": "explain",
                "field": "work_dir.solve_artifact_path",
                "path": solve_path,
                "writes_performed": false
            }),
            Some("canon entity run --help".to_string()),
        ));
    }
    let work_dir = entity_lifecycle_run_work_dir(run_path, run_artifact)?;
    let (solve_artifact, _) =
        read_entity_lifecycle_json_artifact(&work_dir.join(relative), "entity solve artifact")?;
    Ok(solve_artifact)
}

#[allow(clippy::result_large_err)]
fn entity_lifecycle_run_work_dir(
    run_path: &Path,
    run_artifact: &serde_json::Value,
) -> Result<PathBuf, CanonOutput> {
    if let Some(selector) = entity_lifecycle_stable_path_selector(run_path)
        && selector.logical_path == "run/run.json"
    {
        return Ok(selector.work_dir);
    }
    entity::schema::validate_entity_v1_self_hash(run_artifact)
        .map_err(|refusal| refusal.to_canon_output())?;
    let root_dir = entity_json_string_at_path(run_artifact, &["metadata", "workdir", "root_dir"])
        .ok_or_else(|| {
        refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            "Run artifact is missing workdir root metadata".to_string(),
            serde_json::json!({
                "stage": "explain",
                "field": "metadata.workdir.root_dir",
                "writes_performed": false
            }),
            Some("canon entity run --help".to_string()),
        )
    })?;
    Ok(PathBuf::from(root_dir))
}

#[allow(clippy::result_large_err)]
fn entity_lifecycle_publication_selector(
    path: &Path,
    artifact: &serde_json::Value,
) -> Result<Option<EntityLifecyclePublicationSelector>, CanonOutput> {
    if let Some(selector) = entity_lifecycle_stable_path_selector(path) {
        return Ok(Some(selector));
    }
    let Some(version) = entity_artifact_version(artifact) else {
        return Ok(None);
    };
    let Some(contract) = entity::entity_artifact_v1_contract_for_version(version) else {
        return Ok(None);
    };
    if !matches!(
        contract.stage,
        entity::EntityArtifactStageV1::Solve | entity::EntityArtifactStageV1::Run
    ) {
        return Ok(None);
    }
    entity::schema::validate_entity_v1_self_hash(artifact)
        .map_err(|refusal| refusal.to_canon_output())?;
    let root_dir = entity_json_string_at_path(artifact, &["metadata", "workdir", "root_dir"])
        .ok_or_else(|| {
            refusal::create_refusal(
                RefusalCode::EEntityArtifactContract,
                "Entity lifecycle artifact is missing workdir root metadata".to_string(),
                serde_json::json!({
                    "stage": "entity_lifecycle_source",
                    "field": "metadata.workdir.root_dir",
                    "version": version,
                    "writes_performed": false
                }),
                Some(format!("{} --help", contract.command)),
            )
        })?;
    Ok(Some(EntityLifecyclePublicationSelector {
        work_dir: PathBuf::from(root_dir),
        logical_path: contract.artifact_relpath.to_string(),
        from_stable_path: false,
    }))
}

fn entity_lifecycle_stable_path_selector(
    path: &Path,
) -> Option<EntityLifecyclePublicationSelector> {
    let file_name = path.file_name()?.to_str()?;
    let stage_dir = path.parent()?.file_name()?.to_str()?;
    let logical_path = match (stage_dir, file_name) {
        ("solve", "solve.json") => "solve/solve.json",
        ("run", "run.json") => "run/run.json",
        ("link", "link.json") => "link/link.json",
        _ => return None,
    };
    let work_dir = path
        .parent()
        .and_then(|parent| parent.parent())
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Some(EntityLifecyclePublicationSelector {
        work_dir,
        logical_path: logical_path.to_string(),
        from_stable_path: true,
    })
}

#[allow(clippy::result_large_err)]
fn parse_entity_lifecycle_json_bytes(
    bytes: &[u8],
    path: &Path,
    label: &str,
    logical_path: &str,
) -> Result<serde_json::Value, CanonOutput> {
    serde_json::from_slice(bytes).map_err(|error| {
        refusal::create_refusal(
            RefusalCode::EParse,
            format!(
                "Failed to parse committed {} '{}' for logical path '{}': {}",
                label,
                path.display(),
                logical_path,
                error
            ),
            serde_json::json!({
                "path": path.display().to_string(),
                "artifact": label,
                "logical_path": logical_path,
                "error": error.to_string(),
                "committed": true,
                "writes_performed": false
            }),
            None,
        )
    })
}

#[allow(clippy::result_large_err)]
fn validate_entity_lifecycle_committed_value(
    value: &serde_json::Value,
    logical_path: &str,
) -> Result<(), CanonOutput> {
    let expected_version = match logical_path {
        "solve/solve.json" => entity::CANON_ENTITY_SOLVE_VERSION_V1,
        "run/run.json" => entity::CANON_ENTITY_RUN_VERSION_V1,
        "link/link.json" => entity::run::link::ENTITY_LINK_VERSION,
        _ => {
            return Err(refusal::create_refusal(
                RefusalCode::EEntityArtifactContract,
                "Committed entity lifecycle source used an unsupported logical path".to_string(),
                serde_json::json!({
                    "stage": "entity_lifecycle_source",
                    "logical_path": logical_path,
                    "writes_performed": false
                }),
                Some("canon entity run --help".to_string()),
            ));
        }
    };
    let actual_version = entity_artifact_version(value).unwrap_or("<missing>");
    if actual_version != expected_version {
        return Err(refusal::create_refusal(
            RefusalCode::EEntityArtifactContract,
            "Committed entity lifecycle source has the wrong artifact version".to_string(),
            serde_json::json!({
                "stage": "entity_lifecycle_source",
                "logical_path": logical_path,
                "expected_version": expected_version,
                "actual_version": actual_version,
                "committed": true,
                "writes_performed": false
            }),
            Some("canon entity run --help".to_string()),
        ));
    }
    if logical_path == "link/link.json" {
        entity::run::link::validate_entity_link_artifact_raw_shape(value)
            .map_err(|refusal| refusal.to_canon_output())?;
        let link = serde_json::from_value::<entity::run::link::EntityLinkArtifact>(value.clone())
            .map_err(|error| {
            refusal::create_refusal(
                RefusalCode::EEntityArtifactContract,
                "Committed entity link artifact failed typed validation".to_string(),
                serde_json::json!({
                    "stage": "entity_lifecycle_source",
                    "logical_path": logical_path,
                    "error": error.to_string(),
                    "committed": true,
                    "writes_performed": false
                }),
                Some("canon entity link --help".to_string()),
            )
        })?;
        entity::run::link::validate_entity_link_artifact_contract(&link)
            .map_err(|refusal| refusal.to_canon_output())?;
        return Ok(());
    }
    entity::schema::validate_artifact_v1_core_contract(value)
        .map_err(|refusal| refusal.to_canon_output())?;
    entity::schema::validate_entity_v1_self_hash(value)
        .map_err(|refusal| refusal.to_canon_output())?;
    Ok(())
}

#[allow(clippy::result_large_err)]
fn validate_entity_lifecycle_copy_matches_committed(
    path: &Path,
    label: &str,
    requested: &serde_json::Value,
    committed: &serde_json::Value,
    selector: &EntityLifecyclePublicationSelector,
) -> Result<(), CanonOutput> {
    let requested_hash = entity_json_string_at_path(requested, &["artifact_content_hash"])
        .unwrap_or_else(|| "<missing>".to_string());
    let committed_hash = entity_json_string_at_path(committed, &["artifact_content_hash"])
        .unwrap_or_else(|| "<missing>".to_string());
    if requested_hash == committed_hash {
        return Ok(());
    }
    Err(refusal::create_refusal(
        RefusalCode::EEntityArtifactContract,
        "Entity lifecycle artifact copy does not match the committed logical artifact".to_string(),
        serde_json::json!({
            "stage": "entity_lifecycle_source",
            "path": path.display().to_string(),
            "artifact": label,
            "work_dir": selector.work_dir.display().to_string(),
            "logical_path": selector.logical_path.as_str(),
            "expected": committed_hash,
            "actual": requested_hash,
            "committed": true,
            "writes_performed": false
        }),
        Some("Use the canonical artifact path under the entity work directory".to_string()),
    ))
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
fn read_block_candidate_records_artifact(
    path: &Path,
) -> Result<Vec<entity::block::BlockCandidateRecord>, CanonOutput> {
    let label = "entity block candidate records";
    let bytes = read_artifact_bytes(path, label)?;
    match bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
    {
        Some(b'[') => serde_json::from_slice(&bytes).map_err(|error| {
            refusal::create_refusal(
                RefusalCode::EParse,
                format!(
                    "Failed to parse {label} '{}' as JSON: {error}",
                    path.display()
                ),
                serde_json::json!({
                    "path": path.display().to_string(),
                    "artifact": label,
                    "format": "json_array",
                    "error": error.to_string(),
                }),
                None,
            )
        }),
        _ => read_block_candidate_records_jsonl(path, &bytes, label),
    }
}

#[allow(clippy::result_large_err)]
fn read_block_candidate_records_jsonl(
    path: &Path,
    bytes: &[u8],
    label: &str,
) -> Result<Vec<entity::block::BlockCandidateRecord>, CanonOutput> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        refusal::create_refusal(
            RefusalCode::EParse,
            format!(
                "{label} '{}' must be valid UTF-8 JSONL: {error}",
                path.display()
            ),
            serde_json::json!({
                "path": path.display().to_string(),
                "artifact": label,
                "format": "jsonl",
                "error": error.to_string(),
            }),
            None,
        )
    })?;

    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(line_index, line)| {
            serde_json::from_str(line).map_err(|error| {
                refusal::create_refusal(
                    RefusalCode::EParse,
                    format!(
                        "Failed to parse {label} '{}' as JSONL: {error}",
                        path.display()
                    ),
                    serde_json::json!({
                        "path": path.display().to_string(),
                        "artifact": label,
                        "format": "jsonl",
                        "line_number": line_index + 1,
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

/// Long flags an agent commonly types on the core lookup command. Used to
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
const KNOWN_SUBCOMMANDS: [&str; 5] = ["doctor", "package", "registry", "entity", "strategy"];

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
            Some("Check entity link row, strategy, registry, and work-dir paths, then rerun canon entity link".to_string()),
        ),
        resolve::ResolveErrorCode::Parse => refusal::create_refusal(
            RefusalCode::EParse,
            message,
            detail,
            Some("Use supported entity link row formats (.csv, .tsv, .jsonl, .ndjson) and valid JSON/YAML, then rerun canon entity link".to_string()),
        ),
        resolve::ResolveErrorCode::Strategy => refusal::create_refusal(
            RefusalCode::EBadStrategy,
            message,
            detail,
            Some("Fix the strategy YAML and rerun canon entity link with --strategy".to_string()),
        ),
        resolve::ResolveErrorCode::InputContract => refusal::create_refusal(
            RefusalCode::EColumnNotFound,
            message,
            detail,
            Some("Fix strategy field mappings or row headers, then rerun canon entity link".to_string()),
        ),
        resolve::ResolveErrorCode::Registry => refusal::create_refusal(
            RefusalCode::EBadRegistry,
            message,
            detail,
            Some("Check the entity link registry and rerun canon entity link".to_string()),
        ),
        resolve::ResolveErrorCode::TooLarge => refusal::create_refusal(
            RefusalCode::ETooLarge,
            message,
            detail,
            Some("Increase --max-rows or --max-bytes, or reduce the linked row sets, then rerun canon entity link".to_string()),
        ),
        resolve::ResolveErrorCode::TooManyCandidates => refusal::create_refusal(
            RefusalCode::ETooManyCandidates,
            message,
            detail,
            Some("Tighten candidate_filter or raise --max-candidates, then rerun canon entity link".to_string()),
        ),
        resolve::ResolveErrorCode::EmptyTape => refusal::create_refusal(
            RefusalCode::EEmptyTape,
            message,
            detail,
            Some("Provide reference and target rows with processable records, then rerun canon entity link".to_string()),
        ),
        resolve::ResolveErrorCode::IncompatibleTapes => refusal::create_refusal(
            RefusalCode::EIncompatibleTapes,
            message,
            detail,
            Some("Fix the strategy so reference and target fields can be compared, then rerun canon entity link".to_string()),
        ),
        resolve::ResolveErrorCode::Gold => refusal::create_refusal(
            RefusalCode::EParse,
            message,
            detail,
            Some("Repair the gold JSONL cross-reference file and rerun canon entity link".to_string()),
        ),
        resolve::ResolveErrorCode::WriteBack => refusal::create_refusal(
            RefusalCode::EParse,
            message,
            detail,
            Some("Resolve registry write-back conflicts before rerunning canon entity link --write-back".to_string()),
        ),
        resolve::ResolveErrorCode::Unimplemented => refusal::create_refusal(
            RefusalCode::EParse,
            message,
            detail,
            Some("Complete the entity link implementation beads before using canon entity link".to_string()),
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
    pub registry: Option<Box<RegistryMeta>>,
    pub summary: Option<Box<Summary>>,
    pub mappings: Vec<Mapping>,
    pub unresolved: Vec<UnresolvedEntry>,
    pub refusal: Option<Box<Refusal>>,
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
    EPackageNonCanonical,
    EPackageContract,
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
    EGeoCommandUnavailable,
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
            RefusalCode::EPackageNonCanonical => "E_PACKAGE_NONCANONICAL",
            RefusalCode::EPackageContract => "E_PACKAGE_CONTRACT",
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
            RefusalCode::EGeoCommandUnavailable => "E_GEO_COMMAND_UNAVAILABLE",
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
            RefusalCode::EPackageNonCanonical => {
                "Rewrite --package JSON as sorted-key compact UTF-8 bytes, then rerun canon package pack"
            }
            RefusalCode::EPackageContract => {
                "Fix package fields, digests, paths, or archive constraints, then rerun canon package pack"
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
            RefusalCode::EGeoCommandUnavailable => {
                "Run canon geo capabilities --emit json to inspect implemented Geo commands"
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
                "Tighten candidate_filter or raise --max-candidates, then rerun canon entity link"
            }
            RefusalCode::EEmptyTape => {
                "Provide reference and target rows with processable records, then rerun canon entity link"
            }
            RefusalCode::EIncompatibleTapes => {
                "Fix strategy field mappings so the linked row sets share comparable fields, then rerun canon entity link"
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
    use std::path::Path;

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
        assert_eq!(super::suggest_subcommand("registry"), None);
        // Transpositions are distance 2 under Levenshtein, so not suggested.
        assert_eq!(super::suggest_subcommand("ogr"), None);
        // A real data filename is far from any subcommand.
        assert_eq!(super::suggest_subcommand("positions.csv"), None);
    }

    #[test]
    fn link_review_export_expected_summary_uses_json_boundary() {
        let expected = crate::resolve::ResolveSummary {
            target_records: 11,
            matched: 1,
            unmatched: 10,
            ambiguous: 0,
            match_rate: 1.0 / 11.0,
        };
        let boundary_bytes = serde_json::to_vec(&expected).expect("summary serializes");
        let boundary: crate::resolve::ResolveSummary =
            serde_json::from_slice(&boundary_bytes).expect("summary round trips");

        let canonical =
            super::canonicalize_entity_link_review_export_expected_summary(&expected, "test")
                .expect("summary canonicalizes");

        assert!(canonical.partition_holds());
        assert_eq!(canonical, boundary);
        assert_eq!(
            canonical.match_rate.to_bits(),
            boundary.match_rate.to_bits()
        );
    }

    #[test]
    fn generalization_redaction_hashes_identifiers_but_keeps_hashes_and_counts() {
        let mut value = serde_json::json!({
            "benchmark_id": "private.benchmark",
            "corpus_ref": "private://corpus",
            "trial_id": "trial.private",
            "observation_ids": ["obs.private.1", "obs.private.2"],
            "path": "../private/path.json",
            "locator": "private-locator",
            "cutoff": "2026-04-17",
            "content_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "result_count": 2,
            "self_attested_outcomes_used": false
        });

        super::generalization_redact_identifier_fields(&mut value);

        assert_ne!(value["benchmark_id"], "private.benchmark");
        assert!(
            value["benchmark_id"]
                .as_str()
                .expect("benchmark id")
                .starts_with("blake3:")
        );
        assert_ne!(value["corpus_ref"], "private://corpus");
        assert_ne!(value["observation_ids"][0], "obs.private.1");
        assert_ne!(value["path"], "../private/path.json");
        assert_ne!(value["locator"], "private-locator");
        assert_ne!(value["cutoff"], "2026-04-17");
        assert!(
            value["cutoff"]
                .as_str()
                .expect("cutoff")
                .starts_with("blake3:")
        );
        assert_eq!(
            value["content_hash"],
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(value["result_count"], 2);
        assert_eq!(value["self_attested_outcomes_used"], false);
    }

    #[test]
    fn generalization_redaction_preserves_public_quality_ids_and_hashes_private_ids() {
        let mut value = serde_json::json!({
            "quality": {
                "version": "canon.evaluation.generalization.quality_gate_report.v0",
                "contract_version": "canon.entity.quality.v1",
                "release_claim_status": "blocked",
                "gates": [
                    {
                        "gate_id": "candidate_recall_at_50_min",
                        "metric_id": "candidate_recall_at_50",
                        "status": "fail",
                        "private_artifact_id": "operator-private-artifact",
                        "artifact_path": "../private/artifact.json"
                    }
                ]
            },
            "corpus_ref": "private://corpus",
            "artifact_ref": "private://artifact"
        });

        super::generalization_redact_identifier_fields(&mut value);

        let gate = &value["quality"]["gates"][0];
        assert_eq!(gate["gate_id"], "candidate_recall_at_50_min");
        assert_eq!(gate["metric_id"], "candidate_recall_at_50");
        assert_eq!(gate["status"], "fail");
        for redacted in [
            &gate["private_artifact_id"],
            &gate["artifact_path"],
            &value["corpus_ref"],
            &value["artifact_ref"],
        ] {
            assert!(
                redacted
                    .as_str()
                    .expect("redacted identifier")
                    .starts_with("blake3:")
            );
        }
    }

    #[test]
    fn generalization_summary_exposes_blocked_quality_status_and_gate_counts() {
        use crate::evaluation::generalization::{
            CANON_ENTITY_QUALITY_VERSION, CANON_GENERALIZATION_QUALITY_GATE_REPORT_VERSION,
            CANON_GENERALIZATION_VERSION, CorpusVisibility, GeneralizationAggregate,
            GeneralizationQualityContractReport, GeneralizationQualityGateReport,
            GeneralizationQualityGateStatus, GeneralizationReleaseClaimStatus,
            GeneralizationReport,
        };

        let report = GeneralizationReport {
            version: CANON_GENERALIZATION_VERSION.to_string(),
            benchmark_id: "private.benchmark".to_string(),
            corpus_visibility: CorpusVisibility::PrivateCorpusRef,
            corpus_ref: "private://corpus".to_string(),
            benchmark_digest:
                "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
            report_digest:
                "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                    .to_string(),
            entity_disjoint: Vec::new(),
            time_forward: Vec::new(),
            aggregate: GeneralizationAggregate {
                entity_disjoint_trial_count: 0,
                time_forward_trial_count: 0,
                result_count: 0,
                correct_count: 0,
                abstain_count: 0,
                critical_false_merge_count: 0,
                directional_cross_source_count: 0,
                head_result_count: 0,
                tail_result_count: 0,
                easy_result_count: 0,
                hard_result_count: 0,
                strata: Vec::new(),
            },
            quality: GeneralizationQualityContractReport {
                version: CANON_GENERALIZATION_QUALITY_GATE_REPORT_VERSION.to_string(),
                contract_version: CANON_ENTITY_QUALITY_VERSION.to_string(),
                release_claim_status: GeneralizationReleaseClaimStatus::Blocked,
                gates: vec![
                    GeneralizationQualityGateReport {
                        gate_id: "candidate_recall_at_50_min".to_string(),
                        metric_id: "candidate_recall_at_50".to_string(),
                        status: GeneralizationQualityGateStatus::Fail,
                        observed_value: Some(0.5),
                        operator: ">=".to_string(),
                        threshold: 0.995,
                        waiver_bead_id: None,
                    },
                    GeneralizationQualityGateReport {
                        gate_id: "auto_link_recall_min".to_string(),
                        metric_id: "auto_link_recall".to_string(),
                        status: GeneralizationQualityGateStatus::NotApplicable,
                        observed_value: None,
                        operator: ">=".to_string(),
                        threshold: 0.98,
                        waiver_bead_id: None,
                    },
                    GeneralizationQualityGateReport {
                        gate_id: "critical_false_merges_max".to_string(),
                        metric_id: "hard_negative_false_merges".to_string(),
                        status: GeneralizationQualityGateStatus::Pass,
                        observed_value: Some(0.0),
                        operator: "==".to_string(),
                        threshold: 0.0,
                        waiver_bead_id: None,
                    },
                ],
            },
            derivation: None,
        };

        let summary = super::render_generalization_report_summary(&report);

        assert!(summary.contains("release_claim_status=blocked"));
        assert!(summary.contains("failed_gate_count=1"));
        assert!(summary.contains("not_applicable_gate_count=1"));
        assert!(!summary.contains("release_claim_status=eligible"));
    }

    #[test]
    fn generalization_refusal_omits_raw_manifest_and_error_text() {
        let output = super::generalization_refusal(
            Path::new("../private/envelope.json"),
            crate::evaluation::generalization::GeneralizationError::new(
                crate::evaluation::generalization::GeneralizationErrorCode::ArtifactContract,
                "private raw error text",
            ),
        );
        let rendered = serde_json::to_string(&output).expect("refusal serializes");

        assert!(!rendered.contains("../private/envelope.json"));
        assert!(!rendered.contains("private raw error text"));
        let detail = &output.refusal.expect("refusal").detail;
        assert_eq!(detail["stage"], "generalization");
        assert_eq!(detail["public_reason"], "artifact_contract");
        assert_eq!(detail["writes_performed"], false);
        assert!(
            detail["manifest_fingerprint"]
                .as_str()
                .expect("manifest fingerprint")
                .starts_with("blake3:")
        );
        assert!(
            detail["message_fingerprint"]
                .as_str()
                .expect("message fingerprint")
                .starts_with("blake3:")
        );
    }
}
