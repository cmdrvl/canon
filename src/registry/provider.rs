use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    error::Error,
    thread,
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryMaterializedEntry {
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFailure {
    pub input: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDiagnostic {
    pub input: String,
    pub kind: String,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimit {
    pub delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub seed_column: String,
    pub version: String,
    pub batch_size: usize,
    pub rate_limit_ms: Option<u64>,
    pub provider_options: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFetchResult {
    pub files: BTreeMap<String, Vec<RegistryMaterializedEntry>>,
    pub unresolved: Vec<String>,
    pub failures: Vec<ProviderFailure>,
    pub diagnostics: Vec<ProviderDiagnostic>,
    pub api_calls: usize,
}

/// One option accepted via `--provider-config KEY=VALUE` for a provider.
///
/// The schema is the machine-discoverable contract: agents read it to learn
/// which keys a provider accepts, their types, secret status, defaults, and
/// examples, instead of scraping prose or source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderOption {
    /// `--provider-config` key, e.g. `exchCode`.
    pub key: String,
    /// Value shape: `string`, `bool`, `enum`, `url`, `numeric_interval`, `date_interval`.
    #[serde(rename = "type")]
    pub value_type: String,
    /// Whether the option must be supplied (most provider options are optional).
    pub required: bool,
    /// Secret values must never be echoed by agents and are redacted in `_build.json`.
    pub secret: bool,
    /// One-line human/agent description.
    pub description: String,
    /// Allowed values for `enum`-typed options.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub enum_values: Vec<String>,
    /// Environment variable consulted when the option is absent.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub env_fallback: Option<String>,
    /// Default value applied when neither the option nor its env fallback is set.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default: Option<String>,
    /// Copy-paste-ready example value.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub example: Option<String>,
}

/// A worked `--provider-config` example for a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderExample {
    pub title: String,
    /// Repeatable `--provider-config KEY=VALUE` strings, in order.
    pub provider_config: Vec<String>,
}

/// The full, deterministic, JSON-serializable option contract for one provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSchema {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Seed columns the provider can materialize from.
    pub seed_columns: Vec<String>,
    /// Accepted `id_type` enum values (empty for providers without id types).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub id_types: Vec<String>,
    /// Deterministic seed-column → id_type inference map.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub id_type_inference: BTreeMap<String, String>,
    /// `--provider-config` options, in stable declaration order.
    pub options: Vec<ProviderOption>,
    /// Sets of option keys that cannot be combined, e.g. `["exchCode", "micCode"]`.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub mutual_exclusions: Vec<Vec<String>>,
    /// Human-readable rule for encoding interval-typed options.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub interval_encoding: Option<String>,
    pub examples: Vec<ProviderExample>,
}

/// A short catalog entry for `canon registry providers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub seed_columns: Vec<String>,
}

pub trait RegistryProvider {
    fn name(&self) -> &str;
    fn registry_id(&self, seed_column: &str) -> String;
    fn fetch(
        &self,
        identifiers: &[String],
        config: &ProviderConfig,
    ) -> Result<ProviderFetchResult, Box<dyn Error>>;
    fn id_types(&self) -> &[&str];
    fn rate_limit(&self, config: &ProviderConfig) -> Option<RateLimit>;

    /// Machine-discoverable `--provider-config` option contract for this provider.
    fn schema(&self) -> ProviderSchema;

    fn default_batch_size(&self, _provider_options: &BTreeMap<String, String>) -> usize {
        100
    }

    fn description(&self, seed_column: &str) -> String {
        format!(
            "Materialized {} registry for {} identifiers",
            self.name(),
            seed_column
        )
    }
}

pub fn provider_for_source(source: &str) -> Option<Box<dyn RegistryProvider>> {
    match source {
        "mock" => Some(Box::new(MockProvider)),
        "openfigi" => Some(Box::new(OpenFigiProvider)),
        _ => None,
    }
}

pub fn available_sources() -> Vec<&'static str> {
    vec!["mock", "openfigi"]
}

/// The deterministic provider catalog for `canon registry providers`, ordered
/// by `available_sources()`.
pub fn provider_catalog() -> Vec<ProviderCatalogEntry> {
    available_sources()
        .into_iter()
        .filter_map(provider_for_source)
        .map(|provider| {
            let schema = provider.schema();
            ProviderCatalogEntry {
                id: schema.id,
                name: schema.name,
                description: schema.description,
                seed_columns: schema.seed_columns,
            }
        })
        .collect()
}

/// The full option schema for one provider id, or `None` if unknown.
pub fn provider_schema(source: &str) -> Option<ProviderSchema> {
    provider_for_source(source).map(|provider| provider.schema())
}

struct MockProvider;

impl RegistryProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn registry_id(&self, seed_column: &str) -> String {
        format!("mock-{}", seed_column)
    }

    fn fetch(
        &self,
        identifiers: &[String],
        config: &ProviderConfig,
    ) -> Result<ProviderFetchResult, Box<dyn Error>> {
        let mut files = BTreeMap::new();
        let mut unresolved = Vec::new();
        let mut failures = Vec::new();
        let file_name = format!("{}-to-mock.json", config.seed_column.replace('_', "-"));
        let rule_id = format!("MOCK_{}_LOOKUP", config.seed_column.to_uppercase());

        for identifier in identifiers {
            if identifier.starts_with("FAIL_") {
                failures.push(ProviderFailure {
                    input: identifier.clone(),
                    kind: "provider_error".to_string(),
                    message: "mock provider failure".to_string(),
                });
                continue;
            }
            if identifier.starts_with("MISS_") {
                unresolved.push(identifier.clone());
                continue;
            }
            files
                .entry(file_name.clone())
                .or_insert_with(Vec::new)
                .push(RegistryMaterializedEntry {
                    input: identifier.clone(),
                    canonical_id: format!("MOCK::{}", identifier),
                    canonical_type: "mock_id".to_string(),
                    rule_id: rule_id.clone(),
                });
        }

        Ok(ProviderFetchResult {
            files,
            unresolved,
            failures,
            diagnostics: Vec::new(),
            api_calls: 1,
        })
    }

    fn id_types(&self) -> &[&str] {
        &["mock"]
    }

    fn rate_limit(&self, config: &ProviderConfig) -> Option<RateLimit> {
        config
            .rate_limit_ms
            .filter(|delay_ms| *delay_ms > 0)
            .map(|delay_ms| RateLimit { delay_ms })
    }

    fn schema(&self) -> ProviderSchema {
        ProviderSchema {
            id: "mock".to_string(),
            name: "Mock".to_string(),
            description:
                "Deterministic local materializer for tests and examples; ignores provider-config and contacts no network"
                    .to_string(),
            seed_columns: vec!["any".to_string()],
            id_types: Vec::new(),
            id_type_inference: BTreeMap::new(),
            options: Vec::new(),
            mutual_exclusions: Vec::new(),
            interval_encoding: None,
            examples: vec![ProviderExample {
                title: "Materialize a mock registry from a CUSIP seed".to_string(),
                provider_config: Vec::new(),
            }],
        }
    }
}

const OPENFIGI_API_URL: &str = "https://api.openfigi.com/v3/mapping";
const OPENFIGI_ID_TYPES: [&str; 3] = ["ID_CUSIP", "ID_ISIN", "ID_SEDOL"];
const OPENFIGI_RETRY_STATUSES: [u16; 5] = [429, 500, 502, 503, 504];
const OPENFIGI_STRING_FILTERS: [&str; 7] = [
    "exchCode",
    "micCode",
    "currency",
    "marketSecDes",
    "securityType",
    "securityType2",
    "optionType",
];
const OPENFIGI_NUMERIC_INTERVAL_FILTERS: [&str; 3] = ["strike", "contractSize", "coupon"];
const OPENFIGI_DATE_INTERVAL_FILTERS: [&str; 2] = ["expiration", "maturity"];
const OPENFIGI_BOOL_FILTERS: [&str; 1] = ["includeUnlistedEquities"];

struct OpenFigiProvider;

impl RegistryProvider for OpenFigiProvider {
    fn name(&self) -> &str {
        "openfigi"
    }

    fn registry_id(&self, seed_column: &str) -> String {
        format!("openfigi-{}", seed_column.replace('_', "-"))
    }

    fn fetch(
        &self,
        identifiers: &[String],
        config: &ProviderConfig,
    ) -> Result<ProviderFetchResult, Box<dyn Error>> {
        let id_type = openfigi_id_type(config)?;
        let base_url = openfigi_base_url(&config.provider_options);
        let auth_header_value = openfigi_auth_header_value(&config.provider_options);
        let mapping_filters = openfigi_mapping_filters(&config.provider_options)?;
        let jobs = identifiers
            .iter()
            .map(|identifier| OpenFigiMappingJob {
                id_type: id_type.as_str(),
                id_value: identifier.as_str(),
                filters: mapping_filters.clone(),
            })
            .collect::<Vec<_>>();

        let (responses, api_calls) = execute_openfigi_request(
            &base_url,
            auth_header_value.as_deref(),
            &jobs,
            self.rate_limit(config),
        )?;
        if responses.len() != identifiers.len() {
            return Err(format!(
                "OpenFIGI returned {} results for {} identifiers",
                responses.len(),
                identifiers.len()
            )
            .into());
        }

        let mut files = BTreeMap::new();
        let mut unresolved = Vec::new();
        let mut failures = Vec::new();
        let mut diagnostics = Vec::new();

        let file_prefix = config.seed_column.replace('_', "-");
        let rule_prefix = config.seed_column.to_ascii_uppercase();

        for (identifier, response) in identifiers.iter().zip(responses) {
            if let Some(error) = response.error {
                failures.push(ProviderFailure {
                    input: identifier.clone(),
                    kind: "provider_error".to_string(),
                    message: format!("OpenFIGI error: {error}"),
                });
                continue;
            }

            if response.data.is_empty() {
                unresolved.push(identifier.clone());
                continue;
            }

            match select_openfigi_candidate(identifier, &response.data) {
                OpenFigiSelection::Selected(candidate) => {
                    if let Some(primary_id) = candidate.primary_identifier() {
                        let rule_suffix = if candidate.composite_figi.is_some() {
                            "COMPOSITE_FIGI"
                        } else {
                            "FIGI"
                        };
                        let canonical_type = if candidate.composite_figi.is_some() {
                            "composite_figi"
                        } else {
                            "figi"
                        };
                        push_mapping(
                            &mut files,
                            format!("{file_prefix}-to-figi.json"),
                            RegistryMaterializedEntry {
                                input: identifier.clone(),
                                canonical_id: primary_id,
                                canonical_type: canonical_type.to_string(),
                                rule_id: format!("OPENFIGI_{rule_prefix}_TO_{rule_suffix}"),
                            },
                        );
                    }

                    if let Some(ticker) = normalize_nonempty(candidate.ticker.as_deref()) {
                        push_mapping(
                            &mut files,
                            format!("{file_prefix}-to-ticker.json"),
                            RegistryMaterializedEntry {
                                input: identifier.clone(),
                                canonical_id: ticker.to_string(),
                                canonical_type: "ticker".to_string(),
                                rule_id: format!("OPENFIGI_{rule_prefix}_TO_TICKER"),
                            },
                        );
                    }

                    if let Some(name) = normalize_nonempty(candidate.name.as_deref()) {
                        push_mapping(
                            &mut files,
                            format!("{file_prefix}-to-name.json"),
                            RegistryMaterializedEntry {
                                input: identifier.clone(),
                                canonical_id: name.to_string(),
                                canonical_type: "security_name".to_string(),
                                rule_id: format!("OPENFIGI_{rule_prefix}_TO_NAME"),
                            },
                        );
                    }
                }
                OpenFigiSelection::Ambiguous(candidates) => {
                    failures.push(ProviderFailure {
                        input: identifier.clone(),
                        kind: "ambiguous_match".to_string(),
                        message: format!(
                            "OpenFIGI returned multiple plausible matches for {}",
                            identifier
                        ),
                    });
                    diagnostics.push(ProviderDiagnostic {
                        input: identifier.clone(),
                        kind: "ambiguous_match".to_string(),
                        detail: serde_json::json!({ "candidates": candidates }),
                    });
                }
            }
        }

        Ok(ProviderFetchResult {
            files,
            unresolved,
            failures,
            diagnostics,
            api_calls,
        })
    }

    fn id_types(&self) -> &[&str] {
        &OPENFIGI_ID_TYPES
    }

    fn rate_limit(&self, config: &ProviderConfig) -> Option<RateLimit> {
        if let Some(delay_ms) = config.rate_limit_ms.filter(|delay_ms| *delay_ms > 0) {
            return Some(RateLimit { delay_ms });
        }

        let delay_ms = if openfigi_auth_header_value(&config.provider_options).is_some() {
            240
        } else {
            2_400
        };
        Some(RateLimit { delay_ms })
    }

    fn default_batch_size(&self, provider_options: &BTreeMap<String, String>) -> usize {
        if openfigi_auth_header_value(provider_options).is_some() {
            100
        } else {
            10
        }
    }

    fn description(&self, seed_column: &str) -> String {
        format!("Materialized OpenFIGI registry for {seed_column} identifiers")
    }

    fn schema(&self) -> ProviderSchema {
        let mut options = vec![
            ProviderOption {
                key: "id_type".to_string(),
                value_type: "enum".to_string(),
                required: false,
                secret: false,
                description:
                    "OpenFIGI identifier type; inferred from --seed-column when omitted".to_string(),
                enum_values: OPENFIGI_ID_TYPES.iter().map(|v| v.to_string()).collect(),
                env_fallback: None,
                default: None,
                example: Some("ID_CUSIP".to_string()),
            },
            ProviderOption {
                key: "base_url".to_string(),
                value_type: "url".to_string(),
                required: false,
                secret: false,
                description:
                    "OpenFIGI mapping endpoint; override for local twins and tests".to_string(),
                enum_values: Vec::new(),
                env_fallback: None,
                default: Some(OPENFIGI_API_URL.to_string()),
                example: Some("http://127.0.0.1:8080/v3/mapping".to_string()),
            },
            ProviderOption {
                key: "api_key".to_string(),
                value_type: "string".to_string(),
                required: false,
                secret: true,
                description:
                    "OpenFIGI API key; raises batch size and rate limit. Secret: never echo the value"
                        .to_string(),
                enum_values: Vec::new(),
                env_fallback: Some("OPENFIGI_API_KEY".to_string()),
                default: None,
                example: None,
            },
        ];

        for key in OPENFIGI_STRING_FILTERS {
            options.push(ProviderOption {
                key: key.to_string(),
                value_type: "string".to_string(),
                required: false,
                secret: false,
                description: openfigi_filter_description(key),
                enum_values: Vec::new(),
                env_fallback: None,
                default: None,
                example: openfigi_filter_example(key),
            });
        }
        for key in OPENFIGI_BOOL_FILTERS {
            options.push(ProviderOption {
                key: key.to_string(),
                value_type: "bool".to_string(),
                required: false,
                secret: false,
                description: openfigi_filter_description(key),
                enum_values: Vec::new(),
                env_fallback: None,
                default: None,
                example: Some("false".to_string()),
            });
        }
        for key in OPENFIGI_NUMERIC_INTERVAL_FILTERS {
            options.push(ProviderOption {
                key: key.to_string(),
                value_type: "numeric_interval".to_string(),
                required: false,
                secret: false,
                description: openfigi_filter_description(key),
                enum_values: Vec::new(),
                env_fallback: None,
                default: None,
                example: Some("[3.0,5.0]".to_string()),
            });
        }
        for key in OPENFIGI_DATE_INTERVAL_FILTERS {
            options.push(ProviderOption {
                key: key.to_string(),
                value_type: "date_interval".to_string(),
                required: false,
                secret: false,
                description: openfigi_filter_description(key),
                enum_values: Vec::new(),
                env_fallback: None,
                default: None,
                example: Some("[\"2028-01-01\",null]".to_string()),
            });
        }

        let id_type_inference = BTreeMap::from([
            ("cusip".to_string(), "ID_CUSIP".to_string()),
            ("isin".to_string(), "ID_ISIN".to_string()),
            ("sedol".to_string(), "ID_SEDOL".to_string()),
        ]);

        ProviderSchema {
            id: "openfigi".to_string(),
            name: "OpenFIGI".to_string(),
            description:
                "Corpus-scoped securities identifier materializer for CUSIP/ISIN/SEDOL seeds; maintenance-only, never used during normal lookup"
                    .to_string(),
            seed_columns: vec!["cusip".to_string(), "isin".to_string(), "sedol".to_string()],
            id_types: OPENFIGI_ID_TYPES.iter().map(|v| v.to_string()).collect(),
            id_type_inference,
            options,
            mutual_exclusions: vec![vec!["exchCode".to_string(), "micCode".to_string()]],
            interval_encoding: Some(
                "JSON array of exactly two values; null is allowed for an open bound; at least one bound is required"
                    .to_string(),
            ),
            examples: vec![
                ProviderExample {
                    title: "Disambiguate to U.S.-listed instruments".to_string(),
                    provider_config: vec!["exchCode=US".to_string()],
                },
                ProviderExample {
                    title: "Bond/note cohort by coupon and maturity".to_string(),
                    provider_config: vec![
                        "securityType2=Note".to_string(),
                        "coupon=[3.0,5.0]".to_string(),
                        "maturity=[\"2028-01-01\",null]".to_string(),
                    ],
                },
            ],
        }
    }
}

fn openfigi_filter_description(key: &str) -> String {
    let text = match key {
        "exchCode" => "Exchange code filter; mutually exclusive with micCode",
        "micCode" => "ISO 10383 market identifier code filter; mutually exclusive with exchCode",
        "currency" => "Trading currency filter (ISO 4217)",
        "marketSecDes" => "Market sector description filter, e.g. Equity, Corp, Govt",
        "securityType" => "OpenFIGI security type filter",
        "securityType2" => "OpenFIGI secondary security type filter, e.g. Common Stock, Note",
        "optionType" => "Option type filter, e.g. Call or Put",
        "includeUnlistedEquities" => "Include unlisted equities in mapping results (true/false)",
        "strike" => "Strike-price interval filter",
        "contractSize" => "Contract-size interval filter",
        "coupon" => "Coupon interval filter",
        "expiration" => "Expiration-date interval filter (YYYY-MM-DD bounds)",
        "maturity" => "Maturity-date interval filter (YYYY-MM-DD bounds)",
        _ => "OpenFIGI mapping filter",
    };
    text.to_string()
}

fn openfigi_filter_example(key: &str) -> Option<String> {
    let example = match key {
        "exchCode" => "US",
        "micCode" => "XNAS",
        "currency" => "USD",
        "marketSecDes" => "Equity",
        "securityType" => "Common Stock",
        "securityType2" => "Common Stock",
        "optionType" => "Call",
        _ => return None,
    };
    Some(example.to_string())
}

#[derive(Debug, Serialize)]
struct OpenFigiMappingJob<'a> {
    #[serde(rename = "idType")]
    id_type: &'a str,
    #[serde(rename = "idValue")]
    id_value: &'a str,
    #[serde(flatten)]
    filters: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct OpenFigiResponseItem {
    #[serde(default)]
    data: Vec<OpenFigiCandidate>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct OpenFigiCandidate {
    #[serde(default)]
    figi: Option<String>,
    #[serde(rename = "compositeFIGI", default)]
    composite_figi: Option<String>,
    #[serde(default)]
    ticker: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "securityType", default)]
    security_type: Option<String>,
    #[serde(rename = "exchCode", default)]
    exch_code: Option<String>,
}

impl OpenFigiCandidate {
    fn primary_identifier(&self) -> Option<String> {
        normalize_nonempty(self.composite_figi.as_deref())
            .or_else(|| normalize_nonempty(self.figi.as_deref()))
            .map(ToOwned::to_owned)
    }

    fn score(&self) -> usize {
        let mut score = 0;
        if self.composite_figi.is_some() {
            score += 100;
        } else if self.figi.is_some() {
            score += 50;
        }

        if let Some(security_type) = self.security_type.as_deref() {
            let lower = security_type.to_ascii_lowercase();
            if lower.contains("common stock") {
                score += 20;
            } else if lower.contains("ordinary shares") || lower.contains("equity") {
                score += 10;
            }
        }

        if normalize_nonempty(self.ticker.as_deref()).is_some() {
            score += 2;
        }
        if normalize_nonempty(self.name.as_deref()).is_some() {
            score += 1;
        }

        score
    }
}

enum OpenFigiSelection {
    Selected(OpenFigiCandidate),
    Ambiguous(Vec<OpenFigiCandidate>),
}

fn select_openfigi_candidate(
    _identifier: &str,
    candidates: &[OpenFigiCandidate],
) -> OpenFigiSelection {
    let unique_primary_ids = candidates
        .iter()
        .filter_map(OpenFigiCandidate::primary_identifier)
        .collect::<BTreeSet<_>>();

    if unique_primary_ids.len() <= 1 {
        let mut ranked = candidates.to_vec();
        ranked.sort_by(|left, right| {
            right
                .score()
                .cmp(&left.score())
                .then_with(|| left.primary_identifier().cmp(&right.primary_identifier()))
        });
        return OpenFigiSelection::Selected(
            ranked
                .into_iter()
                .next()
                .expect("candidates should not be empty"),
        );
    }

    let mut ranked = candidates
        .iter()
        .map(|candidate| (candidate.score(), candidate.clone()))
        .collect::<Vec<_>>();
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.0));

    let top_score = ranked.first().map(|(score, _)| *score).unwrap_or_default();
    let top_candidates = ranked
        .into_iter()
        .filter(|(score, _)| *score == top_score)
        .map(|(_, candidate)| candidate)
        .collect::<Vec<_>>();

    if top_candidates.len() == 1 {
        OpenFigiSelection::Selected(top_candidates.into_iter().next().unwrap())
    } else {
        OpenFigiSelection::Ambiguous(top_candidates)
    }
}

fn openfigi_id_type(config: &ProviderConfig) -> Result<String, Box<dyn Error>> {
    if let Some(configured) = config.provider_options.get("id_type") {
        let normalized = configured.trim().to_ascii_uppercase();
        if OPENFIGI_ID_TYPES.contains(&normalized.as_str()) {
            return Ok(normalized);
        }

        return Err(format!(
            "Unsupported OpenFIGI id_type '{}'; supported values: {}",
            configured,
            OPENFIGI_ID_TYPES.join(", ")
        )
        .into());
    }

    match config.seed_column.to_ascii_lowercase().as_str() {
        "cusip" => Ok("ID_CUSIP".to_string()),
        "isin" => Ok("ID_ISIN".to_string()),
        "sedol" => Ok("ID_SEDOL".to_string()),
        _ => Err(format!(
            "Cannot infer OpenFIGI id_type from seed column '{}'; set --provider-config id_type=...",
            config.seed_column
        )
        .into()),
    }
}

fn openfigi_base_url(provider_options: &BTreeMap<String, String>) -> String {
    provider_options
        .get("base_url")
        .map(String::as_str)
        .and_then(|value| normalize_nonempty(Some(value)))
        .unwrap_or(OPENFIGI_API_URL)
        .to_string()
}

fn openfigi_auth_header_value(provider_options: &BTreeMap<String, String>) -> Option<String> {
    provider_options
        .get("api_key")
        .cloned()
        .or_else(|| env::var("OPENFIGI_API_KEY").ok())
        .filter(|value| !value.trim().is_empty())
}

fn openfigi_mapping_filters(
    provider_options: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, serde_json::Value>, Box<dyn Error>> {
    let mut filters = BTreeMap::new();

    for key in OPENFIGI_STRING_FILTERS {
        if let Some(value) = provider_options
            .get(key)
            .and_then(|value| normalize_nonempty(Some(value)))
        {
            filters.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }

    for key in OPENFIGI_BOOL_FILTERS {
        if let Some(value) = provider_options
            .get(key)
            .and_then(|value| normalize_nonempty(Some(value)))
        {
            filters.insert(
                key.to_string(),
                serde_json::Value::Bool(parse_bool_filter(key, value)?),
            );
        }
    }

    for key in OPENFIGI_NUMERIC_INTERVAL_FILTERS {
        if let Some(value) = provider_options
            .get(key)
            .and_then(|value| normalize_nonempty(Some(value)))
        {
            filters.insert(
                key.to_string(),
                parse_interval_filter(key, value, IntervalKind::Numeric)?,
            );
        }
    }

    for key in OPENFIGI_DATE_INTERVAL_FILTERS {
        if let Some(value) = provider_options
            .get(key)
            .and_then(|value| normalize_nonempty(Some(value)))
        {
            filters.insert(
                key.to_string(),
                parse_interval_filter(key, value, IntervalKind::Date)?,
            );
        }
    }

    if provider_options.contains_key("exchCode") && provider_options.contains_key("micCode") {
        return Err("OpenFIGI filters exchCode and micCode cannot be used together".into());
    }

    Ok(filters)
}

fn parse_bool_filter(key: &str, value: &str) -> Result<bool, Box<dyn Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("OpenFIGI filter {key} must be a boolean literal: true or false").into()),
    }
}

#[derive(Clone, Copy)]
enum IntervalKind {
    Numeric,
    Date,
}

fn parse_interval_filter(
    key: &str,
    value: &str,
    kind: IntervalKind,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let parsed = serde_json::from_str::<serde_json::Value>(value)
        .map_err(|error| format!("OpenFIGI filter {key} must be a JSON array interval: {error}"))?;
    let items = parsed
        .as_array()
        .ok_or_else(|| format!("OpenFIGI filter {key} must be a JSON array interval"))?;
    if items.len() != 2 {
        return Err(
            format!("OpenFIGI filter {key} interval must contain exactly two values").into(),
        );
    }

    let mut has_bound = false;
    for item in items {
        match kind {
            IntervalKind::Numeric => {
                if item.is_number() {
                    has_bound = true;
                } else if !item.is_null() {
                    return Err(format!(
                        "OpenFIGI filter {key} interval values must be numbers or null"
                    )
                    .into());
                }
            }
            IntervalKind::Date => {
                if let Some(date) = item.as_str() {
                    validate_openfigi_date(key, date)?;
                    has_bound = true;
                } else if !item.is_null() {
                    return Err(format!(
                        "OpenFIGI filter {key} interval values must be YYYY-MM-DD strings or null"
                    )
                    .into());
                }
            }
        }
    }
    if !has_bound {
        return Err(
            format!("OpenFIGI filter {key} interval must include at least one bound").into(),
        );
    }
    if matches!(kind, IntervalKind::Numeric) {
        let [left_value, right_value] = items.as_slice() else {
            return Err(
                format!("OpenFIGI filter {key} interval must contain exactly two values").into(),
            );
        };
        if let (Some(left), Some(right)) = (left_value.as_f64(), right_value.as_f64())
            && left > right
        {
            return Err(
                format!("OpenFIGI filter {key} interval lower bound exceeds upper bound").into(),
            );
        }
    }

    Ok(parsed)
}

fn validate_openfigi_date(key: &str, value: &str) -> Result<(), Box<dyn Error>> {
    let bytes = value.as_bytes();
    let valid_shape = match bytes {
        [
            year_0,
            year_1,
            year_2,
            year_3,
            b'-',
            month_0,
            month_1,
            b'-',
            day_0,
            day_1,
        ] => [
            year_0, year_1, year_2, year_3, month_0, month_1, day_0, day_1,
        ]
        .into_iter()
        .all(|byte| byte.is_ascii_digit()),
        _ => false,
    };
    if valid_shape {
        return Ok(());
    }

    Err(format!("OpenFIGI filter {key} date must use YYYY-MM-DD format").into())
}

fn execute_openfigi_request(
    base_url: &str,
    auth_header_value: Option<&str>,
    jobs: &[OpenFigiMappingJob<'_>],
    rate_limit: Option<RateLimit>,
) -> Result<(Vec<OpenFigiResponseItem>, usize), Box<dyn Error>> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .timeout_write(Duration::from_secs(30))
        .build();
    let body = serde_json::to_value(jobs)?;
    let mut api_calls = 0usize;

    for attempt in 0..3 {
        api_calls += 1;

        let mut request = agent
            .post(base_url)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json");
        if let Some(auth_header_value) = auth_header_value {
            request = request.set("X-OPENFIGI-APIKEY", auth_header_value);
        }

        match request.send_json(body.clone()) {
            Ok(response) => {
                let payload = response.into_json::<Vec<OpenFigiResponseItem>>()?;
                return Ok((payload, api_calls));
            }
            Err(ureq::Error::Status(status, response))
                if OPENFIGI_RETRY_STATUSES.contains(&status) && attempt < 2 =>
            {
                thread::sleep(Duration::from_millis(openfigi_retry_delay_ms(
                    &response, attempt, rate_limit,
                )));
            }
            Err(ureq::Error::Status(status, response)) => {
                let response_body = response.into_string().unwrap_or_default();
                return Err(format!(
                    "OpenFIGI request failed with status {}: {}",
                    status, response_body
                )
                .into());
            }
            Err(error) if attempt < 2 => {
                thread::sleep(Duration::from_millis(
                    rate_limit.map(|limit| limit.delay_ms).unwrap_or(250),
                ));
                if attempt == 2 {
                    return Err(error.into());
                }
            }
            Err(error) => return Err(error.into()),
        }
    }

    Err("OpenFIGI request retries exhausted".into())
}

fn openfigi_retry_delay_ms(
    response: &ureq::Response,
    attempt: usize,
    rate_limit: Option<RateLimit>,
) -> u64 {
    if let Some(value) = response.header("Retry-After")
        && let Ok(seconds) = value.parse::<u64>()
    {
        return seconds.saturating_mul(1_000);
    }

    if let Some(value) = response.header("ratelimit-reset")
        && let Ok(seconds) = value.parse::<u64>()
    {
        return seconds.saturating_mul(1_000);
    }

    let base_delay = rate_limit.map(|limit| limit.delay_ms).unwrap_or(250);
    base_delay.saturating_mul(1_u64 << attempt)
}

fn push_mapping(
    files: &mut BTreeMap<String, Vec<RegistryMaterializedEntry>>,
    file_name: String,
    entry: RegistryMaterializedEntry,
) {
    files.entry(file_name).or_default().push(entry);
}

fn normalize_nonempty(value: Option<&str>) -> Option<&str> {
    value.filter(|candidate| !candidate.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        thread::{self, JoinHandle},
    };
    use tiny_http::{Header, Response, Server, StatusCode};

    #[derive(Debug)]
    struct RecordedRequest {
        path: String,
        body: String,
        headers: BTreeMap<String, String>,
    }

    struct MockServer {
        base_url: String,
        requests: mpsc::Receiver<RecordedRequest>,
        handle: JoinHandle<()>,
    }

    type MockResponse = (u16, Vec<(&'static str, &'static str)>, String);

    impl MockServer {
        fn finish(self) -> Vec<RecordedRequest> {
            self.handle.join().unwrap();
            self.requests.try_iter().collect()
        }
    }

    fn spawn_server(responses: Vec<MockResponse>) -> MockServer {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}/v3/mapping", server.server_addr());
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            for (status, headers, body) in responses {
                let mut request = server.recv().unwrap();
                let mut request_body = String::new();
                request
                    .as_reader()
                    .read_to_string(&mut request_body)
                    .unwrap();
                let headers_map = request
                    .headers()
                    .iter()
                    .map(|header| {
                        (
                            header.field.as_str().to_string().to_ascii_lowercase(),
                            header.value.as_str().to_string(),
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                tx.send(RecordedRequest {
                    path: request.url().to_string(),
                    body: request_body,
                    headers: headers_map,
                })
                .unwrap();

                let mut response = Response::from_string(body).with_status_code(StatusCode(status));
                for (name, value) in headers {
                    response = response.with_header(Header::from_bytes(name, value).unwrap());
                }
                request.respond(response).unwrap();
            }
        });

        MockServer {
            base_url,
            requests: rx,
            handle,
        }
    }

    fn openfigi_config(base_url: &str) -> ProviderConfig {
        ProviderConfig {
            seed_column: "cusip".to_string(),
            version: "2026.03.13".to_string(),
            batch_size: 10,
            rate_limit_ms: Some(1),
            provider_options: BTreeMap::from([("base_url".to_string(), base_url.to_string())]),
        }
    }

    #[test]
    fn openfigi_fetch_handles_success_and_partial_failure() {
        let server = spawn_server(vec![(
            200,
            vec![("Content-Type", "application/json")],
            serde_json::json!([
                {
                    "data": [{
                        "figi": "BBG000B9XRY4",
                        "compositeFIGI": "BBG000B9XRY4",
                        "ticker": "AAPL",
                        "name": "APPLE INC",
                        "securityType": "Common Stock"
                    }]
                },
                { "error": "No identifier found." }
            ])
            .to_string(),
        )]);
        let provider = OpenFigiProvider;
        let result = provider
            .fetch(
                &["037833100".to_string(), "BAD_ID".to_string()],
                &openfigi_config(&server.base_url),
            )
            .unwrap();

        assert_eq!(result.api_calls, 1);
        assert_eq!(
            result.files["cusip-to-figi.json"][0].canonical_id,
            "BBG000B9XRY4"
        );
        assert_eq!(result.files["cusip-to-ticker.json"][0].canonical_id, "AAPL");
        assert_eq!(
            result.files["cusip-to-name.json"][0].canonical_id,
            "APPLE INC"
        );
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].kind, "provider_error");
        assert_eq!(result.failures[0].input, "BAD_ID");

        let requests = server.finish();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/v3/mapping");
        assert!(requests[0].body.contains("\"idType\":\"ID_CUSIP\""));
        assert!(requests[0].body.contains("\"idValue\":\"037833100\""));
    }

    #[test]
    fn openfigi_fetch_passes_mapping_filters() {
        let server = spawn_server(vec![(
            200,
            vec![("Content-Type", "application/json")],
            serde_json::json!([
                {
                    "data": [{
                        "figi": "BBG000BPH459",
                        "compositeFIGI": "BBG000BPH459",
                        "ticker": "MSFT",
                        "name": "MICROSOFT CORP",
                        "securityType": "Common Stock"
                    }]
                }
            ])
            .to_string(),
        )]);
        let provider = OpenFigiProvider;
        let mut config = openfigi_config(&server.base_url);
        config.seed_column = "isin".to_string();
        config
            .provider_options
            .insert("id_type".to_string(), "ID_ISIN".to_string());
        config
            .provider_options
            .insert("exchCode".to_string(), "US".to_string());
        config
            .provider_options
            .insert("marketSecDes".to_string(), "Equity".to_string());
        config
            .provider_options
            .insert("securityType2".to_string(), "Common Stock".to_string());
        config
            .provider_options
            .insert("includeUnlistedEquities".to_string(), "false".to_string());
        config
            .provider_options
            .insert("coupon".to_string(), "[3.125,3.125]".to_string());
        config
            .provider_options
            .insert("maturity".to_string(), "[\"2028-01-01\",null]".to_string());

        let result = provider
            .fetch(&["US5949181045".to_string()], &config)
            .unwrap();

        let figi_entry = result
            .files
            .get("isin-to-figi.json")
            .and_then(|entries| entries.first())
            .unwrap();
        assert_eq!(figi_entry.canonical_id, "BBG000BPH459");
        let requests = server.finish();
        let request = requests.first().unwrap();
        let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
        let job = body.as_array().and_then(|jobs| jobs.first()).unwrap();
        assert_eq!(job["idType"], "ID_ISIN");
        assert_eq!(job["idValue"], "US5949181045");
        assert_eq!(job["exchCode"], "US");
        assert_eq!(job["marketSecDes"], "Equity");
        assert_eq!(job["securityType2"], "Common Stock");
        assert_eq!(job["includeUnlistedEquities"], false);
        assert_eq!(job["coupon"], serde_json::json!([3.125, 3.125]));
        assert_eq!(job["maturity"], serde_json::json!(["2028-01-01", null]));
        assert!(job.get("base_url").is_none());
        assert!(job.get("api_key").is_none());
    }

    #[test]
    fn openfigi_filters_reject_invalid_values() {
        let mut config = openfigi_config("http://127.0.0.1:1/v3/mapping");
        config
            .provider_options
            .insert("exchCode".to_string(), "US".to_string());
        config
            .provider_options
            .insert("micCode".to_string(), "XNAS".to_string());
        assert!(
            openfigi_mapping_filters(&config.provider_options)
                .unwrap_err()
                .to_string()
                .contains("cannot be used together")
        );

        config.provider_options.remove("micCode");
        config
            .provider_options
            .insert("includeUnlistedEquities".to_string(), "yes".to_string());
        assert!(
            openfigi_mapping_filters(&config.provider_options)
                .unwrap_err()
                .to_string()
                .contains("must be a boolean")
        );

        config.provider_options.remove("includeUnlistedEquities");
        config
            .provider_options
            .insert("coupon".to_string(), "[4.0,3.0]".to_string());
        assert!(
            openfigi_mapping_filters(&config.provider_options)
                .unwrap_err()
                .to_string()
                .contains("lower bound exceeds upper bound")
        );
    }

    #[test]
    fn openfigi_fetch_marks_ambiguous_matches() {
        let server = spawn_server(vec![(
            200,
            vec![("Content-Type", "application/json")],
            serde_json::json!([
                {
                    "data": [
                        {
                            "figi": "BBG000ONE111",
                            "compositeFIGI": "BBG000ONE111",
                            "ticker": "ONE",
                            "name": "ONE CORP",
                            "securityType": "Common Stock"
                        },
                        {
                            "figi": "BBG000TWO222",
                            "compositeFIGI": "BBG000TWO222",
                            "ticker": "TWO",
                            "name": "TWO CORP",
                            "securityType": "Common Stock"
                        }
                    ]
                }
            ])
            .to_string(),
        )]);
        let provider = OpenFigiProvider;
        let result = provider
            .fetch(
                &["037833100".to_string()],
                &openfigi_config(&server.base_url),
            )
            .unwrap();

        assert!(result.files.is_empty());
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].kind, "ambiguous_match");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].kind, "ambiguous_match");
        assert!(result.diagnostics[0].detail["candidates"].is_array());

        let _ = server.finish();
    }

    #[test]
    fn openfigi_fetch_records_no_match_as_unresolved() {
        let server = spawn_server(vec![(
            200,
            vec![("Content-Type", "application/json")],
            serde_json::json!([{ "data": [] }]).to_string(),
        )]);
        let provider = OpenFigiProvider;
        let result = provider
            .fetch(
                &["000000000".to_string()],
                &openfigi_config(&server.base_url),
            )
            .unwrap();

        assert!(result.files.is_empty());
        assert_eq!(result.unresolved, vec!["000000000"]);
        assert!(result.failures.is_empty());
        assert_eq!(result.api_calls, 1);

        let _ = server.finish();
    }

    #[test]
    fn openfigi_fetch_rejects_mismatched_result_count() {
        let server = spawn_server(vec![(
            200,
            vec![("Content-Type", "application/json")],
            serde_json::json!([{ "data": [] }]).to_string(),
        )]);
        let provider = OpenFigiProvider;
        let error = provider
            .fetch(
                &["037833100".to_string(), "594918104".to_string()],
                &openfigi_config(&server.base_url),
            )
            .unwrap_err()
            .to_string();

        assert!(error.contains("OpenFIGI returned 1 results for 2 identifiers"));

        let _ = server.finish();
    }

    #[test]
    fn openfigi_fetch_rejects_malformed_top_level_payload() {
        let server = spawn_server(vec![(
            200,
            vec![("Content-Type", "application/json")],
            serde_json::json!({ "data": null, "warning": null }).to_string(),
        )]);
        let provider = OpenFigiProvider;
        let result = provider.fetch(
            &["037833100".to_string()],
            &openfigi_config(&server.base_url),
        );

        assert!(result.is_err());

        let _ = server.finish();
    }

    #[test]
    fn openfigi_fetch_retries_rate_limits_and_sends_api_key() {
        let server = spawn_server(vec![
            (
                429,
                vec![
                    ("Content-Type", "application/json"),
                    ("ratelimit-reset", "0"),
                ],
                "{\"error\":\"rate limited\"}".to_string(),
            ),
            (
                200,
                vec![("Content-Type", "application/json")],
                serde_json::json!([
                    {
                        "data": [{
                            "figi": "BBG000BPH459",
                            "compositeFIGI": "BBG000BPH459",
                            "ticker": "MSFT",
                            "name": "MICROSOFT CORP",
                            "securityType": "Common Stock"
                        }]
                    }
                ])
                .to_string(),
            ),
        ]);
        let provider = OpenFigiProvider;
        let mut config = openfigi_config(&server.base_url);
        config
            .provider_options
            .insert("api_key".to_string(), "secret-key".to_string());

        let result = provider.fetch(&["594918104".to_string()], &config).unwrap();

        assert_eq!(result.api_calls, 2);
        assert_eq!(
            result.files["cusip-to-figi.json"][0].canonical_id,
            "BBG000BPH459"
        );

        let requests = server.finish();
        assert_eq!(requests.len(), 2);
        for request in requests {
            assert_eq!(
                request.headers.get("x-openfigi-apikey").map(String::as_str),
                Some("secret-key")
            );
        }
    }

    #[test]
    fn openfigi_fetch_retries_retry_after_header() {
        let server = spawn_server(vec![
            (
                429,
                vec![("Content-Type", "application/json"), ("Retry-After", "0")],
                "{\"error\":\"rate limited\"}".to_string(),
            ),
            (
                200,
                vec![("Content-Type", "application/json")],
                serde_json::json!([
                    {
                        "data": [{
                            "figi": "BBG000B9XRY4",
                            "compositeFIGI": "BBG000B9XRY4",
                            "ticker": "AAPL",
                            "name": "APPLE INC",
                            "securityType": "Common Stock"
                        }]
                    }
                ])
                .to_string(),
            ),
        ]);
        let provider = OpenFigiProvider;

        let result = provider
            .fetch(
                &["037833100".to_string()],
                &openfigi_config(&server.base_url),
            )
            .unwrap();

        assert_eq!(result.api_calls, 2);
        assert_eq!(
            result.files["cusip-to-figi.json"][0].canonical_id,
            "BBG000B9XRY4"
        );

        let requests = server.finish();
        assert_eq!(requests.len(), 2);
    }

    #[test]
    fn openfigi_batch_and_rate_limit_defaults_depend_on_api_key() {
        let provider = OpenFigiProvider;
        let no_key = BTreeMap::new();
        let with_key = BTreeMap::from([("api_key".to_string(), "abc".to_string())]);

        assert_eq!(provider.default_batch_size(&no_key), 10);
        assert_eq!(provider.default_batch_size(&with_key), 100);
        assert_eq!(
            provider
                .rate_limit(&ProviderConfig {
                    seed_column: "cusip".to_string(),
                    version: "2026.03.13".to_string(),
                    batch_size: 10,
                    rate_limit_ms: None,
                    provider_options: no_key,
                })
                .unwrap()
                .delay_ms,
            2_400
        );
        assert_eq!(
            provider
                .rate_limit(&ProviderConfig {
                    seed_column: "cusip".to_string(),
                    version: "2026.03.13".to_string(),
                    batch_size: 100,
                    rate_limit_ms: None,
                    provider_options: with_key,
                })
                .unwrap()
                .delay_ms,
            240
        );
    }

    #[test]
    fn openfigi_id_type_infers_supported_seed_columns_and_accepts_explicit_override() {
        let mut config = openfigi_config("http://127.0.0.1:1/v3/mapping");

        config.seed_column = "isin".to_string();
        assert_eq!(openfigi_id_type(&config).unwrap(), "ID_ISIN");

        config.seed_column = "sedol".to_string();
        assert_eq!(openfigi_id_type(&config).unwrap(), "ID_SEDOL");

        config
            .provider_options
            .insert("id_type".to_string(), "id_cusip".to_string());
        assert_eq!(openfigi_id_type(&config).unwrap(), "ID_CUSIP");

        config
            .provider_options
            .insert("id_type".to_string(), "ID_BOGUS".to_string());
        assert!(
            openfigi_id_type(&config)
                .unwrap_err()
                .to_string()
                .contains("Unsupported OpenFIGI id_type")
        );
    }

    #[test]
    fn openfigi_schema_matches_implementation_constants() {
        let schema = OpenFigiProvider.schema();
        assert_eq!(schema.id, "openfigi");

        // id_type enum and inference are driven by the same constants the
        // resolver uses, so the published schema cannot drift from validation.
        assert_eq!(schema.id_types, OPENFIGI_ID_TYPES);
        let id_type_option = schema
            .options
            .iter()
            .find(|option| option.key == "id_type")
            .expect("id_type option present");
        assert_eq!(id_type_option.value_type, "enum");
        assert_eq!(id_type_option.enum_values, OPENFIGI_ID_TYPES);

        // api_key is the only secret and carries its env fallback.
        let api_key = schema
            .options
            .iter()
            .find(|option| option.key.as_str().eq("api_key"))
            .expect("api_key option present");
        assert!(api_key.secret);
        assert_eq!(api_key.env_fallback.as_deref(), Some("OPENFIGI_API_KEY"));
        assert_eq!(
            schema.options.iter().filter(|option| option.secret).count(),
            1,
            "api_key must be the only secret option"
        );

        // base_url default tracks the live endpoint constant.
        let base_url = schema
            .options
            .iter()
            .find(|option| option.key == "base_url")
            .expect("base_url option present");
        assert_eq!(base_url.default.as_deref(), Some(OPENFIGI_API_URL));
        assert!(!base_url.secret);

        // Every filter constant is published exactly once with the right type,
        // and the schema introduces no filter keys the resolver does not honor.
        for key in OPENFIGI_STRING_FILTERS {
            assert_eq!(option_type(&schema, key), Some("string"), "{key}");
        }
        for key in OPENFIGI_BOOL_FILTERS {
            assert_eq!(option_type(&schema, key), Some("bool"), "{key}");
        }
        for key in OPENFIGI_NUMERIC_INTERVAL_FILTERS {
            assert_eq!(option_type(&schema, key), Some("numeric_interval"), "{key}");
        }
        for key in OPENFIGI_DATE_INTERVAL_FILTERS {
            assert_eq!(option_type(&schema, key), Some("date_interval"), "{key}");
        }

        let mut published_filters = schema
            .options
            .iter()
            .map(|option| option.key.as_str())
            .filter(|key| !matches!(*key, "id_type" | "base_url" | "api_key"))
            .collect::<Vec<_>>();
        published_filters.sort_unstable();
        let mut expected_filters = OPENFIGI_STRING_FILTERS
            .iter()
            .chain(OPENFIGI_BOOL_FILTERS.iter())
            .chain(OPENFIGI_NUMERIC_INTERVAL_FILTERS.iter())
            .chain(OPENFIGI_DATE_INTERVAL_FILTERS.iter())
            .copied()
            .collect::<Vec<_>>();
        expected_filters.sort_unstable();
        assert_eq!(published_filters, expected_filters);

        // The exchCode/micCode mutual exclusion the resolver enforces is published.
        assert!(
            schema
                .mutual_exclusions
                .iter()
                .any(|pair| pair.contains(&"exchCode".to_string())
                    && pair.contains(&"micCode".to_string()))
        );
        assert!(schema.interval_encoding.is_some());
    }

    #[test]
    fn mock_schema_has_no_provider_options() {
        let schema = MockProvider.schema();
        assert_eq!(schema.id, "mock");
        assert!(schema.options.is_empty());
        assert!(schema.id_types.is_empty());
    }

    fn option_type<'a>(schema: &'a ProviderSchema, key: &str) -> Option<&'a str> {
        schema
            .options
            .iter()
            .find(|option| option.key == key)
            .map(|option| option.value_type.as_str())
    }
}
