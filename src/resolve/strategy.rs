use super::{ResolveError, ResolveErrorCode, ResolveOperatorSpec, ResolveResult, ResolveStrategy};
use serde_json::json;
use std::{fs, path::Path};

const SUPPORTED_OPERATORS: &[&str] = &[
    "exact",
    "canon_match",
    "tolerance_pct",
    "tolerance_abs",
    "range",
    "date_range",
    "prefix",
];

pub fn load_strategy(path: &Path) -> ResolveResult<ResolveStrategy> {
    let bytes = fs::read(path).map_err(|error| {
        ResolveError::with_detail(
            ResolveErrorCode::Io,
            format!(
                "Unable to read resolve strategy '{}': {}",
                path.display(),
                error
            ),
            json!({
                "strategy": path.display().to_string(),
                "error": error.to_string()
            }),
        )
    })?;
    parse_strategy_bytes(&bytes)
}

pub fn parse_strategy_bytes(bytes: &[u8]) -> ResolveResult<ResolveStrategy> {
    let mut strategy: ResolveStrategy = serde_yaml::from_slice(bytes).map_err(|error| {
        ResolveError::with_detail(
            ResolveErrorCode::Strategy,
            format!("Resolve strategy YAML is invalid: {error}"),
            json!({
                "reason": error.to_string()
            }),
        )
    })?;

    strategy.content_hash = format!("blake3:{}", blake3::hash(bytes).to_hex());
    validate_strategy(&strategy)?;
    Ok(strategy)
}

pub fn validate_strategy(strategy: &ResolveStrategy) -> ResolveResult<()> {
    validate_non_empty("strategy_id", &strategy.id)?;
    validate_non_empty("strategy_version", &strategy.version)?;
    validate_non_empty("entity_type", &strategy.entity_type)?;

    validate_id_columns(
        "identity.reference.id_columns",
        &strategy.identity.reference.id_columns,
    )?;
    validate_id_columns(
        "identity.target.id_columns",
        &strategy.identity.target.id_columns,
    )?;

    if strategy.assertions.is_empty() {
        return Err(strategy_error(
            "Resolve strategy must include at least one assertion",
            json!({ "field": "assertions" }),
        ));
    }

    validate_probability("match_threshold", strategy.match_threshold)?;
    validate_probability("ambiguity_gap", strategy.ambiguity_gap)?;

    if let Some(max_candidates) = strategy.max_candidates
        && max_candidates == 0
    {
        return Err(strategy_error(
            "max_candidates must be greater than zero when provided",
            json!({ "field": "max_candidates", "value": max_candidates }),
        ));
    }

    for (index, filter) in strategy.candidate_filter.iter().enumerate() {
        validate_operator_spec(
            "candidate_filter",
            index,
            filter,
            OperatorContext::CandidateFilter,
        )?;
    }

    for (index, assertion) in strategy.assertions.iter().enumerate() {
        validate_operator_spec("assertions", index, assertion, OperatorContext::Assertion)?;
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum OperatorContext {
    CandidateFilter,
    Assertion,
}

fn validate_operator_spec(
    section: &'static str,
    index: usize,
    spec: &ResolveOperatorSpec,
    context: OperatorContext,
) -> ResolveResult<()> {
    let path = format!("{section}[{index}]");
    validate_non_empty(&format!("{path}.field_ref"), &spec.field_ref)?;
    validate_non_empty(&format!("{path}.field_tgt"), &spec.field_tgt)?;
    validate_non_empty(&format!("{path}.op"), &spec.op)?;

    if !SUPPORTED_OPERATORS.contains(&spec.op.as_str()) {
        return Err(strategy_error(
            format!("Unsupported resolve operator '{}'", spec.op),
            json!({
                "field": format!("{path}.op"),
                "op": spec.op,
                "supported": SUPPORTED_OPERATORS
            }),
        ));
    }

    if matches!(context, OperatorContext::Assertion) {
        if !spec.weight.is_finite() || spec.weight <= 0.0 || spec.weight > 1.0 {
            return Err(strategy_error(
                "Assertion weight must be greater than 0.0 and less than or equal to 1.0",
                json!({
                    "field": format!("{path}.weight"),
                    "weight": spec.weight
                }),
            ));
        }
    } else if !spec.weight.is_finite() || spec.weight < 0.0 || spec.weight > 1.0 {
        return Err(strategy_error(
            "Candidate filter weight must be between 0.0 and 1.0 when provided",
            json!({
                "field": format!("{path}.weight"),
                "weight": spec.weight
            }),
        ));
    }

    validate_operator_params(&path, spec)
}

fn validate_operator_params(path: &str, spec: &ResolveOperatorSpec) -> ResolveResult<()> {
    match spec.op.as_str() {
        "tolerance_pct" | "tolerance_abs" => {
            require_non_negative_number(path, spec, "tolerance")?;
        }
        "range" => {
            require_non_negative_number(path, spec, "range_pct")?;
        }
        "date_range" => {
            require_non_negative_integer(path, spec, "days")?;
        }
        "exact" | "canon_match" | "prefix" => {}
        _ => unreachable!("operator support checked before params"),
    }

    Ok(())
}

fn validate_id_columns(field: &str, columns: &[String]) -> ResolveResult<()> {
    if columns.is_empty() {
        return Err(strategy_error(
            format!("{field} must contain at least one column"),
            json!({ "field": field }),
        ));
    }

    let mut seen = std::collections::BTreeSet::new();
    for column in columns {
        validate_non_empty(field, column)?;
        if !seen.insert(column) {
            return Err(strategy_error(
                format!("{field} contains duplicate column '{column}'"),
                json!({ "field": field, "column": column }),
            ));
        }
    }

    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> ResolveResult<()> {
    if value.trim().is_empty() {
        Err(strategy_error(
            format!("{field} must not be empty"),
            json!({ "field": field }),
        ))
    } else {
        Ok(())
    }
}

fn validate_probability(field: &str, value: f64) -> ResolveResult<()> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        Err(strategy_error(
            format!("{field} must be between 0.0 and 1.0"),
            json!({
                "field": field,
                "value": value
            }),
        ))
    } else {
        Ok(())
    }
}

fn require_non_negative_number(
    path: &str,
    spec: &ResolveOperatorSpec,
    key: &'static str,
) -> ResolveResult<f64> {
    let value = spec.params.get(key).ok_or_else(|| {
        strategy_error(
            format!("Operator '{}' requires parameter '{key}'", spec.op),
            json!({
                "field": format!("{path}.{key}"),
                "op": spec.op
            }),
        )
    })?;

    let number = value.as_f64().ok_or_else(|| {
        strategy_error(
            format!(
                "Parameter '{key}' for operator '{}' must be numeric",
                spec.op
            ),
            json!({
                "field": format!("{path}.{key}"),
                "value": value
            }),
        )
    })?;

    if !number.is_finite() || number < 0.0 {
        return Err(strategy_error(
            format!(
                "Parameter '{key}' for operator '{}' must be non-negative",
                spec.op
            ),
            json!({
                "field": format!("{path}.{key}"),
                "value": value
            }),
        ));
    }

    Ok(number)
}

fn require_non_negative_integer(
    path: &str,
    spec: &ResolveOperatorSpec,
    key: &'static str,
) -> ResolveResult<u64> {
    let value = spec.params.get(key).ok_or_else(|| {
        strategy_error(
            format!("Operator '{}' requires parameter '{key}'", spec.op),
            json!({
                "field": format!("{path}.{key}"),
                "op": spec.op
            }),
        )
    })?;

    let integer = value.as_u64().ok_or_else(|| {
        strategy_error(
            format!(
                "Parameter '{key}' for operator '{}' must be a non-negative integer",
                spec.op
            ),
            json!({
                "field": format!("{path}.{key}"),
                "value": value
            }),
        )
    })?;

    Ok(integer)
}

fn strategy_error(message: impl Into<String>, detail: serde_json::Value) -> ResolveError {
    ResolveError::with_detail(ResolveErrorCode::Strategy, message, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value as YamlValue;
    use std::{fs, path::Path};

    fn fixture(relative: &str) -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/resolve/strategies")
                .join(relative),
        )
        .expect("strategy fixture")
    }

    #[test]
    fn parses_valid_minimal_and_cmbs_strategies() {
        let minimal = parse_strategy_bytes(&fixture("minimal.valid.yaml")).unwrap();
        assert_eq!(minimal.id, "minimal-loan-match.v1");
        assert_eq!(minimal.assertions.len(), 1);
        assert!(minimal.content_hash.starts_with("blake3:"));

        let cmbs = parse_strategy_bytes(&fixture("cmbs_loans.valid.yaml")).unwrap();
        assert_eq!(cmbs.id, "cmbs-loan-match.v1");
        assert_eq!(cmbs.candidate_filter.len(), 3);
        assert_eq!(cmbs.assertions.len(), 7);
        assert_eq!(cmbs.max_candidates, Some(10));
    }

    #[test]
    fn content_hash_is_stable_for_identical_bytes() {
        let bytes = fixture("cmbs_loans.valid.yaml");
        let left = parse_strategy_bytes(&bytes).unwrap();
        let right = parse_strategy_bytes(&bytes).unwrap();
        assert_eq!(left.content_hash, right.content_hash);
    }

    #[test]
    fn rejects_malformed_yaml_and_missing_required_threshold() {
        let refusal =
            parse_strategy_bytes(&fixture("malformed_missing_threshold.yaml")).unwrap_err();
        assert_eq!(refusal.code, ResolveErrorCode::Strategy);
        assert!(refusal.message.contains("YAML"));
    }

    #[test]
    fn rejects_unknown_operator() {
        let yaml = r#"
strategy_id: bad-op.v1
strategy_version: "0.1.0"
entity_type: loan
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [deal, loan_number]
assertions:
  - field_ref: property_address
    field_tgt: address
    op: magic
    weight: 1.0
match_threshold: 0.8
ambiguity_gap: 0.1
"#;
        let refusal = parse_strategy_bytes(yaml.as_bytes()).unwrap_err();
        assert_eq!(refusal.code, ResolveErrorCode::Strategy);
        assert!(refusal.message.contains("Unsupported"));
    }

    #[test]
    fn rejects_invalid_threshold_and_negative_weight() {
        let bad_threshold = r#"
strategy_id: bad-threshold.v1
strategy_version: "0.1.0"
entity_type: loan
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [deal, loan_number]
assertions:
  - field_ref: property_address
    field_tgt: address
    op: exact
    weight: 1.0
match_threshold: 1.5
ambiguity_gap: 0.1
"#;
        assert!(parse_strategy_bytes(bad_threshold.as_bytes()).is_err());

        let negative_weight = bad_threshold
            .replace("match_threshold: 1.5", "match_threshold: 0.8")
            .replace("weight: 1.0", "weight: -0.1");
        let refusal = parse_strategy_bytes(negative_weight.as_bytes()).unwrap_err();
        assert_eq!(refusal.code, ResolveErrorCode::Strategy);
        assert!(refusal.message.contains("weight"));
    }

    #[test]
    fn rejects_invalid_operator_parameters() {
        let missing_tolerance = r#"
strategy_id: bad-param.v1
strategy_version: "0.1.0"
entity_type: loan
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [deal, loan_number]
assertions:
  - field_ref: upb
    field_tgt: balance
    op: tolerance_pct
    weight: 1.0
match_threshold: 0.8
ambiguity_gap: 0.1
"#;
        let refusal = parse_strategy_bytes(missing_tolerance.as_bytes()).unwrap_err();
        assert_eq!(refusal.code, ResolveErrorCode::Strategy);
        assert!(refusal.message.contains("tolerance"));

        let negative_days = missing_tolerance
            .replace("op: tolerance_pct", "op: date_range")
            .replace("weight: 1.0", "days: -5\n    weight: 1.0");
        assert!(parse_strategy_bytes(negative_days.as_bytes()).is_err());
    }

    #[test]
    fn rejects_duplicate_identity_columns_and_zero_max_candidates() {
        let yaml = r#"
strategy_id: duplicate-id.v1
strategy_version: "0.1.0"
entity_type: loan
identity:
  reference:
    id_columns: [loan_id, loan_id]
  target:
    id_columns: [deal, loan_number]
assertions:
  - field_ref: property_address
    field_tgt: address
    op: exact
    weight: 1.0
match_threshold: 0.8
ambiguity_gap: 0.1
max_candidates: 0
"#;
        let refusal = parse_strategy_bytes(yaml.as_bytes()).unwrap_err();
        assert_eq!(refusal.code, ResolveErrorCode::Strategy);
        assert!(refusal.message.contains("duplicate"));

        let zero_max = yaml.replace("id_columns: [loan_id, loan_id]", "id_columns: [loan_id]");
        let refusal = parse_strategy_bytes(zero_max.as_bytes()).unwrap_err();
        assert!(refusal.message.contains("max_candidates"));
    }

    #[test]
    fn serde_yaml_value_import_keeps_yaml_dependency_visible_to_tests() {
        let parsed: YamlValue = serde_yaml::from_slice(&fixture("minimal.valid.yaml")).unwrap();
        assert!(parsed.is_mapping());
    }
}
