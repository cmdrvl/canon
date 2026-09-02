#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s [--work-dir DIR]\n' "${0##*/}" >&2
  printf 'Validates the retained D4 E1 point-population fixtures and SQL pins.\n' >&2
}

work_dir=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir)
      if [[ $# -lt 2 ]]; then
        usage
        exit 64
      fi
      work_dir="$2"
      shift 2
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

if ! command -v jq >/dev/null 2>&1; then
  printf 'e2e_point_population requires jq on PATH\n' >&2
  exit 69
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"

if [[ -z "$work_dir" ]]; then
  work_dir="$(mktemp -d "${TMPDIR:-/tmp}/canon-geo-point-population.XXXXXX")"
else
  mkdir -p "$work_dir"
fi
log="$work_dir/run.log"
: > "$log"

log_line() {
  printf '%s\n' "$*" | tee -a "$log"
}

run_cmd() {
  log_line "$ $*"
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

check_sql_pin() {
  local label="$1"
  local fixture="$2"
  local sql="$3"
  local expected actual
  expected="$(jq -r '.selection_query_sha256' "$fixture")"
  actual="$(sha256_file "$sql")"
  log_line "$label fixture=$fixture"
  log_line "$label sql=$sql"
  log_line "$label selection_query_sha256=$expected"
  log_line "$label source_sql_sha256=$actual"
  if [[ "$expected" != "$actual" ]]; then
    printf '%s SQL SHA-256 mismatch: fixture=%s sql=%s\n' "$label" "$expected" "$actual" >&2
    exit 65
  fi
}

check_jq() {
  local label="$1"
  local fixture="$2"
  local expression="$3"
  local expected="$4"
  local actual
  actual="$(jq -r "$expression" "$fixture")"
  log_line "$label $expression -> $actual"
  if [[ "$actual" != "$expected" ]]; then
    printf '%s expected %s from %s, got %s\n' "$label" "$expected" "$expression" "$actual" >&2
    exit 65
  fi
}

gross_fixture="$repo_root/tests/fixtures/geo/e1_gross_class_points.json"
condo_fixture="$repo_root/tests/fixtures/geo/e1_condo_points.json"
gross_sql="$repo_root/scripts/geo_measurements/e1_gross_class_points.sql"
condo_sql="$repo_root/scripts/geo_measurements/e1_condo_points.sql"

check_sql_pin gross "$gross_fixture" "$gross_sql"
check_sql_pin condo "$condo_fixture" "$condo_sql"
check_jq gross "$gross_fixture" '.points | length' 40
check_jq condo "$condo_fixture" '.points | length' 31
check_jq condo "$condo_fixture" '[.points[] | select(.billing_equals_pip == true)] | length' 10
check_jq gross "$gross_fixture" '.source_dataset | startswith("fixture.")' true
check_jq condo "$condo_fixture" '.source_dataset | startswith("fixture.")' true

run_cmd cargo test --manifest-path "$repo_root/Cargo.toml" --test geo_point_population -- --nocapture
run_cmd cargo test --manifest-path "$repo_root/Cargo.toml" --test geo_schemas point_population_schema_matches_a_real_instance -- --nocapture

log_line "point-population e2e ok work_dir=$work_dir"
