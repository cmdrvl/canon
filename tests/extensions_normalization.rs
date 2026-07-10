#![forbid(unsafe_code)]

#[path = "../src/extensions/normalization.rs"]
mod normalization;

use normalization::{
    NormalizationBundle, NormalizationBundleCompatibility, NormalizationConsumerMode,
    NormalizationErrorCode, NormalizationObservationInput, NormalizationOutput,
    NormalizationPrimitive, NormalizationStepDefinition, NormalizationViewDefinition,
    NormalizationViewKind, ProtectedFeatureDefinition, RunnerExtensionPrimitive,
    RunnerVerificationMode, SafeRunnerAdapter, apply_bundle, bundle_compatibility,
    canonical_bundle_bytes, compare_protected_features, finalize_bundle,
    normalization_bundle_schema_version, views_for_mode,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.normalization.bundle.v1.schema.json");
const MODULE_SOURCE: &str = include_str!("../src/extensions/normalization.rs");

#[test]
fn schema_declares_trace_preserving_bundle_and_safe_extension_boundary() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], normalization_bundle_schema_version());
    assert_eq!(
        schema["properties"]["version"]["const"],
        normalization_bundle_schema_version()
    );
    assert_eq!(
        schema["x-canon-contract"]["raw_observation_preserved"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["safe_extension_boundary"],
        "read_only_deterministic_runner_adapters_only"
    );
    assert_eq!(
        schema["$defs"]["view"]["required"],
        serde_json::json!([
            "view_id",
            "output_kind",
            "consumer_modes",
            "protected_feature_refs",
            "steps"
        ])
    );
}

#[test]
fn two_unrelated_bundles_run_without_domain_specific_branches() {
    let bundle_a = finalize_bundle(audit_bundle()).expect("bundle a finalizes");
    let bundle_b = finalize_bundle(catalog_bundle()).expect("bundle b finalizes");

    let output_a = apply_bundle(
        &bundle_a,
        &NormalizationObservationInput {
            observation_id: "obs-a".to_string(),
            raw_value: "  Café---North // shield  ".to_string(),
        },
    )
    .expect("bundle a applies");
    let output_b = apply_bundle(
        &bundle_b,
        &NormalizationObservationInput {
            observation_id: "obs-b".to_string(),
            raw_value: "Series: 2024 Edition -- amber  ".to_string(),
        },
    )
    .expect("bundle b applies");

    assert_eq!(view(&output_a, "core_name").rendered_value, "cafe north");
    assert_eq!(
        view(&output_b, "sortable_label").rendered_value,
        "2024 amber"
    );
}

#[test]
fn unicode_punctuation_and_step_trace_are_preserved_with_rule_ids() {
    let bundle = finalize_bundle(audit_bundle()).expect("bundle finalizes");
    let output = apply_bundle(
        &bundle,
        &NormalizationObservationInput {
            observation_id: "obs-unicode".to_string(),
            raw_value: "  ÁCMÉ—North, shield!  ".to_string(),
        },
    )
    .expect("bundle applies");

    let core = view(&output, "core_name");
    assert_eq!(output.raw_value, "  ÁCMÉ—North, shield!  ");
    assert_eq!(core.rendered_value, "acme north");
    assert_eq!(
        core.trace
            .iter()
            .map(|step| step.rule_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "trim_edges",
            "fold_unicode",
            "space_punctuation",
            "collapse_ws",
            "lower_ascii",
            "drop_noise"
        ]
    );
    assert!(core.trace.iter().all(|step| !step.primitive_id.is_empty()));
}

#[test]
fn protected_token_differences_remain_available_for_antimerge_evidence() {
    let bundle = finalize_bundle(audit_bundle()).expect("bundle finalizes");
    let left = apply_bundle(
        &bundle,
        &NormalizationObservationInput {
            observation_id: "obs-left".to_string(),
            raw_value: "orbit shield".to_string(),
        },
    )
    .expect("left applies");
    let right = apply_bundle(
        &bundle,
        &NormalizationObservationInput {
            observation_id: "obs-right".to_string(),
            raw_value: "orbit".to_string(),
        },
    )
    .expect("right applies");

    let left_brand = view(&left, "brand_guard");
    let right_brand = view(&right, "brand_guard");
    let conflicts = compare_protected_features(left_brand, right_brand);

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].feature_id, "protected_brand");
    assert_eq!(conflicts[0].left_only_tokens, vec!["shield"]);
    assert!(conflicts[0].right_only_tokens.is_empty());
}

#[test]
fn cluster_and_link_modes_consume_the_same_named_view() {
    let bundle = finalize_bundle(audit_bundle()).expect("bundle finalizes");
    let output = apply_bundle(
        &bundle,
        &NormalizationObservationInput {
            observation_id: "obs-modes".to_string(),
            raw_value: "cafe north".to_string(),
        },
    )
    .expect("bundle applies");

    let cluster_views = views_for_mode(&output, NormalizationConsumerMode::Cluster);
    let link_views = views_for_mode(&output, NormalizationConsumerMode::Link);
    assert_eq!(
        cluster_views
            .iter()
            .map(|view| view.view_id.as_str())
            .collect::<Vec<_>>(),
        vec!["brand_guard", "core_name"]
    );
    assert_eq!(
        link_views
            .iter()
            .map(|view| view.view_id.as_str())
            .collect::<Vec<_>>(),
        vec!["brand_guard", "core_name"]
    );
    assert_eq!(
        cluster_views[1].rendered_value,
        link_views[1].rendered_value
    );
}

#[test]
fn empty_and_noisy_values_preserve_raw_and_emit_empty_views_deterministically() {
    let bundle = finalize_bundle(audit_bundle()).expect("bundle finalizes");
    let output = apply_bundle(
        &bundle,
        &NormalizationObservationInput {
            observation_id: "obs-noisy".to_string(),
            raw_value: " \t---!!!  ".to_string(),
        },
    )
    .expect("bundle applies");

    let core = view(&output, "core_name");
    let brand = view(&output, "brand_guard");
    assert_eq!(output.raw_value, " \t---!!!  ");
    assert_eq!(core.rendered_value, "");
    assert_eq!(brand.rendered_value, "");
    assert!(core.lossy);
    assert!(brand.lossy);
}

#[test]
fn bundle_minor_upgrades_can_be_compatible_for_required_views() {
    let locked = finalize_bundle(audit_bundle()).expect("locked bundle finalizes");
    let mut candidate = audit_bundle();
    candidate.package_version = "1.3.0".to_string();
    candidate.views.push(NormalizationViewDefinition {
        view_id: "support_signature".to_string(),
        output_kind: NormalizationViewKind::Tokens,
        consumer_modes: vec![NormalizationConsumerMode::Cluster],
        protected_feature_refs: vec![],
        steps: vec![
            NormalizationStepDefinition {
                rule_id: "trim_edges".to_string(),
                primitive: NormalizationPrimitive::AsciiTrim,
            },
            NormalizationStepDefinition {
                rule_id: "lower_ascii".to_string(),
                primitive: NormalizationPrimitive::LowercaseAscii,
            },
        ],
    });
    let candidate = finalize_bundle(candidate).expect("candidate bundle finalizes");

    assert_eq!(
        bundle_compatibility(&locked, &candidate, &["core_name", "brand_guard"])
            .expect("bundles remain compatible"),
        NormalizationBundleCompatibility::CompatibleSameMajor
    );
}

#[test]
fn malicious_or_unsupported_runner_extensions_are_rejected() {
    let mut bundle = audit_bundle();
    bundle.views[0].steps.push(NormalizationStepDefinition {
        rule_id: "evil_step".to_string(),
        primitive: NormalizationPrimitive::RunnerExtension {
            extension: RunnerExtensionPrimitive {
                extension_id: "runner.synthetic.evil".to_string(),
                package_digest: sample_hash('e'),
                verification_mode: RunnerVerificationMode::ReadOnlyVerify,
                deterministic: false,
                allows_network: true,
                writes_files: true,
                max_input_bytes: 512,
                adapter: SafeRunnerAdapter::LiteralReplace {
                    from: "alpha".to_string(),
                    to: "beta".to_string(),
                },
            },
        },
    });
    let error = finalize_bundle(bundle).expect_err("unsafe extension must fail");
    assert_eq!(error.code, NormalizationErrorCode::UnsafeExtensionPrimitive);
}

#[test]
fn safe_runner_extensions_and_declared_metamorphic_transforms_are_deterministic() {
    let bundle = finalize_bundle(catalog_bundle()).expect("bundle finalizes");
    let left = apply_bundle(
        &bundle,
        &NormalizationObservationInput {
            observation_id: "obs-same".to_string(),
            raw_value: "Series edition amber".to_string(),
        },
    )
    .expect("left applies");
    let right = apply_bundle(
        &bundle,
        &NormalizationObservationInput {
            observation_id: "obs-same".to_string(),
            raw_value: "Series edition amber".to_string(),
        },
    )
    .expect("right applies");
    let different_spacing = apply_bundle(
        &bundle,
        &NormalizationObservationInput {
            observation_id: "obs-spacing".to_string(),
            raw_value: "  Series   edition   amber ".to_string(),
        },
    )
    .expect("spacing variant applies");

    assert_eq!(canonical_json(&left), canonical_json(&right));
    assert_eq!(
        view(&left, "sortable_label").rendered_value,
        view(&different_spacing, "sortable_label").rendered_value
    );
}

#[test]
fn canonical_bundle_bytes_are_stable_across_reordered_components() {
    let left = finalize_bundle(audit_bundle()).expect("left finalizes");
    let mut right = audit_bundle();
    right.protected_features.reverse();
    right.views.reverse();
    for view in &mut right.views {
        view.consumer_modes.reverse();
        view.protected_feature_refs.reverse();
    }
    let right = finalize_bundle(right).expect("right finalizes");

    assert_eq!(
        canonical_bundle_bytes(&left).expect("left bytes"),
        canonical_bundle_bytes(&right).expect("right bytes")
    );
}

#[test]
fn source_scan_keeps_domain_dictionaries_and_unsafe_execution_out_of_core_contract() {
    let lower_source = MODULE_SOURCE.to_ascii_lowercase();
    let lower_schema = SCHEMA_JSON.to_ascii_lowercase();

    for banned in ["cusip", "isin", "lei", "servicer", "loan"] {
        assert!(
            !lower_source.contains(banned),
            "normalization module should not embed domain dictionary term {banned}"
        );
        assert!(
            !lower_schema.contains(banned),
            "normalization schema should not embed domain dictionary term {banned}"
        );
    }

    for banned in [
        "std::process::command",
        "command::new",
        "spawn(",
        "tokio::process",
    ] {
        assert!(
            !lower_source.contains(banned),
            "normalization module should not execute unsafe runner hook {banned}"
        );
    }
}

fn audit_bundle() -> NormalizationBundle {
    NormalizationBundle {
        version: String::new(),
        package_id: "pkg.synthetic.audit".to_string(),
        package_version: "1.2.3".to_string(),
        max_input_bytes: 512,
        protected_features: vec![ProtectedFeatureDefinition {
            feature_id: "protected_brand".to_string(),
            tokens: vec!["shield".to_string()],
        }],
        views: vec![
            NormalizationViewDefinition {
                view_id: "core_name".to_string(),
                output_kind: NormalizationViewKind::String,
                consumer_modes: vec![
                    NormalizationConsumerMode::Cluster,
                    NormalizationConsumerMode::Link,
                ],
                protected_feature_refs: vec!["protected_brand".to_string()],
                steps: vec![
                    NormalizationStepDefinition {
                        rule_id: "trim_edges".to_string(),
                        primitive: NormalizationPrimitive::AsciiTrim,
                    },
                    NormalizationStepDefinition {
                        rule_id: "fold_unicode".to_string(),
                        primitive: NormalizationPrimitive::LatinAsciiFold,
                    },
                    NormalizationStepDefinition {
                        rule_id: "space_punctuation".to_string(),
                        primitive: NormalizationPrimitive::PunctuationToSpace,
                    },
                    NormalizationStepDefinition {
                        rule_id: "collapse_ws".to_string(),
                        primitive: NormalizationPrimitive::CollapseWhitespace,
                    },
                    NormalizationStepDefinition {
                        rule_id: "lower_ascii".to_string(),
                        primitive: NormalizationPrimitive::LowercaseAscii,
                    },
                    NormalizationStepDefinition {
                        rule_id: "drop_noise".to_string(),
                        primitive: NormalizationPrimitive::DropLiteralTokens {
                            tokens: vec!["shield".to_string()],
                        },
                    },
                ],
            },
            NormalizationViewDefinition {
                view_id: "brand_guard".to_string(),
                output_kind: NormalizationViewKind::Tokens,
                consumer_modes: vec![
                    NormalizationConsumerMode::Cluster,
                    NormalizationConsumerMode::Link,
                ],
                protected_feature_refs: vec!["protected_brand".to_string()],
                steps: vec![
                    NormalizationStepDefinition {
                        rule_id: "trim_edges".to_string(),
                        primitive: NormalizationPrimitive::AsciiTrim,
                    },
                    NormalizationStepDefinition {
                        rule_id: "fold_unicode".to_string(),
                        primitive: NormalizationPrimitive::LatinAsciiFold,
                    },
                    NormalizationStepDefinition {
                        rule_id: "space_punctuation".to_string(),
                        primitive: NormalizationPrimitive::PunctuationToSpace,
                    },
                    NormalizationStepDefinition {
                        rule_id: "collapse_ws".to_string(),
                        primitive: NormalizationPrimitive::CollapseWhitespace,
                    },
                    NormalizationStepDefinition {
                        rule_id: "lower_ascii".to_string(),
                        primitive: NormalizationPrimitive::LowercaseAscii,
                    },
                ],
            },
        ],
    }
}

fn catalog_bundle() -> NormalizationBundle {
    NormalizationBundle {
        version: String::new(),
        package_id: "pkg.synthetic.catalog".to_string(),
        package_version: "2.1.0".to_string(),
        max_input_bytes: 512,
        protected_features: vec![ProtectedFeatureDefinition {
            feature_id: "protected_series".to_string(),
            tokens: vec!["amber".to_string()],
        }],
        views: vec![NormalizationViewDefinition {
            view_id: "sortable_label".to_string(),
            output_kind: NormalizationViewKind::String,
            consumer_modes: vec![NormalizationConsumerMode::Link],
            protected_feature_refs: vec!["protected_series".to_string()],
            steps: vec![
                NormalizationStepDefinition {
                    rule_id: "trim_edges".to_string(),
                    primitive: NormalizationPrimitive::AsciiTrim,
                },
                NormalizationStepDefinition {
                    rule_id: "fold_unicode".to_string(),
                    primitive: NormalizationPrimitive::LatinAsciiFold,
                },
                NormalizationStepDefinition {
                    rule_id: "space_punctuation".to_string(),
                    primitive: NormalizationPrimitive::PunctuationToSpace,
                },
                NormalizationStepDefinition {
                    rule_id: "collapse_ws".to_string(),
                    primitive: NormalizationPrimitive::CollapseWhitespace,
                },
                NormalizationStepDefinition {
                    rule_id: "lower_ascii".to_string(),
                    primitive: NormalizationPrimitive::LowercaseAscii,
                },
                NormalizationStepDefinition {
                    rule_id: "runner_drop_series".to_string(),
                    primitive: NormalizationPrimitive::RunnerExtension {
                        extension: RunnerExtensionPrimitive {
                            extension_id: "runner.synthetic.drop_series".to_string(),
                            package_digest: sample_hash('c'),
                            verification_mode: RunnerVerificationMode::ReadOnlyVerify,
                            deterministic: true,
                            allows_network: false,
                            writes_files: false,
                            max_input_bytes: 256,
                            adapter: SafeRunnerAdapter::DropLiteralTokens {
                                tokens: vec!["edition".to_string(), "series".to_string()],
                            },
                        },
                    },
                },
                NormalizationStepDefinition {
                    rule_id: "runner_relabel".to_string(),
                    primitive: NormalizationPrimitive::RunnerExtension {
                        extension: RunnerExtensionPrimitive {
                            extension_id: "runner.synthetic.relabel".to_string(),
                            package_digest: sample_hash('d'),
                            verification_mode: RunnerVerificationMode::ReadOnlyVerify,
                            deterministic: true,
                            allows_network: false,
                            writes_files: false,
                            max_input_bytes: 256,
                            adapter: SafeRunnerAdapter::LiteralReplace {
                                from: "special".to_string(),
                                to: "spcl".to_string(),
                            },
                        },
                    },
                },
                NormalizationStepDefinition {
                    rule_id: "sort_tokens".to_string(),
                    primitive: NormalizationPrimitive::SortTokens,
                },
            ],
        }],
    }
}

fn view<'a>(output: &'a NormalizationOutput, view_id: &str) -> &'a normalization::NormalizedView {
    output
        .views
        .iter()
        .find(|view| view.view_id == view_id)
        .unwrap_or_else(|| panic!("missing view {view_id}"))
}

fn canonical_json(output: &NormalizationOutput) -> Vec<u8> {
    serde_json::to_vec(output).expect("output serializes")
}

fn sample_hash(hex: char) -> String {
    format!(
        "blake3:{}",
        std::iter::repeat_n(hex, 64).collect::<String>()
    )
}
