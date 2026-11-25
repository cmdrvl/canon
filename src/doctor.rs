use crate::{
    cli::{Cli, DoctorArgs, DoctorCommand, DoctorJsonArgs},
    operator::{
        DOCTOR_SCHEMA_VERSION, OperatorManifestValidationReport, embedded_operator_manifest,
        validate_operator_manifest_json,
    },
    paths,
    registry::package::REGISTRY_PACKAGE_VERIFY_SCHEMA_VERSION,
};
use clap::CommandFactory;
use serde_json::{Value, json};
use std::error::Error;

/// canon's read-only doctor guarantee. Before `cmdrvl.doctor.v1` this was the
/// report's `contract` string; `contract` now carries the operator.v1 document
/// in capabilities mode, so the guarantee is reported as `read_only_contract`.
const READ_ONLY_CONTRACT: &str = "cmdrvl.read_only_doctor.v1";
/// `fix` text of a failing finding. canon has no repair command, so the fix is
/// to rebuild from a tree whose operator contract matches the compiled CLI.
const FINDING_FIX: &str = "doctor has no --fix surface: read `contract_parity` and this finding's check in `canon doctor --json`, then rebuild canon from a release tree whose operator.json matches the compiled CLI";

/// `canon doctor triage [--json]`, plus the deprecated `canon --robot-triage`
/// and `canon doctor --robot-triage`, which always print JSON. Exit 0 when
/// healthy and 1 when a check fails (never 2: that is for usage errors).
pub fn run_robot_triage() -> Result<u8, Box<dyn Error>> {
    run_triage(true)
}

/// `canon robot-docs guide`: the markdown guide, read-only, always exit 0.
pub fn run_robot_guide() -> Result<u8, Box<dyn Error>> {
    print!("{}", robot_guide_markdown());
    Ok(0)
}

pub fn run(args: &DoctorArgs) -> Result<u8, Box<dyn Error>> {
    if args.robot_triage {
        return run_robot_triage();
    }

    // `--json` is accepted before or after the subcommand: `doctor --json
    // triage` and `doctor triage --json` are the same request.
    match &args.command {
        Some(DoctorCommand::Health(command)) => run_health(args.json || command.json),
        Some(DoctorCommand::Capabilities(command)) => {
            run_doctor_capabilities(args.json || command.json)
        }
        Some(DoctorCommand::Triage(command)) => run_triage(args.json || command.json),
        Some(DoctorCommand::RobotDocs) => {
            print_robot_docs();
            Ok(0)
        }
        None => run_health(args.json),
    }
}

fn run_health(json: bool) -> Result<u8, Box<dyn Error>> {
    let payload = health_payload();
    let exit_code = health_exit_code(&payload);
    if json {
        write_json_with_code(&payload, exit_code)
    } else {
        print_health_summary(&payload);
        Ok(exit_code)
    }
}

fn run_triage(json: bool) -> Result<u8, Box<dyn Error>> {
    let payload = triage_payload();
    let exit_code = health_exit_code(&payload);
    if json {
        write_json_with_code(&payload, exit_code)
    } else {
        print_triage_summary(&payload);
        Ok(exit_code)
    }
}

fn run_doctor_capabilities(json: bool) -> Result<u8, Box<dyn Error>> {
    let payload = capabilities_payload();
    let exit_code = health_exit_code(&payload);
    if json {
        write_json_with_code(&payload, exit_code)
    } else {
        println!("canon doctor capabilities");
        println!("read_only=true");
        println!("fixers=0");
        println!("commands=health,capabilities,triage,robot-docs");
        println!("deprecated=doctor health,doctor --robot-triage,--robot-triage");
        println!("top_level=capabilities --json,robot-docs guide");
        Ok(exit_code)
    }
}

/// `canon capabilities [--json]`: with `--json` the operator.v1 contract,
/// byte for byte the document `canon --describe` prints (spine-rules asserts
/// they match). It is not a doctor report; `canon doctor capabilities --json`
/// wraps this same document in a `cmdrvl.doctor.v1` report.
pub fn run_capabilities(args: &DoctorJsonArgs) -> Result<u8, Box<dyn Error>> {
    if args.json {
        println!("{}", embedded_operator_manifest());
    } else {
        println!("canon capabilities");
        println!("read_only=true");
        println!("fixers=0");
        println!("contract=operator.v1 (canon capabilities --json, same as canon --describe)");
        println!("doctor=canon doctor [--json|capabilities --json|triage --json]");
        println!("docs=canon robot-docs guide");
    }
    Ok(0)
}

fn write_json_with_code(payload: &Value, code: u8) -> Result<u8, Box<dyn Error>> {
    println!("{}", serde_json::to_string(payload)?);
    Ok(code)
}

/// `cmdrvl.doctor.v1`: exit 0 when `healthy`, 1 when not. A report without a
/// boolean `healthy` counts as unhealthy.
fn health_exit_code(payload: &Value) -> u8 {
    if payload
        .get("healthy")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        0
    } else {
        1
    }
}

fn check_passed(check: &Value) -> bool {
    check.get("ok").and_then(Value::as_bool).unwrap_or(false)
}

/// The `findings` of a `cmdrvl.doctor.v1` report: one per check, with the
/// check's severity, and a `fix` on each failing one. Check ids are unique.
fn findings(checks: &[Value]) -> Vec<Value> {
    checks
        .iter()
        .map(|check| {
            let mut finding = json!({
                "id": check.get("id").cloned().unwrap_or(Value::Null),
                "severity": check.get("severity").cloned().unwrap_or_else(|| json!("error")),
                "message": check.get("message").cloned().unwrap_or(Value::Null),
            });
            if !check_passed(check) {
                finding["fix"] = json!(FINDING_FIX);
            }
            finding
        })
        .collect()
}

/// Run every check once. `healthy` in each report is derived from these, so a
/// report never contradicts its own findings.
fn run_checks(report: &OperatorManifestValidationReport) -> Vec<Value> {
    health_checks(report, doctor_specific_checks())
}

fn health_payload() -> Value {
    let report = operator_manifest_report();
    let checks = run_checks(&report);
    health_payload_for(&report, checks)
}

fn health_payload_for(report: &OperatorManifestValidationReport, checks: Vec<Value>) -> Value {
    let failed = checks.iter().filter(|check| !check_passed(check)).count();
    let total = checks.len();
    let passed = total.saturating_sub(failed);

    json!({
        "schema_version": DOCTOR_SCHEMA_VERSION,
        "mode": "health",
        "tool": "canon",
        "tool_version": env!("CARGO_PKG_VERSION"),
        "healthy": failed == 0,
        "findings": findings(&checks),
        "ok": failed == 0,
        "read_only": true,
        "read_only_contract": READ_ONLY_CONTRACT,
        "summary": {
            "checks_total": total,
            "checks_passed": passed,
            "checks_failed": failed
        },
        "checks": checks,
        "operator_manifest": operator_manifest_payload(report),
        "contract_parity": contract_parity_payload(report),
        "observed_paths": observed_paths(),
        "config_footprint": paths::config_footprint(),
        "composition": composition_payload(),
        "registry_package_verification": registry_package_verification_payload(),
        "side_effects": side_effects(),
        "fixers": []
    })
}

fn capabilities_payload() -> Value {
    let report = operator_manifest_report();
    let checks = run_checks(&report);
    let healthy = checks.iter().all(check_passed);
    // The operator.v1 document itself, parsed from the one embedded text that
    // `--describe` and `capabilities --json` print. `null` only if that text
    // fails to parse, which `operator_manifest_embedded` already reports.
    let contract =
        serde_json::from_str::<Value>(&embedded_operator_manifest()).unwrap_or(Value::Null);
    json!({
        "schema_version": DOCTOR_SCHEMA_VERSION,
        "mode": "capabilities",
        "tool": "canon",
        "tool_version": env!("CARGO_PKG_VERSION"),
        "healthy": healthy,
        "findings": findings(&checks),
        "contract": contract,
        "ok": healthy,
        "read_only": true,
        "read_only_contract": READ_ONLY_CONTRACT,
        "commands": [
            {
                "name": "health",
                "usage": "canon doctor [--json]",
                "mode": "health",
                "output_schema": DOCTOR_SCHEMA_VERSION,
                "description": "Report compiled manifest and read-only contract health"
            },
            {
                "name": "capabilities",
                "usage": "canon doctor capabilities [--json]",
                "mode": "capabilities",
                "output_schema": DOCTOR_SCHEMA_VERSION,
                "description": "Describe doctor commands, exit codes, side-effect boundaries, and disabled fixers; `contract` is the operator.v1 document"
            },
            {
                "name": "triage",
                "usage": "canon doctor triage [--json]",
                "mode": "triage",
                "output_schema": DOCTOR_SCHEMA_VERSION,
                "description": "Report health with next actions for an agent"
            },
            {
                "name": "robot-docs",
                "usage": "canon doctor robot-docs",
                "output_schema": "text/plain",
                "description": "Emit concise machine-oriented usage notes"
            }
        ],
        "deprecated_aliases": [
            {"command": "canon doctor health [--json]", "use": "canon doctor [--json]"},
            {"command": "canon doctor --robot-triage", "use": "canon doctor triage --json"},
            {"command": "canon --robot-triage", "use": "canon doctor triage --json"}
        ],
        "top_level_commands": {
            "capabilities": {
                "command": "canon capabilities --json",
                "same_as": "canon --describe",
                "output_schema": "operator.v1"
            },
            "robot_docs": {
                "command": "canon robot-docs guide",
                "output_schema": "text/markdown"
            }
        },
        "exit_codes": {
            "0": "healthy: no error finding",
            "1": "unhealthy: at least one error finding (never reported as 2)",
            "2": "CLI usage error"
        },
        "operator_manifest": operator_manifest_payload(&report),
        "contract_parity": contract_parity_payload(&report),
        "config_footprint": paths::config_footprint(),
        "composition": composition_payload(),
        "registry_package_verification": registry_package_verification_payload(),
        "side_effects": side_effects(),
        "fixers": []
    })
}

fn composition_payload() -> Value {
    json!({
        "family": {
            "name": "cmdrvl-spine",
            "siblings": [
                {"tool": "canon", "capabilities": "canon capabilities --json"},
                {"tool": "profile", "capabilities": "profile capabilities --json"},
                {"tool": "shape", "capabilities": "shape capabilities --json"},
                {"tool": "rvl", "capabilities": "rvl capabilities --json"},
                {"tool": "pack", "capabilities": "pack capabilities --json"}
            ]
        },
        "role": "canonical identifier normalization before structural checks and reconciliation",
        "position": "before shape/rvl when messy IDs need canonical IDs",
        "accepts": ["CSV or JSONL input", "versioned canon registry"],
        "produces": ["canon mapping JSON", "canonicalized CSV when --emit csv is used"],
        "canonical_chain": [
            "canon old.csv --registry <REGISTRY> --column <COLUMN> --emit csv --map-out evidence/old.map.json > old.canon.csv",
            "canon new.csv --registry <REGISTRY> --column <COLUMN> --emit csv --map-out evidence/new.map.json > new.canon.csv",
            "shape old.canon.csv new.canon.csv --key <CANONICAL_COLUMN> --json > evidence/shape.report.json",
            "rvl old.canon.csv new.canon.csv --key <CANONICAL_COLUMN> --json > evidence/rvl.report.json"
        ],
        "agent_rules": [
            "Use canon before shape or rvl when source identifiers are aliases, legacy IDs, or vendor-specific IDs.",
            "Preserve --map-out artifacts; they explain how source IDs mapped to canonical IDs.",
            "Use profile after canon when downstream tools need an explicit column scope."
        ]
    })
}

fn registry_package_verification_payload() -> Value {
    json!({
        "verify_schema": REGISTRY_PACKAGE_VERIFY_SCHEMA_VERSION,
        "lint_profile": "package",
        "scope": [
            "recompute registry package descriptors and digest from local files",
            "validate effective mappings, entry counts, duplicate inputs, provenance descriptors, sidecar scopes, dependency pins, and signature attachment references",
            "emit deterministic robot JSON or summary findings without mutating registry state"
        ],
        "trust_boundary": "Package integrity verification is structural and does not approve signer identity, attestation policy, or promotion trust."
    })
}

fn triage_payload() -> Value {
    triage_payload_for(health_payload())
}

fn triage_payload_for(health: Value) -> Value {
    let healthy = health
        .get("healthy")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let checks = health
        .get("checks")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));

    json!({
        "schema_version": DOCTOR_SCHEMA_VERSION,
        "mode": "triage",
        "tool": "canon",
        "tool_version": env!("CARGO_PKG_VERSION"),
        "healthy": healthy,
        "findings": health.get("findings").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
        "next_actions": next_actions(healthy),
        "ok": healthy,
        "score": if healthy { 100 } else { 0 },
        "read_only": true,
        "read_only_contract": READ_ONLY_CONTRACT,
        "checks": checks,
        "operator_manifest": health
            .get("operator_manifest")
            .cloned()
            .unwrap_or(Value::Null),
        "contract_parity": health
            .get("contract_parity")
            .cloned()
            .unwrap_or(Value::Null),
        "config_footprint": paths::config_footprint(),
        "registry_package_verification": registry_package_verification_payload(),
        "side_effects": side_effects(),
        "fixers": []
    })
}

/// What an agent should run next. An unhealthy report must name at least one
/// action (`cmdrvl.doctor.v1`); canon's are all read-only because doctor never
/// repairs anything.
fn next_actions(healthy: bool) -> Vec<&'static str> {
    let mut actions = Vec::new();
    if !healthy {
        actions.push(
            "Run `canon doctor --json` and read every finding with severity error, with `contract_parity.manifest_errors`.",
        );
        actions.push(
            "Rebuild canon from a release tree whose operator.json matches the compiled CLI; doctor has no --fix surface.",
        );
    }
    actions.push("Use `canon capabilities --json` for the full operator.v1 contract.");
    actions.push("Use `canon registry lint` for registry-file diagnostics.");
    actions
}

fn operator_manifest_report() -> OperatorManifestValidationReport {
    validate_operator_manifest_json(&Cli::command(), &embedded_operator_manifest())
}

fn operator_manifest_payload(report: &OperatorManifestValidationReport) -> Value {
    json!({
        "source": "embedded:operator.json",
        "blake3": report.manifest_digest.as_deref(),
    })
}

fn contract_parity_payload(report: &OperatorManifestValidationReport) -> Value {
    json!(report)
}

fn health_checks(
    report: &OperatorManifestValidationReport,
    mut doctor_checks: Vec<Value>,
) -> Vec<Value> {
    let mut checks = vec![
        check_detail(
            "operator_manifest_embedded",
            report.manifest_digest.is_some(),
            "compiled operator.json parses and has a deterministic manifest digest",
            json!({
                "manifest_digest": report.manifest_digest.as_deref(),
                "manifest_errors": &report.manifest_errors
            }),
        ),
        check_detail(
            "operator_contract_parity",
            report.ok,
            "compiled Clap surface and embedded operator manifest are in parity",
            contract_parity_payload(report),
        ),
        check(
            "config_footprint_declared",
            paths::config_footprint()
                .get("self_contained")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            "doctor reports the canonical ~/.cmdrvl/ configuration footprint",
        ),
        check(
            "registry_unopened",
            true,
            "doctor reads no registry, strategy, review, tape, CSV, JSONL, or SQLite files",
        ),
        check(
            "network_disabled",
            true,
            "doctor performs no provider, DNS, HTTP, TLS, or other network probes",
        ),
    ];
    checks.append(&mut doctor_checks);
    checks
}

fn doctor_specific_checks() -> Vec<Value> {
    let manifest = match serde_json::from_str::<Value>(&embedded_operator_manifest()) {
        Ok(manifest) => manifest,
        Err(error) => {
            return vec![check_detail(
                "fix_mode_disabled",
                false,
                "doctor --fix is intentionally absent from the CLI surface",
                json!({ "operator_manifest_error": error.to_string() }),
            )];
        }
    };
    doctor_specific_checks_for_manifest(&manifest)
}

fn doctor_specific_checks_for_manifest(manifest: &Value) -> Vec<Value> {
    let Some(doctor) = manifest
        .get("commands")
        .and_then(Value::as_array)
        .and_then(|commands| {
            commands
                .iter()
                .find(|command| command.get("name").and_then(Value::as_str) == Some("doctor"))
        })
    else {
        return vec![check_detail(
            "fix_mode_disabled",
            false,
            "doctor --fix is intentionally absent from the CLI surface",
            json!({ "missing": "operator command row 'doctor'" }),
        )];
    };

    let fixers_empty = doctor
        .get("fixers")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty);
    let recovery_hits = doctor_recovery_surface_hits(doctor);
    let side_effects_false = doctor
        .get("side_effects")
        .and_then(Value::as_object)
        .is_some_and(|effects| effects.values().all(|value| value.as_bool() == Some(false)));

    vec![
        check_detail(
            "fix_mode_disabled",
            fixers_empty && recovery_hits.is_empty(),
            "doctor --fix is intentionally absent from the CLI surface",
            json!({
                "fixers_empty": fixers_empty,
                "recovery_surface_hits": recovery_hits
            }),
        ),
        check_detail(
            "doctor_manifest_read_only_side_effects",
            side_effects_false,
            "operator.json doctor side-effect row declares read-only behavior",
            json!({
                "side_effects": doctor.get("side_effects").cloned().unwrap_or(Value::Null)
            }),
        ),
        check(
            "witness_ledger_unopened",
            true,
            "doctor resolves no input and does not append the witness ledger",
        ),
    ]
}

fn doctor_recovery_surface_hits(doctor: &Value) -> Vec<String> {
    let mut hits = Vec::new();
    if let Some(usage) = doctor.get("usage").and_then(Value::as_str) {
        push_recovery_hit(&mut hits, "doctor.usage", usage);
    }
    if let Some(options) = doctor.get("options").and_then(Value::as_array) {
        for (index, option) in options.iter().enumerate() {
            for field in ["name", "flag", "description"] {
                if let Some(value) = option.get(field).and_then(Value::as_str) {
                    push_recovery_hit(
                        &mut hits,
                        &format!("doctor.options[{index}].{field}"),
                        value,
                    );
                }
            }
        }
    }
    for key in ["next_command", "recovery_command", "recovery_commands"] {
        if let Some(value) = doctor.get(key).and_then(Value::as_str) {
            push_recovery_hit(&mut hits, &format!("doctor.{key}"), value);
        }
    }
    if let Some(value) = doctor
        .get("recovery")
        .and_then(|recovery| recovery.get("next_command"))
        .and_then(Value::as_str)
    {
        push_recovery_hit(&mut hits, "doctor.recovery.next_command", value);
    }
    hits
}

fn push_recovery_hit(hits: &mut Vec<String>, field: &str, value: &str) {
    let lower = value.to_ascii_lowercase();
    let has_token = lower.contains("--fix")
        || lower
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-'))
            .any(|token| matches!(token, "fix" | "repair" | "recover"));
    if has_token {
        hits.push(field.to_string());
    }
}

fn check(id: &str, ok: bool, message: &str) -> Value {
    json!({
        "id": id,
        "ok": ok,
        "severity": if ok { "info" } else { "error" },
        "message": message
    })
}

fn check_detail(id: &str, ok: bool, message: &str, detail: Value) -> Value {
    json!({
        "id": id,
        "ok": ok,
        "severity": if ok { "info" } else { "error" },
        "message": message,
        "detail": detail
    })
}

fn observed_paths() -> Value {
    json!({
        "operator_manifest": "embedded:operator.json",
        "mapping_schema": "inline:canon.v0",
        "witness_ledger": paths::default_witness_path().display().to_string()
    })
}

fn side_effects() -> Value {
    json!({
        "reads_stdin": false,
        "reads_input_files": false,
        "reads_registry_files": false,
        "reads_strategy_files": false,
        "reads_schema_files": false,
        "reads_review_files": false,
        "loads_sqlite": false,
        "runs_lookup": false,
        "runs_resolve": false,
        "runs_org_identity": false,
        "runs_strategy_audit": false,
        "calls_providers": false,
        "opens_witness_ledger": false,
        "appends_witness_ledger": false,
        "creates_witness_directory": false,
        "writes_migration_logs": false,
        "writes_deprecation_notices": false,
        "writes_registry_files": false,
        "writes_mapping_sidecars": false,
        "writes_csv": false,
        "writes_doctor_artifacts": false,
        "uses_network": false,
        "changes_cwd": false,
        "rewrites_operator_manifest": false
    })
}

fn print_triage_summary(payload: &Value) {
    let healthy = payload
        .get("healthy")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    println!(
        "canon doctor triage: {}",
        if healthy { "healthy" } else { "unhealthy" }
    );
    println!("read_only=true");
    println!("fixers=0");
    for finding in payload
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|finding| finding.get("severity").and_then(Value::as_str) == Some("error"))
    {
        println!(
            "error: {}: {}",
            finding.get("id").and_then(Value::as_str).unwrap_or("?"),
            finding.get("message").and_then(Value::as_str).unwrap_or("")
        );
    }
    for action in payload
        .get("next_actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        println!("next: {action}");
    }
}

fn print_health_summary(payload: &Value) {
    let ok = payload
        .get("healthy")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let passed = payload
        .get("summary")
        .and_then(|summary| summary.get("checks_passed"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total = payload
        .get("summary")
        .and_then(|summary| summary.get("checks_total"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let status = if ok { "healthy" } else { "unhealthy" };
    println!("canon doctor health: {status}");
    println!("read_only=true");
    println!("fixers=0");
    println!("checks={passed}/{total}");
}

fn print_robot_docs() {
    println!("canon doctor robot-docs");
    println!("report: {DOCTOR_SCHEMA_VERSION}");
    println!("read_only_contract: {READ_ONLY_CONTRACT}");
    println!("commands:");
    println!("  canon doctor --json");
    println!("  canon doctor capabilities --json");
    println!("  canon doctor triage --json");
    println!("  canon capabilities --json");
    #[cfg(feature = "geo")]
    println!("  canon geo capabilities --guide");
    println!("deprecated_aliases:");
    println!("  canon doctor health --json  (use canon doctor --json)");
    println!("  canon doctor --robot-triage  (use canon doctor triage --json)");
    println!("  canon --robot-triage  (use canon doctor triage --json)");
    println!("exit_codes:");
    println!("  0 healthy, 1 unhealthy, 2 usage error");
    #[cfg(feature = "geo")]
    {
        println!("geo:");
        println!(
            "  - start with the installed Geo guide for supported clues, input schemas, and an offline example"
        );
        println!(
            "  - the guide is documentation, not a planning capability artifact; use geo capabilities without --guide for --capabilities"
        );
    }
    println!("entity:");
    println!(
        "  - the entity workbench how-to and operator catalog is docs/ENTITY_WORKBENCH_AGENT_GUIDE.md in the canon repo"
    );
    println!("  - start with canon entity profile list, then canon entity run");
    println!("read_only:");
    println!(
        "  - does not read stdin, input tapes, registries, schemas, review queues, or SQLite indexes"
    );
    println!(
        "  - does not append witness ledgers, create directories, write sidecars, or contact providers"
    );
    println!("composition:");
    println!("  - canon normalizes IDs before shape and rvl compare canonicalized rows");
    println!(
        "  - canon old.csv --registry <REGISTRY> --column <COLUMN> --emit csv --map-out evidence/old.map.json > old.canon.csv"
    );
    println!(
        "  - shape old.canon.csv new.canon.csv --key <CANONICAL_COLUMN> --json > evidence/shape.report.json"
    );
    println!(
        "  - rvl old.canon.csv new.canon.csv --key <CANONICAL_COLUMN> --json > evidence/rvl.report.json"
    );
    println!("fix_mode:");
    println!("  - no --fix surface is implemented in this release");
    println!("next_steps:");
    println!("  - use canon registry lint for registry diagnostics");
    println!("  - use canon --describe for the full operator manifest");
}

/// The `canon robot-docs guide` body. Plain markdown with no ANSI escapes, so
/// it reads the same on a pipe, under `NO_COLOR`, and under `TERM=dumb`.
fn robot_guide_markdown() -> String {
    let mut guide = String::new();
    guide.push_str(
        "# canon robot-docs guide\n\
         \n\
         canon's agent discovery surface is read-only. The discovery commands never read input tapes, registries, schemas, or SQLite indexes. They never append witness ledgers, write files, create directories, or contact providers.\n\
         \n\
         Commands:\n\
         - canon capabilities --json\n\
         - canon doctor --json\n\
         - canon doctor capabilities --json\n\
         - canon doctor triage --json\n\
         - canon robot-docs guide\n\
         - canon <INPUT> --registry <DIR> --column <COLUMN>\n\
         - canon doctor robot-docs\n\
         - canon --describe\n\
         - canon --schema\n\
         \n\
         Deprecated aliases (same documents as the canonical forms):\n\
         - canon doctor health [--json] is canon doctor [--json]\n\
         - canon doctor --robot-triage and canon --robot-triage are canon doctor triage --json\n\
         \n\
         Output:\n\
         - `canon capabilities --json` prints the operator.v1 contract, the same document as `canon --describe`.\n\
         - `canon doctor --json`, `canon doctor capabilities --json` and `canon doctor triage --json` print one cmdrvl.doctor.v1 JSON object each (mode health, capabilities, triage). `healthy` is false exactly when a finding has severity error. The capabilities report carries the operator.v1 contract as `contract`; the triage report carries `next_actions`.\n\
         - Without `--json`, the doctor commands and `canon capabilities` print a short text summary.\n\
         - Output is plain text or JSON with no color, so NO_COLOR, TERM=dumb, and a non-tty stdout change nothing.\n\
         \n\
         Exit codes:\n\
         - 0: report emitted and the compiled operator contract is healthy\n\
         - 1: a doctor report found the compiled operator contract unhealthy\n\
         - 2: command-line usage error or refusal\n\
         \n\
         Composition:\n\
         - Run `canon <old.csv> --registry <REGISTRY> --column <COLUMN> --emit csv --map-out evidence/old.map.json > old.canon.csv` before `shape` and `rvl` when source identifiers are aliases or legacy IDs.\n\
         - Keep the `--map-out` artifacts; they explain how each source ID mapped to a canonical ID.\n\
         - Lookup is exact byte match after ASCII-trim. Uncertain evidence belongs in `canon entity` and `canon resolve`, not in lookup.\n",
    );
    // Ask the compiled command tree, not a build flag, so the guide never
    // advertises a command this binary cannot run.
    if Cli::command().find_subcommand("geo").is_some() {
        guide.push_str(
            "\n\
             Geo:\n\
             - Start with `canon geo capabilities --guide` for supported clues, input schemas, and an offline example.\n\
             - The guide is documentation, not a planning capability artifact; use `canon geo capabilities` without `--guide` for the capabilities JSON.\n",
        );
    }
    guide.push_str(
        "\n\
         Entity workbench:\n\
         - The how-to and operator catalog is docs/ENTITY_WORKBENCH_AGENT_GUIDE.md in the canon repo.\n\
         - Start with `canon entity profile list`, then `canon entity run`.\n\
         \n\
         Repair policy: canon doctor has no --fix surface in this release. Use `canon doctor triage --json` or `canon capabilities --json` for read-only diagnostics, and `canon registry lint` for registry-file diagnostics.\n",
    );
    guide
}

#[cfg(test)]
mod tests {
    use super::*;
    use spine_rules::doctor::{DoctorMode, DoctorReport, Severity, check_doctor_run};

    fn report_of(payload: &Value) -> DoctorReport {
        DoctorReport::parse(&payload.to_string()).expect("payload parses as a doctor report")
    }

    #[test]
    fn robot_guide_names_every_top_level_discovery_command() {
        let guide = robot_guide_markdown();
        for command in [
            "canon --robot-triage",
            "canon capabilities --json",
            "canon robot-docs guide",
            "canon doctor --robot-triage",
            "canon doctor triage --json",
            "canon doctor --json",
        ] {
            assert!(guide.contains(command), "guide must name `{command}`");
        }
        assert!(
            !guide.contains('\u{1b}'),
            "guide must be plain text with no ANSI escapes"
        );
        assert_eq!(
            guide.contains("canon geo capabilities --guide"),
            Cli::command().find_subcommand("geo").is_some()
        );
    }

    #[test]
    fn capabilities_payload_declares_commands_and_deprecated_aliases() {
        let payload = capabilities_payload();
        let commands = &payload["top_level_commands"];
        assert_eq!(
            commands["capabilities"]["command"],
            "canon capabilities --json"
        );
        assert_eq!(commands["capabilities"]["output_schema"], "operator.v1");
        assert_eq!(commands["robot_docs"]["command"], "canon robot-docs guide");
        let aliases = payload["deprecated_aliases"].as_array().unwrap();
        for alias in [
            "canon doctor health [--json]",
            "canon doctor --robot-triage",
            "canon --robot-triage",
        ] {
            assert!(
                aliases.iter().any(|entry| entry["command"] == alias),
                "capabilities must declare deprecated alias `{alias}`"
            );
        }
        assert_eq!(payload["read_only"], true);
        assert_eq!(payload["read_only_contract"], READ_ONLY_CONTRACT);
        assert_eq!(payload["fixers"], json!([]));
    }

    #[test]
    fn shipped_doctor_reports_conform_to_cmdrvl_doctor_v1() {
        for (mode, payload) in [
            (DoctorMode::Health, health_payload()),
            (DoctorMode::Capabilities, capabilities_payload()),
            (DoctorMode::Triage, triage_payload()),
        ] {
            let report = report_of(&payload);
            assert_eq!(report.mode, mode);
            assert_eq!(
                report.validate(),
                Vec::<String>::new(),
                "{mode:?} report must have no cmdrvl.doctor.v1 violations"
            );
            assert!(report.healthy, "{mode:?} report must be healthy");
            let stdout = format!("{payload}\n");
            assert_eq!(
                check_doctor_run(
                    mode,
                    stdout.as_bytes(),
                    Some(health_exit_code(&payload).into())
                ),
                Vec::<String>::new()
            );
        }
    }

    #[test]
    fn shipped_doctor_contract_report_is_healthy() {
        let payload = health_payload();
        assert_eq!(payload["healthy"], true);
        assert_eq!(payload["ok"], true);
        assert_eq!(payload["contract_parity"]["ok"], true);
        assert!(
            payload["operator_manifest"]["blake3"]
                .as_str()
                .is_some_and(|digest| digest.starts_with("blake3:"))
        );
    }

    #[test]
    fn capabilities_report_embeds_the_operator_contract_it_prints() {
        let payload = capabilities_payload();
        let printed: Value = serde_json::from_str(&embedded_operator_manifest()).unwrap();
        assert_eq!(
            payload["contract"], printed,
            "`contract` must be the document `capabilities --json` and `--describe` print"
        );
        assert_eq!(payload["contract"]["schema_version"], "operator.v1");
        assert_eq!(payload["contract"]["name"], "canon");
        assert_eq!(payload["contract"]["tool_version"], payload["tool_version"]);
        // `contract` is only allowed in capabilities mode.
        assert!(health_payload().get("contract").is_none());
        assert!(triage_payload().get("contract").is_none());
    }

    #[test]
    fn unhealthy_reports_conform_and_exit_one_never_two() {
        let operator = operator_manifest_report();
        let failing = vec![
            check("operator_manifest_embedded", true, "parses"),
            check("forced_failure", false, "a check that fails"),
        ];
        let health = health_payload_for(&operator, failing);
        let triage = triage_payload_for(health.clone());

        assert_eq!(health["healthy"], false);
        assert_eq!(health_exit_code(&health), 1);
        assert_eq!(health_exit_code(&triage), 1);
        for (mode, payload) in [(DoctorMode::Health, &health), (DoctorMode::Triage, &triage)] {
            let report = report_of(payload);
            assert_eq!(report.validate(), Vec::<String>::new(), "{mode:?}");
            assert!(!report.healthy);
            let stdout = format!("{payload}\n");
            assert_eq!(
                check_doctor_run(mode, stdout.as_bytes(), Some(1)),
                Vec::<String>::new(),
                "unhealthy {mode:?} exits 1"
            );
            assert!(
                !check_doctor_run(mode, stdout.as_bytes(), Some(2)).is_empty(),
                "an unhealthy report answered with 2 must be flagged"
            );
        }

        let report = report_of(&triage);
        let errors: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .collect();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].id, "forced_failure");
        assert!(errors[0].fix.is_some(), "a failing finding carries a fix");
        assert!(
            !report.next_actions.unwrap().is_empty(),
            "an unhealthy triage report must name next actions"
        );
    }

    #[test]
    fn healthy_findings_have_no_fix_and_unique_ids() {
        let report = report_of(&health_payload());
        assert!(report.findings.iter().all(|finding| finding.fix.is_none()));
        let mut ids: Vec<_> = report.findings.iter().map(|finding| &finding.id).collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), total, "finding ids must be unique");
    }

    #[test]
    fn doctor_exit_code_tracks_unhealthy_payloads() {
        assert_eq!(health_exit_code(&json!({ "healthy": true })), 0);
        assert_eq!(health_exit_code(&json!({ "healthy": false })), 1);
        assert_eq!(health_exit_code(&json!({})), 1);
    }

    #[test]
    fn doctor_specific_checks_reject_fix_surface() {
        let mut manifest: Value = serde_json::from_str(&embedded_operator_manifest()).unwrap();
        let doctor = manifest
            .get_mut("commands")
            .and_then(Value::as_array_mut)
            .unwrap()
            .iter_mut()
            .find(|entry| entry.get("name").and_then(Value::as_str) == Some("doctor"))
            .unwrap();
        doctor["fixers"] = json!(["canon doctor --fix"]);

        let checks = doctor_specific_checks_for_manifest(&manifest);
        assert!(
            checks
                .iter()
                .any(|check| { check["id"] == "fix_mode_disabled" && check["ok"] == false })
        );
    }
}
