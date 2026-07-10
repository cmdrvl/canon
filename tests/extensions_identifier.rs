#![forbid(unsafe_code)]

#[path = "../src/extensions/identifier.rs"]
mod identifier;

use identifier::{
    IdentifierConflictClass, IdentifierConflictDisposition, IdentifierDisplayPolicy,
    IdentifierErrorCode, IdentifierExtensionPackage, IdentifierFieldBinding,
    IdentifierNamespaceDefinition, IdentifierNamespaceRef, IdentifierNormalizationMode,
    IdentifierObservationInput, IdentifierRedactionPolicy, IdentifierReusePolicy,
    IdentifierScopeKind, IdentifierScopedCharset, IdentifierTemporalScope, IdentifierTrustPolicy,
    IdentifierTrustPolicyRef, IdentifierValidatorDefinition, IdentifierValidatorPrimitive,
    IdentifierValidatorRef, canonical_package_bytes, collect_conflicts, finalize_field_binding,
    finalize_package, identifier_extension_schema_version, interpret_identifier, namespace_digest,
    trust_policy_digest, validate_identifier_observation, validator_digest,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.extension.identifier.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/extensions/identifier.rs");

#[test]
fn schema_declares_explicit_namespace_binding_and_safe_validator_primitives() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], identifier_extension_schema_version());
    assert_eq!(
        schema["properties"]["version"]["const"],
        identifier_extension_schema_version()
    );
    assert_eq!(
        schema["$defs"]["namespace_id"]["pattern"],
        "^[a-z0-9][a-z0-9._:-]*:[a-z0-9][a-z0-9._:-]*$"
    );
    assert_eq!(
        schema["x-canon-contract"]["bare_strings_are_not_namespaced"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["validator_execution_model"],
        "declared_safe_primitives_only"
    );
}

#[test]
fn unknown_namespace_and_validator_bind_without_core_enum_changes() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let binding = binding_for(
        &package,
        "pkg.synthetic:numeric_external",
        "validator.synthetic.digits",
        "policy.synthetic.strict_numeric",
    )
    .expect("binding computes");
    let observed = validate_identifier_observation(
        &package,
        &binding,
        &IdentifierObservationInput {
            object_key: "row-1".to_string(),
            raw_value: " 0012345 ".to_string(),
            scope_key: None,
            valid_from: Some("2025-01-01T00:00:00Z".to_string()),
            valid_to: Some("2025-12-31T00:00:00Z".to_string()),
            source_ref: "feed.synthetic.numeric".to_string(),
        },
    )
    .expect("identifier validates");

    assert_eq!(observed.namespace_id, "pkg.synthetic:numeric_external");
    assert_eq!(observed.validator_id, "validator.synthetic.digits");
    assert_eq!(observed.normalized_value, "0012345");
}

#[test]
fn bare_strings_are_rejected_without_declared_namespace_binding() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let error = interpret_identifier(
        &package,
        None,
        &IdentifierObservationInput {
            object_key: "row-1".to_string(),
            raw_value: "12345".to_string(),
            scope_key: None,
            valid_from: None,
            valid_to: None,
            source_ref: "feed.synthetic.raw".to_string(),
        },
    )
    .expect_err("missing namespace binding must fail");
    assert_eq!(
        error.code,
        IdentifierErrorCode::NamespaceDeclarationRequired
    );
}

#[test]
fn numeric_alphanumeric_checksum_and_scoped_validators_are_extensible_and_resource_limited() {
    let package = finalize_package(sample_package()).expect("package finalizes");

    let numeric = validate_identifier_observation(
        &package,
        &binding_for(
            &package,
            "pkg.synthetic:numeric_external",
            "validator.synthetic.digits",
            "policy.synthetic.strict_numeric",
        )
        .unwrap(),
        &IdentifierObservationInput {
            object_key: "row-numeric".to_string(),
            raw_value: " 0009981 ".to_string(),
            scope_key: None,
            valid_from: None,
            valid_to: None,
            source_ref: "feed.synthetic.numeric".to_string(),
        },
    )
    .expect("numeric identifier validates");
    assert_eq!(numeric.normalized_value, "0009981");

    let alphanumeric = validate_identifier_observation(
        &package,
        &binding_for(
            &package,
            "pkg.synthetic:alpha_code",
            "validator.synthetic.alnum",
            "policy.synthetic.shared",
        )
        .unwrap(),
        &IdentifierObservationInput {
            object_key: "row-alpha".to_string(),
            raw_value: " ab12z9 ".to_string(),
            scope_key: None,
            valid_from: None,
            valid_to: None,
            source_ref: "feed.synthetic.alnum".to_string(),
        },
    )
    .expect("alphanumeric identifier validates");
    assert_eq!(alphanumeric.normalized_value, "AB12Z9");

    let checksum = validate_identifier_observation(
        &package,
        &binding_for(
            &package,
            "pkg.synthetic:check_digit",
            "validator.synthetic.luhn",
            "policy.synthetic.shared",
        )
        .unwrap(),
        &IdentifierObservationInput {
            object_key: "row-luhn".to_string(),
            raw_value: "79927398713".to_string(),
            scope_key: None,
            valid_from: None,
            valid_to: None,
            source_ref: "feed.synthetic.luhn".to_string(),
        },
    )
    .expect("checksum identifier validates");
    assert_eq!(checksum.normalized_value, "79927398713");

    let scoped = validate_identifier_observation(
        &package,
        &binding_for(
            &package,
            "pkg.synthetic:scoped_code",
            "validator.synthetic.scoped",
            "policy.synthetic.shared",
        )
        .unwrap(),
        &IdentifierObservationInput {
            object_key: "row-scoped".to_string(),
            raw_value: "us:ab12".to_string(),
            scope_key: Some("tenant-us".to_string()),
            valid_from: None,
            valid_to: None,
            source_ref: "feed.synthetic.scoped".to_string(),
        },
    )
    .expect("scoped identifier validates");
    assert_eq!(scoped.normalized_value, "US:AB12");

    let bad_inputs = [
        "contains space",
        "lower!punct",
        "999999999999999999999999999999999999999999999",
        "abc:def:ghi",
        "79927398714",
    ];
    for input in bad_inputs {
        let error = validate_identifier_observation(
            &package,
            &binding_for(
                &package,
                "pkg.synthetic:check_digit",
                "validator.synthetic.luhn",
                "policy.synthetic.shared",
            )
            .unwrap(),
            &IdentifierObservationInput {
                object_key: "row-bad".to_string(),
                raw_value: input.to_string(),
                scope_key: None,
                valid_from: None,
                valid_to: None,
                source_ref: "feed.synthetic.fuzz".to_string(),
            },
        )
        .expect_err("invalid input should be rejected");
        assert!(
            matches!(
                error.code,
                IdentifierErrorCode::ValidationFailed | IdentifierErrorCode::ResourceLimitExceeded
            ),
            "unexpected code for {input}: {:?}",
            error.code
        );
    }

    let overlong = "A".repeat(1024);
    let error = validate_identifier_observation(
        &package,
        &binding_for(
            &package,
            "pkg.synthetic:alpha_code",
            "validator.synthetic.alnum",
            "policy.synthetic.shared",
        )
        .unwrap(),
        &IdentifierObservationInput {
            object_key: "row-overlong".to_string(),
            raw_value: overlong,
            scope_key: None,
            valid_from: None,
            valid_to: None,
            source_ref: "feed.synthetic.fuzz".to_string(),
        },
    )
    .expect_err("resource limit should trigger");
    assert_eq!(error.code, IdentifierErrorCode::ResourceLimitExceeded);
}

#[test]
fn redacted_namespace_masks_display_without_changing_stable_fingerprint() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let binding = binding_for(
        &package,
        "pkg.synthetic:redacted_code",
        "validator.synthetic.alnum",
        "policy.synthetic.shared",
    )
    .expect("binding computes");

    let left = validate_identifier_observation(
        &package,
        &binding,
        &IdentifierObservationInput {
            object_key: "row-redacted-1".to_string(),
            raw_value: " abcd1234 ".to_string(),
            scope_key: None,
            valid_from: None,
            valid_to: None,
            source_ref: "feed.synthetic.redacted".to_string(),
        },
    )
    .expect("left validates");
    let right = validate_identifier_observation(
        &package,
        &binding,
        &IdentifierObservationInput {
            object_key: "row-redacted-2".to_string(),
            raw_value: "ABCD1234".to_string(),
            scope_key: None,
            valid_from: None,
            valid_to: None,
            source_ref: "feed.synthetic.redacted".to_string(),
        },
    )
    .expect("right validates");

    assert_eq!(left.rendered_value, "****1234");
    assert_eq!(left.rendered_value, right.rendered_value);
    assert_eq!(left.stable_fingerprint, right.stable_fingerprint);
}

#[test]
fn conflicting_exclusive_and_recycled_identifiers_emit_typed_evidence() {
    let package = finalize_package(sample_package()).expect("package finalizes");

    let strict_binding = binding_for(
        &package,
        "pkg.synthetic:numeric_external",
        "validator.synthetic.digits",
        "policy.synthetic.strict_numeric",
    )
    .expect("strict binding computes");
    let conflicts = collect_conflicts(
        &package,
        &strict_binding,
        &[
            IdentifierObservationInput {
                object_key: "entity-1".to_string(),
                raw_value: "1234567".to_string(),
                scope_key: None,
                valid_from: Some("2025-01-01T00:00:00Z".to_string()),
                valid_to: Some("2025-12-31T00:00:00Z".to_string()),
                source_ref: "feed.synthetic.a".to_string(),
            },
            IdentifierObservationInput {
                object_key: "entity-1".to_string(),
                raw_value: "7654321".to_string(),
                scope_key: None,
                valid_from: Some("2025-01-01T00:00:00Z".to_string()),
                valid_to: Some("2025-12-31T00:00:00Z".to_string()),
                source_ref: "feed.synthetic.b".to_string(),
            },
        ],
    )
    .expect("exclusive conflict computes");
    assert!(conflicts.iter().any(|conflict| {
        conflict.class == IdentifierConflictClass::ExclusiveIdentifierConflict
            && conflict.disposition == IdentifierConflictDisposition::AntiMerge
    }));

    let recycled_binding = binding_for(
        &package,
        "pkg.synthetic:recycled_badge",
        "validator.synthetic.digits",
        "policy.synthetic.recyclable",
    )
    .expect("recycled binding computes");
    let recycled = collect_conflicts(
        &package,
        &recycled_binding,
        &[
            IdentifierObservationInput {
                object_key: "entity-old".to_string(),
                raw_value: "0001111".to_string(),
                scope_key: None,
                valid_from: Some("2022-01-01T00:00:00Z".to_string()),
                valid_to: Some("2022-12-31T00:00:00Z".to_string()),
                source_ref: "feed.synthetic.legacy".to_string(),
            },
            IdentifierObservationInput {
                object_key: "entity-new".to_string(),
                raw_value: "0001111".to_string(),
                scope_key: None,
                valid_from: Some("2024-01-01T00:00:00Z".to_string()),
                valid_to: Some("2024-12-31T00:00:00Z".to_string()),
                source_ref: "feed.synthetic.current".to_string(),
            },
        ],
    )
    .expect("recycled conflict computes");
    assert!(recycled.iter().any(|conflict| {
        conflict.class == IdentifierConflictClass::RecycledIdentifier
            && conflict.disposition == IdentifierConflictDisposition::HistoricalOnly
    }));
}

#[test]
fn namespace_validator_and_trust_policy_refs_invalidate_independently() {
    let package = finalize_package(sample_package()).expect("package finalizes");
    let binding = binding_for(
        &package,
        "pkg.synthetic:alpha_code",
        "validator.synthetic.alnum",
        "policy.synthetic.shared",
    )
    .expect("binding computes");
    finalize_field_binding(&package, binding.clone()).expect("original binding resolves");

    let mut namespace_changed = sample_package();
    namespace_changed.namespaces[1].display_policy = IdentifierDisplayPolicy::Last4;
    let namespace_changed =
        finalize_package(namespace_changed).expect("namespace-changed package finalizes");
    let namespace_error = finalize_field_binding(&namespace_changed, binding.clone())
        .expect_err("namespace digest must change");
    assert_eq!(namespace_error.code, IdentifierErrorCode::DigestMismatch);
    assert!(namespace_error.message.contains("namespace"));

    let mut validator_changed = sample_package();
    validator_changed.validators[1].max_input_bytes = 9;
    let validator_changed =
        finalize_package(validator_changed).expect("validator-changed package finalizes");
    let validator_error = finalize_field_binding(&validator_changed, binding.clone())
        .expect_err("validator digest must change");
    assert_eq!(validator_error.code, IdentifierErrorCode::DigestMismatch);
    assert!(validator_error.message.contains("validator"));

    let mut trust_changed = sample_package();
    trust_changed.trust_policies[1]
        .source_trust_hints
        .push("operator_attested_only".to_string());
    let trust_changed = finalize_package(trust_changed).expect("trust-changed package finalizes");
    let trust_error = finalize_field_binding(&trust_changed, binding)
        .expect_err("trust-policy digest must change");
    assert_eq!(trust_error.code, IdentifierErrorCode::DigestMismatch);
    assert!(trust_error.message.contains("trust policy"));
}

#[test]
fn package_bytes_are_stable_across_reordered_components() {
    let left = finalize_package(sample_package()).expect("left finalizes");
    let mut right = sample_package();
    right.namespaces.reverse();
    right.validators.reverse();
    right.trust_policies.reverse();
    let right = finalize_package(right).expect("right finalizes");

    let left_bytes = canonical_package_bytes(&left).expect("left bytes");
    let right_bytes = canonical_package_bytes(&right).expect("right bytes");
    assert_eq!(left_bytes, right_bytes);
}

#[test]
fn source_scan_keeps_real_authority_lists_and_code_execution_out_of_core_contract() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();

    for banned in ["cusip", "isin", "lei", "passport", "social_security"] {
        assert!(
            !lower_source.contains(banned),
            "identifier module should not embed real authority {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "identifier schema should not embed real authority {banned}"
        );
    }

    for banned in ["std::process::command", "command::new", "spawn("] {
        assert!(
            !lower_source.contains(banned),
            "identifier validator contract should not execute undeclared code via {banned}"
        );
    }
}

fn sample_package() -> IdentifierExtensionPackage {
    IdentifierExtensionPackage {
        version: String::new(),
        package_id: "pkg.synthetic".to_string(),
        package_version: "1.2.3".to_string(),
        namespaces: vec![
            IdentifierNamespaceDefinition {
                namespace_id: "pkg.synthetic:numeric_external".to_string(),
                namespace_uri: "urn:synthetic:identifier:numeric_external".to_string(),
                applicable_object_types: vec!["object.synthetic.record".to_string()],
                normalization: IdentifierNormalizationMode::Trim,
                temporal_scope: IdentifierTemporalScope::ValidTimeOptional,
                scope_kind: IdentifierScopeKind::Global,
                display_policy: IdentifierDisplayPolicy::Full,
                redaction_policy: IdentifierRedactionPolicy::CleartextAllowed,
            },
            IdentifierNamespaceDefinition {
                namespace_id: "pkg.synthetic:alpha_code".to_string(),
                namespace_uri: "urn:synthetic:identifier:alpha_code".to_string(),
                applicable_object_types: vec!["object.synthetic.record".to_string()],
                normalization: IdentifierNormalizationMode::Upper,
                temporal_scope: IdentifierTemporalScope::Persistent,
                scope_kind: IdentifierScopeKind::Global,
                display_policy: IdentifierDisplayPolicy::Full,
                redaction_policy: IdentifierRedactionPolicy::CleartextAllowed,
            },
            IdentifierNamespaceDefinition {
                namespace_id: "pkg.synthetic:check_digit".to_string(),
                namespace_uri: "urn:synthetic:identifier:check_digit".to_string(),
                applicable_object_types: vec!["object.synthetic.record".to_string()],
                normalization: IdentifierNormalizationMode::Trim,
                temporal_scope: IdentifierTemporalScope::Persistent,
                scope_kind: IdentifierScopeKind::Global,
                display_policy: IdentifierDisplayPolicy::Full,
                redaction_policy: IdentifierRedactionPolicy::CleartextAllowed,
            },
            IdentifierNamespaceDefinition {
                namespace_id: "pkg.synthetic:scoped_code".to_string(),
                namespace_uri: "urn:synthetic:identifier:scoped_code".to_string(),
                applicable_object_types: vec!["object.synthetic.record".to_string()],
                normalization: IdentifierNormalizationMode::Upper,
                temporal_scope: IdentifierTemporalScope::Persistent,
                scope_kind: IdentifierScopeKind::ScopedByDeclaredKey,
                display_policy: IdentifierDisplayPolicy::Full,
                redaction_policy: IdentifierRedactionPolicy::CleartextAllowed,
            },
            IdentifierNamespaceDefinition {
                namespace_id: "pkg.synthetic:redacted_code".to_string(),
                namespace_uri: "urn:synthetic:identifier:redacted_code".to_string(),
                applicable_object_types: vec!["object.synthetic.record".to_string()],
                normalization: IdentifierNormalizationMode::Upper,
                temporal_scope: IdentifierTemporalScope::Persistent,
                scope_kind: IdentifierScopeKind::Global,
                display_policy: IdentifierDisplayPolicy::Full,
                redaction_policy: IdentifierRedactionPolicy::MaskAllButLast4,
            },
            IdentifierNamespaceDefinition {
                namespace_id: "pkg.synthetic:recycled_badge".to_string(),
                namespace_uri: "urn:synthetic:identifier:recycled_badge".to_string(),
                applicable_object_types: vec!["object.synthetic.record".to_string()],
                normalization: IdentifierNormalizationMode::Trim,
                temporal_scope: IdentifierTemporalScope::HistoricalReuse,
                scope_kind: IdentifierScopeKind::Global,
                display_policy: IdentifierDisplayPolicy::Full,
                redaction_policy: IdentifierRedactionPolicy::CleartextAllowed,
            },
        ],
        validators: vec![
            IdentifierValidatorDefinition {
                validator_id: "validator.synthetic.digits".to_string(),
                max_input_bytes: 32,
                checks: vec![IdentifierValidatorPrimitive::Digits {
                    min_length: 7,
                    max_length: 7,
                }],
            },
            IdentifierValidatorDefinition {
                validator_id: "validator.synthetic.alnum".to_string(),
                max_input_bytes: 32,
                checks: vec![IdentifierValidatorPrimitive::AsciiAlphanumeric {
                    min_length: 6,
                    max_length: 8,
                }],
            },
            IdentifierValidatorDefinition {
                validator_id: "validator.synthetic.luhn".to_string(),
                max_input_bytes: 32,
                checks: vec![IdentifierValidatorPrimitive::LuhnChecksum {
                    min_length: 11,
                    max_length: 11,
                }],
            },
            IdentifierValidatorDefinition {
                validator_id: "validator.synthetic.scoped".to_string(),
                max_input_bytes: 32,
                checks: vec![IdentifierValidatorPrimitive::ScopedSegments {
                    separator: ':',
                    segment_count: 2,
                    segment_min_length: 2,
                    segment_max_length: 4,
                    charset: IdentifierScopedCharset::UpperAlnum,
                }],
            },
        ],
        trust_policies: vec![
            IdentifierTrustPolicy {
                policy_id: "policy.synthetic.strict_numeric".to_string(),
                single_value_per_object: true,
                single_object_per_value: true,
                reuse_policy: IdentifierReusePolicy::Never,
                source_trust_hints: vec![
                    "primary_source_preferred".to_string(),
                    "exclusive_current_value".to_string(),
                ],
            },
            IdentifierTrustPolicy {
                policy_id: "policy.synthetic.shared".to_string(),
                single_value_per_object: false,
                single_object_per_value: false,
                reuse_policy: IdentifierReusePolicy::Never,
                source_trust_hints: vec!["shared_lookup_ok".to_string()],
            },
            IdentifierTrustPolicy {
                policy_id: "policy.synthetic.recyclable".to_string(),
                single_value_per_object: false,
                single_object_per_value: true,
                reuse_policy: IdentifierReusePolicy::AllowHistoricalNonOverlapping,
                source_trust_hints: vec![
                    "history_required".to_string(),
                    "non_overlapping_reuse_only".to_string(),
                ],
            },
        ],
    }
}

fn binding_for(
    package: &IdentifierExtensionPackage,
    namespace_id: &str,
    validator_id: &str,
    policy_id: &str,
) -> Result<IdentifierFieldBinding, String> {
    let namespace = package
        .namespaces
        .iter()
        .find(|namespace| namespace.namespace_id == namespace_id)
        .ok_or_else(|| format!("missing namespace {namespace_id}"))?;
    let validator = package
        .validators
        .iter()
        .find(|validator| validator.validator_id == validator_id)
        .ok_or_else(|| format!("missing validator {validator_id}"))?;
    let policy = package
        .trust_policies
        .iter()
        .find(|policy| policy.policy_id == policy_id)
        .ok_or_else(|| format!("missing policy {policy_id}"))?;

    Ok(IdentifierFieldBinding {
        field_path: "identifiers.synthetic".to_string(),
        object_type: "object.synthetic.record".to_string(),
        namespace: IdentifierNamespaceRef {
            package_id: package.package_id.clone(),
            namespace_id: namespace.namespace_id.clone(),
            namespace_digest: namespace_digest(namespace).map_err(|error| error.to_string())?,
        },
        validator: IdentifierValidatorRef {
            package_id: package.package_id.clone(),
            validator_id: validator.validator_id.clone(),
            validator_digest: validator_digest(validator).map_err(|error| error.to_string())?,
        },
        trust_policy: IdentifierTrustPolicyRef {
            package_id: package.package_id.clone(),
            policy_id: policy.policy_id.clone(),
            policy_digest: trust_policy_digest(policy).map_err(|error| error.to_string())?,
        },
    })
}
