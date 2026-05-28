#!/usr/bin/env bash
# LONG-RUNNING self-driving loop with CONTINUOUS DISPATCH.
#
# Every fleet worker is kept busy until the frontier is exhausted. As soon as
# any worker's land_wave.py finishes, the scheduler dispatches the next pending
# wave to that worker — NO batch-wait. Throughput is bounded by the fleet's
# aggregate rate, not by the slowest wave in each batch.
#
# Race-safety: the test-and-land phase inside land_wave.py is serialized via
# flock on .zigrun-land.lock (the only operator-tree mutator). dispatch + wait
# + recover_patch run fully concurrently.
#
# Each land_wave is wrapped so its exit code lands in a per-worker flag file
# (out/land-flags/<worker>.done). The scheduler polls those files — works on
# bash 3.2+ (macOS default), no `wait -n` required.
cd "$(dirname "$0")/.." || exit 2
BUDGET="${BUDGET_SECONDS:-14400}"          # 4 hours; override with BUDGET_SECONDS
start=$(date +%s)

WORKERS=(
  agent-b147cc87.claude-sonnet-0
  agent-b147cc87.claude-sonnet-1
  agent-b147cc87.claude-haiku-0
  agent-b147cc87.native-actor-0
  agent-b147cc87.native-actor-1
  agent-b147cc87.native-actor-2
)

FLAGS=../out/land-flags; mkdir -p "$FLAGS"
rm -f "$FLAGS"/*.done 2>/dev/null   # clean stale flags from prior run

# bash 3.2 doesn't support assoc arrays, but bash 4+ does. macOS default is 3.2;
# /opt/homebrew/bin/bash and the brew shell are 5.x. Use parallel indexed arrays
# instead so the shebang-resolved bash version doesn't matter.
WPID=()    # WPID[i] = pid running on WORKERS[i], or "" if idle
WAVE=()    # WAVE[i] = wave id WORKERS[i] is working on
for i in "${!WORKERS[@]}"; do WPID[$i]=""; WAVE[$i]=""; done

attempted=" "; landed=0; failed=0; discovered=0; nogap=0
elapsed() { echo $(( ($(date +%s) - start) / 60 )); }

# One-time self-heal — operator tree from HEAD. After this, the only mutator
# is land_wave.py's flock'd gate+land block.
git checkout HEAD -- zigrun/src zigrun/oracle/check.sh zigrun/oracle/diff.sh 2>/dev/null

next_pending() {
  local id
  for id in $(grep -oE '^- \[ \] [a-zA-Z0-9_]+' evolve/WAVES.md | awk '{print $NF}'); do
    [ -f "oracle/pending/$id.zig" ] || continue
    case "$attempted" in *" $id "*) continue ;; esac
    echo "$id"; return
  done
}

dispatch_to() {  # $1 = worker index
  local i=$1 w="${WORKERS[$1]}" nxt
  nxt=$(next_pending) || return 1
  [ -z "$nxt" ] && return 1
  attempted="$attempted$nxt "
  (
    python3 evolve/land_wave.py "$nxt" --worker "$w"
    echo $? > "$FLAGS/$w.done"
  ) &
  WPID[$i]=$!
  WAVE[$i]=$nxt
  echo "[$(elapsed)m] dispatch  $nxt -> $w  (pid=${WPID[$i]})"
  return 0
}

echo "[$(elapsed)m] continuous-dispatch scheduler START  budget=${BUDGET}s  fleet=${#WORKERS[@]}"

# Prime the fleet: dispatch to every worker.
for i in "${!WORKERS[@]}"; do dispatch_to "$i" || true; done

while [ $(( $(date +%s) - start )) -lt "$BUDGET" ]; do
  # 1. Reap finished workers via flag files. Each finished land_wave wrote
  #    its exit code to $FLAGS/<worker>.done; consume + free that slot.
  for i in "${!WORKERS[@]}"; do
    [ -z "${WPID[$i]}" ] && continue
    w="${WORKERS[$i]}"
    [ -f "$FLAGS/$w.done" ] || continue
    rc=$(cat "$FLAGS/$w.done"); rm -f "$FLAGS/$w.done"
    wait "${WPID[$i]}" 2>/dev/null   # reap the zombie subshell
    if [ "$rc" = "0" ]; then
      landed=$((landed + 1))
      echo "[$(elapsed)m] LANDED    ${WAVE[$i]} on $w  (landed=$landed failed=$failed)"
    else
      failed=$((failed + 1))
      echo "[$(elapsed)m] failed    ${WAVE[$i]} on $w (rc=$rc)  (landed=$landed failed=$failed)"
    fi
    WPID[$i]=""; WAVE[$i]=""
  done

  # 2. Refill idle workers immediately — continuous dispatch, no batch-wait.
  refilled=0
  for i in "${!WORKERS[@]}"; do
    if [ -z "${WPID[$i]}" ]; then
      dispatch_to "$i" && refilled=$((refilled + 1))
    fi
  done

  # 3. If fleet is fully idle AND there's nothing to pick, discover.
  active=0
  for i in "${!WORKERS[@]}"; do [ -n "${WPID[$i]}" ] && active=$((active + 1)); done
  if [ "$active" -eq 0 ] && [ -z "$(next_pending)" ]; then
    echo "[$(elapsed)m] frontier empty + fleet idle — DISCOVERING next gap from the oracle..."
    if python3 evolve/spec_author.py; then
      discovered=$((discovered + 1)); nogap=0
    else
      nogap=$((nogap + 1))
      echo "[$(elapsed)m] discovery found no gap (streak $nogap) — pause + retry (don't stop)"
      sleep 30
    fi
    continue   # immediately re-loop and prime workers with the new pending
  fi

  sleep 3
done

echo "[$(elapsed)m] budget reached — waiting on in-flight land_waves to finish their work..."
for i in "${!WORKERS[@]}"; do
  [ -n "${WPID[$i]}" ] && wait "${WPID[$i]}" 2>/dev/null
done
echo "===== AUTOLOOP BUDGET REACHED — landed=$landed failed=$failed discovered=$discovered ====="
