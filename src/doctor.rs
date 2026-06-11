use crate::{
    cli::{DoctorArgs, DoctorCommand, DoctorJsonArgs},
    paths,
};
use serde_json::{Value, json};
use std::error::Error;

const CONTRACT: &str = "cmdrvl.read_only_doctor.v1";
const HEALTH_SCHEMA: &str = "canon.doctor.health.v1";
const CAPABILITIES_SCHEMA: &str = "canon.doctor.capabilities.v1";
const TRIAGE_SCHEMA: &str = "canon.doctor.triage.v1";
const OPERATOR_JSON: &str = include_str!("../operator.json");

pub fn run(args: &DoctorArgs) -> Result<u8, Box<dyn Error>> {
    if args.robot_triage {
        return write_json(&triage_payload());
    }

    match &args.command {
        Some(DoctorCommand::Health(command)) => run_health(command),
        Some(DoctorCommand::Capabilities(command)) => run_capabilities(command),
        Some(DoctorCommand::RobotDocs) => {
            print_robot_docs();
            Ok(0)
        }
        None if args.json => write_json(&health_payload()),
        None => {
            print_health_summary(&health_payload());
            Ok(0)
        }
    }
}

fn run_health(args: &DoctorJsonArgs) -> Result<u8, Box<dyn Error>> {
    let payload = health_payload();
    if args.json {
        write_json(&payload)
    } else {
        print_health_summary(&payload);
        Ok(0)
    }
}

fn run_capabilities(args: &DoctorJsonArgs) -> Result<u8, Box<dyn Error>> {
    let payload = capabilities_payload();
    if args.json {
        write_json(&payload)
    } else {
        println!("canon doctor capabilities");
        println!("read_only=true");
        println!("fixers=0");
        println!("commands=health,capabilities,robot-docs,--robot-triage");
        Ok(0)
    }
}

fn write_json(payload: &Value) -> Result<u8, Box<dyn Error>> {
    println!("{}", serde_json::to_string(payload)?);
    Ok(0)
}

fn health_payload() -> Value {
    let checks = health_checks();
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
        "observed_paths": observed_paths(),
        "config_footprint": paths::config_footprint(),
        "composition": composition_payload(),
        "side_effects": side_effects(),
        "fixers": []
    })
}

fn capabilities_payload() -> Value {
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
        "exit_codes": {
            "0": "doctor report emitted successfully",
            "1": "reserved for future unhealthy read-only findings",
            "2": "CLI usage error or refusal"
        },
        "config_footprint": paths::config_footprint(),
        "composition": composition_payload(),
        "side_effects": side_effects(),
        "fixers": []
    })
}

fn composition_payload() -> Value {
    json!({
        "family": {
            "name": "cmdrvl-spine",
            "siblings": [
                {"tool": "canon", "capabilities": "canon doctor capabilities --json"},
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
        "config_footprint": paths::config_footprint(),
        "side_effects": side_effects(),
        "fixers": [],
        "recommended_next_steps": [
            "Use canon --describe for the full compiled operator contract.",
            "Use canon registry lint for registry-file diagnostics.",
            "Do not expect canon doctor to read inputs, append witness ledgers, contact providers, or mutate registries."
        ]
    })
}

fn health_checks() -> Vec<Value> {
    let manifest = serde_json::from_str::<Value>(OPERATOR_JSON).ok();
    let manifest_version = manifest
        .as_ref()
        .and_then(|value| value.get("version"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let output_schema = manifest
        .as_ref()
        .and_then(|value| value.get("invocation"))
        .and_then(|value| value.get("output_schema"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let doctor_declared = manifest
        .as_ref()
        .and_then(|value| value.get("subcommands"))
        .and_then(Value::as_array)
        .map(|subcommands| {
            subcommands
                .iter()
                .any(|entry| entry.get("name").and_then(Value::as_str) == Some("doctor"))
        })
        .unwrap_or(false);

    vec![
        check(
            "operator_manifest_embedded",
            manifest.is_some(),
            "compiled operator.json parses as JSON",
        ),
        check(
            "operator_manifest_version",
            manifest_version == env!("CARGO_PKG_VERSION"),
            "operator.json version matches the compiled crate version",
        ),
        check(
            "operator_output_schema",
            output_schema == "canon.v0",
            "operator.json declares the canon.v0 output contract",
        ),
        check(
            "doctor_manifest_entry",
            doctor_declared,
            "operator.json declares the read-only doctor subcommand",
        ),
        check(
            "fix_mode_disabled",
            true,
            "doctor --fix is intentionally absent from the CLI surface",
        ),
        check(
            "witness_ledger_unopened",
            true,
            "doctor resolves no input and does not append the witness ledger",
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
    ]
}

fn check(id: &str, ok: bool, message: &str) -> Value {
    json!({
        "id": id,
        "ok": ok,
        "severity": if ok { "info" } else { "error" },
        "message": message
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
