#![forbid(unsafe_code)]

use crate::{
    CanonOutput, RefusalCode,
    cli::{
        ProjectCli, ProjectDescribeCli, ProjectEmitMode, ProjectInitCli, ProjectSubcommand,
        ProjectValidateCli,
    },
    refusal,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use super::{
    ProjectManifest, ProjectManifestError, ProjectManifestProjection, ProjectNetworkPolicy,
    ProjectTemporalContract, ProjectTemporalMode, load_project_manifest_toml,
    project_manifest_digest, project_manifest_projection, project_temporal_contract,
};

const PROJECT_CLI_SCHEMA_VERSION: &str = "canon.project.cli.v1";
const PROJECT_MANIFEST_FILENAME: &str = "canon.project.toml";

pub fn run(project: &ProjectCli) -> Result<u8, Box<dyn Error>> {
    match &project.command {
        ProjectSubcommand::Init(args) => run_init(args),
        ProjectSubcommand::Validate(args) => run_validate(args),
        ProjectSubcommand::Describe(args) => run_describe(args),
    }
}

#[derive(Debug, Serialize)]
struct ProjectInitReceipt {
    schema_version: &'static str,
    command: &'static str,
    project_dir: String,
    manifest_path: String,
    project_id: String,
    manifest_digest: String,
    written_files: Vec<String>,
    side_effects: Vec<ProjectSideEffect>,
    ignore_guidance: Vec<String>,
    next_commands: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ProjectValidationReport {
    schema_version: &'static str,
    command: &'static str,
    valid: bool,
    project_dir: String,
    manifest_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_digest: Option<String>,
    diagnostics: Vec<ProjectDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest: Option<ProjectManifestSummary>,
    next_commands: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ProjectDescribeReport {
    schema_version: &'static str,
    command: &'static str,
    project_dir: String,
    manifest_path: String,
    manifest_digest: String,
    state_flags: BTreeMap<String, Value>,
    manifest: ProjectManifestSummary,
    manifest_projection: ProjectManifestProjection,
    temporal_contract: ProjectTemporalContract,
    capabilities: ProjectCapabilities,
    next_commands: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ProjectManifestSummary {
    schema_version: String,
    project_id: String,
    package_count: usize,
    source_count: usize,
    mode_count: usize,
    output_count: usize,
    secret_count: usize,
    extension_count: usize,
    offline_build_only: bool,
    network_policy: &'static str,
    temporal_mode: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct ProjectDiagnostic {
    code: String,
    severity: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    next_command: String,
}

#[derive(Debug, Serialize)]
struct ProjectSideEffect {
    command: &'static str,
    mutates: bool,
    scope: &'static str,
    files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProjectCapabilities {
    schema_version: &'static str,
    commands: Vec<ProjectCommandCapability>,
    output_modes: Vec<&'static str>,
    exit_codes: BTreeMap<String, &'static str>,
}

#[derive(Debug, Serialize)]
struct ProjectCommandCapability {
    command: &'static str,
    read_only: bool,
    side_effects: Vec<&'static str>,
    outputs: Vec<&'static str>,
    next_command: &'static str,
    examples: Vec<&'static str>,
}

struct LoadedProject {
    manifest_path: PathBuf,
    manifest: ProjectManifest,
    digest: String,
    projection: ProjectManifestProjection,
    temporal_contract: ProjectTemporalContract,
}

fn run_init(args: &ProjectInitCli) -> Result<u8, Box<dyn Error>> {
    let manifest_path = args.directory.join(PROJECT_MANIFEST_FILENAME);
    let manifest_text = render_minimal_manifest(&args.project_id, &args.mapping_profile);
    let manifest = match load_project_manifest_toml(&manifest_text) {
        Ok(manifest) => manifest,
        Err(error) => {
            return emit_refusal(
                RefusalCode::EEntityArtifactContract,
                "Project init inputs do not produce a valid project manifest",
                json!({
                    "project_id": args.project_id,
                    "mapping_profile": args.mapping_profile,
                    "diagnostic": diagnostic_from_manifest_error(error, Some(path_string(&manifest_path))),
                }),
                Some(format!(
                    "canon project init {} --project-id project.synthetic.alpha",
                    shell_path(&args.directory)
                )),
                &args.emit,
            );
        }
    };
    let digest = project_manifest_digest(&manifest)?;

    if args.directory.exists() {
        if !args.directory.is_dir() {
            return emit_refusal(
                RefusalCode::EIo,
                "Project init target is not a directory",
                json!({ "project_dir": path_string(&args.directory) }),
                Some(format!(
                    "choose an empty directory, then rerun canon project init {}",
                    shell_path(&args.directory)
                )),
                &args.emit,
            );
        }
        match directory_is_empty(&args.directory) {
            Ok(true) => {}
            Ok(false) => {
                return emit_refusal(
                    RefusalCode::EIo,
                    "Project init refuses to write into a non-empty directory",
                    json!({
                        "project_dir": path_string(&args.directory),
                        "mutation": "none",
                    }),
                    Some(format!(
                        "choose an empty directory, then rerun canon project init {}",
                        shell_path(&args.directory)
                    )),
                    &args.emit,
                );
            }
            Err(error) => {
                return emit_refusal(
                    RefusalCode::EIo,
                    "Project init could not inspect the target directory",
                    json!({
                        "project_dir": path_string(&args.directory),
                        "error": error.to_string(),
                    }),
                    Some("fix directory permissions, then rerun canon project init".to_string()),
                    &args.emit,
                );
            }
        }
    }

    let created_directory = !args.directory.exists();
    if let Err(error) = fs::create_dir_all(&args.directory) {
        return emit_refusal(
            RefusalCode::EIo,
            "Project init could not create the target directory",
            json!({
                "project_dir": path_string(&args.directory),
                "error": error.to_string(),
            }),
            Some("fix parent directory permissions, then rerun canon project init".to_string()),
            &args.emit,
        );
    }

    if let Err(error) = write_new_file(&manifest_path, manifest_text.as_bytes()) {
        return emit_refusal(
            RefusalCode::EIo,
            "Project init refused to overwrite an existing manifest or could not write it",
            json!({
                "manifest_path": path_string(&manifest_path),
                "error": error.to_string(),
            }),
            Some(format!(
                "choose an empty directory, then rerun canon project init {}",
                shell_path(&args.directory)
            )),
            &args.emit,
        );
    }

    let mut next_commands = project_next_commands(&args.directory, &manifest_path);
    next_commands.insert(
        "dry_run_plan".to_string(),
        "canon project plan --manifest <MANIFEST> --lock <LOCK>".to_string(),
    );
    let receipt = ProjectInitReceipt {
        schema_version: PROJECT_CLI_SCHEMA_VERSION,
        command: "project.init",
        project_dir: path_string(&args.directory),
        manifest_path: path_string(&manifest_path),
        project_id: manifest.project_id,
        manifest_digest: digest,
        written_files: vec![path_string(&manifest_path)],
        side_effects: vec![ProjectSideEffect {
            command: "project.init",
            mutates: true,
            scope: "explicit_project_directory_only",
            files: if created_directory {
                vec![path_string(&args.directory), path_string(&manifest_path)]
            } else {
                vec![path_string(&manifest_path)]
            },
        }],
        ignore_guidance: vec![
            "Generated runtime outputs are declared under out/ in the manifest.".to_string(),
            "Add out/ and local work directories to repository ignore rules when they are generated locally.".to_string(),
        ],
        next_commands,
    };
    emit_init_receipt(&receipt, &args.emit)?;
    Ok(0)
}

fn run_validate(args: &ProjectValidateCli) -> Result<u8, Box<dyn Error>> {
    let report = validate_project(&args.directory, &args.manifest);
    emit_validation_report(&report, &args.emit)?;
    Ok(if report.valid { 0 } else { 1 })
}

fn run_describe(args: &ProjectDescribeCli) -> Result<u8, Box<dyn Error>> {
    let loaded = match load_project(&args.directory, &args.manifest) {
        Ok(loaded) => loaded,
        Err(diagnostics) => {
            return emit_refusal(
                RefusalCode::EEntityArtifactContract,
                "Project describe requires a valid project manifest",
                json!({
                    "project_dir": path_string(&args.directory),
                    "diagnostics": diagnostics,
                }),
                Some(format!(
                    "canon project validate {} --manifest {}",
                    shell_path(&args.directory),
                    shell_path(&args.manifest)
                )),
                &args.emit,
            );
        }
    };
    let manifest = &loaded.manifest;
    let mut state_flags = BTreeMap::new();
    state_flags.insert("valid_manifest".to_string(), json!(true));
    state_flags.insert(
        "offline_build_only".to_string(),
        json!(manifest.runtime.offline_build_only),
    );
    state_flags.insert(
        "network_policy".to_string(),
        json!(network_policy_name(&manifest.runtime.network_policy)),
    );
    state_flags.insert(
        "declared_hosts".to_string(),
        json!(manifest.runtime.declared_hosts),
    );
    state_flags.insert(
        "requires_review".to_string(),
        json!(
            manifest.review.review_required_min_score_basis_points
                < manifest.review.auto_promote_min_score_basis_points
        ),
    );
    state_flags.insert(
        "temporal_mode".to_string(),
        json!(temporal_mode_name(&loaded.temporal_contract.mode)),
    );

    let report = ProjectDescribeReport {
        schema_version: PROJECT_CLI_SCHEMA_VERSION,
        command: "project.describe",
        project_dir: path_string(&args.directory),
        manifest_path: path_string(&loaded.manifest_path),
        manifest_digest: loaded.digest,
        state_flags,
        manifest: manifest_summary(manifest, &loaded.temporal_contract),
        manifest_projection: loaded.projection,
        temporal_contract: loaded.temporal_contract,
        capabilities: project_capabilities(),
        next_commands: project_next_commands(&args.directory, &loaded.manifest_path),
    };
    emit_describe_report(&report, &args.emit)?;
    Ok(0)
}

fn validate_project(directory: &Path, manifest_arg: &Path) -> ProjectValidationReport {
    let mut diagnostics = Vec::new();
    let manifest_path = match checked_manifest_path(directory, manifest_arg) {
        Ok(path) => path,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            directory.join(PROJECT_MANIFEST_FILENAME)
        }
    };

    if !directory.exists() {
        diagnostics.push(ProjectDiagnostic {
            code: "missing_project_directory".to_string(),
            severity: "error",
            message: "project directory does not exist".to_string(),
            path: Some(path_string(directory)),
            next_command: format!("canon project init {}", shell_path(directory)),
        });
    } else if !directory.is_dir() {
        diagnostics.push(ProjectDiagnostic {
            code: "project_path_not_directory".to_string(),
            severity: "error",
            message: "project path is not a directory".to_string(),
            path: Some(path_string(directory)),
            next_command: "choose a directory, then rerun canon project validate".to_string(),
        });
    }

    if manifest_path.extension().and_then(|value| value.to_str()) != Some("toml") {
        diagnostics.push(ProjectDiagnostic {
            code: "unsupported_manifest_extension".to_string(),
            severity: "error",
            message: "project manifest must use the .toml extension".to_string(),
            path: Some(path_string(&manifest_path)),
            next_command: format!(
                "canon project validate {} --manifest {}",
                shell_path(directory),
                PROJECT_MANIFEST_FILENAME
            ),
        });
    }

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "missing_project_directory")
    {
        return validation_report(directory, manifest_path, None, diagnostics, None);
    }

    let manifest_text = match fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(error) => {
            diagnostics.push(ProjectDiagnostic {
                code: "manifest_read_error".to_string(),
                severity: "error",
                message: format!("project manifest could not be read: {error}"),
                path: Some(path_string(&manifest_path)),
                next_command: format!("canon project init {}", shell_path(directory)),
            });
            return validation_report(directory, manifest_path, None, diagnostics, None);
        }
    };

    diagnostics.extend(required_manifest_diagnostics(
        &manifest_text,
        Some(path_string(&manifest_path)),
    ));

    let manifest = match load_project_manifest_toml(&manifest_text) {
        Ok(manifest) => manifest,
        Err(error) => {
            diagnostics.push(diagnostic_from_manifest_error(
                error,
                Some(path_string(&manifest_path)),
            ));
            return validation_report(directory, manifest_path, None, diagnostics, None);
        }
    };

    let digest = match project_manifest_digest(&manifest) {
        Ok(digest) => digest,
        Err(error) => {
            diagnostics.push(diagnostic_from_manifest_error(
                error,
                Some(path_string(&manifest_path)),
            ));
            return validation_report(directory, manifest_path, None, diagnostics, None);
        }
    };

    if let Err(error) = project_manifest_projection(&manifest, &manifest_path, &BTreeMap::new()) {
        diagnostics.push(diagnostic_from_manifest_error(
            error,
            Some(path_string(&manifest_path)),
        ));
    }
    let temporal_contract = match project_temporal_contract(&manifest) {
        Ok(contract) => Some(contract),
        Err(error) => {
            diagnostics.push(diagnostic_from_manifest_error(
                error,
                Some(path_string(&manifest_path)),
            ));
            None
        }
    };

    let manifest_summary = temporal_contract
        .as_ref()
        .map(|contract| manifest_summary(&manifest, contract));
    validation_report(
        directory,
        manifest_path,
        Some(digest),
        diagnostics,
        manifest_summary,
    )
}

fn load_project(
    directory: &Path,
    manifest_arg: &Path,
) -> Result<LoadedProject, Vec<ProjectDiagnostic>> {
    let report = validate_project(directory, manifest_arg);
    if !report.valid {
        return Err(report.diagnostics);
    }
    let manifest_path =
        checked_manifest_path(directory, manifest_arg).map_err(|diagnostic| vec![diagnostic])?;
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| {
        vec![ProjectDiagnostic {
            code: "manifest_read_error".to_string(),
            severity: "error",
            message: format!("project manifest could not be read: {error}"),
            path: Some(path_string(&manifest_path)),
            next_command: format!(
                "canon project validate {} --manifest {}",
                shell_path(directory),
                shell_path(manifest_arg)
            ),
        }]
    })?;
    let manifest = load_project_manifest_toml(&manifest_text).map_err(|error| {
        vec![diagnostic_from_manifest_error(
            error,
            Some(path_string(&manifest_path)),
        )]
    })?;
    let digest = project_manifest_digest(&manifest).map_err(|error| {
        vec![diagnostic_from_manifest_error(
            error,
            Some(path_string(&manifest_path)),
        )]
    })?;
    let projection = project_manifest_projection(&manifest, &manifest_path, &BTreeMap::new())
        .map_err(|error| {
            vec![diagnostic_from_manifest_error(
                error,
                Some(path_string(&manifest_path)),
            )]
        })?;
    let temporal_contract = project_temporal_contract(&manifest).map_err(|error| {
        vec![diagnostic_from_manifest_error(
            error,
            Some(path_string(&manifest_path)),
        )]
    })?;
    Ok(LoadedProject {
        manifest_path,
        manifest,
        digest,
        projection,
        temporal_contract,
    })
}

fn validation_report(
    directory: &Path,
    manifest_path: PathBuf,
    digest: Option<String>,
    diagnostics: Vec<ProjectDiagnostic>,
    manifest: Option<ProjectManifestSummary>,
) -> ProjectValidationReport {
    ProjectValidationReport {
        schema_version: PROJECT_CLI_SCHEMA_VERSION,
        command: "project.validate",
        valid: diagnostics.is_empty(),
        project_dir: path_string(directory),
        manifest_path: path_string(&manifest_path),
        manifest_digest: digest,
        diagnostics,
        manifest,
        next_commands: project_next_commands(directory, &manifest_path),
    }
}

fn checked_manifest_path(
    directory: &Path,
    manifest_arg: &Path,
) -> Result<PathBuf, ProjectDiagnostic> {
    if manifest_arg.as_os_str().is_empty() {
        return Err(ProjectDiagnostic {
            code: "empty_manifest_path".to_string(),
            severity: "error",
            message: "manifest path must be non-empty".to_string(),
            path: None,
            next_command: format!(
                "canon project validate {} --manifest {}",
                shell_path(directory),
                PROJECT_MANIFEST_FILENAME
            ),
        });
    }
    if manifest_arg.is_absolute() {
        return Ok(manifest_arg.to_path_buf());
    }
    let mut normalized = PathBuf::new();
    for component in manifest_arg.components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ProjectDiagnostic {
                    code: "manifest_path_traversal".to_string(),
                    severity: "error",
                    message: "manifest path must stay inside the project directory unless absolute"
                        .to_string(),
                    path: Some(path_string(manifest_arg)),
                    next_command: format!(
                        "canon project validate {} --manifest {}",
                        shell_path(directory),
                        PROJECT_MANIFEST_FILENAME
                    ),
                });
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        normalized.push(PROJECT_MANIFEST_FILENAME);
    }
    Ok(directory.join(normalized))
}

fn required_manifest_diagnostics(text: &str, path: Option<String>) -> Vec<ProjectDiagnostic> {
    let checks = [
        (
            "missing_schema_version",
            "manifest must declare top-level schema_version",
            has_top_level_key(text, "schema_version"),
        ),
        (
            "missing_project_id",
            "manifest must declare top-level project_id",
            has_top_level_key(text, "project_id"),
        ),
        (
            "missing_review_table",
            "manifest must declare a [review] table",
            has_section(text, "[review]"),
        ),
        (
            "missing_temporal_table",
            "manifest must declare a [temporal] table",
            has_section(text, "[temporal]"),
        ),
        (
            "missing_budgets_table",
            "manifest must declare a [budgets] table",
            has_section(text, "[budgets]"),
        ),
        (
            "missing_runtime_table",
            "manifest must declare a [runtime] table",
            has_section(text, "[runtime]"),
        ),
        (
            "missing_packages",
            "manifest must declare at least one [[packages]] entry",
            has_section(text, "[[packages]]"),
        ),
        (
            "missing_sources",
            "manifest must declare at least one [[sources]] entry",
            has_section(text, "[[sources]]"),
        ),
        (
            "missing_outputs",
            "manifest must declare at least one [[outputs]] entry",
            has_section(text, "[[outputs]]"),
        ),
        (
            "missing_modes",
            "manifest must declare at least one [[modes]] entry",
            has_section(text, "[[modes]]"),
        ),
    ];
    checks
        .into_iter()
        .filter(|(_, _, present)| !present)
        .map(|(code, message, _)| ProjectDiagnostic {
            code: code.to_string(),
            severity: "error",
            message: message.to_string(),
            path: path.clone(),
            next_command: "canon project init <EMPTY_DIR>".to_string(),
        })
        .collect()
}

fn has_top_level_key(text: &str, key: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        line.starts_with(key) && line[key.len()..].trim_start().starts_with('=')
    })
}

fn has_section(text: &str, section: &str) -> bool {
    text.lines().any(|line| line.trim() == section)
}

fn diagnostic_from_manifest_error(
    error: ProjectManifestError,
    path: Option<String>,
) -> ProjectDiagnostic {
    ProjectDiagnostic {
        code: format!("manifest_{:?}", error.code).to_ascii_lowercase(),
        severity: "error",
        message: error.message,
        path,
        next_command: "fix the project manifest, then rerun canon project validate".to_string(),
    }
}

fn manifest_summary(
    manifest: &ProjectManifest,
    temporal_contract: &ProjectTemporalContract,
) -> ProjectManifestSummary {
    ProjectManifestSummary {
        schema_version: manifest.schema_version.clone(),
        project_id: manifest.project_id.clone(),
        package_count: manifest.packages.len(),
        source_count: manifest.sources.len(),
        mode_count: manifest.modes.len(),
        output_count: manifest.outputs.len(),
        secret_count: manifest.secrets.len(),
        extension_count: manifest.extensions.len(),
        offline_build_only: manifest.runtime.offline_build_only,
        network_policy: network_policy_name(&manifest.runtime.network_policy),
        temporal_mode: temporal_mode_name(&temporal_contract.mode),
    }
}

fn project_capabilities() -> ProjectCapabilities {
    let mut exit_codes = BTreeMap::new();
    exit_codes.insert("0".to_string(), "command succeeded");
    exit_codes.insert(
        "1".to_string(),
        "project validate found manifest diagnostics",
    );
    exit_codes.insert("2".to_string(), "refusal envelope emitted");
    ProjectCapabilities {
        schema_version: PROJECT_CLI_SCHEMA_VERSION,
        commands: vec![
            ProjectCommandCapability {
                command: "canon project init <DIR>",
                read_only: false,
                side_effects: vec![
                    "creates the explicit project directory when missing",
                    "writes canon.project.toml with create-new semantics",
                ],
                outputs: vec!["canon.project.cli.v1 init receipt"],
                next_command: "canon project validate <DIR>",
                examples: vec![
                    "canon project init ./canon-project --project-id project.synthetic.alpha",
                ],
            },
            ProjectCommandCapability {
                command: "canon project validate <DIR>",
                read_only: true,
                side_effects: vec!["none"],
                outputs: vec!["canon.project.cli.v1 validation report"],
                next_command: "canon project describe <DIR>",
                examples: vec!["canon project validate ./canon-project --emit json"],
            },
            ProjectCommandCapability {
                command: "canon project describe <DIR>",
                read_only: true,
                side_effects: vec!["none"],
                outputs: vec!["canon.project.cli.v1 describe report"],
                next_command: "canon project plan --manifest <MANIFEST> --lock <LOCK>",
                examples: vec!["canon project describe ./canon-project --emit summary"],
            },
        ],
        output_modes: vec!["json", "summary"],
        exit_codes,
    }
}

fn project_next_commands(directory: &Path, manifest_path: &Path) -> BTreeMap<String, String> {
    let mut next = BTreeMap::new();
    next.insert(
        "validate".to_string(),
        format!("canon project validate {}", shell_path(directory)),
    );
    next.insert(
        "describe".to_string(),
        format!("canon project describe {}", shell_path(directory)),
    );
    next.insert(
        "plan".to_string(),
        format!(
            "canon project plan --manifest {} --lock <LOCK>",
            shell_path(manifest_path)
        ),
    );
    next
}

fn emit_init_receipt(
    receipt: &ProjectInitReceipt,
    emit: &ProjectEmitMode,
) -> Result<(), Box<dyn Error>> {
    match emit {
        ProjectEmitMode::Json => println!("{}", serde_json::to_string(receipt)?),
        ProjectEmitMode::Summary => println!(
            "initialized project={} manifest={} digest={} next=[{}]",
            receipt.project_id,
            receipt.manifest_path,
            receipt.manifest_digest,
            next_command_keys(&receipt.next_commands)
        ),
    }
    Ok(())
}

fn emit_validation_report(
    report: &ProjectValidationReport,
    emit: &ProjectEmitMode,
) -> Result<(), Box<dyn Error>> {
    match emit {
        ProjectEmitMode::Json => println!("{}", serde_json::to_string(report)?),
        ProjectEmitMode::Summary => {
            if report.valid {
                let project_id = report
                    .manifest
                    .as_ref()
                    .map(|manifest| manifest.project_id.as_str())
                    .unwrap_or("<unknown>");
                println!(
                    "valid project={} manifest={} digest={} next=[{}]",
                    project_id,
                    report.manifest_path,
                    report.manifest_digest.as_deref().unwrap_or("<missing>"),
                    next_command_keys(&report.next_commands)
                );
            } else {
                let first = report
                    .diagnostics
                    .first()
                    .map(|diagnostic| diagnostic.code.as_str())
                    .unwrap_or("unknown");
                println!(
                    "invalid diagnostics={} first={} next=\"{}\"",
                    report.diagnostics.len(),
                    first,
                    report
                        .diagnostics
                        .first()
                        .map(|diagnostic| diagnostic.next_command.as_str())
                        .unwrap_or("canon project validate <DIR>")
                );
            }
        }
    }
    Ok(())
}

fn emit_describe_report(
    report: &ProjectDescribeReport,
    emit: &ProjectEmitMode,
) -> Result<(), Box<dyn Error>> {
    match emit {
        ProjectEmitMode::Json => println!("{}", serde_json::to_string(report)?),
        ProjectEmitMode::Summary => println!(
            "project={} manifest={} packages={} sources={} outputs={} side_effects=init:writes_manifest,validate:none,describe:none next=[{}]",
            report.manifest.project_id,
            report.manifest_path,
            report.manifest.package_count,
            report.manifest.source_count,
            report.manifest.output_count,
            next_command_keys(&report.next_commands)
        ),
    }
    Ok(())
}

fn emit_refusal(
    code: RefusalCode,
    message: impl Into<String>,
    detail: Value,
    next_command: Option<String>,
    emit: &ProjectEmitMode,
) -> Result<u8, Box<dyn Error>> {
    let output = refusal::create_refusal(code, message.into(), detail, next_command);
    match emit {
        ProjectEmitMode::Json => println!("{}", serde_json::to_string(&output)?),
        ProjectEmitMode::Summary => eprintln!("{}", refusal_summary(&output)),
    }
    Ok(2)
}

fn refusal_summary(output: &CanonOutput) -> String {
    let Some(refusal) = &output.refusal else {
        return "refused code=unknown message=\"unknown refusal\"".to_string();
    };
    format!(
        "refused code={} message=\"{}\" next=\"{}\"",
        serde_json::to_string(&refusal.code).unwrap_or_else(|_| "\"E_PARSE\"".to_string()),
        refusal.message,
        refusal.next_command.as_deref().unwrap_or("")
    )
}

fn directory_is_empty(path: &Path) -> io::Result<bool> {
    Ok(fs::read_dir(path)?.next().transpose()?.is_none())
}

fn write_new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)
}

fn render_minimal_manifest(project_id: &str, mapping_profile: &str) -> String {
    format!(
        r#"schema_version = "canon.project.v1"
project_id = {project_id}

[review]
cannot_link_max_score_basis_points = 3000
review_required_min_score_basis_points = 7000
auto_promote_min_score_basis_points = 9500

[temporal]
valid_at = "timeless"
known_as_of = "timeless"

[budgets]
max_input_bytes = 1048576
max_rows = 50000
max_candidates = 5000
max_review_items = 1000
max_runtime_seconds = 600

[runtime]
offline_build_only = true
network_policy = "deny_all"
declared_hosts = []

[[packages]]
alias = "registry"
kind = "registry_package"
id = "pkg.synthetic.registry"
version = "1.0.0"
content_hash = "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

[[packages]]
alias = "strategy"
kind = "strategy_package"
id = "pkg.synthetic.strategy"
version = "1.0.0"
content_hash = "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"

[[packages]]
alias = "profile"
kind = "entity_profile_package"
id = "pkg.synthetic.profile"
version = "1.0.0"
content_hash = "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"

[[packages]]
alias = "mapping"
kind = "source_mapping_package"
id = "pkg.synthetic.mapping"
version = "1.0.0"
content_hash = "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"

[[sources]]
source_id = "source_alpha"
path = "input/minimal.csv"
format = "csv"
mapping_package = "mapping"
mapping_profile = {mapping_profile}
required = true

[[outputs]]
output_id = "summary"
kind = "summary_json"
path = "out/summary.json"
redact_identity = false

[[modes]]
mode_id = "cluster_default"
kind = "cluster"
source_ids = ["source_alpha"]
registry_package = "registry"
strategy_package = "strategy"
profile_package = "profile"
output_ids = ["summary"]
"#,
        project_id = toml_string(project_id),
        mapping_profile = toml_string(mapping_profile)
    )
}

fn toml_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped.push('"');
    escaped
}

fn network_policy_name(policy: &ProjectNetworkPolicy) -> &'static str {
    match policy {
        ProjectNetworkPolicy::DenyAll => "deny_all",
        ProjectNetworkPolicy::AllowDeclaredHosts => "allow_declared_hosts",
    }
}

fn temporal_mode_name(mode: &ProjectTemporalMode) -> &'static str {
    match mode {
        ProjectTemporalMode::Timeless => "timeless",
        ProjectTemporalMode::AsOf => "as_of",
    }
}

fn next_command_keys(commands: &BTreeMap<String, String>) -> String {
    commands.keys().cloned().collect::<Vec<_>>().join(",")
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn shell_path(path: &Path) -> String {
    let value = path_string(path);
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        value
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
