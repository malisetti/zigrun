#!/usr/bin/env bash
# Differential oracle — ground truth is the REAL zig compiler.
#
# For each program, run it through `zig run` AND through `zigrun run`, and
# require the exit codes (and stdout) to match. This anchors zigrun's
# correctness to what Zig ACTUALLY does, not to hand-authored expectations a
# worker (or operator) could get wrong. A DIFF means zigrun diverges from real
# Zig — a true bug, surfaced by ground truth.
#
#   oracle/diff.sh                 # differential check (full suite)
#   oracle/diff.sh add fib         # subset
#   oracle/diff.sh --update [names] # refresh oracle/<name>.exit FROM real zig
#                                   #   (so the zig-free check.sh cache = zig truth)
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

# Self-provision ground truth: the oracle installs its own zig if needed.
ZIG="$(bash "$(dirname "$0")/ensure_zig.sh")" || {
  echo "diff: could not provision real zig"
  exit 2
}
if ! cargo build --quiet 2>/dev/null; then
  echo "diff: zigrun BUILD FAILED"
  exit 2
fi
zigrun=target/debug/zigrun

update=0; errors=0
case "${1:-}" in
  --update) update=1; shift ;;
  --errors) errors=1; shift ;;  # invalid programs: BOTH compilers must reject
esac
progs=("$@")
if [ ${#progs[@]} -eq 0 ]; then
  if [ $errors -eq 1 ]; then
    progs=(); for f in oracle/err/*.zig; do progs+=("$(basename "$f" .zig)"); done
  else
    progs=(add vars ifelse while fn fib bitops forloop switch elseif loopctl inttypes signedints u64wide unaryneg boollogic arraysum arrayidx atmod structfield struct optional recursion arithmetic enum for errors bitwise enums taggedunion_s5 taggedunion_s2 taggedunion_s3 structmethod_s1 structmethod_s3 structmethod_s2 structmethod_s4 structmethod_s5 packedstruct_s3 packedstruct_s2 packedstruct_s4 errorunion_s5 errorunion_s1 errorset_s2 labeledloop switchrange_s5 switchrange_s1 switchrange_s2 multidim_s4 errorunion_s3 errorunion_s2 helloworld print packedstruct_s1 multidim_s1 multidim_s2 errorset_s1 switchrange_s3 multidim_s3 errorset_s3 slice_s2 optional_s2 slice_s3 optional_s3 taggedunion_s4 switchrange_s4 errorunion_s4 errorset_s4 slice_s4 optional_s4 loops packedstruct_s5 multidim_s5 errorset_s5 slice_s5 optional_s5 error_unions tagged_unions switch_ranges comptime structs optionals packed_struct)
  fi
fi

fail=0
for p in "${progs[@]}"; do
  if [ $errors -eq 1 ]; then
    src="oracle/err/$p.zig"
    "$ZIG" run "$src" >/dev/null 2>&1; zrc=$?
    "$zigrun" run "$src" >/dev/null 2>&1; rrc=$?
    if [ "$zrc" -ne 0 ] && [ "$rrc" -ne 0 ]; then
      echo "ok    err/$p: both reject (zig=$zrc zigrun=$rrc)"
    else
      echo "DIFF  err/$p: zig=$zrc zigrun=$rrc — an INVALID program was accepted"
      fail=1
    fi
    continue
  fi
  src="oracle/$p.zig"
  if [ ! -f "$src" ]; then echo "MISSING $src"; fail=1; continue; fi

  # Warm the zig build cache first, so the comparison run's stderr is the
  # PROGRAM's output only (not one-time build progress). Then compare exit code,
  # stdout AND stderr. stderr matters: std.debug.print writes there, so comparing
  # it is what makes print/output waves false-green-safe (a worker can't pass by
  # exiting 0 while printing nothing).
  td="$(mktemp -d)"
  "$ZIG" run "$src" >/dev/null 2>&1
  zout=$("$ZIG" run "$src" 2>"$td/ze"); zrc=$?; zerr=$(cat "$td/ze")

  if [ $update -eq 1 ]; then
    echo "$zrc" > "oracle/$p.exit"
    echo "updated $p.exit = $zrc (from real zig)"
    rm -rf "$td"; continue
  fi

  rout=$("$zigrun" run "$src" 2>"$td/re"); rrc=$?; rerr=$(cat "$td/re")
  rm -rf "$td"
  if [ "$zrc" = "$rrc" ] && [ "$zout" = "$rout" ] && [ "$zerr" = "$rerr" ]; then
    echo "ok    $p: zig=$zrc zigrun=$rrc (exit+stdout+stderr match)"
  else
    echo "DIFF  $p: zig{exit=$zrc out=$(printf '%q' "$zout") err=$(printf '%q' "$zerr")} != zigrun{exit=$rrc out=$(printf '%q' "$rout") err=$(printf '%q' "$rerr")}"
    fail=1
  fi
done

if [ $fail -eq 0 ]; then
  echo "DIFFERENTIAL GREEN vs real zig (${#progs[@]} program(s))"
else
  echo "DIFFERENTIAL RED — zigrun diverges from real Zig"
fi
exit $fail
