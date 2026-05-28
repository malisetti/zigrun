#!/usr/bin/env bash
# LONG-RUNNING self-driving loop — runs for a time budget (default 4h) and DOES
# NOT STOP on failures. Each iteration:
#   1. self-heal: restore the operator src/suite from HEAD (overwrites any rogue
#      worker edits or an interrupted land — the real hazard on this host is that
#      a dispatched worker can edit the operator tree directly).
#   2. fan out up to N pending waves to N workers IN PARALLEL — every fleet
#      worker is used per iteration (user directive: 4-6 parallel slices,
#      maximize parallelism, minimize wall time). dispatch+wait+recover_patch
#      run concurrently; the test-and-land phase serializes via a flock inside
#      land_wave.py (the only step that mutates the shared operator tree).
#   3. if no pending specced waves: DISCOVER the next gap from the oracle
#      (planner generates programs; real zig labels divergences).
# Tries to close every feature gap real zig exposes, for hours, skipping ones
# the workers can't build and moving on — never terminating until the budget.
cd "$(dirname "$0")/.." || exit 2
BUDGET="${BUDGET_SECONDS:-14400}"          # 4 hours; override with BUDGET_SECONDS
start=$(date +%s)

# Fleet workers — one parallel slice per worker per iteration.
WORKERS=(
  agent-b147cc87.claude-sonnet-0
  agent-b147cc87.claude-sonnet-1
  agent-b147cc87.claude-haiku-0
  agent-b147cc87.native-actor-0
  agent-b147cc87.native-actor-1
  agent-b147cc87.native-actor-2
)
N=${#WORKERS[@]}

attempted=" "; landed=0; failed=0; discovered=0; nogap=0; round=0
elapsed() { echo $(( ($(date +%s) - start) / 60 )); }

while [ $(( $(date +%s) - start )) -lt "$BUDGET" ]; do
  round=$((round + 1))
  # 1. self-heal the operator tree (clean rogue/interrupted edits)
  git checkout HEAD -- zigrun/src zigrun/oracle/check.sh zigrun/oracle/diff.sh 2>/dev/null

  # 2. pick up to N un-attempted pending waves with specs
  slate=()
  for id in $(grep -oE '^- \[ \] [a-zA-Z0-9_]+' evolve/WAVES.md | awk '{print $NF}'); do
    [ -f "oracle/pending/$id.zig" ] || continue
    case "$attempted" in *" $id "*) continue ;; esac
    slate+=("$id")
    [ ${#slate[@]} -ge $N ] && break
  done

  if [ ${#slate[@]} -eq 0 ]; then
    echo "[r$round $(elapsed)m] frontier empty — DISCOVERING next gap from the oracle..."
    if python3 evolve/spec_author.py; then
      discovered=$((discovered + 1)); nogap=0
    else
      nogap=$((nogap + 1))
      echo "[r$round] discovery found no new gap (streak $nogap) — pause + retry (don't stop)"
      sleep 30
    fi
    continue
  fi

  echo "[r$round $(elapsed)m] LANDING ${#slate[@]} waves in parallel: ${slate[*]}  (cum landed=$landed failed=$failed)"
  pids=()
  for i in "${!slate[@]}"; do
    wid="${slate[$i]}"
    wkr="${WORKERS[$((i % N))]}"
    python3 evolve/land_wave.py "$wid" --worker "$wkr" &
    pids+=("$!")
    attempted="$attempted$wid "
  done
  # Wait for all parallel land_waves; tally landed vs failed.
  for p in "${pids[@]}"; do
    if wait "$p"; then landed=$((landed + 1)); else failed=$((failed + 1)); fi
  done
done

echo "===== AUTOLOOP BUDGET REACHED after $(elapsed)m — landed=$landed failed=$failed discovered=$discovered ====="
