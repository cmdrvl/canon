#![forbid(unsafe_code)]

#[path = "../src/registry/domain_compile.rs"]
mod domain_compile;

use domain_compile::{
    CANON_REGISTRY_BUILD_VERSION, CompiledRegistryPackage, DomainConflictKind, DomainFact,
    DomainLicenseRef, DomainPackageKind, DomainPackagePin, DomainProvenanceRef,
    DomainRegistryBuildErrorCode, DomainRegistryBuildPackage, DomainRelationshipFact,
    LicenseRedistribution, RegistryBuildOptions, RegistryBuildVisibility,
    canonical_compiled_registry_bytes, compile_domain_registry_package,
    domain_registry_build_schema_version, semantic_diff,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.registry.build.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/registry/domain_compile.rs");

#[test]
fn schema_declares_generic_registry_build_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_REGISTRY_BUILD_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_REGISTRY_BUILD_VERSION
    );
    assert_eq!(schema["x-canon-contract"]["generic_domain_ids_only"], true);
    assert_eq!(
        schema["x-canon-contract"]["unknown_domain_ids_round_trip"],
        true
    );
    assert_eq!(schema["x-canon-contract"]["rebuild_byte_stable"], true);
    assert_eq!(
        schema["x-canon-contract"]["semantic_diff_exposes_additions_remaps_conflicts_package_changes"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["restricted_public_manifest_leak_forbidden"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["mapping_requires_fact_review_policy_refs"],
        true
    );

    let kinds = schema["$defs"]["package_kind"]["enum"]
        .as_array()
        .expect("package kind enum");
    for required in [
        "ontology",
        "namespace",
        "vocabulary",
        "fact",
        "review",
        "trust_conflict",
        "temporal",
        "projection",
    ] {
        assert!(
            kinds.iter().any(|kind| kind == required),
            "schema should declare package kind {required}"
        );
    }
    assert_eq!(
        domain_registry_build_schema_version(),
        CANON_REGISTRY_BUILD_VERSION
    );
}

#[test]
fn unknown_domain_ids_round_trip_and_compile_exact_outputs() {
    let compiled = compile_domain_registry_package(base_build()).expect("build compiles");

    assert_eq!(compiled.aliases.len(), 2);
    assert!(compiled.aliases.iter().any(|alias| {
        alias.domain_id == "opaque.alpha:external-001"
            && alias.namespace_id == "ns.unknown:display"
            && alias.alias == "Alpha One"
            && alias.canonical_id == "RID-001"
    }));
    assert!(compiled.relationship_sidecars.iter().any(|relationship| {
        relationship.left_domain_id == "opaque.alpha:external-001"
            && relationship.right_domain_id == "opaque.beta:external-002"
            && relationship.relation_type_id == "rel.synthetic:adjacent_to"
    }));
    assert_eq!(compiled.unresolved_conflicts, Vec::new());
    assert!(compiled.proof_chains.iter().any(|proof| {
        proof.package_digests.contains(&digest('d'))
            && proof
                .review_refs
                .contains(&"review://fact.alpha".to_string())
            && proof
                .policy_refs
                .contains(&"policy://generic-reviewed-identity-fact".to_string())
    }));

    let second_domain =
        compile_domain_registry_package(alternate_domain_build()).expect("second domain compiles");
    assert!(second_domain.aliases.iter().any(|alias| {
        alias.domain_id == "opaque.second-domain:identifier-900"
            && alias.namespace_id == "ns.second:label"
            && alias.alias == "Second Domain Label"
            && alias.canonical_id == "RID-900"
    }));
}

#[test]
fn rebuild_is_byte_stable_and_recipe_is_reproducible() {
    let mut shuffled = base_build();
    shuffled.package_pins.reverse();
    shuffled.facts.reverse();
    shuffled.relationships.reverse();
    shuffled.licenses.reverse();

    let first = compile_domain_registry_package(base_build()).expect("first compile");
    let second = compile_domain_registry_package(shuffled).expect("second compile");

    assert_eq!(first.artifact_content_hash, second.artifact_content_hash);
    assert_eq!(
        first.reproducible_build_recipe.input_digest,
        second.reproducible_build_recipe.input_digest
    );
    assert_eq!(
        canonical_compiled_registry_bytes(&first).unwrap(),
        canonical_compiled_registry_bytes(&second).unwrap()
    );
}

#[test]
fn semantic_diff_exposes_additions_remaps_conflicts_and_package_changes() {
    let old = compile_domain_registry_package(base_build()).expect("old compile");

    let mut changed = base_build();
    changed.package_pins[0].package_digest = digest('e');
    changed.package_pins[0].package_version = "2.0.0".to_string();
    changed.facts.push(fact(
        "fact.gamma",
        "opaque.gamma:external-003",
        "RID-003",
        "Gamma Three",
        false,
    ));
    changed.facts.push(fact(
        "fact.alpha-remap",
        "opaque.alpha:external-001",
        "RID-999",
        "Alpha One",
        false,
    ));
    changed.facts.push(fact(
        "fact.alias-left",
        "opaque.delta:external-004",
        "RID-004",
        "Shared Alias",
        false,
    ));
    changed.facts.push(fact(
        "fact.alias-right",
        "opaque.epsilon:external-005",
        "RID-005",
        "Shared Alias",
        false,
    ));
    let new = compile_domain_registry_package(changed).expect("changed compile");

    let diff = semantic_diff(&old, &new);
    assert!(
        diff.additions
            .iter()
            .any(|addition| addition.domain_id == "opaque.gamma:external-003")
    );
    assert!(
        diff.remaps
            .iter()
            .any(|remap| remap.domain_id == "opaque.alpha:external-001")
    );
    assert!(
        diff.conflicts
            .iter()
            .any(|conflict| conflict.kind == DomainConflictKind::AliasCollision)
    );
    assert_eq!(diff.package_changes.len(), 1);
    assert_eq!(
        diff.package_changes[0].package_kind,
        DomainPackageKind::Ontology
    );
}

#[test]
fn mapping_without_review_or_policy_reference_refuses_compile() {
    let mut missing_review = base_build();
    missing_review.facts[0].review_refs.clear();
    let error = compile_domain_registry_package(missing_review)
        .expect_err("fact without review evidence refuses");
    assert_eq!(
        error.code,
        DomainRegistryBuildErrorCode::MissingJustification
    );
    assert!(error.message.contains("fact.alpha"));
    assert!(error.message.contains("review_refs"));

    let mut missing_policy = base_build();
    missing_policy.facts[0].policy_refs.clear();
    let error = compile_domain_registry_package(missing_policy)
        .expect_err("fact without policy evidence refuses");
    assert_eq!(
        error.code,
        DomainRegistryBuildErrorCode::MissingJustification
    );
    assert!(error.message.contains("fact.alpha"));
    assert!(error.message.contains("policy_refs"));
}

#[test]
fn conflicting_fact_pair_without_trust_policy_yields_unresolved_conflict() {
    let mut build = base_build();
    build.facts.push(fact(
        "fact.alpha-competing",
        "opaque.alpha:external-001",
        "RID-999",
        "Alpha One Alternate",
        false,
    ));

    let compiled = compile_domain_registry_package(build).expect("conflict compiles");

    assert!(
        compiled
            .aliases
            .iter()
            .all(|alias| alias.domain_id != "opaque.alpha:external-001"),
        "conflicted mappings must not choose a silent winner"
    );
    let conflict = compiled
        .unresolved_conflicts
        .iter()
        .find(|conflict| conflict.kind == DomainConflictKind::Remap)
        .expect("remap conflict emitted");
    assert_eq!(
        conflict.domain_id.as_deref(),
        Some("opaque.alpha:external-001")
    );
    assert!(conflict.fact_ids.contains(&"fact.alpha".to_string()));
    assert!(
        conflict
            .fact_ids
            .contains(&"fact.alpha-competing".to_string())
    );
    assert!(
        conflict
            .review_refs
            .contains(&"review://fact.alpha-competing".to_string())
    );
    assert!(
        conflict
            .policy_refs
            .contains(&"policy://generic-reviewed-identity-fact".to_string())
    );
}

#[test]
fn restricted_public_pack_does_not_leak_private_values_but_local_pack_can() {
    let secret_value = "SECRET-ALIAS-72";
    let secret_detail = "PRIVATE-ROW-99";

    let mut public_build = base_build();
    public_build
        .facts
        .push(restricted_fact(secret_value, secret_detail));
    let public_pack = compile_domain_registry_package(public_build).expect("public thin compile");
    let public_json = String::from_utf8(canonical_compiled_registry_bytes(&public_pack).unwrap())
        .expect("public json is utf8");
    assert!(!public_json.contains(secret_value));
    assert!(!public_json.contains(secret_detail));
    assert_eq!(
        public_pack
            .reproducible_build_recipe
            .restricted_omitted_count,
        1
    );

    let mut refused = base_build();
    refused.build_options.include_restricted_values = true;
    refused
        .facts
        .push(restricted_fact(secret_value, secret_detail));
    let error = compile_domain_registry_package(refused).expect_err("public leak refuses");
    assert_eq!(error.code, DomainRegistryBuildErrorCode::RestrictedDataLeak);

    let mut private_build = base_build();
    private_build.visibility = RegistryBuildVisibility::LocalPrivate;
    private_build.build_options.include_restricted_values = true;
    private_build
        .facts
        .push(restricted_fact(secret_value, secret_detail));
    let private_pack =
        compile_domain_registry_package(private_build).expect("private pack compiles");
    let private_json = String::from_utf8(canonical_compiled_registry_bytes(&private_pack).unwrap())
        .expect("private json is utf8");
    assert!(private_json.contains(secret_value));
    assert!(private_json.contains(secret_detail));
}

#[test]
fn relation_only_facts_do_not_create_identity_aliases() {
    let mut build = base_build();
    build.facts.clear();
    let compiled = compile_domain_registry_package(build).expect("relation-only build compiles");

    assert_eq!(compiled.aliases, Vec::new());
    assert_eq!(compiled.relationship_sidecars.len(), 1);
    assert_eq!(
        compiled.relationship_sidecars[0].relation_type_id,
        "rel.synthetic:adjacent_to"
    );
}

#[test]
fn source_and_schema_stay_domain_neutral() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();
    for banned in ["cmbs", "regab", "loan", "servicer", "tranche"] {
        assert!(
            !lower_source.contains(banned),
            "compiler source should not embed domain term {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "registry build schema should not embed domain term {banned}"
        );
    }
}

fn base_build() -> DomainRegistryBuildPackage {
    DomainRegistryBuildPackage {
        version: CANON_REGISTRY_BUILD_VERSION.to_string(),
        build_id: "build.synthetic.001".to_string(),
        registry_id: "registry.synthetic".to_string(),
        registry_version: "2026.07.11".to_string(),
        visibility: RegistryBuildVisibility::PublicThin,
        build_options: RegistryBuildOptions {
            include_restricted_values: false,
            restricted_manifest_policy: "public_thin_omits_restricted_values".to_string(),
        },
        package_pins: package_pins(),
        facts: vec![
            fact(
                "fact.alpha",
                "opaque.alpha:external-001",
                "RID-001",
                "Alpha One",
                false,
            ),
            fact(
                "fact.beta",
                "opaque.beta:external-002",
                "RID-002",
                "Beta Two",
                false,
            ),
        ],
        relationships: vec![relationship("relfact.alpha-beta")],
        licenses: vec![
            DomainLicenseRef {
                license_id: "lic.public".to_string(),
                label: "Public terms".to_string(),
                uri: "https://example.test/public".to_string(),
                redistribution: LicenseRedistribution::Public,
            },
            DomainLicenseRef {
                license_id: "lic.restricted".to_string(),
                label: "Restricted terms".to_string(),
                uri: "local://restricted/terms".to_string(),
                redistribution: LicenseRedistribution::Restricted,
            },
        ],
    }
}

fn alternate_domain_build() -> DomainRegistryBuildPackage {
    let mut build = base_build();
    build.build_id = "build.synthetic.002".to_string();
    build.registry_id = "registry.second.synthetic".to_string();
    build.registry_version = "2026.07.12".to_string();
    build.facts = vec![DomainFact {
        fact_id: "fact.second-domain".to_string(),
        domain_id: "opaque.second-domain:identifier-900".to_string(),
        canonical_id: "RID-900".to_string(),
        alias: "Second Domain Label".to_string(),
        namespace_id: "ns.second:label".to_string(),
        source_package_id: "pkg.synthetic.fact".to_string(),
        proof_refs: vec!["proof://fact.second-domain".to_string()],
        review_refs: vec!["review://fact.second-domain".to_string()],
        policy_refs: vec!["policy://generic-reviewed-identity-fact".to_string()],
        provenance: provenance("pkg.synthetic.fact", "source://public/second-domain", None),
        restricted_value: false,
    }];
    build.relationships = Vec::new();
    build
}

fn package_pins() -> Vec<DomainPackagePin> {
    [
        (DomainPackageKind::Ontology, "pkg.synthetic.ontology", 'a'),
        (DomainPackageKind::Namespace, "pkg.synthetic.namespace", 'b'),
        (
            DomainPackageKind::Vocabulary,
            "pkg.synthetic.vocabulary",
            'c',
        ),
        (DomainPackageKind::Fact, "pkg.synthetic.fact", 'd'),
        (DomainPackageKind::Review, "pkg.synthetic.review", 'e'),
        (
            DomainPackageKind::TrustConflict,
            "pkg.synthetic.trust_conflict",
            'f',
        ),
        (DomainPackageKind::Temporal, "pkg.synthetic.temporal", '1'),
        (
            DomainPackageKind::Projection,
            "pkg.synthetic.projection",
            '2',
        ),
    ]
    .into_iter()
    .map(|(package_kind, package_id, hex)| DomainPackagePin {
        package_kind,
        package_id: package_id.to_string(),
        package_version: "1.0.0".to_string(),
        package_digest: digest(hex),
        license_id: if package_kind == DomainPackageKind::Fact {
            "lic.restricted".to_string()
        } else {
            "lic.public".to_string()
        },
        restricted: package_kind == DomainPackageKind::Fact,
    })
    .collect()
}

fn fact(
    fact_id: &str,
    domain_id: &str,
    canonical_id: &str,
    alias: &str,
    restricted_value: bool,
) -> DomainFact {
    DomainFact {
        fact_id: fact_id.to_string(),
        domain_id: domain_id.to_string(),
        canonical_id: canonical_id.to_string(),
        alias: alias.to_string(),
        namespace_id: "ns.unknown:display".to_string(),
        source_package_id: "pkg.synthetic.fact".to_string(),
        proof_refs: vec![format!("proof://{fact_id}")],
        review_refs: vec![format!("review://{fact_id}")],
        policy_refs: vec!["policy://generic-reviewed-identity-fact".to_string()],
        provenance: provenance("pkg.synthetic.fact", "source://public/facts", None),
        restricted_value,
    }
}

fn restricted_fact(alias: &str, restricted_detail: &str) -> DomainFact {
    let mut fact = fact(
        "fact.secret",
        "opaque.secret:external-777",
        "RID-777",
        alias,
        true,
    );
    fact.provenance = provenance(
        "pkg.synthetic.fact",
        "source://public/restricted-redacted",
        Some(restricted_detail),
    );
    fact
}

fn relationship(relationship_id: &str) -> DomainRelationshipFact {
    DomainRelationshipFact {
        relationship_id: relationship_id.to_string(),
        left_domain_id: "opaque.alpha:external-001".to_string(),
        right_domain_id: "opaque.beta:external-002".to_string(),
        relation_type_id: "rel.synthetic:adjacent_to".to_string(),
        source_package_id: "pkg.synthetic.vocabulary".to_string(),
        proof_refs: vec![format!("proof://{relationship_id}")],
        review_refs: vec![format!("review://{relationship_id}")],
        policy_refs: vec!["policy://generic-reviewed-relationship-fact".to_string()],
        provenance: provenance(
            "pkg.synthetic.vocabulary",
            "source://public/relations",
            None,
        ),
        restricted_value: false,
    }
}

fn provenance(
    source_package_id: &str,
    public_ref: &str,
    restricted_detail: Option<&str>,
) -> DomainProvenanceRef {
    DomainProvenanceRef {
        source_package_id: source_package_id.to_string(),
        public_ref: public_ref.to_string(),
        license_id: if restricted_detail.is_some() {
            "lic.restricted".to_string()
        } else {
            "lic.public".to_string()
        },
        restricted_detail: restricted_detail.map(ToString::to_string),
    }
}

fn digest(hex: char) -> String {
    assert!(hex.is_ascii_digit() || ('a'..='f').contains(&hex));
    format!("blake3:{}", hex.to_string().repeat(64))
}

fn _assert_compiled_is_send_sync(package: &CompiledRegistryPackage) {
    fn assert_send_sync<T: Send + Sync>(_: &T) {}
    assert_send_sync(package);
}
