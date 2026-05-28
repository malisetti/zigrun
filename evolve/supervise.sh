#!/usr/bin/env bash
# Long-running PROGRESS supervisor for the zigrun self-driving loop.
# Operators start THIS script only — they do not implement waves (see OPERATOR_BOUNDARY.md).
#
# MODE=frontier (default): six cursor implementers (max-tasks=1 each) +
# cursor local-integrator; restarts frontier_run.sh until budget.
# MODE=land_wave: legacy autoloop.sh + land_wave.py continuous dispatch.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2          # zigrun/
REPO_ROOT="$(cd .. && pwd)"
NF=/Users/b/.local/bin/nfltr
KEY=rpc_eb308b4f651879bde55a79de2acc1371916176bffdd1745d61dd8586997760a0831d79c9a288b96c46fad9441ddfa55c
LOGD=../out/fleet; mkdir -p "$LOGD"
CHECK=120
BUDGET="${SUP_BUDGET:-14400}"
MODE="${MODE:-frontier}"
# One pending wave per cursor worker per batch (fleet size = 6).
BATCH_SIZE="${FRONTIER_BATCH_SIZE:-6}"
start=$(date +%s)
now() { date +%H:%M; }

spawn_cursor() { # name
  pgrep -f "nfltr worker --name $1 " >/dev/null && return
  echo "[$(now)] respawn $1"
  nohup "$NF" worker --name "$1" --api-key "$KEY" --flavor cursor \
    --labels model=composer-2.5,tier=heavy,flavor=cursor \
    --execution-roles implementer,verifier,reducer --max-tasks 1 --per-task-worktree \
    --heartbeat-interval 15s \
    --mcp-command "nfltr cursor-mcp --cursor-command cursor-agent --model composer-2.5 --git-code-result --max-verifier-turns 5" \
    > "$LOGD/$1.log" 2>&1 &
}

spawn_integrator() {
  pgrep -f "nfltr worker --name local-integrator " >/dev/null && return
  echo "[$(now)] respawn local-integrator (cursor)"
  nohup "$NF" worker --name local-integrator --api-key "$KEY" --flavor cursor \
    --labels "role=integrator,flavor=cursor,tier=light" \
    --execution-roles integrator --max-tasks 1 \
    --heartbeat-interval 15s \
    --mcp-command "nfltr cursor-mcp --cursor-command cursor-agent --model composer-2.5 --git-code-result --max-verifier-turns 5" \
    > "$LOGD/local-integrator.log" 2>&1 &
}

fleet_up() {
  spawn_cursor native-actor-0
  spawn_cursor native-actor-1
  spawn_cursor native-actor-2
  spawn_cursor native-actor-3
  spawn_cursor native-actor-4
  spawn_cursor native-actor-5
  if [ "$MODE" = "frontier" ]; then
    spawn_integrator
  fi
}

rekick() {
  echo "[$(now)] PROGRESS STALLED — re-kicking fleet (kill + prune + respawn)"
  pkill -9 -f "nfltr worker --name" 2>/dev/null
  pkill -9 -f "cursor-agent.* agent" 2>/dev/null
  if [ "$MODE" = "frontier" ]; then
    pkill -9 -f "orch frontier-run" 2>/dev/null
    pkill -9 -f "evolve/frontier_run.sh" 2>/dev/null
  else
    pkill -9 -f "evolve/autoloop.sh" 2>/dev/null
  fi
  sleep 3
  git worktree prune 2>/dev/null
  fleet_up
}

landed() { grep -c "^- \[x\]" evolve/WAVES.md; }

frontier_driver_up() {
  if pgrep -f "evolve/frontier_run.sh" >/dev/null; then
    return
  fi
  local pending
  pending=$(grep -c "^- \[ \] " evolve/WAVES.md || true)
  if [ "$pending" -eq 0 ]; then
    echo "[$(now)] frontier-run idle: no pending waves in WAVES.md"
    return
  fi
  local rem=$(( BUDGET - ($(date +%s) - start) ))
  echo "[$(now)] frontier-run start: pending=$pending batch=$BATCH_SIZE fleet=6 (remaining ${rem}s)"
  echo "===== $(date) supervise.sh starting frontier_run (6x cursor, pending=$pending) =====" \
    >> "$REPO_ROOT/out/frontier-run.log"
  (
    cd "$REPO_ROOT" || exit 2
    export REPO_ROOT
    export FRONTIER_BATCH_SIZE="$BATCH_SIZE"
    export FRONTIER_CONCURRENCY="$BATCH_SIZE"
    export FRONTIER_BUDGET_SEC="$rem"
    nohup bash zigrun/evolve/frontier_run.sh >> "$REPO_ROOT/out/frontier-run.log" 2>&1 &
  )
}

autoloop_up() {
  if pgrep -f "evolve/autoloop.sh" >/dev/null; then
    return
  fi
  local rem=$(( BUDGET - ($(date +%s) - start) ))
  echo "[$(now)] autoloop dead — restart (remaining ${rem}s)"
  BUDGET_SECONDS=$rem nohup bash evolve/autoloop.sh >> ../out/autoloop-long.log 2>&1 &
}

last=$(landed); stall=0
echo "[$(now)] supervisor start (mode=$MODE, cursor x6): landed=$last budget=${BUDGET}s"

while [ $(( $(date +%s) - start )) -lt "$BUDGET" ]; do
  fleet_up
  if [ "$MODE" = "frontier" ]; then
    frontier_driver_up
  else
    autoloop_up
  fi
  cur=$(landed)
  if [ "$cur" -gt "$last" ]; then
    echo "[$(now)] PROGRESS: landed $last -> $cur"
    last=$cur
    stall=0
  else
    stall=$((stall + 1))
    echo "[$(now)] no new land (landed=$cur, stall streak $stall/3)"
  fi
  if [ "$stall" -ge 3 ]; then
    rekick
    stall=0
  fi
  sleep "$CHECK"
done

echo "[$(now)] supervisor budget reached — final landed=$(landed)"
