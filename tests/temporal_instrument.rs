#![forbid(unsafe_code)]

use canon::temporal::{
    IntervalBoundary, RecordedTime, SourceLocator, TimeInterval,
    instrument::{
        IDENTIFIES_SAME_INSTRUMENT_PREDICATE, InstrumentIdentifierObservation,
        InstrumentSuccessionDecisionKind, InstrumentSuccessionReason,
        canonical_instrument_succession_bytes, derive_instrument_identifier_succession,
    },
    relation::{
        CoreRelationClass, RelationIdentityImplicationMode, RelationKindRef,
        relation_edge_implies_alias,
    },
};

#[test]
fn cusip_handover_emits_valid_time_equality_and_relation_context() {
    let predecessor = observation(ObservationSpec {
        surface_id: "surface:old-cusip",
        cusip: Some("111111111"),
        issuer_lei: Some("549300SUCCESSION"),
        maturity_date: Some("2030-06-30"),
        rate_bps: Some(500),
        balance_profile: Some("principal_bucket:10m"),
        figi: Some("BBG00STABLE1"),
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: Some("2026-03-31T23:59:59Z"),
        fragment: "row-old",
    });
    let successor = observation(ObservationSpec {
        surface_id: "surface:new-cusip",
        cusip: Some("222222222"),
        issuer_lei: Some("549300SUCCESSION"),
        maturity_date: Some("2030-06-30"),
        rate_bps: Some(500),
        balance_profile: Some("principal_bucket:10m"),
        figi: Some("BBG00STABLE1"),
        valid_start: "2026-04-01T00:00:00Z",
        valid_end: Some("2026-06-30T23:59:59Z"),
        fragment: "row-new",
    });

    let surface = derive_instrument_identifier_succession(predecessor, successor)
        .expect("succession derives");
    assert_eq!(
        surface.decision.kind,
        InstrumentSuccessionDecisionKind::Succession
    );
    assert_eq!(
        surface.decision.reason,
        InstrumentSuccessionReason::ValidTimeEqualityWithSupersededByRelation
    );
    assert_eq!(surface.identity_facts.len(), 1);
    assert_eq!(surface.relations.len(), 1);

    let fact = &surface.identity_facts[0];
    assert_eq!(fact.predicate, IDENTIFIES_SAME_INSTRUMENT_PREDICATE);
    assert_eq!(fact.subject_id, "instrument_identifier:cusip:111111111");
    assert_eq!(fact.object_id, "instrument_identifier:cusip:222222222");
    assert_eq!(
        fact.valid_time.start_at.as_deref(),
        Some("2026-04-01T00:00:00Z")
    );
    assert_eq!(
        fact.valid_time.end_at.as_deref(),
        Some("2026-06-30T23:59:59Z")
    );

    let relation = &surface.relations[0];
    assert_eq!(
        relation.relation,
        RelationKindRef::Core {
            class: CoreRelationClass::Succession
        }
    );
    assert!(!relation_edge_implies_alias(relation));
    assert_eq!(
        relation.identity_implication.mode,
        RelationIdentityImplicationMode::SupportedBySeparateEqualityFact
    );
    assert_eq!(
        relation.identity_implication.equality_fact_ref.as_deref(),
        Some(fact.fact_id.as_str())
    );

    let bytes_a = canonical_instrument_succession_bytes(&surface).expect("surface bytes serialize");
    let bytes_b =
        canonical_instrument_succession_bytes(&surface).expect("surface bytes serialize again");
    assert_eq!(bytes_a, bytes_b);
}

#[test]
fn figi_change_is_time_series_break_not_instrument_succession() {
    let predecessor = observation(ObservationSpec {
        surface_id: "surface:old-figi",
        cusip: Some("111111111"),
        issuer_lei: Some("549300SUCCESSION"),
        maturity_date: Some("2030-06-30"),
        rate_bps: Some(500),
        balance_profile: Some("principal_bucket:10m"),
        figi: Some("BBG00OLD001"),
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: Some("2026-03-31T23:59:59Z"),
        fragment: "row-old",
    });
    let successor = observation(ObservationSpec {
        surface_id: "surface:new-figi",
        cusip: Some("222222222"),
        issuer_lei: Some("549300SUCCESSION"),
        maturity_date: Some("2030-06-30"),
        rate_bps: Some(500),
        balance_profile: Some("principal_bucket:10m"),
        figi: Some("BBG00NEW001"),
        valid_start: "2026-04-01T00:00:00Z",
        valid_end: Some("2026-06-30T23:59:59Z"),
        fragment: "row-new",
    });

    let surface = derive_instrument_identifier_succession(predecessor, successor)
        .expect("figi break derives");
    assert_eq!(
        surface.decision.kind,
        InstrumentSuccessionDecisionKind::TimeSeriesBreak
    );
    assert_eq!(
        surface.decision.reason,
        InstrumentSuccessionReason::FigiChanged
    );
    assert!(surface.identity_facts.is_empty());
    assert!(surface.relations.is_empty());
    assert_eq!(surface.breaks.len(), 1);
}

#[test]
fn missing_inputs_and_issuer_only_matches_do_not_emit_evidence() {
    let missing_balance = observation(ObservationSpec {
        surface_id: "surface:missing-balance",
        cusip: Some("111111111"),
        issuer_lei: Some("549300SUCCESSION"),
        maturity_date: Some("2030-06-30"),
        rate_bps: Some(500),
        balance_profile: None,
        figi: Some("BBG00STABLE1"),
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: Some("2026-03-31T23:59:59Z"),
        fragment: "row-missing",
    });
    let successor = observation(ObservationSpec {
        surface_id: "surface:new-cusip",
        cusip: Some("222222222"),
        issuer_lei: Some("549300SUCCESSION"),
        maturity_date: Some("2030-06-30"),
        rate_bps: Some(500),
        balance_profile: Some("principal_bucket:10m"),
        figi: Some("BBG00STABLE1"),
        valid_start: "2026-04-01T00:00:00Z",
        valid_end: Some("2026-06-30T23:59:59Z"),
        fragment: "row-new",
    });

    let surface = derive_instrument_identifier_succession(missing_balance, successor.clone())
        .expect("missing profile input abstains");
    assert_eq!(
        surface.decision.reason,
        InstrumentSuccessionReason::MissingProfileInput
    );
    assert!(surface.identity_facts.is_empty());
    assert!(surface.relations.is_empty());

    let different_maturity = observation(ObservationSpec {
        surface_id: "surface:different-maturity",
        cusip: Some("333333333"),
        issuer_lei: Some("549300SUCCESSION"),
        maturity_date: Some("2031-06-30"),
        rate_bps: Some(500),
        balance_profile: Some("principal_bucket:10m"),
        figi: None,
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: Some("2026-03-31T23:59:59Z"),
        fragment: "row-different",
    });
    let surface = derive_instrument_identifier_succession(different_maturity, successor)
        .expect("profile mismatch abstains");
    assert_eq!(
        surface.decision.reason,
        InstrumentSuccessionReason::ProfileMismatch
    );
    assert!(surface.identity_facts.is_empty());
    assert!(surface.relations.is_empty());
}

#[test]
fn valid_time_boundaries_must_show_a_handover() {
    let predecessor = observation(ObservationSpec {
        surface_id: "surface:overlap-old",
        cusip: Some("111111111"),
        issuer_lei: Some("549300SUCCESSION"),
        maturity_date: Some("2030-06-30"),
        rate_bps: Some(500),
        balance_profile: Some("principal_bucket:10m"),
        figi: Some("BBG00STABLE1"),
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: Some("2026-06-30T23:59:59Z"),
        fragment: "row-old",
    });
    let successor = observation(ObservationSpec {
        surface_id: "surface:overlap-new",
        cusip: Some("222222222"),
        issuer_lei: Some("549300SUCCESSION"),
        maturity_date: Some("2030-06-30"),
        rate_bps: Some(500),
        balance_profile: Some("principal_bucket:10m"),
        figi: Some("BBG00STABLE1"),
        valid_start: "2026-04-01T00:00:00Z",
        valid_end: Some("2026-09-30T23:59:59Z"),
        fragment: "row-new",
    });

    let surface =
        derive_instrument_identifier_succession(predecessor, successor).expect("overlap abstains");
    assert_eq!(
        surface.decision.reason,
        InstrumentSuccessionReason::NoPeriodHandover
    );
    assert!(surface.identity_facts.is_empty());
    assert!(surface.relations.is_empty());

    let predecessor = observation(ObservationSpec {
        surface_id: "surface:inclusive-old",
        cusip: Some("111111111"),
        issuer_lei: Some("549300SUCCESSION"),
        maturity_date: Some("2030-06-30"),
        rate_bps: Some(500),
        balance_profile: Some("principal_bucket:10m"),
        figi: Some("BBG00STABLE1"),
        valid_start: "2026-01-01T00:00:00Z",
        valid_end: Some("2026-04-01T00:00:00Z"),
        fragment: "row-inclusive-old",
    });
    let successor = observation(ObservationSpec {
        surface_id: "surface:inclusive-new",
        cusip: Some("222222222"),
        issuer_lei: Some("549300SUCCESSION"),
        maturity_date: Some("2030-06-30"),
        rate_bps: Some(500),
        balance_profile: Some("principal_bucket:10m"),
        figi: Some("BBG00STABLE1"),
        valid_start: "2026-04-01T00:00:00Z",
        valid_end: Some("2026-06-30T23:59:59Z"),
        fragment: "row-inclusive-new",
    });
    let surface = derive_instrument_identifier_succession(predecessor, successor)
        .expect("inclusive boundary overlap abstains");
    assert_eq!(
        surface.decision.reason,
        InstrumentSuccessionReason::NoPeriodHandover
    );
    assert!(surface.identity_facts.is_empty());
    assert!(surface.relations.is_empty());
}

#[derive(Clone, Copy)]
struct ObservationSpec<'a> {
    surface_id: &'a str,
    cusip: Option<&'a str>,
    issuer_lei: Option<&'a str>,
    maturity_date: Option<&'a str>,
    rate_bps: Option<i64>,
    balance_profile: Option<&'a str>,
    figi: Option<&'a str>,
    valid_start: &'a str,
    valid_end: Option<&'a str>,
    fragment: &'a str,
}

fn observation(spec: ObservationSpec<'_>) -> InstrumentIdentifierObservation {
    InstrumentIdentifierObservation {
        surface_id: spec.surface_id.to_string(),
        identifier_namespace: "cusip".to_string(),
        identifier_value: spec.cusip.map(ToString::to_string),
        issuer_lei: spec.issuer_lei.map(ToString::to_string),
        maturity_date: spec.maturity_date.map(ToString::to_string),
        annualized_rate_bps: spec.rate_bps,
        balance_profile: spec.balance_profile.map(ToString::to_string),
        figi: spec.figi.map(ToString::to_string),
        valid_time: TimeInterval {
            start_at: Some(spec.valid_start.to_string()),
            start_bound: IntervalBoundary::Inclusive,
            end_at: spec.valid_end.map(ToString::to_string),
            end_bound: if spec.valid_end.is_some() {
                IntervalBoundary::Inclusive
            } else {
                IntervalBoundary::Open
            },
        },
        recorded_time: RecordedTime {
            start_at: Some("2026-07-15T12:00:00Z".to_string()),
            start_bound: IntervalBoundary::Inclusive,
            end_at: None,
            end_bound: IntervalBoundary::Open,
            transaction_seq: Some(1),
        },
        source_locator: SourceLocator {
            source_system: "proof_case_0_fixture".to_string(),
            locator: "tests/fixtures/entity/profiles/instrument_identity.yaml".to_string(),
            fragment: Some(spec.fragment.to_string()),
        },
    }
}
