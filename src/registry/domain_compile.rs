#![forbid(unsafe_code)]

//! Domain-neutral compiler for external fact packages into immutable registry
//! build artifacts.
//!
//! The compiler treats all ontology, namespace, vocabulary, relation, and
//! source identifiers as opaque strings. It only enforces deterministic package
//! mechanics: pinned inputs, exact alias emission, relation sidecars, conflict
//! reporting, proof/provenance inventories, restricted-data projection, and
//! byte-stable rebuilds.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_REGISTRY_BUILD_VERSION: &str = "canon.registry.build.v1";

pub type DomainRegistryBuildResult<T> = Result<T, DomainRegistryBuildError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DomainRegistryBuildErrorCode {
    ArtifactContract,
    CompatibilityPolicy,
    Conflict,
    RestrictedDataLeak,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainRegistryBuildError {
    pub code: DomainRegistryBuildErrorCode,
    pub message: String,
}

impl DomainRegistryBuildError {
    pub fn new(code: DomainRegistryBuildErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for DomainRegistryBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for DomainRegistryBuildError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainPackageKind {
    Ontology,
    Namespace,
    Vocabulary,
    Fact,
    Review,
    TrustConflict,
    Temporal,
    Projection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryBuildVisibility {
    PublicThin,
    LocalPrivate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseRedistribution {
    Public,
    Internal,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainConflictKind {
    Remap,
    AliasCollision,
    RelationshipCollision,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DomainPackagePin {
    pub package_kind: DomainPackageKind,
    pub package_id: String,
    pub package_version: String,
    pub package_digest: String,
    pub license_id: String,
    #[serde(default)]
    pub restricted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DomainLicenseRef {
    pub license_id: String,
    pub label: String,
    pub uri: String,
    pub redistribution: LicenseRedistribution,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DomainProvenanceRef {
    pub source_package_id: String,
    pub public_ref: String,
    pub license_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restricted_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DomainFact {
    pub fact_id: String,
    pub domain_id: String,
    pub canonical_id: String,
    pub alias: String,
    pub namespace_id: String,
    pub source_package_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_refs: Vec<String>,
    pub provenance: DomainProvenanceRef,
    #[serde(default)]
    pub restricted_value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DomainRelationshipFact {
    pub relationship_id: String,
    pub left_domain_id: String,
    pub right_domain_id: String,
    pub relation_type_id: String,
    pub source_package_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_refs: Vec<String>,
    pub provenance: DomainProvenanceRef,
    #[serde(default)]
    pub restricted_value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegistryBuildOptions {
    #[serde(default)]
    pub include_restricted_values: bool,
    pub restricted_manifest_policy: String,
}

impl Default for RegistryBuildOptions {
    fn default() -> Self {
        Self {
            include_restricted_values: false,
            restricted_manifest_policy: "public_thin_omits_restricted_values".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainRegistryBuildPackage {
    pub version: String,
    pub build_id: String,
    pub registry_id: String,
    pub registry_version: String,
    pub visibility: RegistryBuildVisibility,
    #[serde(default)]
    pub build_options: RegistryBuildOptions,
    pub package_pins: Vec<DomainPackagePin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<DomainFact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<DomainRelationshipFact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub licenses: Vec<DomainLicenseRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CompiledAlias {
    pub canonical_id: String,
    pub alias: String,
    pub namespace_id: String,
    pub domain_id: String,
    pub source_package_id: String,
    pub proof_chain_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CompiledRelationshipSidecar {
    pub relationship_id: String,
    pub left_domain_id: String,
    pub right_domain_id: String,
    pub relation_type_id: String,
    pub source_package_id: String,
    pub proof_chain_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DomainRegistryConflict {
    pub conflict_id: String,
    pub kind: DomainConflictKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canonical_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fact_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationship_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_package_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DomainProofChain {
    pub proof_chain_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fact_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationship_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_package_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub package_digests: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DomainLicenseInventoryEntry {
    pub license_id: String,
    pub label: String,
    pub uri: String,
    pub redistribution: LicenseRedistribution,
    pub pinned_package_count: usize,
    pub visible_fact_count: usize,
    pub hidden_restricted_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DomainProvenanceInventoryEntry {
    pub source_package_id: String,
    pub public_ref: String,
    pub license_id: String,
    pub visible_fact_count: usize,
    pub restricted_value_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restricted_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReproducibleBuildRecipe {
    pub recipe_version: String,
    pub input_digest: String,
    pub package_pins: Vec<DomainPackagePin>,
    pub deterministic_ordering: String,
    pub exact_alias_policy: String,
    pub relationship_policy: String,
    pub restricted_manifest_policy: String,
    pub restricted_omitted_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledRegistryPackage {
    pub version: String,
    pub artifact_content_hash: String,
    pub build_id: String,
    pub registry_id: String,
    pub registry_version: String,
    pub visibility: RegistryBuildVisibility,
    pub aliases: Vec<CompiledAlias>,
    pub relationship_sidecars: Vec<CompiledRelationshipSidecar>,
    pub unresolved_conflicts: Vec<DomainRegistryConflict>,
    pub proof_chains: Vec<DomainProofChain>,
    pub license_inventory: Vec<DomainLicenseInventoryEntry>,
    pub provenance_inventory: Vec<DomainProvenanceInventoryEntry>,
    pub package_pins: Vec<DomainPackagePin>,
    pub reproducible_build_recipe: ReproducibleBuildRecipe,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegistryBuildAddition {
    pub domain_id: String,
    pub namespace_id: String,
    pub alias: String,
    pub canonical_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegistryBuildRemap {
    pub domain_id: String,
    pub old_canonical_ids: Vec<String>,
    pub new_canonical_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegistryPackageChange {
    pub package_kind: DomainPackageKind,
    pub package_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryBuildSemanticDiff {
    pub additions: Vec<RegistryBuildAddition>,
    pub remaps: Vec<RegistryBuildRemap>,
    pub conflicts: Vec<DomainRegistryConflict>,
    pub package_changes: Vec<RegistryPackageChange>,
}

pub fn domain_registry_build_schema_version() -> &'static str {
    CANON_REGISTRY_BUILD_VERSION
}

pub fn compile_domain_registry_package(
    package: DomainRegistryBuildPackage,
) -> DomainRegistryBuildResult<CompiledRegistryPackage> {
    let package = finalize_build_package(package)?;
    reject_public_restricted_value_projection(&package)?;

    let visible_facts = package
        .facts
        .iter()
        .filter(|fact| {
            value_visible(
                package.visibility,
                &package.build_options,
                fact.restricted_value,
            )
        })
        .collect::<Vec<_>>();
    let visible_relationships = package
        .relationships
        .iter()
        .filter(|relationship| {
            value_visible(
                package.visibility,
                &package.build_options,
                relationship.restricted_value,
            )
        })
        .collect::<Vec<_>>();

    let hidden_restricted_count = package
        .facts
        .iter()
        .filter(|fact| {
            !value_visible(
                package.visibility,
                &package.build_options,
                fact.restricted_value,
            )
        })
        .count()
        + package
            .relationships
            .iter()
            .filter(|relationship| {
                !value_visible(
                    package.visibility,
                    &package.build_options,
                    relationship.restricted_value,
                )
            })
            .count();

    let remap_conflicts = remap_conflicts(&visible_facts)?;
    let alias_collision_conflicts = alias_collision_conflicts(&visible_facts)?;
    let relationship_conflicts = relationship_collision_conflicts(&visible_relationships)?;
    let mut unresolved_conflicts = remap_conflicts
        .iter()
        .chain(alias_collision_conflicts.iter())
        .chain(relationship_conflicts.iter())
        .cloned()
        .collect::<Vec<_>>();
    unresolved_conflicts.sort();
    unresolved_conflicts.dedup();

    let conflicted_domains = remap_conflicts
        .iter()
        .filter_map(|conflict| conflict.domain_id.clone())
        .collect::<BTreeSet<_>>();
    let conflicted_aliases = alias_collision_conflicts
        .iter()
        .filter_map(|conflict| match (&conflict.namespace_id, &conflict.alias) {
            (Some(namespace_id), Some(alias)) => Some((namespace_id.clone(), alias.clone())),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let conflicted_relationships = relationship_conflicts
        .iter()
        .filter_map(|conflict| conflict.relationship_id.clone())
        .collect::<BTreeSet<_>>();

    let package_digest_by_id = package_digest_by_id(&package.package_pins);
    let mut proof_chains = BTreeMap::new();
    let mut aliases = Vec::new();
    for fact in visible_facts {
        if conflicted_domains.contains(&fact.domain_id)
            || conflicted_aliases.contains(&(fact.namespace_id.clone(), fact.alias.clone()))
        {
            continue;
        }

        let proof_chain = proof_chain_for_fact(fact, &package_digest_by_id)?;
        let proof_chain_id = proof_chain.proof_chain_id.clone();
        proof_chains.insert(proof_chain_id.clone(), proof_chain);
        aliases.push(CompiledAlias {
            canonical_id: fact.canonical_id.clone(),
            alias: fact.alias.clone(),
            namespace_id: fact.namespace_id.clone(),
            domain_id: fact.domain_id.clone(),
            source_package_id: fact.source_package_id.clone(),
            proof_chain_id,
        });
    }
    aliases.sort();
    aliases.dedup();

    let mut relationship_sidecars = Vec::new();
    for relationship in visible_relationships {
        if conflicted_relationships.contains(&relationship.relationship_id) {
            continue;
        }
        let proof_chain = proof_chain_for_relationship(relationship, &package_digest_by_id)?;
        let proof_chain_id = proof_chain.proof_chain_id.clone();
        proof_chains.insert(proof_chain_id.clone(), proof_chain);
        relationship_sidecars.push(CompiledRelationshipSidecar {
            relationship_id: relationship.relationship_id.clone(),
            left_domain_id: relationship.left_domain_id.clone(),
            right_domain_id: relationship.right_domain_id.clone(),
            relation_type_id: relationship.relation_type_id.clone(),
            source_package_id: relationship.source_package_id.clone(),
            proof_chain_id,
        });
    }
    relationship_sidecars.sort();
    relationship_sidecars.dedup();

    let license_inventory = license_inventory(&package, hidden_restricted_count);
    let provenance_inventory = provenance_inventory(
        &package,
        package.visibility,
        &package.build_options,
        &conflicted_domains,
        &conflicted_aliases,
        &conflicted_relationships,
    );
    let reproducible_build_recipe = reproducible_build_recipe(&package, hidden_restricted_count)?;

    let mut compiled = CompiledRegistryPackage {
        version: CANON_REGISTRY_BUILD_VERSION.to_string(),
        artifact_content_hash: String::new(),
        build_id: package.build_id,
        registry_id: package.registry_id,
        registry_version: package.registry_version,
        visibility: package.visibility,
        aliases,
        relationship_sidecars,
        unresolved_conflicts,
        proof_chains: proof_chains.into_values().collect(),
        license_inventory,
        provenance_inventory,
        package_pins: package.package_pins,
        reproducible_build_recipe,
    };
    compiled = canonicalize_compiled_registry_package(compiled);
    compiled.artifact_content_hash = compiled_registry_package_digest(&compiled)?;
    Ok(canonicalize_compiled_registry_package(compiled))
}

pub fn finalize_build_package(
    mut package: DomainRegistryBuildPackage,
) -> DomainRegistryBuildResult<DomainRegistryBuildPackage> {
    if ascii_trim(&package.version).is_empty() {
        package.version = CANON_REGISTRY_BUILD_VERSION.to_string();
    }
    if package.version != CANON_REGISTRY_BUILD_VERSION {
        return Err(error(
            DomainRegistryBuildErrorCode::ArtifactContract,
            format!(
                "unsupported domain registry build version {}",
                package.version
            ),
        ));
    }

    package.build_id = normalize_non_empty(package.build_id, "build_id")?;
    package.registry_id = normalize_non_empty(package.registry_id, "registry_id")?;
    package.registry_version = normalize_non_empty(package.registry_version, "registry_version")?;
    package.build_options.restricted_manifest_policy = normalize_non_empty(
        package.build_options.restricted_manifest_policy,
        "restricted_manifest_policy",
    )?;

    package.package_pins = normalize_package_pins(package.package_pins)?;
    require_package_kind_coverage(&package.package_pins)?;
    let pinned_package_ids = package
        .package_pins
        .iter()
        .map(|pin| pin.package_id.clone())
        .collect::<BTreeSet<_>>();

    package.licenses = normalize_licenses(package.licenses)?;
    let known_licenses = package
        .licenses
        .iter()
        .map(|license| license.license_id.clone())
        .collect::<BTreeSet<_>>();
    for pin in &package.package_pins {
        if !known_licenses.contains(&pin.license_id) {
            return Err(error(
                DomainRegistryBuildErrorCode::ArtifactContract,
                format!(
                    "package pin {} references unknown license {}",
                    pin.package_id, pin.license_id
                ),
            ));
        }
    }

    package.facts = package
        .facts
        .into_iter()
        .map(|fact| normalize_fact(fact, &pinned_package_ids, &known_licenses))
        .collect::<DomainRegistryBuildResult<Vec<_>>>()?;
    package.facts.sort();
    package.facts = dedup_or_conflict(package.facts, |fact| fact.fact_id.clone(), "fact_id")?;

    package.relationships = package
        .relationships
        .into_iter()
        .map(|relationship| {
            normalize_relationship(relationship, &pinned_package_ids, &known_licenses)
        })
        .collect::<DomainRegistryBuildResult<Vec<_>>>()?;
    package.relationships.sort();
    package.relationships = dedup_or_conflict(
        package.relationships,
        |relationship| relationship.relationship_id.clone(),
        "relationship_id",
    )?;

    Ok(package)
}

pub fn canonical_build_package_bytes(
    package: &DomainRegistryBuildPackage,
) -> DomainRegistryBuildResult<Vec<u8>> {
    let package = finalize_build_package(package.clone())?;
    serde_json::to_vec(&package).map_err(|error| {
        DomainRegistryBuildError::new(
            DomainRegistryBuildErrorCode::ArtifactContract,
            format!("failed to serialize canonical build package: {error}"),
        )
    })
}

pub fn canonical_compiled_registry_bytes(
    package: &CompiledRegistryPackage,
) -> DomainRegistryBuildResult<Vec<u8>> {
    let package = canonicalize_compiled_registry_package(package.clone());
    serde_json::to_vec(&package).map_err(|error| {
        DomainRegistryBuildError::new(
            DomainRegistryBuildErrorCode::ArtifactContract,
            format!("failed to serialize canonical compiled package: {error}"),
        )
    })
}

pub fn domain_registry_build_digest(
    package: &DomainRegistryBuildPackage,
) -> DomainRegistryBuildResult<String> {
    Ok(hash_bytes(&canonical_build_package_bytes(package)?))
}

pub fn compiled_registry_package_digest(
    package: &CompiledRegistryPackage,
) -> DomainRegistryBuildResult<String> {
    let mut package = canonicalize_compiled_registry_package(package.clone());
    package.artifact_content_hash.clear();
    Ok(hash_bytes(&canonical_compiled_registry_bytes(&package)?))
}

pub fn semantic_diff(
    old_package: &CompiledRegistryPackage,
    new_package: &CompiledRegistryPackage,
) -> RegistryBuildSemanticDiff {
    let old_aliases = alias_keys(old_package);
    let new_aliases = alias_keys(new_package);

    let mut additions = new_aliases
        .difference(&old_aliases)
        .map(
            |(domain_id, namespace_id, alias, canonical_id)| RegistryBuildAddition {
                domain_id: domain_id.clone(),
                namespace_id: namespace_id.clone(),
                alias: alias.clone(),
                canonical_id: canonical_id.clone(),
            },
        )
        .collect::<Vec<_>>();
    additions.sort();

    let mut remaps = Vec::new();
    for domain_id in old_package
        .aliases
        .iter()
        .map(|alias| alias.domain_id.clone())
        .chain(
            new_package
                .aliases
                .iter()
                .map(|alias| alias.domain_id.clone()),
        )
        .collect::<BTreeSet<_>>()
    {
        let old_canonical_ids = canonical_ids_for_domain(old_package, &domain_id);
        let new_canonical_ids = canonical_ids_for_domain(new_package, &domain_id);
        if !old_canonical_ids.is_empty()
            && !new_canonical_ids.is_empty()
            && old_canonical_ids != new_canonical_ids
        {
            remaps.push(RegistryBuildRemap {
                domain_id,
                old_canonical_ids,
                new_canonical_ids,
            });
        }
    }
    for conflict in &new_package.unresolved_conflicts {
        if conflict.kind != DomainConflictKind::Remap {
            continue;
        }
        let Some(domain_id) = &conflict.domain_id else {
            continue;
        };
        let old_canonical_ids = canonical_ids_for_domain(old_package, domain_id);
        if old_canonical_ids.is_empty() || old_canonical_ids == conflict.canonical_ids {
            continue;
        }
        remaps.push(RegistryBuildRemap {
            domain_id: domain_id.clone(),
            old_canonical_ids,
            new_canonical_ids: conflict.canonical_ids.clone(),
        });
    }
    remaps.sort();
    remaps.dedup();

    let old_conflicts = old_package
        .unresolved_conflicts
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut conflicts = new_package
        .unresolved_conflicts
        .iter()
        .filter(|conflict| !old_conflicts.contains(*conflict))
        .cloned()
        .collect::<Vec<_>>();
    conflicts.sort();

    let old_pins = package_pin_map(&old_package.package_pins);
    let new_pins = package_pin_map(&new_package.package_pins);
    let mut package_changes = Vec::new();
    for key in old_pins
        .keys()
        .chain(new_pins.keys())
        .collect::<BTreeSet<_>>()
    {
        let old_pin = old_pins.get(key);
        let new_pin = new_pins.get(key);
        if old_pin == new_pin {
            continue;
        }
        package_changes.push(RegistryPackageChange {
            package_kind: key.0,
            package_id: key.1.clone(),
            old_version: old_pin.map(|pin| pin.package_version.clone()),
            new_version: new_pin.map(|pin| pin.package_version.clone()),
            old_digest: old_pin.map(|pin| pin.package_digest.clone()),
            new_digest: new_pin.map(|pin| pin.package_digest.clone()),
        });
    }
    package_changes.sort();

    RegistryBuildSemanticDiff {
        additions,
        remaps,
        conflicts,
        package_changes,
    }
}

fn reject_public_restricted_value_projection(
    package: &DomainRegistryBuildPackage,
) -> DomainRegistryBuildResult<()> {
    let restricted_count = package
        .facts
        .iter()
        .filter(|fact| fact.restricted_value)
        .count()
        + package
            .relationships
            .iter()
            .filter(|relationship| relationship.restricted_value)
            .count();
    if package.visibility == RegistryBuildVisibility::PublicThin
        && package.build_options.include_restricted_values
        && restricted_count > 0
    {
        return Err(error(
            DomainRegistryBuildErrorCode::RestrictedDataLeak,
            "public-thin registry builds cannot include restricted values",
        ));
    }
    Ok(())
}

fn remap_conflicts(
    facts: &[&DomainFact],
) -> DomainRegistryBuildResult<Vec<DomainRegistryConflict>> {
    let mut by_domain: BTreeMap<String, Vec<&DomainFact>> = BTreeMap::new();
    for fact in facts {
        by_domain
            .entry(fact.domain_id.clone())
            .or_default()
            .push(*fact);
    }

    let mut conflicts = Vec::new();
    for (domain_id, facts) in by_domain {
        let canonical_ids = facts
            .iter()
            .map(|fact| fact.canonical_id.clone())
            .collect::<BTreeSet<_>>();
        if canonical_ids.len() < 2 {
            continue;
        }
        let fact_ids = facts
            .iter()
            .map(|fact| fact.fact_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let proof_refs = sorted_flattened_proof_refs(facts.iter().map(|fact| &fact.proof_refs));
        let source_package_ids = facts
            .iter()
            .map(|fact| fact.source_package_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        conflicts.push(conflict_with_digest(DomainRegistryConflict {
            conflict_id: String::new(),
            kind: DomainConflictKind::Remap,
            domain_id: Some(domain_id),
            namespace_id: None,
            alias: None,
            relationship_id: None,
            canonical_ids: canonical_ids.into_iter().collect(),
            fact_ids,
            relationship_ids: Vec::new(),
            proof_refs,
            source_package_ids,
        })?);
    }
    Ok(conflicts)
}

fn alias_collision_conflicts(
    facts: &[&DomainFact],
) -> DomainRegistryBuildResult<Vec<DomainRegistryConflict>> {
    let mut by_alias: BTreeMap<(String, String), Vec<&DomainFact>> = BTreeMap::new();
    for fact in facts {
        by_alias
            .entry((fact.namespace_id.clone(), fact.alias.clone()))
            .or_default()
            .push(*fact);
    }

    let mut conflicts = Vec::new();
    for ((namespace_id, alias), facts) in by_alias {
        let canonical_ids = facts
            .iter()
            .map(|fact| fact.canonical_id.clone())
            .collect::<BTreeSet<_>>();
        if canonical_ids.len() < 2 {
            continue;
        }
        let fact_ids = facts
            .iter()
            .map(|fact| fact.fact_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let proof_refs = sorted_flattened_proof_refs(facts.iter().map(|fact| &fact.proof_refs));
        let source_package_ids = facts
            .iter()
            .map(|fact| fact.source_package_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        conflicts.push(conflict_with_digest(DomainRegistryConflict {
            conflict_id: String::new(),
            kind: DomainConflictKind::AliasCollision,
            domain_id: None,
            namespace_id: Some(namespace_id),
            alias: Some(alias),
            relationship_id: None,
            canonical_ids: canonical_ids.into_iter().collect(),
            fact_ids,
            relationship_ids: Vec::new(),
            proof_refs,
            source_package_ids,
        })?);
    }
    Ok(conflicts)
}

fn relationship_collision_conflicts(
    relationships: &[&DomainRelationshipFact],
) -> DomainRegistryBuildResult<Vec<DomainRegistryConflict>> {
    let mut by_id: BTreeMap<String, Vec<&DomainRelationshipFact>> = BTreeMap::new();
    for relationship in relationships {
        by_id
            .entry(relationship.relationship_id.clone())
            .or_default()
            .push(*relationship);
    }

    let mut conflicts = Vec::new();
    for (relationship_id, relationships) in by_id {
        let unique_shapes = relationships
            .iter()
            .map(|relationship| {
                (
                    relationship.left_domain_id.clone(),
                    relationship.right_domain_id.clone(),
                    relationship.relation_type_id.clone(),
                )
            })
            .collect::<BTreeSet<_>>();
        if unique_shapes.len() < 2 {
            continue;
        }
        let proof_refs =
            sorted_flattened_proof_refs(relationships.iter().map(|rel| &rel.proof_refs));
        let source_package_ids = relationships
            .iter()
            .map(|relationship| relationship.source_package_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        conflicts.push(conflict_with_digest(DomainRegistryConflict {
            conflict_id: String::new(),
            kind: DomainConflictKind::RelationshipCollision,
            domain_id: None,
            namespace_id: None,
            alias: None,
            relationship_id: Some(relationship_id.clone()),
            canonical_ids: Vec::new(),
            fact_ids: Vec::new(),
            relationship_ids: vec![relationship_id],
            proof_refs,
            source_package_ids,
        })?);
    }
    Ok(conflicts)
}

fn proof_chain_for_fact(
    fact: &DomainFact,
    package_digest_by_id: &BTreeMap<String, String>,
) -> DomainRegistryBuildResult<DomainProofChain> {
    let package_digest = package_digest_by_id
        .get(&fact.source_package_id)
        .ok_or_else(|| {
            error(
                DomainRegistryBuildErrorCode::ArtifactContract,
                format!("missing package digest for {}", fact.source_package_id),
            )
        })?
        .clone();
    proof_chain_with_digest(DomainProofChain {
        proof_chain_id: String::new(),
        fact_ids: vec![fact.fact_id.clone()],
        relationship_ids: Vec::new(),
        source_package_ids: vec![fact.source_package_id.clone()],
        package_digests: vec![package_digest],
        proof_refs: fact.proof_refs.clone(),
    })
}

fn proof_chain_for_relationship(
    relationship: &DomainRelationshipFact,
    package_digest_by_id: &BTreeMap<String, String>,
) -> DomainRegistryBuildResult<DomainProofChain> {
    let package_digest = package_digest_by_id
        .get(&relationship.source_package_id)
        .ok_or_else(|| {
            error(
                DomainRegistryBuildErrorCode::ArtifactContract,
                format!(
                    "missing package digest for {}",
                    relationship.source_package_id
                ),
            )
        })?
        .clone();
    proof_chain_with_digest(DomainProofChain {
        proof_chain_id: String::new(),
        fact_ids: Vec::new(),
        relationship_ids: vec![relationship.relationship_id.clone()],
        source_package_ids: vec![relationship.source_package_id.clone()],
        package_digests: vec![package_digest],
        proof_refs: relationship.proof_refs.clone(),
    })
}

fn proof_chain_with_digest(
    mut proof_chain: DomainProofChain,
) -> DomainRegistryBuildResult<DomainProofChain> {
    proof_chain.fact_ids.sort();
    proof_chain.fact_ids.dedup();
    proof_chain.relationship_ids.sort();
    proof_chain.relationship_ids.dedup();
    proof_chain.source_package_ids.sort();
    proof_chain.source_package_ids.dedup();
    proof_chain.package_digests.sort();
    proof_chain.package_digests.dedup();
    proof_chain.proof_refs.sort();
    proof_chain.proof_refs.dedup();
    proof_chain.proof_chain_id = digest_struct("proof", &proof_chain)?;
    Ok(proof_chain)
}

fn conflict_with_digest(
    mut conflict: DomainRegistryConflict,
) -> DomainRegistryBuildResult<DomainRegistryConflict> {
    conflict.canonical_ids.sort();
    conflict.canonical_ids.dedup();
    conflict.fact_ids.sort();
    conflict.fact_ids.dedup();
    conflict.relationship_ids.sort();
    conflict.relationship_ids.dedup();
    conflict.proof_refs.sort();
    conflict.proof_refs.dedup();
    conflict.source_package_ids.sort();
    conflict.source_package_ids.dedup();
    conflict.conflict_id = digest_struct("conflict", &conflict)?;
    Ok(conflict)
}

fn reproducible_build_recipe(
    package: &DomainRegistryBuildPackage,
    hidden_restricted_count: usize,
) -> DomainRegistryBuildResult<ReproducibleBuildRecipe> {
    Ok(ReproducibleBuildRecipe {
        recipe_version: CANON_REGISTRY_BUILD_VERSION.to_string(),
        input_digest: domain_registry_build_digest(package)?,
        package_pins: package.package_pins.clone(),
        deterministic_ordering: "lexicographic_structural_order".to_string(),
        exact_alias_policy: "ascii_trimmed_exact_aliases_only".to_string(),
        relationship_policy: "relationship_facts_emit_sidecars_never_identity_aliases".to_string(),
        restricted_manifest_policy: package.build_options.restricted_manifest_policy.clone(),
        restricted_omitted_count: hidden_restricted_count,
    })
}

fn license_inventory(
    package: &DomainRegistryBuildPackage,
    hidden_restricted_count: usize,
) -> Vec<DomainLicenseInventoryEntry> {
    let pinned_count_by_license =
        package
            .package_pins
            .iter()
            .fold(BTreeMap::<String, usize>::new(), |mut counts, pin| {
                *counts.entry(pin.license_id.clone()).or_default() += 1;
                counts
            });
    let visible_count_by_license = package
        .facts
        .iter()
        .filter(|fact| {
            value_visible(
                package.visibility,
                &package.build_options,
                fact.restricted_value,
            )
        })
        .map(|fact| fact.provenance.license_id.clone())
        .chain(
            package
                .relationships
                .iter()
                .filter(|relationship| {
                    value_visible(
                        package.visibility,
                        &package.build_options,
                        relationship.restricted_value,
                    )
                })
                .map(|relationship| relationship.provenance.license_id.clone()),
        )
        .fold(
            BTreeMap::<String, usize>::new(),
            |mut counts, license_id| {
                *counts.entry(license_id).or_default() += 1;
                counts
            },
        );

    let mut inventory = package
        .licenses
        .iter()
        .map(|license| DomainLicenseInventoryEntry {
            license_id: license.license_id.clone(),
            label: license.label.clone(),
            uri: license.uri.clone(),
            redistribution: license.redistribution,
            pinned_package_count: pinned_count_by_license
                .get(&license.license_id)
                .copied()
                .unwrap_or(0),
            visible_fact_count: visible_count_by_license
                .get(&license.license_id)
                .copied()
                .unwrap_or(0),
            hidden_restricted_count: if license.redistribution == LicenseRedistribution::Restricted
            {
                hidden_restricted_count
            } else {
                0
            },
        })
        .collect::<Vec<_>>();
    inventory.sort();
    inventory
}

fn provenance_inventory(
    package: &DomainRegistryBuildPackage,
    visibility: RegistryBuildVisibility,
    options: &RegistryBuildOptions,
    conflicted_domains: &BTreeSet<String>,
    conflicted_aliases: &BTreeSet<(String, String)>,
    conflicted_relationships: &BTreeSet<String>,
) -> Vec<DomainProvenanceInventoryEntry> {
    let mut counts = BTreeMap::<(String, String, String, Option<String>), (usize, usize)>::new();
    for fact in &package.facts {
        if !value_visible(visibility, options, fact.restricted_value)
            || conflicted_domains.contains(&fact.domain_id)
            || conflicted_aliases.contains(&(fact.namespace_id.clone(), fact.alias.clone()))
        {
            continue;
        }
        let key = provenance_inventory_key(&fact.provenance, visibility);
        let entry = counts.entry(key).or_default();
        entry.0 += 1;
        if fact.restricted_value {
            entry.1 += 1;
        }
    }
    for relationship in &package.relationships {
        if !value_visible(visibility, options, relationship.restricted_value)
            || conflicted_relationships.contains(&relationship.relationship_id)
        {
            continue;
        }
        let key = provenance_inventory_key(&relationship.provenance, visibility);
        let entry = counts.entry(key).or_default();
        entry.0 += 1;
        if relationship.restricted_value {
            entry.1 += 1;
        }
    }

    counts
        .into_iter()
        .map(
            |((source_package_id, public_ref, license_id, restricted_detail), counts)| {
                DomainProvenanceInventoryEntry {
                    source_package_id,
                    public_ref,
                    license_id,
                    visible_fact_count: counts.0,
                    restricted_value_count: counts.1,
                    restricted_detail,
                }
            },
        )
        .collect()
}

fn provenance_inventory_key(
    provenance: &DomainProvenanceRef,
    visibility: RegistryBuildVisibility,
) -> (String, String, String, Option<String>) {
    (
        provenance.source_package_id.clone(),
        provenance.public_ref.clone(),
        provenance.license_id.clone(),
        match visibility {
            RegistryBuildVisibility::PublicThin => None,
            RegistryBuildVisibility::LocalPrivate => provenance.restricted_detail.clone(),
        },
    )
}

fn canonicalize_compiled_registry_package(
    mut package: CompiledRegistryPackage,
) -> CompiledRegistryPackage {
    package.aliases.sort();
    package.aliases.dedup();
    package.relationship_sidecars.sort();
    package.relationship_sidecars.dedup();
    package.unresolved_conflicts.sort();
    package.unresolved_conflicts.dedup();
    package.proof_chains.sort();
    package.proof_chains.dedup();
    package.license_inventory.sort();
    package.license_inventory.dedup();
    package.provenance_inventory.sort();
    package.provenance_inventory.dedup();
    package.package_pins.sort();
    package.package_pins.dedup();
    package.reproducible_build_recipe.package_pins.sort();
    package.reproducible_build_recipe.package_pins.dedup();
    package
}

fn normalize_package_pins(
    pins: Vec<DomainPackagePin>,
) -> DomainRegistryBuildResult<Vec<DomainPackagePin>> {
    let mut pins = pins
        .into_iter()
        .map(|pin| {
            Ok(DomainPackagePin {
                package_kind: pin.package_kind,
                package_id: normalize_non_empty(pin.package_id, "package_id")?,
                package_version: normalize_non_empty(pin.package_version, "package_version")?,
                package_digest: normalize_digest(pin.package_digest, "package_digest")?,
                license_id: normalize_non_empty(pin.license_id, "license_id")?,
                restricted: pin.restricted,
            })
        })
        .collect::<DomainRegistryBuildResult<Vec<_>>>()?;
    pins.sort();
    dedup_or_conflict(
        pins,
        |pin| (pin.package_kind, pin.package_id.clone()),
        "package_pin",
    )
}

fn normalize_licenses(
    licenses: Vec<DomainLicenseRef>,
) -> DomainRegistryBuildResult<Vec<DomainLicenseRef>> {
    let mut licenses = licenses
        .into_iter()
        .map(|license| {
            Ok(DomainLicenseRef {
                license_id: normalize_non_empty(license.license_id, "license_id")?,
                label: normalize_non_empty(license.label, "license_label")?,
                uri: normalize_non_empty(license.uri, "license_uri")?,
                redistribution: license.redistribution,
            })
        })
        .collect::<DomainRegistryBuildResult<Vec<_>>>()?;
    licenses.sort();
    dedup_or_conflict(licenses, |license| license.license_id.clone(), "license_id")
}

fn normalize_fact(
    fact: DomainFact,
    pinned_package_ids: &BTreeSet<String>,
    known_licenses: &BTreeSet<String>,
) -> DomainRegistryBuildResult<DomainFact> {
    let source_package_id = normalize_known_package_id(fact.source_package_id, pinned_package_ids)?;
    Ok(DomainFact {
        fact_id: normalize_non_empty(fact.fact_id, "fact_id")?,
        domain_id: normalize_non_empty(fact.domain_id, "domain_id")?,
        canonical_id: normalize_non_empty(fact.canonical_id, "canonical_id")?,
        alias: normalize_non_empty(fact.alias, "alias")?,
        namespace_id: normalize_non_empty(fact.namespace_id, "namespace_id")?,
        source_package_id,
        proof_refs: normalize_string_vec(fact.proof_refs, "proof_ref")?,
        provenance: normalize_provenance(fact.provenance, pinned_package_ids, known_licenses)?,
        restricted_value: fact.restricted_value,
    })
}

fn normalize_relationship(
    relationship: DomainRelationshipFact,
    pinned_package_ids: &BTreeSet<String>,
    known_licenses: &BTreeSet<String>,
) -> DomainRegistryBuildResult<DomainRelationshipFact> {
    let source_package_id =
        normalize_known_package_id(relationship.source_package_id, pinned_package_ids)?;
    Ok(DomainRelationshipFact {
        relationship_id: normalize_non_empty(relationship.relationship_id, "relationship_id")?,
        left_domain_id: normalize_non_empty(relationship.left_domain_id, "left_domain_id")?,
        right_domain_id: normalize_non_empty(relationship.right_domain_id, "right_domain_id")?,
        relation_type_id: normalize_non_empty(relationship.relation_type_id, "relation_type_id")?,
        source_package_id,
        proof_refs: normalize_string_vec(relationship.proof_refs, "proof_ref")?,
        provenance: normalize_provenance(
            relationship.provenance,
            pinned_package_ids,
            known_licenses,
        )?,
        restricted_value: relationship.restricted_value,
    })
}

fn normalize_provenance(
    provenance: DomainProvenanceRef,
    pinned_package_ids: &BTreeSet<String>,
    known_licenses: &BTreeSet<String>,
) -> DomainRegistryBuildResult<DomainProvenanceRef> {
    let source_package_id =
        normalize_known_package_id(provenance.source_package_id, pinned_package_ids)?;
    let license_id = normalize_non_empty(provenance.license_id, "provenance_license_id")?;
    if !known_licenses.contains(&license_id) {
        return Err(error(
            DomainRegistryBuildErrorCode::ArtifactContract,
            format!("provenance references unknown license {license_id}"),
        ));
    }
    Ok(DomainProvenanceRef {
        source_package_id,
        public_ref: normalize_non_empty(provenance.public_ref, "public_ref")?,
        license_id,
        restricted_detail: provenance
            .restricted_detail
            .map(|detail| normalize_non_empty(detail, "restricted_detail"))
            .transpose()?,
    })
}

fn normalize_known_package_id(
    package_id: String,
    pinned_package_ids: &BTreeSet<String>,
) -> DomainRegistryBuildResult<String> {
    let package_id = normalize_non_empty(package_id, "source_package_id")?;
    if !pinned_package_ids.contains(&package_id) {
        return Err(error(
            DomainRegistryBuildErrorCode::ArtifactContract,
            format!("source_package_id {package_id} is not pinned"),
        ));
    }
    Ok(package_id)
}

fn require_package_kind_coverage(pins: &[DomainPackagePin]) -> DomainRegistryBuildResult<()> {
    let present = pins
        .iter()
        .map(|pin| pin.package_kind)
        .collect::<BTreeSet<_>>();
    for required in [
        DomainPackageKind::Ontology,
        DomainPackageKind::Namespace,
        DomainPackageKind::Vocabulary,
        DomainPackageKind::Fact,
        DomainPackageKind::Review,
        DomainPackageKind::TrustConflict,
        DomainPackageKind::Temporal,
        DomainPackageKind::Projection,
    ] {
        if !present.contains(&required) {
            return Err(error(
                DomainRegistryBuildErrorCode::CompatibilityPolicy,
                format!("missing required package kind {required:?}"),
            ));
        }
    }
    Ok(())
}

fn normalize_digest(digest: String, field: &str) -> DomainRegistryBuildResult<String> {
    let digest = normalize_non_empty(digest, field)?;
    if is_blake3_digest(&digest) {
        Ok(digest)
    } else {
        Err(error(
            DomainRegistryBuildErrorCode::ArtifactContract,
            format!("{field} must be a lowercase blake3 digest"),
        ))
    }
}

fn normalize_string_vec(
    values: Vec<String>,
    field: &str,
) -> DomainRegistryBuildResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalize_non_empty(value, field))
        .collect::<DomainRegistryBuildResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalize_non_empty(value: String, field: &str) -> DomainRegistryBuildResult<String> {
    let trimmed = ascii_trim(&value).to_string();
    if trimmed.is_empty() {
        Err(error(
            DomainRegistryBuildErrorCode::ArtifactContract,
            format!("{field} must not be empty"),
        ))
    } else {
        Ok(trimmed)
    }
}

fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_ascii_whitespace())
}

fn value_visible(
    visibility: RegistryBuildVisibility,
    options: &RegistryBuildOptions,
    restricted_value: bool,
) -> bool {
    !restricted_value
        || (visibility == RegistryBuildVisibility::LocalPrivate
            && options.include_restricted_values)
}

fn is_blake3_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn digest_struct<T: Serialize>(prefix: &str, value: &T) -> DomainRegistryBuildResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        DomainRegistryBuildError::new(
            DomainRegistryBuildErrorCode::ArtifactContract,
            format!("failed to serialize {prefix} digest input: {error}"),
        )
    })?;
    Ok(format!("{prefix}:{}", blake3::hash(&bytes).to_hex()))
}

fn package_digest_by_id(pins: &[DomainPackagePin]) -> BTreeMap<String, String> {
    pins.iter()
        .map(|pin| (pin.package_id.clone(), pin.package_digest.clone()))
        .collect()
}

fn sorted_flattened_proof_refs<'a>(
    proof_refs: impl Iterator<Item = &'a Vec<String>>,
) -> Vec<String> {
    proof_refs
        .flat_map(|refs| refs.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn alias_keys(package: &CompiledRegistryPackage) -> BTreeSet<(String, String, String, String)> {
    package
        .aliases
        .iter()
        .map(|alias| {
            (
                alias.domain_id.clone(),
                alias.namespace_id.clone(),
                alias.alias.clone(),
                alias.canonical_id.clone(),
            )
        })
        .collect()
}

fn canonical_ids_for_domain(package: &CompiledRegistryPackage, domain_id: &str) -> Vec<String> {
    package
        .aliases
        .iter()
        .filter(|alias| alias.domain_id == domain_id)
        .map(|alias| alias.canonical_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn package_pin_map(
    pins: &[DomainPackagePin],
) -> BTreeMap<(DomainPackageKind, String), DomainPackagePin> {
    pins.iter()
        .map(|pin| ((pin.package_kind, pin.package_id.clone()), pin.clone()))
        .collect()
}

fn dedup_or_conflict<T, K>(
    values: Vec<T>,
    key: impl Fn(&T) -> K,
    label: &str,
) -> DomainRegistryBuildResult<Vec<T>>
where
    T: Clone + PartialEq,
    K: Ord + fmt::Debug,
{
    let mut deduped = Vec::with_capacity(values.len());
    for value in values {
        if let Some(previous) = deduped.iter().find(|previous| key(previous) == key(&value)) {
            if previous != &value {
                return Err(error(
                    DomainRegistryBuildErrorCode::Conflict,
                    format!(
                        "duplicate {label} {:?} has conflicting content",
                        key(&value)
                    ),
                ));
            }
            continue;
        }
        deduped.push(value);
    }
    Ok(deduped)
}

fn error(
    code: DomainRegistryBuildErrorCode,
    message: impl Into<String>,
) -> DomainRegistryBuildError {
    DomainRegistryBuildError::new(code, message)
}
