//! The other corrections beside merge (bd-vdcw.53): detach an alias or key from
//! an id, split one id into two, and remove a cannot-link from registry memory.
//! Same rules as merge: reviewed, evidenced, a new registry version, never
//! automatic. Nothing here rewrites history: a detached entry is recorded on the
//! correction line with the outcome it carried, and the corrections file is
//! append-only.

use super::{
    Correction, Model, RegistryCorrectionSummary, entry_str, identity_names, key_namespace, refusal,
};
use crate::Refusal;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub(super) struct Ctx {
    pub correction_id: String,
    pub closure_hash: String,
    pub outcome_ref: String,
}

#[derive(Debug, Clone)]
pub(super) struct Review {
    pub reviewer: String,
    pub evidence_ids: Vec<String>,
    pub reason: String,
    pub source: Value,
}

impl Review {
    pub(super) fn parts(&self) -> (&str, &[String], &str, &Value) {
        (
            &self.reviewer,
            &self.evidence_ids,
            &self.reason,
            &self.source,
        )
    }

    fn put(&self, object: &mut serde_json::Map<String, Value>) {
        object.insert("reviewer".to_string(), json!(self.reviewer));
        object.insert("evidence_ids".to_string(), json!(self.evidence_ids));
        object.insert("reason".to_string(), json!(self.reason));
        object.insert("source".to_string(), self.source.clone());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct KeySpec {
    pub namespace: String,
    pub value: String,
}

fn key_values(keys: &[KeySpec]) -> Value {
    json!(
        keys.iter()
            .map(|key| json!({"namespace": key.namespace, "value": key.value}))
            .collect::<Vec<_>>()
    )
}

#[derive(Debug, Clone)]
pub(super) struct DetachLine {
    pub review: Review,
    pub canonical_id: String,
    pub inputs: Vec<String>,
    pub keys: Vec<KeySpec>,
}

impl DetachLine {
    pub(super) fn canonical(&self) -> Value {
        let mut object = serde_json::Map::new();
        object.insert("op".to_string(), json!("detach"));
        object.insert("canonical_id".to_string(), json!(self.canonical_id));
        object.insert("inputs".to_string(), json!(self.inputs));
        object.insert("keys".to_string(), key_values(&self.keys));
        self.review.put(&mut object);
        Value::Object(object)
    }
}

#[derive(Debug, Clone)]
pub(super) struct SplitLine {
    pub review: Review,
    pub canonical_id: String,
    pub inputs: Vec<String>,
    pub keys: Vec<KeySpec>,
}

impl SplitLine {
    pub(super) fn canonical(&self) -> Value {
        let mut object = serde_json::Map::new();
        object.insert("op".to_string(), json!("split"));
        object.insert("canonical_id".to_string(), json!(self.canonical_id));
        object.insert(
            "move".to_string(),
            json!({"inputs": self.inputs, "keys": key_values(&self.keys)}),
        );
        self.review.put(&mut object);
        Value::Object(object)
    }
}

#[derive(Debug, Clone)]
pub(super) struct RemoveLine {
    pub review: Review,
    pub left: String,
    pub right: String,
}

impl RemoveLine {
    pub(super) fn canonical(&self) -> Value {
        let mut object = serde_json::Map::new();
        object.insert("op".to_string(), json!("remove_cannot_link"));
        object.insert("left".to_string(), json!(self.left));
        object.insert("right".to_string(), json!(self.right));
        self.review.put(&mut object);
        Value::Object(object)
    }
}

fn strings(value: Option<&Value>) -> Vec<String> {
    let mut out = value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

fn key_specs(value: Option<&Value>) -> Vec<KeySpec> {
    let mut out = value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let namespace = item.get("namespace")?.as_str()?.trim().to_ascii_lowercase();
                    let value = item.get("value")?.as_str()?.trim().to_string();
                    (!namespace.is_empty() && !value.is_empty())
                        .then_some(KeySpec { namespace, value })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

pub(super) fn parse_op(
    registry: &Path,
    op: &str,
    value: &Value,
    number: usize,
    review: Review,
) -> Result<Correction, Refusal> {
    let text = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let need = |name: &str| -> Result<String, Refusal> {
        text(name).ok_or_else(|| {
            refusal(
                registry,
                format!("corrections line {number}: '{name}' is required"),
                json!({"line": number, "field": name, "writes_performed": false}),
                "Complete the correction line, then rerun",
            )
        })
    };
    match op {
        "detach" => {
            let inputs = strings(value.get("inputs"));
            let keys = key_specs(value.get("keys"));
            if inputs.is_empty() && keys.is_empty() {
                return Err(refusal(
                    registry,
                    format!("corrections line {number}: a detach names at least one input or key"),
                    json!({"line": number, "writes_performed": false}),
                    "List the spellings or keys to take off the id",
                ));
            }
            Ok(Correction::Detach(DetachLine {
                review,
                canonical_id: need("canonical_id")?,
                inputs,
                keys,
            }))
        }
        "split" => {
            let moved = value.get("move");
            let inputs = strings(moved.and_then(|moved| moved.get("inputs")));
            let keys = key_specs(moved.and_then(|moved| moved.get("keys")));
            if inputs.is_empty() && keys.is_empty() {
                return Err(refusal(
                    registry,
                    format!("corrections line {number}: a split needs a non-empty move list"),
                    json!({"line": number, "writes_performed": false}),
                    "List the inputs and keys to move to the new id",
                ));
            }
            Ok(Correction::Split(SplitLine {
                review,
                canonical_id: need("canonical_id")?,
                inputs,
                keys,
            }))
        }
        _ => Ok(Correction::RemoveCannotLink(RemoveLine {
            review,
            left: need("left")?,
            right: need("right")?,
        })),
    }
}

/// Where a selected entry sits: file and index.
type Slot = (PathBuf, usize);

fn select(
    registry: &Path,
    model: &Model,
    id: &str,
    inputs: &[String],
    keys: &[KeySpec],
) -> Result<Vec<Slot>, Refusal> {
    if model.entries_of(id).next().is_none() {
        return Err(refusal(
            registry,
            format!("'{id}' is not a canonical id in this registry"),
            json!({"canonical_id": id, "writes_performed": false}),
            "Name a canonical id that exists",
        ));
    }
    let mut slots = Vec::new();
    let mut missing = Vec::new();
    for input in inputs {
        let before = slots.len();
        for (path, rows) in &model.mapping {
            for (index, entry) in rows.iter().enumerate() {
                if entry_str(entry, "canonical_id") == id
                    && entry_str(entry, "input") == input
                    && key_namespace(entry_str(entry, "rule_id")).is_none()
                {
                    slots.push((path.clone(), index));
                }
            }
        }
        if slots.len() == before {
            missing.push(json!({"input": input}));
        }
    }
    for key in keys {
        let before = slots.len();
        for (path, rows) in &model.mapping {
            for (index, entry) in rows.iter().enumerate() {
                if entry_str(entry, "canonical_id") == id
                    && entry_str(entry, "input") == key.value
                    && key_namespace(entry_str(entry, "rule_id")).as_deref()
                        == Some(key.namespace.as_str())
                {
                    slots.push((path.clone(), index));
                }
            }
        }
        if slots.len() == before {
            missing.push(json!({"namespace": key.namespace, "value": key.value}));
        }
    }
    if !missing.is_empty() {
        return Err(refusal(
            registry,
            format!("'{id}' does not have every entry the correction names"),
            json!({"canonical_id": id, "missing": missing, "writes_performed": false}),
            "Name only inputs and keys the id carries",
        ));
    }
    slots.sort();
    slots.dedup();
    Ok(slots)
}

fn authority(entry: &Value, name: &str) -> Value {
    entry
        .get("session_authority")
        .and_then(|authority| authority.get(name))
        .cloned()
        .unwrap_or(Value::Null)
}

fn entry_report(entry: &Value) -> Value {
    json!({
        "input": entry_str(entry, "input"),
        "rule_id": entry_str(entry, "rule_id"),
        "outcome_ref": authority(entry, "outcome_ref"),
        "closure_hash": authority(entry, "closure_hash"),
    })
}

fn ordered(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn pair_of(text: &str) -> Option<(String, String)> {
    let fact = serde_json::from_str::<Value>(text).ok()?;
    Some(ordered(
        entry_str(&fact, "left_key"),
        entry_str(&fact, "right_key"),
    ))
}

/// Write one cannot-link fact to registry memory; `false` when the pair is
/// already there.
fn add_cannot_link(model: &mut Model, left: &str, right: &str, reason: &str, ctx: &Ctx) -> bool {
    let (left_key, right_key) = ordered(left, right);
    let Some((_, lines)) = model.cannot_link.as_mut() else {
        return false;
    };
    if lines
        .iter()
        .any(|text| pair_of(text) == Some((left_key.clone(), right_key.clone())))
    {
        return false;
    }
    lines.push(
        json!({
            "left_key": left_key,
            "right_key": right_key,
            "reason": reason,
            "outcome_ref": ctx.outcome_ref,
            "closure_hash": ctx.closure_hash,
        })
        .to_string(),
    );
    lines.sort_by_key(|text| pair_of(text));
    true
}

pub(super) fn apply_detach(
    registry: &Path,
    model: &mut Model,
    line: &DetachLine,
    ctx: &Ctx,
) -> Result<RegistryCorrectionSummary, Refusal> {
    let slots = select(
        registry,
        model,
        &line.canonical_id,
        &line.inputs,
        &line.keys,
    )?;
    let total = model.entries_of(&line.canonical_id).count();
    if slots.len() >= total {
        return Err(refusal(
            registry,
            "detaching every entry would leave the id with nothing; retire it with a merge instead",
            json!({"canonical_id": line.canonical_id, "writes_performed": false}),
            "Use a merge to retire an id",
        ));
    }
    let removed = slots
        .iter()
        .filter_map(|(path, index)| model.mapping.get(path).and_then(|rows| rows.get(*index)))
        .map(entry_report)
        .collect::<Vec<_>>();
    for (path, rows) in &mut model.mapping {
        let mut index = 0;
        rows.retain(|_| {
            let keep = !slots.contains(&(path.clone(), index));
            index += 1;
            keep
        });
    }
    // Each removed spelling and key becomes a cannot-link against the id, so
    // the next session does not re-attach it.
    let mut memory_written = 0;
    for input in &line.inputs {
        memory_written += usize::from(add_cannot_link(
            model,
            &format!("surface:{input}"),
            &line.canonical_id,
            "detached",
            ctx,
        ));
    }
    for key in &line.keys {
        memory_written += usize::from(add_cannot_link(
            model,
            &format!("{}:{}", key.namespace, key.value),
            &line.canonical_id,
            "detached",
            ctx,
        ));
    }
    Ok(RegistryCorrectionSummary {
        correction_id: ctx.correction_id.clone(),
        op: "detach".to_string(),
        canonical_id: Some(line.canonical_id.clone()),
        outcome_ref: ctx.outcome_ref.clone(),
        closure_hash: ctx.closure_hash.clone(),
        detached: slots.len(),
        memory_written,
        stored: json!({
            "canonical_id": line.canonical_id,
            "inputs": line.inputs,
            "keys": key_values(&line.keys),
            "removed": removed,
            "memory_written": memory_written,
        }),
        ..Default::default()
    })
}

/// A new id in the old id's scheme: the same prefix and width, one past the
/// highest id the registry has used, retired ids included.
fn allocate_id(registry: &Path, model: &Model, old: &str) -> Result<String, Refusal> {
    let Some((prefix, digits)) = old.rsplit_once('-') else {
        return Err(refusal(
            registry,
            format!("cannot derive an id scheme from '{old}'"),
            json!({"canonical_id": old, "writes_performed": false}),
            "Split needs ids of the form PREFIX-NNN",
        ));
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(refusal(
            registry,
            format!("cannot derive an id scheme from '{old}'"),
            json!({"canonical_id": old, "writes_performed": false}),
            "Split needs ids of the form PREFIX-NNN",
        ));
    }
    let width = digits.len();
    let namespace = format!("{prefix}-");
    let mut used = model
        .mapping
        .values()
        .flatten()
        .map(|entry| entry_str(entry, "canonical_id").to_string())
        .collect::<BTreeSet<_>>();
    used.extend(
        model
            .corrections_text
            .lines()
            .filter_map(|text| serde_json::from_str::<Value>(text).ok())
            .filter_map(|line| {
                line.get("retired")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
    );
    let highest = used
        .iter()
        .filter_map(|id| id.strip_prefix(&namespace))
        .filter(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
        .filter_map(|rest| rest.parse::<u128>().ok())
        .max()
        .unwrap_or(0);
    Ok(format!("{namespace}{:0width$}", highest + 1))
}

pub(super) fn apply_split(
    registry: &Path,
    model: &mut Model,
    line: &SplitLine,
    ctx: &Ctx,
) -> Result<RegistryCorrectionSummary, Refusal> {
    let slots = select(
        registry,
        model,
        &line.canonical_id,
        &line.inputs,
        &line.keys,
    )?;
    let total = model.entries_of(&line.canonical_id).count();
    if slots.len() >= total {
        return Err(refusal(
            registry,
            "a split cannot move every entry of the id",
            json!({"canonical_id": line.canonical_id, "writes_performed": false}),
            "Leave at least one entry on the old id",
        ));
    }
    let new_id = allocate_id(registry, model, &line.canonical_id)?;
    let mark = json!({
        "op": "split",
        "from": line.canonical_id,
        "correction_id": ctx.correction_id,
    });
    let mut moved = Vec::new();
    for (path, index) in &slots {
        let Some(entry) = model
            .mapping
            .get_mut(path)
            .and_then(|rows| rows.get_mut(*index))
        else {
            continue;
        };
        moved.push(entry_report(entry));
        if let Some(object) = entry.as_object_mut() {
            object.insert("canonical_id".to_string(), json!(new_id));
            object.insert("correction".to_string(), mark.clone());
        }
    }
    let memory_written = usize::from(add_cannot_link(
        model,
        &line.canonical_id,
        &new_id,
        "split",
        ctx,
    ));
    let relations_left = model.relations.as_ref().map_or(0, |(_, lines)| {
        lines
            .iter()
            .filter_map(|text| serde_json::from_str::<Value>(text).ok())
            .filter(|fact| {
                fact.pointer("/subject/identity_id").and_then(Value::as_str)
                    == Some(line.canonical_id.as_str())
                    || fact.pointer("/object/identity_id").and_then(Value::as_str)
                        == Some(line.canonical_id.as_str())
            })
            .count()
    });
    let kept = model.entries_of(&line.canonical_id).count();
    Ok(RegistryCorrectionSummary {
        correction_id: ctx.correction_id.clone(),
        op: "split".to_string(),
        canonical_id: Some(line.canonical_id.clone()),
        new_id: Some(new_id.clone()),
        outcome_ref: ctx.outcome_ref.clone(),
        closure_hash: ctx.closure_hash.clone(),
        moved: slots.len(),
        memory_written,
        relations_left_on_old_id: relations_left,
        stored: json!({
            "canonical_id": line.canonical_id,
            "new_id": new_id,
            "moved": moved,
            "kept": kept,
            "relations_left_on_old_id": relations_left,
            "memory_written": memory_written,
        }),
        ..Default::default()
    })
}

pub(super) fn apply_remove_cannot_link(
    registry: &Path,
    model: &mut Model,
    line: &RemoveLine,
    ctx: &Ctx,
) -> Result<RegistryCorrectionSummary, Refusal> {
    let names = |model: &Model, id: &str| -> BTreeSet<String> {
        if model.entries_of(id).next().is_some() {
            identity_names(model, id)
        } else {
            BTreeSet::from([id.to_string()])
        }
    };
    let left = names(model, &line.left);
    let right = names(model, &line.right);
    let mut removed = Vec::new();
    if let Some((_, lines)) = model.cannot_link.as_mut() {
        lines.retain(|text| {
            let Ok(fact) = serde_json::from_str::<Value>(text) else {
                return true;
            };
            let (l, r) = (
                entry_str(&fact, "left_key").to_string(),
                entry_str(&fact, "right_key").to_string(),
            );
            let matches = (left.contains(&l) && right.contains(&r))
                || (left.contains(&r) && right.contains(&l));
            if matches {
                removed.push(json!({
                    "left_key": l,
                    "right_key": r,
                    "reason": fact.get("reason").cloned().unwrap_or(Value::Null),
                    "outcome_ref": fact.get("outcome_ref").cloned().unwrap_or(Value::Null),
                    "closure_hash": fact.get("closure_hash").cloned().unwrap_or(Value::Null),
                }));
            }
            !matches
        });
    }
    if removed.is_empty() {
        return Err(refusal(
            registry,
            "registry memory holds no cannot-link between those two",
            json!({"left": line.left, "right": line.right, "writes_performed": false}),
            "Name two ids or keys that have a cannot-link",
        ));
    }
    Ok(RegistryCorrectionSummary {
        correction_id: ctx.correction_id.clone(),
        op: "remove_cannot_link".to_string(),
        outcome_ref: ctx.outcome_ref.clone(),
        closure_hash: ctx.closure_hash.clone(),
        cannot_links_removed: removed.len(),
        stored: json!({
            "left": line.left,
            "right": line.right,
            "removed_cannot_links": removed,
        }),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, path::PathBuf};

    fn ctx() -> Ctx {
        Ctx {
            correction_id: "correction:abc".to_string(),
            closure_hash: "blake3:abc".to_string(),
            outcome_ref: "cmdrvl://canon/outcome/orgs/correction/abc".to_string(),
        }
    }

    fn review() -> Review {
        Review {
            reviewer: "human:zac".to_string(),
            evidence_ids: vec!["evidence:blake3:a".to_string()],
            reason: "r".to_string(),
            source: Value::Null,
        }
    }

    fn entry(input: &str, id: &str, rule: &str, closure: &str) -> Value {
        json!({
            "input": input,
            "canonical_id": id,
            "canonical_type": "firm",
            "rule_id": rule,
            "session_authority": {"outcome_ref": format!("cmdrvl://o/{closure}"), "closure_hash": closure},
        })
    }

    fn model(entries: Vec<Value>) -> Model {
        Model {
            registry_path: PathBuf::from("registry.json"),
            registry_id: "orgs".to_string(),
            version_before: "1.0.0".to_string(),
            mapping: BTreeMap::from([(PathBuf::from("aliases.json"), entries)]),
            relations: None,
            cannot_link: Some((PathBuf::from("cannot_link.jsonl"), Vec::new())),
            anchors: Vec::new(),
            corrections_text: String::new(),
        }
    }

    fn two_entities() -> Model {
        model(vec![
            entry("Apex Holdings", "ORG-001", "SESSION_TIER_A", "h1"),
            entry("Apex Capital LLC", "ORG-001", "SESSION_TIER_A", "h2"),
            entry("111", "ORG-001", "SESSION_KEY_CIK", "h3"),
            entry("444", "ORG-001", "SESSION_KEY_CIK", "h4"),
            entry("ORG-009", "ORG-009", "ID", "h5"),
        ])
    }

    #[test]
    fn detach_removes_the_entry_and_writes_a_cannot_link_against_the_id() {
        let mut model = two_entities();
        let line = DetachLine {
            review: review(),
            canonical_id: "ORG-001".to_string(),
            inputs: vec!["Apex Capital LLC".to_string()],
            keys: vec![KeySpec {
                namespace: "cik".to_string(),
                value: "444".to_string(),
            }],
        };
        let summary = apply_detach(Path::new("."), &mut model, &line, &ctx()).unwrap();
        assert_eq!(summary.detached, 2);
        assert_eq!(summary.memory_written, 2);
        let rows = &model.mapping[Path::new("aliases.json")];
        assert_eq!(rows.len(), 3);
        let (_, lines) = model.cannot_link.as_ref().unwrap();
        let text = lines.join("\n");
        assert!(text.contains("surface:Apex Capital LLC"), "{text}");
        assert!(text.contains("cik:444"));
        assert!(text.contains("ORG-001"));
        // The removed entries' closures stay reachable from the line.
        let removed = summary.stored["removed"].as_array().unwrap();
        assert_eq!(removed[0]["closure_hash"], "h2");
        assert_eq!(removed[1]["closure_hash"], "h4");
    }

    #[test]
    fn detaching_the_last_entry_or_a_missing_one_is_refused() {
        let mut model = model(vec![entry("Only", "ORG-001", "SESSION_TIER_A", "h1")]);
        let last = DetachLine {
            review: review(),
            canonical_id: "ORG-001".to_string(),
            inputs: vec!["Only".to_string()],
            keys: Vec::new(),
        };
        let error = apply_detach(Path::new("."), &mut model, &last, &ctx()).unwrap_err();
        assert!(error.message.contains("merge"), "{}", error.message);
        let missing = DetachLine {
            review: review(),
            canonical_id: "ORG-001".to_string(),
            inputs: vec!["Nope".to_string()],
            keys: Vec::new(),
        };
        let error =
            apply_detach(Path::new("."), &mut two_entities(), &missing, &ctx()).unwrap_err();
        assert_eq!(error.detail["missing"][0]["input"], "Nope");
    }

    #[test]
    fn split_moves_the_named_entries_to_a_new_id_and_records_a_cannot_link() {
        let mut model = two_entities();
        let line = SplitLine {
            review: review(),
            canonical_id: "ORG-001".to_string(),
            inputs: vec!["Apex Capital LLC".to_string()],
            keys: vec![KeySpec {
                namespace: "cik".to_string(),
                value: "444".to_string(),
            }],
        };
        let summary = apply_split(Path::new("."), &mut model, &line, &ctx()).unwrap();
        // ORG-009 is the highest id in the registry.
        assert_eq!(summary.new_id.as_deref(), Some("ORG-010"));
        assert_eq!(summary.moved, 2);
        let rows = &model.mapping[Path::new("aliases.json")];
        let moved = rows
            .iter()
            .filter(|row| entry_str(row, "canonical_id") == "ORG-010")
            .collect::<Vec<_>>();
        assert_eq!(moved.len(), 2);
        assert!(moved.iter().all(|row| row["correction"]["op"] == "split"));
        assert!(
            moved
                .iter()
                .all(|row| row["session_authority"]["closure_hash"].is_string()),
            "moved entries keep their closure"
        );
        let (_, lines) = model.cannot_link.as_ref().unwrap();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("ORG-001") && lines[0].contains("ORG-010"));
    }

    #[test]
    fn split_refuses_to_move_everything_or_an_entry_the_id_lacks() {
        let everything = SplitLine {
            review: review(),
            canonical_id: "ORG-009".to_string(),
            inputs: vec!["ORG-009".to_string()],
            keys: Vec::new(),
        };
        assert!(apply_split(Path::new("."), &mut two_entities(), &everything, &ctx()).is_err());
        let lacking = SplitLine {
            review: review(),
            canonical_id: "ORG-001".to_string(),
            inputs: vec!["Not Here".to_string()],
            keys: Vec::new(),
        };
        assert!(apply_split(Path::new("."), &mut two_entities(), &lacking, &ctx()).is_err());
    }

    #[test]
    fn a_new_id_never_reuses_a_retired_one() {
        let mut model = two_entities();
        model.corrections_text = "{\"retired\":\"ORG-050\",\"op\":\"merge\"}\n".to_string();
        assert_eq!(
            allocate_id(Path::new("."), &model, "ORG-001").unwrap(),
            "ORG-051"
        );
        assert!(allocate_id(Path::new("."), &model, "NODASH").is_err());
    }

    #[test]
    fn remove_cannot_link_is_the_only_way_a_fact_leaves_memory() {
        let mut model = two_entities();
        let (_, lines) = model.cannot_link.as_mut().unwrap();
        lines.push(
            json!({"left_key": "ORG-001", "right_key": "ORG-009", "reason": "split",
                   "outcome_ref": "cmdrvl://o/x", "closure_hash": "hx"})
            .to_string(),
        );
        let line = RemoveLine {
            review: review(),
            left: "ORG-009".to_string(),
            right: "ORG-001".to_string(),
        };
        let summary = apply_remove_cannot_link(Path::new("."), &mut model, &line, &ctx()).unwrap();
        assert_eq!(summary.cannot_links_removed, 1);
        assert_eq!(
            summary.stored["removed_cannot_links"][0]["closure_hash"],
            "hx"
        );
        assert!(model.cannot_link.as_ref().unwrap().1.is_empty());
        // Nothing left to remove: refused.
        assert!(apply_remove_cannot_link(Path::new("."), &mut model, &line, &ctx()).is_err());
    }
}
