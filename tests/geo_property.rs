#![forbid(unsafe_code)]

use canon::geo::property::{
    CANON_GEO_PROPERTY_ASSERTION_REQUEST_VERSION, GeoPropertyAssertionErrorCode,
    GeoPropertyAssertionProofClass, GeoPropertyAssertionRequest, GeoPropertyBlockingStrategy,
    GeoPropertyDocumentAssertionRequest, GeoPropertyMemberAssertion,
    GeoPropertyMembershipAbstentionReason, GeoPropertyMembershipStatus,
    GeoPropertyRelationGraphProduct, GeoPropertySourceCorpus, GeoPropertySourceRecordRef,
    materialize_property_assertions,
};
use canon::geo::{
    GeoControlRelation, GeoEntityLevel, GeoEntityRef, GeoPropertySetRelation,
    canonical_property_assertion_bytes,
};
use serde_json::Value;

fn digest(byte: u8) -> String {
    format!("blake3:{}", blake3::hash(&[byte]).to_hex())
}

fn source_record(id: &str, byte: u8) -> GeoPropertySourceRecordRef {
    GeoPropertySourceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "fixture-2026-09-03".to_string(),
        record_blake3: digest(byte),
    }
}

fn entity(level: GeoEntityLevel, id: &str) -> GeoEntityRef {
    GeoEntityRef {
        level,
        id: id.to_string(),
    }
}

fn member(
    level: GeoEntityLevel,
    id: &str,
    tile_id: &str,
    status: GeoPropertyMembershipStatus,
    reason: Option<GeoPropertyMembershipAbstentionReason>,
    source_suffix: &str,
) -> GeoPropertyMemberAssertion {
    GeoPropertyMemberAssertion {
        member: entity(level, id),
        tile_id: tile_id.to_string(),
        status,
        abstention_reason: reason,
        source_record: source_record(
            &format!("fixture.property.member:{source_suffix}"),
            source_suffix.as_bytes()[0],
        ),
    }
}

fn corpus() -> GeoPropertySourceCorpus {
    GeoPropertySourceCorpus {
        corpus_id: "fixture.cmbs.annex_a".to_string(),
        corpus_version: "2026-09-03".to_string(),
        temporal_scope: "document_valid_time".to_string(),
        native_key_fields: vec![
            "accession".to_string(),
            "deal_id".to_string(),
            "loan_id".to_string(),
        ],
    }
}

fn assertion(
    suffix: &str,
    accession: &str,
    loan_id: &str,
    members: Vec<GeoPropertyMemberAssertion>,
) -> GeoPropertyDocumentAssertionRequest {
    GeoPropertyDocumentAssertionRequest {
        assertion_id: format!("assertion-{suffix}"),
        document_id: format!("document-{suffix}"),
        accession: accession.to_string(),
        deal_id: "fixture-deal".to_string(),
        loan_id: loan_id.to_string(),
        collateral_set_id: format!("collateral:{accession}:{loan_id}"),
        source_record: source_record(&format!("fixture.property.document:{suffix}"), b'a'),
        members,
    }
}

fn request(assertions: Vec<GeoPropertyDocumentAssertionRequest>) -> GeoPropertyAssertionRequest {
    GeoPropertyAssertionRequest {
        version: CANON_GEO_PROPERTY_ASSERTION_REQUEST_VERSION.to_string(),
        proof_class: GeoPropertyAssertionProofClass::Fixture,
        blocking_strategy: GeoPropertyBlockingStrategy::DocumentFirstThenGeography,
        relation_graph_product: GeoPropertyRelationGraphProduct::PublishedDerivedProjection,
        source_corpus: corpus(),
        max_assertions: 8,
        max_members_per_assertion: 8,
        max_pairwise_comparisons: 28,
        assertions,
    }
}

#[test]
fn document_scoped_property_spans_two_tiles_without_fanout() {
    let parcel_a = "cmdrvl:parcel:01J7X00000000000000PA";
    let parcel_b = "cmdrvl:parcel:01J7X00000000000000PB";
    let parcel_c = "cmdrvl:parcel:01J7X00000000000000PC";
    let artifact = materialize_property_assertions(&request(vec![assertion(
        "span-two-tiles",
        "0000000000-26-000101",
        "loan-101",
        vec![
            member(
                GeoEntityLevel::Parcel,
                parcel_a,
                "h3:r8:alpha",
                GeoPropertyMembershipStatus::AssertedMember,
                None,
                "a",
            ),
            member(
                GeoEntityLevel::Parcel,
                parcel_b,
                "h3:r8:alpha",
                GeoPropertyMembershipStatus::AssertedMember,
                None,
                "b",
            ),
            member(
                GeoEntityLevel::Parcel,
                parcel_c,
                "h3:r8:beta",
                GeoPropertyMembershipStatus::AssertedMember,
                None,
                "c",
            ),
        ],
    )]))
    .expect("document-scoped property assertion materializes");

    assert_eq!(artifact.summary.document_assertions, 1);
    assert_eq!(artifact.summary.property_entities, 1);
    assert_eq!(artifact.summary.collateral_memberships, 1);
    assert_eq!(artifact.summary.membership_edges, 3);
    assert_eq!(artifact.summary.tile_spanning_properties, 1);
    assert_eq!(artifact.property_assertions.len(), 1);
    let property = &artifact.property_assertions[0];
    assert!(property.property_id.starts_with("cmdrvl:property:"));
    assert_eq!(
        property.document_alias,
        "cmbs:annexa:0000000000-26-000101:loan-101"
    );
    assert_eq!(
        property.parcel_ids,
        vec![
            parcel_a.to_string(),
            parcel_b.to_string(),
            parcel_c.to_string()
        ]
    );
    assert_eq!(
        property.tile_ids,
        vec!["h3:r8:alpha".to_string(), "h3:r8:beta".to_string()]
    );

    let relation_property_ids = artifact
        .member_relations
        .iter()
        .map(|relation| relation.property.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(relation_property_ids.len(), 1, "property must not fan out");
    assert_eq!(
        relation_property_ids.iter().next().copied(),
        Some(property.property_id.as_str())
    );
    assert!(
        artifact
            .member_relations
            .iter()
            .all(|relation| relation.relation == GeoControlRelation::Contains)
    );
    assert_eq!(
        artifact.collateral_memberships[0].property.id,
        property.property_id
    );

    canonical_property_assertion_bytes(&artifact).expect("artifact canonicalizes");
}

#[test]
fn tile_local_blocking_is_refused_for_property_assertions() {
    let mut bad = request(vec![assertion(
        "tile-local",
        "0000000000-26-000102",
        "loan-102",
        vec![member(
            GeoEntityLevel::Parcel,
            "cmdrvl:parcel:01J7X00000000000000PD",
            "h3:r8:alpha",
            GeoPropertyMembershipStatus::AssertedMember,
            None,
            "d",
        )],
    )]);
    bad.blocking_strategy = GeoPropertyBlockingStrategy::TileLocalOnly;
    let error = materialize_property_assertions(&bad)
        .expect_err("property assertions must be document-first");
    assert_eq!(
        error.code,
        GeoPropertyAssertionErrorCode::TileLocalBlockingRefused
    );
    assert_eq!(
        error.detail.get("required_strategy").map(String::as_str),
        Some("document_first_then_geography")
    );
}

#[test]
fn membership_abstention_is_typed_and_does_not_fabricate_membership() {
    let asserted = "cmdrvl:parcel:01J7X00000000000000PE";
    let uncertain = "cmdrvl:parcel:01J7X00000000000000PF";
    let artifact = materialize_property_assertions(&request(vec![assertion(
        "partial",
        "0000000000-26-000103",
        "loan-103",
        vec![
            member(
                GeoEntityLevel::Parcel,
                asserted,
                "h3:r8:alpha",
                GeoPropertyMembershipStatus::AssertedMember,
                None,
                "e",
            ),
            member(
                GeoEntityLevel::Parcel,
                uncertain,
                "h3:r8:beta",
                GeoPropertyMembershipStatus::AbstainedMembership,
                Some(GeoPropertyMembershipAbstentionReason::VariousAddressRequiresAnnexAParse),
                "f",
            ),
        ],
    )]))
    .expect("partial membership materializes with abstention");

    assert_eq!(artifact.summary.membership_edges, 1);
    assert_eq!(artifact.summary.membership_abstentions, 1);
    assert_eq!(artifact.property_assertions[0].parcel_ids, vec![asserted]);
    assert_eq!(
        artifact.membership_abstentions[0].candidate_member.id,
        uncertain
    );
    assert_eq!(
        artifact.membership_abstentions[0].reason,
        GeoPropertyMembershipAbstentionReason::VariousAddressRequiresAnnexAParse
    );
}

#[test]
fn overlapping_unequal_document_assertions_are_retained_and_compared() {
    let shared = "cmdrvl:parcel:01J7X00000000000000PG";
    let left_only = "cmdrvl:parcel:01J7X00000000000000PH";
    let right_only = "cmdrvl:parcel:01J7X00000000000000PI";
    let artifact = materialize_property_assertions(&request(vec![
        assertion(
            "left",
            "0000000000-26-000104",
            "loan-104",
            vec![
                member(
                    GeoEntityLevel::Parcel,
                    shared,
                    "h3:r8:alpha",
                    GeoPropertyMembershipStatus::AssertedMember,
                    None,
                    "g",
                ),
                member(
                    GeoEntityLevel::Parcel,
                    left_only,
                    "h3:r8:alpha",
                    GeoPropertyMembershipStatus::AssertedMember,
                    None,
                    "h",
                ),
            ],
        ),
        assertion(
            "right",
            "0000000000-26-000105",
            "loan-105",
            vec![
                member(
                    GeoEntityLevel::Parcel,
                    shared,
                    "h3:r8:alpha",
                    GeoPropertyMembershipStatus::AssertedMember,
                    None,
                    "i",
                ),
                member(
                    GeoEntityLevel::Parcel,
                    right_only,
                    "h3:r8:beta",
                    GeoPropertyMembershipStatus::AssertedMember,
                    None,
                    "j",
                ),
            ],
        ),
    ]))
    .expect("overlapping document assertions materialize");

    assert_eq!(artifact.property_assertions.len(), 2);
    assert_eq!(artifact.summary.pairwise_comparisons, 1);
    assert_eq!(artifact.summary.overlapping_unequal_assertions, 1);
    let comparison = &artifact.assertion_comparisons[0];
    assert!(comparison.overlapping_unequal);
    assert_eq!(
        comparison.comparison.relation,
        GeoPropertySetRelation::Intersects
    );
    assert_eq!(comparison.comparison.shared_parcel_ids, vec![shared]);
    assert_eq!(comparison.comparison.left_only_parcel_ids, vec![left_only]);
    assert_eq!(
        comparison.comparison.right_only_parcel_ids,
        vec![right_only]
    );
}

#[test]
fn relation_graph_option_b_is_required_and_scoreless() {
    let mut bad = request(vec![assertion(
        "internal-only",
        "0000000000-26-000106",
        "loan-106",
        vec![member(
            GeoEntityLevel::Parcel,
            "cmdrvl:parcel:01J7X00000000000000PJ",
            "h3:r8:alpha",
            GeoPropertyMembershipStatus::AssertedMember,
            None,
            "k",
        )],
    )]);
    bad.relation_graph_product = GeoPropertyRelationGraphProduct::WorkbenchInternalOnly;
    let error = materialize_property_assertions(&bad)
        .expect_err("property relation graph is a published product artifact");
    assert_eq!(
        error.code,
        GeoPropertyAssertionErrorCode::RelationGraphProjectionRequired
    );

    let artifact = materialize_property_assertions(&request(vec![assertion(
        "scoreless",
        "0000000000-26-000107",
        "loan-107",
        vec![member(
            GeoEntityLevel::Parcel,
            "cmdrvl:parcel:01J7X00000000000000PK",
            "h3:r8:alpha",
            GeoPropertyMembershipStatus::AssertedMember,
            None,
            "l",
        )],
    )]))
    .expect("published projection materializes");
    assert_no_probabilistic_fields(&serde_json::to_value(&artifact).expect("artifact serializes"));
}

fn assert_no_probabilistic_fields(value: &Value) {
    match value {
        Value::Object(map) => {
            for key in map.keys() {
                assert!(
                    !matches!(key.as_str(), "score" | "confidence" | "probability"),
                    "property projection must not serialize probabilistic field {key}"
                );
            }
            for nested in map.values() {
                assert_no_probabilistic_fields(nested);
            }
        }
        Value::Array(values) => {
            for nested in values {
                assert_no_probabilistic_fields(nested);
            }
        }
        _ => {}
    }
}
