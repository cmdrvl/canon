#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s --work-dir DIR [--skip-cargo-tests]\n' "${0##*/}" >&2
  printf 'Derives fixture-class Franklin deed truth through measurement tooling and scores the truth plane through geo evaluate.\n' >&2
}

work_dir=""
run_cargo_tests=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      work_dir="$2"
      shift 2
      ;;
    --skip-cargo-tests)
      run_cargo_tests=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 64
      ;;
  esac
done

if [[ -z "$work_dir" ]]; then
  usage
  printf '--work-dir is required.\n' >&2
  exit 64
fi

if ! command -v jq >/dev/null 2>&1; then
  printf 'e2e_deed_truth requires jq on PATH\n' >&2
  exit 69
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
canon_bin="${CANON_BIN:-$repo_root/target/debug/canon}"
measurements_bin="${MEASUREMENTS_BIN:-$repo_root/target/debug/canon_geo_measurements}"

if [[ ! -x "$canon_bin" ]]; then
  printf 'missing executable %s; build target/debug/canon before running this harness\n' "$canon_bin" >&2
  exit 69
fi
if [[ ! -x "$measurements_bin" ]]; then
  printf 'missing executable %s; build target/debug/canon_geo_measurements before running this harness\n' "$measurements_bin" >&2
  exit 69
fi

mkdir -p "$work_dir"
log="$work_dir/run.log"
: > "$log"

log_line() {
  printf '%s\n' "$*" | tee -a "$log"
}

quote_command() {
  printf '%q' "$1"
  shift
  for arg in "$@"; do
    printf ' %q' "$arg"
  done
}

run_json() {
  local output="$1"
  shift
  {
    printf '$ '
    quote_command "$@"
    printf ' > %q\n' "$output"
  } | tee -a "$log"
  set +e
  "$@" >"$output" 2>>"$log"
  local status=$?
  set -e
  log_line "exit_code=$status"
  return "$status"
}

run_cmd() {
  {
    printf '$ '
    quote_command "$@"
    printf '\n'
  } | tee -a "$log"
  set +e
  "$@" 2>&1 | tee -a "$log"
  local status=${PIPESTATUS[0]}
  set -e
  log_line "exit_code=$status"
  return "$status"
}

sha256_file() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    printf 'no sha256sum or shasum available on PATH\n' >&2
    exit 69
  fi
}

check_jq() {
  local label="$1"
  local file="$2"
  local expression="$3"
  local expected="$4"
  local actual
  actual="$(jq -r "$expression" "$file")"
  log_line "$label $expression -> $actual"
  if [[ "$actual" != "$expected" ]]; then
    printf '%s expected %s from %s, got %s\n' "$label" "$expected" "$expression" "$actual" >&2
    exit 65
  fi
}

check_jq_true() {
  local label="$1"
  local file="$2"
  local expression="$3"
  if ! jq -e "$expression" "$file" >/dev/null; then
    printf '%s failed jq assertion: %s\n' "$label" "$expression" >&2
    exit 65
  fi
  log_line "$label $expression -> true"
}

log_sha256_artifact() {
  local file="$1"
  log_line "artifact_sha256 file=$file sha256=$(sha256_file "$file")"
}

log_blake3_artifacts_from_index() {
  local index="$1"
  jq -r '.cases[] |
    "artifact_blake3 case_id=\(.case_id) solve_file=\(.solve_file) solver_digest=\(.solver_digest) propagation_file=\(.propagation_file) propagation_digest=\(.propagation_digest) evidence_file=\(.evidence_file) compilation_digest=\(.compilation_digest)"' "$index" |
    while IFS= read -r line; do
      log_line "$line"
    done
}

deed_truth="$work_dir/deed-truth.json"
evaluation="$work_dir/evaluation.json"
artifact_dir="$work_dir/evaluation-artifacts"
artifact_index="$artifact_dir/index.json"
loan_fixture="$repo_root/tests/fixtures/geo/deed_truth_loans_fixture.json"
deed_fixture="$repo_root/tests/fixtures/geo/deed_index_fixture.json"
population_fixture="$repo_root/tests/fixtures/geo/franklin_population_fixture.json"

run_json "$deed_truth" "$measurements_bin" derive-deed-truth \
  --loans "$loan_fixture" \
  --deeds "$deed_fixture" \
  --window-days 45

check_jq deed_truth "$deed_truth" '.version' canon_geo_deed_truth.v0
check_jq deed_truth "$deed_truth" '.truth_plane' deed_grain_instrument
check_jq deed_truth "$deed_truth" '.proof_class' fixture
check_jq deed_truth "$deed_truth" '.summary.loans' 6
check_jq deed_truth "$deed_truth" '.summary.unique' 4
check_jq deed_truth "$deed_truth" '.summary.non_unique_discarded' 1
check_jq deed_truth "$deed_truth" '.summary.no_match' 1
check_jq deed_truth "$deed_truth" '.summary.round_amount_loans' 1
check_jq deed_truth "$deed_truth" '.summary.round_amount_unique' 1
check_jq deed_truth "$deed_truth" '(.summary.unique + .summary.non_unique_discarded + .summary.no_match) == .summary.loans' true
check_jq deed_truth "$deed_truth" '[.per_loan[] | select(.match_kind == "unique")] | length' 4
check_jq deed_truth "$deed_truth" '[.per_loan[] | select(.loan_id == "loan-004" and .match_kind == "non_unique_discarded" and (.parcel_ids | length) == 0)] | length' 1
check_jq deed_truth "$deed_truth" '[.per_loan[] | select(.loan_id == "loan-005" and .match_kind == "no_match" and (.parcel_ids | length) == 0)] | length' 1

run_json "$evaluation" "$canon_bin" geo evaluate \
  --population "$population_fixture" \
  --truth "$deed_truth" \
  --truth-plane deed_grain_instrument \
  --artifact-dir "$artifact_dir"

check_jq evaluation "$evaluation" '.version' canon_geo_population_evaluation.v0
check_jq evaluation "$evaluation" '.truth_binding.truth_plane' deed_grain_instrument
check_jq evaluation "$evaluation" '.truth_binding.source_proof_class' fixture
check_jq evaluation "$evaluation" '.truth_binding.input_loans' 6
check_jq evaluation "$evaluation" '.truth_binding.unique_truth_rows' 4
check_jq evaluation "$evaluation" '.truth_binding.bound_unique_cases' 4
check_jq evaluation "$evaluation" '.truth_binding.unique_not_in_population_cases' 0
check_jq evaluation "$evaluation" '.truth_binding.non_unique_discarded' 1
check_jq evaluation "$evaluation" '.truth_binding.no_match' 1
check_jq evaluation "$evaluation" '.truth_binding.deed_truth_unbound_cases' 2
check_jq evaluation "$evaluation" '.summary.cases' 4
check_jq evaluation "$evaluation" '.summary.truth_planes | length' 1
check_jq evaluation "$evaluation" '.summary.truth_planes[0].truth_plane' deed_grain_instrument
check_jq evaluation "$evaluation" '.summary.truth_planes[0].solver_truth_scored_cases' 3
check_jq evaluation "$evaluation" '.summary.truth_planes[0].candidate_reach_full_cases' 3
check_jq evaluation "$evaluation" '.summary.truth_planes[0].candidate_reach_partial_cases' 1
check_jq evaluation "$evaluation" '.summary.truth_planes[0].truth_members' 7
check_jq evaluation "$evaluation" '.summary.truth_planes[0].truth_members_in_universe' 5
check_jq evaluation "$evaluation" '.summary.truth_planes[0].false_merge_cases' 0
check_jq evaluation "$evaluation" '.summary.truth_planes[0].evidence_no_observation_cases' 4
check_jq evaluation "$evaluation" '[.cases[].case_id] | sort | join(",")' loan-001,loan-002,loan-003,loan-006

unique_truth_ids="$(jq -r '[.per_loan[] | select(.match_kind == "unique") | .loan_id] | sort | join(",")' "$deed_truth")"
scored_case_ids="$(jq -r '[.cases[].case_id] | sort | join(",")' "$evaluation")"
log_line "deed_truth_unique_loan_ids=$unique_truth_ids"
log_line "evaluation_scored_case_ids=$scored_case_ids"
if [[ "$unique_truth_ids" != "$scored_case_ids" ]]; then
  printf 'deed truth unique loan ids do not match evaluated case ids\n' >&2
  exit 65
fi

check_jq artifacts "$artifact_index" '.cases | length' 4
check_jq_true artifacts "$artifact_index" 'all(.cases[]; .truth_plane == "deed_grain_instrument")'
check_jq_true artifacts "$artifact_index" 'all(.cases[]; (.solver_digest | test("^[0-9a-f]{64}$")) and (.propagation_digest | test("^[0-9a-f]{64}$")) and (.compilation_digest | test("^[0-9a-f]{64}$")))'
log_blake3_artifacts_from_index "$artifact_index"

for artifact in "$deed_truth" "$evaluation" "$artifact_index"; do
  log_sha256_artifact "$artifact"
done

if [[ "$run_cargo_tests" -eq 1 ]]; then
  run_cmd cargo test --manifest-path "$repo_root/Cargo.toml" --test geo_deed_truth -- --nocapture
  run_cmd cargo test --manifest-path "$repo_root/Cargo.toml" --test geo_schemas deed_ -- --nocapture
  run_cmd cargo test --manifest-path "$repo_root/Cargo.toml" --test geo_e5_franklin_parcel franklin_instance_names_do_not_enter_the_generic_geo_engine -- --nocapture
else
  log_line "cargo_tests=skipped"
fi

log_line "deed-truth e2e ok work_dir=$work_dir"
shasum -a 256 "$log" > "$work_dir/run.log.sha256"
