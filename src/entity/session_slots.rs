#![forbid(unsafe_code)]

//! `same_slot` in `session derive` (bd-6iif).
//!
//! A profile declares a support operator `same_slot` with `slot_fields`, a
//! comma-separated list. Two observations of one declared entity type, from
//! different lineages, whose values for every listed field are present and equal
//! hold the same slot. Values come from `context`, else `attrs`, and are
//! compared trimmed, with runs of whitespace collapsed, ignoring case.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::{Refusal, RefusalCode, entity::profile::EntityProfileDocument, keys::is_placeholder};

pub const SAME_SLOT_OPERATOR: &str = "same_slot";

/// A slot held by more observations than this does not identify one thing, and
/// pairing it would grow with the square of its size.
pub const MAX_SLOT_OBSERVATIONS: usize = 50;

/// What the slot operator did, reported in the derive output.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SlotStats {
    /// Slots held by at least two observations of one entity type.
    pub shared_slots: usize,
    /// Slots over [`MAX_SLOT_OBSERVATIONS`], which produced nothing.
    pub oversized_slots_skipped: usize,
}

/// The slot field lists the profile declares, one per `same_slot` operator.
pub fn slot_rules(profile: &EntityProfileDocument) -> Result<Vec<Vec<String>>, Refusal> {
    profile
        .evidence
        .support
        .iter()
        .filter(|spec| spec.op == SAME_SLOT_OPERATOR)
        .map(|spec| {
            let fields = spec
                .params
                .get("slot_fields")
                .map(|raw| raw.split(',').map(str::trim).collect::<Vec<_>>())
                .unwrap_or_default();
            if fields.is_empty() || fields.iter().any(|field| field.is_empty()) {
                return Err(Refusal {
                    code: RefusalCode::EParse,
                    message: "same_slot needs params.slot_fields: a comma-separated list of context or attrs fields".to_string(),
                    detail: json!({"operator": SAME_SLOT_OPERATOR, "field": "params.slot_fields", "writes_performed": false}),
                    next_command: Some(
                        "Add params.slot_fields to the same_slot operator, then rerun derive"
                            .to_string(),
                    ),
                });
            }
            Ok(fields.into_iter().map(str::to_string).collect())
        })
        .collect()
}

fn slot_part(observation: &Value, field: &str) -> Option<String> {
    let raw = ["context", "attrs"]
        .iter()
        .find_map(|object| observation.get(*object)?.get(field))?;
    let text = match raw {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty() && !is_placeholder(&text)).then(|| text.to_lowercase())
}

struct Holder<'a> {
    obs_id: &'a str,
    surface: &'a str,
    lineage: &'a str,
}

/// The `same_slot` assertion lines for `observations` under one field list.
pub fn slot_assertions<'a>(
    observations: impl IntoIterator<Item = &'a Value>,
    fields: &[String],
    namespace: &str,
    profile_hash: &str,
    stats: &mut SlotStats,
) -> Vec<Value> {
    let rule = format!("{SAME_SLOT_OPERATOR}:{}", fields.join("+"));
    let mut slots = BTreeMap::<(&str, Vec<String>), Vec<Holder<'a>>>::new();
    for observation in observations {
        let text = |name: &str| observation.get(name).and_then(Value::as_str);
        let lineage = observation
            .get("source")
            .and_then(|source| source.get("lineage"))
            .and_then(Value::as_str)
            .filter(|lineage| !lineage.trim().is_empty());
        let (Some(obs_id), Some(surface), Some(entity_type), Some(lineage)) = (
            text("obs_id"),
            text("surface"),
            text("entity_type"),
            lineage,
        ) else {
            continue;
        };
        let Some(values) = fields
            .iter()
            .map(|field| slot_part(observation, field))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        slots
            .entry((entity_type, values))
            .or_default()
            .push(Holder {
                obs_id,
                surface,
                lineage,
            });
    }

    let mut lines = Vec::new();
    for ((_, values), mut holders) in slots {
        if holders.len() < 2 {
            continue;
        }
        stats.shared_slots += 1;
        if holders.len() > MAX_SLOT_OBSERVATIONS {
            stats.oversized_slots_skipped += 1;
            continue;
        }
        holders.sort_by_key(|holder| holder.obs_id);
        let slot_value = fields
            .iter()
            .cloned()
            .zip(values.into_iter().map(Value::String))
            .collect::<Map<_, _>>();
        // one spelling is one node in solve, so such a pair says nothing
        let mut paired = BTreeSet::new();
        for (index, left) in holders.iter().enumerate() {
            for right in &holders[index + 1..] {
                if left.lineage == right.lineage
                    || left.surface == right.surface
                    || !paired.insert((left.obs_id, right.obs_id))
                {
                    continue;
                }
                lines.push(json!({
                    "type": SAME_SLOT_OPERATOR,
                    "left": left.obs_id,
                    "right": right.obs_id,
                    "basis": {
                        "rule": rule,
                        "slot_fields": fields,
                        "slot_value": slot_value,
                        "applies_to": "observation",
                        "explanation": "two lineages state the same slot",
                    },
                    "source": {"channel": "canon", "id": format!("derive:{rule}")},
                    "asserted_by": format!("canon:{namespace}:{rule}@{profile_hash}"),
                }));
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(obs_id: &str, entity_type: &str, lineage: &str, surface: &str, loan: Value) -> Value {
        json!({
            "obs_id": obs_id,
            "entity_type": entity_type,
            "surface": surface,
            "context": {"deal_id": "D 1", "loan_id": loan},
            "attrs": {"role": "Borrower"},
            "source": {"lineage": lineage},
        })
    }

    fn run(rows: &[Value]) -> (Vec<Value>, SlotStats) {
        let fields = ["deal_id", "loan_id", "role"].map(str::to_string);
        let mut stats = SlotStats::default();
        let lines = slot_assertions(rows, &fields, "p", "h", &mut stats);
        (lines, stats)
    }

    #[test]
    fn one_type_two_lineages_and_a_complete_equal_slot_make_one_record() {
        let (lines, stats) = run(&[
            row("b", "org", "two", "Acme Co", json!(" 7 ")),
            row("a", "org", "one", "Acme", json!(7)),
        ]);
        assert_eq!(stats.shared_slots, 1);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["left"], "a");
        assert_eq!(lines[0]["right"], "b");
        assert_eq!(
            lines[0]["basis"]["slot_value"],
            json!({"deal_id": "d 1", "loan_id": "7", "role": "borrower"})
        );
        assert_eq!(lines[0]["basis"]["applies_to"], "observation");
    }

    #[test]
    fn incomplete_slots_other_types_and_single_lineages_make_nothing() {
        for loan in [json!(""), json!("  "), json!("N/A"), json!(null)] {
            let (lines, _) = run(&[
                row("a", "org", "one", "Acme", loan.clone()),
                row("b", "org", "two", "Acme Co", loan.clone()),
            ]);
            assert!(lines.is_empty(), "{loan}: {lines:?}");
        }
        let (other_type, _) = run(&[
            row("a", "org", "one", "Acme", json!("7")),
            row("b", "person", "two", "Acme Co", json!("7")),
        ]);
        assert!(other_type.is_empty(), "{other_type:?}");
        let (one_lineage, _) = run(&[
            row("a", "org", "one", "Acme", json!("7")),
            row("b", "org", "one", "Acme Co", json!("7")),
        ]);
        assert!(one_lineage.is_empty(), "{one_lineage:?}");
    }

    #[test]
    fn a_slot_shared_too_widely_is_counted_and_skipped() {
        let rows = (0..=MAX_SLOT_OBSERVATIONS)
            .map(|index| {
                row(
                    &format!("o{index}"),
                    "org",
                    &format!("l{index}"),
                    &format!("Name {index}"),
                    json!("7"),
                )
            })
            .collect::<Vec<_>>();
        let (lines, stats) = run(&rows);
        assert!(lines.is_empty());
        assert_eq!(stats.oversized_slots_skipped, 1);
    }
}
