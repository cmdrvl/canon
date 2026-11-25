use crate::{
    cli::{Cli, DoctorArgs, DoctorCommand, DoctorJsonArgs},
    operator::{OperatorManifestValidationReport, validate_operator_manifest_json},
    paths,
    registry::package::REGISTRY_PACKAGE_VERIFY_SCHEMA_VERSION,
};
use clap::CommandFactory;
use serde_json::{Value, json};
use std::error::Error;

const CONTRACT: &str = "cmdrvl.read_only_doctor.v1";
const HEALTH_SCHEMA: &str = "canon.doctor.health.v1";
const CAPABILITIES_SCHEMA: &str = "canon.doctor.capabilities.v1";
const TRIAGE_SCHEMA: &str = "canon.doctor.triage.v1";
const OPERATOR_JSON: &str = include_str!("../operator.json");

/// `canon --robot-triage` and `canon doctor --robot-triage`: one compact JSON
/// triage report; exit 0 when healthy, 1 when the contract parity check fails.
pub fn run_robot_triage() -> Result<u8, Box<dyn Error>> {
    let payload = triage_payload();
    let exit_code = health_exit_code(&payload);
    write_json_with_code(&payload, exit_code)
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

    match &args.command {
        Some(DoctorCommand::Health(command)) => run_health(command),
        Some(DoctorCommand::Capabilities(command)) => run_capabilities(command),
        Some(DoctorCommand::RobotDocs) => {
            print_robot_docs();
            Ok(0)
        }
        None if args.json => {
            let payload = health_payload();
            let exit_code = health_exit_code(&payload);
            write_json_with_code(&payload, exit_code)
        }
        None => {
            let payload = health_payload();
            print_health_summary(&payload);
            Ok(health_exit_code(&payload))
        }
    }
}

fn run_health(args: &DoctorJsonArgs) -> Result<u8, Box<dyn Error>> {
    let payload = health_payload();
    let exit_code = health_exit_code(&payload);
    if args.json {
        write_json_with_code(&payload, exit_code)
    } else {
        print_health_summary(&payload);
        Ok(exit_code)
    }
}

/// `canon capabilities` and `canon doctor capabilities`.
pub fn run_capabilities(args: &DoctorJsonArgs) -> Result<u8, Box<dyn Error>> {
    let payload = capabilities_payload();
    if args.json {
        write_json_with_code(&payload, 0)
    } else {
        println!("canon doctor capabilities");
        println!("read_only=true");
        println!("fixers=0");
        println!("commands=health,capabilities,robot-docs,--robot-triage");
        println!("top_level=--robot-triage,capabilities --json,robot-docs guide");
        Ok(0)
    }
}

fn write_json_with_code(payload: &Value, code: u8) -> Result<u8, Box<dyn Error>> {
    println!("{}", serde_json::to_string(payload)?);
    Ok(code)
}

fn health_exit_code(payload: &Value) -> u8 {
    if payload.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        0
    } else {
        1
    }
}

fn health_payload() -> Value {
    let report = operator_manifest_report();
    let doctor_checks = doctor_specific_checks();
    let checks = health_checks(&report, doctor_checks);
    let failed = checks
        .iter()
        .filter(|check| !check.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let total = checks.len();
    let passed = total.saturating_sub(failed);

    json!({
        "schema": HEALTH_SCHEMA,
        "contract": CONTRACT,
        "tool": "canon",
        "version": env!("CARGO_PKG_VERSION"),
        "ok": failed == 0,
        "read_only": true,
        "summary": {
            "checks_total": total,
            "checks_passed": passed,
            "checks_failed": failed
        },
        "checks": checks,
        "operator_manifest": operator_manifest_payload(&report),
        "contract_parity": contract_parity_payload(&report),
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
    json!({
        "schema": CAPABILITIES_SCHEMA,
        "contract": CONTRACT,
        "tool": "canon",
        "version": env!("CARGO_PKG_VERSION"),
        "read_only": true,
        "commands": [
            {
                "name": "health",
                "usage": "canon doctor health [--json]",
                "output_schema": HEALTH_SCHEMA,
                "description": "Report compiled manifest and read-only contract health"
            },
            {
                "name": "capabilities",
                "usage": "canon doctor capabilities [--json]",
                "output_schema": CAPABILITIES_SCHEMA,
                "description": "Describe doctor commands, exit codes, side-effect boundaries, and disabled fixers"
            },
            {
                "name": "robot-docs",
                "usage": "canon doctor robot-docs",
                "output_schema": "text/plain",
                "description": "Emit concise machine-oriented usage notes"
            },
            {
                "name": "robot-triage",
                "usage": "canon doctor --robot-triage",
                "output_schema": TRIAGE_SCHEMA,
                "description": "Emit a compact triage report for automation"
            }
        ],
        "top_level_aliases": {
            "robot_triage": {
                "command": "canon --robot-triage",
                "same_as": "canon doctor --robot-triage",
                "output_schema": TRIAGE_SCHEMA
            },
            "capabilities": {
                "command": "canon capabilities --json",
                "same_as": "canon doctor capabilities --json",
                "output_schema": CAPABILITIES_SCHEMA
            },
            "robot_docs": {
                "command": "canon robot-docs guide",
                "output_schema": "text/markdown"
            }
        },
        "exit_codes": {
            "0": "doctor report emitted successfully",
            "1": "unhealthy compiled operator contract parity in health or triage reports",
            "2": "CLI usage error or refusal"
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
    let health = health_payload();
    let ok = health.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let checks = health
        .get("checks")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));

    json!({
        "schema": TRIAGE_SCHEMA,
        "contract": CONTRACT,
        "tool": "canon",
        "version": env!("CARGO_PKG_VERSION"),
        "ok": ok,
        "score": if ok { 100 } else { 0 },
        "read_only": true,
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
        "fixers": [],
        "recommended_next_steps": [
            "Use canon --describe for the full compiled operator contract.",
            "Use canon registry lint for registry-file diagnostics.",
            "Do not expect canon doctor to read inputs, append witness ledgers, contact providers, or mutate registries."
        ]
    })
}

fn operator_manifest_report() -> OperatorManifestValidationReport {
    validate_operator_manifest_json(&Cli::command(), OPERATOR_JSON)
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
    let manifest = match serde_json::from_str::<Value>(OPERATOR_JSON) {
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
        .get("subcommands")
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
            json!({ "missing": "operator subcommand row 'doctor'" }),
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

fn print_health_summary(payload: &Value) {
    let ok = payload.get("ok").and_then(Value::as_bool).unwrap_or(false);
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
    let status = if ok { "ok" } else { "unhealthy" };
    println!("canon doctor health: {status}");
    println!("read_only=true");
    println!("fixers=0");
    println!("checks={passed}/{total}");
}

fn print_robot_docs() {
    println!("canon doctor robot-docs");
    println!("contract: {CONTRACT}");
    println!("commands:");
    println!("  canon doctor health --json");
    println!("  canon doctor capabilities --json");
    println!("  canon doctor --robot-triage");
    println!("  canon geo capabilities --guide");
    println!("geo:");
    println!(
        "  - start with the installed Geo guide for supported clues, input schemas, and an offline example"
    );
    println!(
        "  - the guide is documentation, not a planning capability artifact; use geo capabilities without --guide for --capabilities"
    );
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
         - canon --robot-triage\n\
         - canon capabilities --json\n\
         - canon robot-docs guide\n\
         - canon <INPUT> --registry <DIR> --column <COLUMN>\n\
         - canon doctor health [--json]\n\
         - canon doctor capabilities [--json]\n\
         - canon doctor robot-docs\n\
         - canon doctor --robot-triage\n\
         - canon --describe\n\
         - canon --schema\n\
         \n\
         Output:\n\
         - `canon --robot-triage` and `canon doctor --robot-triage` print one JSON object (canon.doctor.triage.v1).\n\
         - `canon capabilities --json` and `canon doctor capabilities --json` print one JSON object (canon.doctor.capabilities.v1).\n\
         - Without `--json`, `canon capabilities` prints a short text summary.\n\
         - Output is plain text or JSON with no color, so NO_COLOR, TERM=dumb, and a non-tty stdout change nothing.\n\
         \n\
         Exit codes:\n\
         - 0: report emitted and the compiled operator contract is healthy\n\
         - 1: triage found the compiled operator contract unhealthy\n\
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
         Repair policy: canon doctor has no --fix surface in this release. Use `canon --robot-triage` or `canon capabilities --json` for read-only diagnostics, and `canon registry lint` for registry-file diagnostics.\n",
    );
    guide
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robot_guide_names_every_top_level_discovery_command() {
        let guide = robot_guide_markdown();
        for command in [
            "canon --robot-triage",
            "canon capabilities --json",
            "canon robot-docs guide",
            "canon doctor --robot-triage",
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
    fn capabilities_payload_declares_top_level_aliases() {
        let payload = capabilities_payload();
        let aliases = &payload["top_level_aliases"];
        assert_eq!(aliases["robot_triage"]["command"], "canon --robot-triage");
        assert_eq!(
            aliases["capabilities"]["command"],
            "canon capabilities --json"
        );
        assert_eq!(aliases["robot_docs"]["command"], "canon robot-docs guide");
        assert_eq!(payload["read_only"], true);
        assert_eq!(payload["fixers"], json!([]));
    }

    #[test]
    fn shipped_doctor_contract_report_is_healthy() {
        let payload = health_payload();
        assert_eq!(payload["ok"], true);
        assert_eq!(payload["contract_parity"]["ok"], true);
        assert!(
            payload["operator_manifest"]["blake3"]
                .as_str()
                .is_some_and(|digest| digest.starts_with("blake3:"))
        );
    }

    #[test]
    fn doctor_exit_code_tracks_unhealthy_payloads() {
        assert_eq!(health_exit_code(&json!({ "ok": true })), 0);
        assert_eq!(health_exit_code(&json!({ "ok": false })), 1);
        assert_eq!(health_exit_code(&json!({})), 1);
    }

    #[test]
    fn doctor_specific_checks_reject_fix_surface() {
        let mut manifest: Value = serde_json::from_str(OPERATOR_JSON).unwrap();
        let doctor = manifest
            .get_mut("subcommands")
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
