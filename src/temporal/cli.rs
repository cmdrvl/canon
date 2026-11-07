//! Offline command surface for native temporal snapshot and relation artifacts.

use super::compile::{
    TemporalCompileRequest, canonical_compile_bytes, compile_exact_lookup_snapshot,
};
use super::relation::{DirectedRelationFact, canonical_relation_set_bytes};
use crate::{RefusalCode, cli::RegistryEmitMode, refusal, witness};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::{error::Error, fs::File, io::Read, path::PathBuf};

#[derive(Args, Debug, Clone)]
pub struct TemporalCli {
    #[command(subcommand)]
    pub command: TemporalCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum TemporalCommand {
    /// Compile accepted alias claims at explicit valid-time and knowledge-time cutoffs.
    /// Emits a package artifact; does not install or promote registry entries.
    Compile(CompileArgs),
    /// Validate and canonicalize typed directed relation facts without changing identity lookup.
    Relation(TemporalRelationCli),
}

#[derive(Args, Debug, Clone)]
pub struct TemporalRelationCli {
    #[command(subcommand)]
    pub command: TemporalRelationCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum TemporalRelationCommand {
    /// Finalize a JSON array of canon.identity.relation.v1 facts.
    Finalize(RelationFinalizeArgs),
}

#[derive(Args, Debug, Clone)]
pub struct RelationFinalizeArgs {
    /// JSON array of DirectedRelationFact inputs. Generated relation IDs are recomputed.
    #[arg(long)]
    pub facts: PathBuf,
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
    /// Bound input bytes read before parsing.
    #[arg(long, default_value_t = 16_777_216, value_parser = clap::value_parser!(u64).range(1..=1_073_741_824))]
    pub max_bytes: u64,
    #[arg(long)]
    pub no_witness: bool,
}

#[derive(Args, Debug, Clone)]
pub struct CompileArgs {
    /// JSON TemporalCompileRequest, including valid_at, known_as_of, claims and policy
    #[arg(long)]
    pub request: PathBuf,
    #[arg(long, value_enum, default_value = "json")]
    pub emit: RegistryEmitMode,
    /// Bound request bytes read before parsing
    #[arg(long, default_value_t = 16_777_216, value_parser = clap::value_parser!(u64).range(1..=1_073_741_824))]
    pub max_bytes: u64,
    #[arg(long)]
    pub no_witness: bool,
}

pub fn run(cli: &TemporalCli) -> Result<u8, Box<dyn Error>> {
    match &cli.command {
        TemporalCommand::Compile(args) => compile(args),
        TemporalCommand::Relation(cli) => match &cli.command {
            TemporalRelationCommand::Finalize(args) => finalize_relation_facts(args),
        },
    }
}

fn compile(args: &CompileArgs) -> Result<u8, Box<dyn Error>> {
    let mut bytes = Vec::new();
    let read = File::open(&args.request)
        .and_then(|file| file.take(args.max_bytes + 1).read_to_end(&mut bytes));
    if let Err(error) = read {
        return refuse(
            args,
            None,
            RefusalCode::EIo,
            "Cannot read temporal request",
            json!({"path": args.request, "error": error.to_string()}),
        );
    }
    if bytes.len() as u64 > args.max_bytes {
        return refuse(
            args,
            None,
            RefusalCode::EParse,
            "Temporal request exceeds --max-bytes",
            json!({"max_bytes": args.max_bytes}),
        );
    }
    let request: TemporalCompileRequest = match serde_json::from_slice(&bytes) {
        Ok(request) => request,
        Err(error) => {
            return refuse(
                args,
                Some(&bytes),
                RefusalCode::EParse,
                "Invalid temporal compile request JSON",
                json!({"error": error.to_string()}),
            );
        }
    };
    let artifact = match compile_exact_lookup_snapshot(request) {
        Ok(artifact) => artifact,
        Err(error) => {
            return refuse(
                args,
                Some(&bytes),
                RefusalCode::EEntityArtifactContract,
                "Temporal snapshot compilation refused",
                json!({"temporal_error_code": error.code, "message": error.message}),
            );
        }
    };
    let output = canonical_compile_bytes(&artifact)?;
    match args.emit {
        RegistryEmitMode::Json => println!("{}", std::str::from_utf8(&output)?),
        RegistryEmitMode::Summary => println!(
            "compiled registry={} version={} valid_at={} known_as_of={} mappings={} omissions={} registry_written=false",
            artifact.registry_package.registry.id,
            artifact.registry_package.registry.version,
            artifact.valid_at,
            artifact.known_as_of,
            artifact.mapping_proofs.len(),
            artifact.omissions.len()
        ),
    }
    append_witness(args, Some(&bytes), &output, "COMPILED", 0);
    Ok(0)
}

fn refuse(
    args: &CompileArgs,
    input: Option<&[u8]>,
    code: RefusalCode,
    message: &str,
    detail: Value,
) -> Result<u8, Box<dyn Error>> {
    let refusal = refusal::create_refusal(
        code,
        message.to_string(),
        detail,
        Some("Repair the request using canon temporal compile --help, then rerun".to_string()),
    );
    let output = serde_json::to_vec(&refusal)?;
    match args.emit {
        RegistryEmitMode::Json => println!("{}", std::str::from_utf8(&output)?),
        RegistryEmitMode::Summary => eprintln!("{}", std::str::from_utf8(&output)?),
    }
    append_witness(args, input, &output, "REFUSAL", 2);
    Ok(2)
}

fn append_witness(
    args: &CompileArgs,
    input: Option<&[u8]>,
    output: &[u8],
    outcome: &str,
    exit: u8,
) {
    if args.no_witness {
        return;
    }
    let record = witness::WitnessRecord::new(
        vec![witness::WitnessInput {
            path: args.request.display().to_string(),
            hash: input.map(witness::hash_bytes),
            bytes: input.map(|bytes| bytes.len() as u64),
        }],
        serde_json::Map::from_iter([
            ("command".to_string(), json!("temporal compile")),
            ("max_bytes".to_string(), json!(args.max_bytes)),
        ]),
        &witness::hash_bytes(output),
        outcome,
        exit,
    );
    if let Err(error) = witness::append_witness_record(&record, false) {
        eprintln!("Warning: temporal witness append failed: {error}");
    }
}

fn finalize_relation_facts(args: &RelationFinalizeArgs) -> Result<u8, Box<dyn Error>> {
    let mut bytes = Vec::new();
    let read = File::open(&args.facts)
        .and_then(|file| file.take(args.max_bytes + 1).read_to_end(&mut bytes));
    if let Err(error) = read {
        return refuse_relation(
            args,
            None,
            RefusalCode::EIo,
            "Cannot read temporal relation facts",
            json!({"path": args.facts, "error": error.to_string()}),
        );
    }
    if bytes.len() as u64 > args.max_bytes {
        return refuse_relation(
            args,
            Some(&bytes),
            RefusalCode::EParse,
            "Temporal relation facts exceed --max-bytes",
            json!({"max_bytes": args.max_bytes}),
        );
    }
    let facts: Vec<DirectedRelationFact> = match serde_json::from_slice(&bytes) {
        Ok(facts) => facts,
        Err(error) => {
            return refuse_relation(
                args,
                Some(&bytes),
                RefusalCode::EParse,
                "Invalid temporal relation facts JSON",
                json!({"error": error.to_string()}),
            );
        }
    };
    let output = match canonical_relation_set_bytes(&facts) {
        Ok(output) => output,
        Err(error) => {
            return refuse_relation(
                args,
                Some(&bytes),
                RefusalCode::EEntityArtifactContract,
                "Temporal relation finalization refused",
                json!({"relation_error_code": error.code, "message": error.message}),
            );
        }
    };
    let finalized: Vec<DirectedRelationFact> = serde_json::from_slice(&output)?;
    match args.emit {
        RegistryEmitMode::Json => println!("{}", std::str::from_utf8(&output)?),
        RegistryEmitMode::Summary => println!(
            "finalized relations={} registry_written=false aliases_written=false",
            finalized.len()
        ),
    }
    append_relation_witness(args, Some(&bytes), &output, "FINALIZED", 0);
    Ok(0)
}

fn refuse_relation(
    args: &RelationFinalizeArgs,
    input: Option<&[u8]>,
    code: RefusalCode,
    message: &str,
    detail: Value,
) -> Result<u8, Box<dyn Error>> {
    let refusal = refusal::create_refusal(
        code,
        message.to_string(),
        detail,
        Some(
            "Repair the relation facts using schemas/canon.identity.relation.v1.schema.json, then rerun canon temporal relation finalize".to_string(),
        ),
    );
    let output = serde_json::to_vec(&refusal)?;
    match args.emit {
        RegistryEmitMode::Json => println!("{}", std::str::from_utf8(&output)?),
        RegistryEmitMode::Summary => eprintln!("{}", std::str::from_utf8(&output)?),
    }
    append_relation_witness(args, input, &output, "REFUSAL", 2);
    Ok(2)
}

fn append_relation_witness(
    args: &RelationFinalizeArgs,
    input: Option<&[u8]>,
    output: &[u8],
    outcome: &str,
    exit: u8,
) {
    if args.no_witness {
        return;
    }
    let record = witness::WitnessRecord::new(
        vec![witness::WitnessInput {
            path: args.facts.display().to_string(),
            hash: input.map(witness::hash_bytes),
            bytes: input.map(|bytes| bytes.len() as u64),
        }],
        serde_json::Map::from_iter([
            ("command".to_string(), json!("temporal relation finalize")),
            ("max_bytes".to_string(), json!(args.max_bytes)),
        ]),
        &witness::hash_bytes(output),
        outcome,
        exit,
    );
    if let Err(error) = witness::append_witness_record(&record, false) {
        eprintln!("Warning: temporal relation witness append failed: {error}");
    }
}
