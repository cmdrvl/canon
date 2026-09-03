#!/usr/bin/env bash
set -euo pipefail

fixture_dir="scripts/geo_measurements/fixtures/e4_gate_v2"
canon_bin="${CANON_BIN:-target/debug/canon}"
work_dir=""
refresh=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir)
      work_dir="${2:?missing --work-dir value}"
      shift 2
      ;;
    --refresh)
      refresh=1
      shift
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$work_dir" ]]; then
  echo "usage: $0 --work-dir <dir> [--refresh]" >&2
  exit 2
fi

if [[ ! -x "$canon_bin" ]]; then
  echo "missing executable $canon_bin; build target/debug/canon before running this harness" >&2
  exit 2
fi

if [[ "$refresh" -eq 1 ]]; then
  CANON_GEO_E4_RESTACK_WRITE=1 cargo test --test geo_e4_restack rewrite_e4_gate_v2_measurement_artifacts -- --ignored --nocapture
fi

mkdir -p "$work_dir"
log="$work_dir/run.log"
: > "$log"

run_json() {
  local output="$1"
  shift
  {
    printf '+'
    printf ' %q' "$@"
    printf ' > %q\n' "$output"
  } | tee -a "$log"
  set +e
  "$@" >"$output" 2>>"$log"
  local status=$?
  set -e
  echo "exit_code=$status" | tee -a "$log"
  return "$status"
}

pad_stack="$work_dir/pad_only_stack.json"
pad_eval="$work_dir/pad_only_evaluation.json"
pad_artifacts="$work_dir/pad_only_artifacts"
stacked_stack="$work_dir/stacked_stack.json"
stacked_eval="$work_dir/stacked_evaluation.json"
stacked_artifacts="$work_dir/stacked_artifacts"

run_json "$pad_stack" "$canon_bin" geo stack-evidence \
  --population "$fixture_dir/base_population_request.json" \
  --overlay "$fixture_dir/pad_only_overlay_request.json"
run_json "$pad_eval" "$canon_bin" geo evaluate \
  --population "$pad_stack" \
  --artifact-dir "$pad_artifacts"

run_json "$stacked_stack" "$canon_bin" geo stack-evidence \
  --population "$fixture_dir/widened_population_request.json" \
  --overlay "$fixture_dir/stacked_overlay_request.json"
run_json "$stacked_eval" "$canon_bin" geo evaluate \
  --population "$stacked_stack" \
  --artifact-dir "$stacked_artifacts"

jq -n \
  --slurpfile baseline "$pad_eval" \
  --slurpfile stacked "$stacked_eval" \
  '{
    proof_class: "fixture replay of retained warehouse snapshot; not live proof",
    g1_numbers: {
      pad_only_baseline: {
        cases: $baseline[0].summary.cases,
        evidence_no_observation_cases: $baseline[0].summary.evidence_no_observation_cases,
        reachable_cases: $baseline[0].summary.candidate_reach_full_cases,
        resolved_cases: $baseline[0].summary.resolved_cases,
        ambiguous_cases: $baseline[0].summary.ambiguous_cases,
        conflict_cases: $baseline[0].summary.conflict_cases,
        component_budget_fallback_cases: $baseline[0].summary.component_budget_fallback_cases,
        deed_exact_cases: ($baseline[0].cases | map(select(.status == "resolved" and .truth_model_in_residual == true)) | length),
        false_merge_cases: $baseline[0].summary.false_merge_cases,
        truth_exclusion_cases: $baseline[0].summary.solver_truth_exclusion_cases,
        residual_count_le16_cases: ($baseline[0].cases | map(select(.residual_model_count != null and .residual_model_count <= 16)) | length)
      },
      stacked: {
        cases: $stacked[0].summary.cases,
        evidence_no_observation_cases: $stacked[0].summary.evidence_no_observation_cases,
        reachable_cases: $stacked[0].summary.candidate_reach_full_cases,
        resolved_cases: $stacked[0].summary.resolved_cases,
        ambiguous_cases: $stacked[0].summary.ambiguous_cases,
        conflict_cases: $stacked[0].summary.conflict_cases,
        component_budget_fallback_cases: $stacked[0].summary.component_budget_fallback_cases,
        deed_exact_cases: ($stacked[0].cases | map(select(.status == "resolved" and .truth_model_in_residual == true)) | length),
        false_merge_cases: $stacked[0].summary.false_merge_cases,
        truth_exclusion_cases: $stacked[0].summary.solver_truth_exclusion_cases,
        residual_count_le16_cases: ($stacked[0].cases | map(select(.residual_model_count != null and .residual_model_count <= 16)) | length)
      }
    }
  }' | tee "$work_dir/g1_numbers.json" | tee -a "$log"
