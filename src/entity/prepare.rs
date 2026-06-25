//! Prepare-stage input contract and profile field projection.
//!
//! This module owns only the row-to-observation contract for `canon entity
//! prepare`: validate profile-required fields, decode side-surface JSON fields,
//! and keep `source_row_id` as provenance instead of identity evidence.

use crate::entity::{
    CANON_ENTITY_PREPARE_VERSION, EntityProfileDocument, EntityProfileReference,
    error::EntityRefusalKind,
};
use crate::{Refusal, witness};
use csv::{ReaderBuilder, StringRecord};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

const BUILTIN_CMBS_TENANT_LABEL_PROFILE: &str =
    include_str!("../../tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");
const BUILTIN_REGAB_FIRM_IDENTITY_PROFILE: &str =
    include_str!("../../tests/fixtures/entity/profiles/regab_firm_identity.yaml");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PrepareFieldMapping {
    #[serde(default)]
    pub primary_surface_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias_surfaces_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mention_surfaces_field: Option<String>,
    #[serde(default)]
    pub anchor_fields: BTreeMap<String, String>,
    #[serde(default)]
    pub context_fields: Vec<String>,
    #[serde(default)]
    pub provenance_fields: Vec<String>,
}

impl PrepareFieldMapping {
    pub fn cmbs_tenant_label() -> Self {
        Self {
            primary_surface_fields: vec!["raw_tenant_name".to_string()],
            alias_surfaces_field: Some("alias_surfaces_json".to_string()),
            mention_surfaces_field: Some("mention_surfaces_json".to_string()),
            anchor_fields: BTreeMap::new(),
            context_fields: vec![
                "deal_id".to_string(),
                "loan_id".to_string(),
                "property_id".to_string(),
            ],
            provenance_fields: vec![
                "source_row_id".to_string(),
                "deal_id".to_string(),
                "loan_id".to_string(),
                "property_id".to_string(),
            ],
        }
    }

    pub fn regab_firm_identity() -> Self {
        Self {
            primary_surface_fields: vec!["org_name".to_string()],
            alias_surfaces_field: Some("alias_surfaces_json".to_string()),
            mention_surfaces_field: Some("mention_surfaces_json".to_string()),
            anchor_fields: BTreeMap::from([
                ("cik".to_string(), "filing_cik".to_string()),
                ("accession".to_string(), "accession".to_string()),
            ]),
            context_fields: vec![
                "dataset".to_string(),
                "field_name".to_string(),
                "role_context".to_string(),
                "capacity".to_string(),
                "subject_role".to_string(),
            ],
            provenance_fields: vec![
                "source_row_id".to_string(),
                "dataset".to_string(),
                "doc_id".to_string(),
                "accession".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrepareInputContract {
    pub profile: EntityProfileReference,
    pub required_fields: Vec<String>,
    pub mapping: PrepareFieldMapping,
}

impl PrepareInputContract {
    pub fn new(
        profile: &EntityProfileDocument,
        mapping: PrepareFieldMapping,
    ) -> Result<Self, Refusal> {
        profile.validate().map_err(|error| error.to_refusal())?;
        validate_mapping(&mapping)?;

        Ok(Self {
            profile: profile.to_reference(),
            required_fields: profile.required_fields.clone(),
            mapping,
        })
    }

    pub fn for_builtin_profile(profile: &EntityProfileDocument) -> Result<Self, Refusal> {
        let mapping = match profile.profile.as_str() {
            "cmbs_tenant_label" => PrepareFieldMapping::cmbs_tenant_label(),
            "regab_firm_identity" => PrepareFieldMapping::regab_firm_identity(),
            _ => {
                return Err(EntityRefusalKind::Profile.to_refusal(
                    "Entity profile has no prepare field mapping",
                    json!({ "profile": profile.profile }),
                    None,
                ));
            }
        };
        Self::new(profile, mapping)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedInputObservation {
    pub profile_id: String,
    pub row_number: usize,
    pub primary_surface: PreparedSurface,
    #[serde(default)]
    pub alias_surfaces: Vec<PreparedSurface>,
    #[serde(default)]
    pub mention_surfaces: Vec<PreparedSurface>,
    #[serde(default)]
    pub anchors: Vec<PreparedAnchor>,
    #[serde(default)]
    pub context: BTreeMap<String, Value>,
    #[serde(default)]
    pub provenance: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedSurface {
    pub value: String,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedAnchor {
    pub namespace: String,
    pub value: String,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareRunRequest<'a> {
    pub rows: &'a Path,
    pub profile: &'a str,
    pub registry: &'a Path,
    pub work_dir: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrepareRunArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub profile: EntityProfileReference,
    pub registry_snapshot: PrepareRegistrySnapshot,
    pub input: PrepareInputReference,
    pub summary: BTreeMap<String, u64>,
    pub surfaces_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrepareRegistrySnapshot {
    pub id: String,
    pub version: String,
    pub source: String,
    pub lookup_snapshot_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrepareInputReference {
    pub row_count: u64,
    pub content_hash: String,
}

#[derive(Debug, Deserialize)]
struct RegistryJsonLite {
    id: String,
    version: String,
}

#[derive(Debug, Clone)]
struct PrepareRawRow {
    row_number: usize,
    values: BTreeMap<String, Value>,
}

impl PrepareRawRow {
    fn get(&self, field: &str) -> Option<&Value> {
        self.values.get(field)
    }

    fn required_scalar(&self, field: &str) -> Result<String, Refusal> {
        let Some(value) = self.get(field) else {
            return Err(input_contract_refusal(
                format!("Input row is missing required profile field '{field}'"),
                self.row_number,
                field,
                None,
                None,
            ));
        };

        scalar_string(value, self.row_number, field)?.ok_or_else(|| {
            input_contract_refusal(
                format!("Required profile field '{field}' must be non-empty"),
                self.row_number,
                field,
                Some(value),
                None,
            )
        })
    }
}

pub fn project_prepare_csv_reader<R: Read>(
    reader: R,
    delimiter: u8,
    contract: &PrepareInputContract,
) -> Result<Vec<PreparedInputObservation>, Refusal> {
    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(reader);
    let headers = reader
        .headers()
        .map_err(|error| {
            EntityRefusalKind::InputContract.to_refusal(
                "Failed to read prepare CSV headers",
                json!({ "error": error.to_string() }),
                None,
            )
        })?
        .iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    validate_headers(&headers, contract)?;

    let mut observations = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(|error| {
            EntityRefusalKind::InputContract.to_refusal(
                "Failed to parse prepare CSV row",
                json!({ "row_number": index + 1, "error": error.to_string() }),
                None,
            )
        })?;
        if blank_record(&record) {
            continue;
        }
        let row = csv_row_to_raw(index + 1, &headers, &record);
        observations.push(project_prepare_row(&row, contract)?);
    }

    Ok(observations)
}

pub fn project_prepare_jsonl_reader<R: BufRead>(
    mut reader: R,
    contract: &PrepareInputContract,
) -> Result<Vec<PreparedInputObservation>, Refusal> {
    let mut observations = Vec::new();
    let mut line = String::new();
    let mut row_number = 0usize;

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).map_err(|error| {
            EntityRefusalKind::InputContract.to_refusal(
                "Failed to read prepare JSONL input",
                json!({ "error": error.to_string() }),
                None,
            )
        })?;
        if bytes_read == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }

        row_number += 1;
        let value = serde_json::from_str::<Value>(&line).map_err(|error| {
            EntityRefusalKind::InputContract.to_refusal(
                "Invalid prepare JSONL row",
                json!({ "row_number": row_number, "error": error.to_string() }),
                None,
            )
        })?;
        let object = value.as_object().ok_or_else(|| {
            EntityRefusalKind::InputContract.to_refusal(
                "Prepare JSONL rows must be JSON objects",
                json!({ "row_number": row_number }),
                None,
            )
        })?;
        let row = json_object_to_raw(row_number, object);
        observations.push(project_prepare_row(&row, contract)?);
    }

    Ok(observations)
}

pub fn load_prepare_profile(profile: &str) -> Result<EntityProfileDocument, Refusal> {
    let profile_source = if Path::new(profile).exists() {
        fs::read_to_string(profile).map_err(|error| {
            EntityRefusalKind::Profile.to_refusal(
                "Failed to read entity profile",
                json!({ "profile": profile, "error": error.to_string() }),
                None,
            )
        })?
    } else {
        match profile {
            "cmbs_tenant_label" => BUILTIN_CMBS_TENANT_LABEL_PROFILE.to_string(),
            "regab_firm_identity" => BUILTIN_REGAB_FIRM_IDENTITY_PROFILE.to_string(),
            _ => {
                return Err(EntityRefusalKind::Profile.to_refusal(
                    "Unknown entity prepare profile",
                    json!({
                        "profile": profile,
                        "available_profiles": ["cmbs_tenant_label", "regab_firm_identity"]
                    }),
                    None,
                ));
            }
        }
    };

    EntityProfileDocument::from_yaml_str(&profile_source).map_err(|error| error.to_refusal())
}

pub fn project_prepare_path(
    rows: &Path,
    contract: &PrepareInputContract,
) -> Result<Vec<PreparedInputObservation>, Refusal> {
    let file = File::open(rows).map_err(|error| {
        EntityRefusalKind::InputContract.to_refusal(
            "Failed to read prepare input rows",
            json!({ "path": rows.display().to_string(), "error": error.to_string() }),
            None,
        )
    })?;

    match prepare_input_format(rows)? {
        PrepareInputFormat::Csv(delimiter) => project_prepare_csv_reader(file, delimiter, contract),
        PrepareInputFormat::Jsonl => project_prepare_jsonl_reader(BufReader::new(file), contract),
    }
}

pub fn run_prepare(request: PrepareRunRequest<'_>) -> Result<PrepareRunArtifact, Refusal> {
    let profile = load_prepare_profile(request.profile)?;
    let contract = PrepareInputContract::for_builtin_profile(&profile)?;
    let observations = project_prepare_path(request.rows, &contract)?;

    let registry_snapshot = load_prepare_registry_snapshot(request.registry)?;
    let input_hash = witness::hash_file(request.rows).map_err(|error| {
        io_budget_refusal(
            "Failed to hash prepare input rows",
            request.rows,
            error.to_string(),
        )
    })?;
    let input = PrepareInputReference {
        row_count: u64::try_from(observations.len()).expect("observation count fits u64"),
        content_hash: input_hash,
    };

    let prepare_dir = request.work_dir.join("prepare");
    fs::create_dir_all(&prepare_dir).map_err(|error| {
        io_budget_refusal(
            "Failed to create entity prepare work directory",
            &prepare_dir,
            error.to_string(),
        )
    })?;

    let surfaces_relative = PathBuf::from("prepare").join("surfaces.jsonl");
    let surfaces_path = request.work_dir.join(&surfaces_relative);
    write_observations_jsonl(&surfaces_path, &observations)?;

    let mut artifact = PrepareRunArtifact {
        version: CANON_ENTITY_PREPARE_VERSION.to_string(),
        artifact_content_hash: String::new(),
        profile: contract.profile.clone(),
        registry_snapshot,
        input,
        summary: prepare_summary(&observations),
        surfaces_path: surfaces_relative.to_string_lossy().into_owned(),
    };
    artifact.artifact_content_hash = hash_artifact_without_self(&artifact)?;

    let artifact_path = prepare_dir.join("prepare.json");
    write_json_file(&artifact_path, &artifact)?;
    Ok(artifact)
}

fn project_prepare_row(
    row: &PrepareRawRow,
    contract: &PrepareInputContract,
) -> Result<PreparedInputObservation, Refusal> {
    for field in &contract.required_fields {
        row.required_scalar(field)?;
    }

    let primary_surface = primary_surface(row, contract)?;
    let alias_surfaces = side_surfaces(row, contract.mapping.alias_surfaces_field.as_deref())?;
    let mention_surfaces = side_surfaces(row, contract.mapping.mention_surfaces_field.as_deref())?;
    let anchors = anchors(row, contract)?;
    let context = context(row, contract)?;
    let provenance = provenance(row, contract)?;

    Ok(PreparedInputObservation {
        profile_id: contract.profile.id.clone(),
        row_number: row.row_number,
        primary_surface,
        alias_surfaces,
        mention_surfaces,
        anchors,
        context,
        provenance,
    })
}

fn validate_mapping(mapping: &PrepareFieldMapping) -> Result<(), Refusal> {
    if mapping.primary_surface_fields.is_empty() {
        return Err(EntityRefusalKind::Profile.to_refusal(
            "Prepare field mapping must declare at least one primary surface field",
            json!({ "field": "primary_surface_fields" }),
            None,
        ));
    }
    ensure_unique_non_empty("primary_surface_fields", &mapping.primary_surface_fields)?;
    ensure_unique_non_empty("context_fields", &mapping.context_fields)?;
    ensure_unique_non_empty("provenance_fields", &mapping.provenance_fields)?;

    let mut anchor_namespaces = BTreeSet::new();
    let mut anchor_fields = BTreeSet::new();
    for (namespace, field) in &mapping.anchor_fields {
        if namespace.trim().is_empty() || field.trim().is_empty() {
            return Err(EntityRefusalKind::Profile.to_refusal(
                "Prepare anchor mappings must use non-empty namespace and field names",
                json!({ "namespace": namespace, "field": field }),
                None,
            ));
        }
        if !anchor_namespaces.insert(namespace) || !anchor_fields.insert(field) {
            return Err(EntityRefusalKind::Profile.to_refusal(
                "Prepare anchor mappings must not repeat namespaces or fields",
                json!({ "namespace": namespace, "field": field }),
                None,
            ));
        }
    }
    Ok(())
}

fn ensure_unique_non_empty(field: &str, values: &[String]) -> Result<(), Refusal> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() {
            return Err(EntityRefusalKind::Profile.to_refusal(
                "Prepare field mapping contains an empty field name",
                json!({ "field": field }),
                None,
            ));
        }
        if !seen.insert(value.as_str()) {
            return Err(EntityRefusalKind::Profile.to_refusal(
                "Prepare field mapping contains duplicate field names",
                json!({ "field": field, "value": value }),
                None,
            ));
        }
    }
    Ok(())
}

fn validate_headers(headers: &[String], contract: &PrepareInputContract) -> Result<(), Refusal> {
    let header_set = headers.iter().map(String::as_str).collect::<BTreeSet<_>>();
    for field in &contract.required_fields {
        if !header_set.contains(field.as_str()) {
            return Err(EntityRefusalKind::InputContract.to_refusal(
                "Prepare CSV input is missing a required profile field",
                json!({ "field": field, "available_fields": headers }),
                None,
            ));
        }
    }
    if !contract
        .mapping
        .primary_surface_fields
        .iter()
        .any(|field| header_set.contains(field.as_str()))
    {
        return Err(EntityRefusalKind::InputContract.to_refusal(
            "Prepare CSV input does not include any primary surface field",
            json!({
                "primary_surface_fields": contract.mapping.primary_surface_fields,
                "available_fields": headers
            }),
            None,
        ));
    }
    Ok(())
}

fn csv_row_to_raw(row_number: usize, headers: &[String], record: &StringRecord) -> PrepareRawRow {
    let values = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            (
                header.clone(),
                Value::String(record.get(index).unwrap_or_default().to_string()),
            )
        })
        .collect();
    PrepareRawRow { row_number, values }
}

fn json_object_to_raw(row_number: usize, object: &Map<String, Value>) -> PrepareRawRow {
    PrepareRawRow {
        row_number,
        values: object
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    }
}

fn primary_surface(
    row: &PrepareRawRow,
    contract: &PrepareInputContract,
) -> Result<PreparedSurface, Refusal> {
    for field in &contract.mapping.primary_surface_fields {
        let Some(value) = row.get(field) else {
            continue;
        };
        if let Some(surface) = scalar_string(value, row.row_number, field)? {
            return Ok(PreparedSurface {
                value: surface,
                field: field.clone(),
            });
        }
    }

    Err(EntityRefusalKind::InputContract.to_refusal(
        "No primary surface field produced a non-empty value",
        json!({
            "row_number": row.row_number,
            "primary_surface_fields": contract.mapping.primary_surface_fields
        }),
        None,
    ))
}

fn side_surfaces(
    row: &PrepareRawRow,
    field_name: Option<&str>,
) -> Result<Vec<PreparedSurface>, Refusal> {
    let Some(field_name) = field_name else {
        return Ok(Vec::new());
    };
    let Some(value) = row.get(field_name) else {
        return Ok(Vec::new());
    };
    let surfaces = decode_side_surfaces(value, row.row_number, field_name)?;
    Ok(surfaces
        .into_iter()
        .map(|value| PreparedSurface {
            value,
            field: field_name.to_string(),
        })
        .collect())
}

fn decode_side_surfaces(
    value: &Value,
    row_number: usize,
    field_name: &str,
) -> Result<Vec<String>, Refusal> {
    let items = match value {
        Value::Null => return Ok(Vec::new()),
        Value::Array(items) => items.clone(),
        Value::String(text) if text.trim().is_empty() => return Ok(Vec::new()),
        Value::String(text) => serde_json::from_str::<Value>(text.trim())
            .map_err(|error| {
                input_contract_refusal(
                    format!("Side field '{field_name}' must be valid JSON"),
                    row_number,
                    field_name,
                    Some(value),
                    Some(error.to_string()),
                )
            })?
            .as_array()
            .cloned()
            .ok_or_else(|| {
                input_contract_refusal(
                    format!("Side field '{field_name}' must decode to a JSON array"),
                    row_number,
                    field_name,
                    Some(value),
                    None,
                )
            })?,
        _ => {
            return Err(input_contract_refusal(
                format!("Side field '{field_name}' must be a JSON array of strings"),
                row_number,
                field_name,
                Some(value),
                None,
            ));
        }
    };

    let mut surfaces = Vec::new();
    for item in items {
        let text = item.as_str().ok_or_else(|| {
            input_contract_refusal(
                format!("Side field '{field_name}' may contain only strings"),
                row_number,
                field_name,
                Some(&item),
                None,
            )
        })?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(input_contract_refusal(
                format!("Side field '{field_name}' may not contain empty strings"),
                row_number,
                field_name,
                Some(&item),
                None,
            ));
        }
        surfaces.push(trimmed.to_string());
    }
    Ok(surfaces)
}

fn anchors(
    row: &PrepareRawRow,
    contract: &PrepareInputContract,
) -> Result<Vec<PreparedAnchor>, Refusal> {
    let mut anchors = Vec::new();
    for (namespace, field) in &contract.mapping.anchor_fields {
        let Some(value) = row.get(field) else {
            continue;
        };
        let Some(anchor_value) = scalar_string(value, row.row_number, field)? else {
            continue;
        };
        anchors.push(PreparedAnchor {
            namespace: namespace.clone(),
            value: anchor_value,
            field: field.clone(),
        });
    }
    Ok(anchors)
}

fn context(
    row: &PrepareRawRow,
    contract: &PrepareInputContract,
) -> Result<BTreeMap<String, Value>, Refusal> {
    let mut context = BTreeMap::new();
    for field in &contract.mapping.context_fields {
        if field == "source_row_id" {
            continue;
        }
        let Some(value) = row.get(field) else {
            continue;
        };
        if let Some(normalized) = normalized_context_value(value, row.row_number, field)? {
            context.insert(field.clone(), normalized);
        }
    }
    Ok(context)
}

fn provenance(
    row: &PrepareRawRow,
    contract: &PrepareInputContract,
) -> Result<BTreeMap<String, String>, Refusal> {
    let mut provenance = BTreeMap::new();
    for field in &contract.mapping.provenance_fields {
        let Some(value) = row.get(field) else {
            continue;
        };
        if let Some(text) = scalar_string(value, row.row_number, field)? {
            provenance.insert(field.clone(), text);
        }
    }
    Ok(provenance)
}

fn scalar_string(value: &Value, row_number: usize, field: &str) -> Result<Option<String>, Refusal> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Value::Bool(value) => Ok(Some(value.to_string())),
        Value::Number(value) => Ok(Some(value.to_string())),
        _ => Err(input_contract_refusal(
            format!("Prepare field '{field}' must be scalar"),
            row_number,
            field,
            Some(value),
            None,
        )),
    }
}

fn normalized_context_value(
    value: &Value,
    row_number: usize,
    field: &str,
) -> Result<Option<Value>, Refusal> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(Value::String(trimmed.to_string())))
            }
        }
        Value::Bool(_) | Value::Number(_) => Ok(Some(value.clone())),
        _ => Err(input_contract_refusal(
            format!("Prepare context field '{field}' must be scalar"),
            row_number,
            field,
            Some(value),
            None,
        )),
    }
}

fn blank_record(record: &StringRecord) -> bool {
    record.iter().all(|field| field.trim().is_empty())
}

enum PrepareInputFormat {
    Csv(u8),
    Jsonl,
}

fn prepare_input_format(path: &Path) -> Result<PrepareInputFormat, Refusal> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("csv") => Ok(PrepareInputFormat::Csv(b',')),
        Some("tsv") => Ok(PrepareInputFormat::Csv(b'\t')),
        Some("jsonl" | "ndjson") => Ok(PrepareInputFormat::Jsonl),
        _ => Err(EntityRefusalKind::InputContract.to_refusal(
            "Prepare input must be CSV, TSV, JSONL, or NDJSON",
            json!({
                "path": path.display().to_string(),
                "supported_extensions": ["csv", "tsv", "jsonl", "ndjson"]
            }),
            None,
        )),
    }
}

fn load_prepare_registry_snapshot(registry_dir: &Path) -> Result<PrepareRegistrySnapshot, Refusal> {
    let registry_json_path = registry_dir.join("registry.json");
    let registry_json_bytes = fs::read(&registry_json_path).map_err(|error| {
        EntityRefusalKind::RegistrySnapshot.to_refusal(
            "Failed to read entity registry snapshot metadata",
            json!({
                "registry": registry_dir.display().to_string(),
                "path": registry_json_path.display().to_string(),
                "error": error.to_string()
            }),
            None,
        )
    })?;
    let registry: RegistryJsonLite =
        serde_json::from_slice(&registry_json_bytes).map_err(|error| {
            EntityRefusalKind::RegistrySnapshot.to_refusal(
                "Failed to parse entity registry snapshot metadata",
                json!({
                    "registry": registry_dir.display().to_string(),
                    "path": registry_json_path.display().to_string(),
                    "error": error.to_string()
                }),
                None,
            )
        })?;

    Ok(PrepareRegistrySnapshot {
        id: registry.id,
        version: registry.version,
        source: registry_dir.display().to_string(),
        lookup_snapshot_hash: hash_registry_json_files(registry_dir)?,
    })
}

fn hash_registry_json_files(registry_dir: &Path) -> Result<String, Refusal> {
    let mut files = Vec::new();
    for entry in fs::read_dir(registry_dir).map_err(|error| {
        EntityRefusalKind::RegistrySnapshot.to_refusal(
            "Failed to read entity registry directory",
            json!({ "registry": registry_dir.display().to_string(), "error": error.to_string() }),
            None,
        )
    })? {
        let entry = entry.map_err(|error| {
            EntityRefusalKind::RegistrySnapshot.to_refusal(
                "Failed to inspect entity registry directory entry",
                json!({ "registry": registry_dir.display().to_string(), "error": error.to_string() }),
                None,
            )
        })?;
        let path = entry.path();
        if path.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("json")
        {
            files.push(path);
        }
    }
    files.sort();

    let mut hasher = blake3::Hasher::new();
    for path in files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        let bytes = fs::read(&path).map_err(|error| {
            EntityRefusalKind::RegistrySnapshot.to_refusal(
                "Failed to hash entity registry snapshot file",
                json!({ "path": path.display().to_string(), "error": error.to_string() }),
                None,
            )
        })?;
        hasher.update(&bytes);
        hasher.update(&[0]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn prepare_summary(observations: &[PreparedInputObservation]) -> BTreeMap<String, u64> {
    let observation_count = u64::try_from(observations.len()).expect("observation count fits u64");
    BTreeMap::from([
        ("row_count".to_string(), observation_count),
        ("prepared_observations".to_string(), observation_count),
        ("primary_surface_count".to_string(), observation_count),
        (
            "alias_surface_count".to_string(),
            observations
                .iter()
                .map(|observation| {
                    u64::try_from(observation.alias_surfaces.len())
                        .expect("alias surface count fits u64")
                })
                .sum(),
        ),
        (
            "mention_surface_count".to_string(),
            observations
                .iter()
                .map(|observation| {
                    u64::try_from(observation.mention_surfaces.len())
                        .expect("mention surface count fits u64")
                })
                .sum(),
        ),
        (
            "anchor_count".to_string(),
            observations
                .iter()
                .map(|observation| {
                    u64::try_from(observation.anchors.len()).expect("anchor count fits u64")
                })
                .sum(),
        ),
    ])
}

fn write_observations_jsonl(
    path: &Path,
    observations: &[PreparedInputObservation],
) -> Result<(), Refusal> {
    let mut file = File::create(path).map_err(|error| {
        io_budget_refusal(
            "Failed to create prepare surfaces file",
            path,
            error.to_string(),
        )
    })?;
    for observation in observations {
        let line = serde_json::to_string(observation).map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Failed to serialize prepare observation",
                json!({ "error": error.to_string() }),
                None,
            )
        })?;
        writeln!(file, "{line}").map_err(|error| {
            io_budget_refusal(
                "Failed to write prepare surfaces file",
                path,
                error.to_string(),
            )
        })?;
    }
    Ok(())
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), Refusal> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to serialize prepare artifact",
            json!({ "error": error.to_string() }),
            None,
        )
    })?;
    fs::write(path, bytes).map_err(|error| {
        io_budget_refusal("Failed to write prepare artifact", path, error.to_string())
    })
}

fn hash_artifact_without_self(artifact: &PrepareRunArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to hash prepare artifact",
            json!({ "error": error.to_string() }),
            None,
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn io_budget_refusal(message: &str, path: &Path, error: String) -> Refusal {
    EntityRefusalKind::IoBudget.to_refusal(
        message,
        json!({ "path": path.display().to_string(), "error": error }),
        None,
    )
}

fn input_contract_refusal(
    message: impl Into<String>,
    row_number: usize,
    field: &str,
    sample: Option<&Value>,
    error: Option<String>,
) -> Refusal {
    let mut detail = json!({
        "row_number": row_number,
        "field": field,
    });
    if let Some(sample) = sample {
        detail["sample"] = sample.clone();
    }
    if let Some(error) = error {
        detail["error"] = Value::String(error);
    }
    EntityRefusalKind::InputContract.to_refusal(message, detail, None)
}
