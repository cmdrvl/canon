use super::{
    GoldScore, MatchRecord, ResolveError, ResolveErrorCode, ResolveResult, ResolveStrategy,
    WriteBackSummary,
};
use crate::{Registry, registry::load_registry};
use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone)]
pub struct WriteBackRequest<'a> {
    pub registry_dir: &'a Path,
    pub strategy: &'a ResolveStrategy,
    pub matches: &'a [MatchRecord],
    pub gold_score: Option<&'a GoldScore>,
    pub write_back: bool,
    pub mapping_file_name: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveMappingEntry {
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExistingMapping {
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
}

pub fn write_back_matches(request: WriteBackRequest<'_>) -> ResolveResult<WriteBackSummary> {
    if !request.write_back {
        return Ok(WriteBackSummary {
            requested: false,
            written: false,
            entry_count: 0,
            mapping_file: None,
        });
    }

    if let Some(gold_score) = request.gold_score
        && !gold_score.regressions.is_empty()
    {
        return Ok(WriteBackSummary {
            requested: true,
            written: false,
            entry_count: 0,
            mapping_file: None,
        });
    }

    if request.matches.is_empty() {
        return Ok(WriteBackSummary {
            requested: true,
            written: false,
            entry_count: 0,
            mapping_file: None,
        });
    }

    let registry = load_registry_for_writeback(request.registry_dir)?;
    let entries = planned_entries(&registry, request.strategy, request.matches)?;
    if entries.is_empty() {
        return Ok(WriteBackSummary {
            requested: true,
            written: false,
            entry_count: 0,
            mapping_file: None,
        });
    }

    let mapping_file = request
        .mapping_file_name
        .map(str::to_string)
        .unwrap_or_else(default_mapping_file_name);
    validate_mapping_file_name(&mapping_file)?;
    let mapping_path = request.registry_dir.join(&mapping_file);
    if mapping_path.exists() {
        return Err(writeback_error(
            format!(
                "Resolve write-back mapping file '{}' already exists",
                mapping_path.display()
            ),
            json!({
                "mapping_file": mapping_file,
                "registry": request.registry_dir.display().to_string()
            }),
        ));
    }

    let bytes = serde_json::to_vec_pretty(&entries).map_err(|error| {
        writeback_error(
            format!("Failed to serialize resolve write-back entries: {error}"),
            json!({ "error": error.to_string() }),
        )
    })?;
    fs::write(&mapping_path, bytes).map_err(|error| {
        writeback_error(
            format!(
                "Failed to write resolve mapping file '{}': {}",
                mapping_path.display(),
                error
            ),
            json!({
                "mapping_file": mapping_file,
                "error": error.to_string()
            }),
        )
    })?;

    append_entries_to_index(&registry, &mapping_file, &entries)?;

    // Re-open through the normal registry loader so the derived SQLite index is
    // rebuilt when needed and subsequent canon lookup sees the new entries.
    load_registry_for_writeback(request.registry_dir)?;

    Ok(WriteBackSummary {
        requested: true,
        written: true,
        entry_count: entries.len(),
        mapping_file: Some(mapping_file),
    })
}

fn planned_entries(
    registry: &Registry,
    strategy: &ResolveStrategy,
    matches: &[MatchRecord],
) -> ResolveResult<Vec<ResolveMappingEntry>> {
    let canonical_type = canonical_type(strategy);
    let mut planned = BTreeMap::<String, ResolveMappingEntry>::new();

    for record in matches {
        let reference_canonical_id = match lookup_existing(registry, &record.reference_id)? {
            Some(existing) => existing.canonical_id,
            None => {
                let canonical_id = record.reference_id.clone();
                insert_planned(
                    &mut planned,
                    ResolveMappingEntry {
                        input: record.reference_id.clone(),
                        canonical_id: canonical_id.clone(),
                        canonical_type: canonical_type.clone(),
                        rule_id: "IDENTITY:reference".to_string(),
                    },
                )?;
                canonical_id
            }
        };

        match lookup_existing(registry, &record.target_id)? {
            Some(existing) if existing.canonical_id == reference_canonical_id => {}
            Some(existing) => {
                return Err(writeback_error(
                    format!(
                        "Resolve write-back refuses to overwrite existing mapping for '{}'",
                        record.target_id
                    ),
                    json!({
                        "input": record.target_id,
                        "existing_canonical_id": existing.canonical_id,
                        "new_canonical_id": reference_canonical_id,
                        "existing_canonical_type": existing.canonical_type,
                        "existing_rule_id": existing.rule_id
                    }),
                ));
            }
            None => insert_planned(
                &mut planned,
                ResolveMappingEntry {
                    input: record.target_id.clone(),
                    canonical_id: reference_canonical_id,
                    canonical_type: canonical_type.clone(),
                    rule_id: format!("STRUCTURAL_MATCH:{}", strategy.id),
                },
            )?,
        }
    }

    Ok(planned.into_values().collect())
}

fn insert_planned(
    planned: &mut BTreeMap<String, ResolveMappingEntry>,
    entry: ResolveMappingEntry,
) -> ResolveResult<()> {
    if let Some(existing) = planned.get(&entry.input) {
        if existing == &entry {
            return Ok(());
        }
        return Err(writeback_error(
            format!(
                "Resolve write-back planned conflicting entries for '{}'",
                entry.input
            ),
            json!({
                "input": entry.input,
                "left_canonical_id": existing.canonical_id,
                "right_canonical_id": entry.canonical_id
            }),
        ));
    }

    planned.insert(entry.input.clone(), entry);
    Ok(())
}

fn lookup_existing(registry: &Registry, input: &str) -> ResolveResult<Option<ExistingMapping>> {
    let connection = Connection::open(&registry.db_path).map_err(|error| {
        writeback_error(
            format!("Cannot open registry index for write-back checks: {error}"),
            json!({
                "registry": registry.meta.source,
                "error": error.to_string()
            }),
        )
    })?;
    let mut statement = connection
        .prepare(
            "SELECT canonical_id, canonical_type, rule_id
             FROM entries
             WHERE input = ?1
             ORDER BY source_file ASC, entry_order ASC
             LIMIT 1",
        )
        .map_err(|error| {
            writeback_error(
                format!("Cannot prepare registry lookup for write-back checks: {error}"),
                json!({ "error": error.to_string() }),
            )
        })?;

    match statement.query_row(params![input], |row| {
        Ok(ExistingMapping {
            canonical_id: row.get(0)?,
            canonical_type: row.get(1)?,
            rule_id: row.get(2)?,
        })
    }) {
        Ok(existing) => Ok(Some(existing)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(writeback_error(
            format!("Registry lookup failed for write-back input '{input}': {error}"),
            json!({
                "input": input,
                "error": error.to_string()
            }),
        )),
    }
}

fn append_entries_to_index(
    registry: &Registry,
    mapping_file: &str,
    entries: &[ResolveMappingEntry],
) -> ResolveResult<()> {
    let mut connection = Connection::open(&registry.db_path).map_err(|error| {
        writeback_error(
            format!("Cannot open registry index to append resolve write-back entries: {error}"),
            json!({
                "registry": registry.meta.source,
                "error": error.to_string()
            }),
        )
    })?;
    let transaction = connection.transaction().map_err(|error| {
        writeback_error(
            format!("Cannot start registry index update transaction: {error}"),
            json!({ "error": error.to_string() }),
        )
    })?;

    {
        let mut statement = transaction
            .prepare(
                "INSERT INTO entries
                 (input, canonical_id, canonical_type, rule_id, source_file, entry_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|error| {
                writeback_error(
                    format!("Cannot prepare registry index append: {error}"),
                    json!({ "error": error.to_string() }),
                )
            })?;

        for (entry_order, entry) in entries.iter().enumerate() {
            statement
                .execute(params![
                    entry.input,
                    entry.canonical_id,
                    entry.canonical_type,
                    entry.rule_id,
                    mapping_file,
                    entry_order as i64,
                ])
                .map_err(|error| {
                    writeback_error(
                        format!(
                            "Cannot append resolve entry '{}' to registry index: {error}",
                            entry.input
                        ),
                        json!({
                            "input": entry.input,
                            "mapping_file": mapping_file,
                            "error": error.to_string()
                        }),
                    )
                })?;
        }
    }

    transaction.commit().map_err(|error| {
        writeback_error(
            format!("Cannot commit registry index append: {error}"),
            json!({ "error": error.to_string() }),
        )
    })
}

fn load_registry_for_writeback(registry_dir: &Path) -> ResolveResult<Registry> {
    load_registry(registry_dir).map_err(|error| {
        ResolveError::with_detail(
            ResolveErrorCode::Registry,
            format!(
                "Cannot load registry '{}' for resolve write-back: {}",
                registry_dir.display(),
                error
            ),
            json!({
                "registry": registry_dir.display().to_string(),
                "error": error.to_string()
            }),
        )
    })
}

fn canonical_type(strategy: &ResolveStrategy) -> String {
    if strategy.entity_type.ends_with("_id") {
        strategy.entity_type.clone()
    } else {
        format!("{}_id", strategy.entity_type)
    }
}

fn default_mapping_file_name() -> String {
    format!("resolve-matches-{}.json", Utc::now().format("%Y%m%d"))
}

fn validate_mapping_file_name(file_name: &str) -> ResolveResult<()> {
    let path = Path::new(file_name);
    if path.components().count() != 1
        || path.file_name().and_then(|name| name.to_str()) != Some(file_name)
        || !file_name.ends_with(".json")
        || matches!(file_name, "registry.json" | "_build.json")
    {
        return Err(writeback_error(
            format!("Invalid resolve write-back mapping file name '{file_name}'"),
            json!({ "mapping_file": file_name }),
        ));
    }
    Ok(())
}

fn writeback_error(message: impl Into<String>, detail: serde_json::Value) -> ResolveError {
    ResolveError::with_detail(ResolveErrorCode::WriteBack, message, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        InputFormat, InputValues, Mapping,
        lookup::resolve_values,
        resolve::{ResolveIdentity, ResolveIdentitySide},
    };
    use serde_json::json;
    use std::{collections::HashMap, path::Path};
    use tempfile::TempDir;

    fn strategy() -> ResolveStrategy {
        ResolveStrategy {
            id: "cmbs-loan-match.v1".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "loan".to_string(),
            identity: ResolveIdentity {
                reference: ResolveIdentitySide {
                    id_columns: vec!["loan_id".to_string()],
                },
                target: ResolveIdentitySide {
                    id_columns: vec!["deal".to_string(), "loan_number".to_string()],
                },
            },
            candidate_filter: vec![],
            assertions: vec![],
            match_threshold: 0.75,
            ambiguity_gap: 0.15,
            max_candidates: None,
            description: String::new(),
            content_hash: String::new(),
        }
    }

    fn match_record(reference_id: &str, target_id: &str) -> MatchRecord {
        MatchRecord {
            reference_id: reference_id.to_string(),
            target_id: target_id.to_string(),
            canonical_id: reference_id.to_string(),
            score: 1.0,
            assertions: vec![],
            runner_up: None,
        }
    }

    fn gold_score(regressions: Vec<&str>) -> GoldScore {
        GoldScore {
            total: 1,
            correct: usize::from(regressions.is_empty()),
            incorrect: regressions.len(),
            unmatched_in_gold: 0,
            accuracy: if regressions.is_empty() { 1.0 } else { 0.0 },
            regressions: regressions.into_iter().map(str::to_string).collect(),
        }
    }

    fn write_registry_json(path: &Path) {
        std::fs::write(
            path.join("registry.json"),
            r#"{
  "id": "resolve-loans",
  "version": "0.1.0",
  "description": "Resolve write-back test registry",
  "updated": "2026-05-06",
  "entry_count": 0
}"#,
        )
        .unwrap();
    }

    fn write_mapping(path: &Path, name: &str, entries: serde_json::Value) {
        std::fs::write(
            path.join(name),
            serde_json::to_string_pretty(&entries).unwrap(),
        )
        .unwrap();
    }

    fn temp_registry(existing_entries: serde_json::Value) -> TempDir {
        let temp_dir = tempfile::tempdir().unwrap();
        write_registry_json(temp_dir.path());
        if !existing_entries.as_array().unwrap().is_empty() {
            write_mapping(temp_dir.path(), "existing.json", existing_entries);
        }
        temp_dir
    }

    fn request<'a>(
        registry_dir: &'a Path,
        strategy: &'a ResolveStrategy,
        matches: &'a [MatchRecord],
        gold_score: Option<&'a GoldScore>,
        write_back: bool,
    ) -> WriteBackRequest<'a> {
        WriteBackRequest {
            registry_dir,
            strategy,
            matches,
            gold_score,
            write_back,
            mapping_file_name: Some("resolve-test.json"),
        }
    }

    #[test]
    fn writeback_is_skipped_without_explicit_flag() {
        let strategy = strategy();
        let matches = vec![match_record("223232", "WFCM2019-C50|1")];
        let summary = write_back_matches(request(
            Path::new("missing-registry"),
            &strategy,
            &matches,
            None,
            false,
        ))
        .unwrap();

        assert!(!summary.requested);
        assert!(!summary.written);
        assert_eq!(summary.entry_count, 0);
        assert!(summary.mapping_file.is_none());
    }

    #[test]
    fn writeback_with_gold_regressions_writes_nothing() {
        let temp_dir = temp_registry(json!([]));
        let strategy = strategy();
        let matches = vec![match_record("223232", "WFCM2019-C50|1")];
        let gold = gold_score(vec!["WFCM2019-C50|1"]);

        let summary = write_back_matches(request(
            temp_dir.path(),
            &strategy,
            &matches,
            Some(&gold),
            true,
        ))
        .unwrap();

        assert!(summary.requested);
        assert!(!summary.written);
        assert_eq!(summary.entry_count, 0);
        assert!(!temp_dir.path().join("resolve-test.json").exists());
    }

    #[test]
    fn clean_writeback_writes_reference_and_target_entries() {
        let temp_dir = temp_registry(json!([]));
        let strategy = strategy();
        let matches = vec![match_record("223232", "WFCM2019-C50|1")];
        let gold = gold_score(vec![]);

        let summary = write_back_matches(request(
            temp_dir.path(),
            &strategy,
            &matches,
            Some(&gold),
            true,
        ))
        .unwrap();

        assert!(summary.written);
        assert_eq!(summary.entry_count, 2);
        assert_eq!(summary.mapping_file.as_deref(), Some("resolve-test.json"));

        let entries = read_entries(&temp_dir.path().join("resolve-test.json"));
        assert_eq!(
            entries,
            vec![
                ResolveMappingEntry {
                    input: "223232".to_string(),
                    canonical_id: "223232".to_string(),
                    canonical_type: "loan_id".to_string(),
                    rule_id: "IDENTITY:reference".to_string(),
                },
                ResolveMappingEntry {
                    input: "WFCM2019-C50|1".to_string(),
                    canonical_id: "223232".to_string(),
                    canonical_type: "loan_id".to_string(),
                    rule_id: "STRUCTURAL_MATCH:cmbs-loan-match.v1".to_string(),
                },
            ]
        );
    }

    #[test]
    fn written_target_id_resolves_via_normal_lookup() {
        let temp_dir = temp_registry(json!([]));
        let strategy = strategy();
        let matches = vec![match_record("223232", "WFCM2019-C50|1")];
        write_back_matches(request(temp_dir.path(), &strategy, &matches, None, true)).unwrap();

        let registry = load_registry(temp_dir.path()).unwrap();
        let resolved = resolve_values(
            &registry,
            &InputValues {
                values: HashMap::from([("WFCM2019-C50|1".to_string(), ())]),
                special: HashMap::new(),
                format: InputFormat::Csv,
                delimiter: Some(b','),
                source_hash: None,
                source_bytes: None,
            },
        )
        .unwrap();

        assert_eq!(
            resolved.mappings,
            vec![Mapping {
                input: "WFCM2019-C50|1".to_string(),
                canonical_id: "223232".to_string(),
                canonical_type: "loan_id".to_string(),
                rule_id: "STRUCTURAL_MATCH:cmbs-loan-match.v1".to_string(),
                confidence: "deterministic".to_string(),
            }]
        );
    }

    #[test]
    fn existing_conflicting_target_mapping_is_refused() {
        let temp_dir = temp_registry(json!([
            {
                "input": "WFCM2019-C50|1",
                "canonical_id": "OTHER",
                "canonical_type": "loan_id",
                "rule_id": "OLD"
            }
        ]));
        let strategy = strategy();
        let matches = vec![match_record("223232", "WFCM2019-C50|1")];

        let error = write_back_matches(request(temp_dir.path(), &strategy, &matches, None, true))
            .unwrap_err();

        assert_eq!(error.code, ResolveErrorCode::WriteBack);
        assert!(error.message.contains("refuses to overwrite"));
        assert!(!temp_dir.path().join("resolve-test.json").exists());
    }

    #[test]
    fn existing_reference_canonical_id_is_used_for_target_mapping() {
        let temp_dir = temp_registry(json!([
            {
                "input": "223232",
                "canonical_id": "CANON-223232",
                "canonical_type": "loan_id",
                "rule_id": "EXISTING"
            }
        ]));
        let strategy = strategy();
        let matches = vec![match_record("223232", "WFCM2019-C50|1")];

        write_back_matches(request(temp_dir.path(), &strategy, &matches, None, true)).unwrap();

        let entries = read_entries(&temp_dir.path().join("resolve-test.json"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].input, "WFCM2019-C50|1");
        assert_eq!(entries[0].canonical_id, "CANON-223232");
        assert_eq!(entries[0].rule_id, "STRUCTURAL_MATCH:cmbs-loan-match.v1");
    }

    #[test]
    fn structural_attributes_are_not_written_to_registry_file() {
        let temp_dir = temp_registry(json!([]));
        let strategy = strategy();
        let mut match_record = match_record("223232", "WFCM2019-C50|1");
        match_record.assertions = vec![super::super::AssertionResult {
            field_ref: "upb".to_string(),
            field_tgt: "balance".to_string(),
            op: "tolerance_pct".to_string(),
            passed: true,
            score: 1.0,
            weight: 1.0,
            required: false,
            detail: [("ref_val".to_string(), json!(2450000))]
                .into_iter()
                .collect(),
        }];
        let matches = vec![match_record];

        write_back_matches(request(temp_dir.path(), &strategy, &matches, None, true)).unwrap();
        let content = std::fs::read_to_string(temp_dir.path().join("resolve-test.json")).unwrap();

        assert!(!content.contains("upb"));
        assert!(!content.contains("balance"));
        assert!(!content.contains("2450000"));
    }

    #[test]
    fn existing_equivalent_target_mapping_is_idempotent_noop() {
        let temp_dir = temp_registry(json!([
            {
                "input": "223232",
                "canonical_id": "223232",
                "canonical_type": "loan_id",
                "rule_id": "IDENTITY:reference"
            },
            {
                "input": "WFCM2019-C50|1",
                "canonical_id": "223232",
                "canonical_type": "loan_id",
                "rule_id": "STRUCTURAL_MATCH:cmbs-loan-match.v1"
            }
        ]));
        let strategy = strategy();
        let matches = vec![match_record("223232", "WFCM2019-C50|1")];

        let summary =
            write_back_matches(request(temp_dir.path(), &strategy, &matches, None, true)).unwrap();

        assert!(summary.requested);
        assert!(!summary.written);
        assert_eq!(summary.entry_count, 0);
        assert!(!temp_dir.path().join("resolve-test.json").exists());
    }

    fn read_entries(path: &Path) -> Vec<ResolveMappingEntry> {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }
}
