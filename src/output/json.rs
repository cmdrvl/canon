use crate::{Mapping, Outcome, RegistryMeta, ResolveResult, Summary, UnresolvedEntry};
use serde::Serialize;
use std::error::Error;

pub fn emit_json(
    registry_meta: &RegistryMeta,
    result: &ResolveResult,
) -> Result<String, Box<dyn Error>> {
    emit_json_with_options(registry_meta, result, false)
}

pub fn emit_json_explicit(
    registry_meta: &RegistryMeta,
    result: &ResolveResult,
    explicit: bool,
) -> Result<String, Box<dyn Error>> {
    emit_json_with_options(registry_meta, result, explicit)
}

fn emit_json_with_options(
    registry_meta: &RegistryMeta,
    result: &ResolveResult,
    explicit: bool,
) -> Result<String, Box<dyn Error>> {
    let outcome = derive_outcome(&result.summary)?;

    let mut mappings = result
        .mappings
        .iter()
        .map(|m| mapping_to_wire(m, explicit))
        .collect::<Vec<_>>();
    mappings.sort_by(|left, right| left.input.cmp(&right.input));

    let mut unresolved = result
        .unresolved
        .iter()
        .map(|u| unresolved_to_wire(u, explicit))
        .collect::<Vec<_>>();
    unresolved.sort_by(|left, right| match (&left.input, &right.input) {
        (Some(left_input), Some(right_input)) => left_input.cmp(right_input),
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, None) => left.reason.cmp(&right.reason),
    });

    let output = WireCanonOutput {
        version: "canon.v0".to_string(),
        outcome,
        registry: Some(registry_meta.clone()),
        summary: Some(result.summary.clone()),
        // Discovery breadcrumb: when false, input/canonical_id values are shown
        // verbatim; when true they are masked as "[REDACTED]" and the operator
        // can re-run with --explicit to reveal them.
        redacted: !explicit,
        mappings,
        unresolved,
        refusal: None,
    };

    Ok(serde_json::to_string(&output)?)
}

fn derive_outcome(summary: &Summary) -> Result<Outcome, Box<dyn Error>> {
    match (summary.resolved, summary.unresolved) {
        (resolved, 0) if resolved > 0 => Ok(Outcome::Resolved),
        (resolved, unresolved) if resolved > 0 && unresolved > 0 => Ok(Outcome::Partial),
        (0, unresolved) if unresolved > 0 => Ok(Outcome::Unresolved),
        _ => Err("invalid summary for non-refusal output".into()),
    }
}

const REDACTED: &str = "[REDACTED]";

fn mapping_to_wire(mapping: &Mapping, explicit: bool) -> WireMapping {
    if explicit {
        WireMapping {
            input: encode_identifier(mapping.input.as_bytes()),
            canonical_id: encode_identifier(mapping.canonical_id.as_bytes()),
            canonical_type: mapping.canonical_type.clone(),
            rule_id: mapping.rule_id.clone(),
            confidence: mapping.confidence.clone(),
        }
    } else {
        WireMapping {
            input: REDACTED.to_string(),
            canonical_id: REDACTED.to_string(),
            canonical_type: mapping.canonical_type.clone(),
            rule_id: mapping.rule_id.clone(),
            confidence: mapping.confidence.clone(),
        }
    }
}

fn unresolved_to_wire(unresolved_entry: &UnresolvedEntry, explicit: bool) -> WireUnresolvedEntry {
    WireUnresolvedEntry {
        input: if explicit {
            unresolved_entry
                .input
                .as_ref()
                .map(|value| encode_identifier(value.as_bytes()))
        } else {
            unresolved_entry
                .input
                .as_ref()
                .map(|_| REDACTED.to_string())
        },
        reason: unresolved_entry.reason.clone(),
    }
}

pub(crate) fn encode_identifier(raw: &[u8]) -> String {
    if raw.iter().all(|byte| !is_ascii_control(*byte)) {
        format!("u8:{}", String::from_utf8_lossy(raw))
    } else {
        format!("hex:{}", hex_encode(raw))
    }
}

fn is_ascii_control(byte: u8) -> bool {
    byte <= 0x1f || byte == 0x7f
}

fn hex_encode(raw: &[u8]) -> String {
    let mut output = String::with_capacity(raw.len() * 2);
    for byte in raw {
        output.push_str(&format!("{:02x}", byte));
    }
    output
}

#[derive(Debug, Serialize)]
struct WireCanonOutput {
    version: String,
    outcome: Outcome,
    registry: Option<RegistryMeta>,
    summary: Option<Summary>,
    redacted: bool,
    mappings: Vec<WireMapping>,
    unresolved: Vec<WireUnresolvedEntry>,
    refusal: Option<crate::Refusal>,
}

#[derive(Debug, Serialize)]
struct WireMapping {
    input: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
    confidence: String,
}

#[derive(Debug, Serialize)]
struct WireUnresolvedEntry {
    input: Option<String>,
    reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Mapping, RefusalCode, ResolveResult, Summary, UnresolvedEntry, refusal::create_refusal,
    };
    use serde_json::json;

    #[test]
    fn encode_identifier_uses_u8_prefix() {
        let encoded = encode_identifier("AAPL".as_bytes());
        assert_eq!(encoded, "u8:AAPL");
    }

    #[test]
    fn encode_identifier_uses_hex_for_ascii_control_bytes() {
        let encoded = encode_identifier(b"AAPL\x7f");
        assert_eq!(encoded, "hex:4141504c7f");
    }

    #[test]
    fn derive_outcome_from_summary_counts() {
        assert_eq!(
            derive_outcome(&Summary {
                total: 2,
                resolved: 2,
                unresolved: 0
            })
            .unwrap(),
            Outcome::Resolved
        );
        assert_eq!(
            derive_outcome(&Summary {
                total: 2,
                resolved: 1,
                unresolved: 1
            })
            .unwrap(),
            Outcome::Partial
        );
        assert_eq!(
            derive_outcome(&Summary {
                total: 2,
                resolved: 0,
                unresolved: 2
            })
            .unwrap(),
            Outcome::Unresolved
        );
    }

    #[test]
    fn emit_json_serializes_expected_schema() {
        let registry_meta = RegistryMeta {
            id: "cusip-isin".to_string(),
            version: "3.2.1".to_string(),
            source: "registries/cusip-isin/".to_string(),
        };
        let result = ResolveResult {
            mappings: vec![Mapping {
                input: "037833100".to_string(),
                canonical_id: "AAPL".to_string(),
                canonical_type: "ticker".to_string(),
                rule_id: "CUSIP_TO_TICKER".to_string(),
                confidence: "deterministic".to_string(),
            }],
            unresolved: vec![
                UnresolvedEntry {
                    input: Some("UNKNOWN99".to_string()),
                    reason: "no matching rule".to_string(),
                },
                UnresolvedEntry {
                    input: None,
                    reason: "empty_value".to_string(),
                },
            ],
            summary: Summary {
                total: 3,
                resolved: 1,
                unresolved: 2,
            },
        };

        let output = emit_json_explicit(&registry_meta, &result, true).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["version"], "canon.v0");
        assert_eq!(parsed["outcome"], "PARTIAL");
        assert_eq!(parsed["registry"]["id"], "cusip-isin");
        assert_eq!(parsed["registry"]["version"], "3.2.1");
        assert_eq!(parsed["summary"]["total"], 3);
        assert_eq!(parsed["summary"]["resolved"], 1);
        assert_eq!(parsed["summary"]["unresolved"], 2);
        assert_eq!(parsed["mappings"][0]["input"], "u8:037833100");
        assert_eq!(parsed["mappings"][0]["canonical_id"], "u8:AAPL");
        assert_eq!(parsed["mappings"][0]["canonical_type"], "ticker");
        assert_eq!(parsed["mappings"][0]["rule_id"], "CUSIP_TO_TICKER");
        assert_eq!(parsed["mappings"][0]["confidence"], "deterministic");
        assert_eq!(parsed["unresolved"][0]["input"], "u8:UNKNOWN99");
        assert_eq!(parsed["unresolved"][0]["reason"], "no matching rule");
        assert_eq!(parsed["unresolved"][1]["input"], serde_json::Value::Null);
        assert_eq!(parsed["unresolved"][1]["reason"], "empty_value");
        assert_eq!(parsed["refusal"], serde_json::Value::Null);
    }

    #[test]
    fn emit_json_redacts_by_default() {
        let registry_meta = RegistryMeta {
            id: "cusip-isin".to_string(),
            version: "3.2.1".to_string(),
            source: "registries/cusip-isin/".to_string(),
        };
        let result = ResolveResult {
            mappings: vec![Mapping {
                input: "037833100".to_string(),
                canonical_id: "AAPL".to_string(),
                canonical_type: "ticker".to_string(),
                rule_id: "CUSIP_TO_TICKER".to_string(),
                confidence: "deterministic".to_string(),
            }],
            unresolved: vec![UnresolvedEntry {
                input: Some("UNKNOWN99".to_string()),
                reason: "no matching rule".to_string(),
            }],
            summary: Summary {
                total: 2,
                resolved: 1,
                unresolved: 1,
            },
        };

        let output = emit_json(&registry_meta, &result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["mappings"][0]["input"], "[REDACTED]");
        assert_eq!(parsed["mappings"][0]["canonical_id"], "[REDACTED]");
        assert_eq!(parsed["mappings"][0]["canonical_type"], "ticker");
        assert_eq!(parsed["mappings"][0]["rule_id"], "CUSIP_TO_TICKER");
        assert_eq!(parsed["unresolved"][0]["input"], "[REDACTED]");
        assert_eq!(parsed["unresolved"][0]["reason"], "no matching rule");
    }

    #[test]
    fn refusal_envelope_serializes_expected_structure() {
        let refusal_output = create_refusal(
            RefusalCode::EColumnNotFound,
            "Column 'cusip' not found in input file".to_string(),
            json!({
                "column": "cusip",
                "available_columns": ["security_id", "isin", "name"]
            }),
            Some(
                "canon positions.csv --registry registries/cusip-isin/ --column security_id"
                    .to_string(),
            ),
        );

        let serialized = serde_json::to_string(&refusal_output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        assert_eq!(parsed["version"], "canon.v0");
        assert_eq!(parsed["outcome"], "REFUSAL");
        assert_eq!(parsed["registry"], serde_json::Value::Null);
        assert_eq!(parsed["summary"], serde_json::Value::Null);
        assert_eq!(parsed["mappings"], serde_json::Value::Array(vec![]));
        assert_eq!(parsed["unresolved"], serde_json::Value::Array(vec![]));
        assert_eq!(parsed["refusal"]["code"], "E_COLUMN_NOT_FOUND");
        assert_eq!(
            parsed["refusal"]["message"],
            "Column 'cusip' not found in input file"
        );
        assert_eq!(parsed["refusal"]["detail"]["column"], "cusip");
        assert_eq!(
            parsed["refusal"]["next_command"],
            "canon positions.csv --registry registries/cusip-isin/ --column security_id"
        );
    }
}
