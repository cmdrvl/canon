#!/usr/bin/env bash
# Shared local-only command evidence for Geo integration gates.
e2e_event() {
    python3 -I - "$E2E_LOG" "$@" <<'PY'
import datetime,json,sys
row=dict(event=sys.argv[2],ts=datetime.datetime.now(datetime.timezone.utc).isoformat())
for arg in sys.argv[3:]:
    key,value=arg.split('=',1);row[key]=value
with open(sys.argv[1],'a') as f:f.write(json.dumps(row,sort_keys=True)+'\n')
PY
}

e2e_init() {
    set -E
    E2E_OUT="$1"; export E2E_OUT
    case "$3" in fixture|retained|live) ;; *) printf 'invalid proof class: %s\n' "$3" >&2; return 1 ;; esac
    mkdir -p "$E2E_OUT"
    E2E_LOG="$E2E_OUT/run.log"; export E2E_LOG
    export CARGO_NET_OFFLINE=true
    E2E_STARTED=$SECONDS; E2E_PASSED=0; E2E_FAILED=0
    local revision
    revision=$(git rev-parse HEAD) || revision=unknown
    local binary="${CANON_BIN:-target/debug/canon}"
    e2e_event start "script=$2" "proof_class=$3" "revision=$revision" "binary=$binary" "host=$(uname -a)"
    e2e_file "$binary"
    trap 'e2e_on_err "$LINENO" "$BASH_COMMAND"' ERR
}

e2e_on_err() {
    trap - ERR
    e2e_event error "line=$1" "command=$2"
    printf 'Geo E2E failed at line %s: %s\n' "$1" "$2" >&2
    exit 1
}

e2e_cmd() {
    local label="$1" expected="$2"; shift 2
    test "$1" = --; shift
    python3 -I - "$E2E_LOG" "$E2E_OUT" "$label" "$expected" "$@" <<'PY'
import datetime,hashlib,json,pathlib,subprocess,sys,time
log,out,label,expected,*argv=sys.argv[1:];start=time.monotonic()
p=subprocess.run(argv,capture_output=True);sys.stderr.buffer.write(p.stderr)
stdout=pathlib.Path(out)/(label+'.stdout');stdout.write_bytes(p.stdout)
stderr=pathlib.Path(out)/(label+'.stderr');stderr.write_bytes(p.stderr)
row=dict(event='command',ts=datetime.datetime.now(datetime.timezone.utc).isoformat(),argv=argv,cwd=str(pathlib.Path.cwd()),exit=p.returncode,elapsed_ms=round((time.monotonic()-start)*1000),stdout=str(stdout),stdout_sha256=hashlib.sha256(p.stdout).hexdigest(),stderr=str(stderr),stderr_tail=p.stderr.decode(errors='replace')[-4000:])
with open(log,'a') as f:f.write(json.dumps(row,sort_keys=True)+'\n')
if p.returncode!=int(expected):
    print('unexpected command exit: '+json.dumps(row),file=sys.stderr);sys.exit(1)
PY
    E2E_PASSED=$((E2E_PASSED + 1))
}

e2e_geo_run() {
    local label="$1" expected_status="$2"; shift 2
    e2e_cmd "$label" 0 "$@"
    python3 -I - "$E2E_LOG" "$E2E_OUT/$label.stdout" "$expected_status" <<'PY'
import datetime,json,sys,pathlib,subprocess,shutil
log,path,expected=sys.argv[1:];run=json.load(open(path))
row=dict(event='geo_run',ts=datetime.datetime.now(datetime.timezone.utc).isoformat(),status=run.get('status'),phase=run.get('phase'),semantic_hash=run.get('semantic_hash'),output_refs=run.get('output_refs'))
with open(log,'a') as f:f.write(json.dumps(row,sort_keys=True)+'\n')
if row['status']!=expected:
    print('unexpected Geo status: '+json.dumps(row),file=sys.stderr);sys.exit(1)
commands=[json.loads(line) for line in pathlib.Path(log).read_text().splitlines() if json.loads(line)['event']=='command']
argv=commands[-1]['argv'];work=pathlib.Path(argv[argv.index('--work-dir')+1])
for ref in run.get('output_refs',[]):
    file=work/'.canon/geo-run/artifacts/cas'/(ref['content_digest'].split(':')[1]+'.bin')
    if shutil.which('b3sum'):
        p=subprocess.run(['b3sum',str(file)],capture_output=True);sys.stderr.buffer.write(p.stderr)
        if p.returncode or 'blake3:'+p.stdout.decode().split()[0]!=ref['content_digest']:
            print('CAS digest mismatch: '+str(file),file=sys.stderr);sys.exit(1)
    with open(log,'a') as f:f.write(json.dumps(dict(event='artifact',ts=datetime.datetime.now(datetime.timezone.utc).isoformat(),**ref))+'\n')
PY
    if ! command -v b3sum >/dev/null; then e2e_event b3sum_unavailable; fi
}

e2e_file() {
    python3 -I - "$E2E_LOG" "$1" <<'PY'
import datetime,hashlib,json,pathlib,sys
p=pathlib.Path(sys.argv[2]);b=p.read_bytes()
with open(sys.argv[1],'a') as f:f.write(json.dumps(dict(event='file',ts=datetime.datetime.now(datetime.timezone.utc).isoformat(),path=str(p),bytes=len(b),sha256=hashlib.sha256(b).hexdigest()))+'\n')
PY
}

e2e_check() {
    local name="$1" filter="$2" file="$3" value
    if value=$(jq -ce "$filter" "$file"); then
        e2e_event check "name=$name" "passed=true" "value=$value"
        E2E_PASSED=$((E2E_PASSED + 1))
    else
        e2e_event check "name=$name" "passed=false" "value=$value"
        E2E_FAILED=$((E2E_FAILED + 1))
        return 1
    fi
}

e2e_finish() {
    e2e_event finish "passed=$E2E_PASSED" "failed=$E2E_FAILED" "elapsed_seconds=$((SECONDS - E2E_STARTED))"
    printf 'Geo E2E: %s checks passed, %s failed, %ss elapsed\n' "$E2E_PASSED" "$E2E_FAILED" "$((SECONDS - E2E_STARTED))" >&2
    test "$E2E_FAILED" = 0
}
