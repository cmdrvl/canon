#[cfg(test)]
use crate::SpecialReason;
use crate::{InputValues, Mapping, Registry, ResolveResult, Summary, UnresolvedEntry};
use rusqlite::{Connection, params};

/// Resolve input values against registry and return mapping results
pub fn resolve_values(
    registry: &Registry,
    input_values: &InputValues,
) -> Result<ResolveResult, LookupError> {
    // Open SQLite connection
    let conn = Connection::open(&registry.db_path)
        .map_err(|e| LookupError::Database(format!("Cannot open registry database: {}", e)))?;

    // Resolve regular values
    let mut mappings = Vec::new();
    let mut unresolved = Vec::new();

    if !input_values.values.is_empty() {
        // Prepare statement for efficient querying
        let mut stmt = conn
            .prepare(
                "SELECT canonical_id, canonical_type, rule_id
                 FROM entries
                 WHERE input = ?1
                 ORDER BY source_file ASC, entry_order ASC
                 LIMIT 1",
            )
            .map_err(|e| LookupError::Database(format!("Cannot prepare query statement: {}", e)))?;

        // Query each unique input value
        for input_value in input_values.values.keys() {
            match stmt.query_row(params![input_value], |row| {
                Ok((
                    row.get::<_, String>(0)?, // canonical_id
                    row.get::<_, String>(1)?, // canonical_type
                    row.get::<_, String>(2)?, // rule_id
                ))
            }) {
                Ok((canonical_id, canonical_type, rule_id)) => {
                    mappings.push(Mapping {
                        input: input_value.clone(),
                        canonical_id,
                        canonical_type,
                        rule_id,
                        confidence: "deterministic".to_string(),
                    });
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    unresolved.push(UnresolvedEntry {
                        input: Some(input_value.clone()),
                        reason: "no matching rule".to_string(),
                    });
                }
                Err(e) => {
                    return Err(LookupError::Database(format!(
                        "Query error for '{}': {}",
                        input_value, e
                    )));
                }
            }
        }
    }

    // Handle special reasons
    for (special_reason, count) in &input_values.special {
        if *count > 0 {
            unresolved.push(UnresolvedEntry {
                input: None,
                reason: special_reason.to_string(),
            });
        }
    }

    // Build summary
    let resolved = mappings.len();
    let unresolved_count = unresolved.len();
    let total = resolved + unresolved_count;

    let summary = Summary {
        total,
        resolved,
        unresolved: unresolved_count,
    };

    // Verify invariant
    if total != resolved + unresolved_count {
        return Err(LookupError::Invariant(format!(
            "Summary invariant violated: total ({}) != resolved ({}) + unresolved ({})",
            total, resolved, unresolved_count
        )));
    }

    Ok(ResolveResult {
        mappings,
        unresolved,
        summary,
    })
}

/// Lookup engine errors
#[derive(Debug)]
pub enum LookupError {
    Database(String),
    Invariant(String),
}

impl std::fmt::Display for LookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LookupError::Database(msg) => write!(f, "Database error: {}", msg),
            LookupError::Invariant(msg) => write!(f, "Invariant violation: {}", msg),
        }
    }
}

impl std::error::Error for LookupError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InputFormat, RegistryMeta};
    use rusqlite::{Connection, params};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn create_test_registry_db() -> (Registry, NamedTempFile) {
        let temp_db = NamedTempFile::with_suffix(".sqlite").unwrap();
        let conn = Connection::open(&temp_db).unwrap();

        // Create tables
        conn.execute(
            "CREATE TABLE entries (
                input TEXT,
                canonical_id TEXT,
                canonical_type TEXT,
                rule_id TEXT,
                source_file TEXT,
                entry_order INTEGER
            )",
            [],
        )
        .unwrap();

        // Insert test data with precedence order
        let entries = vec![
            (
                "AAPL",
                "037833100",
                "cusip",
                "TICKER_TO_CUSIP",
                "a_file.json",
                0,
            ),
            (
                "MSFT",
                "594918104",
                "cusip",
                "TICKER_TO_CUSIP",
                "a_file.json",
                1,
            ),
            (
                "GOOGL",
                "02079K305",
                "cusip",
                "TICKER_TO_CUSIP",
                "b_file.json",
                0,
            ),
            // Duplicate entry in different file (should be ignored due to precedence)
            ("AAPL", "DIFFERENT", "other", "OTHER_RULE", "z_file.json", 0),
        ];

        for (input, canonical_id, canonical_type, rule_id, source_file, entry_order) in entries {
            conn.execute(
                "INSERT INTO entries (input, canonical_id, canonical_type, rule_id, source_file, entry_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![input, canonical_id, canonical_type, rule_id, source_file, entry_order],
            ).unwrap();
        }

        let registry = Registry {
            meta: RegistryMeta {
                id: "test".to_string(),
                version: "1.0.0".to_string(),
                source: "test".to_string(),
            },
            db_path: PathBuf::from(temp_db.path()),
        };

        (registry, temp_db)
    }

    fn create_test_input_values(
        values: Vec<&str>,
        special: Vec<(SpecialReason, usize)>,
    ) -> InputValues {
        let mut values_map = HashMap::new();
        for value in values {
            values_map.insert(value.to_string(), ());
        }

        let mut special_map = HashMap::new();
        for (reason, count) in special {
            special_map.insert(reason, count);
        }

        InputValues {
            values: values_map,
            special: special_map,
            format: InputFormat::Csv,
            delimiter: Some(b','),
        }
    }

    #[test]
    fn test_resolve_all_found() {
        let (registry, _temp_db) = create_test_registry_db();
        let input_values = create_test_input_values(vec!["AAPL", "MSFT"], vec![]);

        let result = resolve_values(&registry, &input_values).unwrap();

        assert_eq!(result.mappings.len(), 2);
        assert_eq!(result.unresolved.len(), 0);
        assert_eq!(result.summary.total, 2);
        assert_eq!(result.summary.resolved, 2);
        assert_eq!(result.summary.unresolved, 0);

        // Check specific mappings
        let aapl_mapping = result.mappings.iter().find(|m| m.input == "AAPL").unwrap();
        assert_eq!(aapl_mapping.canonical_id, "037833100");
        assert_eq!(aapl_mapping.canonical_type, "cusip");
        assert_eq!(aapl_mapping.rule_id, "TICKER_TO_CUSIP");
        assert_eq!(aapl_mapping.confidence, "deterministic");
    }

    #[test]
    fn test_resolve_partial() {
        let (registry, _temp_db) = create_test_registry_db();
        let input_values = create_test_input_values(vec!["AAPL", "UNKNOWN"], vec![]);

        let result = resolve_values(&registry, &input_values).unwrap();

        assert_eq!(result.mappings.len(), 1);
        assert_eq!(result.unresolved.len(), 1);
        assert_eq!(result.summary.total, 2);
        assert_eq!(result.summary.resolved, 1);
        assert_eq!(result.summary.unresolved, 1);

        // Check resolved
        let mapping = &result.mappings[0];
        assert_eq!(mapping.input, "AAPL");
        assert_eq!(mapping.canonical_id, "037833100");

        // Check unresolved
        let unresolved = &result.unresolved[0];
        assert_eq!(unresolved.input, Some("UNKNOWN".to_string()));
        assert_eq!(unresolved.reason, "no matching rule");
    }

    #[test]
    fn test_resolve_all_unresolved() {
        let (registry, _temp_db) = create_test_registry_db();
        let input_values = create_test_input_values(vec!["UNKNOWN1", "UNKNOWN2"], vec![]);

        let result = resolve_values(&registry, &input_values).unwrap();

        assert_eq!(result.mappings.len(), 0);
        assert_eq!(result.unresolved.len(), 2);
        assert_eq!(result.summary.total, 2);
        assert_eq!(result.summary.resolved, 0);
        assert_eq!(result.summary.unresolved, 2);

        // All should be "no matching rule"
        for unresolved in &result.unresolved {
            assert!(unresolved.input.is_some());
            assert_eq!(unresolved.reason, "no matching rule");
        }
    }

    #[test]
    fn test_resolve_with_special_reasons() {
        let (registry, _temp_db) = create_test_registry_db();
        let input_values = create_test_input_values(
            vec!["AAPL"],
            vec![
                (SpecialReason::EmptyValue, 5),
                (SpecialReason::NullValue, 2),
                (SpecialReason::MissingField, 1),
            ],
        );

        let result = resolve_values(&registry, &input_values).unwrap();

        assert_eq!(result.mappings.len(), 1);
        assert_eq!(result.unresolved.len(), 3); // 3 special reasons
        assert_eq!(result.summary.total, 4); // 1 resolved + 3 unresolved
        assert_eq!(result.summary.resolved, 1);
        assert_eq!(result.summary.unresolved, 3);

        // Check that special reasons have input: None
        let special_unresolved: Vec<_> = result
            .unresolved
            .iter()
            .filter(|u| u.input.is_none())
            .collect();
        assert_eq!(special_unresolved.len(), 3);

        // Check specific special reasons
        let reasons: Vec<&str> = result
            .unresolved
            .iter()
            .map(|u| u.reason.as_str())
            .collect();
        assert!(reasons.contains(&"empty_value"));
        assert!(reasons.contains(&"null_value"));
        assert!(reasons.contains(&"missing_field"));
    }

    #[test]
    fn test_precedence_first_match_wins() {
        let (registry, _temp_db) = create_test_registry_db();
        let input_values = create_test_input_values(vec!["AAPL"], vec![]);

        let result = resolve_values(&registry, &input_values).unwrap();

        assert_eq!(result.mappings.len(), 1);

        // Should get the first match (from a_file.json), not the second (from z_file.json)
        let mapping = &result.mappings[0];
        assert_eq!(mapping.input, "AAPL");
        assert_eq!(mapping.canonical_id, "037833100"); // Not "DIFFERENT"
        assert_eq!(mapping.rule_id, "TICKER_TO_CUSIP"); // Not "OTHER_RULE"
    }

    #[test]
    fn test_empty_input() {
        let (registry, _temp_db) = create_test_registry_db();
        let input_values = create_test_input_values(vec![], vec![]);

        let result = resolve_values(&registry, &input_values).unwrap();

        assert_eq!(result.mappings.len(), 0);
        assert_eq!(result.unresolved.len(), 0);
        assert_eq!(result.summary.total, 0);
        assert_eq!(result.summary.resolved, 0);
        assert_eq!(result.summary.unresolved, 0);
    }

    #[test]
    fn test_summary_invariant() {
        let (registry, _temp_db) = create_test_registry_db();
        let input_values = create_test_input_values(
            vec!["AAPL", "UNKNOWN"],
            vec![(SpecialReason::EmptyValue, 1)],
        );

        let result = resolve_values(&registry, &input_values).unwrap();

        // Verify invariant: total == resolved + unresolved
        assert_eq!(
            result.summary.total,
            result.summary.resolved + result.summary.unresolved
        );
        assert_eq!(result.summary.total, 3); // 1 resolved + 2 unresolved (1 no match + 1 special)
    }
}
