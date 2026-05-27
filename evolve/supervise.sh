#!/usr/bin/env bash
# 4-hour PROGRESS supervisor for the self-driving zigrun loop. Goal: keep
# progress happening, not just keep processes alive. Every ~8 min it:
#   - respawns any DEAD fleet worker (6 total),
#   - restarts the autoloop if it died (with the remaining budget),
#   - if no wave has LANDED in ~24 min, RE-KICKS the whole fleet (kill + prune
#     worktrees + respawn) to clear the stuck-in-accepted wedging.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2          # zigrun/
NF=/Users/b/.local/bin/nfltr
KEY=rpc_eb308b4f651879bde55a79de2acc1371916176bffdd1745d61dd8586997760a0831d79c9a288b96c46fad9441ddfa55c
LOGD=../out/fleet; mkdir -p "$LOGD"
CHECK=480
BUDGET="${SUP_BUDGET:-14400}"
start=$(date +%s)
now() { date +%H:%M; }

spawn_claude() { # name model labels co-author
  pgrep -f "nfltr worker --name $1 " >/dev/null && return
  echo "[$(now)] respawn $1"
  nohup "$NF" worker --name "$1" --api-key "$KEY" --flavor claude-code --labels "$3" \
    --execution-roles implementer,verifier,reducer,integrator --max-tasks 4 --per-task-worktree \
    --heartbeat-interval 15s \
    --mcp-command "nfltr claude-mcp --model $2 --reasoning-effort medium --permission-mode bypassPermissions --co-author \"$4\"" \
    > "$LOGD/$1.log" 2>&1 &
}
spawn_cursor() { # name
  pgrep -f "nfltr worker --name $1 " >/dev/null && return
  echo "[$(now)] respawn $1"
  nohup "$NF" worker --name "$1" --api-key "$KEY" --flavor cursor \
    --labels model=composer-2.5,tier=heavy,flavor=cursor \
    --execution-roles implementer,verifier,reducer,integrator --max-tasks 4 --per-task-worktree \
    --heartbeat-interval 15s \
    --mcp-command "nfltr cursor-mcp --cursor-command cursor-agent --model composer-2.5 --git-code-result --max-verifier-turns 5" \
    > "$LOGD/$1.log" 2>&1 &
}
fleet_up() {
  spawn_claude claude-sonnet-0 claude-sonnet-4-6 model=sonnet,tier=heavy,flavor=claude-code "Claude Sonnet 4.6"
  spawn_claude claude-sonnet-1 claude-sonnet-4-6 model=sonnet,tier=heavy,flavor=claude-code "Claude Sonnet 4.6"
  spawn_claude claude-haiku-0 claude-haiku-4-5 model=haiku,tier=light,flavor=claude-code "Claude Haiku 4.5"
  spawn_cursor native-actor-0; spawn_cursor native-actor-1; spawn_cursor native-actor-2
}
rekick() {
  echo "[$(now)] PROGRESS STALLED — re-kicking fleet (kill + prune + respawn)"
  pkill -9 -f "nfltr worker --name" 2>/dev/null; pkill -9 -f "cursor-agent.* agent" 2>/dev/null; sleep 3
  git worktree prune 2>/dev/null
  fleet_up
}
landed() { grep -c "^- \[x\]" evolve/WAVES.md; }    # cheap progress signal (no rebuild)

last=$(landed); stall=0
echo "[$(now)] supervisor start: landed=$last budget=${BUDGET}s"
while [ $(( $(date +%s) - start )) -lt "$BUDGET" ]; do
  fleet_up
  if ! pgrep -f "evolve/autoloop.sh" >/dev/null; then
    rem=$(( BUDGET - ($(date +%s) - start) ))
    echo "[$(now)] autoloop dead — restart (remaining ${rem}s)"
    BUDGET_SECONDS=$rem nohup bash evolve/autoloop.sh >> ../out/autoloop-long.log 2>&1 &
  fi
  cur=$(landed)
  if [ "$cur" -gt "$last" ]; then echo "[$(now)] PROGRESS: landed $last -> $cur"; last=$cur; stall=0
  else stall=$((stall + 1)); echo "[$(now)] no new land (landed=$cur, stall streak $stall/3)"; fi
  if [ "$stall" -ge 3 ]; then rekick; stall=0; fi
  sleep "$CHECK"
done
echo "[$(now)] supervisor budget reached — final landed=$(landed)"
