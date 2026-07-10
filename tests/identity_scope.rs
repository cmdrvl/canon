#![forbid(unsafe_code)]

#[path = "../src/identity_scope.rs"]
mod identity_scope;

use identity_scope::{
    CANON_IDENTITY_SCOPE_VERSION, CanonicalTypeRef, CoreCanonicalTypeClass,
    CoreIdentifierNamespaceClass, CoreScopeDimension, CrossScopeAliasPolicy,
    ExactLookupQualification, IdentifierNamespaceRef, IdentityCompatibility, IdentityFactHeader,
    IdentityScope, IdentityScopeErrorCode, IdentitySnapshotHeader, QualifiedIdentityRef,
    ScopeBinding, ScopeDimensionBinding, ScopeDimensionRef, authorize_cross_scope_alias,
    canonical_qualified_identity_bytes, finalize_fact_header, finalize_qualified_identity,
    finalize_scope, finalize_snapshot_header, identity_compatibility, qualify_exact_lookup,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.identity.scope.v1.schema.json");

#[test]
fn schema_declares_scope_and_header_contracts() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], CANON_IDENTITY_SCOPE_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_IDENTITY_SCOPE_VERSION
    );
    assert!(
        schema["description"]
            .as_str()
            .unwrap()
            .contains("prevent accidental reuse")
    );
    assert_eq!(
        schema["$defs"]["fact_header"]["required"],
        serde_json::json!(["canonical_type", "namespace", "scope"])
    );
    assert_eq!(
        schema["$defs"]["snapshot_header"]["required"],
        serde_json::json!(["canonical_type", "namespace", "scope"])
    );
    assert_eq!(
        schema["$defs"]["extension_vocabulary_ref"]["properties"]["package_digest"]["$ref"],
        "#/$defs/blake3_hash"
    );
    assert_eq!(
        schema["$defs"]["blake3_hash"]["pattern"],
        "^blake3:[0-9a-f]{64}$"
    );
}

#[test]
fn invented_namespace_collisions_are_incompatible() {
    let left = sample_identity("ABC-123", "opaque_record", "loan_ticket");
    let right = sample_identity("ABC-123", "opaque_record", "property_ticket");

    assert_eq!(
        identity_compatibility(&left, &right).expect("compatibility evaluates"),
        IdentityCompatibility::Incompatible
    );
    assert_eq!(
        qualify_exact_lookup(&left, &right).expect("lookup qualification evaluates"),
        ExactLookupQualification::Incompatible
    );
}

#[test]
fn jurisdiction_changes_do_not_silently_reuse() {
    let left = QualifiedIdentityRef {
        scope: scope(vec![
            exact_dimension(CoreScopeDimension::Dataset, "sec10d"),
            exact_dimension(CoreScopeDimension::Jurisdiction, "us"),
        ]),
        ..sample_identity("LEI-123", "issuer", "external-id")
    };
    let right = QualifiedIdentityRef {
        scope: scope(vec![
            exact_dimension(CoreScopeDimension::Dataset, "sec10d"),
            exact_dimension(CoreScopeDimension::Jurisdiction, "ca"),
        ]),
        ..sample_identity("LEI-123", "issuer", "external-id")
    };

    assert_eq!(
        identity_compatibility(&left, &right).expect("compatibility evaluates"),
        IdentityCompatibility::Incompatible
    );
}

#[test]
fn source_local_ids_require_source_system_scope() {
    let invalid = QualifiedIdentityRef {
        namespace: IdentifierNamespaceRef::Core {
            class: CoreIdentifierNamespaceClass::SourceLocalId,
        },
        scope: scope(vec![exact_dimension(
            CoreScopeDimension::Dataset,
            "servicer_feed",
        )]),
        ..sample_identity("12345", "organization_local", "ignored")
    };
    let error =
        finalize_qualified_identity(invalid, None).expect_err("missing source system fails");
    assert_eq!(error.code, IdentityScopeErrorCode::ArtifactContract);

    let valid = QualifiedIdentityRef {
        namespace: IdentifierNamespaceRef::Core {
            class: CoreIdentifierNamespaceClass::SourceLocalId,
        },
        scope: scope(vec![exact_dimension(
            CoreScopeDimension::SourceSystem,
            "servicer_a",
        )]),
        ..sample_identity("12345", "organization_local", "ignored")
    };
    let finalized = finalize_qualified_identity(valid, None).expect("source local id finalizes");
    assert_eq!(
        qualify_exact_lookup(&finalized, &finalized).unwrap(),
        ExactLookupQualification::QualifiedMatch
    );
}

#[test]
fn unknown_scope_requires_explicit_evidence() {
    let left = QualifiedIdentityRef {
        scope: scope(vec![ScopeDimensionBinding {
            dimension: ScopeDimensionRef::Core {
                dimension: CoreScopeDimension::Jurisdiction,
            },
            binding: ScopeBinding::Unknown,
        }]),
        ..sample_identity("ID-7", "issuer", "external-id")
    };
    let right = QualifiedIdentityRef {
        scope: scope(vec![exact_dimension(
            CoreScopeDimension::Jurisdiction,
            "us",
        )]),
        ..sample_identity("ID-7", "issuer", "external-id")
    };

    assert_eq!(
        identity_compatibility(&left, &right).expect("compatibility evaluates"),
        IdentityCompatibility::RequiresExplicitEvidence
    );
    assert_eq!(
        qualify_exact_lookup(&left, &right).expect("lookup qualification evaluates"),
        ExactLookupQualification::RequiresExplicitEvidence
    );
}

#[test]
fn incompatible_types_do_not_collide() {
    let person = QualifiedIdentityRef {
        canonical_type: CanonicalTypeRef::Core {
            class: CoreCanonicalTypeClass::Person,
        },
        ..sample_identity("alpha", "shared_namespace", "shared_namespace")
    };
    let organization = QualifiedIdentityRef {
        canonical_type: CanonicalTypeRef::Core {
            class: CoreCanonicalTypeClass::Organization,
        },
        ..sample_identity("alpha", "shared_namespace", "shared_namespace")
    };

    assert_eq!(
        identity_compatibility(&person, &organization).expect("compatibility evaluates"),
        IdentityCompatibility::Incompatible
    );
}

#[test]
fn extension_package_pins_prevent_vocabulary_drift() {
    let left = sample_identity("CUSIP-1", "issuer", "external-id");
    let right = QualifiedIdentityRef {
        canonical_type: CanonicalTypeRef::Extension {
            package_digest: sample_hash('b'),
            vocabulary: "canonical_type".to_string(),
            value: "issuer".to_string(),
        },
        ..left.clone()
    };

    assert_eq!(
        identity_compatibility(&left, &right).expect("compatibility evaluates"),
        IdentityCompatibility::Incompatible
    );
}

#[test]
fn scope_inheritance_is_resolved_deterministically() {
    let parent = scope(vec![
        exact_dimension(CoreScopeDimension::Dataset, "sec10d"),
        exact_dimension(CoreScopeDimension::Jurisdiction, "us"),
    ]);
    let inherited = scope(vec![
        ScopeDimensionBinding {
            dimension: ScopeDimensionRef::Core {
                dimension: CoreScopeDimension::Dataset,
            },
            binding: ScopeBinding::Inherit,
        },
        exact_dimension(CoreScopeDimension::Profile, "issuer_mentions"),
    ]);

    let resolved = finalize_scope(inherited, Some(&parent)).expect("scope inherits");
    assert_eq!(
        resolved.dimensions[0],
        exact_dimension(CoreScopeDimension::Dataset, "sec10d")
    );
    assert_eq!(
        resolved.dimensions[1],
        exact_dimension(CoreScopeDimension::Profile, "issuer_mentions")
    );

    let identity = QualifiedIdentityRef {
        scope: resolved.clone(),
        ..sample_identity("alpha", "issuer", "external-id")
    };
    let bytes_a = canonical_qualified_identity_bytes(&identity).expect("identity serializes");
    let bytes_b = canonical_qualified_identity_bytes(&QualifiedIdentityRef {
        scope: scope(vec![
            exact_dimension(CoreScopeDimension::Profile, "issuer_mentions"),
            exact_dimension(CoreScopeDimension::Dataset, "sec10d"),
        ]),
        ..sample_identity("alpha", "issuer", "external-id")
    })
    .expect("identity serializes");
    assert_eq!(bytes_a, bytes_b);
}

#[test]
fn cross_scope_alias_requires_policy_and_evidence() {
    let broader = QualifiedIdentityRef {
        scope: scope(vec![exact_dimension(CoreScopeDimension::Dataset, "sec10d")]),
        ..sample_identity("alpha", "issuer", "external-id")
    };
    let narrower = QualifiedIdentityRef {
        scope: scope(vec![
            exact_dimension(CoreScopeDimension::Dataset, "sec10d"),
            exact_dimension(CoreScopeDimension::Profile, "issuer_mentions"),
        ]),
        ..sample_identity("alpha", "issuer", "external-id")
    };

    let error = authorize_cross_scope_alias(
        &broader,
        &narrower,
        CrossScopeAliasPolicy::SameScopeOnly,
        None,
    )
    .expect_err("same-scope-only policy rejects cross-scope alias");
    assert_eq!(error.code, IdentityScopeErrorCode::CompatibilityPolicy);

    let compatibility = authorize_cross_scope_alias(
        &broader,
        &narrower,
        CrossScopeAliasPolicy::RequireExplicitEvidence,
        Some("blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"),
    )
    .expect("explicit evidence authorizes alias");
    assert_eq!(compatibility, IdentityCompatibility::Compatible);
}

#[test]
fn exact_lookup_requires_equal_scope_not_broader_compatibility() {
    let broader = QualifiedIdentityRef {
        scope: scope(vec![exact_dimension(CoreScopeDimension::Dataset, "sec10d")]),
        ..sample_identity("alpha", "issuer", "external-id")
    };
    let narrower = QualifiedIdentityRef {
        scope: scope(vec![
            exact_dimension(CoreScopeDimension::Dataset, "sec10d"),
            exact_dimension(CoreScopeDimension::Profile, "issuer_mentions"),
        ]),
        ..sample_identity("alpha", "issuer", "external-id")
    };

    assert_eq!(
        identity_compatibility(&broader, &narrower).unwrap(),
        IdentityCompatibility::Compatible
    );
    assert_eq!(
        qualify_exact_lookup(&broader, &narrower).unwrap(),
        ExactLookupQualification::RequiresExplicitEvidence
    );
}

#[test]
fn fact_and_snapshot_headers_require_type_namespace_and_scope() {
    let fact = finalize_fact_header(
        IdentityFactHeader {
            canonical_type: CanonicalTypeRef::Core {
                class: CoreCanonicalTypeClass::Organization,
            },
            namespace: IdentifierNamespaceRef::Core {
                class: CoreIdentifierNamespaceClass::CanonicalId,
            },
            scope: scope(vec![exact_dimension(CoreScopeDimension::Dataset, "sec10d")]),
        },
        None,
    )
    .expect("fact header finalizes");
    assert_eq!(fact.scope.dimensions.len(), 1);

    let snapshot = finalize_snapshot_header(
        IdentitySnapshotHeader {
            canonical_type: CanonicalTypeRef::Core {
                class: CoreCanonicalTypeClass::Organization,
            },
            namespace: IdentifierNamespaceRef::Core {
                class: CoreIdentifierNamespaceClass::CanonicalId,
            },
            scope: scope(vec![exact_dimension(CoreScopeDimension::Dataset, "sec10d")]),
        },
        None,
    )
    .expect("snapshot header finalizes");
    assert_eq!(snapshot.scope.dimensions.len(), 1);
}

fn sample_identity(
    identifier_value: &str,
    canonical_type: &str,
    namespace: &str,
) -> QualifiedIdentityRef {
    QualifiedIdentityRef {
        version: String::new(),
        identifier_value: identifier_value.to_string(),
        canonical_type: CanonicalTypeRef::Extension {
            package_digest: sample_hash('a'),
            vocabulary: "canonical_type".to_string(),
            value: canonical_type.to_string(),
        },
        namespace: IdentifierNamespaceRef::Extension {
            package_digest: sample_hash('c'),
            vocabulary: "identifier_namespace".to_string(),
            value: namespace.to_string(),
        },
        scope: IdentityScope::default(),
    }
}

fn scope(dimensions: Vec<ScopeDimensionBinding>) -> IdentityScope {
    IdentityScope { dimensions }
}

fn exact_dimension(dimension: CoreScopeDimension, value: &str) -> ScopeDimensionBinding {
    ScopeDimensionBinding {
        dimension: ScopeDimensionRef::Core { dimension },
        binding: ScopeBinding::Exact {
            value: value.to_string(),
        },
    }
}

fn sample_hash(hex: char) -> String {
    format!(
        "blake3:{}",
        std::iter::repeat_n(hex, 64).collect::<String>()
    )
}
