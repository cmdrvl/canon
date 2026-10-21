#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export CARGO_NET_OFFLINE=true
source "$SCRIPT_DIR/e2e_lib.sh"
arguments=("$@")
argument_count=$#
replay_out=""
usage_only=false
for ((i=0; i < argument_count; i++)); do
    case "${arguments[$i]}" in
        --out) replay_out="${arguments[$((i + 1))]}" ;;
        --binary) CANON_BIN="${arguments[$((i + 1))]}"; export CANON_BIN ;;
        --usage-only) usage_only=true ;;
    esac
done
if "$usage_only"; then exec python3 -I "$SCRIPT_DIR/replay.py" "$@"; fi
if [[ -z "$replay_out" ]]; then
    replay_out=$(mktemp -d "${TMPDIR:-/tmp}/canon-retained-suite.XXXXXX")
    arguments+=(--out "$replay_out")
fi
cd "$SCRIPT_DIR/../.."
e2e_init "$replay_out" replay_fast.sh retained
e2e_cmd replay 0 -- python3 -I "$SCRIPT_DIR/replay.py" "${arguments[@]}"
cat "$E2E_OUT/replay.stdout"
e2e_finish
