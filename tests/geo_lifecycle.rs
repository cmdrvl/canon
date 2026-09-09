use canon::geo::{
    CANON_GEO_TEMPORAL_CONTAINMENT_VERSION, GeoContainmentAsOfQuery, GeoEntityExistenceAsOfQuery,
    GeoEntityExistenceInterval, GeoEntityExistenceNextEvidenceKind, GeoEntityExistenceReason,
    GeoEntityExistenceStatus, GeoEntityLevel, GeoEntityLifecycleEvidenceKind,
    GeoEntityLifecycleEvidenceRow, GeoLifecycleErrorCode, GeoPropertyMembershipAsOfQuery,
    GeoTemporalContainmentArtifact, GeoTemporalContainmentCluster, GeoTemporalContainmentEdge,
    GeoTemporalContainmentInterval, GeoTemporalContainmentRelation,
    GeoTemporalContainmentSourceReceipt, GeoTemporalContainmentSummary,
    GeoTemporalPresenceDisagreementQuery, GeoTemporalPresenceDisagreementReason,
    GeoTemporalPresenceDisagreementStatus, GeoTemporalPresenceObservation,
    GeoTemporalPresenceObservationKind, GeoTemporalPropertyMembershipEdge,
    GeoTemporalPropertyMembershipRelation, canonical_temporal_containment_bytes,
    classify_temporal_presence_disagreements, containment_as_of, entity_existence_as_of,
    entity_existence_intervals_from_lifecycle_evidence, property_membership_as_of,
    validate_temporal_containment_artifact,
};
use std::collections::BTreeMap;

#[test]
fn containment_as_of_answers_the_2020_shape_without_leaking_to_2019() {
    let artifact = temporal_containment_fixture();
    validate_temporal_containment_artifact(&artifact).expect("fixture is canonical");

    let as_2020 = containment_as_of(
        &artifact,
        &GeoContainmentAsOfQuery {
            as_of_utc_day: "2020-06-01".to_string(),
            parent_cluster_id: Some(parcel_id()),
            child_cluster_id: None,
        },
    )
    .expect("2020 query succeeds");
    assert_eq!(as_2020.edges.len(), 7);
    assert_eq!(as_2020.summary.parent_clusters, 1);
    assert_eq!(as_2020.summary.child_clusters, 7);
    assert!(
        as_2020
            .edges
            .iter()
            .all(|edge| edge.parent_cluster_id == parcel_id())
    );

    let as_2019 = containment_as_of(
        &artifact,
        &GeoContainmentAsOfQuery {
            as_of_utc_day: "2019-06-01".to_string(),
            parent_cluster_id: Some(parcel_id()),
            child_cluster_id: None,
        },
    )
    .expect("2019 query succeeds");
    assert!(
        as_2019.edges.is_empty(),
        "2019 query must not return buildings whose containment starts in 2020"
    );
}

#[test]
fn property_membership_as_of_keeps_three_corpora_distinct_on_one_parcel() {
    let artifact = temporal_containment_fixture();
    let active = property_membership_as_of(
        &artifact,
        &GeoPropertyMembershipAsOfQuery {
            as_of_utc_day: "2020-06-01".to_string(),
            property_cluster_id: None,
            member_cluster_id: None,
        },
    )
    .expect("property membership query succeeds");
    assert_eq!(active.summary.memberships, 3);
    assert_eq!(active.summary.property_clusters, 3);
    assert_eq!(active.summary.member_clusters, 3);

    let memberships = active
        .memberships
        .iter()
        .map(|membership| {
            (
                membership.property_cluster_id.as_str(),
                membership.member_cluster_id.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        memberships.get(property_id("reit", 1).as_str()).copied(),
        Some(building_id(1).as_str())
    );
    assert_eq!(
        memberships.get(property_id("cmbs", 2).as_str()).copied(),
        Some(building_id(2).as_str())
    );
    assert_eq!(
        memberships.get(property_id("rca", 3).as_str()).copied(),
        Some(building_id(3).as_str())
    );

    for member_cluster_id in memberships.values() {
        let containing_parcel = containment_as_of(
            &artifact,
            &GeoContainmentAsOfQuery {
                as_of_utc_day: "2020-06-01".to_string(),
                parent_cluster_id: Some(parcel_id()),
                child_cluster_id: Some((*member_cluster_id).to_string()),
            },
        )
        .expect("building containment query succeeds");
        assert_eq!(
            containing_parcel.summary.edges, 1,
            "all three property members should share the same parcel without merging properties"
        );
    }

    let before_construction = property_membership_as_of(
        &artifact,
        &GeoPropertyMembershipAsOfQuery {
            as_of_utc_day: "2019-06-01".to_string(),
            property_cluster_id: None,
            member_cluster_id: None,
        },
    )
    .expect("pre-construction property membership query succeeds");
    assert!(
        before_construction.memberships.is_empty(),
        "property memberships must not leak before their valid interval"
    );

    let cmbs_only = property_membership_as_of(
        &artifact,
        &GeoPropertyMembershipAsOfQuery {
            as_of_utc_day: "2020-06-01".to_string(),
            property_cluster_id: Some(property_id("cmbs", 2)),
            member_cluster_id: None,
        },
    )
    .expect("property-scoped membership query succeeds");
    assert_eq!(cmbs_only.summary.memberships, 1);
    assert_eq!(
        cmbs_only.memberships[0].member_cluster_id,
        building_id(2),
        "CMBS and REIT positions must remain separate even when the buildings share one BBL"
    );
}

#[test]
fn canonical_bytes_are_deterministic_under_edge_and_cluster_order_shuffle() {
    let canonical = temporal_containment_fixture();
    let mut shuffled = canonical.clone();
    shuffled.clusters.reverse();
    shuffled.existence_intervals.reverse();
    for interval in &mut shuffled.existence_intervals {
        interval.source_receipts.reverse();
    }
    shuffled.presence_observations.reverse();
    shuffled.edges.reverse();
    shuffled.property_membership_edges.reverse();
    shuffled.summary = GeoTemporalContainmentSummary {
        clusters: shuffled.clusters.len() as u64,
        existence_intervals: shuffled.existence_intervals.len() as u64,
        presence_observations: shuffled.presence_observations.len() as u64,
        edges: shuffled.edges.len() as u64,
        property_membership_edges: shuffled.property_membership_edges.len() as u64,
    };

    let left = canonical_temporal_containment_bytes(&canonical).expect("canonical bytes");
    let right = canonical_temporal_containment_bytes(&shuffled).expect("shuffled canonical bytes");
    assert_eq!(left, right);
}

#[test]
fn entity_existence_as_of_keeps_observation_gaps_separate_from_absence() {
    let artifact = temporal_containment_fixture();

    let observed_gap = only_existence_row(
        entity_existence_as_of(
            &artifact,
            &GeoEntityExistenceAsOfQuery {
                as_of_utc_day: "2019-06-01".to_string(),
                cluster_id: Some(building_id(1)),
                observation_window: None,
            },
        )
        .expect("existence query succeeds"),
    );
    assert_eq!(observed_gap.status, GeoEntityExistenceStatus::Unverified);
    assert_eq!(
        observed_gap.reason,
        GeoEntityExistenceReason::OutsideObservedWindow
    );

    let observed_present = only_existence_row(
        entity_existence_as_of(
            &artifact,
            &GeoEntityExistenceAsOfQuery {
                as_of_utc_day: "2020-06-01".to_string(),
                cluster_id: Some(building_id(1)),
                observation_window: None,
            },
        )
        .expect("existence query succeeds"),
    );
    assert_eq!(observed_present.status, GeoEntityExistenceStatus::Present);
    assert_eq!(
        observed_present.reason,
        GeoEntityExistenceReason::ObservedPresent
    );

    let before_birth = only_existence_row(
        entity_existence_as_of(
            &artifact,
            &GeoEntityExistenceAsOfQuery {
                as_of_utc_day: "2019-06-01".to_string(),
                cluster_id: Some(building_id(2)),
                observation_window: None,
            },
        )
        .expect("existence query succeeds"),
    );
    assert_eq!(before_birth.status, GeoEntityExistenceStatus::Absent);
    assert_eq!(
        before_birth.reason,
        GeoEntityExistenceReason::BeforeAuthoritativeBirth
    );

    let after_death = only_existence_row(
        entity_existence_as_of(
            &artifact,
            &GeoEntityExistenceAsOfQuery {
                as_of_utc_day: "2021-06-01".to_string(),
                cluster_id: Some(building_id(3)),
                observation_window: None,
            },
        )
        .expect("existence query succeeds"),
    );
    assert_eq!(after_death.status, GeoEntityExistenceStatus::Absent);
    assert_eq!(
        after_death.reason,
        GeoEntityExistenceReason::AfterAuthoritativeDeath
    );

    let death_only_before_observation = only_existence_row(
        entity_existence_as_of(
            &artifact,
            &GeoEntityExistenceAsOfQuery {
                as_of_utc_day: "2019-06-01".to_string(),
                cluster_id: Some(building_id(4)),
                observation_window: None,
            },
        )
        .expect("existence query succeeds"),
    );
    assert_eq!(
        death_only_before_observation.status,
        GeoEntityExistenceStatus::Unverified
    );
    assert_eq!(
        death_only_before_observation.reason,
        GeoEntityExistenceReason::OutsideObservedWindow
    );

    let no_evidence = only_existence_row(
        entity_existence_as_of(
            &artifact,
            &GeoEntityExistenceAsOfQuery {
                as_of_utc_day: "2020-06-01".to_string(),
                cluster_id: Some(building_id(7)),
                observation_window: None,
            },
        )
        .expect("existence query succeeds"),
    );
    assert_eq!(no_evidence.status, GeoEntityExistenceStatus::Unverified);
    assert_eq!(
        no_evidence.reason,
        GeoEntityExistenceReason::NoExistenceEvidence
    );
}

#[test]
fn entity_existence_as_of_reports_new_construction_cold_start_with_refresh_remedy() {
    let artifact = temporal_containment_fixture();
    let cold_start = only_existence_row(
        entity_existence_as_of(
            &artifact,
            &GeoEntityExistenceAsOfQuery {
                as_of_utc_day: "2024-06-01".to_string(),
                cluster_id: Some(building_id(7)),
                observation_window: Some(GeoTemporalContainmentInterval {
                    start_utc_day: "2020-01-01".to_string(),
                    end_utc_day: "2022-12-31".to_string(),
                }),
            },
        )
        .expect("existence query succeeds"),
    );

    assert_eq!(cold_start.status, GeoEntityExistenceStatus::Unverified);
    assert_eq!(
        cold_start.reason,
        GeoEntityExistenceReason::AfterObservationWindow
    );
    let next = cold_start
        .next_evidence
        .expect("cold start carries a remedy");
    assert_eq!(
        next.kind,
        GeoEntityExistenceNextEvidenceKind::RefreshTemporalEvidence
    );
    assert!(
        next.reason.contains("latest retained observation vintage"),
        "remedy should name the observation-window limit"
    );
}

#[test]
fn presence_disagreement_classifier_keeps_absence_diagnostic() {
    let artifact = temporal_containment_fixture();
    let classified = classify_temporal_presence_disagreements(
        &artifact,
        &GeoTemporalPresenceDisagreementQuery { cluster_id: None },
    )
    .expect("presence disagreements classify");

    assert_eq!(classified.summary.rows, 4);
    assert_eq!(classified.summary.demolition_supported, 1);
    assert_eq!(classified.summary.coverage_gap, 1);
    assert_eq!(classified.summary.new_construction_cold_start, 1);
    assert_eq!(classified.summary.indistinguishable, 1);
    assert!(
        classified.rows.iter().all(|row| row.diagnostic_only),
        "present/absent vintage rows must remain diagnostic before Allen/STP composition lands"
    );

    let rows = classified
        .rows
        .iter()
        .map(|row| (row.cluster_id.as_str(), row))
        .collect::<BTreeMap<_, _>>();

    let demolition = rows
        .get(building_id(3).as_str())
        .expect("authoritative death supports demolition");
    assert_eq!(
        demolition.status,
        GeoTemporalPresenceDisagreementStatus::DemolitionSupported
    );
    assert_eq!(
        demolition.reason,
        GeoTemporalPresenceDisagreementReason::AuthoritativeDeathBetweenPresentAndAbsent
    );
    assert!(demolition.next_evidence.is_none());

    let coverage_gap = rows
        .get(building_id(1).as_str())
        .expect("same-vintage source disagreement is a coverage gap");
    assert_eq!(
        coverage_gap.status,
        GeoTemporalPresenceDisagreementStatus::CoverageGap
    );
    assert_eq!(
        coverage_gap.reason,
        GeoTemporalPresenceDisagreementReason::SameVintagePresentAndAbsent
    );
    assert_eq!(
        coverage_gap.next_evidence.as_ref().map(|next| next.kind),
        Some(GeoEntityExistenceNextEvidenceKind::AcquireAuthoritativeLifecycleEvidence)
    );

    let cold_start = rows
        .get(building_id(2).as_str())
        .expect("authoritative birth after retained absence is a cold start");
    assert_eq!(
        cold_start.status,
        GeoTemporalPresenceDisagreementStatus::NewConstructionColdStart
    );
    assert_eq!(
        cold_start.reason,
        GeoTemporalPresenceDisagreementReason::AuthoritativeBirthAfterRetainedAbsence
    );
    assert_eq!(
        cold_start.next_evidence.as_ref().map(|next| next.kind),
        Some(GeoEntityExistenceNextEvidenceKind::RefreshTemporalEvidence)
    );

    let indistinguishable = rows
        .get(building_id(5).as_str())
        .expect("unsupported present-then-absent sequence abstains");
    assert_eq!(
        indistinguishable.status,
        GeoTemporalPresenceDisagreementStatus::Indistinguishable
    );
    assert_eq!(
        indistinguishable.reason,
        GeoTemporalPresenceDisagreementReason::MissingAuthoritativeLifecycleEvidence
    );
    assert_eq!(
        indistinguishable.present_observation_ids,
        vec!["presence-b05-2019".to_string()]
    );
    assert_eq!(
        indistinguishable.absent_observation_ids,
        vec!["absence-b05-2022".to_string()]
    );

    let building_6 = classify_temporal_presence_disagreements(
        &artifact,
        &GeoTemporalPresenceDisagreementQuery {
            cluster_id: Some(building_id(6)),
        },
    )
    .expect("cluster-scoped presence disagreement query succeeds");
    assert!(building_6.rows.is_empty());
}

#[test]
fn lifecycle_evidence_materializes_existence_intervals_without_source_specific_branches() {
    let nyc_building = building_id(1);
    let franklin_parcel = "cmdrvl:parcel:franklin:parcel:010-000101".to_string();
    let intervals = entity_existence_intervals_from_lifecycle_evidence(&[
        lifecycle_evidence(
            "nyc-birth",
            &nyc_building,
            GeoEntityLevel::Building,
            GeoEntityLifecycleEvidenceKind::AuthoritativeBirth,
            "2020-01-01",
            "NYC_DOB_CERTIFICATES_OF_OCCUPANCY",
        ),
        lifecycle_evidence(
            "nyc-present",
            &nyc_building,
            GeoEntityLevel::Building,
            GeoEntityLifecycleEvidenceKind::ObservedPresent,
            "2021-05-31",
            "MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT",
        ),
        lifecycle_evidence(
            "franklin-create",
            &franklin_parcel,
            GeoEntityLevel::Parcel,
            GeoEntityLifecycleEvidenceKind::AuthoritativeBirth,
            "2021-09-01",
            "FRANKLIN_COUNTY_AUDITOR_DROPS_ADDS_HOT",
        ),
        lifecycle_evidence(
            "franklin-retire",
            &franklin_parcel,
            GeoEntityLevel::Parcel,
            GeoEntityLifecycleEvidenceKind::AuthoritativeDeath,
            "2024-09-01",
            "FRANKLIN_COUNTY_AUDITOR_DROPS_ADDS_HOT",
        ),
    ])
    .expect("typed lifecycle evidence materializes");

    assert_eq!(intervals.len(), 2);
    let building = intervals
        .iter()
        .find(|interval| interval.cluster_id == nyc_building)
        .expect("building interval exists");
    assert_eq!(building.entity_level, GeoEntityLevel::Building);
    assert_eq!(building.observed_interval.start_utc_day, "2020-01-01");
    assert_eq!(building.observed_interval.end_utc_day, "2021-05-31");
    assert_eq!(
        building.authoritative_birth_utc_day.as_deref(),
        Some("2020-01-01")
    );
    assert_eq!(building.authoritative_death_utc_day, None);
    assert_eq!(building.source_receipts.len(), 2);

    let parcel = intervals
        .iter()
        .find(|interval| interval.cluster_id == franklin_parcel)
        .expect("Franklin parcel interval exists");
    assert_eq!(parcel.entity_level, GeoEntityLevel::Parcel);
    assert_eq!(parcel.observed_interval.start_utc_day, "2021-09-01");
    assert_eq!(parcel.observed_interval.end_utc_day, "2024-09-01");
    assert_eq!(
        parcel.authoritative_birth_utc_day.as_deref(),
        Some("2021-09-01")
    );
    assert_eq!(
        parcel.authoritative_death_utc_day.as_deref(),
        Some("2024-09-01")
    );
}

#[test]
fn lifecycle_evidence_materialization_rejects_duplicates_and_inverted_authoritative_life() {
    let duplicate = lifecycle_evidence(
        "duplicate",
        &building_id(1),
        GeoEntityLevel::Building,
        GeoEntityLifecycleEvidenceKind::ObservedPresent,
        "2020-01-01",
        "fixture.lifecycle",
    );
    let error = entity_existence_intervals_from_lifecycle_evidence(&[duplicate.clone(), duplicate])
        .expect_err("duplicate evidence ids are rejected");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("evidence_id").map(String::as_str),
        Some("duplicate")
    );

    let inverted = entity_existence_intervals_from_lifecycle_evidence(&[
        lifecycle_evidence(
            "birth-after-death",
            &building_id(2),
            GeoEntityLevel::Building,
            GeoEntityLifecycleEvidenceKind::AuthoritativeBirth,
            "2021-01-01",
            "fixture.lifecycle",
        ),
        lifecycle_evidence(
            "death-before-birth",
            &building_id(2),
            GeoEntityLevel::Building,
            GeoEntityLifecycleEvidenceKind::AuthoritativeDeath,
            "2020-01-01",
            "fixture.lifecycle",
        ),
    ])
    .expect_err("death before birth is rejected");
    assert_eq!(inverted.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        inverted.detail.get("field").map(String::as_str),
        Some("lifecycle_evidence_rows")
    );
}

#[test]
fn validator_rejects_unsorted_and_duplicate_edges() {
    let mut unsorted = temporal_containment_fixture();
    unsorted.edges.swap(0, 1);
    let error = validate_temporal_containment_artifact(&unsorted)
        .expect_err("validator rejects non-canonical edge order");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(error.detail.get("field").map(String::as_str), Some("edges"));

    let mut duplicate = temporal_containment_fixture();
    duplicate.edges[1].child_cluster_id = duplicate.edges[0].child_cluster_id.clone();
    duplicate.edges[1].edge_id = "edge-duplicate-id-kept-distinct".to_string();
    let error = validate_temporal_containment_artifact(&duplicate)
        .expect_err("validator rejects duplicate semantic containment");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(error.detail.get("field").map(String::as_str), Some("edges"));
}

#[test]
fn validator_rejects_invalid_existence_intervals() {
    let mut unsorted = temporal_containment_fixture();
    unsorted.existence_intervals.swap(0, 1);
    let error = validate_temporal_containment_artifact(&unsorted)
        .expect_err("validator rejects non-canonical existence interval order");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("existence_intervals")
    );

    let mut duplicate = temporal_containment_fixture();
    duplicate
        .existence_intervals
        .push(duplicate.existence_intervals[0].clone());
    duplicate.summary.existence_intervals = duplicate.existence_intervals.len() as u64;
    let error = validate_temporal_containment_artifact(&duplicate)
        .expect_err("validator rejects duplicate semantic existence intervals");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("existence_intervals")
    );

    let mut missing_receipt = temporal_containment_fixture();
    missing_receipt.existence_intervals[0]
        .source_receipts
        .clear();
    let error = validate_temporal_containment_artifact(&missing_receipt)
        .expect_err("validator rejects existence intervals without source receipts");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("existence_intervals[].source_receipts")
    );
}

#[test]
fn validator_rejects_unknown_or_misleveled_endpoints() {
    let mut unknown = temporal_containment_fixture();
    unknown.edges[0].child_cluster_id = "cmdrvl:building:missing".to_string();
    let error = validate_temporal_containment_artifact(&unknown)
        .expect_err("validator rejects unknown child cluster");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("edges[].child_cluster_id")
    );

    let mut misleveled = temporal_containment_fixture();
    misleveled.edges[0].child_level = GeoEntityLevel::Parcel;
    let error = validate_temporal_containment_artifact(&misleveled)
        .expect_err("validator rejects endpoint level mismatch");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("edges[].child_cluster_id")
    );
}

#[test]
fn validator_rejects_invalid_temporal_property_memberships() {
    let mut unknown_member = temporal_containment_fixture();
    unknown_member.property_membership_edges[0].member_cluster_id =
        "cmdrvl:building:missing".to_string();
    let error = validate_temporal_containment_artifact(&unknown_member)
        .expect_err("validator rejects unknown property member cluster");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("property_membership_edges[].member_cluster_id")
    );

    let mut unsupported_member_level = temporal_containment_fixture();
    unsupported_member_level.property_membership_edges[0].member_level = GeoEntityLevel::Property;
    unsupported_member_level.property_membership_edges[0].member_cluster_id =
        property_id("reit", 1);
    let error = validate_temporal_containment_artifact(&unsupported_member_level)
        .expect_err("validator rejects property-as-member edges");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("property_membership_edges[].member_level")
    );

    let mut duplicate = temporal_containment_fixture();
    let mut duplicate_edge = duplicate.property_membership_edges[0].clone();
    duplicate_edge.membership_id = "membership-duplicate-semantic".to_string();
    duplicate.property_membership_edges.push(duplicate_edge);
    duplicate.summary.property_membership_edges = duplicate.property_membership_edges.len() as u64;
    let error = validate_temporal_containment_artifact(&duplicate)
        .expect_err("validator rejects duplicate semantic property memberships");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("property_membership_edges")
    );
}

#[test]
fn validator_rejects_invalid_temporal_presence_observations() {
    let mut unsorted = temporal_containment_fixture();
    unsorted.presence_observations.swap(0, 1);
    let error = validate_temporal_containment_artifact(&unsorted)
        .expect_err("validator rejects non-canonical presence observation order");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("presence_observations")
    );

    let mut duplicate = temporal_containment_fixture();
    let mut duplicate_observation = duplicate.presence_observations[0].clone();
    duplicate_observation.observation_id = "presence-duplicate-semantic".to_string();
    duplicate.presence_observations.push(duplicate_observation);
    duplicate.summary.presence_observations = duplicate.presence_observations.len() as u64;
    let error = validate_temporal_containment_artifact(&duplicate)
        .expect_err("validator rejects duplicate semantic presence observations");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("presence_observations")
    );

    let mut misleveled = temporal_containment_fixture();
    misleveled.presence_observations[0].entity_level = GeoEntityLevel::Parcel;
    let error = validate_temporal_containment_artifact(&misleveled)
        .expect_err("validator rejects presence observation endpoint level mismatch");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("presence_observations[].cluster_id")
    );
}

#[test]
fn validator_rejects_bad_intervals_and_missing_receipts() {
    let mut bad_interval = temporal_containment_fixture();
    bad_interval.edges[0].valid_interval.start_utc_day = "2021-01-01".to_string();
    bad_interval.edges[0].valid_interval.end_utc_day = "2020-01-01".to_string();
    let error = validate_temporal_containment_artifact(&bad_interval)
        .expect_err("validator rejects inverted intervals");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("valid_interval")
    );

    let mut missing_receipt = temporal_containment_fixture();
    missing_receipt.edges[0].source_receipt.rule_id.clear();
    let error = validate_temporal_containment_artifact(&missing_receipt)
        .expect_err("validator rejects missing source receipt rule_id");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("source_receipt.rule_id")
    );

    let mut bad_digest = temporal_containment_fixture();
    bad_digest.edges[0].source_receipt.source_record_blake3 = "sha256:not-a-blake3".to_string();
    let error = validate_temporal_containment_artifact(&bad_digest)
        .expect_err("validator rejects non-blake3 receipt digest");
    assert_eq!(error.code, GeoLifecycleErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("source_receipt.source_record_blake3")
    );
}

fn temporal_containment_fixture() -> GeoTemporalContainmentArtifact {
    let mut clusters = vec![GeoTemporalContainmentCluster {
        cluster_id: parcel_id(),
        entity_level: GeoEntityLevel::Parcel,
    }];
    clusters.extend((1..=7).map(|building| GeoTemporalContainmentCluster {
        cluster_id: building_id(building),
        entity_level: GeoEntityLevel::Building,
    }));
    clusters.extend([
        GeoTemporalContainmentCluster {
            cluster_id: property_id("cmbs", 2),
            entity_level: GeoEntityLevel::Property,
        },
        GeoTemporalContainmentCluster {
            cluster_id: property_id("rca", 3),
            entity_level: GeoEntityLevel::Property,
        },
        GeoTemporalContainmentCluster {
            cluster_id: property_id("reit", 1),
            entity_level: GeoEntityLevel::Property,
        },
    ]);
    clusters.sort_by(|left, right| left.cluster_id.cmp(&right.cluster_id));

    let mut edges = (1..=7)
        .map(|building| GeoTemporalContainmentEdge {
            edge_id: format!("edge-b{building:02}-part-of-p201"),
            parent_cluster_id: parcel_id(),
            parent_level: GeoEntityLevel::Parcel,
            child_cluster_id: building_id(building),
            child_level: GeoEntityLevel::Building,
            relation: GeoTemporalContainmentRelation::PartOf,
            valid_interval: GeoTemporalContainmentInterval {
                start_utc_day: "2020-01-01".to_string(),
                end_utc_day: "2020-12-31".to_string(),
            },
            source_receipt: GeoTemporalContainmentSourceReceipt {
                receipt_id: format!("receipt-building-{building:02}"),
                source_dataset: "fixture.nyc.lifecycle".to_string(),
                source_record_id: format!("dob-job:fixture:{building:02}"),
                source_record_blake3: blake3_uri(&format!("dob-job:fixture:{building:02}")),
                proof_class: "fixture".to_string(),
                rule_id: "geo_temporal_containment_fixture.v1".to_string(),
            },
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        left.parent_cluster_id
            .cmp(&right.parent_cluster_id)
            .then_with(|| left.child_cluster_id.cmp(&right.child_cluster_id))
            .then_with(|| {
                left.valid_interval
                    .start_utc_day
                    .cmp(&right.valid_interval.start_utc_day)
            })
            .then_with(|| {
                left.valid_interval
                    .end_utc_day
                    .cmp(&right.valid_interval.end_utc_day)
            })
            .then_with(|| left.edge_id.cmp(&right.edge_id))
    });

    let existence_intervals = vec![
        GeoEntityExistenceInterval {
            cluster_id: building_id(1),
            entity_level: GeoEntityLevel::Building,
            observed_interval: GeoTemporalContainmentInterval {
                start_utc_day: "2020-01-01".to_string(),
                end_utc_day: "2020-12-31".to_string(),
            },
            authoritative_birth_utc_day: None,
            authoritative_death_utc_day: None,
            source_receipts: vec![
                source_receipt("existence-b01-a", "observer:fixture:201:b01:a"),
                source_receipt("existence-b01-b", "observer:fixture:201:b01:b"),
            ],
        },
        GeoEntityExistenceInterval {
            cluster_id: building_id(2),
            entity_level: GeoEntityLevel::Building,
            observed_interval: GeoTemporalContainmentInterval {
                start_utc_day: "2020-01-01".to_string(),
                end_utc_day: "2020-12-31".to_string(),
            },
            authoritative_birth_utc_day: Some("2020-01-01".to_string()),
            authoritative_death_utc_day: None,
            source_receipts: vec![source_receipt("existence-b02", "dob:fixture:201:b02")],
        },
        GeoEntityExistenceInterval {
            cluster_id: building_id(3),
            entity_level: GeoEntityLevel::Building,
            observed_interval: GeoTemporalContainmentInterval {
                start_utc_day: "2020-01-01".to_string(),
                end_utc_day: "2020-12-31".to_string(),
            },
            authoritative_birth_utc_day: Some("2020-01-01".to_string()),
            authoritative_death_utc_day: Some("2020-12-31".to_string()),
            source_receipts: vec![source_receipt("existence-b03", "co:fixture:201:b03")],
        },
        GeoEntityExistenceInterval {
            cluster_id: building_id(4),
            entity_level: GeoEntityLevel::Building,
            observed_interval: GeoTemporalContainmentInterval {
                start_utc_day: "2020-01-01".to_string(),
                end_utc_day: "2020-12-31".to_string(),
            },
            authoritative_birth_utc_day: None,
            authoritative_death_utc_day: Some("2020-12-31".to_string()),
            source_receipts: vec![source_receipt(
                "existence-b04",
                "demolition:fixture:201:b04",
            )],
        },
    ];
    let mut presence_observations = vec![
        presence_observation(
            "presence-b01-2020",
            1,
            GeoTemporalPresenceObservationKind::PresentAtVintage,
            "2020-06-01",
            "fixture.fema.2020",
        ),
        presence_observation(
            "absence-b01-2020",
            1,
            GeoTemporalPresenceObservationKind::AbsentAtVintage,
            "2020-06-01",
            "fixture.overture.2020",
        ),
        presence_observation(
            "absence-b02-2019",
            2,
            GeoTemporalPresenceObservationKind::AbsentAtVintage,
            "2019-06-01",
            "fixture.overture.2019",
        ),
        presence_observation(
            "presence-b03-2020",
            3,
            GeoTemporalPresenceObservationKind::PresentAtVintage,
            "2020-06-01",
            "fixture.fema.2020",
        ),
        presence_observation(
            "absence-b03-2021",
            3,
            GeoTemporalPresenceObservationKind::AbsentAtVintage,
            "2021-06-01",
            "fixture.overture.2021",
        ),
        presence_observation(
            "presence-b05-2019",
            5,
            GeoTemporalPresenceObservationKind::PresentAtVintage,
            "2019-06-01",
            "fixture.fema.2019",
        ),
        presence_observation(
            "absence-b05-2022",
            5,
            GeoTemporalPresenceObservationKind::AbsentAtVintage,
            "2022-06-01",
            "fixture.overture.2022",
        ),
        presence_observation(
            "presence-b06-2020",
            6,
            GeoTemporalPresenceObservationKind::PresentAtVintage,
            "2020-06-01",
            "fixture.microsoft.2020",
        ),
    ];
    presence_observations.sort_by(|left, right| {
        left.cluster_id
            .cmp(&right.cluster_id)
            .then_with(|| left.vintage_utc_day.cmp(&right.vintage_utc_day))
            .then_with(|| left.observation_kind.cmp(&right.observation_kind))
            .then_with(|| left.observation_id.cmp(&right.observation_id))
    });
    let mut property_membership_edges = vec![
        property_membership_edge("reit", 1, "fixture.reit.positions"),
        property_membership_edge("cmbs", 2, "fixture.cmbs.positions"),
        property_membership_edge("rca", 3, "fixture.rca.positions"),
    ];
    property_membership_edges.sort_by(|left, right| {
        left.property_cluster_id
            .cmp(&right.property_cluster_id)
            .then_with(|| left.member_cluster_id.cmp(&right.member_cluster_id))
            .then_with(|| {
                left.valid_interval
                    .start_utc_day
                    .cmp(&right.valid_interval.start_utc_day)
            })
            .then_with(|| {
                left.valid_interval
                    .end_utc_day
                    .cmp(&right.valid_interval.end_utc_day)
            })
            .then_with(|| left.membership_id.cmp(&right.membership_id))
    });

    GeoTemporalContainmentArtifact {
        version: CANON_GEO_TEMPORAL_CONTAINMENT_VERSION.to_string(),
        mart_id: "fixture.nyc.lifecycle.2019-2020".to_string(),
        summary: GeoTemporalContainmentSummary {
            clusters: clusters.len() as u64,
            existence_intervals: existence_intervals.len() as u64,
            presence_observations: presence_observations.len() as u64,
            edges: edges.len() as u64,
            property_membership_edges: property_membership_edges.len() as u64,
        },
        clusters,
        existence_intervals,
        presence_observations,
        edges,
        property_membership_edges,
    }
}

fn only_existence_row(
    artifact: canon::geo::GeoEntityExistenceAsOfArtifact,
) -> canon::geo::GeoEntityExistenceAsOfRow {
    assert_eq!(artifact.summary.rows, 1);
    artifact.rows.into_iter().next().expect("one existence row")
}

fn parcel_id() -> String {
    "cmdrvl:parcel:nyc:bbl:fixture-201".to_string()
}

fn building_id(number: u8) -> String {
    format!("cmdrvl:building:nyc:bin:fixture-201-{number:02}")
}

fn property_id(corpus: &str, number: u8) -> String {
    format!("cmdrvl:property:fixture:{corpus}:building-{number:02}")
}

fn blake3_uri(input: &str) -> String {
    format!("blake3:{}", blake3::hash(input.as_bytes()).to_hex())
}

fn source_receipt(receipt_id: &str, source_record_id: &str) -> GeoTemporalContainmentSourceReceipt {
    GeoTemporalContainmentSourceReceipt {
        receipt_id: receipt_id.to_string(),
        source_dataset: "fixture.nyc.lifecycle".to_string(),
        source_record_id: source_record_id.to_string(),
        source_record_blake3: blake3_uri(source_record_id),
        proof_class: "fixture".to_string(),
        rule_id: "geo_entity_existence_fixture.v1".to_string(),
    }
}

fn presence_observation(
    observation_id: &str,
    building: u8,
    observation_kind: GeoTemporalPresenceObservationKind,
    vintage_utc_day: &str,
    source_dataset: &str,
) -> GeoTemporalPresenceObservation {
    GeoTemporalPresenceObservation {
        observation_id: observation_id.to_string(),
        cluster_id: building_id(building),
        entity_level: GeoEntityLevel::Building,
        observation_kind,
        vintage_utc_day: vintage_utc_day.to_string(),
        source_receipt: GeoTemporalContainmentSourceReceipt {
            receipt_id: format!("receipt-{observation_id}"),
            source_dataset: source_dataset.to_string(),
            source_record_id: format!("{source_dataset}:{observation_id}"),
            source_record_blake3: blake3_uri(&format!("{source_dataset}:{observation_id}")),
            proof_class: "fixture".to_string(),
            rule_id: "geo_temporal_presence_observation_fixture.v1".to_string(),
        },
    }
}

fn property_membership_edge(
    corpus: &str,
    building: u8,
    source_dataset: &str,
) -> GeoTemporalPropertyMembershipEdge {
    GeoTemporalPropertyMembershipEdge {
        membership_id: format!("membership-{corpus}-b{building:02}"),
        property_cluster_id: property_id(corpus, building),
        member_cluster_id: building_id(building),
        member_level: GeoEntityLevel::Building,
        relation: GeoTemporalPropertyMembershipRelation::CollateralMember,
        valid_interval: GeoTemporalContainmentInterval {
            start_utc_day: "2020-01-01".to_string(),
            end_utc_day: "2020-12-31".to_string(),
        },
        source_receipt: GeoTemporalContainmentSourceReceipt {
            receipt_id: format!("receipt-membership-{corpus}-b{building:02}"),
            source_dataset: source_dataset.to_string(),
            source_record_id: format!("{source_dataset}:b{building:02}"),
            source_record_blake3: blake3_uri(&format!("{source_dataset}:b{building:02}")),
            proof_class: "fixture".to_string(),
            rule_id: "geo_temporal_property_membership_fixture.v1".to_string(),
        },
    }
}

fn lifecycle_evidence(
    evidence_id: &str,
    cluster_id: &str,
    entity_level: GeoEntityLevel,
    evidence_kind: GeoEntityLifecycleEvidenceKind,
    observed_utc_day: &str,
    source_dataset: &str,
) -> GeoEntityLifecycleEvidenceRow {
    GeoEntityLifecycleEvidenceRow {
        evidence_id: evidence_id.to_string(),
        cluster_id: cluster_id.to_string(),
        entity_level,
        evidence_kind,
        observed_utc_day: observed_utc_day.to_string(),
        source_receipt: GeoTemporalContainmentSourceReceipt {
            receipt_id: format!("receipt-{evidence_id}"),
            source_dataset: source_dataset.to_string(),
            source_record_id: format!("{source_dataset}:{evidence_id}"),
            source_record_blake3: blake3_uri(&format!("{source_dataset}:{evidence_id}")),
            proof_class: "fixture".to_string(),
            rule_id: "geo_entity_lifecycle_evidence_fixture.v1".to_string(),
        },
    }
}
