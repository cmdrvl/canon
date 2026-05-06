use crate::{CanonOutput, Outcome, Refusal, RefusalCode};
use serde_json::{Value, json};

impl Refusal {
    pub fn io_error(path: &str, err: &str) -> Self {
        Self {
            code: RefusalCode::EIo,
            message: format!("Unable to read path '{}': {}", path, err),
            detail: json!({
                "path": path,
                "error": err
            }),
            next_command: Some(
                "Check paths and permissions, then rerun the same canon command".to_string(),
            ),
        }
    }

    pub fn encoding_error(input: &str, reason: &str) -> Self {
        Self {
            code: RefusalCode::EEncoding,
            message: format!("Unsupported text encoding in '{}': {}", input, reason),
            detail: json!({
                "input": input,
                "reason": reason
            }),
            next_command: Some(format!(
                "iconv -f <SOURCE_ENCODING> -t UTF-8 {} > converted.csv && canon converted.csv --registry <REGISTRY> --column <COLUMN>",
                input
            )),
        }
    }

    pub fn csv_parse(input: &str, reason: &str) -> Self {
        Self {
            code: RefusalCode::ECsvParse,
            message: format!("Failed to parse CSV input '{}': {}", input, reason),
            detail: json!({
                "input": input,
                "reason": reason
            }),
            next_command: Some(format!(
                "Re-export '{}' as standard CSV and rerun canon",
                input
            )),
        }
    }

    pub fn bad_registry(path: &str, reason: &str) -> Self {
        Self {
            code: RefusalCode::EBadRegistry,
            message: format!("Registry '{}' is invalid: {}", path, reason),
            detail: json!({
                "registry": path,
                "reason": reason
            }),
            next_command: Some(format!(
                "Fix '{}'/registry.json or mapping files, then rerun canon",
                path
            )),
        }
    }

    pub fn column_not_found(column: &str, available: &[String]) -> Self {
        let suggested = available
            .first()
            .cloned()
            .unwrap_or_else(|| "<COLUMN>".to_string());
        Self {
            code: RefusalCode::EColumnNotFound,
            message: format!("Column '{}' not found in input file", column),
            detail: json!({
                "column": column,
                "available_columns": available
            }),
            next_command: Some(format!(
                "canon <INPUT> --registry <REGISTRY> --column {}",
                suggested
            )),
        }
    }

    pub fn parse_error(input: &str, reason: &str) -> Self {
        Self {
            code: RefusalCode::EParse,
            message: format!("Cannot parse input '{}': {}", input, reason),
            detail: json!({
                "input": input,
                "reason": reason
            }),
            next_command: Some(
                "Use a supported extension (.csv, .tsv, .jsonl, .ndjson) and rerun".to_string(),
            ),
        }
    }

    pub fn empty_input(input: &str) -> Self {
        Self {
            code: RefusalCode::EEmptyInput,
            message: format!("Input '{}' contains no processable data", input),
            detail: json!({
                "input": input
            }),
            next_command: Some(
                "Check the file has data rows and the selected column is populated".to_string(),
            ),
        }
    }

    pub fn too_large(limit_type: &str, limit: &str, actual: &str) -> Self {
        Self {
            code: RefusalCode::ETooLarge,
            message: format!(
                "Input exceeds --{} limit ({} > {})",
                limit_type, actual, limit
            ),
            detail: json!({
                "limit_type": limit_type,
                "limit": limit,
                "actual": actual
            }),
            next_command: Some(format!(
                "Increase --{} above {} or reduce input size, then rerun",
                limit_type, actual
            )),
        }
    }

    pub fn emit_format(input_format: &str, emit_mode: &str) -> Self {
        Self {
            code: RefusalCode::EEmitFormat,
            message: format!(
                "--emit {} is not supported with {} input",
                emit_mode, input_format
            ),
            detail: json!({
                "emit": emit_mode,
                "input_format": input_format
            }),
            next_command: Some(
                "Use --emit json for JSONL input or provide CSV input for --emit csv".to_string(),
            ),
        }
    }

    pub fn column_exists(column: &str) -> Self {
        Self {
            code: RefusalCode::EColumnExists,
            message: format!(
                "Canonical column '{}' already exists in input header",
                column
            ),
            detail: json!({
                "canon_column": column
            }),
            next_command: Some(format!("Rerun with --canon-column {}_new", column)),
        }
    }

    pub fn org_input_contract(message: impl Into<String>, detail: Value) -> Self {
        Self {
            code: RefusalCode::EOrgInputContract,
            message: message.into(),
            detail,
            next_command: Some(
                "Fix the required org row fields or structured side-field JSON, then rerun canon org"
                    .to_string(),
            ),
        }
    }

    pub fn org_bad_strategy(message: impl Into<String>, detail: Value) -> Self {
        Self {
            code: RefusalCode::EOrgBadStrategy,
            message: message.into(),
            detail,
            next_command: Some(
                "Fix the strategy YAML and rerun the same canon org command with --strategy"
                    .to_string(),
            ),
        }
    }

    pub fn org_bad_suite(message: impl Into<String>, detail: Value) -> Self {
        Self {
            code: RefusalCode::EOrgBadSuite,
            message: message.into(),
            detail,
            next_command: Some(
                "Check the frozen suite directory and profile compatibility, then rerun canon org audit"
                    .to_string(),
            ),
        }
    }

    pub fn org_fixture_invalid(message: impl Into<String>, detail: Value) -> Self {
        Self {
            code: RefusalCode::EOrgFixtureInvalid,
            message: message.into(),
            detail,
            next_command: Some(
                "Repair the suite fixture references or row catalog, then rerun canon org audit"
                    .to_string(),
            ),
        }
    }

    pub fn org_version_bump_required(message: impl Into<String>, detail: Value) -> Self {
        Self {
            code: RefusalCode::EOrgVersionBumpRequired,
            message: message.into(),
            detail,
            next_command: Some(
                "Rerun canon org promote with an explicit --next-version different from registry.json"
                    .to_string(),
            ),
        }
    }

    pub fn org_stale_registry(message: impl Into<String>, detail: Value) -> Self {
        Self {
            code: RefusalCode::EOrgStaleRegistry,
            message: message.into(),
            detail,
            next_command: Some(
                "Refresh the org result and audit against the current registry snapshot, then retry promotion"
                    .to_string(),
            ),
        }
    }

    pub fn strategy_input_contract(message: impl Into<String>, detail: Value) -> Self {
        Self {
            code: RefusalCode::EStrategyInputContract,
            message: message.into(),
            detail,
            next_command: Some(
                "Fix the schema profile, skill identity, or frozen-script metadata, then rerun canon strategy"
                    .to_string(),
            ),
        }
    }

    pub fn strategy_proof_invalid(message: impl Into<String>, detail: Value) -> Self {
        Self {
            code: RefusalCode::EStrategyProofInvalid,
            message: message.into(),
            detail,
            next_command: Some(
                "Rerun verify, assess, and airlock for the script, then retry canon strategy register"
                    .to_string(),
            ),
        }
    }

    pub fn strategy_version_bump_required(message: impl Into<String>, detail: Value) -> Self {
        Self {
            code: RefusalCode::EStrategyVersionBumpRequired,
            message: message.into(),
            detail,
            next_command: Some(
                "Rerun canon strategy register with an explicit --next-version different from registry.json"
                    .to_string(),
            ),
        }
    }

    pub fn to_canon_output(self) -> CanonOutput {
        CanonOutput {
            version: "canon.v0".to_string(),
            outcome: Outcome::Refusal,
            registry: None,
            summary: None,
            mappings: vec![],
            unresolved: vec![],
            refusal: Some(self),
        }
    }
}

pub fn create_refusal(
    code: RefusalCode,
    message: String,
    detail: serde_json::Value,
    next_command: Option<String>,
) -> CanonOutput {
    Refusal {
        code,
        message,
        detail,
        next_command,
    }
    .to_canon_output()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_codes_serialize_to_expected_strings() {
        let cases = vec![
            (RefusalCode::EIo, "\"E_IO\""),
            (RefusalCode::EEncoding, "\"E_ENCODING\""),
            (RefusalCode::ECsvParse, "\"E_CSV_PARSE\""),
            (RefusalCode::EBadRegistry, "\"E_BAD_REGISTRY\""),
            (RefusalCode::EColumnNotFound, "\"E_COLUMN_NOT_FOUND\""),
            (RefusalCode::EParse, "\"E_PARSE\""),
            (RefusalCode::EEmptyInput, "\"E_EMPTY_INPUT\""),
            (RefusalCode::ETooLarge, "\"E_TOO_LARGE\""),
            (RefusalCode::EEmitFormat, "\"E_EMIT_FORMAT\""),
            (RefusalCode::EColumnExists, "\"E_COLUMN_EXISTS\""),
            (RefusalCode::EOrgInputContract, "\"E_ORG_INPUT_CONTRACT\""),
            (RefusalCode::EOrgBadStrategy, "\"E_ORG_BAD_STRATEGY\""),
            (RefusalCode::EOrgBadSuite, "\"E_ORG_BAD_SUITE\""),
            (RefusalCode::EOrgFixtureInvalid, "\"E_ORG_FIXTURE_INVALID\""),
            (
                RefusalCode::EOrgVersionBumpRequired,
                "\"E_ORG_VERSION_BUMP_REQUIRED\"",
            ),
            (RefusalCode::EOrgStaleRegistry, "\"E_ORG_STALE_REGISTRY\""),
            (RefusalCode::EBadStrategy, "\"E_BAD_STRATEGY\""),
            (RefusalCode::ETooManyCandidates, "\"E_TOO_MANY_CANDIDATES\""),
            (RefusalCode::EEmptyTape, "\"E_EMPTY_TAPE\""),
            (RefusalCode::EIncompatibleTapes, "\"E_INCOMPATIBLE_TAPES\""),
        ];

        for (code, expected) in cases {
            let got = serde_json::to_string(&code).unwrap();
            assert_eq!(got, expected);
        }
    }

    #[test]
    fn helper_constructors_populate_actionable_fields() {
        let column_refusal =
            Refusal::column_not_found("cusip", &["security_id".to_string(), "isin".to_string()]);
        assert_eq!(column_refusal.code, RefusalCode::EColumnNotFound);
        assert!(column_refusal.message.contains("cusip"));
        assert_eq!(
            column_refusal.detail["available_columns"],
            json!(["security_id", "isin"])
        );
        assert!(
            column_refusal
                .next_command
                .as_deref()
                .unwrap()
                .contains("--column security_id")
        );

        let registry_refusal =
            Refusal::bad_registry("registries/cusip-isin", "missing registry.json");
        assert_eq!(registry_refusal.code, RefusalCode::EBadRegistry);
        assert_eq!(registry_refusal.detail["registry"], "registries/cusip-isin");
        assert!(registry_refusal.next_command.is_some());

        let org_refusal = Refusal::org_bad_strategy(
            "Strategy YAML is invalid",
            json!({ "path": "strategies/bdc.yaml" }),
        );
        assert_eq!(org_refusal.code, RefusalCode::EOrgBadStrategy);
        assert_eq!(org_refusal.detail["path"], "strategies/bdc.yaml");
        assert!(
            org_refusal
                .next_command
                .as_deref()
                .unwrap()
                .contains("--strategy")
        );
    }

    #[test]
    fn to_canon_output_wraps_refusal_envelope() {
        let refusal = Refusal::emit_format("jsonl", "csv");
        let output = refusal.to_canon_output();

        assert_eq!(output.version, "canon.v0");
        assert_eq!(output.outcome, Outcome::Refusal);
        assert!(output.registry.is_none());
        assert!(output.summary.is_none());
        assert!(output.mappings.is_empty());
        assert!(output.unresolved.is_empty());
        assert!(output.refusal.is_some());
        assert_eq!(output.refusal.unwrap().code, RefusalCode::EEmitFormat);
    }
}
