//! Retained integration gate. These subjects are development evidence, never held-out truth.
#[path = "common/retained_archive.rs"]
mod retained_archive;

use retained_archive::{RetainedDigest, read_json_gz_files, verify_archive};
use serde::Deserialize;
use serde_json::Value;
use std::{fs, path::Path, process::Command};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum GeoReplayInvariant {
    CompositionStatus {
        artifact_id: String,
        value: String,
    },
    ResidualSaturated {
        artifact_id: String,
    },
    ResidualCount {
        artifact_id: String,
        value: u64,
    },
    ForcedMembers {
        artifact_id: String,
        parcels: Vec<String>,
        buildings: Vec<String>,
    },
    NoForcedMember {
        artifact_id: String,
    },
    ComponentHardFeasible {
        artifact_id: String,
        key: String,
        count: u64,
    },
    SoftTopTie {
        artifact_id: String,
        members: Vec<String>,
        cost: u64,
    },
    SoftAllTied {
        artifact_id: String,
        count: u64,
    },
    RunStatus {
        value: String,
    },
    FailedNode {
        node_id: String,
        message_contains: String,
    },
    PointerEquals {
        artifact_id: Option<String>,
        pointer: String,
        value: Value,
        projection: Option<String>,
        observation_ids: Option<Vec<String>>,
    },
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
fn catalog() -> Value {
    serde_json::from_slice(
        &fs::read(root().join("tests/fixtures/geo/replay/catalog.json")).unwrap(),
    )
    .unwrap()
}

fn replay(case: &str) {
    // Keep the evidence: TempDir cleanup would discard the required command/error logs.
    let directory = tempfile::Builder::new()
        .prefix("canon-retained-test-")
        .tempdir()
        .unwrap()
        .keep();
    let output = Command::new("python3")
        .arg("-I")
        .arg(root().join("scripts/geo_replay/replay.py"))
        .args([
            "--case",
            case,
            "--binary",
            env!("CARGO_BIN_EXE_canon"),
            "--out",
        ])
        .arg(&directory)
        .output()
        .unwrap();
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "retained case {case}; logs {}",
        directory.display()
    );
    let log = fs::read_to_string(directory.join(case).join("run.log")).unwrap();
    let events: Vec<Value> = log
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let commands: Vec<_> = events
        .iter()
        .filter(|r| r["event"] == "command")
        .map(|r| r["argv"].clone())
        .collect();
    assert_eq!(
        commands.len(),
        if case.ends_with("workspace_refusal") || case.starts_with("resume.") {
            2
        } else if case.ends_with("address_evidence") || case == "g4.retry_recovery" {
            1
        } else {
            3
        },
        "declared public command sequence executed"
    );
    let usage = events.iter().find(|r| r["event"] == "USAGE").unwrap();
    assert_eq!(
        usage["argv"],
        serde_json::to_value(commands).unwrap(),
        "RPL-13 demonstration equals executed commands"
    );
    let relative_commands: Vec<_> = events
        .iter()
        .filter(|r| r["event"] == "command")
        .map(|r| r["argv_relative"].clone())
        .collect();
    assert_eq!(
        usage["argv_relative"],
        serde_json::to_value(relative_commands).unwrap()
    );
    for artifact in events.iter().filter(|r| r["event"] == "artifact") {
        let bytes = fs::read(artifact["path"].as_str().unwrap()).unwrap();
        assert_eq!(
            format!("blake3:{}", blake3::hash(&bytes)),
            artifact["content_digest"].as_str().unwrap()
        );
    }
    if case.starts_with("brooklyn.") && case != "brooklyn.address_evidence" {
        let run: Value =
            serde_json::from_slice(&fs::read(directory.join(case).join("run.json")).unwrap())
                .unwrap();
        assert_eq!(run["status"], "FAILED", "RPL-15: exit zero is insufficient");
        assert!(
            events
                .iter()
                .filter(|r| r["event"] == "command")
                .all(|r| r["exit"] == 0)
        );
        assert_ne!(
            run["status"], "COMPLETED",
            "a status-vs-exit assertion is load-bearing"
        );
    }
}

#[test]
fn fast_brooklyn_00_neighborhood() {
    replay("brooklyn.00_neighborhood");
}
#[test]
fn fast_brooklyn_01_address_parcel() {
    replay("brooklyn.01_address_parcel");
}
#[test]
fn fast_brooklyn_02_address_building() {
    replay("brooklyn.02_address_building");
}
#[test]
fn fast_cornerstone_national_building_supported() {
    replay("cornerstone.national_building_supported");
}
#[test]
fn fast_cornerstone_national_building() {
    replay("cornerstone.national_building");
}
#[test]
fn fast_brooklyn_address_evidence() {
    replay("brooklyn.address_evidence");
}

#[test]
fn fast_bank18_nyc_address_evidence() {
    replay("bank18.nyc_address_evidence");
}
#[test]
fn fast_nxrt_failed_baseline() {
    replay("nxrt.experiment");
}
#[test]
fn fast_orlando_lineage_order_failure() {
    replay("orlando.case0_hints.lineage_order_failure");
}
#[test]
fn fast_g4_retry_recovery() {
    replay("g4.retry_recovery");
}
#[test]
fn fast_orlando_workspace_refusal() {
    replay("orlando.case0_hints.workspace_refusal");
}
#[test]
fn resume_cornerstone_national_parcel_supported() {
    replay("resume.cornerstone.national_parcel_supported");
}
#[test]
fn resume_courtney_final_hints() {
    replay("resume.courtney.final_hints");
}
#[test]
fn rpl_16_resume_nxrt_preserves_visible_recompute_divergence() {
    replay("resume.nxrt.experiment");
}

fn python_test(source: &str) {
    let output = Command::new("python3")
        .args(["-I", "-c", source])
        .current_dir(root())
        .output()
        .unwrap();
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(output.status.success());
}

#[test]
fn rpl_05_corrupt_retained_archive_refuses_before_canon() {
    let c = catalog();
    let case = c["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["case_id"] == "cornerstone.national_building_supported")
        .unwrap();
    let path = root().join(case["archive"]["path"].as_str().unwrap());
    let expected = RetainedDigest::Sha256(case["archive"]["sha256"].as_str().unwrap().into());
    verify_archive(&path, &expected).unwrap();
    let mut bytes = fs::read(path).unwrap();
    let at = bytes.len() / 2;
    bytes[at] ^= 1;
    let temp = tempfile::NamedTempFile::new().unwrap();
    fs::write(temp.path(), bytes).unwrap();
    assert_eq!(
        verify_archive(temp.path(), &expected).unwrap_err().code,
        "digest_mismatch"
    );
    // Verifier API cannot launch Canon. Public harness also verifies before creating its command log.
}

#[test]
fn rpl_05b_corrupt_extracted_member_refuses_before_canon() {
    python_test(
        r#"
import copy,runpy,json,pathlib,tempfile
m=runpy.run_path('scripts/geo_replay/replay.py');c=json.loads(pathlib.Path('tests/fixtures/geo/replay/catalog.json').read_text())
case=copy.deepcopy(next(row for row in c['cases'] if row['case_id']=='cornerstone.national_building_supported'))
files=m['load_archive'](case);name=case['plan_inputs']['question'];data=bytearray(files[name]);data[len(data)//2]^=1;files[name]=bytes(data)
out=pathlib.Path(tempfile.mkdtemp(prefix='canon-corrupt-member-'))/'case'
called=[]
def forbidden(*args,**kwargs):
 called.append(args);raise AssertionError('Canon process ran before corrupted member refusal')
globals=m['replay'].__globals__;original=globals['load_archive'];command=m['Log'].command
try:
 globals['load_archive']=lambda related: files if related['archive']['path']==case['archive']['path'] else original(related);m['Log'].command=forbidden
 try:m['replay'](case,pathlib.Path('/must-not-execute-canon'),out)
 except m['ReplayError'] as e:assert e.code=='digest_mismatch',str(e)
 else:raise AssertionError('corrupt extracted member accepted')
 assert not called and not out.exists(), 'command log/process created before evidence validation'
finally:
 globals['load_archive']=original;m['Log'].command=command
"#,
    );
}

#[test]
fn rpl_06_07_proof_class_and_exact_golden_chain() {
    python_test(
        r#"
import copy,runpy,json,pathlib
m=runpy.run_path('scripts/geo_replay/replay.py');c=json.loads(pathlib.Path('tests/fixtures/geo/replay/catalog.json').read_text())['cases'][0]
for proof in ['live','fresh']:
 d=copy.deepcopy(c);d['proof_class']=proof
 try:m['load_archive'](d)
 except m['ReplayError'] as e:assert e.code=='proof_class_invalid'
 else:raise AssertionError('accepted inflated proof class')
d=copy.deepcopy(c);d['golden_changes']=[]
def reject(case,code):
 try:m['parity'](case,'test-artifact','old','new','test.v0')
 except m['ReplayError'] as e:assert e.code==code
 else:raise AssertionError('unapproved drift passed')
reject(d,'digest_drift')
g=dict(bead='bd-2omi',date='2026-10-21',artifact_ids=['different-artifact'],old='old',new='new',semantic_note='negative test',changed_paths=['/field'])
d['golden_changes']=[g];reject(d,'digest_drift')
g['artifact_ids']=['test-artifact'];m['parity'](d,'test-artifact','old','new','test.v0')
g['old']='fabricated';reject(d,'invalid_golden_change')
"#,
    );
}

#[test]
fn rpl_08_09_catalog_retained_expectations_and_members_are_immutable() {
    for case in catalog()["cases"].as_array().unwrap() {
        for check in case["invariants"].as_array().unwrap() {
            serde_json::from_value::<GeoReplayInvariant>(check.clone()).unwrap();
        }
    }
    python_test(
        r#"
import runpy,json,pathlib,copy
m=runpy.run_path('scripts/geo_replay/replay.py');catalog=json.loads(pathlib.Path('tests/fixtures/geo/replay/catalog.json').read_text())
for c in catalog['cases']:
 files=m['load_archive'](c);m['retained'](c,files)
 for check in c['invariants']:assert check['kind']
 m['load_archive'](c)
c=copy.deepcopy(catalog['cases'][0]);c['run_bindings'][0]['binding_id']='fabricated'
try:m['retained'](c,m['load_archive'](c))
except m['ReplayError'] as e:assert e.code=='argv_binding_drift'
else:raise AssertionError('mutated binding passed')
"#,
    );
}

#[test]
fn rpl_12_shared_logging_preserves_command_failure() {
    let out = tempfile::Builder::new()
        .prefix("canon-e2e-lib-")
        .tempdir()
        .unwrap()
        .keep();
    let output = Command::new("bash").args(["-euo", "pipefail", "-c",
        "source scripts/geo_replay/e2e_lib.sh; e2e_init \"$1\" self-test retained; e2e_cmd version 0 -- \"$CANON_BIN\" --version; e2e_cmd planted-failure 0 -- false; e2e_finish",
        "e2e-self-test"]).arg(&out).env("CANON_BIN", env!("CARGO_BIN_EXE_canon")).current_dir(root()).output().unwrap();
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        !output.status.success(),
        "planted false must fail even though expected exit is zero"
    );
    let rows: Vec<Value> = fs::read_to_string(out.join("run.log"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(
        rows.iter()
            .all(|r| r["event"].is_string() && r["ts"].is_string())
    );
    assert!(
        rows.iter()
            .any(|r| r["event"] == "command" && r["exit"] == 1)
    );
    assert!(rows.iter().any(|r| r["event"] == "error"));
}

#[test]
fn rpl_17_missing_evidence_fails_never_skips() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(
        verify_archive(
            &tmp.path().join("absent"),
            &RetainedDigest::Sha256("0".repeat(64))
        )
        .unwrap_err()
        .code,
        "missing_member"
    );
    let c = catalog();
    let a = &c["cases"][0]["archive"];
    let files = read_json_gz_files(
        &root().join(a["path"].as_str().unwrap()),
        &RetainedDigest::Sha256(a["sha256"].as_str().unwrap().into()),
    )
    .unwrap();
    assert!(files.contains_key("02_address_building/warehouse_rows.json"));
    assert_eq!(
        c["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["tier"] == "fast")
            .count(),
        11
    );
}

#[test]
fn rpl_11_fixture_coverage_has_real_consumers() {
    let c = catalog();
    for directory in fs::read_dir(root().join("scripts/geo_measurements/fixtures")).unwrap() {
        let directory = directory.unwrap();
        if !directory.file_type().unwrap().is_dir() {
            continue;
        }
        let name = directory.file_name().into_string().unwrap();
        let source = c["cases"].as_array().unwrap().iter().any(|case| {
            case["archive"]["path"]
                .as_str()
                .unwrap()
                .contains(&format!("fixtures/{name}/"))
        });
        if source {
            continue;
        }
        let external = c["external_coverage"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["fixture_dir"] == name)
            .unwrap_or_else(|| panic!("uncovered retained fixture {name}"));
        let consumers = external["consumers"].as_array().unwrap();
        assert!(!consumers.is_empty());
        for file in consumers {
            assert!(
                fs::read_to_string(root().join(file.as_str().unwrap()))
                    .unwrap()
                    .contains(&name)
            );
        }
    }
}

#[test]
fn rpl_14_development_subjects_include_retained_candidates_and_g4_points() {
    let c = catalog();
    let subjects = c["development_subjects"].as_array().unwrap();
    for known in [
        "3051090025",
        "3117371",
        "a6145c70-10cb-46d0-95b8-b85ff82ac700",
    ] {
        assert!(subjects.iter().any(|s| s == known));
    }
    for n in 1..=40 {
        assert!(
            subjects
                .iter()
                .any(|s| s == &format!("e1.gross_class.{n:04}"))
        );
    }
    python_test(
        r#"
import runpy,pathlib,json
m=runpy.run_path('scripts/geo_replay/replay.py');c=json.loads(pathlib.Path('tests/fixtures/geo/replay/catalog.json').read_text())
assert m['development_subjects'](c) == set(c['development_subjects']), 'development exclusion export differs from source subjects'
sample={'contract':{'id':'shared-contract'},'profile_id':'shared-profile','inventory_id':'shared-inventory','members':[{'level':'parcel','id':'parcel-member'}],'parcels':['parcel-array'],'buildings':['building-array'],'subject_bindings':[{'role':'target','value':'Remembered property'}],'address':'123 Main Street','per_point':[{'point_id':'source-derived-point'}]}
assert m['subject_values'](sample)=={'parcel-member','parcel-array','building-array','Remembered property','123 Main Street','source-derived-point'}, 'subject extraction mixed operational IDs with entities or lost typed subject values'
assert m['count_subject_facts']({'null':None,'empty':'','blank':'   ','zero':0,'false':False,'name':'Actual claim'})==3, 'null/blank slots are not facts; zero/false are explicit assertions'
"#,
    );
}
