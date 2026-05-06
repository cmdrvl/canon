use super::{AssertionResult, ResolveOperatorSpec, ResolveRecord};
use crate::Registry;
use chrono::NaiveDate;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const DATE_FORMATS: &[&str] = &["%Y-%m-%d", "%Y/%m/%d", "%m/%d/%Y", "%Y%m%d"];

pub fn evaluate_assertion(
    spec: &ResolveOperatorSpec,
    reference: &ResolveRecord,
    target: &ResolveRecord,
    registry: Option<&Registry>,
) -> AssertionResult {
    let reference_value = reference.attributes.get(&spec.field_ref);
    let target_value = target.attributes.get(&spec.field_tgt);

    let outcome = match spec.op.as_str() {
        "exact" => evaluate_exact(reference_value, target_value),
        "prefix" => evaluate_prefix(reference_value, target_value),
        "tolerance_pct" => evaluate_tolerance_pct(reference_value, target_value, spec),
        "tolerance_abs" => evaluate_tolerance_abs(reference_value, target_value, spec),
        "range" => evaluate_range(reference_value, target_value, spec),
        "date_range" => evaluate_date_range(reference_value, target_value, spec),
        "canon_match" => evaluate_canon_match(reference_value, target_value, registry),
        _ => Outcome::fail(detail_reason("unsupported_operator")),
    };

    AssertionResult {
        field_ref: spec.field_ref.clone(),
        field_tgt: spec.field_tgt.clone(),
        op: spec.op.clone(),
        passed: outcome.passed,
        score: pass_score(outcome.passed),
        weight: spec.weight,
        required: spec.required,
        detail: outcome.detail,
    }
}

fn evaluate_exact(reference_value: Option<&Value>, target_value: Option<&Value>) -> Outcome {
    let (reference, target) = match scalar_pair(reference_value, target_value) {
        Ok(pair) => pair,
        Err(detail) => return Outcome::fail(detail),
    };

    let reference_trimmed = ascii_trim(&reference);
    let target_trimmed = ascii_trim(&target);
    let passed = reference_trimmed.as_bytes() == target_trimmed.as_bytes();

    if passed {
        Outcome::pass()
    } else {
        Outcome::fail(value_detail(
            "not_equal",
            json!(reference_trimmed),
            json!(target_trimmed),
        ))
    }
}

fn evaluate_prefix(reference_value: Option<&Value>, target_value: Option<&Value>) -> Outcome {
    let (reference, target) = match scalar_pair(reference_value, target_value) {
        Ok(pair) => pair,
        Err(detail) => return Outcome::fail(detail),
    };

    let reference_trimmed = ascii_trim(&reference);
    let target_trimmed = ascii_trim(&target);
    if reference_trimmed.is_empty() || target_trimmed.is_empty() {
        return Outcome::fail(value_detail(
            "empty_value",
            json!(reference_trimmed),
            json!(target_trimmed),
        ));
    }

    let passed = reference_trimmed.starts_with(target_trimmed)
        || target_trimmed.starts_with(reference_trimmed);
    if passed {
        Outcome::pass()
    } else {
        Outcome::fail(value_detail(
            "not_prefix",
            json!(reference_trimmed),
            json!(target_trimmed),
        ))
    }
}

fn evaluate_tolerance_pct(
    reference_value: Option<&Value>,
    target_value: Option<&Value>,
    spec: &ResolveOperatorSpec,
) -> Outcome {
    let tolerance = match non_negative_number_param(spec, "tolerance") {
        Ok(tolerance) => tolerance,
        Err(detail) => return Outcome::fail(detail),
    };
    let (reference, target) = match numeric_pair(reference_value, target_value) {
        Ok(pair) => pair,
        Err(detail) => return Outcome::fail(detail),
    };

    let diff_abs = (reference - target).abs();
    let diff_pct = symmetric_pct_diff(reference, target);
    let passed = diff_pct <= tolerance;
    let mut detail = numeric_detail(reference, target);
    detail.insert("diff_abs".to_string(), json!(diff_abs));
    detail.insert("diff_pct".to_string(), json!(diff_pct));
    detail.insert("tolerance".to_string(), json!(tolerance));

    if passed {
        Outcome::pass_with_detail(detail)
    } else {
        detail.insert("reason".to_string(), json!("outside_tolerance_pct"));
        Outcome::fail(detail)
    }
}

fn evaluate_tolerance_abs(
    reference_value: Option<&Value>,
    target_value: Option<&Value>,
    spec: &ResolveOperatorSpec,
) -> Outcome {
    let tolerance = match non_negative_number_param(spec, "tolerance") {
        Ok(tolerance) => tolerance,
        Err(detail) => return Outcome::fail(detail),
    };
    let (reference, target) = match numeric_pair(reference_value, target_value) {
        Ok(pair) => pair,
        Err(detail) => return Outcome::fail(detail),
    };

    let diff_abs = (reference - target).abs();
    let passed = diff_abs <= tolerance;
    let mut detail = numeric_detail(reference, target);
    detail.insert("diff_abs".to_string(), json!(diff_abs));
    detail.insert("tolerance".to_string(), json!(tolerance));

    if passed {
        Outcome::pass_with_detail(detail)
    } else {
        detail.insert("reason".to_string(), json!("outside_tolerance_abs"));
        Outcome::fail(detail)
    }
}

fn evaluate_range(
    reference_value: Option<&Value>,
    target_value: Option<&Value>,
    spec: &ResolveOperatorSpec,
) -> Outcome {
    let range_pct = match non_negative_number_param(spec, "range_pct") {
        Ok(range_pct) => range_pct,
        Err(detail) => return Outcome::fail(detail),
    };
    let (reference, target) = match numeric_pair(reference_value, target_value) {
        Ok(pair) => pair,
        Err(detail) => return Outcome::fail(detail),
    };

    let diff_abs = (reference - target).abs();
    let allowed_abs = reference.abs() * range_pct;
    let diff_pct = if reference == 0.0 {
        if target == 0.0 { 0.0 } else { f64::INFINITY }
    } else {
        diff_abs / reference.abs()
    };
    let passed = if reference == 0.0 {
        target == 0.0
    } else {
        diff_pct <= range_pct
    };

    let mut detail = numeric_detail(reference, target);
    detail.insert("allowed_abs".to_string(), json!(allowed_abs));
    detail.insert("diff_abs".to_string(), json!(diff_abs));
    if diff_pct.is_finite() {
        detail.insert("diff_pct".to_string(), json!(diff_pct));
    }
    detail.insert("range_pct".to_string(), json!(range_pct));

    if passed {
        Outcome::pass_with_detail(detail)
    } else {
        detail.insert("reason".to_string(), json!("outside_range"));
        Outcome::fail(detail)
    }
}

fn evaluate_date_range(
    reference_value: Option<&Value>,
    target_value: Option<&Value>,
    spec: &ResolveOperatorSpec,
) -> Outcome {
    let days = match non_negative_integer_param(spec, "days") {
        Ok(days) => days,
        Err(detail) => return Outcome::fail(detail),
    };
    let (reference, target) = match date_pair(reference_value, target_value) {
        Ok(pair) => pair,
        Err(detail) => return Outcome::fail(detail),
    };

    let delta_days = reference.signed_duration_since(target).num_days().abs();
    let passed = delta_days <= days;
    let mut detail = BTreeMap::new();
    detail.insert("days".to_string(), json!(days));
    detail.insert("delta_days".to_string(), json!(delta_days));
    detail.insert("ref_val".to_string(), json!(reference.to_string()));
    detail.insert("tgt_val".to_string(), json!(target.to_string()));

    if passed {
        Outcome::pass_with_detail(detail)
    } else {
        detail.insert("reason".to_string(), json!("outside_date_range"));
        Outcome::fail(detail)
    }
}

fn evaluate_canon_match(
    reference_value: Option<&Value>,
    target_value: Option<&Value>,
    registry: Option<&Registry>,
) -> Outcome {
    let registry = match registry {
        Some(registry) => registry,
        None => return Outcome::fail(detail_reason("registry_required")),
    };
    let (reference, target) = match scalar_pair(reference_value, target_value) {
        Ok(pair) => pair,
        Err(detail) => return Outcome::fail(detail),
    };

    let reference_input = ascii_trim(&reference);
    let target_input = ascii_trim(&target);
    let reference_lookup = lookup_canonical_id(registry, reference_input);
    let target_lookup = lookup_canonical_id(registry, target_input);
    let mut detail = BTreeMap::new();
    detail.insert("ref_val".to_string(), json!(reference_input));
    detail.insert("tgt_val".to_string(), json!(target_input));

    let reference_canonical = match reference_lookup {
        Ok(canonical_id) => canonical_id,
        Err(error) => {
            detail.insert("error".to_string(), json!(error));
            detail.insert("reason".to_string(), json!("lookup_error"));
            return Outcome::fail(detail);
        }
    };
    let target_canonical = match target_lookup {
        Ok(canonical_id) => canonical_id,
        Err(error) => {
            detail.insert("error".to_string(), json!(error));
            detail.insert("reason".to_string(), json!("lookup_error"));
            return Outcome::fail(detail);
        }
    };

    if let Some(canonical_id) = &reference_canonical {
        detail.insert("ref_canonical_id".to_string(), json!(canonical_id));
    }
    if let Some(canonical_id) = &target_canonical {
        detail.insert("tgt_canonical_id".to_string(), json!(canonical_id));
    }

    match (reference_canonical, target_canonical) {
        (Some(reference_id), Some(target_id)) if reference_id == target_id => {
            Outcome::pass_with_detail(detail)
        }
        (Some(_), Some(_)) => {
            detail.insert("reason".to_string(), json!("canonical_id_mismatch"));
            Outcome::fail(detail)
        }
        (None, None) => {
            detail.insert("reason".to_string(), json!("both_unresolved"));
            Outcome::fail(detail)
        }
        (None, Some(_)) => {
            detail.insert("reason".to_string(), json!("ref_unresolved"));
            Outcome::fail(detail)
        }
        (Some(_), None) => {
            detail.insert("reason".to_string(), json!("tgt_unresolved"));
            Outcome::fail(detail)
        }
    }
}

fn lookup_canonical_id(registry: &Registry, input: &str) -> Result<Option<String>, String> {
    let connection = Connection::open(&registry.db_path)
        .map_err(|error| format!("Cannot open registry database: {error}"))?;
    let mut statement = connection
        .prepare(
            "SELECT canonical_id
             FROM entries
             WHERE input = ?1
             ORDER BY source_file ASC, entry_order ASC
             LIMIT 1",
        )
        .map_err(|error| format!("Cannot prepare registry lookup: {error}"))?;

    match statement.query_row(params![input], |row| row.get::<_, String>(0)) {
        Ok(canonical_id) => Ok(Some(canonical_id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(format!("Registry lookup failed for '{input}': {error}")),
    }
}

fn scalar_pair(
    reference_value: Option<&Value>,
    target_value: Option<&Value>,
) -> Result<(String, String), BTreeMap<String, Value>> {
    let reference = scalar_to_string(reference_value);
    let target = scalar_to_string(target_value);

    match (reference, target) {
        (Ok(reference), Ok(target)) => Ok((reference, target)),
        (reference, target) => {
            let mut detail = detail_reason("missing_value");
            if let Err(reason) = reference {
                detail.insert("ref_reason".to_string(), json!(reason));
            }
            if let Err(reason) = target {
                detail.insert("tgt_reason".to_string(), json!(reason));
            }
            Err(detail)
        }
    }
}

fn scalar_to_string(value: Option<&Value>) -> Result<String, &'static str> {
    match value {
        None => Err("missing_field"),
        Some(Value::Null) => Err("null_value"),
        Some(Value::String(value)) => Ok(value.clone()),
        Some(Value::Number(value)) => Ok(value.to_string()),
        Some(Value::Bool(value)) => Ok(value.to_string()),
        Some(Value::Array(_) | Value::Object(_)) => Err("non_scalar_value"),
    }
}

fn numeric_pair(
    reference_value: Option<&Value>,
    target_value: Option<&Value>,
) -> Result<(f64, f64), BTreeMap<String, Value>> {
    let (reference, target) = scalar_pair(reference_value, target_value)?;
    let reference_number = parse_number(&reference);
    let target_number = parse_number(&target);

    match (reference_number, target_number) {
        (Ok(reference_number), Ok(target_number)) => Ok((reference_number, target_number)),
        (reference_number, target_number) => {
            let mut detail = value_detail("invalid_number", json!(reference), json!(target));
            if let Err(reason) = reference_number {
                detail.insert("ref_reason".to_string(), json!(reason));
            }
            if let Err(reason) = target_number {
                detail.insert("tgt_reason".to_string(), json!(reason));
            }
            Err(detail)
        }
    }
}

fn parse_number(value: &str) -> Result<f64, &'static str> {
    let trimmed = ascii_trim(value);
    if trimmed.is_empty() {
        return Err("empty_value");
    }

    let parsed = trimmed.parse::<f64>().map_err(|_| "parse_error")?;
    if parsed.is_finite() {
        Ok(parsed)
    } else {
        Err("non_finite_number")
    }
}

fn date_pair(
    reference_value: Option<&Value>,
    target_value: Option<&Value>,
) -> Result<(NaiveDate, NaiveDate), BTreeMap<String, Value>> {
    let (reference, target) = scalar_pair(reference_value, target_value)?;
    let reference_date = parse_date(&reference);
    let target_date = parse_date(&target);

    match (reference_date, target_date) {
        (Some(reference_date), Some(target_date)) => Ok((reference_date, target_date)),
        (reference_date, target_date) => {
            let mut detail = value_detail("invalid_date", json!(reference), json!(target));
            if reference_date.is_none() {
                detail.insert("ref_reason".to_string(), json!("parse_error"));
            }
            if target_date.is_none() {
                detail.insert("tgt_reason".to_string(), json!("parse_error"));
            }
            Err(detail)
        }
    }
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    let trimmed = ascii_trim(value);
    if trimmed.is_empty() {
        return None;
    }

    DATE_FORMATS
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(trimmed, format).ok())
}

fn non_negative_number_param(
    spec: &ResolveOperatorSpec,
    key: &'static str,
) -> Result<f64, BTreeMap<String, Value>> {
    let value = spec.params.get(key).ok_or_else(|| {
        let mut detail = detail_reason("missing_parameter");
        detail.insert("parameter".to_string(), json!(key));
        detail
    })?;

    let Some(number) = value.as_f64() else {
        let mut detail = detail_reason("invalid_parameter");
        detail.insert("parameter".to_string(), json!(key));
        detail.insert("value".to_string(), value.clone());
        return Err(detail);
    };

    if !number.is_finite() || number < 0.0 {
        let mut detail = detail_reason("invalid_parameter");
        detail.insert("parameter".to_string(), json!(key));
        detail.insert("value".to_string(), value.clone());
        return Err(detail);
    }

    Ok(number)
}

fn non_negative_integer_param(
    spec: &ResolveOperatorSpec,
    key: &'static str,
) -> Result<i64, BTreeMap<String, Value>> {
    let value = spec.params.get(key).ok_or_else(|| {
        let mut detail = detail_reason("missing_parameter");
        detail.insert("parameter".to_string(), json!(key));
        detail
    })?;

    let Some(number) = value.as_i64() else {
        let mut detail = detail_reason("invalid_parameter");
        detail.insert("parameter".to_string(), json!(key));
        detail.insert("value".to_string(), value.clone());
        return Err(detail);
    };

    if number < 0 {
        let mut detail = detail_reason("invalid_parameter");
        detail.insert("parameter".to_string(), json!(key));
        detail.insert("value".to_string(), value.clone());
        return Err(detail);
    }

    Ok(number)
}

fn symmetric_pct_diff(reference: f64, target: f64) -> f64 {
    let diff = (reference - target).abs();
    let denominator = reference.abs().max(target.abs());
    if denominator == 0.0 {
        0.0
    } else {
        diff / denominator
    }
}

fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_ascii_whitespace())
}

fn pass_score(passed: bool) -> f64 {
    if passed { 1.0 } else { 0.0 }
}

fn numeric_detail(reference: f64, target: f64) -> BTreeMap<String, Value> {
    let mut detail = BTreeMap::new();
    detail.insert("ref_val".to_string(), json!(reference));
    detail.insert("tgt_val".to_string(), json!(target));
    detail
}

fn value_detail(reason: &'static str, reference: Value, target: Value) -> BTreeMap<String, Value> {
    let mut detail = detail_reason(reason);
    detail.insert("ref_val".to_string(), reference);
    detail.insert("tgt_val".to_string(), target);
    detail
}

fn detail_reason(reason: &'static str) -> BTreeMap<String, Value> {
    let mut detail = BTreeMap::new();
    detail.insert("reason".to_string(), json!(reason));
    detail
}

#[derive(Debug, Clone, PartialEq)]
struct Outcome {
    passed: bool,
    detail: BTreeMap<String, Value>,
}

impl Outcome {
    fn pass() -> Self {
        Self {
            passed: true,
            detail: BTreeMap::new(),
        }
    }

    fn pass_with_detail(detail: BTreeMap<String, Value>) -> Self {
        Self {
            passed: true,
            detail,
        }
    }

    fn fail(detail: BTreeMap<String, Value>) -> Self {
        Self {
            passed: false,
            detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RegistryMeta;
    use crate::registry::load_registry;
    use crate::resolve::TapeSide;
    use serde_json::json;
    use std::path::Path;

    fn spec(op: &str, params: &[(&str, Value)]) -> ResolveOperatorSpec {
        ResolveOperatorSpec {
            field_ref: "ref".to_string(),
            field_tgt: "tgt".to_string(),
            op: op.to_string(),
            weight: 0.5,
            required: false,
            params: params
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        }
    }

    fn required_spec(op: &str) -> ResolveOperatorSpec {
        ResolveOperatorSpec {
            required: true,
            ..spec(op, &[])
        }
    }

    fn record(side: TapeSide, field: &str, value: Value) -> ResolveRecord {
        ResolveRecord {
            side,
            composite_id: format!("{side:?}-1"),
            row_index: 0,
            attributes: [(field.to_string(), value)].into_iter().collect(),
        }
    }

    fn pair(reference: Value, target: Value) -> (ResolveRecord, ResolveRecord) {
        (
            record(TapeSide::Reference, "ref", reference),
            record(TapeSide::Target, "tgt", target),
        )
    }

    fn missing_target_pair(reference: Value) -> (ResolveRecord, ResolveRecord) {
        (
            record(TapeSide::Reference, "ref", reference),
            ResolveRecord {
                side: TapeSide::Target,
                composite_id: "target-1".to_string(),
                row_index: 0,
                attributes: BTreeMap::new(),
            },
        )
    }

    fn assert_score_bounds(result: &AssertionResult) {
        assert!(
            (0.0..=1.0).contains(&result.score),
            "score out of bounds: {}",
            result.score
        );
    }

    #[test]
    fn exact_is_byte_equal_after_ascii_trim() {
        let (reference, target) = pair(json!("  ABC\t"), json!("ABC"));
        let result = evaluate_assertion(&spec("exact", &[]), &reference, &target, None);
        assert!(result.passed);
        assert_eq!(result.score, 1.0);
        assert_score_bounds(&result);

        let (reference, target) = pair(json!("\u{00a0}ABC"), json!("ABC"));
        let result = evaluate_assertion(&spec("exact", &[]), &reference, &target, None);
        assert!(!result.passed);
        assert_eq!(result.detail.get("reason"), Some(&json!("not_equal")));
        assert_score_bounds(&result);
    }

    #[test]
    fn exact_is_reflexive_for_scalar_values() {
        for value in [json!("Loan-7"), json!(1234), json!(true)] {
            let (reference, target) = pair(value.clone(), value);
            let result = evaluate_assertion(&spec("exact", &[]), &reference, &target, None);
            assert!(result.passed);
            assert_score_bounds(&result);
        }
    }

    #[test]
    fn prefix_passes_when_either_trimmed_side_is_prefix() {
        for (left, right) in [("ABC", "AB"), ("AB", "ABC"), ("  ABC ", "ABC-99")] {
            let (reference, target) = pair(json!(left), json!(right));
            let result = evaluate_assertion(&spec("prefix", &[]), &reference, &target, None);
            assert!(result.passed, "{left:?} {right:?}");
            assert_score_bounds(&result);
        }

        let (reference, target) = pair(json!("ABC"), json!("AX"));
        let result = evaluate_assertion(&spec("prefix", &[]), &reference, &target, None);
        assert!(!result.passed);
        assert_eq!(result.detail.get("reason"), Some(&json!("not_prefix")));

        let (reference, target) = pair(json!(""), json!("ABC"));
        let result = evaluate_assertion(&spec("prefix", &[]), &reference, &target, None);
        assert!(!result.passed);
        assert_eq!(result.detail.get("reason"), Some(&json!("empty_value")));
    }

    #[test]
    fn tolerance_pct_uses_symmetric_percentage_and_explicit_zero_handling() {
        let pct_spec = spec("tolerance_pct", &[("tolerance", json!(0.1))]);
        let (reference, target) = pair(json!(100), json!(110));
        let result = evaluate_assertion(&pct_spec, &reference, &target, None);
        assert!(result.passed);
        assert_eq!(result.detail.get("tolerance"), Some(&json!(0.1)));
        assert_score_bounds(&result);

        let (reference, target) = pair(json!(100), json!(112));
        let result = evaluate_assertion(&pct_spec, &reference, &target, None);
        assert!(!result.passed);
        assert_eq!(
            result.detail.get("reason"),
            Some(&json!("outside_tolerance_pct"))
        );

        let (reference, target) = pair(json!(0), json!(0));
        let result = evaluate_assertion(&pct_spec, &reference, &target, None);
        assert!(result.passed);
        assert_eq!(result.detail.get("diff_pct"), Some(&json!(0.0)));

        let (reference, target) = pair(json!(0), json!(10));
        let result = evaluate_assertion(&pct_spec, &reference, &target, None);
        assert!(!result.passed);
        assert_eq!(result.detail.get("diff_pct"), Some(&json!(1.0)));
    }

    #[test]
    fn tolerance_pct_is_symmetric() {
        let pct_spec = spec("tolerance_pct", &[("tolerance", json!(0.125))]);
        for (left, right) in [(-100.0, -90.0), (0.0, 0.0), (0.0, 10.0), (8.0, 9.0)] {
            let (reference, target) = pair(json!(left), json!(right));
            let forward = evaluate_assertion(&pct_spec, &reference, &target, None);

            let (reference, target) = pair(json!(right), json!(left));
            let reverse = evaluate_assertion(&pct_spec, &reference, &target, None);

            assert_eq!(forward.passed, reverse.passed);
            assert_eq!(
                forward.detail.get("diff_pct"),
                reverse.detail.get("diff_pct")
            );
            assert_score_bounds(&forward);
            assert_score_bounds(&reverse);
        }
    }

    #[test]
    fn tolerance_abs_covers_boundaries_and_symmetry() {
        let abs_spec = spec("tolerance_abs", &[("tolerance", json!(1.0))]);
        let (reference, target) = pair(json!(10), json!(11));
        let result = evaluate_assertion(&abs_spec, &reference, &target, None);
        assert!(result.passed);

        let (reference, target) = pair(json!(10), json!(11.01));
        let result = evaluate_assertion(&abs_spec, &reference, &target, None);
        assert!(!result.passed);
        assert_eq!(
            result.detail.get("reason"),
            Some(&json!("outside_tolerance_abs"))
        );

        for (left, right) in [(-4.0, -4.5), (0.0, 0.0), (8.0, 7.25)] {
            let (reference, target) = pair(json!(left), json!(right));
            let forward = evaluate_assertion(&abs_spec, &reference, &target, None);

            let (reference, target) = pair(json!(right), json!(left));
            let reverse = evaluate_assertion(&abs_spec, &reference, &target, None);

            assert_eq!(forward.passed, reverse.passed);
            assert_eq!(
                forward.detail.get("diff_abs"),
                reverse.detail.get("diff_abs")
            );
            assert_score_bounds(&forward);
            assert_score_bounds(&reverse);
        }
    }

    #[test]
    fn range_uses_reference_value_as_candidate_window() {
        let range_spec = spec("range", &[("range_pct", json!(0.2))]);
        let (reference, target) = pair(json!(100), json!(120));
        let result = evaluate_assertion(&range_spec, &reference, &target, None);
        assert!(result.passed);

        let (reference, target) = pair(json!(100), json!(121));
        let result = evaluate_assertion(&range_spec, &reference, &target, None);
        assert!(!result.passed);
        assert_eq!(result.detail.get("reason"), Some(&json!("outside_range")));

        let (reference, target) = pair(json!(0), json!(0));
        let result = evaluate_assertion(&range_spec, &reference, &target, None);
        assert!(result.passed);

        let (reference, target) = pair(json!(0), json!(1));
        let result = evaluate_assertion(&range_spec, &reference, &target, None);
        assert!(!result.passed);
        assert!(!result.detail.contains_key("diff_pct"));
    }

    #[test]
    fn date_range_parses_deterministic_formats_and_day_boundaries() {
        let date_spec = spec("date_range", &[("days", json!(5))]);
        let (reference, target) = pair(json!("2026-05-06"), json!("05/11/2026"));
        let result = evaluate_assertion(&date_spec, &reference, &target, None);
        assert!(result.passed);
        assert_eq!(result.detail.get("delta_days"), Some(&json!(5)));

        let (reference, target) = pair(json!("20260506"), json!("2026/05/12"));
        let result = evaluate_assertion(&date_spec, &reference, &target, None);
        assert!(!result.passed);
        assert_eq!(
            result.detail.get("reason"),
            Some(&json!("outside_date_range"))
        );

        let (reference, target) = pair(json!("May 6, 2026"), json!("2026-05-06"));
        let result = evaluate_assertion(&date_spec, &reference, &target, None);
        assert!(!result.passed);
        assert_eq!(result.detail.get("reason"), Some(&json!("invalid_date")));
    }

    #[test]
    fn canon_match_resolves_both_sides_through_loaded_registry() {
        let registry = load_registry(Path::new("tests/fixtures/registries/resolve-servicers"))
            .expect("load registry fixture");
        let canon_spec = spec("canon_match", &[]);
        let (reference, target) = pair(json!("Wells Fargo"), json!("Wells Fargo Bank N.A."));
        let result = evaluate_assertion(&canon_spec, &reference, &target, Some(&registry));
        assert!(result.passed);
        assert_eq!(
            result.detail.get("ref_canonical_id"),
            Some(&json!("SERVICER-WELLS-FARGO"))
        );
        assert_eq!(
            result.detail.get("tgt_canonical_id"),
            Some(&json!("SERVICER-WELLS-FARGO"))
        );

        let (reference, target) = pair(json!("Wells Fargo"), json!("JPMorgan"));
        let result = evaluate_assertion(&canon_spec, &reference, &target, Some(&registry));
        assert!(!result.passed);
        assert_eq!(
            result.detail.get("reason"),
            Some(&json!("canonical_id_mismatch"))
        );

        let (reference, target) = pair(json!("Wells Fargo"), json!("Unknown Servicer"));
        let result = evaluate_assertion(&canon_spec, &reference, &target, Some(&registry));
        assert!(!result.passed);
        assert_eq!(result.detail.get("reason"), Some(&json!("tgt_unresolved")));
    }

    #[test]
    fn canon_match_requires_registry_context() {
        let (reference, target) = pair(json!("Wells Fargo"), json!("Wells Fargo"));
        let result = evaluate_assertion(&spec("canon_match", &[]), &reference, &target, None);
        assert!(!result.passed);
        assert_eq!(
            result.detail.get("reason"),
            Some(&json!("registry_required"))
        );
    }

    #[test]
    fn missing_and_invalid_values_fail_with_structured_reasons() {
        let (reference, target) = missing_target_pair(json!("ABC"));
        let result = evaluate_assertion(&spec("exact", &[]), &reference, &target, None);
        assert!(!result.passed);
        assert_eq!(result.detail.get("reason"), Some(&json!("missing_value")));
        assert_eq!(
            result.detail.get("tgt_reason"),
            Some(&json!("missing_field"))
        );

        let (reference, target) = pair(json!({"bad": true}), json!(10));
        let result = evaluate_assertion(
            &spec("tolerance_abs", &[("tolerance", json!(1.0))]),
            &reference,
            &target,
            None,
        );
        assert!(!result.passed);
        assert_eq!(result.detail.get("reason"), Some(&json!("missing_value")));
        assert_eq!(
            result.detail.get("ref_reason"),
            Some(&json!("non_scalar_value"))
        );

        let (reference, target) = pair(json!("not-a-number"), json!(10));
        let result = evaluate_assertion(
            &spec("tolerance_abs", &[("tolerance", json!(1.0))]),
            &reference,
            &target,
            None,
        );
        assert!(!result.passed);
        assert_eq!(result.detail.get("reason"), Some(&json!("invalid_number")));

        let (reference, target) = pair(json!(10), json!(11));
        let result = evaluate_assertion(
            &spec("tolerance_abs", &[("tolerance", json!("1"))]),
            &reference,
            &target,
            None,
        );
        assert!(!result.passed);
        assert_eq!(
            result.detail.get("reason"),
            Some(&json!("invalid_parameter"))
        );
    }

    #[test]
    fn required_failure_semantics_are_carried_for_later_scoring() {
        let (reference, target) = pair(json!("ABC"), json!("XYZ"));
        let result = evaluate_assertion(&required_spec("exact"), &reference, &target, None);
        assert!(!result.passed);
        assert!(result.required);
        assert_eq!(result.weight, 0.5);
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn detail_serialization_is_deterministic() {
        let (reference, target) = pair(json!(10), json!(12));
        let result = evaluate_assertion(
            &spec("tolerance_abs", &[("tolerance", json!(1.0))]),
            &reference,
            &target,
            None,
        );

        assert_eq!(
            serde_json::to_string(&result.detail).unwrap(),
            r#"{"diff_abs":2.0,"reason":"outside_tolerance_abs","ref_val":10.0,"tgt_val":12.0,"tolerance":1.0}"#
        );
    }

    #[test]
    fn registry_lookup_uses_first_match_precedence() {
        let temp_db = tempfile::NamedTempFile::with_suffix(".sqlite").unwrap();
        let connection = Connection::open(temp_db.path()).unwrap();
        connection
            .execute(
                "CREATE TABLE entries (
                    input TEXT NOT NULL,
                    canonical_id TEXT NOT NULL,
                    canonical_type TEXT NOT NULL,
                    rule_id TEXT NOT NULL,
                    source_file TEXT NOT NULL,
                    entry_order INTEGER NOT NULL
                )",
                [],
            )
            .unwrap();
        for (canonical_id, source_file, entry_order) in [
            ("SECOND", "b.json", 0),
            ("FIRST", "a.json", 1),
            ("LATER", "a.json", 2),
        ] {
            connection
                .execute(
                    "INSERT INTO entries
                     (input, canonical_id, canonical_type, rule_id, source_file, entry_order)
                     VALUES ('Alias', ?1, 'entity', 'RULE', ?2, ?3)",
                    params![canonical_id, source_file, entry_order],
                )
                .unwrap();
        }
        let registry = Registry {
            meta: RegistryMeta {
                id: "test".to_string(),
                version: "1.0.0".to_string(),
                source: "test".to_string(),
            },
            db_path: temp_db.path().to_path_buf(),
        };

        assert_eq!(
            lookup_canonical_id(&registry, "Alias").unwrap(),
            Some("FIRST".to_string())
        );
    }
}
