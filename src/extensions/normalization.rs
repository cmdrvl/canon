#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub fn normalization_bundle_schema_version() -> &'static str {
    concat!("canon.normalization.bundle", ".v1")
}

pub type NormalizationResult<T> = Result<T, NormalizationError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationErrorCode {
    ArtifactContract,
    MissingView,
    MissingProtectedFeature,
    UnsupportedPrimitive,
    UnsafeExtensionPrimitive,
    ResourceLimitExceeded,
    CompatibilityPolicy,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationError {
    pub code: NormalizationErrorCode,
    pub message: String,
}

impl NormalizationError {
    pub fn new(code: NormalizationErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for NormalizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for NormalizationError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationBundle {
    pub version: String,
    pub package_id: String,
    pub package_version: String,
    pub max_input_bytes: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protected_features: Vec<ProtectedFeatureDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<NormalizationViewDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProtectedFeatureDefinition {
    pub feature_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationViewKind {
    String,
    Tokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationConsumerMode {
    Cluster,
    Link,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationViewDefinition {
    pub view_id: String,
    pub output_kind: NormalizationViewKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumer_modes: Vec<NormalizationConsumerMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protected_feature_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<NormalizationStepDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationStepDefinition {
    pub rule_id: String,
    pub primitive: NormalizationPrimitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerVerificationMode {
    ReadOnlyVerify,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "adapter", rename_all = "snake_case")]
pub enum SafeRunnerAdapter {
    LiteralReplace {
        from: String,
        to: String,
    },
    DropLiteralTokens {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tokens: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RunnerExtensionPrimitive {
    pub extension_id: String,
    pub package_digest: String,
    pub verification_mode: RunnerVerificationMode,
    pub deterministic: bool,
    pub allows_network: bool,
    pub writes_files: bool,
    pub max_input_bytes: usize,
    pub adapter: SafeRunnerAdapter,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizationPrimitive {
    AsciiTrim,
    CollapseWhitespace,
    LowercaseAscii,
    LatinAsciiFold,
    PunctuationToSpace,
    DropLiteralTokens {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tokens: Vec<String>,
    },
    SortTokens,
    DedupeTokens,
    RunnerExtension {
        extension: RunnerExtensionPrimitive,
    },
}

impl NormalizationPrimitive {
    pub const fn primitive_id(&self) -> &'static str {
        match self {
            Self::AsciiTrim => "ascii_trim",
            Self::CollapseWhitespace => "collapse_whitespace",
            Self::LowercaseAscii => "lowercase_ascii",
            Self::LatinAsciiFold => "latin_ascii_fold",
            Self::PunctuationToSpace => "punctuation_to_space",
            Self::DropLiteralTokens { .. } => "drop_literal_tokens",
            Self::SortTokens => "sort_tokens",
            Self::DedupeTokens => "dedupe_tokens",
            Self::RunnerExtension { .. } => "runner_extension",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationObservationInput {
    pub observation_id: String,
    pub raw_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationTraceStep {
    pub rule_id: String,
    pub primitive_id: String,
    pub input: String,
    pub output: String,
    pub lossy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProtectedFeatureObservation {
    pub feature_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_tokens: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub view_tokens: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped_tokens: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedView {
    pub view_id: String,
    pub output_kind: NormalizationViewKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumer_modes: Vec<NormalizationConsumerMode>,
    pub rendered_value: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<String>,
    pub lossy: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protected_features: Vec<ProtectedFeatureObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<NormalizationTraceStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationOutput {
    pub bundle_version: String,
    pub bundle_digest: String,
    pub observation_id: String,
    pub raw_value: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<NormalizedView>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProtectedFeatureConflict {
    pub feature_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left_only_tokens: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub right_only_tokens: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationBundleCompatibility {
    ExactDigest,
    CompatibleSameMajor,
}

pub fn finalize_bundle(
    mut bundle: NormalizationBundle,
) -> NormalizationResult<NormalizationBundle> {
    if bundle.version.trim().is_empty() {
        bundle.version = normalization_bundle_schema_version().to_string();
    }
    if bundle.version != normalization_bundle_schema_version() {
        return Err(artifact_contract_error(format!(
            "unsupported normalization bundle version: {}",
            bundle.version
        )));
    }

    bundle.package_id = normalized_package_id(&bundle.package_id, "package_id")?;
    bundle.package_version = normalized_semver(&bundle.package_version, "package_version")?;
    if bundle.max_input_bytes == 0 {
        return Err(artifact_contract_error(
            "max_input_bytes must be greater than zero",
        ));
    }

    bundle.protected_features = dedupe_components(
        bundle
            .protected_features
            .into_iter()
            .map(normalize_protected_feature)
            .collect::<NormalizationResult<Vec<_>>>()?,
        |feature| feature.feature_id.clone(),
        "protected feature",
    )?;
    let feature_index = bundle
        .protected_features
        .iter()
        .map(|feature| (feature.feature_id.clone(), feature))
        .collect::<BTreeMap<_, _>>();

    bundle.views = dedupe_components(
        bundle
            .views
            .into_iter()
            .map(|view| normalize_view(view, &feature_index, bundle.max_input_bytes))
            .collect::<NormalizationResult<Vec<_>>>()?,
        |view| view.view_id.clone(),
        "view",
    )?;
    if bundle.views.is_empty() {
        return Err(artifact_contract_error(
            "normalization bundle must declare at least one named view",
        ));
    }

    Ok(bundle)
}

pub fn canonical_bundle_bytes(bundle: &NormalizationBundle) -> NormalizationResult<Vec<u8>> {
    let bundle = finalize_bundle(bundle.clone())?;
    serde_json::to_vec(&bundle).map_err(|error| {
        artifact_contract_error(format!("failed to serialize normalization bundle: {error}"))
    })
}

pub fn bundle_digest(bundle: &NormalizationBundle) -> NormalizationResult<String> {
    let bytes = canonical_bundle_bytes(bundle)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn apply_bundle(
    bundle: &NormalizationBundle,
    input: &NormalizationObservationInput,
) -> NormalizationResult<NormalizationOutput> {
    let bundle = finalize_bundle(bundle.clone())?;
    let observation_id = normalized_non_empty(&input.observation_id, "observation_id")?;
    if input.raw_value.len() > bundle.max_input_bytes {
        return Err(resource_limit_error(format!(
            "raw_value exceeded max_input_bytes {}",
            bundle.max_input_bytes
        )));
    }

    let raw_token_index = observable_tokens(&input.raw_value);
    let mut views = bundle
        .views
        .iter()
        .map(|view| {
            apply_view(
                view,
                &bundle.protected_features,
                &raw_token_index,
                &input.raw_value,
            )
        })
        .collect::<NormalizationResult<Vec<_>>>()?;
    views.sort_by(|left, right| left.view_id.cmp(&right.view_id));

    let bundle_digest = bundle_digest(&bundle)?;
    Ok(NormalizationOutput {
        bundle_version: bundle.version,
        bundle_digest,
        observation_id,
        raw_value: input.raw_value.clone(),
        views,
    })
}

pub fn views_for_mode(
    output: &NormalizationOutput,
    mode: NormalizationConsumerMode,
) -> Vec<NormalizedView> {
    output
        .views
        .iter()
        .filter(|view| view.consumer_modes.contains(&mode))
        .cloned()
        .collect()
}

pub fn compare_protected_features(
    left: &NormalizedView,
    right: &NormalizedView,
) -> Vec<ProtectedFeatureConflict> {
    let left_index = left
        .protected_features
        .iter()
        .map(|feature| (feature.feature_id.clone(), feature))
        .collect::<BTreeMap<_, _>>();
    let right_index = right
        .protected_features
        .iter()
        .map(|feature| (feature.feature_id.clone(), feature))
        .collect::<BTreeMap<_, _>>();

    let feature_ids = left_index
        .keys()
        .chain(right_index.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    feature_ids
        .into_iter()
        .filter_map(|feature_id| {
            let left_tokens = left_index
                .get(&feature_id)
                .map(|feature| feature.view_tokens.iter().cloned().collect::<BTreeSet<_>>())
                .unwrap_or_default();
            let right_tokens = right_index
                .get(&feature_id)
                .map(|feature| feature.view_tokens.iter().cloned().collect::<BTreeSet<_>>())
                .unwrap_or_default();
            let left_only = left_tokens
                .difference(&right_tokens)
                .cloned()
                .collect::<Vec<_>>();
            let right_only = right_tokens
                .difference(&left_tokens)
                .cloned()
                .collect::<Vec<_>>();
            (!left_only.is_empty() || !right_only.is_empty()).then_some(ProtectedFeatureConflict {
                feature_id,
                left_only_tokens: left_only,
                right_only_tokens: right_only,
            })
        })
        .collect()
}

pub fn bundle_compatibility(
    locked: &NormalizationBundle,
    candidate: &NormalizationBundle,
    required_views: &[&str],
) -> NormalizationResult<NormalizationBundleCompatibility> {
    let locked = finalize_bundle(locked.clone())?;
    let candidate = finalize_bundle(candidate.clone())?;

    if locked.package_id != candidate.package_id {
        return Err(compatibility_error(format!(
            "bundle package ids differ: {} vs {}",
            locked.package_id, candidate.package_id
        )));
    }

    if bundle_digest(&locked)? == bundle_digest(&candidate)? {
        return Ok(NormalizationBundleCompatibility::ExactDigest);
    }

    if semver_major(&locked.package_version)? != semver_major(&candidate.package_version)? {
        return Err(compatibility_error(format!(
            "bundle {} changed major version from {} to {}",
            locked.package_id,
            semver_major(&locked.package_version)?,
            semver_major(&candidate.package_version)?
        )));
    }

    let locked_views = locked
        .views
        .iter()
        .map(|view| (view.view_id.as_str(), view))
        .collect::<BTreeMap<_, _>>();
    let candidate_views = candidate
        .views
        .iter()
        .map(|view| (view.view_id.as_str(), view))
        .collect::<BTreeMap<_, _>>();

    for required_view in required_views {
        let Some(locked_view) = locked_views.get(required_view) else {
            return Err(compatibility_error(format!(
                "locked bundle does not declare required view {}",
                required_view
            )));
        };
        let Some(candidate_view) = candidate_views.get(required_view) else {
            return Err(compatibility_error(format!(
                "candidate bundle does not declare required view {}",
                required_view
            )));
        };
        if locked_view.output_kind != candidate_view.output_kind {
            return Err(compatibility_error(format!(
                "required view {} changed output kind from {:?} to {:?}",
                required_view, locked_view.output_kind, candidate_view.output_kind
            )));
        }
        if locked_view.consumer_modes != candidate_view.consumer_modes {
            return Err(compatibility_error(format!(
                "required view {} changed consumer modes",
                required_view
            )));
        }
    }

    Ok(NormalizationBundleCompatibility::CompatibleSameMajor)
}

fn normalize_protected_feature(
    mut feature: ProtectedFeatureDefinition,
) -> NormalizationResult<ProtectedFeatureDefinition> {
    feature.feature_id = normalized_component_id(&feature.feature_id, "feature_id")?;
    feature.tokens = normalize_literal_tokens(feature.tokens, "protected_feature.tokens")?;
    if feature.tokens.is_empty() {
        return Err(artifact_contract_error(
            "protected feature must declare at least one token",
        ));
    }
    Ok(feature)
}

fn normalize_view(
    mut view: NormalizationViewDefinition,
    feature_index: &BTreeMap<String, &ProtectedFeatureDefinition>,
    bundle_limit: usize,
) -> NormalizationResult<NormalizationViewDefinition> {
    view.view_id = normalized_component_id(&view.view_id, "view_id")?;
    if view.consumer_modes.is_empty() {
        return Err(artifact_contract_error(format!(
            "view {} must declare at least one consumer mode",
            view.view_id
        )));
    }
    view.consumer_modes.sort();
    view.consumer_modes.dedup();
    view.protected_feature_refs =
        normalize_string_vec(view.protected_feature_refs, "protected_feature_refs")?;
    for feature_ref in &view.protected_feature_refs {
        if !feature_index.contains_key(feature_ref) {
            return Err(missing_protected_feature_error(format!(
                "view {} references unknown protected feature {}",
                view.view_id, feature_ref
            )));
        }
    }
    if view.steps.is_empty() {
        return Err(artifact_contract_error(format!(
            "view {} must declare at least one transformation step",
            view.view_id
        )));
    }

    let mut seen_rule_ids = BTreeSet::new();
    for step in &mut view.steps {
        step.rule_id = normalized_component_id(&step.rule_id, "rule_id")?;
        if !seen_rule_ids.insert(step.rule_id.clone()) {
            return Err(artifact_contract_error(format!(
                "view {} contains duplicate rule_id {}",
                view.view_id, step.rule_id
            )));
        }
        normalize_primitive(&mut step.primitive, bundle_limit)?;
    }
    Ok(view)
}

fn normalize_primitive(
    primitive: &mut NormalizationPrimitive,
    bundle_limit: usize,
) -> NormalizationResult<()> {
    match primitive {
        NormalizationPrimitive::DropLiteralTokens { tokens } => {
            *tokens =
                normalize_literal_tokens(std::mem::take(tokens), "drop_literal_tokens.tokens")?;
            if tokens.is_empty() {
                return Err(artifact_contract_error(
                    "drop_literal_tokens must declare at least one token",
                ));
            }
        }
        NormalizationPrimitive::RunnerExtension { extension } => {
            extension.extension_id =
                normalized_component_id(&extension.extension_id, "extension_id")?;
            extension.package_digest =
                normalized_hash(&extension.package_digest, "extension.package_digest")?;
            if extension.max_input_bytes == 0 {
                return Err(artifact_contract_error(
                    "runner extension max_input_bytes must be greater than zero",
                ));
            }
            if extension.max_input_bytes > bundle_limit {
                return Err(unsafe_extension_error(format!(
                    "runner extension {} exceeds bundle max_input_bytes {}",
                    extension.extension_id, bundle_limit
                )));
            }
            if !extension.deterministic
                || extension.allows_network
                || extension.writes_files
                || extension.verification_mode != RunnerVerificationMode::ReadOnlyVerify
            {
                return Err(unsafe_extension_error(format!(
                    "runner extension {} must be deterministic, read-only, offline, and side-effect free",
                    extension.extension_id
                )));
            }
            match &mut extension.adapter {
                SafeRunnerAdapter::LiteralReplace { from, to } => {
                    *from = normalized_non_empty(from, "extension.adapter.from")?;
                    if from.len() > bundle_limit || to.len() > bundle_limit {
                        return Err(unsafe_extension_error(format!(
                            "runner extension {} adapter literals exceed bundle input budget",
                            extension.extension_id
                        )));
                    }
                }
                SafeRunnerAdapter::DropLiteralTokens { tokens } => {
                    *tokens = normalize_literal_tokens(
                        std::mem::take(tokens),
                        "extension.adapter.tokens",
                    )?;
                    if tokens.is_empty() {
                        return Err(artifact_contract_error(
                            "runner extension drop_literal_tokens must declare at least one token",
                        ));
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn apply_view(
    view: &NormalizationViewDefinition,
    protected_features: &[ProtectedFeatureDefinition],
    raw_token_index: &[String],
    raw_value: &str,
) -> NormalizationResult<NormalizedView> {
    let mut working = raw_value.to_string();
    let mut trace = Vec::with_capacity(view.steps.len());
    for step in &view.steps {
        let before = working.clone();
        working = apply_primitive(&before, &step.primitive)?;
        trace.push(NormalizationTraceStep {
            rule_id: step.rule_id.clone(),
            primitive_id: step.primitive.primitive_id().to_string(),
            lossy: before != working,
            input: before,
            output: working.clone(),
        });
    }

    let tokens = tokens_from_view_value(&working);
    let protected_features = view
        .protected_feature_refs
        .iter()
        .map(|feature_ref| {
            let feature = protected_features
                .iter()
                .find(|candidate| candidate.feature_id == *feature_ref)
                .ok_or_else(|| {
                    missing_protected_feature_error(format!(
                        "unknown protected feature {}",
                        feature_ref
                    ))
                })?;
            observe_protected_feature(feature, raw_token_index, &tokens)
        })
        .collect::<NormalizationResult<Vec<_>>>()?;

    Ok(NormalizedView {
        view_id: view.view_id.clone(),
        output_kind: view.output_kind,
        consumer_modes: view.consumer_modes.clone(),
        rendered_value: working,
        lossy: trace.iter().any(|step| step.lossy),
        tokens,
        protected_features,
        trace,
    })
}

fn apply_primitive(input: &str, primitive: &NormalizationPrimitive) -> NormalizationResult<String> {
    match primitive {
        NormalizationPrimitive::AsciiTrim => Ok(input
            .trim_matches(|character: char| character.is_ascii_whitespace())
            .to_string()),
        NormalizationPrimitive::CollapseWhitespace => {
            Ok(input.split_whitespace().collect::<Vec<_>>().join(" "))
        }
        NormalizationPrimitive::LowercaseAscii => Ok(input
            .chars()
            .map(|character| {
                if character.is_ascii() {
                    character.to_ascii_lowercase()
                } else {
                    character
                }
            })
            .collect()),
        NormalizationPrimitive::LatinAsciiFold => Ok(input
            .chars()
            .flat_map(|character| fold_character(character).chars().collect::<Vec<_>>())
            .collect()),
        NormalizationPrimitive::PunctuationToSpace => Ok(input
            .chars()
            .map(|character| {
                if is_punctuation_or_symbol(character) {
                    ' '
                } else {
                    character
                }
            })
            .collect()),
        NormalizationPrimitive::DropLiteralTokens { tokens } => Ok(input
            .split_whitespace()
            .filter(|token| !tokens.iter().any(|candidate| candidate == token))
            .collect::<Vec<_>>()
            .join(" ")),
        NormalizationPrimitive::SortTokens => {
            let mut tokens = tokens_from_view_value(input);
            tokens.sort();
            Ok(tokens.join(" "))
        }
        NormalizationPrimitive::DedupeTokens => {
            let mut seen = BTreeSet::new();
            Ok(tokens_from_view_value(input)
                .into_iter()
                .filter(|token| seen.insert(token.clone()))
                .collect::<Vec<_>>()
                .join(" "))
        }
        NormalizationPrimitive::RunnerExtension { extension } => {
            if input.len() > extension.max_input_bytes {
                return Err(resource_limit_error(format!(
                    "runner extension {} exceeded max_input_bytes {}",
                    extension.extension_id, extension.max_input_bytes
                )));
            }
            match &extension.adapter {
                SafeRunnerAdapter::LiteralReplace { from, to } => Ok(input.replace(from, to)),
                SafeRunnerAdapter::DropLiteralTokens { tokens } => Ok(input
                    .split_whitespace()
                    .filter(|token| !tokens.iter().any(|candidate| candidate == token))
                    .collect::<Vec<_>>()
                    .join(" ")),
            }
        }
    }
}

fn observe_protected_feature(
    feature: &ProtectedFeatureDefinition,
    raw_token_index: &[String],
    view_tokens: &[String],
) -> NormalizationResult<ProtectedFeatureObservation> {
    let feature_token_set = feature.tokens.iter().cloned().collect::<BTreeSet<_>>();
    let raw_tokens = raw_token_index
        .iter()
        .filter(|token| feature_token_set.contains(*token))
        .cloned()
        .collect::<BTreeSet<_>>();
    let view_tokens_set = view_tokens
        .iter()
        .filter(|token| feature_token_set.contains(*token))
        .cloned()
        .collect::<BTreeSet<_>>();
    let dropped_tokens = raw_tokens
        .difference(&view_tokens_set)
        .cloned()
        .collect::<Vec<_>>();

    Ok(ProtectedFeatureObservation {
        feature_id: feature.feature_id.clone(),
        raw_tokens: raw_tokens.into_iter().collect(),
        view_tokens: view_tokens_set.into_iter().collect(),
        dropped_tokens,
    })
}

fn observable_tokens(raw_value: &str) -> Vec<String> {
    tokens_from_view_value(
        &raw_value
            .chars()
            .flat_map(|character| {
                if is_punctuation_or_symbol(character) || character.is_whitespace() {
                    vec![' ']
                } else {
                    fold_character(character).chars().collect::<Vec<_>>()
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn tokens_from_view_value(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_literal_tokens(tokens: Vec<String>, field: &str) -> NormalizationResult<Vec<String>> {
    let mut normalized = tokens
        .into_iter()
        .map(|token| {
            let token = normalized_non_empty(&token, field)?;
            if !token.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            }) {
                return Err(artifact_contract_error(format!(
                    "{field} tokens must use lowercase ASCII letters, digits, or underscores"
                )));
            }
            Ok(token)
        })
        .collect::<NormalizationResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalize_string_vec(values: Vec<String>, field: &str) -> NormalizationResult<Vec<String>> {
    let mut values = values
        .into_iter()
        .map(|value| normalized_non_empty(&value, field))
        .collect::<NormalizationResult<Vec<_>>>()?;
    values.sort();
    values.dedup();
    Ok(values)
}

fn normalized_package_id(value: &str, field: &str) -> NormalizationResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '_' | '-')
    }) && value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must match ^[a-z0-9][a-z0-9._-]*$"
    )))
}

fn normalized_component_id(value: &str, field: &str) -> NormalizationResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '_' | '-' | ':')
    }) && value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must use lowercase ASCII component ids"
    )))
}

fn normalized_semver(value: &str, field: &str) -> NormalizationResult<String> {
    let value = normalized_non_empty(value, field)?;
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must match ^[0-9]+\\.[0-9]+\\.[0-9]+$"
    )))
}

fn semver_major(value: &str) -> NormalizationResult<u64> {
    value
        .split('.')
        .next()
        .ok_or_else(|| artifact_contract_error("missing semver major component"))?
        .parse::<u64>()
        .map_err(|error| artifact_contract_error(format!("invalid semver major: {error}")))
}

fn normalized_hash(value: &str, field: &str) -> NormalizationResult<String> {
    let value = normalized_non_empty(value, field)?;
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must start with blake3:"
        )));
    };
    if hex.len() == 64
        && hex
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must match ^blake3:[0-9a-f]{{64}}$"
    )))
}

fn normalized_non_empty(value: &str, field: &str) -> NormalizationResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must not be empty"
        )));
    }
    Ok(value)
}

fn dedupe_components<T, F>(
    mut components: Vec<T>,
    key: F,
    label: &str,
) -> NormalizationResult<Vec<T>>
where
    T: Clone + PartialEq,
    F: Fn(&T) -> String,
{
    components.sort_by_key(|component| key(component));
    let mut deduped = Vec::with_capacity(components.len());
    for component in components {
        if let Some(previous) = deduped.last()
            && key(previous) == key(&component)
        {
            if previous != &component {
                return Err(artifact_contract_error(format!(
                    "{label} {} cannot be declared with conflicting content",
                    key(&component)
                )));
            }
            continue;
        }
        deduped.push(component);
    }
    Ok(deduped)
}

fn artifact_contract_error(message: impl Into<String>) -> NormalizationError {
    NormalizationError::new(NormalizationErrorCode::ArtifactContract, message)
}

fn missing_protected_feature_error(message: impl Into<String>) -> NormalizationError {
    NormalizationError::new(NormalizationErrorCode::MissingProtectedFeature, message)
}

fn resource_limit_error(message: impl Into<String>) -> NormalizationError {
    NormalizationError::new(NormalizationErrorCode::ResourceLimitExceeded, message)
}

fn unsafe_extension_error(message: impl Into<String>) -> NormalizationError {
    NormalizationError::new(NormalizationErrorCode::UnsafeExtensionPrimitive, message)
}

fn compatibility_error(message: impl Into<String>) -> NormalizationError {
    NormalizationError::new(NormalizationErrorCode::CompatibilityPolicy, message)
}

fn is_punctuation_or_symbol(character: char) -> bool {
    character.is_ascii_punctuation()
        || matches!(
            character,
            '¡' | '¿'
                | '§'
                | '¶'
                | '·'
                | '‐'
                | '‑'
                | '‒'
                | '–'
                | '—'
                | '―'
                | '‘'
                | '’'
                | '‚'
                | '“'
                | '”'
                | '„'
                | '†'
                | '‡'
                | '•'
                | '…'
                | '‰'
                | '′'
                | '″'
                | '‹'
                | '›'
                | '€'
                | '™'
        )
}

fn fold_character(character: char) -> String {
    match character {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å'
        | 'ā' | 'ă' | 'ą' => "a".to_string(),
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' | 'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c".to_string(),
        'Ð' | 'Ď' | 'Đ' | 'ð' | 'ď' | 'đ' => "d".to_string(),
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' | 'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ'
        | 'ė' | 'ę' | 'ě' => "e".to_string(),
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' | 'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g".to_string(),
        'Ĥ' | 'Ħ' | 'ĥ' | 'ħ' => "h".to_string(),
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' | 'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī'
        | 'ĭ' | 'į' | 'ı' => "i".to_string(),
        'Ĵ' | 'ĵ' => "j".to_string(),
        'Ķ' | 'ķ' | 'ĸ' => "k".to_string(),
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' | 'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l".to_string(),
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' | 'ñ' | 'ń' | 'ņ' | 'ň' => "n".to_string(),
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø'
        | 'ō' | 'ŏ' | 'ő' => "o".to_string(),
        'Ŕ' | 'Ŗ' | 'Ř' | 'ŕ' | 'ŗ' | 'ř' => "r".to_string(),
        'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'ś' | 'ŝ' | 'ş' | 'š' => "s".to_string(),
        'ß' => "ss".to_string(),
        'Ţ' | 'Ť' | 'Ŧ' | 'ţ' | 'ť' | 'ŧ' => "t".to_string(),
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' | 'ù' | 'ú' | 'û' | 'ü' | 'ũ'
        | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u".to_string(),
        'Ŵ' | 'ŵ' => "w".to_string(),
        'Ý' | 'Ÿ' | 'Ŷ' | 'ý' | 'ÿ' | 'ŷ' => "y".to_string(),
        'Ź' | 'Ż' | 'Ž' | 'ź' | 'ż' | 'ž' => "z".to_string(),
        'Æ' | 'Ǽ' | 'æ' | 'ǽ' => "ae".to_string(),
        'Œ' | 'œ' => "oe".to_string(),
        _ => character.to_lowercase().collect(),
    }
}
