#!/usr/bin/env bash
# Release gate for retained integration. No live/held-out/E5 claim.
set -euo pipefail
if [[ "${1:-}" == --help || "${1:-}" == -h ]]; then
  printf 'Usage: %s [--tier fast|full|all] [--canon BIN] [--out DIR] [--cost-ceiling-seconds N]\n' "${0##*/}"
  printf 'Builds release Canon by default; --canon uses an existing binary.\n'
  printf 'Full includes retained resume integrity and six bounded numeric-mask cost observations.\n'
  printf 'Requires Python 3 and b3sum. Timings are observations, never semantic thresholds.\n'
  exit 0
fi
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
tier=full; canon_binary=""; output_dir=""; ceiling=1200
while (($#)); do
  case "$1" in
    --tier) tier="${2:?--tier requires a value}"; shift 2 ;;
    --canon) canon_binary="${2:?--canon requires a path}"; shift 2 ;;
    --out) output_dir="${2:?--out requires a path}"; shift 2 ;;
    --cost-ceiling-seconds) ceiling="${2:?--cost-ceiling-seconds requires a number}"; shift 2 ;;
    *) printf 'Unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
done
case "$tier" in fast|full|all) ;; *) printf 'Invalid tier: %s\n' "$tier" >&2; exit 2 ;; esac
command -v b3sum >/dev/null
output_dir="${output_dir:-$(mktemp -d "${TMPDIR:-/tmp}/canon-full-retained.XXXXXX")}"
# Resolve caller-supplied paths before changing to the repository for the build.
output_dir="$(python3 -I -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$output_dir")"
if [[ -n "$canon_binary" ]]; then
  canon_binary="$(python3 -I -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$canon_binary")"
fi
cd "$repo_root"
python3 -I - "$output_dir" "$ceiling" <<'PY'
import math,pathlib,sys
ceiling=float(sys.argv[2])
if not math.isfinite(ceiling) or ceiling<=0:raise SystemExit('Cost ceiling must be finite and positive')
p=pathlib.Path(sys.argv[1]);p.mkdir(parents=True,exist_ok=True)
if any(p.iterdir()):raise SystemExit('Output directory must be empty: '+str(p))
PY
build_profile=provided_binary
if [[ -z "$canon_binary" ]]; then
  build_profile=release_opt2_ltofalse_codegen16
  python3 -I - "$output_dir" "$repo_root" <<'PY'
import datetime,hashlib,json,os,pathlib,subprocess,sys,time
out,repo=map(pathlib.Path,sys.argv[1:]);env=dict(os.environ)
env.update(CARGO_PROFILE_RELEASE_OPT_LEVEL='2',CARGO_PROFILE_RELEASE_LTO='false',CARGO_PROFILE_RELEASE_CODEGEN_UNITS='16')
argv=['cargo','build','--release','--bin','canon'];start=time.monotonic()
p=subprocess.run(argv,cwd=repo,env=env,capture_output=True)
sys.stderr.buffer.write(p.stderr)
(out/'build.stdout').write_bytes(p.stdout);(out/'build.stderr').write_bytes(p.stderr)
row=dict(event='build',ts=datetime.datetime.now(datetime.timezone.utc).isoformat(),argv=argv,exit=p.returncode,elapsed_ms=round((time.monotonic()-start)*1000),cwd=str(repo),build_profile='release_opt2_ltofalse_codegen16',stdout_sha256=hashlib.sha256(p.stdout).hexdigest())
with (out/'run.log').open('a') as f:f.write(json.dumps(row,sort_keys=True)+'\n')
raise SystemExit(p.returncode)
PY
  canon_binary="$repo_root/target/release/canon"
fi
export CANON_BIN="$canon_binary"
source "$script_dir/e2e_lib.sh"
e2e_init "$output_dir" replay_retained.sh retained
e2e_event build_profile "name=$build_profile"
e2e_cmd retained-suite 0 -- python3 -I "$script_dir/replay.py" --suite --tier "$tier" --binary "$CANON_BIN" --out "$output_dir/cases" --cost-ceiling-seconds "$ceiling" --build-profile "$build_profile"
cat "$E2E_OUT/retained-suite.stdout"
cat "$output_dir/cases/run.log" >> "$E2E_LOG"
e2e_file "$output_dir/cases/timings.json"
e2e_check full-tier-complete '.semantic_cases_executed == .semantic_cases_expected and .duplicate_validations_executed == .duplicate_validations_expected and .resume_cases_executed == .resume_cases_expected and .cost_observations_executed == .cost_observations_expected and .invariant_failures == 0 and .proof_class == "retained"' "$output_dir/cases/timings.json"
e2e_finish
printf 'run.log BLAKE3: '
b3sum "$E2E_LOG"
