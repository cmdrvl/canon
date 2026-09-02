#!/usr/bin/env bash
set -euo pipefail

work_dir=""
h7_population=""
overlay=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir)
      work_dir="${2:?missing --work-dir value}"
      shift 2
      ;;
    --h7-population)
      h7_population="${2:?missing --h7-population value}"
      shift 2
      ;;
    --overlay)
      overlay="${2:?missing --overlay value}"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$work_dir" || -z "$h7_population" || -z "$overlay" ]]; then
  echo "usage: $0 --work-dir <dir> --h7-population <canon_geo_h7_population.v0.json> --overlay <canon_geo_population_evidence_stack_request.v0.json>" >&2
  exit 2
fi

mkdir -p "$work_dir"
log="$work_dir/run.log"
: > "$log"

run_logged() {
  {
    printf '+'
    printf ' %q' "$@"
    printf '\n'
  } | tee -a "$log"
  set +e
  "$@" >>"$log" 2>&1
  local status=$?
  set -e
  echo "exit_code=$status" | tee -a "$log"
  return "$status"
}

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

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

if run_logged cargo test --test geo_adjudication e4_acceptance_gate_requires_the_full_population_to_be_reachable -- --ignored --nocapture; then
  echo "frozen_e4_gate=GREEN" | tee -a "$log"
else
  echo "frozen_e4_gate=RED" | tee -a "$log"
fi

cohort_request="$work_dir/d1_population_request.json"
stack_artifact="$work_dir/d1_population_evidence_stack.json"
evaluation_artifact="$work_dir/d1_population_evaluation.json"
artifact_dir="$work_dir/d1_residuals"

jq -e '.version == "canon_geo_h7_population.v0" and .summary.population_scope == "observed_snapshot"' "$h7_population" >>"$log"
jq -e '.population' "$h7_population" > "$cohort_request"
run_json "$stack_artifact" cargo run --quiet -- geo stack-evidence --population "$cohort_request" --overlay "$overlay"
run_json "$evaluation_artifact" cargo run --quiet -- geo evaluate --population "$stack_artifact" --artifact-dir "$artifact_dir"

composition_sha256="$(sha256_file src/geo/composition.rs)"
evidence_sha256="$(sha256_file src/geo/evidence.rs)"
tmp_index="$artifact_dir/index.d1.json"
jq -n \
  --slurpfile h7 "$h7_population" \
  --slurpfile evaluation "$evaluation_artifact" \
  --slurpfile produced "$artifact_dir/index.json" \
  --arg composition_sha256 "$composition_sha256" \
  --arg evidence_sha256 "$evidence_sha256" \
  '{
    proof_class: "observed_snapshot",
    build_id: $h7[0].provenance.bridge_build_id,
    c25_freeze_sha256: {
      algorithm: "sha256",
      "src/geo/composition.rs": $composition_sha256,
      "src/geo/evidence.rs": $evidence_sha256
    },
    per_truth_plane: (
      $evaluation[0].cases
      | sort_by(.truth_plane, .case_id)
      | group_by(.truth_plane)
      | map({(.[0].truth_plane): map(.case_id)})
      | add
    ),
    reach: (
      $evaluation[0].cases
      | map({(.case_id): .candidate_reach})
      | add
    ),
    cases: $produced[0].cases
  }' > "$tmp_index"
mv "$tmp_index" "$artifact_dir/index.json"

jq -e '.per_truth_plane | keys | length >= 2' "$artifact_dir/index.json" >>"$log"
printf '%s  src/geo/composition.rs\n' "$composition_sha256" > "$work_dir/c25_freeze_sha256.txt"
printf '%s  src/geo/evidence.rs\n' "$evidence_sha256" >> "$work_dir/c25_freeze_sha256.txt"
cat "$work_dir/c25_freeze_sha256.txt" >> "$log"

echo "evaluation_cases=$(jq -r '.summary.cases' "$evaluation_artifact")" | tee -a "$log"
echo "evidence_no_observation_cases=$(jq -r '.summary.evidence_no_observation_cases' "$evaluation_artifact")" | tee -a "$log"
jq -r '.per_truth_plane[] | "plane=\(.truth_plane) evidence_no_observation_cases=\(.evidence_no_observation_cases)"' "$evaluation_artifact" | tee -a "$log"
