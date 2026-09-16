#![forbid(unsafe_code)]

use canon::entity::{
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord},
    prepare::PreparedSurfaceRecord,
    run::{EntityRunRequest, run_entity_workbench},
    score::ScoreLane,
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
        .unwrap_or_else(|| panic!("missing prepared surface core {expected}"))
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
        .unwrap_or_else(|| panic!("missing evidence pair {left} <=> {right}"))
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

fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Vec<T> {
    fs::read_to_string(path)
        .expect("jsonl file")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl record parses"))
        .collect()
}
