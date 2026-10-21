#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == --help || "${1:-}" == -h ]]; then
  printf 'Usage: %s [CANON_BINARY] [GEO_ANSWER_TEST_BINARY] [EMPTY_OUTPUT_DIR]\n' "${0##*/}"
  printf 'Offline retained answer projection and registered public geo run. Builds nothing.\n'
  printf 'Prepare binaries with cargo test --test geo_answer --no-run, then pass the printed test executable.\n'
  exit 0
fi
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
export CANON_BIN="${1:-$repo_root/target/debug/canon}"
test_binary="${2:-}"
if [[ -z "$test_binary" || ! -x "$test_binary" ]]; then
  printf 'A prebuilt geo_answer integration-test executable is required; use --help.\n' >&2
  exit 2
fi
out="${3:-$(mktemp -d "${TMPDIR:-/tmp}/canon-answer-e2e.XXXXXX")}"
python3 -I - "$out" <<'PY'
import pathlib,sys
p=pathlib.Path(sys.argv[1]);p.mkdir(parents=True,exist_ok=True)
if any(p.iterdir()):raise SystemExit('Output directory must be empty: '+str(p))
PY
source "$repo_root/scripts/geo_replay/e2e_lib.sh"
e2e_init "$out" e2e_answer.sh retained
e2e_file "$test_binary"
export CANON_ANSWER_E2E_OUT="$out/projections"
e2e_cmd retained-projections 0 -- "$test_binary" --exact answer_e2e_prepare_worker --nocapture
manifest="$CANON_ANSWER_E2E_OUT/manifest.json"
e2e_file "$manifest"
python3 -I - "$E2E_LOG" "$manifest" "$repo_root" <<'PY'
import datetime,hashlib,json,pathlib,sys
log,manifest,repo=sys.argv[1:];m=json.load(open(manifest))
for case in m['cases']:
    source=pathlib.Path(repo)/case['archive']['path']
    digest=hashlib.sha256(source.read_bytes()).hexdigest()
    if digest!=case['archive']['sha256']:raise SystemExit('Archive digest mismatch: '+str(source))
    answer=json.load(open(case['answer']))
    row=dict(event='retained_answer',ts=datetime.datetime.now(datetime.timezone.utc).isoformat(),proof_class='retained',case_id=case['case_id'],archive=str(source),archive_sha256=digest,answer=case['answer'],answer_blake3=case['answer_blake3'],answer_sha256=hashlib.sha256(pathlib.Path(case['answer']).read_bytes()).hexdigest(),input_digests=answer['inputs'],claim_label=answer['claim_label'],composition_view=answer['preference']['composition_view'],grouped_view=answer['preference']['grouped_view'],lineage_groups=answer['association_support']['lineage_groups'],logical_consequence=answer['logical_consequence'],acceptance=answer['acceptance'],retained_execution=case['retained_execution'],no_claim=m['no_claim'])
    with open(log,'a') as f:f.write(json.dumps(row,sort_keys=True)+'\n')
PY
e2e_check parcel-leader '.preference.composition_view.preferred[0].members.parcels == ["302304550403000"] and .preference.grouped_view.tie_count == 1 and .logical_consequence.residual_model_count == 2983 and .acceptance.status == "no_policy_declared"' "$CANON_ANSWER_E2E_OUT/cornerstone.national_parcel_supported.answer.json"
e2e_check building-tie '.preference.composition_view.tie_count == 2 and .preference.grouped_view.tie_count == 2 and .logical_consequence.residual_model_count == 347' "$CANON_ANSWER_E2E_OUT/cornerstone.national_building_supported.answer.json"
e2e_check brooklyn-forcing '.logical_consequence.hard_forced.parcels == ["3051090025"] and .logical_consequence.hard_forced.buildings == ["3117371"] and .logical_consequence.residual_model_count == null and .extent_and_reach.complete_extent_claimed == false' "$CANON_ANSWER_E2E_OUT/brooklyn.02_address_building.answer.json"
e2e_check retained-failed-execution '.cases[] | select(.case_id == "brooklyn.02_address_building") | .retained_execution.actual_runner_history_available == true and .retained_execution.run_snapshot.status == "FAILED" and (.retained_execution.run_snapshot.completed_project_node_ids | length) == 7 and .retained_execution.run_snapshot.failed_project_node_ids == ["geo.parcel.separation"] and .retained_execution.run_snapshot.blocked_project_node_ids == ["geo.parcel.next_evidence"]' "$manifest"
e2e_check courtney-both-views '.preference.composition_view.preferred[0].members.parcels == ["1029330100"] and .preference.composition_view.preferred[0].cost == 6 and .preference.grouped_view.preferred[0].members.parcels == ["1029330100"] and .preference.grouped_view.cost_histogram == [{"cost":0,"model_count":1},{"cost":1,"model_count":6},{"cost":2,"model_count":882}]' "$CANON_ANSWER_E2E_OUT/courtney.final_hints.answer.json"
for case in cornerstone.national_building cornerstone.foursquare_parcel orlando.case_1.hints; do
  e2e_check "$case-no-positional-winner" '.preference.state.kind == "no_soft_evidence" and .preference.composition_view.preferred == [] and .preference.grouped_view.preferred == []' "$CANON_ANSWER_E2E_OUT/$case.answer.json"
done
e2e_check summit-tie-not-identity '.preference.composition_view.tie_count == 2 and .preference.grouped_view.tie_count == 2 and .acceptance.status == "no_policy_declared" and .logical_consequence.hard_forced.parcels == []' "$CANON_ANSWER_E2E_OUT/courtney.final_summit.answer.json"
run_args=()
while IFS= read -r -d '' argument; do run_args+=("$argument"); done < <(python3 -I - "$manifest" <<'PY'
import json,sys
for arg in json.load(open(sys.argv[1]))['run_args']:sys.stdout.buffer.write(arg.encode()+b'\0')
PY
)
for ((i=0; i < ${#run_args[@]}; i++)); do
  if [[ "${run_args[$i]}" == --plan ]]; then e2e_file "${run_args[$((i + 1))]}"; fi
  if [[ "${run_args[$i]}" == --input ]]; then e2e_file "${run_args[$((i + 1))]#*=}"; fi
done
e2e_geo_run registered-answer ABSTAINED -- "$CANON_BIN" "${run_args[@]}"
python3 -I - "$E2E_OUT/registered-answer.stdout" "$manifest" <<'PY'
import json,pathlib,sys
run=json.load(open(sys.argv[1]));m=json.load(open(sys.argv[2]));ref=next(r for r in run['output_refs'] if r['output_id']=='answer')
p=pathlib.Path(m['workspace'])/'.canon/geo-run/artifacts/cas'/(ref['content_digest'].split(':')[1]+'.bin')
expected=next(c for c in m['cases'] if c['case_id']=='cornerstone.national_building_supported')
if p.read_bytes()!=pathlib.Path(expected['answer']).read_bytes():raise SystemExit('Registered public run differs from retained pure projection')
PY
e2e_check registered-stage-usage '.deterministic_usage.answer_members_listed == 2 and .deterministic_usage.answer_preferred_models == 2' "$E2E_OUT/registered-answer.stdout"
e2e_finish
