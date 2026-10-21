#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  printf 'Usage: %s [CANON_BINARY] [EMPTY_WORK_DIR]\n' "${0##*/}"
  printf 'Replay retained Cornerstone evidence offline; verify honest action-discovery state.\n'
  printf 'Outputs and logs remain in the work directory. No files are deleted.\n'
  exit 0
fi
if [[ $# -gt 2 ]]; then
  printf 'Expected at most CANON_BINARY and EMPTY_WORK_DIR; use --help.\n' >&2
  exit 2
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
canon_binary="${1:-$repo_root/target/debug/canon}"
work_dir="${2:-$(mktemp -d "${TMPDIR:-/tmp}/canon-discovery-state.XXXXXX")}"

python3 -I - "$work_dir" <<'CHECK_WORKDIR'
from pathlib import Path
import sys
root = Path(sys.argv[1])
root.mkdir(parents=True, exist_ok=True)
if any(root.iterdir()):
    raise SystemExit(f"Work directory must be empty: {root}")
CHECK_WORKDIR
source "$repo_root/scripts/geo_replay/e2e_lib.sh"
export CANON_BIN="$canon_binary"
e2e_init "$work_dir" e2e_next_evidence_discovery_state.sh retained

# Retain the exact program beside its inputs, outputs and command receipts.
cat > "$work_dir/trial.py" <<'PY'
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import time

repo, binary, root = (Path(value).resolve() for value in sys.argv[1:])
if not binary.is_file():
    raise SystemExit(f"Missing binary {binary}; run cargo build --bin canon first.")
root.mkdir(parents=True, exist_ok=True)
if any(root.iterdir()):
    raise SystemExit(f"Work directory must be empty: {root}")
source = root / 'source'
source.mkdir()
fixture = repo / 'scripts/geo_measurements/fixtures/cornerstone_national_2026-09-17'
archive = fixture / 'run.tar.gz'
manifest = json.loads((fixture / 'measurement.json').read_bytes())['archives']['run.tar.gz']
actual = hashlib.sha256(archive.read_bytes()).hexdigest()
if actual != manifest['sha256']:
    raise SystemExit(f"Retained archive digest mismatch: expected {manifest['sha256']}, got {actual}")
files = {item['path']: item for item in manifest['files']}
names = ['question', 'capabilities', 'inventory', 'profile', 'budget',
         'home', 'section', 'rows', 'separation', 'next', 'run']
with tarfile.open(archive, 'r:gz') as retained:
    for name in names:
        member = f'national-building-supported/{name}.json'
        stream = retained.extractfile(member)
        if stream is None:
            raise SystemExit(f"Missing retained file: {member}")
        data = stream.read()
        if hashlib.sha256(data).hexdigest() != files[member]['sha256']:
            raise SystemExit(f"Retained input digest mismatch: {member}")
        (source / f'{name}.json').write_bytes(data)
    old_run = json.loads((source / 'run.json').read_bytes())
    old_next_ref = next(item for item in old_run['output_refs']
                        if item['artifact_id'] == 'geo.building.next_evidence/next_evidence')
    old_digest = old_next_ref['content_digest'].removeprefix('blake3:')
    member = f'national-building-supported/run/.canon/geo-run/artifacts/cas/{old_digest}.bin'
    data = retained.extractfile(member).read()
    if hashlib.sha256(data).hexdigest() != files[member]['sha256']:
        raise SystemExit(f'Retained next-evidence digest mismatch: {member}')
    old_next = json.loads(data)

log_path = root / 'run.log'
def log(value):
    line = json.dumps(value, sort_keys=True)
    print(line, flush=True)
    with log_path.open('a') as stream:
        stream.write(line + '\n')

def command(label, args):
    start = time.monotonic()
    result = subprocess.run([str(binary), *map(str, args)], capture_output=True, cwd=root)
    elapsed = time.monotonic() - start
    (root / f'{label}.stdout.json').write_bytes(result.stdout)
    (root / f'{label}.stderr').write_bytes(result.stderr)
    if result.stderr:
        sys.stderr.buffer.write(result.stderr)
        sys.stderr.flush()
    log({'step': label, 'argv': [str(binary), *map(str, args)], 'exit': result.returncode,
         'seconds': elapsed, 'stdout_sha256': hashlib.sha256(result.stdout).hexdigest(),
         'stderr': result.stderr.decode(errors='replace')})
    if result.returncode != 0:
        raise SystemExit(f"{label} refused; see {root / (label + '.stderr')}")
    return json.loads(result.stdout)

log({'proof_class': 'retained', 'feature': 'bd-3mrt', 'archive_sha256': actual,
     'no_claim': 'Not fresh acquisition, held-out accuracy, or automatic action discovery.'})
plan = root / 'plan.stdout.json'
command('plan', ['geo', 'plan', *[item for name in names[:5]
        for item in (f'--{name}', source / f'{name}.json')]])

def run(label, next_path):
    workspace = root / label
    workspace.mkdir()
    bindings = [('home_cells', 'rows', source / 'home.json'),
                ('section', 'request', source / 'section.json'),
                ('materialize_evidence', 'rows', source / 'rows.json'),
                ('separation', 'request', source / 'separation.json'),
                ('next_evidence', 'request', next_path)]
    args = ['geo', 'run', '--plan', plan, '--work-dir', workspace]
    for node, binding, path in bindings:
        args.extend(['--input', f'geo.building.{node}:{binding}={path}'])
    report = command(label, args)
    # Inspection validates each stored output's bytes against its BLAKE3 receipt.
    command(f'{label}-inspect', ['geo', 'inspect', '--run', workspace])
    for output in report['output_refs']:
        log({'step': label, 'artifact_id': output['artifact_id'],
             'blake3': output['content_digest']})
    return workspace, report

def next_bytes(workspace, report):
    output = next(item for item in report['output_refs']
                  if item['artifact_id'] == 'geo.building.next_evidence/next_evidence')
    digest = output['content_digest'].removeprefix('blake3:')
    return (workspace / '.canon/geo-run/artifacts/cas' / f'{digest}.bin').read_bytes()

baseline_workspace, baseline = run('baseline', source / 'next.json')
assert baseline['status'] == 'ABSTAINED', baseline
assert json.loads(next_bytes(baseline_workspace, baseline))['action_discovery'] == 'incomplete'
new_next = json.loads(next_bytes(baseline_workspace, baseline))
assert new_next.pop('action_discovery') == 'incomplete'
assert new_next == old_next, 'Useful retained recommendation changed beyond discovery state'
assert old_next['frontier'][0]['action_id'] == 'acquire-source-parcel-membership'
assert old_next['frontier'][0]['worst_case_remaining'] == 174
assert [outcome['residual_model_count'] for outcome in old_next['frontier'][0]['separation']] == [173, 174]
assert old_next['ranking_abstention']['code'] == 'next_evidence_no_loss_model'
assert baseline['semantic_hash'] != old_run['semantic_hash']
old_outputs = {item['artifact_id']: item['content_digest'] for item in old_run['output_refs']}
new_outputs = {item['artifact_id']: item['content_digest'] for item in baseline['output_refs']}
changed = sorted(key for key in old_outputs if old_outputs[key] != new_outputs[key])
assert changed == ['geo.building.next_evidence/next_evidence'], changed
log({'golden_change': 'bd-3mrt', 'changed_artifacts': changed,
     'old': {key: old_outputs[key] for key in changed},
     'new': {key: new_outputs[key] for key in changed},
     'old_run_semantic_hash': old_run['semantic_hash'],
     'new_run_semantic_hash': baseline['semantic_hash'],
     'note': 'Explicit discovery state added; upstream stage digests unchanged.'})

empty = json.loads((source / 'next.json').read_bytes())
empty['candidates'] = []
empty_path = root / 'next-empty.json'
empty_path.write_text(json.dumps(empty))
workspace, report = run('empty', empty_path)
assert report['status'] == 'ABSTAINED', report
data = next_bytes(workspace, report)
state = json.loads(data)
assert state['action_discovery'] == 'not_attempted', state
assert 'stop' not in state, state
assert state['frontier'] == [], state
replay_workspace, replay = run('empty-fresh', empty_path)
assert data == next_bytes(replay_workspace, replay), 'Fresh-process bytes drifted'

bad = dict(empty)
bad['candidates'] = [{'action_id': 'policy.stop', 'class': 'stop',
                      'kind': {'kind': 'stop'}, 'cost_units': 0}]
bad_path = root / 'next-invalid-stop.json'
bad_path.write_text(json.dumps(bad))
_, rejected = run('invalid-stop', bad_path)
assert rejected['status'] == 'FAILED', rejected
failed = [row for row in rejected['project_run_report']['node_reports']
          if row['node_id'] == 'geo.building.next_evidence']
assert len(failed) == 1 and failed[0]['outcome'] == 'failed', failed
assert 'Geo next-evidence inputs validation contract failed: InvalidInput:' in failed[0]['reason'], failed
assert 'need an explicit reason' in failed[0]['reason'], failed
assert not any(item['artifact_id'] == 'geo.building.next_evidence/next_evidence'
               for item in rejected['output_refs']), rejected
log({'result': 'PASS', 'checks': ['useful_retained_workflow', 'upstream_digest_parity',
     'no_false_exhaustion', 'fresh_process_parity', 'invalid_stop_rejected'],
     'work_dir': str(root)})
PY
e2e_file "$work_dir/trial.py"
e2e_cmd discovery 0 -- python3 -I "$work_dir/trial.py" "$repo_root" "$canon_binary" "$work_dir/trial"
cat "$work_dir/discovery.stdout"
e2e_finish
