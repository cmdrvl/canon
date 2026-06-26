#!/usr/bin/env bash
set -euo pipefail

fixture="tests/fixtures/entity/e2e/cmbs_small"
verbose=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --fixture)
      fixture="${2:?missing fixture path}"
      shift 2
      ;;
    --verbose)
      verbose=1
      shift
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

if [[ ! -f "$fixture/manifest.json" ]]; then
  printf 'missing CMBS mini e2e manifest: %s/manifest.json\n' "$fixture" >&2
  exit 2
fi

cmd=(cargo test --test cmbs_e2e entity_cmbs_e2e_small -- --nocapture)
if [[ "$verbose" -eq 1 ]]; then
  printf 'fixture=%s\n' "$fixture"
  printf 'command='
  printf '%q ' "${cmd[@]}"
  printf '\n'
fi

"${cmd[@]}"
