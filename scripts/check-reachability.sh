#!/usr/bin/env bash
# Fail if any file under src/ compiles into neither the library nor the binary.
#
# Rationale: a module that is not declared anywhere still compiles green when a
# test pulls it in with #[path = "../src/..."]. That produces passing tests for
# code the shipped artifact does not contain. This check reads the compiler's
# own dependency info, so it cannot be fooled by #[path] re-homing, by
# #[doc(hidden)], or by a module declared in an unexpected parent.
#
# Usage: scripts/check-reachability.sh [--allow <path>]...
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

ALLOW=()
while [ $# -gt 0 ]; do
  case "$1" in
    --allow) ALLOW+=("$2"); shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

cargo build --quiet

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Every src file the compiler actually fed into the lib or the bin.
cat target/debug/libcanon.d target/debug/canon.d \
  | tr ' ' '\n' \
  | grep -E '^/.*/src/.*\.rs$' \
  | sed "s|^$PWD/||" \
  | sort -u > "$work/compiled.txt"

# Every src file on disk, excluding standalone binaries which have their own targets.
find src -name '*.rs' \
  | grep -v '^src/bin/' \
  | sort > "$work/present.txt"

comm -13 "$work/compiled.txt" "$work/present.txt" > "$work/orphans.txt"

for allowed in ${ALLOW+"${ALLOW[@]}"}; do
  grep -vxF "$allowed" "$work/orphans.txt" > "$work/filtered.txt" || true
  mv "$work/filtered.txt" "$work/orphans.txt"
done

if [ -s "$work/orphans.txt" ]; then
  count=$(wc -l < "$work/orphans.txt" | tr -d ' ')
  lines=$(xargs wc -l < "$work/orphans.txt" | tail -1 | awk '{print $1}')
  echo "error: $count file(s) under src/ (${lines} lines) compile into no target." >&2
  echo "They can only be reached by a #[path] include from tests/, so their tests" >&2
  echo "pass while the shipped binary does not contain them." >&2
  echo >&2
  while read -r f; do
    printf '  %6s  %s\n' "$(wc -l < "$f" | tr -d ' ')" "$f" >&2
  done < "$work/orphans.txt"
  echo >&2
  echo "Fix by declaring the module in its parent and importing it in the test as" >&2
  echo "canon::..., by deleting it, or by adding it to the reviewed allow list." >&2
  exit 1
fi

echo "reachability: every file under src/ compiles into the library or the binary"
