#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
work_dir=""
live_acquisition=false

usage() {
  cat >&2 <<'USAGE'
usage: scripts/geo_observer/e2e_pins.sh --work-dir <dir> [--live-acquisition]

Runs the focused D6 pin/population checks. Whole-repo cargo gates are owned by
the orchestrator in swarm runs and are not launched here by default.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir)
      work_dir="${2:-}"
      shift 2
      ;;
    --live-acquisition)
      live_acquisition=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$work_dir" ]]; then
  usage
  exit 2
fi

mkdir -p "$work_dir"
log="$work_dir/run.log"
: > "$log"

run_step() {
  echo "+ $*" | tee -a "$log"
  "$@" 2>&1 | tee -a "$log"
  local status="${PIPESTATUS[0]}"
  echo "exit=$status" | tee -a "$log"
  return "$status"
}

sha_pin() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | tee -a "$log"
  else
    shasum -a 256 "$1" | tee -a "$log"
  fi
}

cd "$repo_root"
sha_pin scripts/geo_observer/licenses/cc_by_4_0.txt
sha_pin scripts/geo_observer/pins/nyc_ortho_2024.pins.json
sha_pin scripts/geo_observer/pins/nyc_ortho_2022.pins.json
sha_pin scripts/geo_observer/populations/nyc_h7_observer_source_population.v0.json
sha_pin scripts/geo_observer/populations/nyc_h7_observer_population.v0.json

run_step cargo test --test geo_observer_pins license_text_digest_matches_every_pin_row -- --nocapture
run_step cargo test --test geo_observer_pins -- --nocapture
run_step cargo test --test geo_schemas error_population_schema_matches_a_real_instance -- --nocapture
run_step jq -e '.title == "canon.geo.error_population.v0" and .properties.version.const == "canon_geo_error_population.v0" and .additionalProperties == false' schemas/canon.geo.error_population.v0.schema.json

if [[ "$live_acquisition" == true ]]; then
  run_step bash scripts/geo_observer/pin_tiles.sh \
    --source nyc_ortho \
    --vintage 2024 \
    --windows scripts/geo_observer/populations/nyc_h7_observer_source_population.v0.json \
    --out scripts/geo_observer/pins
  run_step git diff --stat -- scripts/geo_observer/pins
fi

echo "full cargo fmt/clippy/test gates intentionally left to the orchestrator" | tee -a "$log"
