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
        if case == "courtney.initial_hints" {
            0
        } else if catalog()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["case_id"] == case && row["mode"] == "retained_resume")
            || case.starts_with("current_answer.")
        {
            4
        } else if case.ends_with("workspace_refusal") || case.starts_with("resume.") {
            2
        } else if case.ends_with("address_evidence")
            || case == "g4.retry_recovery"
            || case == "nxrt.truth_probe"
        {
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
        assert_eq!(
            run["status"], "WAITING_FOR_INPUT",
            "CUR05/RPL15: exit zero is insufficient"
        );
        assert_eq!(run["phase"], "PREFLIGHTED");
        assert!(run["project_run_report"].is_null());
        let typed: canon::geo::GeoRun = serde_json::from_value(run.clone()).unwrap();
        assert!(typed.output_refs.is_empty());
        assert!(run["blockers"].as_array().unwrap().iter().any(|blocker| {
            blocker["kind"] == "INVALID_INPUT"
                && blocker["reason"]
                    .as_str()
                    .unwrap()
                    .contains("field=prospective")
        }));
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
fn fast_current_answer_brooklyn_00_neighborhood() {
    replay("current_answer.brooklyn.00_neighborhood");
}
#[test]
fn fast_current_answer_brooklyn_01_address_parcel() {
    replay("current_answer.brooklyn.01_address_parcel");
}
#[test]
fn fast_current_answer_brooklyn_02_address_building() {
    replay("current_answer.brooklyn.02_address_building");
}
#[test]
fn fast_current_answer_cornerstone_national_building_supported() {
    replay("current_answer.cornerstone.national_building_supported");
}
#[test]
fn fast_current_answer_cornerstone_national_building() {
    replay("current_answer.cornerstone.national_building");
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

macro_rules! full_replay_test {
    ($name:ident, $case:literal) => {
        #[test]
        #[ignore = "bd-1rsv retained full/release gate; logs retained, never live proof"]
        fn $name() {
            replay($case);
        }
    };
}
full_replay_test!(
    geo_replay_full_cornerstone_foursquare_parcel_supported,
    "cornerstone.foursquare_parcel_supported"
);
full_replay_test!(
    geo_replay_full_cornerstone_foursquare_parcel,
    "cornerstone.foursquare_parcel"
);
full_replay_test!(
    geo_replay_full_cornerstone_national_parcel_supported,
    "cornerstone.national_parcel_supported"
);
full_replay_test!(
    geo_replay_full_cornerstone_national_parcel,
    "cornerstone.national_parcel"
);
full_replay_test!(
    geo_replay_full_courtney_final_address,
    "courtney.final_address"
);
full_replay_test!(geo_replay_full_courtney_final_hints, "courtney.final_hints");
full_replay_test!(
    geo_replay_full_courtney_final_summit,
    "courtney.final_summit"
);
full_replay_test!(
    geo_replay_full_courtney_initial_hints,
    "courtney.initial_hints"
);
full_replay_test!(
    geo_replay_full_orlando_case_0_address,
    "orlando.case_0.address"
);
full_replay_test!(geo_replay_full_orlando_case_0_hints, "orlando.case_0.hints");
full_replay_test!(
    geo_replay_full_orlando_case_1_address,
    "orlando.case_1.address"
);
full_replay_test!(geo_replay_full_orlando_case_1_hints, "orlando.case_1.hints");
full_replay_test!(
    geo_replay_full_orlando_case_2_address,
    "orlando.case_2.address"
);
full_replay_test!(geo_replay_full_orlando_case_2_hints, "orlando.case_2.hints");
full_replay_test!(
    geo_replay_full_current_answer_cornerstone_national_parcel_supported,
    "current_answer.cornerstone.national_parcel_supported"
);
full_replay_test!(
    geo_replay_full_current_answer_courtney_final_hints,
    "current_answer.courtney.final_hints"
);
full_replay_test!(geo_replay_full_nxrt_truth_probe, "nxrt.truth_probe");
full_replay_test!(
    geo_replay_resume_cornerstone_foursquare_parcel_supported,
    "resume.cornerstone.foursquare_parcel_supported"
);
full_replay_test!(
    geo_replay_resume_cornerstone_foursquare_parcel,
    "resume.cornerstone.foursquare_parcel"
);
full_replay_test!(
    geo_replay_resume_cornerstone_national_building_supported,
    "resume.cornerstone.national_building_supported"
);
full_replay_test!(
    geo_replay_resume_cornerstone_national_building,
    "resume.cornerstone.national_building"
);
full_replay_test!(
    geo_replay_resume_cornerstone_national_parcel,
    "resume.cornerstone.national_parcel"
);
full_replay_test!(
    geo_replay_resume_courtney_final_address,
    "resume.courtney.final_address"
);
full_replay_test!(
    geo_replay_resume_courtney_final_summit,
    "resume.courtney.final_summit"
);
full_replay_test!(
    geo_replay_resume_orlando_case_0_address,
    "resume.orlando.case_0.address"
);
full_replay_test!(
    geo_replay_resume_orlando_case_0_hints,
    "resume.orlando.case_0.hints"
);
full_replay_test!(
    geo_replay_resume_orlando_case_1_address,
    "resume.orlando.case_1.address"
);
full_replay_test!(
    geo_replay_resume_orlando_case_1_hints,
    "resume.orlando.case_1.hints"
);
full_replay_test!(
    geo_replay_resume_orlando_case_2_address,
    "resume.orlando.case_2.address"
);
full_replay_test!(
    geo_replay_resume_orlando_case_2_hints,
    "resume.orlando.case_2.hints"
);

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
fn rpf_06_every_full_row_has_an_ignored_consumer_and_truth_is_evaluation_only() {
    python_test(
        r#"
import runpy,json,pathlib,re
m=runpy.run_path('scripts/geo_replay/replay.py');catalog=json.loads(pathlib.Path('tests/fixtures/geo/replay/catalog.json').read_text());text=pathlib.Path('tests/geo_replay_retained.rs').read_text()
full=[c for c in catalog['cases'] if c['tier']=='full'];assert len(full)==17
assert len([c for c in full if not c.get('duplicate_of')])==16
assert len([c for c in catalog['cases'] if c['tier']=='fast'])==16
assert len([c for c in catalog['cases'] if c['tier']=='resume'])==16
assert len([c for c in catalog['cases'] if c['mode']=='cost_observation'])==6
for c in full:
 name='geo_replay_full_'+c['case_id'].replace('.','_')
 assert re.search(r'full_replay_test!\(\s*'+re.escape(name)+r',\s*"'+re.escape(c['case_id'])+r'"\s*\)',text),c['case_id']
evaluations=[m['resolve_case'](c,catalog) for c in catalog['cases'] if c.get('mode')=='evaluation_solve']
assert len(evaluations)==1 and evaluations[0]['role']=='evaluation_only'
withheld={c['request'] for c in evaluations}
for raw in catalog['cases']:
 if raw.get('role')=='evaluation_only':continue
 c=m['resolve_case'](raw,catalog)
 assert all(binding['path'] not in withheld for binding in c.get('run_bindings',[])),c['case_id']
"#,
    );
}

#[test]
fn rpf_02_failed_baseline_cannot_be_relabeled_as_identity_success() {
    python_test(
        r#"
import copy,runpy,json,pathlib
m=runpy.run_path('scripts/geo_replay/replay.py');rows=json.loads(pathlib.Path('tests/fixtures/geo/replay/catalog.json').read_text())['cases']
for c in [c for c in rows if c['case_id'] in ['nxrt.experiment','resume.nxrt.experiment']]:
 files=m['load_archive'](c);old=m['retained'](c,files)
 assert m['failed_baseline_check'](c,old,files)['identity_success'] is False
 for patch,run in [(dict(experiment_class='integration'),old),({},dict(old,status='RESOLVED')), (dict(result_class='success'),old)]:
  try:m['failed_baseline_check'](dict(c,**patch),run,files)
  except m['ReplayError'] as e:assert e.code=='failed_baseline_reported_as_success'
  else:raise AssertionError('failed baseline mislabeled as success')
"#,
    );
}

#[test]
fn rpf_03_timing_regression_is_observed_without_failing_semantics() {
    python_test(
        r#"
import runpy
m=runpy.run_path('scripts/geo_replay/replay.py');row=m['timing_observation']('retained-unit',2000,1000)
assert row['ratio_to_retained']==2 and row['elapsed_ms']==2000 and row['timing_affects_semantics'] is False
assert m['timing_observation']('retained-unit',10**12,1000)['timing_affects_semantics'] is False
"#,
    );
}

#[test]
fn rpf_05_numeric_cost_ceiling_records_a_real_terminated_process_without_semantic_success() {
    python_test(
        r#"
import runpy,pathlib,tempfile,sys,json
m=runpy.run_path('scripts/geo_replay/replay.py');out=pathlib.Path(tempfile.mkdtemp(prefix='canon-cost-ceiling-'));log=m['Log'](out,'synthetic-cost-ceiling-unit')
m['bounded_cost_command'](log,pathlib.Path(sys.executable),['-c','import sys,time; print("real bounded cost child",file=sys.stderr,flush=True); time.sleep(60)'],0.1)
rows=[json.loads(line) for line in (out/'run.log').read_text().splitlines()]
command=next(r for r in rows if r['event']=='command');ceiling=next(r for r in rows if r['event']=='cost_observation_ceiling_reached')
assert command['exit']!=0 and 'real bounded cost child' in (out/'cost-run.stderr').read_text()
assert ceiling['semantic_case'] is False and ceiling['semantic_success_claimed'] is False
assert not any(r.get('event')=='invariant' and r.get('passed') for r in rows)
"#,
    );
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
fn current_answer_sources_and_upstream_pins_cannot_be_overridden_by_new_captures() {
    python_test(
        r#"
import copy,runpy,json,pathlib,tempfile
m=runpy.run_path('scripts/geo_replay/replay.py');catalog=json.loads(pathlib.Path('tests/fixtures/geo/replay/catalog.json').read_text())
rows=[c for c in catalog['cases'] if c['mode']=='current_answer'];assert len(rows)==7
assert sum(c['tier']=='fast' for c in rows)==5 and sum(c['tier']=='full' for c in rows)==2
for row in rows:
 c=m['resolve_case'](row,catalog);files=m['load_archive'](c);old=m['retained'](c,files)
 source=next(x for x in catalog['cases'] if x['case_id']==row['source_case_id'])
 assert c['retained']==source['retained'] and c['member_sha256']==source['member_sha256']
 assert c['golden_changes']==[]
 kernel=[ref for ref in old['output_refs'] if ref['output_id'] in m['CURRENT_UPSTREAM']]
 retained_kernel=copy.deepcopy(old);retained_kernel['output_refs']=kernel
 m['validate_current_answer_upstream'](c,retained_kernel)
 blocked=copy.deepcopy(retained_kernel);blocked['blockers']=[dict(kind='PROJECT_FAILURE')]
 try:m['validate_current_answer_capture'](c,b'',blocked)
 except m['ReplayError'] as e:assert e.code=='invariant_failed' and 'current_answer_blockers' in str(e)
 else:raise AssertionError('current answer with a blocker passed')
 tampered=copy.deepcopy(retained_kernel);ref=tampered['output_refs'][0];ref['content_digest']='blake3:'+('0' if ref['content_digest'][-1]!='0' else '1')*64
 try:m['validate_current_answer_upstream'](c,tampered)
 except m['ReplayError'] as e:assert e.code=='digest_drift'
 else:raise AssertionError('new answer capture could conceal upstream drift')
 d=copy.deepcopy(row);d['retained']=source['retained']
 try:m['resolve_case'](d,catalog)
 except m['ReplayError'] as e:assert e.code=='invalid_current_answer_override'
 else:raise AssertionError('derived row overrode original source pins')
"#,
    );
}

#[test]
fn current_answer_missing_capture_refuses_before_canon_instead_of_recording_success() {
    python_test(
        r#"
import copy,runpy,json,pathlib,tempfile
m=runpy.run_path('scripts/geo_replay/replay.py');catalog=json.loads(pathlib.Path('tests/fixtures/geo/replay/catalog.json').read_text())
row=copy.deepcopy(next(c for c in catalog['cases'] if c['mode']=='current_answer'));row.pop('expected_current_answer',None)
out=pathlib.Path(tempfile.mkdtemp(prefix='canon-missing-capture-'))/'case'
try:m['replay'](row,pathlib.Path('/must-not-execute-canon'),out)
except m['ReplayError'] as e:assert e.code=='missing_expected_capture'
else:raise AssertionError('missing reviewed capture passed')
assert not out.exists()
# Pure serialized-shape unit: omitted empty fields are valid; a report,
# output, missing INVALID_INPUT, or success status cannot pass as zero work.
minimal=dict(status='WAITING_FOR_INPUT',phase='PREFLIGHTED',blockers=[dict(kind='INVALID_INPUT')])
assert [check['pointer'] for check in m['preflight_invariants'](minimal)]==['/status','/phase','/blockers']
for patch in [dict(project_run_report={}),dict(output_refs=[{}]),dict(blockers=[]),dict(status='ABSTAINED'),dict(phase='SOLVED')]:
 bad=dict(minimal,**patch)
 try:m['preflight_invariants'](bad)
 except m['ReplayError'] as e:assert e.code=='invalid_preflight_transition'
 else:raise AssertionError('non-preflight shape passed: '+str(patch))
try:m['pointer'](minimal,'/output_refs')
except KeyError:pass
else:raise AssertionError('generic pointer silently defaulted a missing field')
"#,
    );
}

#[test]
fn resume_outcome_changes_chain_from_approved_refusal_and_reject_nonpreflight_work() {
    python_test(
        r#"
import copy,json,pathlib,runpy
m=runpy.run_path('scripts/geo_replay/replay.py');rows=json.loads(pathlib.Path('tests/fixtures/geo/replay/catalog.json').read_text())['cases'];case=next(c for c in rows if c['case_id']=='resume.nxrt.experiment');old=m['retained'](case,m['load_archive'](case))
original=dict(process_exit=0,status=old['status'],semantic_hash=old['semantic_hash'])
first=next(g for g in case['golden_changes'] if g.get('kind')=='invariant_change' and g.get('target')=='resume_current_outcome')
one=dict(case,golden_changes=[first]);refusal=m['expected_resume_outcome'](one,original)
assert refusal==first['new'] and refusal['process_exit']==2 and refusal['node_id'].endswith('.next_evidence')
# Chain-only shape unit, not a runtime capture or a cached-success claim.
wait=dict(process_exit=0,status='WAITING_FOR_INPUT',phase='PREFLIGHTED',blockers=[dict(kind='INVALID_INPUT')],current_cached_success=False)
m['preflight_invariants'](wait)
second=dict(kind='invariant_change',target='resume_current_outcome',old=refusal,new=wait,bead='bd-2dvm',date='2026-10-21',semantic_note='synthetic chain unit only')
two=dict(case,golden_changes=[first,second]);assert m['expected_resume_outcome'](two,original)==wait
bad=copy.deepcopy(two);bad['golden_changes'][1]['old']=original
try:m['expected_resume_outcome'](bad,original)
except m['ReplayError'] as error:assert error.code=='invalid_golden_change'
else:raise AssertionError('transition skipped the prior approved refusal')
bad=copy.deepcopy(two);bad['golden_changes'][0]['old']=refusal
try:m['expected_resume_outcome'](bad,original)
except m['ReplayError'] as error:assert error.code=='invalid_golden_change'
else:raise AssertionError('first link did not bind immutable original')
for patch in [dict(project_run_report={}),dict(output_refs=[{}]),dict(blockers=[]),dict(status='ABSTAINED')]:
 try:m['preflight_invariants'](dict(wait,**patch))
 except m['ReplayError'] as error:assert error.code=='invalid_preflight_transition'
 else:raise AssertionError('nonzero-work or non-invalid outcome accepted')
"#,
    );
}

#[test]
fn workspace_refusal_operational_path_contract_rejects_the_wrong_requested_path() {
    // Pure contract unit only. The public replay separately exercises the real
    // refusal, exit code and absence of created workspace directories.
    python_test(
        r#"
import copy,pathlib,runpy,tempfile
m=runpy.run_path('scripts/geo_replay/replay.py');work=pathlib.Path(tempfile.mkdtemp(prefix='canon-work-refusal-'))/'missing-parent'/'work'
envelope=dict(outcome='REFUSAL',refusal=dict(code='E_ENTITY_ARTIFACT_CONTRACT',detail=dict(geo_run_error_code='INVALID_INPUT',detail=dict(field='work_dir',path=str(work)))))
assert m['workspace_refusal_contract'](envelope,work)['path_binding']=='requested_work_dir'
bad=copy.deepcopy(envelope);bad['refusal']['detail']['detail']['path']=str(work)+'-wrong'
try:m['workspace_refusal_contract'](bad,work)
except m['ReplayError'] as error:assert error.code=='invariant_failed'
else:raise AssertionError('wrong requested workspace path accepted')
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
        16
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
                .is_some_and(|path| path.contains(&format!("fixtures/{name}/")))
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
