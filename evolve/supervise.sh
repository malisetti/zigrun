#!/usr/bin/env bash
# Long-running PROGRESS supervisor for the zigrun self-driving loop.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2          # zigrun/
REPO_ROOT="$(cd .. && pwd)"
NF="${NFLTR:-$REPO_ROOT/out/nfltr}"
if [ ! -x "$NF" ]; then
  NF="${HOME}/.local/bin/nfltr"
fi
if [ -f "${HOME}/.nfltr_new_key" ]; then
  KEY="$(tr -d '[:space:]' < "${HOME}/.nfltr_new_key")"
else
  echo "supervise.sh: missing ~/.nfltr_new_key" >&2
  exit 2
fi
export NFLTR_API_KEY="$KEY"
LOGD="$REPO_ROOT/out/fleet"
mkdir -p "$LOGD"
CHECK=300
# Default 5h when operator asks for long runs; override with SUP_BUDGET=seconds.
BUDGET="${SUP_BUDGET:-18000}"
MODE="${MODE:-frontier}"
FLEET="${FLEET:-cursor}"
FLEET_HOME="${FLEET_HOME:-$HOME/nfltr-fleet}"
AGENT_PREFIX="${AGENT_PREFIX:-agent-b147cc87}"
NFLTR_REPO_URL="$(git -C "$REPO_ROOT" config --get remote.origin.url)"
export NFLTR_REPO_URL
BATCH_SIZE="${FRONTIER_BATCH_SIZE:-3}"
FRONTIER_ROOT_TASK_ID="${FRONTIER_ROOT_TASK_ID:-zigrun-supervise-$(date -u +%Y%m%dT%H%M%SZ)}"
export FRONTIER_ROOT_TASK_ID
start=$(date +%s)
end=$(( start + BUDGET ))
now() { date +%H:%M; }

spawn_cursor() { # name
  pgrep -f "nfltr worker --name $1 " >/dev/null && return
  echo "[$(now)] respawn $1"
  nohup "$NF" worker --name "$1" --api-key "$KEY" --flavor cursor \
    --labels model=composer-2.5,tier=heavy,flavor=cursor \
    --execution-roles implementer,verifier,reducer --max-tasks 1 --per-task-worktree \
    --acceptance-lapse-ms 600000 \
    --heartbeat-interval 15s \
    --mcp-command "$NF cursor-mcp --cursor-command cursor-agent --model composer-2.5 --git-code-result --max-verifier-turns 5" \
    > "$LOGD/$1.log" 2>&1 &
}

spawn_integrator() {
  pgrep -f "nfltr worker --name local-integrator " >/dev/null && return
  echo "[$(now)] respawn local-integrator (cursor)"
  nohup "$NF" worker --name local-integrator --api-key "$KEY" --flavor cursor \
    --labels "role=integrator,flavor=cursor,tier=light" \
    --execution-roles integrator --max-tasks 1 \
    --acceptance-lapse-ms 600000 \
    --heartbeat-interval 15s \
    --mcp-command "$NF cursor-mcp --cursor-command cursor-agent --model composer-2.5 --git-code-result --max-verifier-turns 5" \
    > "$LOGD/local-integrator.log" 2>&1 &
}

spawn_claude() { # name model pretty effort
  pgrep -f "nfltr worker --name $1 " >/dev/null && return
  local cwd="$FLEET_HOME/$1"
  if [ ! -d "$cwd/.git" ]; then
    git clone -q "$NFLTR_REPO_URL" "$cwd" || return
  fi
  echo "[$(now)] respawn $1 (claude $2)"
  nohup "$NF" worker --name "$1" --api-key "$KEY" --flavor claude \
    --labels "model=$2,tier=heavy,flavor=claude" \
    --execution-roles implementer,verifier,reducer --max-tasks 1 \
    --acceptance-lapse-ms 600000 \
    --heartbeat-interval 15s \
    --mcp-command "$NF claude-mcp --cwd $cwd --model $2 --reasoning-effort $4 --co-author \"Claude $3\" --git-code-result" \
    > "$LOGD/$1.log" 2>&1 &
}

fleet_up() {
  if [ "$FLEET" = "cursor" ]; then
    spawn_cursor native-actor-0
    spawn_cursor native-actor-1
    spawn_cursor native-actor-2
    spawn_cursor native-actor-3
    spawn_cursor native-actor-4
    spawn_cursor native-actor-5
  else
    spawn_cursor native-actor-0
    spawn_cursor native-actor-1
    spawn_cursor native-actor-2
    spawn_cursor native-actor-3
    spawn_claude claude-sonnet-0 claude-sonnet-4-6 "Sonnet 4.6" high
    spawn_claude claude-haiku-0 claude-haiku-4-5-20251001 "Haiku 4.5" medium
  fi
  if [ "$MODE" = "frontier" ]; then
    spawn_integrator
  fi
}

rekick() {
  echo "[$(now)] PROGRESS STALLED — re-kicking fleet"
  pkill -9 -f "nfltr worker --name" 2>/dev/null
  pkill -9 -f "cursor-agent.* agent" 2>/dev/null
  pkill -9 -f "nfltr claude-mcp" 2>/dev/null
  pkill -9 -f "evolve/frontier_run.sh" 2>/dev/null
  sleep 3
  git -C "$REPO_ROOT" worktree prune 2>/dev/null
  rm -f "${HOME}/.local/share/nfltr/worker-state/native-actor-"*/active-task.json \
        "${HOME}/.local/share/nfltr/worker-state/local-integrator/active-task.json" 2>/dev/null
  "$NF" orch cancel-stale --older-than 3m --reason "supervise rekick" 2>/dev/null || true
  fleet_up
  sleep 15
}

landed() { grep -c "^- \[x\]" evolve/WAVES.md; }

active_orch_tasks() {
  local n
  n=$("$NF" orch list --active 2>/dev/null | awk 'NR>1 && NF' | wc -l | tr -d ' ')
  echo "${n:-0}"
}

frontier_in_flight() {
  pgrep -f "evolve/frontier_run.sh" >/dev/null || return 1
  [ "$(active_orch_tasks)" -gt 0 ]
}

frontier_driver_up() {
  if pgrep -f "evolve/frontier_run.sh" >/dev/null; then
    return
  fi
  local pending
  pending=$(grep -c "^- \[ \] " evolve/WAVES.md || true)
  [ "$pending" -eq 0 ] && return
  local rem=$(( end - $(date +%s) ))
  [ "$rem" -le 60 ] && return
  local impl_workers
  if [ "$FLEET" = "cursor" ]; then
    impl_workers="${AGENT_PREFIX}.native-actor-0,${AGENT_PREFIX}.native-actor-1,${AGENT_PREFIX}.native-actor-2,${AGENT_PREFIX}.native-actor-3,${AGENT_PREFIX}.native-actor-4,${AGENT_PREFIX}.native-actor-5"
  else
    impl_workers="${AGENT_PREFIX}.native-actor-0,${AGENT_PREFIX}.native-actor-1,${AGENT_PREFIX}.native-actor-2,${AGENT_PREFIX}.native-actor-3,${AGENT_PREFIX}.claude-sonnet-0,${AGENT_PREFIX}.claude-haiku-0"
  fi
  echo "[$(now)] frontier-run start: pending=$pending rem=${rem}s fleet=$FLEET"
  echo "===== $(date) supervise 5h run (pending=$pending budget=${rem}s) =====" >> "$REPO_ROOT/out/frontier-run.log"
  (
    cd "$REPO_ROOT" || exit 2
    export REPO_ROOT NFLTR="$NF" NFLTR_REPO_URL
    export ZIGRUN_IMPL_WORKERS="$impl_workers"
    export FRONTIER_BATCH_SIZE="$BATCH_SIZE" FRONTIER_CONCURRENCY="$BATCH_SIZE"
    export FRONTIER_BUDGET_SEC="$rem"
    nohup bash zigrun/evolve/frontier_run.sh >> "$REPO_ROOT/out/frontier-run.log" 2>&1 &
  )
}

last=$(landed); stall=0
echo "[$(now)] supervisor start: landed=$last budget=${BUDGET}s ($(( BUDGET / 3600 ))h) fleet=$FLEET nfltr=$NF root=$FRONTIER_ROOT_TASK_ID ends=$(date -r "$end" '+%Y-%m-%d %H:%M')"

while [ "$(date +%s)" -lt "$end" ]; do
  fleet_up
  if [ "$MODE" = "frontier" ]; then
    frontier_driver_up
  fi
  cur=$(landed)
  if [ "$cur" -gt "$last" ]; then
    echo "[$(now)] PROGRESS: landed $last -> $cur"
    last=$cur
    stall=0
  elif frontier_in_flight; then
    echo "[$(now)] in-flight (landed=$cur, active=$(active_orch_tasks)) — waiting"
    stall=0
  else
    stall=$((stall + 1))
    echo "[$(now)] no new land (landed=$cur, stall $stall/12)"
  fi
  if [ "$stall" -ge 4 ]; then
    echo "[$(now)] integrate_ready sweep"
    ( cd "$REPO_ROOT" && bash zigrun/evolve/integrate_ready.sh ) >> "$REPO_ROOT/out/supervise-frontier.log" 2>&1 || true
    cur=$(landed)
    if [ "$cur" -gt "$last" ]; then
      echo "[$(now)] PROGRESS (integrate_ready): landed $last -> $cur"
      last=$cur
      stall=0
    fi
  fi
  if [ "$stall" -ge 6 ]; then
    "$NF" orch cancel-stale --older-than 12m --reason "supervise stall" 2>/dev/null || true
  fi
  if [ "$stall" -ge 12 ]; then
    rekick
    stall=0
  fi
  sleep "$CHECK"
done

echo "[$(now)] supervisor budget reached — final landed=$(landed)"
