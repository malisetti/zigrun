#!/usr/bin/env bash
# LONG-RUNNING self-driving loop — runs for a time budget (default 4h) and DOES
# NOT STOP on failures. Each iteration:
#   1. self-heal: restore the operator src/suite from HEAD (overwrites any rogue
#      worker edits or an interrupted land — the real hazard on this host is that
#      a dispatched worker can edit the operator tree directly).
#   2. if a pending specced wave exists (not already failed this run): LAND it.
#   3. else: DISCOVER the next gap from the oracle (planner generates programs;
#      real zig labels divergences). Keep retrying discovery on a no-gap.
# Tries to close every feature gap real zig exposes, for hours, skipping ones the
# workers can't build and moving on — never terminating until the budget elapses.
cd "$(dirname "$0")/.." || exit 2
BUDGET="${BUDGET_SECONDS:-14400}"          # 4 hours; override with BUDGET_SECONDS
start=$(date +%s)
attempted=" "; landed=0; failed=0; discovered=0; nogap=0; round=0
elapsed() { echo $(( ($(date +%s) - start) / 60 )); }

while [ $(( $(date +%s) - start )) -lt "$BUDGET" ]; do
  round=$((round + 1))
  # 1. self-heal the operator tree (clean rogue/interrupted edits)
  git checkout HEAD -- zigrun/src zigrun/oracle/check.sh zigrun/oracle/diff.sh 2>/dev/null

  # 2. next un-attempted pending wave that has a spec
  next=""
  for id in $(grep -oE '^- \[ \] [a-zA-Z0-9_]+' evolve/WAVES.md | awk '{print $NF}'); do
    [ -f "oracle/pending/$id.zig" ] || continue
    case "$attempted" in *" $id "*) continue ;; esac
    next="$id"; break
  done

  if [ -z "$next" ]; then
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

  echo "[r$round $(elapsed)m] LANDING '$next'  (landed=$landed failed=$failed discovered=$discovered)"
  if python3 evolve/land_wave.py "$next"; then
    landed=$((landed + 1))
  else
    failed=$((failed + 1))
  fi
  attempted="$attempted$next "
done

echo "===== AUTOLOOP BUDGET REACHED after $(elapsed)m — landed=$landed failed=$failed discovered=$discovered ====="
