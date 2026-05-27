#!/usr/bin/env bash
# The long-running, evolving zigrun orchestration — driver.
#
# One invocation advances the frontier by one wave. State is durable and
# resumable across runs:
#   evolve/WAVES.md        ordered backlog (status per wave)
#   oracle/ + diff.sh      the spec + the external-truth gate (real zig)
#   ~/.nfltr/ledger.json   per-(worker x role) outcomes (routing improves over time)
#   git history            every landed wave
#
# The driver owns SEQUENCING + the EXTERNAL-TRUTH GATE + bookkeeping. It does not
# write compiler code — that is the inner LLM's job (a dispatched worker) or the
# operator's. Division of labor: fleet/operator writes code, REAL ZIG judges,
# this driver evolves the frontier. Run it, dispatch the emitted objective, land
# the wave, run it again.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

ZIG="$(bash oracle/ensure_zig.sh)" || { echo "evolve: could not provision zig"; exit 2; }
cargo build --quiet 2>/dev/null || { echo "evolve: zigrun build failed"; exit 2; }
zigrun=target/debug/zigrun

done_n=$(grep -c '^- \[x\]' evolve/WAVES.md || true)
todo_n=$(grep -c '^- \[ \]' evolve/WAVES.md || true)
echo "== zigrun evolution frontier: ${done_n} landed, ${todo_n} pending =="

next=$(grep -m1 '^- \[ \]' evolve/WAVES.md || true)
if [ -z "$next" ]; then
  echo "frontier empty — every planned wave has landed."
  exit 0
fi

# parse: "- [ ] <id> | <oracle> | <objective>"
body=${next#- \[ \] }
id=$(printf '%s' "$body"  | awk -F' \\| ' '{print $1}' | tr -d ' ')
oracle=$(printf '%s' "$body" | awk -F' \\| ' '{print $2}' | tr -d ' ')
obj=$(printf '%s' "$body"  | awk -F' \\| ' '{print $3}')

echo "next wave: ${id}  (spec: ${oracle})"
if [ ! -f "$oracle" ]; then echo "  spec program missing: $oracle"; exit 2; fi

# Differential gate against real zig for this wave's program.
zout=$("$ZIG" run "$oracle" 2>/dev/null); zrc=$?
rout=$("$zigrun" run "$oracle" 2>/dev/null); rrc=$?

if [ "$zrc" = "$rrc" ] && [ "$zout" = "$rout" ]; then
  echo "  GATE GREEN: zigrun matches real zig (exit=$zrc) — wave '${id}' is implemented."
  echo "  -> promote: move $oracle into the suite, flip [ ]->[x] in WAVES.md, update FEATURES.md, then rerun."
  exit 0
fi

echo "  GATE RED: real zig{exit=$zrc} != zigrun{exit=$rrc} — '${id}' not implemented yet."
echo
echo "  DISPATCH THIS to a worker (inner LLM writes the compiler code):"
echo "  ------------------------------------------------------------------"
echo "  $obj"
echo "  Acceptance gate (un-fakeable, real zig is truth):"
echo "      bash oracle/diff.sh ${id}   # after promoting $oracle to oracle/${id}.zig"
echo "  ------------------------------------------------------------------"
exit 1
