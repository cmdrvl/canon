#![forbid(unsafe_code)]

use canon::entity::{
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord},
    prepare::PreparedSurfaceRecord,
    run::{EntityRunRequest, run_entity_workbench},
    score::ScoreLane,
};
use canon::temporal::{
    IntervalBoundary, RecordedTime, SourceLocator, TimeInterval,
    instrument::{
        IDENTIFIES_SAME_INSTRUMENT_PREDICATE, InstrumentIdentifierObservation,
        InstrumentSuccessionDecisionKind, InstrumentSuccessionReason,
        derive_instrument_identifier_succession,
    },
    relation::{RelationIdentityImplicationMode, relation_edge_implies_alias},
};
use serde::de::DeserializeOwned;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const PROFILE: &str = "tests/fixtures/entity/profiles/instrument_identity.yaml";

#[test]
fn instrument_profile_runs_prepare_and_evidence_with_configured_mapping() {
    let fixture = InstrumentFixture::new();
    let rows = fixture.write_rows("instruments.csv");
    let first_work = fixture.path("work-first");
    let second_work = fixture.path("work-second");

    run_fixture(&rows, &fixture.registry, &first_work);
    run_fixture(&rows, &fixture.registry, &second_work);

    let first_surfaces =
        fs::read(first_work.join("prepare/surfaces.jsonl")).expect("first prepare surfaces");
    let second_surfaces =
        fs::read(second_work.join("prepare/surfaces.jsonl")).expect("second prepare surfaces");
    assert_eq!(
        first_surfaces, second_surfaces,
        "instrument prepare surfaces must be byte-reproducible"
    );

    let surfaces: Vec<PreparedSurfaceRecord> =
        read_jsonl(&first_work.join("prepare/surfaces.jsonl"));
    let by_core = surface_ids_by_core(&surfaces);
    let positive_surface = surface_by_core(&surfaces, "acme term loan 2030");
    assert_eq!(
        positive_surface.normalized_views["cusip"].value,
        "037833100"
    );
    assert_eq!(
        positive_surface.normalized_views["instrument_maturity"].value,
        "2030-06-30"
    );
    assert!(
        positive_surface
            .anchors
            .iter()
            .any(|anchor| anchor.namespace == "figi" && anchor.value == "BBG000B9XRY4")
    );

    let placeholder_surface = surface_by_core(&surfaces, "acme term loan placeholder");
    assert!(
        !placeholder_surface.normalized_views.contains_key("cusip"),
        "000000000 placeholder CUSIP must not become a prepared identifier view"
    );
    assert!(
        placeholder_surface.anchors.is_empty(),
        "N/A FIGI placeholder must not become an anchor"
    );

    let evidence: Vec<EdgeEvidenceRecord> = read_jsonl(&first_work.join("evidence/evidence.jsonl"));
    let positive = record_for_cores(
        &evidence,
        &by_core,
        "acme term loan 2030",
        "acme term loan due 2030",
    );
    assert!(support_hit(positive, "anchor_match:figi").is_some());
    assert!(support_hit(positive, "isin_cusip_arithmetic:cusip:isin").is_some());
    assert!(anti_merge_hit(positive, "attribute_conflict:instrument_maturity").is_none());
    assert!(anti_merge_hit(positive, "attribute_conflict:annualized_rate").is_none());

    let contradiction = record_for_cores(
        &evidence,
        &by_core,
        "acme term loan 2030",
        "acme term loan 2031",
    );
    assert!(anti_merge_hit(contradiction, "anchor_conflict:figi").is_some());
    assert!(anti_merge_hit(contradiction, "attribute_conflict:instrument_maturity").is_some());
    assert!(anti_merge_hit(contradiction, "attribute_conflict:annualized_rate").is_some());
    assert!(support_hit(contradiction, "isin_cusip_arithmetic:cusip:isin").is_none());

    let missing_records = evidence
        .iter()
        .filter(|record| record_has_core(record, &by_core, "acme term loan placeholder"))
        .collect::<Vec<_>>();
    assert!(
        !missing_records.is_empty(),
        "placeholder row should still participate in candidate review"
    );
    assert!(
        missing_records.iter().all(|record| {
            support_hit(record, "anchor_match:figi").is_none()
                && support_hit(record, "isin_cusip_arithmetic:cusip:isin").is_none()
                && anti_merge_hit(record, "anchor_conflict:figi").is_none()
                && anti_merge_hit(record, "attribute_conflict:instrument_maturity").is_none()
                && anti_merge_hit(record, "attribute_conflict:annualized_rate").is_none()
        }),
        "placeholder identifiers and attributes must abstain"
    );

    let share_class_pair = record_for_cores(
        &evidence,
        &by_core,
        "acme class a common shares",
        "acme class c common shares",
    );
    assert!(
        share_class_pair
            .hits
            .iter()
            .all(|hit| hit.lane != ScoreLane::Support),
        "same issuer LEI must not produce same-instrument support"
    );
    assert!(
        share_class_pair
            .hits
            .iter()
            .all(|hit| !hit.operator_id.contains("issuer_lei")),
        "issuer LEI is relation metadata, not an identity evidence operator"
    );
}

#[test]
fn instrument_profile_outputs_feed_temporal_cusip_succession_without_issuer_merge_support() {
    let fixture = InstrumentFixture::new();
    let rows = fixture.write_rows("temporal-instruments.csv");
    let work = fixture.path("temporal-work");

    run_fixture(&rows, &fixture.registry, &work);

    let surfaces: Vec<PreparedSurfaceRecord> = read_jsonl(&work.join("prepare/surfaces.jsonl"));
    let by_core = surface_ids_by_core(&surfaces);
    let evidence: Vec<EdgeEvidenceRecord> = read_jsonl(&work.join("evidence/evidence.jsonl"));
    let handover_record = record_for_cores(
        &evidence,
        &by_core,
        "acme term loan legacy cusip",
        "acme term loan new cusip",
    );
    assert!(support_hit(handover_record, "anchor_match:figi").is_some());
    assert!(anti_merge_hit(handover_record, "anchor_conflict:figi").is_none());
    assert!(
        handover_record
            .hits
            .iter()
            .all(|hit| !hit.operator_id.contains("issuer_lei")),
        "issuer LEI must remain relation metadata in the evidence path"
    );

    let predecessor = temporal_observation(
        surface_by_core(&surfaces, "acme term loan legacy cusip"),
        PeriodSpec {
            start_at: "2026-01-01T00:00:00Z",
            end_at: "2026-03-31T23:59:59Z",
            fragment: "row-7",
        },
    );
    let successor = temporal_observation(
        surface_by_core(&surfaces, "acme term loan new cusip"),
        PeriodSpec {
            start_at: "2026-04-01T00:00:00Z",
            end_at: "2026-06-30T23:59:59Z",
            fragment: "row-8",
        },
    );

    let succession = derive_instrument_identifier_succession(predecessor, successor)
        .expect("entity-prepared surfaces feed temporal succession");
    assert_eq!(
        succession.decision.kind,
        InstrumentSuccessionDecisionKind::Succession
    );
    assert_eq!(
        succession.decision.reason,
        InstrumentSuccessionReason::ValidTimeEqualityWithSupersededByRelation
    );
    assert_eq!(succession.identity_facts.len(), 1);
    assert_eq!(succession.relations.len(), 1);
    assert_eq!(
        succession.identity_facts[0].predicate,
        IDENTIFIES_SAME_INSTRUMENT_PREDICATE
    );
    assert_eq!(
        succession.identity_facts[0].valid_time.start_at.as_deref(),
        Some("2026-04-01T00:00:00Z")
    );
    assert_eq!(
        succession.relations[0].identity_implication.mode,
        RelationIdentityImplicationMode::SupportedBySeparateEqualityFact
    );
    assert_eq!(
        succession.relations[0]
            .identity_implication
            .equality_fact_ref
            .as_deref(),
        Some(succession.identity_facts[0].fact_id.as_str())
    );
    assert!(!relation_edge_implies_alias(&succession.relations[0]));
}

#[test]
fn instrument_surfaces_key_on_identifier_tuple_not_title() {
    let fixture = InstrumentFixture::new();
    let rows = fixture.path("same-title.csv");
    fs::write(
        &rows,
        concat!(
            "source_row_id,report_period,title,cusip,isin,figi,maturitydt,annualizedrt,issuer_lei,share_class\n",
            "row-1,2024-03-31,Alpha 2029 Note,111111AA1,US111111AA11,BBG00ALPHA111,2029-01-15,4.500,LEIALPHA,A\n",
            "row-2,2024-06-30,Alpha 2029 Note,111111AB9,US111111AB99,BBG00ALPHA111,2029-01-15,4.500,LEIALPHA,A\n",
            "row-3,2024-03-31,Conflicted Note,666666AA6,US666666AA66,,2030-12-15,5.250,LEICONFLICT,A\n",
            "row-4,2024-06-30,Conflicted Note,888888AA8,US888888AA88,,2040-12-15,5.250,LEICONFLICT,A\n",
            "row-5,2024-06-30,Foreign Equity,,GB00PLACE001,,,,,A\n",
        ),
    )
    .expect("rows csv");
    let work = fixture.path("same-title-work");

    run_fixture(&rows, &fixture.registry, &work);

    let surfaces: Vec<PreparedSurfaceRecord> = read_jsonl(&work.join("prepare/surfaces.jsonl"));
    assert_eq!(
        surfaces.len(),
        5,
        "every per-period identifier tuple must be its own surface"
    );
    let cusips = surfaces
        .iter()
        .filter_map(|surface| surface.normalized_views.get("cusip"))
        .map(|view| view.value.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        cusips,
        ["111111AA1", "111111AB9", "666666AA6", "888888AA8"]
            .into_iter()
            .collect(),
        "a re-CUSIP must not be collapsed into the first CUSIP seen under a title"
    );
    let empty_cusip = surfaces
        .iter()
        .find(|surface| surface.primary_surface == "Foreign Equity")
        .expect("row with an empty CUSIP is prepared, not refused");
    assert!(!empty_cusip.normalized_views.contains_key("cusip"));

    let by_cusip = surfaces
        .iter()
        .filter_map(|surface| {
            surface
                .normalized_views
                .get("cusip")
                .map(|view| (view.value.clone(), surface.surface_id.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let evidence: Vec<EdgeEvidenceRecord> = read_jsonl(&work.join("evidence/evidence.jsonl"));
    let conflicted = evidence
        .iter()
        .find(|record| {
            let pair = [&record.left_surface_id, &record.right_surface_id];
            pair.contains(&&by_cusip["666666AA6"]) && pair.contains(&&by_cusip["888888AA8"])
        })
        .expect("same-title pair reaches evidence as two surfaces");
    assert!(anti_merge_hit(conflicted, "attribute_conflict:instrument_maturity").is_some());

    let recusip = evidence
        .iter()
        .find(|record| {
            let pair = [&record.left_surface_id, &record.right_surface_id];
            pair.contains(&&by_cusip["111111AA1"]) && pair.contains(&&by_cusip["111111AB9"])
        })
        .expect("re-CUSIP pair reaches evidence as two surfaces");
    assert!(support_hit(recusip, "anchor_match:figi").is_some());
    assert!(anti_merge_hit(recusip, "attribute_conflict:instrument_maturity").is_none());
}

#[test]
fn placeholder_looking_title_keeps_its_surface_id_view() {
    // Real N-PORT rows carry titles like "N/A"; the title is display text,
    // not an identifier, so it must not be nulled into a surface with no
    // surface-id view (which refused the whole run on 2024 corporate debt).
    let fixture = InstrumentFixture::new();
    let rows = fixture.path("placeholder-title.csv");
    fs::write(
        &rows,
        concat!(
            "source_row_id,report_period,title,cusip,isin,figi,maturitydt,annualizedrt,issuer_lei,share_class\n",
            "row-1,2024-03-31,N/A,111111AA1,,,2029-01-15,4.500,,\n",
            "row-2,2024-06-30,N/A,111111AA1,,,2029-01-15,4.500,,\n",
            "row-3,2024-03-31,Alpha 2029 Note,222222AA2,,,2029-01-15,4.500,,\n",
        ),
    )
    .expect("rows csv");
    let work = fixture.path("placeholder-title-work");

    run_fixture(&rows, &fixture.registry, &work);

    let surfaces: Vec<PreparedSurfaceRecord> = read_jsonl(&work.join("prepare/surfaces.jsonl"));
    assert_eq!(surfaces.len(), 3);
    let placeholder_titled = surfaces
        .iter()
        .filter(|surface| surface.primary_surface == "N/A")
        .collect::<Vec<_>>();
    assert_eq!(placeholder_titled.len(), 2);
    for surface in placeholder_titled {
        assert!(
            surface.normalized_views["core"]
                .reason_codes
                .iter()
                .any(|reason| reason == "surface_id_view")
        );
        assert_eq!(surface.normalized_views["cusip"].value, "111111AA1");
    }
}

fn exchange_twin_rows(
    fixture: &InstrumentFixture,
    name: &str,
    group: &str,
    new_maturity: &str,
) -> PathBuf {
    // AerCap 6.450% 2027 shape: 144A CUSIP through 2024-03, registered
    // exchange CUSIP from 2024-04, distinct FIGIs by design, different
    // filer titles.
    let rows = fixture.path(name);
    fs::write(
        &rows,
        format!(
            "source_row_id,report_period,title,cusip,isin,figi,maturitydt,annualizedrt,issuer_lei,share_class,exchange_group\n\
             row-1,2024-03-31,AERCAP IRELAND CAP GLOBA,00774MBF1,,BBG01K8GY0D8,2027-04-15,6.450,549300TI38531ODB1G63,,{group}\n\
             row-2,2024-04-30,AerCap Ireland Capital DAC 6.45% 2027,00774MBG9,US00774MBG95,BBG01M1MGL15,{new_maturity},6.450,549300TI38531ODB1G63,,{group}\n"
        ),
    )
    .expect("rows csv");
    rows
}

fn twin_evidence(work: &Path) -> EdgeEvidenceRecord {
    let surfaces: Vec<PreparedSurfaceRecord> = read_jsonl(&work.join("prepare/surfaces.jsonl"));
    assert_eq!(surfaces.len(), 2);
    let evidence: Vec<EdgeEvidenceRecord> = read_jsonl(&work.join("evidence/evidence.jsonl"));
    evidence
        .into_iter()
        .find(|record| {
            let ids = [&record.left_surface_id, &record.right_surface_id];
            surfaces
                .iter()
                .all(|surface| ids.contains(&&surface.surface_id))
        })
        .expect("twins reach evidence as a candidate pair")
}

#[test]
fn exchange_offer_twins_link_despite_distinct_figis() {
    let fixture = InstrumentFixture::new();
    let rows = exchange_twin_rows(
        &fixture,
        "twins.csv",
        "EXO:0000950157-24-000611",
        "2027-04-15",
    );
    let work = fixture.path("twins-work");
    run_fixture(&rows, &fixture.registry, &work);

    let record = twin_evidence(&work);
    assert!(support_hit(&record, "anchor_match:exchange_group").is_some());
    assert!(
        anti_merge_hit(&record, "anchor_conflict:figi").is_none(),
        "a shared exchange group must not be cut by the FIGI difference"
    );
    assert!(anti_merge_hit(&record, "attribute_conflict:instrument_maturity").is_none());
}

#[test]
fn distinct_figis_without_exchange_evidence_stay_cannot_linked() {
    let fixture = InstrumentFixture::new();
    let rows = exchange_twin_rows(&fixture, "no-group.csv", "", "2027-04-15");
    let work = fixture.path("no-group-work");
    run_fixture(&rows, &fixture.registry, &work);

    let record = twin_evidence(&work);
    assert!(anti_merge_hit(&record, "anchor_conflict:figi").is_some());
    assert!(support_hit(&record, "anchor_match:exchange_group").is_none());
}

#[test]
fn shared_exchange_group_does_not_override_a_maturity_conflict() {
    let fixture = InstrumentFixture::new();
    let rows = exchange_twin_rows(
        &fixture,
        "bad-group.csv",
        "EXO:0000950157-24-000611",
        "2029-04-15",
    );
    let work = fixture.path("bad-group-work");
    run_fixture(&rows, &fixture.registry, &work);

    let record = twin_evidence(&work);
    assert!(anti_merge_hit(&record, "attribute_conflict:instrument_maturity").is_some());
}

#[test]
fn title_keyed_profile_still_merges_same_title_rows() {
    // Negative control: without surface_key_fields the default key is the
    // canonical title view, which is exactly the collapse the instrument
    // profile must avoid. Proves the positive test can observe a merge.
    let fixture = InstrumentFixture::new();
    let rows = fixture.path("title-keyed.csv");
    fs::write(
        &rows,
        concat!(
            "source_row_id,report_period,title,cusip,isin,figi,maturitydt,annualizedrt,issuer_lei,share_class\n",
            "row-1,2024-03-31,Conflicted Note,666666AA6,US666666AA66,N/A,2030-12-15,5.250,LEICONFLICT,A\n",
            "row-2,2024-06-30,Conflicted Note,888888AA8,US888888AA88,N/A,2040-12-15,5.250,LEICONFLICT,A\n",
        ),
    )
    .expect("rows csv");
    let profile_source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(PROFILE))
        .expect("profile yaml");
    let start = profile_source
        .find("  surface_key_fields:")
        .expect("profile declares surface_key_fields");
    let end = profile_source[start..]
        .find("  nullable_fields:")
        .map(|offset| start + offset)
        .expect("profile declares nullable_fields");
    let title_keyed = format!("{}{}", &profile_source[..start], &profile_source[end..]);
    let strategy = fixture.path("title-keyed.yaml");
    fs::write(&strategy, title_keyed).expect("title keyed profile");
    let work = fixture.path("title-keyed-work");

    run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: strategy.to_str().expect("utf8 profile path"),
        strategy: &strategy,
        registry: &fixture.registry,
        work_dir: &work,
    })
    .expect("title keyed run succeeds");

    let surfaces: Vec<PreparedSurfaceRecord> = read_jsonl(&work.join("prepare/surfaces.jsonl"));
    assert_eq!(surfaces.len(), 1, "title key merges both rows");
}

#[test]
fn surface_key_field_must_be_declared() {
    let fixture = InstrumentFixture::new();
    let rows = fixture.write_rows("undeclared-key.csv");
    let profile_source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(PROFILE))
        .expect("profile yaml");
    let broken = profile_source.replace(
        "  surface_key_fields:\n    - core\n",
        "  surface_key_fields:\n    - core\n    - not_a_declared_field\n",
    );
    assert_ne!(broken, profile_source);
    let strategy = fixture.path("undeclared-key.yaml");
    fs::write(&strategy, broken).expect("broken profile");

    let error = run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: strategy.to_str().expect("utf8 profile path"),
        strategy: &strategy,
        registry: &fixture.registry,
        work_dir: &fixture.path("undeclared-key-work"),
    })
    .expect_err("undeclared surface key field refuses");
    assert!(
        format!("{error:?}").contains("not_a_declared_field"),
        "refusal names the offending field: {error:?}"
    );
}

struct InstrumentFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    registry: PathBuf,
}

impl InstrumentFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().to_path_buf();
        let registry = root.join("registry");
        fs::create_dir_all(&registry).expect("registry dir");
        fs::write(
            registry.join("registry.json"),
            r#"{"id":"instrument-test","version":"2026.09.16","description":"Instrument profile test registry","updated":"2026-09-16","entry_count":0}"#,
        )
        .expect("registry metadata");
        fs::write(registry.join("aliases.json"), "[]").expect("empty aliases");
        Self {
            _temp: temp,
            root,
            registry,
        }
    }

    fn path(&self, relpath: &str) -> PathBuf {
        self.root.join(relpath)
    }

    fn write_rows(&self, name: &str) -> PathBuf {
        let path = self.path(name);
        fs::write(
            &path,
            concat!(
                "source_row_id,report_period,title,cusip,isin,figi,maturitydt,annualizedrt,issuer_lei,share_class\n",
                "row-1,2026-06-30,Acme Term Loan 2030,037833100,N/A,BBG000B9XRY4,2030-06-30,5.000,549300ACMEISSUER,A\n",
                "row-2,2026-06-30,Acme Term Loan Due 2030,N/A,US0378331005,BBG000B9XRY4,2030-06-30,5.010,549300ACMEISSUER,A\n",
                "row-3,2026-06-30,Acme Term Loan 2031,594918104,US5949181044,BBG000DIFFER,2031-06-30,5.020,549300ACMEISSUER,A\n",
                "row-4,2026-06-30,Acme Term Loan Placeholder,000000000,NA,N/A,N/A,NA,549300ACMEISSUER,A\n",
                "row-5,2026-06-30,Acme Class A Common Shares,N/A,NA,N/A,N/A,NA,549300ACMEISSUER,A\n",
                "row-6,2026-06-30,Acme Class C Common Shares,N/A,NA,N/A,N/A,NA,549300ACMEISSUER,C\n",
                "row-7,2026-03-31,Acme Term Loan Legacy CUSIP,111111111,N/A,BBG00SUCCESS,2030-06-30,5.000,549300ACMEISSUER,A\n",
                "row-8,2026-06-30,Acme Term Loan New CUSIP,222222222,N/A,BBG00SUCCESS,2030-06-30,5.000,549300ACMEISSUER,A\n",
            ),
        )
        .expect("rows csv");
        path
    }
}

fn run_fixture(rows: &Path, registry: &Path, work_dir: &Path) {
    let strategy = Path::new(env!("CARGO_MANIFEST_DIR")).join(PROFILE);
    run_entity_workbench(EntityRunRequest {
        rows,
        profile: "instrument_identity",
        strategy: &strategy,
        registry,
        work_dir,
    })
    .expect("instrument profile run succeeds");
}

fn surface_ids_by_core(surfaces: &[PreparedSurfaceRecord]) -> BTreeMap<String, String> {
    surfaces
        .iter()
        .map(|surface| {
            (
                surface.normalized_views["core"].value.clone(),
                surface.surface_id.clone(),
            )
        })
        .collect()
}

fn surface_by_core<'a>(
    surfaces: &'a [PreparedSurfaceRecord],
    expected: &str,
) -> &'a PreparedSurfaceRecord {
    surfaces
        .iter()
        .find(|surface| surface.normalized_views["core"].value == expected)
        .expect("missing prepared surface core")
}

fn record_for_cores<'a>(
    records: &'a [EdgeEvidenceRecord],
    surface_ids: &BTreeMap<String, String>,
    left: &str,
    right: &str,
) -> &'a EdgeEvidenceRecord {
    let left_id = surface_ids.get(left).expect("left surface");
    let right_id = surface_ids.get(right).expect("right surface");
    records
        .iter()
        .find(|record| {
            (&record.left_surface_id == left_id && &record.right_surface_id == right_id)
                || (&record.left_surface_id == right_id && &record.right_surface_id == left_id)
        })
        .expect("missing evidence pair")
}

fn record_has_core(
    record: &EdgeEvidenceRecord,
    surface_ids: &BTreeMap<String, String>,
    expected: &str,
) -> bool {
    let Some(expected_id) = surface_ids.get(expected) else {
        return false;
    };
    &record.left_surface_id == expected_id || &record.right_surface_id == expected_id
}

fn support_hit<'a>(
    record: &'a EdgeEvidenceRecord,
    operator_id: &str,
) -> Option<&'a EdgeEvidenceHit> {
    record
        .hits
        .iter()
        .find(|hit| hit.lane == ScoreLane::Support && hit.operator_id == operator_id)
}

fn anti_merge_hit<'a>(
    record: &'a EdgeEvidenceRecord,
    operator_id: &str,
) -> Option<&'a EdgeEvidenceHit> {
    record
        .hits
        .iter()
        .find(|hit| hit.lane == ScoreLane::AntiMerge && hit.operator_id == operator_id)
}

struct PeriodSpec<'a> {
    start_at: &'a str,
    end_at: &'a str,
    fragment: &'a str,
}

fn temporal_observation(
    surface: &PreparedSurfaceRecord,
    period: PeriodSpec<'_>,
) -> InstrumentIdentifierObservation {
    InstrumentIdentifierObservation {
        surface_id: surface.surface_id.clone(),
        identifier_namespace: "cusip".to_string(),
        identifier_value: surface
            .normalized_views
            .get("cusip")
            .map(|view| view.value.clone()),
        issuer_lei: Some("549300ACMEISSUER".to_string()),
        maturity_date: surface
            .normalized_views
            .get("instrument_maturity")
            .map(|view| view.value.clone()),
        annualized_rate_bps: Some(500),
        balance_profile: Some("principal_bucket:10m".to_string()),
        figi: surface
            .anchors
            .iter()
            .find(|anchor| anchor.namespace == "figi")
            .map(|anchor| anchor.value.clone()),
        valid_time: TimeInterval {
            start_at: Some(period.start_at.to_string()),
            start_bound: IntervalBoundary::Inclusive,
            end_at: Some(period.end_at.to_string()),
            end_bound: IntervalBoundary::Inclusive,
        },
        recorded_time: RecordedTime {
            start_at: Some("2026-07-15T12:00:00Z".to_string()),
            start_bound: IntervalBoundary::Inclusive,
            end_at: None,
            end_bound: IntervalBoundary::Open,
            transaction_seq: Some(1),
        },
        source_locator: SourceLocator {
            source_system: "instrument_profile_fixture".to_string(),
            locator: PROFILE.to_string(),
            fragment: Some(period.fragment.to_string()),
        },
    }
}

fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Vec<T> {
    fs::read_to_string(path)
        .expect("jsonl file")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl record parses"))
        .collect()
}
