use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error};

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
    pub message: String,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFetchResult {
    pub files: BTreeMap<String, Vec<RegistryMaterializedEntry>>,
    pub unresolved: Vec<String>,
    pub failures: Vec<ProviderFailure>,
    pub api_calls: usize,
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

    fn default_batch_size(&self) -> usize {
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
        _ => None,
    }
}

pub fn available_sources() -> Vec<&'static str> {
    vec!["mock"]
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
}
