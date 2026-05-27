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
    progs=(add vars ifelse while fn fib bitops forloop switch elseif loopctl)
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

  # Ground truth from real zig (stderr = build noise, dropped).
  zout=$("$ZIG" run "$src" 2>/dev/null); zrc=$?

  if [ $update -eq 1 ]; then
    echo "$zrc" > "oracle/$p.exit"
    echo "updated $p.exit = $zrc (from real zig)"
    continue
  fi

  rout=$("$zigrun" run "$src" 2>/dev/null); rrc=$?
  if [ "$zrc" = "$rrc" ] && [ "$zout" = "$rout" ]; then
    echo "ok    $p: zig=$zrc zigrun=$rrc (match)"
  else
    echo "DIFF  $p: zig{exit=$zrc out=$(printf '%q' "$zout")} != zigrun{exit=$rrc out=$(printf '%q' "$rout")}"
    fail=1
  fi
done

if [ $fail -eq 0 ]; then
  echo "DIFFERENTIAL GREEN vs real zig (${#progs[@]} program(s))"
else
  echo "DIFFERENTIAL RED — zigrun diverges from real Zig"
fi
exit $fail
