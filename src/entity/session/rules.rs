#![forbid(unsafe_code)]

//! Parsing and validation for the entity evidence-session rule table.
//!
//! The rule table is deliberately data-only.  This module owns the schema and
//! its deterministic hash; the solve module owns the graph decisions that
//! consume the parsed policy.

use crate::{Refusal, RefusalCode, witness::hash_bytes};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

pub const SESSION_RULES_VERSION: &str = "canon.entity.session.rules.v1";
pub const DEFAULT_MAX_CLUSTER_SURFACES: usize = 50;

const TIER_A_TYPES: &[&str] = &["same_key", "asserted_alias"];
const TIER_B_TYPES: &[&str] = &["same_slot", "attribute_agreement", "name_equivalent"];
const INCUMBENT_ATTACH_STEPS: &[&str] = &["casefold", "strip_punctuation", "collapse_whitespace"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyRule {
    pub authoritative: bool,
    pub single_valued: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TierBAlternative {
    pub required_evidence_types: Vec<String>,
    pub min_evidence_types: usize,
    pub min_independent_origins: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionRules {
    pub version: String,
    pub key_rules: BTreeMap<String, KeyRule>,
    pub authoritative_keys: Vec<String>,
    /// `commit.key_namespaces`: extra namespaces whose values commit writes as
    /// key aliases although they are not authoritative.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub commit_key_namespaces: Vec<String>,
    pub tier_a_types: Vec<String>,
    pub tier_b_types: Vec<String>,
    pub tier_b_alternatives: Vec<TierBAlternative>,
    pub min_distinct_evidence_types: usize,
    pub min_distinct_lineages: usize,
    pub required_evidence_types: Vec<String>,
    pub mint_when_authoritative_key: bool,
    pub max_cluster_surfaces: usize,
    pub incumbent_attach_steps: Vec<String>,
}

impl SessionRules {
    pub fn key_rule(&self, namespace: &str) -> Option<&KeyRule> {
        self.key_rules.get(namespace)
    }

    pub fn is_authoritative_key(&self, namespace: &str) -> bool {
        self.key_rule(namespace)
            .is_some_and(|rule| rule.authoritative)
    }

    pub fn is_single_valued_key(&self, namespace: &str) -> bool {
        self.key_rule(namespace)
            .is_some_and(|rule| rule.single_valued)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionRulesInitOutput {
    pub version: String,
    pub path: Option<String>,
    pub template: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionRulesValidationOutput {
    pub version: String,
    pub path: String,
    pub rules_hash: String,
    pub key_namespaces: Vec<String>,
    pub max_cluster_surfaces: usize,
}

pub fn parse_rules(path: &Path) -> Result<(SessionRules, String), Refusal> {
    let text = fs::read_to_string(path).map_err(|error| {
        rules_refusal(
            path,
            "<document>",
            format!("unable to read rules file: {error}"),
            None,
            &[],
        )
    })?;
    let value = serde_yaml::from_str::<Value>(&text).map_err(|error| {
        rules_refusal(
            path,
            "<document>",
            format!("invalid YAML: {error}"),
            None,
            &[],
        )
    })?;
    parse_value(path, &value)
}

pub fn validate_rules(path: &Path) -> Result<SessionRulesValidationOutput, Refusal> {
    let (rules, rules_hash) = parse_rules(path)?;
    Ok(SessionRulesValidationOutput {
        version: rules.version,
        path: path.display().to_string(),
        rules_hash,
        key_namespaces: rules.key_rules.keys().cloned().collect(),
        max_cluster_surfaces: rules.max_cluster_surfaces,
    })
}

pub fn init_rules(path: Option<&Path>) -> Result<SessionRulesInitOutput, Refusal> {
    let template = rules_template();
    if let Some(path) = path {
        fs::write(path, template.as_bytes()).map_err(|error| {
            rules_refusal(
                path,
                "<document>",
                format!("unable to write rules template: {error}"),
                None,
                &[],
            )
        })?;
    }
    Ok(SessionRulesInitOutput {
        version: SESSION_RULES_VERSION.to_string(),
        path: path.map(|path| path.display().to_string()),
        template,
    })
}

pub fn rules_template() -> String {
    format!(
        r#"# Deterministic entity evidence-session policy.
version: {SESSION_RULES_VERSION}
keys:
  cik: {{authoritative: true, single_valued: true}}
  lei: {{authoritative: true, single_valued: true}}
  ticker: {{authoritative: false, single_valued: false}}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
  required_evidence_types: []
standalone:
  mint_when_authoritative_key: true
guards:
  max_cluster_surfaces: {DEFAULT_MAX_CLUSTER_SURFACES}
incumbent_attach:
  name_equivalent_steps: [casefold, strip_punctuation, collapse_whitespace]
"#,
    )
}

fn parse_value(path: &Path, value: &Value) -> Result<(SessionRules, String), Refusal> {
    let root = value.as_object().ok_or_else(|| {
        rules_refusal(
            path,
            "<document>",
            "rules must be a YAML mapping",
            Some(value.clone()),
            &["mapping"],
        )
    })?;
    reject_unknown_keys(
        root,
        &[
            "version",
            "keys",
            "tier_a",
            "tier_b",
            "standalone",
            "guards",
            "incumbent_attach",
            "commit",
        ],
        path,
        "",
    )?;
    let version = required_string(root, "version", path, "version")?;
    if version != SESSION_RULES_VERSION {
        return Err(rules_refusal(
            path,
            "version",
            format!("expected {SESSION_RULES_VERSION}"),
            Some(Value::String(version)),
            &[SESSION_RULES_VERSION],
        ));
    }
    let key_rules = parse_keys(root, path)?;
    let authoritative_keys = key_rules
        .iter()
        .filter(|(_, rule)| rule.authoritative)
        .map(|(namespace, _)| namespace.clone())
        .collect::<Vec<_>>();
    let tier_a_types = parse_string_list(
        required_mapping(root, "tier_a", path, "tier_a")?,
        "evidence_types",
        path,
        "tier_a.evidence_types",
        TIER_A_TYPES,
    )?;
    let tier_b = required_mapping(root, "tier_b", path, "tier_b")?;
    let tier_b_types = parse_string_list(
        tier_b,
        "eligible_evidence_types",
        path,
        "tier_b.eligible_evidence_types",
        TIER_B_TYPES,
    )?;
    let required_evidence_types = optional_string_list(
        tier_b,
        "required_evidence_types",
        path,
        "tier_b.required_evidence_types",
        TIER_B_TYPES,
    )?
    .unwrap_or_default();
    for (index, required) in required_evidence_types.iter().enumerate() {
        if !tier_b_types.contains(required) {
            return Err(rules_refusal(
                path,
                &format!("tier_b.required_evidence_types[{index}]"),
                "required evidence type must be eligible",
                Some(Value::String(required.clone())),
                &tier_b_types.iter().map(String::as_str).collect::<Vec<_>>(),
            ));
        }
    }
    let min_distinct_evidence_types = optional_positive_usize(
        tier_b,
        "min_distinct_evidence_types",
        path,
        "tier_b.min_distinct_evidence_types",
    )?
    .unwrap_or(2);
    let min_distinct_lineages = optional_positive_usize(
        tier_b,
        "min_distinct_lineages",
        path,
        "tier_b.min_distinct_lineages",
    )?
    .unwrap_or(2);
    reject_unknown_keys(
        tier_b,
        &[
            "eligible_evidence_types",
            "min_distinct_evidence_types",
            "min_distinct_lineages",
            "required_evidence_types",
            "any_of",
        ],
        path,
        "tier_b",
    )?;
    let tier_b_alternatives = parse_alternatives(
        tier_b,
        path,
        &tier_b_types,
        &required_evidence_types,
        min_distinct_evidence_types,
        min_distinct_lineages,
    )?;
    let mint_when_authoritative_key = parse_standalone(root, path)?;
    let max_cluster_surfaces = parse_guards(root, path)?;
    let incumbent_attach_steps = parse_incumbent_attach(root, path)?;
    let commit_key_namespaces = parse_commit(root, path, &key_rules)?;
    let rules = SessionRules {
        version,
        key_rules,
        authoritative_keys,
        commit_key_namespaces,
        tier_a_types,
        tier_b_types,
        tier_b_alternatives,
        min_distinct_evidence_types,
        min_distinct_lineages,
        required_evidence_types,
        mint_when_authoritative_key,
        max_cluster_surfaces,
        incumbent_attach_steps,
    };
    let canonical = serde_json::to_vec(&rules).map_err(|error| {
        rules_refusal(
            path,
            "<document>",
            format!("cannot hash rules: {error}"),
            None,
            &[],
        )
    })?;
    Ok((rules, hash_bytes(&canonical)))
}

fn parse_keys(
    root: &Map<String, Value>,
    path: &Path,
) -> Result<BTreeMap<String, KeyRule>, Refusal> {
    let value = root
        .get("keys")
        .ok_or_else(|| rules_refusal(path, "keys", "value is required", None, &["mapping"]))?;
    let mut rules = BTreeMap::new();
    match value {
        Value::Array(_) => {
            for (index, namespace) in
                parse_string_list_value(value, path, "keys.authoritative", &[], false)?
                    .into_iter()
                    .enumerate()
            {
                if namespace.is_empty() {
                    return Err(rules_refusal(
                        path,
                        &format!("keys.authoritative[{index}]"),
                        "key namespace must be non-empty",
                        Some(Value::String(namespace)),
                        &["cik", "lei", "ticker", "cusip", "isin"],
                    ));
                }
                rules.insert(
                    namespace,
                    KeyRule {
                        authoritative: true,
                        single_valued: true,
                    },
                );
            }
        }
        Value::Object(object) => {
            if object.is_empty() {
                return Err(rules_refusal(
                    path,
                    "keys",
                    "mapping must not be empty",
                    Some(value.clone()),
                    &["cik", "lei", "ticker"],
                ));
            }
            if object.contains_key("authoritative") {
                reject_unknown_keys(object, &["authoritative"], path, "keys")?;
                for namespace in
                    parse_string_list(object, "authoritative", path, "keys.authoritative", &[])?
                {
                    rules.insert(
                        namespace,
                        KeyRule {
                            authoritative: true,
                            single_valued: true,
                        },
                    );
                }
                return Ok(rules);
            }
            for (namespace, value) in object {
                let namespace = namespace.trim().to_ascii_lowercase();
                if namespace.is_empty() {
                    return Err(rules_refusal(
                        path,
                        "keys",
                        "key namespace must be non-empty",
                        Some(Value::String(namespace)),
                        &["cik", "lei", "ticker", "cusip", "isin"],
                    ));
                }
                let options = value.as_object().ok_or_else(|| {
                    rules_refusal(
                        path,
                        &format!("keys.{namespace}"),
                        "value must be a YAML mapping",
                        Some(value.clone()),
                        &["authoritative", "single_valued"],
                    )
                })?;
                reject_unknown_keys(
                    options,
                    &["authoritative", "single_valued"],
                    path,
                    &format!("keys.{namespace}"),
                )?;
                let authoritative = optional_bool(
                    options,
                    "authoritative",
                    path,
                    &format!("keys.{namespace}.authoritative"),
                )?
                .unwrap_or(false);
                let single_valued = optional_bool(
                    options,
                    "single_valued",
                    path,
                    &format!("keys.{namespace}.single_valued"),
                )?
                .unwrap_or(false);
                rules.insert(
                    namespace,
                    KeyRule {
                        authoritative,
                        single_valued,
                    },
                );
            }
        }
        _ => {
            return Err(rules_refusal(
                path,
                "keys",
                "value must be a YAML mapping or the legacy authoritative sequence",
                Some(value.clone()),
                &["mapping", "sequence"],
            ));
        }
    }
    Ok(rules)
}

fn parse_alternatives(
    tier_b: &Map<String, Value>,
    path: &Path,
    eligible: &[String],
    required: &[String],
    default_evidence_types: usize,
    default_origins: usize,
) -> Result<Vec<TierBAlternative>, Refusal> {
    let Some(value) = tier_b.get("any_of") else {
        return Ok(vec![TierBAlternative {
            required_evidence_types: required.to_vec(),
            min_evidence_types: default_evidence_types,
            min_independent_origins: default_origins,
        }]);
    };
    let alternatives = value.as_array().ok_or_else(|| {
        rules_refusal(
            path,
            "tier_b.any_of",
            "value must be a YAML sequence",
            Some(value.clone()),
            &["sequence"],
        )
    })?;
    if alternatives.is_empty() {
        return Err(rules_refusal(
            path,
            "tier_b.any_of",
            "sequence must not be empty",
            Some(value.clone()),
            &["one or more alternatives"],
        ));
    }
    let mut parsed = Vec::with_capacity(alternatives.len());
    let eligible_refs = eligible.iter().map(String::as_str).collect::<Vec<_>>();
    for (index, alternative) in alternatives.iter().enumerate() {
        let item_path = format!("tier_b.any_of[{index}]");
        let object = alternative.as_object().ok_or_else(|| {
            rules_refusal(
                path,
                &item_path,
                "value must be a YAML mapping",
                Some(alternative.clone()),
                &["mapping"],
            )
        })?;
        reject_unknown_keys(
            object,
            &[
                "required_evidence_types",
                "min_evidence_types",
                "min_independent_origins",
                "min_distinct_evidence_types",
                "min_distinct_lineages",
            ],
            path,
            &item_path,
        )?;
        let required = optional_string_list(
            object,
            "required_evidence_types",
            path,
            &format!("{item_path}.required_evidence_types"),
            &eligible_refs,
        )?
        .unwrap_or_default();
        for (required_index, kind) in required.iter().enumerate() {
            if !eligible.contains(kind) {
                return Err(rules_refusal(
                    path,
                    &format!("{item_path}.required_evidence_types[{required_index}]"),
                    "required evidence type must be eligible",
                    Some(Value::String(kind.clone())),
                    &eligible.iter().map(String::as_str).collect::<Vec<_>>(),
                ));
            }
        }
        let min_evidence_types = optional_positive_usize(
            object,
            "min_evidence_types",
            path,
            &format!("{item_path}.min_evidence_types"),
        )?
        .or(optional_positive_usize(
            object,
            "min_distinct_evidence_types",
            path,
            &format!("{item_path}.min_distinct_evidence_types"),
        )?)
        .unwrap_or(default_evidence_types);
        let min_independent_origins = optional_positive_usize(
            object,
            "min_independent_origins",
            path,
            &format!("{item_path}.min_independent_origins"),
        )?
        .or(optional_positive_usize(
            object,
            "min_distinct_lineages",
            path,
            &format!("{item_path}.min_distinct_lineages"),
        )?)
        .unwrap_or(default_origins);
        parsed.push(TierBAlternative {
            required_evidence_types: required,
            min_evidence_types,
            min_independent_origins,
        });
    }
    Ok(parsed)
}

fn parse_standalone(root: &Map<String, Value>, path: &Path) -> Result<bool, Refusal> {
    let Some(value) = root.get("standalone") else {
        return Ok(false);
    };
    let object = value.as_object().ok_or_else(|| {
        rules_refusal(
            path,
            "standalone",
            "value must be a YAML mapping",
            Some(value.clone()),
            &["mapping"],
        )
    })?;
    reject_unknown_keys(object, &["mint_when_authoritative_key"], path, "standalone")?;
    Ok(optional_bool(
        object,
        "mint_when_authoritative_key",
        path,
        "standalone.mint_when_authoritative_key",
    )?
    .unwrap_or(false))
}

fn parse_guards(root: &Map<String, Value>, path: &Path) -> Result<usize, Refusal> {
    let Some(value) = root.get("guards") else {
        return Ok(DEFAULT_MAX_CLUSTER_SURFACES);
    };
    let object = value.as_object().ok_or_else(|| {
        rules_refusal(
            path,
            "guards",
            "value must be a YAML mapping",
            Some(value.clone()),
            &["mapping"],
        )
    })?;
    reject_unknown_keys(object, &["max_cluster_surfaces"], path, "guards")?;
    Ok(optional_positive_usize(
        object,
        "max_cluster_surfaces",
        path,
        "guards.max_cluster_surfaces",
    )?
    .unwrap_or(DEFAULT_MAX_CLUSTER_SURFACES))
}

fn parse_incumbent_attach(root: &Map<String, Value>, path: &Path) -> Result<Vec<String>, Refusal> {
    let Some(value) = root.get("incumbent_attach") else {
        return Ok(Vec::new());
    };
    let object = value.as_object().ok_or_else(|| {
        rules_refusal(
            path,
            "incumbent_attach",
            "value must be a YAML mapping",
            Some(value.clone()),
            &["mapping"],
        )
    })?;
    reject_unknown_keys(object, &["name_equivalent_steps"], path, "incumbent_attach")?;
    parse_string_list(
        object,
        "name_equivalent_steps",
        path,
        "incumbent_attach.name_equivalent_steps",
        INCUMBENT_ATTACH_STEPS,
    )
}

fn parse_commit(
    root: &Map<String, Value>,
    path: &Path,
    key_rules: &BTreeMap<String, KeyRule>,
) -> Result<Vec<String>, Refusal> {
    let Some(value) = root.get("commit") else {
        return Ok(Vec::new());
    };
    let object = value.as_object().ok_or_else(|| {
        rules_refusal(
            path,
            "commit",
            "value must be a YAML mapping",
            Some(value.clone()),
            &["mapping"],
        )
    })?;
    reject_unknown_keys(object, &["key_namespaces"], path, "commit")?;
    let declared = key_rules.keys().map(String::as_str).collect::<Vec<_>>();
    let mut namespaces = optional_string_list(
        object,
        "key_namespaces",
        path,
        "commit.key_namespaces",
        &declared,
    )?
    .unwrap_or_default();
    namespaces.sort();
    namespaces.dedup();
    Ok(namespaces)
}

fn reject_unknown_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    path: &Path,
    prefix: &str,
) -> Result<(), Refusal> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            let yaml_path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            return Err(rules_refusal(
                path,
                &yaml_path,
                "unknown key",
                Some(Value::String(key.clone())),
                allowed,
            ));
        }
    }
    Ok(())
}

fn required_mapping<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &Path,
    yaml_path: &str,
) -> Result<&'a Map<String, Value>, Refusal> {
    object.get(key).and_then(Value::as_object).ok_or_else(|| {
        rules_refusal(
            path,
            yaml_path,
            "value must be a YAML mapping",
            None,
            &["mapping"],
        )
    })
}

fn required_string(
    object: &Map<String, Value>,
    key: &str,
    path: &Path,
    yaml_path: &str,
) -> Result<String, Refusal> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            rules_refusal(
                path,
                yaml_path,
                "value must be a non-empty string",
                None,
                &["string"],
            )
        })
}

fn optional_bool(
    object: &Map<String, Value>,
    key: &str,
    path: &Path,
    yaml_path: &str,
) -> Result<Option<bool>, Refusal> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    value.as_bool().map(Some).ok_or_else(|| {
        rules_refusal(
            path,
            yaml_path,
            "value must be true or false",
            Some(value.clone()),
            &["true", "false"],
        )
    })
}

fn optional_positive_usize(
    object: &Map<String, Value>,
    key: &str,
    path: &Path,
    yaml_path: &str,
) -> Result<Option<usize>, Refusal> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    let parsed = value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            rules_refusal(
                path,
                yaml_path,
                "value must be a positive integer",
                Some(value.clone()),
                &["positive integer"],
            )
        })?;
    Ok(Some(parsed))
}

fn parse_string_list(
    object: &Map<String, Value>,
    key: &str,
    path: &Path,
    yaml_path: &str,
    allowed: &[&str],
) -> Result<Vec<String>, Refusal> {
    let value = object
        .get(key)
        .ok_or_else(|| rules_refusal(path, yaml_path, "value is required", None, &["sequence"]))?;
    parse_string_list_value(value, path, yaml_path, allowed, false)
        .map(|values| values.into_iter().collect())
}

fn optional_string_list(
    object: &Map<String, Value>,
    key: &str,
    path: &Path,
    yaml_path: &str,
    allowed: &[&str],
) -> Result<Option<Vec<String>>, Refusal> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    Ok(Some(parse_string_list_value(
        value, path, yaml_path, allowed, true,
    )?))
}

fn parse_string_list_value(
    value: &Value,
    path: &Path,
    yaml_path: &str,
    allowed: &[&str],
    allow_empty: bool,
) -> Result<Vec<String>, Refusal> {
    let values = value.as_array().ok_or_else(|| {
        rules_refusal(
            path,
            yaml_path,
            "value must be a YAML sequence",
            Some(value.clone()),
            &["sequence"],
        )
    })?;
    if values.is_empty() && !allow_empty {
        return Err(rules_refusal(
            path,
            yaml_path,
            "sequence must not be empty",
            Some(value.clone()),
            &["one or more values"],
        ));
    }
    let mut parsed = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let item_path = format!("{yaml_path}[{index}]");
        let item = value
            .as_str()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .ok_or_else(|| {
                rules_refusal(
                    path,
                    &item_path,
                    "value must be a non-empty string",
                    Some(value.clone()),
                    allowed,
                )
            })?;
        if !allowed.is_empty() && !allowed.contains(&item) {
            return Err(rules_refusal(
                path,
                &item_path,
                "unknown evidence or key value",
                Some(Value::String(item.to_string())),
                allowed,
            ));
        }
        parsed.insert(item.to_string());
    }
    Ok(parsed.into_iter().collect())
}

fn rules_refusal(
    path: &Path,
    yaml_path: &str,
    message: impl Into<String>,
    offending: Option<Value>,
    allowed: &[&str],
) -> Refusal {
    let mut detail = json!({
        "path": path.display().to_string(),
        "yaml_path": yaml_path,
        "writes_performed": false,
    });
    if let Some(offending) = offending {
        detail["offending_value"] = offending;
    }
    if !allowed.is_empty() {
        detail["allowed_values"] = json!(allowed);
    }
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: format!("Rules YAML path {yaml_path}: {}", message.into()),
        detail,
        next_command: Some(format!(
            "Fix {yaml_path} in {}, then rerun canon entity session rules validate",
            path.display()
        )),
    }
}
