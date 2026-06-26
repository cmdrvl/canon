#![forbid(unsafe_code)]

//! Redaction-safe benchmark telemetry for entity performance runs.
//!
//! This module is data-only: benchmark and stress harnesses can construct this
//! artifact without pulling in stage implementations. Wall-clock targets are
//! calibrated elsewhere; this schema records the context required to compare
//! those runs without exposing source rows.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, env, fmt, thread, time::Duration};

pub const CANON_ENTITY_BENCHMARK_TELEMETRY_VERSION: &str = "canon_entity_benchmark_telemetry.v0";

pub const REQUIRED_TELEMETRY_FIELDS: &[&str] = &[
    "run_id",
    "suite_id",
    "profile",
    "canon_version",
    "git_sha",
    "rust_profile",
    "target_triple",
    "os",
    "cpu_model",
    "logical_cores",
    "memory_bytes",
    "cache_state",
    "input_hash",
    "profile_hash",
    "strategy_hash",
    "registry_snapshot_hash",
    "patch_hash",
    "holdout_id",
    "metamorphic_relation_id",
    "raw_row_count",
    "raw_observation_count",
    "raw_unique_surface_count",
    "prepared_surface_count",
    "exact_resolved_surface_count",
    "candidate_pair_count",
    "candidate_pairs_per_surface_p50",
    "candidate_pairs_per_surface_p95",
    "candidate_pairs_per_surface_p99",
    "suppressed_candidate_count",
    "exact_bucket_count",
    "exact_bucket_pair_expansion_count",
    "largest_exact_bucket_size",
    "largest_component_size",
    "edge_count",
    "review_group_count",
    "artifact_bytes_by_stage",
    "timings_ms_by_stage",
    "peak_memory_bytes",
    "peak_memory_method",
    "registry_pre_mutation_hash",
    "registry_post_mutation_hash",
    "runtime_guard_status",
    "refusal_code",
    "next_command",
];

pub const REQUIRED_TELEMETRY_STAGE_IDS: &[&str] = &[
    "prepare", "index", "block", "edge", "solve", "audit", "promote", "apply",
];

pub const FORBIDDEN_TELEMETRY_PAYLOAD_KEYS: &[&str] =
    &["raw_rows", "source_rows", "operator_notes", "private_notes"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityTelemetryMachine {
    pub canon_version: String,
    pub git_sha: String,
    pub rust_profile: String,
    pub target_triple: String,
    pub os: String,
    pub cpu_model: String,
    pub logical_cores: u64,
    pub memory_bytes: u64,
}

impl EntityTelemetryMachine {
    pub fn detect_redacted() -> Self {
        Self {
            canon_version: env!("CARGO_PKG_VERSION").to_string(),
            git_sha: option_env!("VERGEN_GIT_SHA")
                .or(option_env!("GIT_SHA"))
                .unwrap_or("unknown")
                .to_string(),
            rust_profile: if cfg!(debug_assertions) {
                "debug".to_string()
            } else {
                "release".to_string()
            },
            target_triple: format!("{}-{}", env::consts::ARCH, env::consts::OS),
            os: env::consts::OS.to_string(),
            cpu_model: env::var("CANON_ENTITY_CPU_MODEL")
                .unwrap_or_else(|_| "redacted".to_string()),
            logical_cores: thread::available_parallelism()
                .map(|cores| u64::try_from(cores.get()).unwrap_or(u64::MAX))
                .unwrap_or(1),
            memory_bytes: env::var("CANON_ENTITY_MEMORY_BYTES")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(1),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityTelemetryRunContext {
    pub run_id: String,
    pub suite_id: String,
    pub profile: String,
    pub machine: EntityTelemetryMachine,
    pub cache_state: String,
    pub input_hash: String,
    pub profile_hash: String,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
    pub patch_hash: String,
    pub holdout_id: String,
    pub metamorphic_relation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityTelemetryCounters {
    pub raw_row_count: u64,
    pub raw_observation_count: u64,
    pub raw_unique_surface_count: u64,
    pub prepared_surface_count: u64,
    pub exact_resolved_surface_count: u64,
    pub candidate_pair_count: u64,
    pub candidate_pairs_per_surface_p50: u64,
    pub candidate_pairs_per_surface_p95: u64,
    pub candidate_pairs_per_surface_p99: u64,
    pub suppressed_candidate_count: u64,
    pub exact_bucket_count: u64,
    pub exact_bucket_pair_expansion_count: u64,
    pub largest_exact_bucket_size: u64,
    pub largest_component_size: u64,
    pub edge_count: u64,
    pub review_group_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityTelemetryOutcome {
    pub peak_memory_bytes: u64,
    pub peak_memory_method: String,
    pub registry_pre_mutation_hash: String,
    pub registry_post_mutation_hash: String,
    pub runtime_guard_status: String,
    pub refusal_code: String,
    pub next_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityTelemetryHarness {
    context: EntityTelemetryRunContext,
    artifact_bytes_by_stage: BTreeMap<String, u64>,
    timings_ms_by_stage: BTreeMap<String, u64>,
}

impl EntityTelemetryHarness {
    pub fn new(context: EntityTelemetryRunContext) -> Self {
        Self {
            context,
            artifact_bytes_by_stage: zero_stage_map(),
            timings_ms_by_stage: zero_stage_map(),
        }
    }

    pub fn record_stage(
        &mut self,
        stage: &str,
        artifact_bytes: u64,
        elapsed: Duration,
    ) -> Result<(), EntityTelemetryValidationError> {
        let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        self.record_stage_ms(stage, artifact_bytes, elapsed_ms)
    }

    pub fn record_stage_ms(
        &mut self,
        stage: &str,
        artifact_bytes: u64,
        elapsed_ms: u64,
    ) -> Result<(), EntityTelemetryValidationError> {
        if !REQUIRED_TELEMETRY_STAGE_IDS.contains(&stage) {
            return Err(EntityTelemetryValidationError::new(
                "stage",
                format!("unsupported telemetry stage {stage}"),
            ));
        }
        self.artifact_bytes_by_stage
            .insert(stage.to_string(), artifact_bytes);
        self.timings_ms_by_stage
            .insert(stage.to_string(), elapsed_ms);
        Ok(())
    }

    pub fn finish(
        self,
        counters: EntityTelemetryCounters,
        outcome: EntityTelemetryOutcome,
    ) -> Result<EntityBenchmarkTelemetry, EntityTelemetryValidationError> {
        let telemetry = EntityBenchmarkTelemetry {
            schema_version: CANON_ENTITY_BENCHMARK_TELEMETRY_VERSION.to_string(),
            run_id: self.context.run_id,
            suite_id: self.context.suite_id,
            profile: self.context.profile,
            canon_version: self.context.machine.canon_version,
            git_sha: self.context.machine.git_sha,
            rust_profile: self.context.machine.rust_profile,
            target_triple: self.context.machine.target_triple,
            os: self.context.machine.os,
            cpu_model: self.context.machine.cpu_model,
            logical_cores: self.context.machine.logical_cores,
            memory_bytes: self.context.machine.memory_bytes,
            cache_state: self.context.cache_state,
            input_hash: self.context.input_hash,
            profile_hash: self.context.profile_hash,
            strategy_hash: self.context.strategy_hash,
            registry_snapshot_hash: self.context.registry_snapshot_hash,
            patch_hash: self.context.patch_hash,
            holdout_id: self.context.holdout_id,
            metamorphic_relation_id: self.context.metamorphic_relation_id,
            raw_row_count: counters.raw_row_count,
            raw_observation_count: counters.raw_observation_count,
            raw_unique_surface_count: counters.raw_unique_surface_count,
            prepared_surface_count: counters.prepared_surface_count,
            exact_resolved_surface_count: counters.exact_resolved_surface_count,
            candidate_pair_count: counters.candidate_pair_count,
            candidate_pairs_per_surface_p50: counters.candidate_pairs_per_surface_p50,
            candidate_pairs_per_surface_p95: counters.candidate_pairs_per_surface_p95,
            candidate_pairs_per_surface_p99: counters.candidate_pairs_per_surface_p99,
            suppressed_candidate_count: counters.suppressed_candidate_count,
            exact_bucket_count: counters.exact_bucket_count,
            exact_bucket_pair_expansion_count: counters.exact_bucket_pair_expansion_count,
            largest_exact_bucket_size: counters.largest_exact_bucket_size,
            largest_component_size: counters.largest_component_size,
            edge_count: counters.edge_count,
            review_group_count: counters.review_group_count,
            artifact_bytes_by_stage: self.artifact_bytes_by_stage,
            timings_ms_by_stage: self.timings_ms_by_stage,
            peak_memory_bytes: outcome.peak_memory_bytes,
            peak_memory_method: outcome.peak_memory_method,
            registry_pre_mutation_hash: outcome.registry_pre_mutation_hash,
            registry_post_mutation_hash: outcome.registry_post_mutation_hash,
            runtime_guard_status: outcome.runtime_guard_status,
            refusal_code: outcome.refusal_code,
            next_command: outcome.next_command,
        };
        telemetry.validate()?;
        Ok(telemetry)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityBenchmarkTelemetry {
    pub schema_version: String,
    pub run_id: String,
    pub suite_id: String,
    pub profile: String,
    pub canon_version: String,
    pub git_sha: String,
    pub rust_profile: String,
    pub target_triple: String,
    pub os: String,
    pub cpu_model: String,
    pub logical_cores: u64,
    pub memory_bytes: u64,
    pub cache_state: String,
    pub input_hash: String,
    pub profile_hash: String,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
    pub patch_hash: String,
    pub holdout_id: String,
    pub metamorphic_relation_id: String,
    pub raw_row_count: u64,
    pub raw_observation_count: u64,
    pub raw_unique_surface_count: u64,
    pub prepared_surface_count: u64,
    pub exact_resolved_surface_count: u64,
    pub candidate_pair_count: u64,
    pub candidate_pairs_per_surface_p50: u64,
    pub candidate_pairs_per_surface_p95: u64,
    pub candidate_pairs_per_surface_p99: u64,
    pub suppressed_candidate_count: u64,
    pub exact_bucket_count: u64,
    pub exact_bucket_pair_expansion_count: u64,
    pub largest_exact_bucket_size: u64,
    pub largest_component_size: u64,
    pub edge_count: u64,
    pub review_group_count: u64,
    pub artifact_bytes_by_stage: BTreeMap<String, u64>,
    pub timings_ms_by_stage: BTreeMap<String, u64>,
    pub peak_memory_bytes: u64,
    pub peak_memory_method: String,
    pub registry_pre_mutation_hash: String,
    pub registry_post_mutation_hash: String,
    pub runtime_guard_status: String,
    pub refusal_code: String,
    pub next_command: String,
}

impl EntityBenchmarkTelemetry {
    pub fn validate(&self) -> Result<(), EntityTelemetryValidationError> {
        if self.schema_version != CANON_ENTITY_BENCHMARK_TELEMETRY_VERSION {
            return Err(EntityTelemetryValidationError::new(
                "schema_version",
                format!(
                    "expected {CANON_ENTITY_BENCHMARK_TELEMETRY_VERSION}, got {}",
                    self.schema_version
                ),
            ));
        }

        for (field, value) in self.required_non_empty_strings() {
            if value.trim().is_empty() {
                return Err(EntityTelemetryValidationError::new(
                    field,
                    "must not be empty",
                ));
            }
        }
        if self.logical_cores == 0 {
            return Err(EntityTelemetryValidationError::new(
                "logical_cores",
                "must be greater than zero",
            ));
        }
        if self.memory_bytes == 0 {
            return Err(EntityTelemetryValidationError::new(
                "memory_bytes",
                "must be greater than zero",
            ));
        }

        require_stage_map("artifact_bytes_by_stage", &self.artifact_bytes_by_stage)?;
        require_stage_map("timings_ms_by_stage", &self.timings_ms_by_stage)?;

        if self.candidate_pairs_per_surface_p95 < self.candidate_pairs_per_surface_p50 {
            return Err(EntityTelemetryValidationError::new(
                "candidate_pairs_per_surface_p95",
                "must be greater than or equal to p50",
            ));
        }
        if self.candidate_pairs_per_surface_p99 < self.candidate_pairs_per_surface_p95 {
            return Err(EntityTelemetryValidationError::new(
                "candidate_pairs_per_surface_p99",
                "must be greater than or equal to p95",
            ));
        }
        if self.exact_resolved_surface_count > self.prepared_surface_count {
            return Err(EntityTelemetryValidationError::new(
                "exact_resolved_surface_count",
                "must not exceed prepared_surface_count",
            ));
        }

        Ok(())
    }

    fn required_non_empty_strings(&self) -> [(&'static str, &str); 21] {
        [
            ("run_id", &self.run_id),
            ("suite_id", &self.suite_id),
            ("profile", &self.profile),
            ("canon_version", &self.canon_version),
            ("git_sha", &self.git_sha),
            ("rust_profile", &self.rust_profile),
            ("target_triple", &self.target_triple),
            ("os", &self.os),
            ("cpu_model", &self.cpu_model),
            ("cache_state", &self.cache_state),
            ("input_hash", &self.input_hash),
            ("profile_hash", &self.profile_hash),
            ("strategy_hash", &self.strategy_hash),
            ("registry_snapshot_hash", &self.registry_snapshot_hash),
            ("patch_hash", &self.patch_hash),
            ("holdout_id", &self.holdout_id),
            ("metamorphic_relation_id", &self.metamorphic_relation_id),
            ("peak_memory_method", &self.peak_memory_method),
            (
                "registry_pre_mutation_hash",
                &self.registry_pre_mutation_hash,
            ),
            (
                "registry_post_mutation_hash",
                &self.registry_post_mutation_hash,
            ),
            ("runtime_guard_status", &self.runtime_guard_status),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityTelemetryValidationError {
    pub field: String,
    pub message: String,
}

impl EntityTelemetryValidationError {
    fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for EntityTelemetryValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for EntityTelemetryValidationError {}

pub fn required_telemetry_fields() -> &'static [&'static str] {
    REQUIRED_TELEMETRY_FIELDS
}

pub fn required_telemetry_stage_ids() -> &'static [&'static str] {
    REQUIRED_TELEMETRY_STAGE_IDS
}

pub fn forbidden_telemetry_payload_keys() -> &'static [&'static str] {
    FORBIDDEN_TELEMETRY_PAYLOAD_KEYS
}

fn zero_stage_map() -> BTreeMap<String, u64> {
    REQUIRED_TELEMETRY_STAGE_IDS
        .iter()
        .map(|stage| ((*stage).to_string(), 0))
        .collect()
}

fn require_stage_map(
    field: &'static str,
    stages: &BTreeMap<String, u64>,
) -> Result<(), EntityTelemetryValidationError> {
    for required in REQUIRED_TELEMETRY_STAGE_IDS {
        if !stages.contains_key(*required) {
            return Err(EntityTelemetryValidationError::new(
                format!("{field}.{required}"),
                "required stage telemetry is missing",
            ));
        }
    }
    Ok(())
}
