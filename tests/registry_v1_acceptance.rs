#![forbid(unsafe_code)]

use canon::registry::{RegistryPackage, canonical_package_bytes, compile_registry_package};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Barrier},
    thread,
    time::Instant,
};
use tempfile::TempDir;

const SELECTION_SEED: &str = "bd-26g5.registry-acceptance.runtime-selection.v1";
const DBT_NAMESPACE: &str = "registry_acceptance_dbt";
const SEARCH_NAMESPACE: &str = "registry_acceptance_search";
const CANONICAL_IRI_PREFIX: &str = "https://canon.example/id/";

#[derive(Debug, Clone, Deserialize)]
struct FixtureEntry {
    input: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
}

#[derive(Debug, Clone)]
struct SelectedSubject {
    entry: FixtureEntry,
    source_file: String,
    entry_order: usize,
    selection_hash: String,
}

#[derive(Debug, Clone)]
struct CommandRun {
    args: Vec<String>,
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ExportedAlias {
    alias: String,
    normalized_key: String,
    canonical_id: String,
    canonical_iri: String,
    canonical_type: String,
    rule_id: String,
    match_source: String,
    source_file: String,
    entry_order: usize,
}

#[derive(Debug, Clone, Serialize)]
struct SourceFileReceipt {
    path: String,
    content_hash: String,
    bytes: u64,
}

#[test]
fn immutable_lookup_and_exports_are_proven_from_package_receipts() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let home = temp.path().join("home");
    fs::create_dir_all(&home)?;
    let prepared = prepare_packaged_registry(temp.path())?;

    assert_package_declares_immutable_export_contracts(&prepared.package);

    let archive = temp.path().join("registry-acceptance.canonpkg");
    let mut command_receipts = Vec::new();
    let pack = run_canon(
        &home,
        vec![
            arg("package"),
            arg("pack"),
            arg("--root"),
            path_arg(&prepared.package_root),
            arg("--package"),
            path_arg(&prepared.package_root.join("package.json")),
            arg("--out"),
            path_arg(&archive),
        ],
    )?;
    assert_exit(&pack, 0);
    command_receipts.push(command_receipt(&pack));

    let verify = run_canon(
        &home,
        vec![arg("package"), arg("verify"), path_arg(&archive)],
    )?;
    assert_exit(&verify, 0);
    let verify_json = json_stdout(&verify)?;
    assert_eq!(
        verify_json["package_content_digest"],
        prepared.package.content_digest
    );
    assert_eq!(
        verify_json["package_bytes_digest"],
        hash_bytes(&prepared.package_bytes)
    );
    assert_eq!(verify_json["verified_files"], 4);
    command_receipts.push(command_receipt(&verify));

    let unpacked = temp.path().join("unpacked");
    fs::create_dir(&unpacked)?;
    let unpack = run_canon(
        &home,
        vec![
            arg("package"),
            arg("unpack"),
            path_arg(&archive),
            arg("--target"),
            path_arg(&unpacked),
        ],
    )?;
    assert_exit(&unpack, 0);
    let unpack_json = json_stdout(&unpack)?;
    assert_eq!(
        unpack_json["package_content_digest"],
        prepared.package.content_digest
    );
    assert_eq!(unpack_json["verified_files"], 4);
    command_receipts.push(command_receipt(&unpack));

    let source_manifest = read_json(&unpacked.join("source_manifest.json"))?;
    assert_source_manifest_matches(&unpacked, &source_manifest)?;
    assert_eq!(
        source_manifest["package_digest"],
        prepared.package.content_digest
    );

    let source_registry = unpacked.join("registry");
    let unpacked_package = compile_registry_package(&source_registry)?;
    assert_eq!(
        unpacked_package.content_digest,
        prepared.package.content_digest
    );

    set_tree_readonly(&source_registry, true)?;
    let readonly_result = run_readonly_acceptance(
        &home,
        &source_registry,
        &prepared.package,
        &source_manifest,
        &mut command_receipts,
        temp.path(),
    );
    let restore_result = set_tree_readonly(&source_registry, false);
    restore_result?;
    readonly_result
}

fn run_readonly_acceptance(
    home: &Path,
    source_registry: &Path,
    package: &RegistryPackage,
    source_manifest: &Value,
    command_receipts: &mut Vec<Value>,
    temp_root: &Path,
) -> Result<(), Box<dyn Error>> {
    assert!(fs::metadata(source_registry)?.permissions().readonly());
    let source_before = tree_file_hashes(source_registry)?;
    let entries = fixture_entries(source_registry)?;
    let expected_aliases = expected_aliases(&entries);
    let selected = select_subjects(&entries, 4);
    let negative_subject = exact_negative_subject(&selected, &entries);

    let input_path = temp_root.join("runtime_selected_subjects.csv");
    write_runtime_input(&input_path, &selected, &negative_subject)?;
    let input_hash = hash_file(&input_path)?;

    let cold_lookup = run_lookup(home, source_registry, &input_path)?;
    assert_exit(&cold_lookup, 1);
    let cold_json = json_stdout(&cold_lookup)?;
    assert_lookup_output(&cold_json, &selected, &negative_subject);
    command_receipts.push(command_receipt(&cold_lookup));

    let warm_lookup = run_lookup(home, source_registry, &input_path)?;
    assert_exit(&warm_lookup, 1);
    assert_eq!(cold_lookup.stdout, warm_lookup.stdout);
    command_receipts.push(command_receipt(&warm_lookup));

    let run_a = temp_root.join("export-a");
    let run_b = temp_root.join("export-b");
    fs::create_dir_all(&run_a)?;
    fs::create_dir_all(&run_b)?;

    let dbt_a = export_dbt(home, source_registry, &run_a)?;
    let dbt_b = export_dbt(home, source_registry, &run_b)?;
    assert_exit(&dbt_a.run, 0);
    assert_exit(&dbt_b.run, 0);
    assert_eq!(dbt_a.json["content_hash"], dbt_b.json["content_hash"]);
    assert_eq!(fs::read(&dbt_a.seed_path)?, fs::read(&dbt_b.seed_path)?);
    assert_eq!(fs::read(&dbt_a.schema_path)?, fs::read(&dbt_b.schema_path)?);
    assert_eq!(fs::read(&dbt_a.test_path)?, fs::read(&dbt_b.test_path)?);
    command_receipts.push(command_receipt(&dbt_a.run));
    command_receipts.push(command_receipt(&dbt_b.run));

    let dbt_rows = dbt_seed_rows(&dbt_a.seed_path, package)?;
    assert_eq!(dbt_rows, expected_aliases);
    assert_no_dbt_normalized_key_collapse(&dbt_rows);

    let search_a = export_search(home, source_registry, &run_a)?;
    let search_b = export_search(home, source_registry, &run_b)?;
    assert_exit(&search_a.run, 0);
    assert_exit(&search_b.run, 0);
    assert_eq!(search_a.json["content_hash"], search_b.json["content_hash"]);
    assert_eq!(
        fs::read(&search_a.sqlite_path)?,
        fs::read(&search_b.sqlite_path)?
    );
    command_receipts.push(command_receipt(&search_a.run));
    command_receipts.push(command_receipt(&search_b.run));

    let search_rows = search_alias_rows(&search_a.sqlite_path)?;
    assert_eq!(search_rows, expected_aliases);
    assert_search_metadata_and_readonly_consumer(
        &search_a.sqlite_path,
        package,
        search_a
            .json
            .get("content_hash")
            .and_then(Value::as_str)
            .expect("search export content hash"),
        search_rows.len(),
    )?;

    let corrupt_cache_hashes = corrupt_managed_cache(home)?;
    let corrupt_recovered = run_lookup(home, source_registry, &input_path)?;
    assert_exit(&corrupt_recovered, 1);
    assert_eq!(cold_lookup.stdout, corrupt_recovered.stdout);
    command_receipts.push(command_receipt(&corrupt_recovered));

    let stale_cache_hashes = poison_managed_cache_with_stale_alias(home, &negative_subject)?;
    let stale_recovered = run_lookup(home, source_registry, &input_path)?;
    assert_exit(&stale_recovered, 1);
    assert_eq!(cold_lookup.stdout, stale_recovered.stdout);
    command_receipts.push(command_receipt(&stale_recovered));

    let concurrent = run_concurrent_lookups(home, source_registry, &input_path, 4)?;
    for run in &concurrent {
        assert_exit(run, 1);
        assert_eq!(cold_lookup.stdout, run.stdout);
        command_receipts.push(command_receipt(run));
    }

    assert_eq!(tree_file_hashes(source_registry)?, source_before);

    let receipt = json!({
        "version": "canon.registry.v1.acceptance_receipt.v1",
        "bead": "bd-26g5",
        "selection": {
            "seed": SELECTION_SEED,
            "algorithm": "blake3(seed, entry_order, input, canonical_id, canonical_type, rule_id) sorted ascending",
            "selected_subjects": selected.iter().map(|subject| {
                json!({
                    "input": subject.entry.input,
                    "canonical_id": subject.entry.canonical_id,
                    "canonical_type": subject.entry.canonical_type,
                    "rule_id": subject.entry.rule_id,
                    "source_file": subject.source_file,
                    "entry_order": subject.entry_order,
                    "selection_hash": subject.selection_hash,
                })
            }).collect::<Vec<_>>(),
            "negative_subject": negative_subject,
        },
        "source_manifest": source_manifest,
        "input_manifest": {
            "path": input_path.display().to_string(),
            "content_hash": input_hash,
            "row_count": selected.len() + 1,
        },
        "package": {
            "registry_id": package.registry.id,
            "registry_version": package.registry.version,
            "content_digest": package.content_digest,
            "effective_mapping_count": package.effective_mapping_count,
        },
        "exports": {
            "dbt_seed": {
                "content_hash": dbt_a.json["content_hash"],
                "file_hash": hash_file(&dbt_a.seed_path)?,
                "row_count": dbt_rows.len(),
            },
            "search_index": {
                "content_hash": search_a.json["content_hash"],
                "file_hash": hash_file(&search_a.sqlite_path)?,
                "row_count": search_rows.len(),
            },
        },
        "recovery_paths": [
            {
                "kind": "corrupt_managed_registry_cache",
                "corrupted_cache_hashes": corrupt_cache_hashes,
                "recovered_stdout_hash": hash_bytes(&corrupt_recovered.stdout),
            },
            {
                "kind": "stale_managed_registry_cache_extra_row",
                "poisoned_cache_hashes": stale_cache_hashes,
                "recovered_stdout_hash": hash_bytes(&stale_recovered.stdout),
            }
        ],
        "commands": command_receipts,
    });
    let receipt_path = temp_root.join("registry_acceptance_receipt.json");
    fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
    let retained_receipt = read_json(&receipt_path)?;
    assert_eq!(retained_receipt["selection"]["seed"], SELECTION_SEED);
    assert_eq!(
        retained_receipt["package"]["content_digest"],
        package.content_digest
    );
    assert_eq!(
        retained_receipt["input_manifest"]["content_hash"],
        hash_file(&input_path)?
    );
    assert_eq!(
        retained_receipt["recovery_paths"].as_array().unwrap().len(),
        2
    );
    assert!(
        retained_receipt["commands"]
            .as_array()
            .unwrap()
            .iter()
            .all(|command| command["argv"].as_array().unwrap()[0] == "canon"
                && command["duration_ms"].as_u64().is_some()
                && command["stdout_hash"]
                    .as_str()
                    .unwrap()
                    .starts_with("blake3:")
                && command["stderr_hash"]
                    .as_str()
                    .unwrap()
                    .starts_with("blake3:"))
    );

    Ok(())
}

struct PreparedRegistryPackage {
    package_root: PathBuf,
    package: RegistryPackage,
    package_bytes: Vec<u8>,
}

struct DbtExport {
    run: CommandRun,
    json: Value,
    seed_path: PathBuf,
    schema_path: PathBuf,
    test_path: PathBuf,
}

struct SearchExport {
    run: CommandRun,
    json: Value,
    sqlite_path: PathBuf,
}

fn prepare_packaged_registry(root: &Path) -> Result<PreparedRegistryPackage, Box<dyn Error>> {
    let package_root = root.join("package-root");
    let registry_dir = package_root.join("registry");
    fs::create_dir_all(&registry_dir)?;
    for name in ["registry.json", "mappings.json"] {
        fs::copy(fixture_dir().join(name), registry_dir.join(name))?;
    }

    let package = compile_registry_package(&registry_dir)?;
    let package_bytes = canonical_package_bytes(&package)?;
    let source_manifest = json!({
        "version": "canon.registry.acceptance.source_manifest.v1",
        "registry_subdir": "registry",
        "package_digest": package.content_digest,
        "files": source_file_receipts(&registry_dir, "registry")?,
    });
    fs::write(
        package_root.join("source_manifest.json"),
        serde_json::to_vec_pretty(&source_manifest)?,
    )?;
    fs::write(package_root.join("package.json"), &package_bytes)?;

    Ok(PreparedRegistryPackage {
        package_root,
        package,
        package_bytes,
    })
}

fn assert_package_declares_immutable_export_contracts(package: &RegistryPackage) {
    assert_eq!(
        package.identity.mapping_precedence,
        "filename_lexicographic_then_entry_order"
    );
    assert!(
        package
            .identity
            .identity_exclusions
            .iter()
            .any(|value| value == "absolute_paths")
    );
    assert!(
        package
            .identity
            .identity_exclusions
            .iter()
            .any(|value| value == "derived_caches")
    );
    for sidecar in [
        "audit",
        "gold",
        "strategy",
        "signature",
        "relation",
        "escrow",
    ] {
        assert!(
            package
                .allowed_sidecars
                .iter()
                .any(|value| value == sidecar)
        );
    }
    let projections = package
        .deployment_projections
        .iter()
        .map(|projection| {
            (
                projection.kind.as_str(),
                projection.first_class,
                projection.identity_excluded,
            )
        })
        .collect::<BTreeSet<_>>();
    assert!(projections.contains(&("dbt-seed", true, true)));
    assert!(projections.contains(&("search-index", true, true)));
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/canon_v1/registry_acceptance")
}

fn fixture_entries(registry_dir: &Path) -> Result<Vec<FixtureEntry>, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(
        registry_dir.join("mappings.json"),
    )?)?)
}

fn select_subjects(entries: &[FixtureEntry], count: usize) -> Vec<SelectedSubject> {
    let mut scored = entries
        .iter()
        .enumerate()
        .map(|(entry_order, entry)| SelectedSubject {
            entry: entry.clone(),
            source_file: "mappings.json".to_string(),
            entry_order,
            selection_hash: selection_hash(entry_order, entry),
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        left.selection_hash
            .cmp(&right.selection_hash)
            .then_with(|| left.entry_order.cmp(&right.entry_order))
    });
    scored.truncate(count);
    scored
}

fn selection_hash(entry_order: usize, entry: &FixtureEntry) -> String {
    let mut hasher = blake3::Hasher::new();
    for part in [
        SELECTION_SEED,
        &entry_order.to_string(),
        &entry.input,
        &entry.canonical_id,
        &entry.canonical_type,
        &entry.rule_id,
    ] {
        hasher.update(part.as_bytes());
        hasher.update(&[0]);
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

fn exact_negative_subject(selected: &[SelectedSubject], entries: &[FixtureEntry]) -> String {
    let all_inputs = entries
        .iter()
        .map(|entry| entry.input.clone())
        .collect::<BTreeSet<_>>();
    for subject in selected {
        let lower = subject.entry.input.to_ascii_lowercase();
        if lower != subject.entry.input && !all_inputs.contains(&lower) {
            return lower;
        }
        let compact = subject
            .entry
            .input
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>();
        if compact != subject.entry.input && !all_inputs.contains(&compact) {
            return compact;
        }
    }
    format!(
        "absent-{}",
        hash_bytes(SELECTION_SEED.as_bytes())
            .trim_start_matches("blake3:")
            .chars()
            .take(16)
            .collect::<String>()
    )
}

fn write_runtime_input(
    path: &Path,
    selected: &[SelectedSubject],
    negative_subject: &str,
) -> Result<(), Box<dyn Error>> {
    let mut writer = csv::Writer::from_path(path)?;
    writer.write_record(["row_id", "subject"])?;
    for (index, subject) in selected.iter().enumerate() {
        writer.write_record([format!("selected-{index}"), subject.entry.input.clone()])?;
    }
    writer.write_record(["negative-case".to_string(), negative_subject.to_string()])?;
    writer.flush()?;
    Ok(())
}

fn run_lookup(home: &Path, registry: &Path, input: &Path) -> Result<CommandRun, Box<dyn Error>> {
    run_canon(
        home,
        vec![
            path_arg(input),
            arg("--registry"),
            path_arg(registry),
            arg("--column"),
            arg("subject"),
            arg("--explicit"),
            arg("--plain-json-values"),
            arg("--no-witness"),
        ],
    )
}

fn export_dbt(home: &Path, registry: &Path, out_dir: &Path) -> Result<DbtExport, Box<dyn Error>> {
    let seed_path = out_dir.join("acceptance_seed.csv");
    let schema_path = out_dir.join("schema.yml");
    let test_path = out_dir.join("assert_acceptance_seed_no_collapse.sql");
    let run = run_canon(
        home,
        vec![
            arg("registry"),
            arg("export"),
            arg("--format"),
            arg("dbt-seed"),
            arg("--registry"),
            path_arg(registry),
            arg("--out"),
            path_arg(&seed_path),
            arg("--namespace"),
            arg(DBT_NAMESPACE),
            arg("--canonical-iri-prefix"),
            arg(CANONICAL_IRI_PREFIX),
            arg("--schema-out"),
            path_arg(&schema_path),
            arg("--anti-collapse-test-out"),
            path_arg(&test_path),
            arg("--emit"),
            arg("json"),
        ],
    )?;
    let json = json_stdout(&run)?;
    Ok(DbtExport {
        run,
        json,
        seed_path,
        schema_path,
        test_path,
    })
}

fn export_search(
    home: &Path,
    registry: &Path,
    out_dir: &Path,
) -> Result<SearchExport, Box<dyn Error>> {
    let sqlite_path = out_dir.join("acceptance_search.sqlite");
    let run = run_canon(
        home,
        vec![
            arg("registry"),
            arg("export"),
            arg("--format"),
            arg("search-index"),
            arg("--registry"),
            path_arg(registry),
            arg("--out"),
            path_arg(&sqlite_path),
            arg("--namespace"),
            arg(SEARCH_NAMESPACE),
            arg("--canonical-iri-prefix"),
            arg(CANONICAL_IRI_PREFIX),
            arg("--emit"),
            arg("json"),
        ],
    )?;
    let json = json_stdout(&run)?;
    Ok(SearchExport {
        run,
        json,
        sqlite_path,
    })
}

fn run_concurrent_lookups(
    home: &Path,
    registry: &Path,
    input: &Path,
    worker_count: usize,
) -> Result<Vec<CommandRun>, Box<dyn Error>> {
    let barrier = Arc::new(Barrier::new(worker_count + 1));
    let handles = (0..worker_count)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let home = home.to_path_buf();
            let registry = registry.to_path_buf();
            let input = input.to_path_buf();
            thread::spawn(move || {
                barrier.wait();
                run_lookup(&home, &registry, &input).map_err(|error| error.to_string())
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let mut runs = Vec::new();
    for handle in handles {
        runs.push(
            handle
                .join()
                .map_err(|_| io::Error::other("concurrent lookup worker panicked"))?
                .map_err(io::Error::other)?,
        );
    }
    Ok(runs)
}

fn run_canon(home: &Path, owned_args: Vec<String>) -> Result<CommandRun, Box<dyn Error>> {
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(&owned_args)
        .env("HOME", home)
        .env("CANON_REGISTRY_INDEX_MODE", "managed")
        .env_remove("OPENFIGI_API_KEY")
        .output()?;
    let duration_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    Ok(CommandRun {
        args: owned_args,
        exit_code: output.status.code().unwrap_or(-1),
        stdout: output.stdout,
        stderr: output.stderr,
        duration_ms,
    })
}

fn assert_exit(run: &CommandRun, expected: i32) {
    assert_eq!(
        run.exit_code,
        expected,
        "canon {:?} exited {}; stderr:\n{}",
        run.args,
        run.exit_code,
        String::from_utf8_lossy(&run.stderr)
    );
}

fn json_stdout(run: &CommandRun) -> Result<Value, Box<dyn Error>> {
    serde_json::from_slice(&run.stdout).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "stdout was not JSON for {:?}: {error}; stdout={}; stderr={}",
                run.args,
                String::from_utf8_lossy(&run.stdout),
                String::from_utf8_lossy(&run.stderr)
            ),
        )
        .into()
    })
}

fn command_receipt(run: &CommandRun) -> Value {
    let mut argv = vec!["canon".to_string()];
    argv.extend(run.args.clone());
    json!({
        "argv": argv,
        "exit_code": run.exit_code,
        "stdout_hash": hash_bytes(&run.stdout),
        "stderr_hash": hash_bytes(&run.stderr),
        "duration_ms": run.duration_ms,
    })
}

fn assert_lookup_output(value: &Value, selected: &[SelectedSubject], negative_subject: &str) {
    assert_eq!(value["version"], "canon.v0");
    assert_eq!(value["outcome"], "PARTIAL");
    assert_eq!(value["redacted"], false);
    assert_eq!(value["summary"]["total"], selected.len() + 1);
    assert_eq!(value["summary"]["resolved"], selected.len());
    assert_eq!(value["summary"]["unresolved"], 1);

    let mappings = value["mappings"]
        .as_array()
        .expect("mappings array")
        .iter()
        .map(|mapping| {
            (
                mapping["input"].as_str().unwrap().to_string(),
                (
                    mapping["canonical_id"].as_str().unwrap().to_string(),
                    mapping["canonical_type"].as_str().unwrap().to_string(),
                    mapping["rule_id"].as_str().unwrap().to_string(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for subject in selected {
        assert_eq!(
            mappings.get(&subject.entry.input),
            Some(&(
                subject.entry.canonical_id.clone(),
                subject.entry.canonical_type.clone(),
                subject.entry.rule_id.clone(),
            ))
        );
    }

    let unresolved = value["unresolved"].as_array().expect("unresolved array");
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0]["input"], negative_subject);
    assert_eq!(unresolved[0]["reason"], "no matching rule");
}

fn expected_aliases(entries: &[FixtureEntry]) -> BTreeSet<ExportedAlias> {
    let mut seen_inputs = BTreeSet::new();
    let mut aliases = BTreeSet::new();
    for (entry_order, entry) in entries.iter().enumerate() {
        if !seen_inputs.insert(entry.input.clone()) {
            continue;
        }
        aliases.insert(ExportedAlias {
            alias: entry.input.clone(),
            normalized_key: independent_search_key(&entry.input),
            canonical_id: entry.canonical_id.clone(),
            canonical_iri: independent_canonical_iri(&entry.canonical_id),
            canonical_type: entry.canonical_type.clone(),
            rule_id: entry.rule_id.clone(),
            match_source: independent_match_source(&entry.rule_id),
            source_file: "mappings.json".to_string(),
            entry_order,
        });
    }
    aliases
}

fn dbt_seed_rows(
    path: &Path,
    package: &RegistryPackage,
) -> Result<BTreeSet<ExportedAlias>, Box<dyn Error>> {
    let mut reader = csv::Reader::from_path(path)?;
    let headers = reader.headers()?.clone();
    let mut rows = BTreeSet::new();
    for record in reader.records() {
        let record = record?;
        assert_eq!(csv_field(&headers, &record, "namespace")?, DBT_NAMESPACE);
        assert_eq!(
            csv_field(&headers, &record, "registry_id")?,
            package.registry.id
        );
        assert_eq!(
            csv_field(&headers, &record, "registry_version")?,
            package.registry.version
        );
        assert_eq!(
            csv_field(&headers, &record, "registry_package_digest")?,
            package.content_digest
        );
        assert_eq!(
            csv_field(&headers, &record, "registry_package_schema_version")?,
            package.schema_version
        );
        rows.insert(ExportedAlias {
            alias: csv_field(&headers, &record, "source_input")?,
            normalized_key: csv_field(&headers, &record, "normalized_key")?,
            canonical_id: csv_field(&headers, &record, "canonical_id")?,
            canonical_iri: csv_field(&headers, &record, "canonical_iri")?,
            canonical_type: csv_field(&headers, &record, "canonical_type")?,
            rule_id: csv_field(&headers, &record, "rule_id")?,
            match_source: csv_field(&headers, &record, "match_source")?,
            source_file: csv_field(&headers, &record, "source_file")?,
            entry_order: csv_field(&headers, &record, "entry_order")?.parse()?,
        });
    }
    Ok(rows)
}

fn csv_field(
    headers: &csv::StringRecord,
    record: &csv::StringRecord,
    name: &str,
) -> Result<String, Box<dyn Error>> {
    let index = headers
        .iter()
        .position(|header| header == name)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("missing CSV header {name}"),
            )
        })?;
    Ok(record
        .get(index)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing CSV field"))?
        .to_string())
}

fn assert_no_dbt_normalized_key_collapse(rows: &BTreeSet<ExportedAlias>) {
    let mut by_key = BTreeMap::<String, BTreeSet<String>>::new();
    for row in rows {
        by_key
            .entry(row.normalized_key.clone())
            .or_default()
            .insert(row.canonical_id.clone());
    }
    assert!(
        by_key
            .values()
            .all(|canonical_ids| canonical_ids.len() == 1),
        "dbt anti-collapse consumer check found a normalized key with multiple canonical IDs"
    );
}

fn search_alias_rows(path: &Path) -> Result<BTreeSet<ExportedAlias>, Box<dyn Error>> {
    let conn = open_readonly_sqlite(path)?;
    let mut stmt = conn.prepare(
        "SELECT alias, normalized_key, canonical_id, canonical_iri, canonical_type, rule_id, match_source, source_file, entry_order
         FROM aliases
         ORDER BY alias, canonical_id, source_file, entry_order",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ExportedAlias {
                alias: row.get(0)?,
                normalized_key: row.get(1)?,
                canonical_id: row.get(2)?,
                canonical_iri: row.get(3)?,
                canonical_type: row.get(4)?,
                rule_id: row.get(5)?,
                match_source: row.get(6)?,
                source_file: row.get(7)?,
                entry_order: row.get::<_, i64>(8)? as usize,
            })
        })?
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(rows)
}

fn assert_search_metadata_and_readonly_consumer(
    path: &Path,
    package: &RegistryPackage,
    content_hash: &str,
    alias_count: usize,
) -> Result<(), Box<dyn Error>> {
    let conn = open_readonly_sqlite(path)?;
    let write_error = conn
        .execute(
            "INSERT INTO metadata (key, value) VALUES ('mutated', 'no')",
            [],
        )
        .unwrap_err();
    assert!(write_error.to_string().contains("readonly"));

    let metadata = metadata_map(&conn)?;
    assert_eq!(
        metadata.get("registry_package_digest"),
        Some(&package.content_digest)
    );
    assert_eq!(
        metadata.get("registry_package_id"),
        Some(&package.registry.id)
    );
    assert_eq!(
        metadata.get("registry_package_version"),
        Some(&package.registry.version)
    );
    assert_eq!(
        metadata.get("registry_package_schema_version"),
        Some(&package.schema_version)
    );
    assert_eq!(
        metadata.get("content_hash"),
        Some(&content_hash.to_string())
    );
    assert_eq!(
        metadata.get("generation_time_policy"),
        Some(&"deterministic_export_no_wall_clock".to_string())
    );
    assert_eq!(
        metadata.get("generated_at"),
        Some(&"1970-01-01T00:00:00Z".to_string())
    );

    let traced_rows: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM aliases
         CROSS JOIN metadata package
         WHERE package.key = 'registry_package_digest'
           AND package.value = ?1",
        params![package.content_digest],
        |row| row.get(0),
    )?;
    assert_eq!(traced_rows as usize, alias_count);

    let fts_rows: i64 = conn.query_row("SELECT COUNT(*) FROM aliases_fts", [], |row| row.get(0))?;
    assert_eq!(fts_rows as usize, alias_count);
    Ok(())
}

fn metadata_map(conn: &Connection) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut stmt = conn.prepare("SELECT key, value FROM metadata ORDER BY key")?;
    Ok(stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<BTreeMap<_, _>, _>>()?)
}

fn open_readonly_sqlite(path: &Path) -> Result<Connection, Box<dyn Error>> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?)
}

fn independent_search_key(value: &str) -> String {
    value
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_uppercase())
            } else {
                None
            }
        })
        .collect()
}

fn independent_canonical_iri(canonical_id: &str) -> String {
    if canonical_id.starts_with(CANONICAL_IRI_PREFIX)
        || canonical_id.starts_with("cmdrvl:")
        || canonical_id.starts_with("urn:")
        || canonical_id.contains("://")
    {
        canonical_id.to_string()
    } else {
        format!("{CANONICAL_IRI_PREFIX}{canonical_id}")
    }
}

fn independent_match_source(rule_id: &str) -> String {
    let rule = rule_id.to_ascii_uppercase();
    if rule.contains("BRAND") {
        "registry_brand".to_string()
    } else if rule.contains("CANON") {
        "canon".to_string()
    } else {
        "registry_exact".to_string()
    }
}

fn corrupt_managed_cache(home: &Path) -> Result<Vec<Value>, Box<dyn Error>> {
    let cache_files = registry_cache_files(home)?;
    assert!(
        !cache_files.is_empty(),
        "managed registry cache was not built"
    );
    let mut receipts = Vec::new();
    for path in cache_files {
        let before = hash_file(&path)?;
        fs::write(&path, b"not a sqlite registry cache")?;
        receipts.push(json!({
            "path": path.display().to_string(),
            "before_hash": before,
            "after_hash": hash_file(&path)?,
        }));
    }
    Ok(receipts)
}

fn poison_managed_cache_with_stale_alias(
    home: &Path,
    negative_subject: &str,
) -> Result<Vec<Value>, Box<dyn Error>> {
    let cache_files = registry_cache_files(home)?;
    assert!(
        !cache_files.is_empty(),
        "managed registry cache was not rebuilt"
    );
    let mut receipts = Vec::new();
    for path in cache_files {
        let before = hash_file(&path)?;
        let conn = Connection::open(&path)?;
        conn.execute(
            "INSERT INTO entries (input, canonical_id, canonical_type, rule_id, source_file, entry_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                negative_subject,
                "POISONED",
                "poison",
                "POISONED_CACHE",
                "poisoned-cache.json",
                0_i64,
            ],
        )?;
        drop(conn);
        receipts.push(json!({
            "path": path.display().to_string(),
            "before_hash": before,
            "after_hash": hash_file(&path)?,
        }));
    }
    Ok(receipts)
}

fn registry_cache_files(home: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let cache_dir = home.join(".cmdrvl").join("cache").join("registry-indexes");
    let mut files = Vec::new();
    for entry in fs::read_dir(&cache_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) == Some("sqlite") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn source_file_receipts(
    registry_dir: &Path,
    package_relative_root: &str,
) -> Result<Vec<SourceFileReceipt>, Box<dyn Error>> {
    let mut receipts = Vec::new();
    for name in ["registry.json", "mappings.json"] {
        let path = registry_dir.join(name);
        receipts.push(SourceFileReceipt {
            path: format!("{package_relative_root}/{name}"),
            content_hash: hash_file(&path)?,
            bytes: fs::metadata(&path)?.len(),
        });
    }
    Ok(receipts)
}

fn assert_source_manifest_matches(
    package_root: &Path,
    manifest: &Value,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        manifest["version"],
        "canon.registry.acceptance.source_manifest.v1"
    );
    for file in manifest["files"].as_array().expect("manifest files") {
        let relative = file["path"].as_str().expect("file path");
        let path = package_root.join(relative);
        assert_eq!(file["content_hash"], hash_file(&path)?);
        assert_eq!(file["bytes"], fs::metadata(&path)?.len());
    }
    Ok(())
}

fn tree_file_hashes(root: &Path) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut hashes = BTreeMap::new();
    collect_file_hashes(root, root, &mut hashes)?;
    Ok(hashes)
}

fn collect_file_hashes(
    root: &Path,
    path: &Path,
    hashes: &mut BTreeMap<String, String>,
) -> Result<(), Box<dyn Error>> {
    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_file_hashes(root, &entry_path, hashes)?;
        } else if entry_path.is_file() {
            let relative = entry_path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            hashes.insert(relative, hash_file(&entry_path)?);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_tree_readonly(root: &Path, readonly: bool) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut dirs = vec![root.to_path_buf()];
    let mut files = Vec::new();
    collect_tree_paths(root, &mut dirs, &mut files)?;
    if readonly {
        for file in &files {
            fs::set_permissions(file, fs::Permissions::from_mode(0o444))?;
        }
        for dir in dirs.iter().rev() {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o555))?;
        }
    } else {
        for dir in &dirs {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o755))?;
        }
        for file in &files {
            fs::set_permissions(file, fs::Permissions::from_mode(0o644))?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_tree_readonly(root: &Path, readonly: bool) -> io::Result<()> {
    let mut dirs = vec![root.to_path_buf()];
    let mut files = Vec::new();
    collect_tree_paths(root, &mut dirs, &mut files)?;
    for path in dirs.iter().chain(files.iter()) {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(readonly);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn collect_tree_paths(
    path: &Path,
    dirs: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry_path = entry?.path();
        if entry_path.is_dir() {
            dirs.push(entry_path.clone());
            collect_tree_paths(&entry_path, dirs, files)?;
        } else if entry_path.is_file() {
            files.push(entry_path);
        }
    }
    dirs.sort();
    files.sort();
    Ok(())
}

fn read_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn hash_file(path: &Path) -> Result<String, Box<dyn Error>> {
    Ok(hash_bytes(&fs::read(path)?))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn path_arg(path: &Path) -> String {
    path.display().to_string()
}

fn arg(value: &str) -> String {
    value.to_string()
}
