//! The session registries map: which registry holds each kind of entity.
//!
//! A map file binds each entity kind (the value of an observation's
//! `entity_type`) to a registry directory, registry id, canonical type, file
//! names and an id scheme. Nothing here names a registry: every name comes from
//! the map.

use crate::registry::agent_id::kind_prefix_is_valid;
use serde_yaml::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
};

pub const SESSION_REGISTRIES_VERSION: &str = "canon.session.registries.v1";
const MAX_ZERO_PAD: u64 = 12;

/// A refusal that names the YAML path of the offending field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistriesMapError {
    pub path: String,
    pub message: String,
}

impl RegistriesMapError {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for RegistriesMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for RegistriesMapError {}

/// How new canonical ids are produced for a kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KindIdMode {
    /// A published registry keeps numbered ids from `prefix` and `zero_pad`.
    /// `agent_kind_prefix`, when set, gives every id one agent id as an alias.
    Numbered {
        prefix: String,
        zero_pad: Option<usize>,
        agent_kind_prefix: Option<String>,
    },
    /// A new registry: the agent id is the canonical id.
    Agent { kind_prefix: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindRegistry {
    pub kind: String,
    pub registry: PathBuf,
    pub registry_id: String,
    pub canonical_type: String,
    pub alias_file: String,
    pub key_file: Option<String>,
    pub id_alias_file: Option<String>,
    pub id_mode: KindIdMode,
}

impl KindRegistry {
    /// The agent-id prefix with its trailing underscore (`org_`), when this
    /// kind mints or aliases agent ids.
    pub fn agent_prefix(&self) -> Option<String> {
        match &self.id_mode {
            KindIdMode::Agent { kind_prefix }
            | KindIdMode::Numbered {
                agent_kind_prefix: Some(kind_prefix),
                ..
            } => Some(format!("{kind_prefix}_")),
            KindIdMode::Numbered { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistriesMap {
    /// Keyed by entity kind; iteration is kind-name order.
    pub kinds: BTreeMap<String, KindRegistry>,
}

impl RegistriesMap {
    pub fn get(&self, kind: &str) -> Option<&KindRegistry> {
        self.kinds.get(kind)
    }
}

/// Parse and validate a registries map from YAML text.
pub fn parse_registries_map(text: &str) -> Result<RegistriesMap, RegistriesMapError> {
    let root: Value = serde_yaml::from_str(text)
        .map_err(|error| RegistriesMapError::new("<document>", format!("invalid YAML: {error}")))?;
    let root = root
        .as_mapping()
        .ok_or_else(|| RegistriesMapError::new("<document>", "expected a mapping"))?;
    reject_unknown_keys(root, &["version", "kinds"], "")?;

    let version = string_field(root, "version", "version")?;
    if version != SESSION_REGISTRIES_VERSION {
        return Err(RegistriesMapError::new(
            "version",
            format!("expected '{SESSION_REGISTRIES_VERSION}', found '{version}'"),
        ));
    }
    let kinds_value = root.get("kinds").ok_or_else(|| {
        RegistriesMapError::new("kinds", "missing; add one entry per entity kind")
    })?;
    let kinds_map = kinds_value.as_mapping().ok_or_else(|| {
        RegistriesMapError::new("kinds", "expected a mapping of kind to registry")
    })?;
    if kinds_map.is_empty() {
        return Err(RegistriesMapError::new(
            "kinds",
            "must hold at least one entity kind",
        ));
    }

    let mut kinds = BTreeMap::new();
    for (key, value) in kinds_map {
        let kind = key
            .as_str()
            .map(str::trim)
            .filter(|kind| !kind.is_empty())
            .ok_or_else(|| {
                RegistriesMapError::new("kinds", "every kind name must be a non-empty string")
            })?
            .to_string();
        let path = format!("kinds.{kind}");
        let entry = parse_kind(&kind, value, &path)?;
        if kinds.insert(kind.clone(), entry).is_some() {
            return Err(RegistriesMapError::new(path, "kind is listed twice"));
        }
    }
    reject_shared_bindings(&kinds)?;
    Ok(RegistriesMap { kinds })
}

fn parse_kind(kind: &str, value: &Value, path: &str) -> Result<KindRegistry, RegistriesMapError> {
    let mapping = value
        .as_mapping()
        .ok_or_else(|| RegistriesMapError::new(path, "expected a mapping"))?;
    reject_unknown_keys(
        mapping,
        &[
            "registry",
            "registry_id",
            "prefix",
            "zero_pad",
            "agent_id",
            "id_scheme",
            "kind_prefix",
            "canonical_type",
            "alias_file",
            "key_file",
            "id_alias_file",
        ],
        path,
    )?;
    let registry = string_field(mapping, "registry", &format!("{path}.registry"))?;
    let registry_id = string_field(mapping, "registry_id", &format!("{path}.registry_id"))?;
    let canonical_type =
        string_field(mapping, "canonical_type", &format!("{path}.canonical_type"))?;
    let alias_file = file_name_field(mapping, "alias_file", path)?
        .ok_or_else(|| RegistriesMapError::new(format!("{path}.alias_file"), "missing"))?;
    let key_file = file_name_field(mapping, "key_file", path)?;
    let id_alias_file = file_name_field(mapping, "id_alias_file", path)?;

    let id_scheme = optional_string_field(mapping, "id_scheme", &format!("{path}.id_scheme"))?;
    let id_mode = match id_scheme.as_deref() {
        Some("agent") => {
            for forbidden in ["prefix", "zero_pad", "agent_id"] {
                if mapping.contains_key(forbidden) {
                    return Err(RegistriesMapError::new(
                        format!("{path}.{forbidden}"),
                        "not allowed with id_scheme: agent (the agent id is the canonical id)",
                    ));
                }
            }
            let kind_prefix =
                kind_prefix_field(mapping, "kind_prefix", &format!("{path}.kind_prefix"))?
                    .ok_or_else(|| {
                        RegistriesMapError::new(
                            format!("{path}.kind_prefix"),
                            "missing; id_scheme: agent needs the kind prefix, for example 'per'",
                        )
                    })?;
            KindIdMode::Agent { kind_prefix }
        }
        Some(other) => {
            return Err(RegistriesMapError::new(
                format!("{path}.id_scheme"),
                format!("unknown scheme '{other}'; the only scheme is 'agent'"),
            ));
        }
        None => {
            if mapping.contains_key("kind_prefix") {
                return Err(RegistriesMapError::new(
                    format!("{path}.kind_prefix"),
                    "only valid with id_scheme: agent; a numbered registry sets agent_id.kind_prefix",
                ));
            }
            let prefix =
                string_field(mapping, "prefix", &format!("{path}.prefix")).map_err(|_| {
                    RegistriesMapError::new(
                        format!("{path}.prefix"),
                        "missing; a numbered registry needs prefix (or set id_scheme: agent)",
                    )
                })?;
            let zero_pad = optional_zero_pad(mapping, &format!("{path}.zero_pad"))?;
            let agent_kind_prefix = match mapping.get("agent_id") {
                None => None,
                Some(agent) => {
                    let agent_path = format!("{path}.agent_id");
                    let agent_map = agent.as_mapping().ok_or_else(|| {
                        RegistriesMapError::new(&agent_path, "expected a mapping with kind_prefix")
                    })?;
                    reject_unknown_keys(agent_map, &["kind_prefix"], &agent_path)?;
                    Some(
                        kind_prefix_field(
                            agent_map,
                            "kind_prefix",
                            &format!("{agent_path}.kind_prefix"),
                        )?
                        .ok_or_else(|| {
                            RegistriesMapError::new(format!("{agent_path}.kind_prefix"), "missing")
                        })?,
                    )
                }
            };
            if id_alias_file.is_some() && agent_kind_prefix.is_none() {
                // id_alias_file only carries agent-id aliases: refuse the dead setting.
                return Err(RegistriesMapError::new(
                    format!("{path}.id_alias_file"),
                    "has no effect without agent_id.kind_prefix",
                ));
            }
            KindIdMode::Numbered {
                prefix,
                zero_pad,
                agent_kind_prefix,
            }
        }
    };
    Ok(KindRegistry {
        kind: kind.to_string(),
        registry: PathBuf::from(registry),
        registry_id,
        canonical_type,
        alias_file,
        key_file,
        id_alias_file,
        id_mode,
    })
}

fn reject_shared_bindings(
    kinds: &BTreeMap<String, KindRegistry>,
) -> Result<(), RegistriesMapError> {
    let mut directories = BTreeMap::new();
    let mut ids = BTreeMap::new();
    let mut prefixes: BTreeMap<String, String> = BTreeMap::new();
    for (kind, entry) in kinds {
        if let Some(other) = directories.insert(entry.registry.clone(), kind.clone()) {
            return Err(RegistriesMapError::new(
                format!("kinds.{kind}.registry"),
                format!("same directory as kind '{other}'; one registry per kind"),
            ));
        }
        if let Some(other) = ids.insert(entry.registry_id.clone(), kind.clone()) {
            return Err(RegistriesMapError::new(
                format!("kinds.{kind}.registry_id"),
                format!("same registry_id as kind '{other}'; one registry per kind"),
            ));
        }
        if let Some(prefix) = entry.agent_prefix()
            && let Some(other) = prefixes.insert(prefix.clone(), kind.clone())
        {
            return Err(RegistriesMapError::new(
                format!("kinds.{kind}"),
                format!("agent id prefix '{prefix}' is already used by kind '{other}'"),
            ));
        }
    }
    let mut seen = BTreeSet::new();
    for (kind, entry) in kinds {
        if let KindIdMode::Numbered { prefix, .. } = &entry.id_mode
            && !seen.insert((entry.registry_id.clone(), prefix.clone()))
        {
            return Err(RegistriesMapError::new(
                format!("kinds.{kind}.prefix"),
                "duplicate registry and prefix",
            ));
        }
    }
    Ok(())
}

fn reject_unknown_keys(
    mapping: &serde_yaml::Mapping,
    allowed: &[&str],
    path: &str,
) -> Result<(), RegistriesMapError> {
    for key in mapping.keys() {
        let name = key.as_str().unwrap_or("<non-string key>");
        if !allowed.contains(&name) {
            let field = if path.is_empty() {
                name.to_string()
            } else {
                format!("{path}.{name}")
            };
            return Err(RegistriesMapError::new(
                field,
                format!("unknown field; allowed: {}", allowed.join(", ")),
            ));
        }
    }
    Ok(())
}

fn string_field(
    mapping: &serde_yaml::Mapping,
    field: &str,
    path: &str,
) -> Result<String, RegistriesMapError> {
    optional_string_field(mapping, field, path)?
        .ok_or_else(|| RegistriesMapError::new(path, "missing"))
}

fn optional_string_field(
    mapping: &serde_yaml::Mapping,
    field: &str,
    path: &str,
) -> Result<Option<String>, RegistriesMapError> {
    match mapping.get(field) {
        None => Ok(None),
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Err(RegistriesMapError::new(path, "must not be empty"))
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Some(_) => Err(RegistriesMapError::new(path, "expected a string")),
    }
}

fn file_name_field(
    mapping: &serde_yaml::Mapping,
    field: &str,
    path: &str,
) -> Result<Option<String>, RegistriesMapError> {
    let field_path = format!("{path}.{field}");
    let Some(name) = optional_string_field(mapping, field, &field_path)? else {
        return Ok(None);
    };
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(RegistriesMapError::new(
            field_path,
            "must be a plain file name inside the registry directory",
        ));
    }
    if !name.ends_with(".json") {
        return Err(RegistriesMapError::new(
            field_path,
            "must be a .json mapping file name",
        ));
    }
    Ok(Some(name))
}

fn kind_prefix_field(
    mapping: &serde_yaml::Mapping,
    field: &str,
    path: &str,
) -> Result<Option<String>, RegistriesMapError> {
    let Some(prefix) = optional_string_field(mapping, field, path)? else {
        return Ok(None);
    };
    if !kind_prefix_is_valid(&format!("{prefix}_")) {
        return Err(RegistriesMapError::new(
            path,
            format!("'{prefix}' must be lower-case letters only; canon adds the underscore"),
        ));
    }
    Ok(Some(prefix))
}

fn optional_zero_pad(
    mapping: &serde_yaml::Mapping,
    path: &str,
) -> Result<Option<usize>, RegistriesMapError> {
    match mapping.get("zero_pad") {
        None => Ok(None),
        Some(value) => match value.as_u64() {
            Some(width) if (1..=MAX_ZERO_PAD).contains(&width) => Ok(Some(width as usize)),
            _ => Err(RegistriesMapError::new(
                path,
                format!("expected an integer from 1 to {MAX_ZERO_PAD}"),
            )),
        },
    }
}

/// Parse a per-kind flag value: `kind=value,kind=value`. A value with no `=`
/// is not per-kind and yields `Ok(None)`.
pub fn parse_per_kind_flag(
    flag: &str,
    raw: &str,
) -> Result<Option<BTreeMap<String, String>>, RegistriesMapError> {
    if !raw.contains('=') {
        return Ok(None);
    }
    let mut parsed = BTreeMap::new();
    for part in raw.split(',') {
        let part = part.trim();
        let Some((kind, value)) = part.split_once('=') else {
            return Err(RegistriesMapError::new(
                flag,
                format!("'{part}' is not kind=value; use {flag} organisation=1.0.0,person=0.1.0"),
            ));
        };
        let (kind, value) = (kind.trim(), value.trim());
        if kind.is_empty() || value.is_empty() {
            return Err(RegistriesMapError::new(
                flag,
                format!("'{part}' needs both a kind and a value"),
            ));
        }
        if parsed.insert(kind.to_string(), value.to_string()).is_some() {
            return Err(RegistriesMapError::new(
                flag,
                format!("kind '{kind}' is given twice"),
            ));
        }
    }
    Ok(Some(parsed))
}

/// Resolve the value of a per-kind flag for one kind: the per-kind form picks
/// that kind's entry, a plain value applies to every kind.
pub fn flag_value_for_kind(
    flag: &str,
    raw: &str,
    kind: &str,
    known_kinds: &[&str],
) -> Result<Option<String>, RegistriesMapError> {
    match parse_per_kind_flag(flag, raw)? {
        None => Ok(Some(raw.trim().to_string())),
        Some(map) => {
            for named in map.keys() {
                if !known_kinds.contains(&named.as_str()) {
                    return Err(RegistriesMapError::new(
                        flag,
                        format!(
                            "kind '{named}' is not in the session; kinds: {}",
                            known_kinds.join(", ")
                        ),
                    ));
                }
            }
            Ok(map.get(kind).cloned())
        }
    }
}

/// The next numbered id after the highest existing id of the form
/// `<prefix>-<digits>` (the `-` is the registry numbering separator). `zero_pad` of `None` falls back to the width of the
/// highest existing id, then to 3.
pub fn next_numbered_id<'a>(
    existing_ids: impl IntoIterator<Item = &'a str>,
    prefix: &str,
    zero_pad: Option<usize>,
) -> String {
    let namespace = format!("{prefix}-");
    let mut highest: Option<(u64, usize)> = None;
    for id in existing_ids {
        let Some(digits) = id.strip_prefix(namespace.as_str()) else {
            continue;
        };
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Ok(number) = digits.parse::<u64>() else {
            continue;
        };
        if highest.is_none_or(|(best, _)| number > best) {
            highest = Some((number, digits.len()));
        }
    }
    let next = highest.map_or(1, |(number, _)| number + 1);
    let width = zero_pad.or(highest.map(|(_, width)| width)).unwrap_or(3);
    format!("{namespace}{next:0width$}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
version: canon.session.registries.v1
kinds:
  organisation:
    registry: /stage/organisations
    registry_id: orgs
    prefix: ORG
    zero_pad: 3
    agent_id: {kind_prefix: org}
    canonical_type: firm
    alias_file: 30-session-aliases.json
    key_file: 20-firm-keys.json
    id_alias_file: 01-firm-canon-ids.json
  person:
    registry: /stage/people
    registry_id: people
    id_scheme: agent
    kind_prefix: per
    canonical_type: person
    alias_file: aliases.json
"#;

    fn refusal_path(text: &str) -> String {
        parse_registries_map(text).unwrap_err().path
    }

    #[test]
    fn parses_numbered_and_agent_kinds() {
        let map = parse_registries_map(GOOD).unwrap();
        let names: Vec<_> = map.kinds.keys().map(String::as_str).collect();
        assert_eq!(names, ["organisation", "person"]);
        let org = map.get("organisation").unwrap();
        assert_eq!(org.registry, PathBuf::from("/stage/organisations"));
        assert_eq!(org.canonical_type, "firm");
        assert_eq!(org.key_file.as_deref(), Some("20-firm-keys.json"));
        assert_eq!(org.agent_prefix().as_deref(), Some("org_"));
        assert_eq!(
            org.id_mode,
            KindIdMode::Numbered {
                prefix: "ORG".to_string(),
                zero_pad: Some(3),
                agent_kind_prefix: Some("org".to_string()),
            }
        );
        let person = map.get("person").unwrap();
        assert_eq!(
            person.id_mode,
            KindIdMode::Agent {
                kind_prefix: "per".to_string()
            }
        );
        assert_eq!(person.agent_prefix().as_deref(), Some("per_"));
    }

    #[test]
    fn refusals_name_the_yaml_path() {
        let cases = [
            (GOOD.replace("canon.session.registries.v1", "v0"), "version"),
            (
                GOOD.replace("registry_id: orgs", "registry_id: ''"),
                "kinds.organisation.registry_id",
            ),
            (
                GOOD.replace("    canonical_type: firm\n", ""),
                "kinds.organisation.canonical_type",
            ),
            (
                GOOD.replace("alias_file: aliases.json", "alias_file: ../aliases.json"),
                "kinds.person.alias_file",
            ),
            (
                GOOD.replace("alias_file: aliases.json", "alias_file: aliases.txt"),
                "kinds.person.alias_file",
            ),
            (
                GOOD.replace("zero_pad: 3", "zero_pad: 0"),
                "kinds.organisation.zero_pad",
            ),
            (
                GOOD.replace("kind_prefix: org}", "kind_prefix: Org_}"),
                "kinds.organisation.agent_id.kind_prefix",
            ),
            (
                GOOD.replace("kind_prefix: per", "kind_prefix: p3r"),
                "kinds.person.kind_prefix",
            ),
            (
                GOOD.replace("id_scheme: agent", "id_scheme: numbered"),
                "kinds.person.id_scheme",
            ),
            (
                GOOD.replace("    kind_prefix: per\n", ""),
                "kinds.person.kind_prefix",
            ),
            (
                GOOD.replace(
                    "canonical_type: person",
                    "canonical_type: person\n    extra: 1",
                ),
                "kinds.person.extra",
            ),
            (format!("{GOOD}other: 1\n"), "other"),
        ];
        for (text, expected) in cases {
            assert_eq!(refusal_path(&text), expected, "{text}");
        }
    }

    #[test]
    fn agent_scheme_refuses_numbered_settings() {
        let text = GOOD.replace("id_scheme: agent", "id_scheme: agent\n    prefix: PER");
        assert_eq!(refusal_path(&text), "kinds.person.prefix");
    }

    #[test]
    fn numbered_kind_without_prefix_is_refused() {
        let text = GOOD.replace("    prefix: ORG\n", "");
        assert_eq!(refusal_path(&text), "kinds.organisation.prefix");
    }

    #[test]
    fn id_alias_file_without_agent_ids_is_refused() {
        let text = GOOD.replace("    agent_id: {kind_prefix: org}\n", "");
        assert_eq!(refusal_path(&text), "kinds.organisation.id_alias_file");
    }

    #[test]
    fn two_kinds_cannot_share_a_registry_or_prefix() {
        let same_dir = GOOD.replace("/stage/people", "/stage/organisations");
        assert_eq!(refusal_path(&same_dir), "kinds.person.registry");
        let same_id = GOOD.replace("registry_id: people", "registry_id: orgs");
        assert_eq!(refusal_path(&same_id), "kinds.person.registry_id");
        let same_prefix = GOOD.replace("kind_prefix: per", "kind_prefix: org");
        assert_eq!(refusal_path(&same_prefix), "kinds.person");
    }

    #[test]
    fn empty_or_malformed_documents_are_refused() {
        assert_eq!(refusal_path("- a\n- b\n"), "<document>");
        assert_eq!(refusal_path("version: [\n"), "<document>");
        assert_eq!(
            refusal_path("version: canon.session.registries.v1\nkinds: {}\n"),
            "kinds"
        );
        assert_eq!(
            refusal_path("version: canon.session.registries.v1\n"),
            "kinds"
        );
    }

    #[test]
    fn per_kind_flags_parse_and_refuse() {
        let map = parse_per_kind_flag("--next-version", "organisation=2026-11-16, person=0.1.0")
            .unwrap()
            .unwrap();
        assert_eq!(map["organisation"], "2026-11-16");
        assert_eq!(map["person"], "0.1.0");
        assert_eq!(
            parse_per_kind_flag("--next-version", "1.2.3").unwrap(),
            None
        );
        for bad in [
            "organisation=1.0.0,person",
            "=1.0.0,a=b",
            "a=,b=c",
            "a=1,a=2",
        ] {
            assert!(parse_per_kind_flag("--next-version", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn flag_value_for_kind_checks_known_kinds() {
        let kinds = ["organisation", "person"];
        assert_eq!(
            flag_value_for_kind("--bump", "patch", "person", &kinds).unwrap(),
            Some("patch".to_string())
        );
        assert_eq!(
            flag_value_for_kind("--next-version", "organisation=1.0.0", "person", &kinds).unwrap(),
            None
        );
        assert_eq!(
            flag_value_for_kind(
                "--next-version",
                "organisation=1.0.0",
                "organisation",
                &kinds
            )
            .unwrap(),
            Some("1.0.0".to_string())
        );
        let error =
            flag_value_for_kind("--next-version", "fund=1.0.0", "person", &kinds).unwrap_err();
        assert!(error.message.contains("fund"));
    }

    #[test]
    fn numbered_ids_continue_after_the_highest_existing_id() {
        let existing = ["ORG-001", "ORG-012", "ORG-007", "OTHER-099", "ORG-x", "ORG"];
        assert_eq!(next_numbered_id(existing, "ORG", Some(3)), "ORG-013");
        assert_eq!(next_numbered_id(existing, "ORG", Some(5)), "ORG-00013");
        assert_eq!(next_numbered_id(existing, "ORG", None), "ORG-013");
        assert_eq!(next_numbered_id([], "ORG", None), "ORG-001");
        assert_eq!(next_numbered_id(["ORG-999"], "ORG", Some(3)), "ORG-1000");
        assert_eq!(next_numbered_id(["ORG-0042"], "ORG", None), "ORG-0043");
    }
}
