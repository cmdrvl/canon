#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "--help" ]]; then
  printf 'Usage: %s [CANON_BINARY] [EMPTY_OUTPUT_DIR]\n' "${0##*/}"
  printf 'Measure retained local cold/warm/refresh execution; builds nothing and retains logs.\n'
  exit 0
fi
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
canon_binary="${1:-$repo_root/target/debug/canon}"
build_profile="${CANON_BUILD_PROFILE:?Set CANON_BUILD_PROFILE=debug or release to identify the measured build}"
case "$build_profile" in debug|release) ;; *) printf 'CANON_BUILD_PROFILE must be debug or release\n' >&2; exit 2 ;; esac
out="${2:-$(mktemp -d "${TMPDIR:-/tmp}/canon-cost-workloads.XXXXXX")}"
python3 -I - "$out" <<'CHECK_OUTPUT'
from pathlib import Path
import sys
root = Path(sys.argv[1])
root.mkdir(parents=True, exist_ok=True)
if any(root.iterdir()):
    raise SystemExit(f"Output directory must be empty: {root}")
CHECK_OUTPUT
source "$repo_root/scripts/geo_replay/e2e_lib.sh"
export CANON_BIN="$canon_binary"
e2e_init "$out" cost_workloads.sh retained
cat > "$out/workloads.py" <<'PY'
import json
from pathlib import Path
import runpy
import sys
import time

repo, binary, out = (Path(value).resolve() for value in sys.argv[1:4])
build_profile = sys.argv[4]
out.mkdir(parents=True, exist_ok=False)
m = runpy.run_path(str(repo / 'scripts/geo_replay/replay.py'))
catalog = json.loads((repo / 'tests/fixtures/geo/replay/catalog.json').read_bytes())
for case_id in ('brooklyn.02_address_building', 'cornerstone.national_building_supported'):
    case = next(row for row in catalog['cases'] if row['case_id'] == case_id)
    files = m['load_archive'](case)
    m['retained'](case, files)
    root = out / case_id
    log = m['Log'](root, case_id)
    log.case_tier = 'fast'
    log.event('measurement_scope', proof='retained inputs, single laptop',
              binary=str(binary), profile=build_profile, profile_provenance='explicit operator declaration',
              unsupported_cost_terms=['licensing', 'transfer', 'review', 'monetary_charges'],
              no_claim='No accuracy, population economics, extrapolation or historical cache-upgrade proof')
    inputs = root / 'inputs'
    for name in case['member_sha256']:
        path = inputs / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(m['get'](files, name))
    args = ['geo', 'plan']
    for flag, path in case['plan_inputs'].items():
        args += ['--' + flag, str(inputs / path)]
    data = log.command('plan', binary, args, repo)
    m['parity'](case, 'plan_sha256', case['expected_plan_sha256'], m['sha'](data))
    plan = json.loads(data)
    plan_path = root / 'plan.json'
    plan_path.write_bytes(data)
    work = root / 'work'
    work.mkdir()
    bindings = { (row['node_id'], row['binding_id']): inputs / row['path'] for row in case['run_bindings'] }

    def run(label, expected):
        args = ['geo', 'run', '--plan', str(plan_path), '--work-dir', str(work)]
        for (node, binding), path in sorted(bindings.items()):
            args += ['--input', node + ':' + binding + '=' + str(path)]
        started = time.monotonic()
        report = json.loads(log.command(label, binary, args, repo))
        wall = (time.monotonic() - started) * 1000
        cost = report['observation']['resource_observations']
        project = report['project_run_report']
        assert cost['cost.workload.' + expected] == 1, cost
        assert sum(cost['cost.workload.' + k] for k in ('cold', 'warm', 'refresh')) == 1
        for key in ('executed', 'resumed', 'invalidated', 'failed'):
            assert cost['cost.nodes.' + key] == len(set(project[key + '_nodes']))
        attempted = set(project['executed_nodes']) | set(project['failed_nodes'])
        timings = [r for r in project['receipt']['node_receipts'] if r['node_id'] in attempted]
        for failed in project['node_reports']:
            if failed['outcome'] != 'failed':
                continue
            token = failed['receipt_hash'].replace(':', '_')
            receipt = json.loads((work / '.canon/geo-run/receipts/cas' / (token + '.json')).read_bytes())
            assert receipt['receipt_hash'] == failed['receipt_hash']
            assert receipt['node_id'] == failed['node_id'] and receipt['outcome'] == 'failed'
            timings.append(receipt)
        assert cost['cost.nodes.timing_unknown'] == 0
        assert cost['cost.millis.executed_total'] == sum(r['duration_millis'] for r in timings)
        assert sum(v for k, v in cost.items() if k.startswith('cost.millis.stage.')) == cost['cost.millis.executed_total']
        assert cost['cost.millis.executed_total'] <= wall
        for receipt in timings:
            stage = next(row['stage'] for row in plan['geo_nodes'] if row['project_node_id'] == receipt['node_id'])
            assert 'cost.millis.stage.' + stage in cost
            log.event('node_timing', node_id=receipt['node_id'], outcome=receipt['outcome'],
                      duration_millis=receipt['duration_millis'], stage=stage,
                      semantic_hash=receipt['semantic_hash'], telemetry_hash=receipt['telemetry_hash'],
                      receipt_hash=receipt['receipt_hash'], producer_binary='producer_unrecorded')
        log.command(label + '-inspect', binary, ['geo', 'inspect', '--run', str(work)], repo)
        log.event('workload', workload=expected, cost=cost, status=report['status'],
                  semantic_hash=report['semantic_hash'], wall_millis=wall)
        return report

    cold = run('cold', 'cold')
    assert cold['observation']['resource_observations']['cost.millis.executed_total'] > 0
    m['parity'](case, 'run_semantic_hash', case['retained']['run_semantic_hash'], cold['semantic_hash'])
    warm = run('warm', 'warm')
    assert warm['semantic_hash'] == cold['semantic_hash']
    assert not warm['project_run_report']['executed_nodes']
    if not warm['project_run_report']['failed_nodes']:
        assert warm['observation']['resource_observations']['cost.millis.executed_total'] == 0
    # Brooklyn's legacy empty separation fails again; its failed work remains visible.
    target = next(key for key in bindings if key[0].endswith('.materialize_evidence'))
    rows = json.loads(bindings[target].read_bytes())
    rows['max_assignments'] += 1
    changed = root / 'rows-refresh.json'
    changed.write_text(json.dumps(rows))
    bindings[target] = changed
    refreshed = run('refresh', 'refresh')
    assert refreshed['project_run_report']['invalidated_nodes']
    m['load_archive'](case)

# Named CST-12 subjects stay named. Current-contract warm runs above cannot
# substitute for their historical cross-binary receipt compatibility evidence.
historical = []
for case_id in ('resume.cornerstone.national_parcel_supported', 'resume.courtney.final_hints'):
    case = next(row for row in catalog['cases'] if row['case_id'] == case_id)
    files = m['load_archive'](case)
    retained_run = m['retained'](case, files)
    execution = json.loads(m['get'](files, case['retained_argv_source']))
    producers = [{'algorithm': algorithm, 'hex': execution[key]}
                 for key, algorithm in (('binary_sha256', 'sha256'), ('binary_blake3', 'blake3'))
                 if execution.get(key)]
    producing_binary = producers or 'producer_unrecorded'
    destination = out / case_id
    m['replay'](case, binary, destination)
    events = [json.loads(line) for line in (destination / 'run.log').read_text().splitlines()]
    refusal = next((row for row in events if row['event'] == 'current_resume_refusal'), None)
    summary = dict(case_id=case_id, proof_class='retained',
                   producer_record=case['retained_argv_source'], producing_binary=producing_binary,
                   receipt_producer='producer_unrecorded',
                   retained_status=retained_run['status'], retained_semantic_hash=retained_run['semantic_hash'])
    if refusal:
        assert refusal['outcome'] == 'REFUSAL' and refusal['process_exit'] == 2
        assert refusal['code'] == 'E_ENTITY_ARTIFACT_CONTRACT'
        assert refusal['geo_run_error_code'] == 'PROJECT_RUN_FAILED'
        assert refusal['node_id'] == 'geo.parcel.next_evidence'
        assert 'bytes are not canonical executor output' in refusal['message']
        assert refusal['current_answer_produced'] is False
        summary.update(status='CST-12_INCOMPLETE', current_outcome='REFUSAL',
                       refusal=refusal, measured_workload=None, cost_terms='unknown',
                       no_claim='Historical warm/refresh classification is unproved; no current run answer or timing envelope was emitted. Do not bypass canonical validation or substitute a current-tree workspace.')
    else:
        report = json.loads((destination / 'run.json').read_bytes())
        cost = report['observation']['resource_observations']
        project = report['project_run_report']
        expected = 'refresh' if project['invalidated_nodes'] else 'warm'
        assert cost['cost.workload.' + expected] == 1
        assert cost['cost.nodes.executed'] == len(set(project['executed_nodes']))
        assert cost['cost.nodes.resumed'] == len(set(project['resumed_nodes']))
        if expected == 'warm':
            assert not project['executed_nodes'] and not project['failed_nodes']
            assert cost['cost.millis.executed_total'] == 0
        summary.update(status='CST-12_MEASURED', current_outcome=report['status'],
                       measured_workload=expected, cost=cost)
    with (destination / 'run.log').open('a') as stream:
        stream.write(json.dumps(dict(event='historical_timing_obligation', **summary), sort_keys=True) + '\n')
    historical.append(summary)
    print(json.dumps(summary, sort_keys=True))

incomplete = [row['case_id'] for row in historical if row['status'] == 'CST-12_INCOMPLETE']
print(json.dumps({'result': 'PARTIAL' if incomplete else 'PASS', 'proof_class': 'retained',
                  'measured_current_contract_workloads': ['cold', 'warm', 'refresh'],
                  'historical_resume_obligation': 'incomplete' if incomplete else 'measured',
                  'remaining_cases': incomplete, 'owner_of_legacy_cache_migration': 'bd-2no5'}))
PY
e2e_file "$out/workloads.py"
e2e_cmd workloads 0 -- python3 -I "$out/workloads.py" "$repo_root" "$canon_binary" "$out/trial" "$build_profile"
cat "$out/workloads.stdout"
e2e_check measured-workload-summary 'select(has("measured_current_contract_workloads")) | .measured_current_contract_workloads == ["cold", "warm", "refresh"] and (.result == "PASS" or .result == "PARTIAL")' "$out/workloads.stdout"
e2e_check historical-resume-complete 'select(has("historical_resume_obligation")) | .result == "PASS" and .historical_resume_obligation == "measured"' "$out/workloads.stdout"
e2e_finish
