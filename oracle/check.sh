#!/usr/bin/env bash
# The EXTERNAL ORACLE for zigrun.
#
# Builds zigrun, runs each oracle/<name>.zig, and checks the process exit code
# (which is the program's `main() u8` return value) against oracle/<name>.exit.
# A program running to the right answer is the definition of "done" — it cannot
# be faked by passing unit tests. Exits 0 iff every requested program is correct.
#
# Usage:
#   oracle/check.sh                 # run the full suite
#   oracle/check.sh add vars        # run only a wave's subset (oracle-gated slices)
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

if ! cargo build --quiet 2>/dev/null; then
  echo "BUILD FAILED"
  exit 2
fi
bin=target/debug/zigrun

progs=("$@")
if [ ${#progs[@]} -eq 0 ]; then
  progs=(add vars ifelse while fn fib bitops forloop switch elseif loopctl inttypes signedints u64wide unaryneg boollogic arraysum arrayidx atmod structfield optional recursion arithmetic enum for errors)
fi

fail=0
for p in "${progs[@]}"; do
  if [ ! -f "oracle/$p.zig" ] || [ ! -f "oracle/$p.exit" ]; then
    echo "MISSING oracle/$p.{zig,exit}"
    fail=1
    continue
  fi
  want=$(tr -d '[:space:]' < "oracle/$p.exit")
  "./$bin" run "oracle/$p.zig" >/dev/null 2>&1
  got=$?
  if [ "$got" = "$want" ]; then
    echo "ok    $p -> $got"
  else
    echo "FAIL  $p -> got $got, want $want"
    fail=1
  fi
done

if [ $fail -eq 0 ]; then
  echo "ORACLE GREEN (${#progs[@]} program(s) run correctly)"
else
  echo "ORACLE RED"
fi
exit $fail
