//! bd-rgjo stage 2: `supersede`, a reviewed correction that records succession,
//! not sameness. The old id stays in the registry with its own keys and names;
//! it is flagged `superseded_by` the successor. The spellings the correction
//! names move to the successor, so a shared name resolves to the active holder
//! only. Same rules as merge: a named human reviewer, cited evidence, a stated
//! effective date, a new registry version, never automatic.

use super::{Correction, Model, RegistryCorrectionSummary, entry_str, key_namespace, ops, refusal};
use crate::Refusal;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// What a succession can be. Only `merged_into` exists today.
pub(super) const SUCCESSION_KINDS: &[&str] = &["merged_into"];
pub(super) const SUCCESSION_NAME_RULE: &str = "SUCCESSION_NAME_HOLDER";

#[derive(Debug, Clone)]
pub(super) struct SupersedeLine {
    pub review: ops::Review,
    pub superseded: String,
    pub successor: String,
    pub succession: String,
    pub effective_date: String,
    pub inputs: Vec<String>,
}

impl SupersedeLine {
    pub(super) fn canonical(&self) -> Value {
        let mut object = serde_json::Map::new();
        object.insert("op".to_string(), json!("supersede"));
        object.insert("superseded".to_string(), json!(self.superseded));
        object.insert("successor".to_string(), json!(self.successor));
        object.insert("succession".to_string(), json!(self.succession));
        object.insert("effective_date".to_string(), json!(self.effective_date));
        object.insert("inputs".to_string(), json!(self.inputs));
        self.review.put(&mut object);
        Value::Object(object)
    }
}

pub(super) fn parse(
    registry: &Path,
    value: &Value,
    number: usize,
    review: ops::Review,
) -> Result<Correction, Refusal> {
    let need = |name: &str| -> Result<String, Refusal> {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                refusal(
                    registry,
                    format!("corrections line {number}: '{name}' is required"),
                    json!({"line": number, "field": name, "writes_performed": false}),
                    "Complete the supersede line, then rerun",
                )
            })
    };
    let superseded = need("superseded")?;
    let successor = need("successor")?;
    if superseded == successor {
        return Err(refusal(
            registry,
            format!("corrections line {number}: an id cannot supersede itself"),
            json!({"line": number, "id": superseded, "writes_performed": false}),
            "Name two different canonical ids",
        ));
    }
    let succession = need("succession")?;
    if !SUCCESSION_KINDS.contains(&succession.as_str()) {
        return Err(refusal(
            registry,
            format!(
                "corrections line {number}: succession must be one of {}",
                SUCCESSION_KINDS.join(", ")
            ),
            json!({"line": number, "succession": succession, "writes_performed": false}),
            "Use a listed succession kind",
        ));
    }
    let effective_date = need("effective_date")?;
    let valid_date = chrono::NaiveDate::parse_from_str(&effective_date, "%Y-%m-%d")
        .is_ok_and(|date| date.format("%Y-%m-%d").to_string() == effective_date);
    if !valid_date {
        return Err(refusal(
            registry,
            format!("corrections line {number}: effective_date must be a calendar date YYYY-MM-DD"),
            json!({"line": number, "effective_date": effective_date, "writes_performed": false}),
            "State the date the cited evidence gives; canon never infers one",
        ));
    }
    Ok(Correction::Supersede(SupersedeLine {
        review,
        superseded,
        successor,
        succession,
        effective_date,
        inputs: ops::strings(value.get("inputs")),
    }))
}

/// Successions already recorded in the corrections file, old id to successor.
pub(super) fn recorded(corrections_text: &str) -> BTreeMap<String, String> {
    corrections_text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line.get("op").and_then(Value::as_str) == Some("supersede"))
        .filter_map(|line| {
            Some((
                line.get("superseded")?.as_str()?.to_string(),
                line.get("successor")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

pub(super) fn apply(
    registry: &Path,
    model: &mut Model,
    line: &SupersedeLine,
    ctx: &ops::Ctx,
) -> Result<RegistryCorrectionSummary, Refusal> {
    for (role, id) in [
        ("superseded", &line.superseded),
        ("successor", &line.successor),
    ] {
        if model.entries_of(id).next().is_none() {
            return Err(refusal(
                registry,
                format!("{role} '{id}' is not a canonical id in this registry"),
                json!({"role": role, "id": id, "writes_performed": false}),
                "Name two canonical ids that exist",
            ));
        }
    }
    let old_types = model.types_of(&line.superseded);
    let new_types = model.types_of(&line.successor);
    if old_types != new_types {
        return Err(refusal(
            registry,
            "the two ids have different canonical types",
            json!({
                "superseded": line.superseded,
                "superseded_types": old_types,
                "successor": line.successor,
                "successor_types": new_types,
                "writes_performed": false,
            }),
            "Only ids of one canonical_type supersede each other",
        ));
    }
    let known = recorded(&model.corrections_text);
    if let Some(existing) = known.get(&line.superseded) {
        return Err(refusal(
            registry,
            format!(
                "'{}' is already superseded by '{existing}'",
                line.superseded
            ),
            json!({
                "superseded": line.superseded,
                "already_superseded_by": existing,
                "writes_performed": false,
            }),
            "Supersede the active end of the chain instead",
        ));
    }
    // The successor's chain must not lead back to the superseded id.
    let mut cursor = line.successor.clone();
    let mut hops = 0;
    while let Some(next) = known.get(&cursor) {
        if *next == line.superseded || hops > known.len() {
            return Err(refusal(
                registry,
                "the succession would make a cycle",
                json!({
                    "superseded": line.superseded,
                    "successor": line.successor,
                    "writes_performed": false,
                }),
                "A succession chain ends at one active id",
            ));
        }
        cursor = next.clone();
        hops += 1;
    }

    // Name inputs: a spelling the superseded id holds moves to the successor; a
    // spelling nobody holds (a shared name) is written to the successor as a new
    // alias carrying this correction's closure. The alias goes into a file that
    // already holds spellings of this canonical type, never into a key file.
    let successor_type = model
        .types_of(&line.successor)
        .into_iter()
        .next()
        .unwrap_or_default();
    let alias_file = model
        .mapping
        .iter()
        .flat_map(|(path, rows)| rows.iter().map(move |entry| (path, entry)))
        .filter(|(_, entry)| key_namespace(entry_str(entry, "rule_id")).is_none())
        .find(|(_, entry)| {
            entry_str(entry, "canonical_id") == line.successor
                || entry_str(entry, "canonical_type") == successor_type
        })
        .map(|(path, _)| path.clone());
    let authority = json!({
        "version": "canon.registry.correction.authority.v1",
        "outcome_ref": ctx.outcome_ref,
        "closure_hash": ctx.closure_hash,
        "correction_id": ctx.correction_id,
    });
    let mark = json!({
        "op": "supersede",
        "from": line.superseded,
        "correction_id": ctx.correction_id,
    });
    let mut moved_inputs = Vec::new();
    let mut added_inputs = Vec::new();
    for input in &line.inputs {
        let mut holders = Vec::<(PathBuf, usize, String)>::new();
        for (path, rows) in &model.mapping {
            for (index, entry) in rows.iter().enumerate() {
                if entry_str(entry, "input") == input
                    && key_namespace(entry_str(entry, "rule_id")).is_none()
                    && entry.get("scope").is_none_or(Value::is_null)
                {
                    holders.push((
                        path.clone(),
                        index,
                        entry_str(entry, "canonical_id").to_string(),
                    ));
                }
            }
        }
        if let Some((_, _, other)) = holders
            .iter()
            .find(|(_, _, holder)| *holder != line.superseded && *holder != line.successor)
        {
            return Err(refusal(
                registry,
                format!("'{input}' already names '{other}', not the superseded or successor id"),
                json!({
                    "input": input,
                    "held_by": other,
                    "superseded": line.superseded,
                    "successor": line.successor,
                    "writes_performed": false,
                }),
                "Name only spellings held by the superseded id or by nobody",
            ));
        }
        if holders.is_empty() {
            let Some(path) = alias_file.clone() else {
                return Err(refusal(
                    registry,
                    "the registry has no spelling file to place the name in",
                    json!({"input": input, "successor": line.successor, "writes_performed": false}),
                    "Add a spelling entry to the registry, then rerun",
                ));
            };
            let entry = json!({
                "input": input,
                "canonical_id": line.successor,
                "canonical_type": successor_type,
                "rule_id": SUCCESSION_NAME_RULE,
                "session_authority": authority,
                "correction": mark,
            });
            if let Some(rows) = model.mapping.get_mut(&path) {
                rows.push(entry);
            }
            added_inputs.push(input.clone());
            continue;
        }
        for (path, index, holder) in holders {
            if holder == line.superseded
                && let Some(entry) = model
                    .mapping
                    .get_mut(&path)
                    .and_then(|rows| rows.get_mut(index))
                && let Some(object) = entry.as_object_mut()
            {
                object.insert("canonical_id".to_string(), json!(line.successor));
                object.insert("correction".to_string(), mark.clone());
                moved_inputs.push(input.clone());
            }
        }
    }
    // A spelling an earlier supersede placed on the superseded id follows it to
    // the successor without being listed again, so a shared name keeps resolving
    // to the current holder. The superseded id's own original spellings stay.
    for rows in model.mapping.values_mut() {
        for entry in rows.iter_mut() {
            let placed_by_supersede = entry
                .get("correction")
                .and_then(|value| value.get("op"))
                .and_then(Value::as_str)
                == Some("supersede");
            if placed_by_supersede
                && entry_str(entry, "canonical_id") == line.superseded
                && key_namespace(entry_str(entry, "rule_id")).is_none()
                && entry.get("scope").is_none_or(Value::is_null)
                && let Some(object) = entry.as_object_mut()
            {
                let input = object
                    .get("input")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                object.insert("canonical_id".to_string(), json!(line.successor));
                object.insert("correction".to_string(), mark.clone());
                moved_inputs.push(input);
            }
        }
    }
    moved_inputs.sort();
    moved_inputs.dedup();
    // The two stay two entities: registry memory says they never merge.
    let memory_written = usize::from(ops::add_cannot_link(
        model,
        &format!("registry:{}", line.superseded),
        &format!("registry:{}", line.successor),
        "superseded",
        ctx,
    ));
    Ok(RegistryCorrectionSummary {
        correction_id: ctx.correction_id.clone(),
        op: "supersede".to_string(),
        canonical_id: Some(line.superseded.clone()),
        survivor: Some(line.successor.clone()),
        outcome_ref: ctx.outcome_ref.clone(),
        closure_hash: ctx.closure_hash.clone(),
        moved: moved_inputs.len() + added_inputs.len(),
        memory_written,
        stored: json!({
            "superseded": line.superseded,
            "successor": line.successor,
            "succession": line.succession,
            "effective_date": line.effective_date,
            "inputs": line.inputs,
            "moved_inputs": moved_inputs,
            "added_inputs": added_inputs,
            "memory_written": memory_written,
        }),
        ..Default::default()
    })
}
