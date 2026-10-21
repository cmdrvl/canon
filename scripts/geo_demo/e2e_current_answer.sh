#!/usr/bin/env bash
# Retained integration only: no acquisition, acceptance, held-out or E5 proof.
set -euo pipefail
if [[ "${1:-}" == --help || "${1:-}" == -h ]]; then
  printf 'Usage: %s [CANON_BINARY] [EMPTY_OUTPUT_DIR]\n' "${0##*/}"
  printf 'Requires a prebuilt Canon, Python 3, and b3sum. Builds nothing.\n'
  printf 'Exercises five retained current-answer cases, scope reuse and invalid-input preflight.\n'
  exit 0
fi
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="${CANON_REPO_ROOT:-$(cd -- "$script_dir/../.." && pwd -P)}"
export CANON_BIN="${1:-$repo_root/target/debug/canon}"
out="${2:-$(mktemp -d "${TMPDIR:-/tmp}/canon-current-answer-e2e.XXXXXX")}"
python3 -I - "$out" <<'PY'
import pathlib,sys
p=pathlib.Path(sys.argv[1]);p.mkdir(parents=True,exist_ok=True)
if any(p.iterdir()):raise SystemExit('Output directory must be empty: '+str(p))
PY
command -v b3sum >/dev/null
source "$repo_root/scripts/geo_replay/e2e_lib.sh"
e2e_init "$out" e2e_current_answer.sh retained
# e2e_cmd consumes stdin itself. Retain this driver as a real file before
# invoking it; never pass a Python stdin program through that wrapper.
cat > "$out/driver.py" <<'PY'
import datetime,hashlib,importlib.util,json,pathlib,subprocess,sys,time
sys.dont_write_bytecode=True
repo,out,binary=pathlib.Path(sys.argv[1]),pathlib.Path(sys.argv[2]),sys.argv[3]
spec=importlib.util.spec_from_file_location('retained_replay',repo/'scripts/geo_replay/replay.py')
replay=importlib.util.module_from_spec(spec);spec.loader.exec_module(replay)
catalog=json.loads(replay.CATALOG.read_bytes())
case_ids=['brooklyn.00_neighborhood','brooklyn.01_address_parcel','brooklyn.02_address_building','cornerstone.national_building_supported','cornerstone.national_building']
upstream={'home_cells','section','materialize_evidence','compile_evidence','propagation','solve','explanation'}
commands=[];checks=0;results=[]
def event(kind,**fields):
    row=dict(event=kind,ts=datetime.datetime.now(datetime.timezone.utc).isoformat(),proof_class='retained')
    row.update(fields)
    with (out/'run.log').open('a') as f:f.write(json.dumps(row,sort_keys=True)+'\n')
def require(ok,label,**fields):
    global checks
    event('check',name=label,passed=bool(ok),**fields)
    if not ok:raise RuntimeError(label+': '+json.dumps(fields,sort_keys=True))
    checks+=1
def write(path,data):
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_bytes(data if isinstance(data,bytes) else (json.dumps(data,separators=(',',':'))+'\n').encode())
    event('file',path=str(path),bytes=path.stat().st_size,sha256=hashlib.sha256(path.read_bytes()).hexdigest())
    return path
def command(label,args):
    argv=[binary,*map(str,args)];start=time.monotonic()
    p=subprocess.run(argv,cwd=repo,capture_output=True)
    sys.stderr.buffer.write(p.stderr);sys.stderr.buffer.flush()
    stdout=out/(label+'.stdout');stderr=out/(label+'.stderr')
    stdout.write_bytes(p.stdout);stderr.write_bytes(p.stderr)
    row=dict(label=label,argv=argv,cwd=str(repo),exit=p.returncode,elapsed_ms=round((time.monotonic()-start)*1000),stdout=str(stdout),stdout_sha256=hashlib.sha256(p.stdout).hexdigest(),stderr=str(stderr))
    event('command',**row);commands.append(row)
    require(p.returncode==0,label+' emits artifact',exit=p.returncode)
    return json.loads(p.stdout),p.stdout
def bindings_args(bindings):
    return [item for b in bindings for item in ['--input',b['node_id']+':'+b['binding_id']+'='+str(b['local'])]]
def plan(case_dir,inputs,label):
    args=['geo','plan']
    for flag,path in inputs.items():args+=['--'+flag,path]
    value,data=command(label,args)
    return value,write(case_dir/(label+'.plan.json'),data)
def run(case_dir,plan_file,bindings,label,workspace=None):
    workspace=workspace or case_dir/'work';workspace.mkdir(exist_ok=True)
    value,_=command(label,['geo','run','--plan',plan_file,'--work-dir',workspace,*bindings_args(bindings)])
    event('geo_run',label=label,status=value['status'],phase=value['phase'],semantic_hash=value['semantic_hash'],output_refs=value.get('output_refs',[]),blockers=value.get('blockers',[]),resumed_nodes=(value.get('project_run_report') or {}).get('resumed_nodes',[]),executed_nodes=(value.get('project_run_report') or {}).get('executed_nodes',[]))
    return value,workspace
def artifact(run,workspace,output_id):
    refs=[r for r in run['output_refs'] if r['output_id']==output_id]
    require(len(refs)==1,'unique '+output_id+' output')
    ref=refs[0];path=workspace/'.canon/geo-run/artifacts/cas'/(ref['content_digest'].split(':')[1]+'.bin')
    data=path.read_bytes();p=subprocess.run(['b3sum',str(path)],capture_output=True)
    sys.stderr.buffer.write(p.stderr);sys.stderr.buffer.flush()
    require(p.returncode==0 and 'blake3:'+p.stdout.decode().split()[0]==ref['content_digest'] and len(data)==ref['byte_count'],'CAS bytes '+output_id)
    event('artifact',path=str(path),sha256=hashlib.sha256(data).hexdigest(),**ref)
    return json.loads(data)
def compare_upstream(case,run,workspace):
    refs=[r for r in run['output_refs'] if r['output_id'] in upstream]
    require(len(refs)==7,'all seven upstream outputs',case_id=case['case_id'])
    for ref in refs:
        expected=case['retained']['output_digests'][ref['artifact_id']]
        require(ref['content_digest']==expected,'immutable upstream digest',case_id=case['case_id'],artifact_id=ref['artifact_id'],expected=expected,actual=ref['content_digest'])
        artifact(run,workspace,ref['output_id'])
def burden(paths,bindings):
    files=set(map(pathlib.Path,paths))|{b['local'] for b in bindings}
    return dict(caller_files=len(files),caller_bytes=sum(p.stat().st_size for p in files),hand_written_bindings=len(bindings),hand_written_node_ids=len({b['node_id'] for b in bindings}),files=[dict(path=str(p),bytes=p.stat().st_size,sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in sorted(files)])
for case_id in case_ids:
    case=next(row for row in catalog['cases'] if row['case_id']==case_id)
    files=replay.load_archive(case);replay.retained(case,files)
    directory=out/case_id;directory.mkdir();inputs={}
    first_command=len(commands)
    caps,caps_bytes=command(case_id+'.capabilities',['geo','capabilities','--emit','json'])
    for flag,member in case['plan_inputs'].items():
        contents=replay.get(files,member)
        if flag=='question':
            question=json.loads(contents);question['answer_scope']='current_answer';contents=question
        elif flag=='capabilities':contents=caps_bytes
        inputs[flag]=write(directory/'inputs'/(flag+'.json'),contents)
    bound=[]
    for index,binding in enumerate(case['run_bindings']):
        if binding['node_id'].rsplit('.',1)[-1] not in {'home_cells','section','materialize_evidence'}:continue
        local=write(directory/'inputs'/('binding-'+str(index)+'.json'),replay.get(files,binding['path']))
        bound.append(dict(binding,local=local))
    require(len(bound)==3,'three caller bindings',case_id=case_id)
    p,pfile=plan(directory,inputs,case_id+'.current.plan')
    run_value,workspace=run(directory,pfile,bound,case_id+'.current.run')
    require(run_value['status']=='ABSTAINED' and run_value['phase']=='SOLVED' and run_value.get('blockers',[])==[],'completed current answer',case_id=case_id,status=run_value['status'],phase=run_value['phase'])
    report=run_value['project_run_report']
    require(report.get('failed_nodes',[])==[] and report.get('blocked_nodes',[])==[] and len(report['executed_nodes'])==8,'eight stages complete',case_id=case_id)
    compare_upstream(case,run_value,workspace)
    answer=artifact(run_value,workspace,'answer');solve=artifact(run_value,workspace,'solve')
    require(answer['further_work']['source']=='not_in_scope' and answer['further_work']['action_discovery']=='not_attempted','optional work remains explicit',case_id=case_id)
    require(answer['acceptance']['status']=='no_policy_declared' and not answer['extent_and_reach']['complete_extent_claimed'],'no acceptance or complete extent',case_id=case_id)
    if case_id.startswith('brooklyn.'):
        expected_parcels=[] if case_id.endswith('00_neighborhood') else ['3051090025']
        expected_buildings=['3117371'] if case_id.endswith('02_address_building') else []
        require(answer['logical_consequence']['hard_forced']==dict(parcels=expected_parcels,buildings=expected_buildings),'Brooklyn forced members',case_id=case_id)
        require(solve['summary']['parcel_candidates']==92 and solve['summary']['building_candidates']==98,'Brooklyn universe preserved',case_id=case_id)
        require(answer['logical_consequence']['residual_model_count'] is None and answer['logical_consequence']['residual_model_count_saturated'],'saturation is not a literal count',case_id=case_id)
        require(answer['preference']['state']['kind']=='no_soft_evidence','Brooklyn no soft winner',case_id=case_id)
        if case_id.endswith('02_address_building'):
            component=next(c for c in solve['factorization'] if c['key']=='parcel:3051090025')
            require(component['hard_feasible_assignments']==1,'target component is forced')
    elif case_id.endswith('_supported'):
        expected={'0a5045cd-8321-429b-b0b7-0f3f24b6e615','a6145c70-10cb-46d0-95b8-b85ff82ac700'}
        for view in ['composition_view','grouped_view']:
            v=answer['preference'][view]
            require(v['tie_count']==2 and len(v['preferred'])==2 and {tuple(m['members']['buildings']) for m in v['preferred']}=={(identifier,) for identifier in expected} and all(not m['members']['parcels'] and m['cost']==1 for m in v['preferred']) and v['next_best_cost']==2,'Cornerstone retained tie',view=view)
        require(answer['logical_consequence']['residual_model_count']==347,'Cornerstone residual count preserved')
    else:
        require(answer['preference']['state']['kind']=='no_soft_evidence' and all(not answer['preference'][view]['preferred'] for view in ['composition_view','grouped_view']),'diagnostic source supplies no winner')
    command(case_id+'.current.inspect',['geo','inspect','--run',workspace])
    original_paths=set(case['plan_inputs'].values())|{b['path'] for b in case['run_bindings']}
    before=dict(proof_class='retained_preparation_records_not_reexecuted',caller_files=len(original_paths),caller_bytes=sum(len(replay.get(files,path)) for path in original_paths),hand_written_bindings=len(case['run_bindings']),hand_written_node_ids=len({b['node_id'] for b in case['run_bindings']}),retained_argv_source=case['retained_argv_source'],retained_argv_sha256=hashlib.sha256(replay.get(files,case['retained_argv_source'])).hexdigest(),retained_argv=json.loads(replay.get(files,case['retained_argv_source'])),canon_invocations=case['before']['canon_invocations'],acquisition_reexecuted=False)
    after=burden(inputs.values(),bound)
    require(before['caller_files']==10 and before['hand_written_bindings']==5 and after['caller_files']==8 and after['hand_written_bindings']==3,'caller burden drops two prospective files',case_id=case_id)
    after['canon_invocations']=len(commands)-first_command
    require(after['canon_invocations']==4,'four actually executed primary commands',case_id=case_id)
    event('USAGE',case_id=case_id,before=before,after=after,argv=commands[first_command:],historical_glue_not_reexecuted=case['before']['glue_steps'])
    results.append(dict(case_id=case_id,status=run_value['status'],phase=run_value['phase'],before=before,after=after))
    if case_id=='cornerstone.national_building_supported':
        full_question=dict(question,answer_scope='full_workflow')
        full_inputs=dict(inputs,question=write(directory/'full-question.json',full_question))
        full_bound=list(bound)
        for index,binding in enumerate(case['run_bindings']):
            if binding['node_id'].rsplit('.',1)[-1] in {'separation','next_evidence'}:
                full_bound.append(dict(binding,local=write(directory/'full-inputs'/('binding-'+str(index)+'.json'),replay.get(files,binding['path']))))
        _,full_plan=plan(directory,full_inputs,case_id+'.full.plan')
        full,full_work=run(directory,full_plan,full_bound,case_id+'.full.run',workspace)
        require(full['status']=='ABSTAINED' and full['phase']=='SOLVED' and full.get('blockers',[])==[],'full workflow still completes')
        expected_resumed={r['project_node_id'] for r in run_value['output_refs'] if r['output_id'] in upstream}
        expected_executed={'geo.building.separation','geo.building.next_evidence','geo.building.answer'}
        report=full['project_run_report']
        require(set(report.get('resumed_nodes',[]))==expected_resumed and set(report['executed_nodes'])==expected_executed and report.get('failed_nodes',[])==[],'scope change reuses seven upstream stages',resumed=report.get('resumed_nodes',[]),executed=report['executed_nodes'])
        compare_upstream(case,full,full_work)
        full_answer=artifact(full,full_work,'answer');next_evidence=artifact(full,full_work,'next_evidence')
        require(full_answer['further_work']['source']=='next_evidence_artifact' and full_answer['further_work']['action_discovery']==next_evidence['action_discovery'] and full_answer['further_work']['frontier_action_ids']==[a['action_id'] for a in next_evidence['frontier']],'full answer reads actual next-evidence frontier')
        event('USAGE',case_id=case_id,mode='full_workflow_scope_change',burden=burden(full_inputs.values(),full_bound),resumed=report['resumed_nodes'],executed=report['executed_nodes'])
    if case_id=='brooklyn.02_address_building':
        legacy_inputs=dict(inputs,question=write(directory/'legacy-question.json',replay.get(files,case['plan_inputs']['question'])))
        legacy_bound=list(bound)
        for index,binding in enumerate(case['run_bindings']):
            if binding['node_id'].rsplit('.',1)[-1] in {'separation','next_evidence'}:
                legacy_bound.append(dict(binding,local=write(directory/'legacy-inputs'/('binding-'+str(index)+'.json'),replay.get(files,binding['path']))))
        _,legacy_plan=plan(directory,legacy_inputs,case_id+'.legacy.plan')
        invalid,_=run(directory,legacy_plan,legacy_bound,case_id+'.legacy.run',directory/'legacy-work')
        require(invalid['status']=='WAITING_FOR_INPUT' and invalid['phase']=='PREFLIGHTED' and invalid.get('output_refs',[])==[] and invalid.get('project_run_report') is None,'legacy empty prospective stops before any work')
        require(any(b['kind']=='INVALID_INPUT' and b['project_node_id']=='geo.parcel.separation' and 'field=prospective' in b['reason'] and 'at least one prospective observation' in b['reason'] for b in invalid['blockers']),'legacy bad prospective is typed invalid input')
summary=dict(proof_class='retained',cases=results,checks_passed=checks,canon_invocations=len(commands),full_tier_pending=['cornerstone.national_parcel_supported','courtney.final_hints'],pending_owner='bd-1rsv',no_claim=['fresh acquisition','held-out accuracy','policy acceptance','E5','full-tier recompute'])
write(out/'summary.json',summary)
event('current_answer_summary',**summary)
print(json.dumps(summary,sort_keys=True))
PY
e2e_file "$out/driver.py"
e2e_cmd current-answer-driver 0 -- python3 -I "$out/driver.py" "$repo_root" "$out" "$CANON_BIN"
test -s "$out/summary.json"
e2e_file "$out/summary.json"
e2e_check five-retained-cases '.proof_class == "retained" and (.cases | length) == 5 and .checks_passed > 0 and .full_tier_pending == ["cornerstone.national_parcel_supported", "courtney.final_hints"]' "$out/summary.json"
e2e_finish
