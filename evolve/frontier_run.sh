#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
cd "$REPO_ROOT"

NF="${NFLTR:-$REPO_ROOT/out/nfltr}"
[ -x "$NF" ] || NF="${HOME}/.local/bin/nfltr}"

BUDGET_SEC="${FRONTIER_BUDGET_SEC:-18000}"
BATCH_SIZE="${FRONTIER_BATCH_SIZE:-3}"
CONCURRENCY="${FRONTIER_CONCURRENCY:-3}"
SCHEDULER="${FRONTIER_SCHEDULER:-parallel}"
IMPL_WINDOW="${FRONTIER_IMPL_WINDOW:-$CONCURRENCY}"
IMPL_READY_AFTER="${FRONTIER_IMPL_READY_AFTER:-30s}"
IMPL_READY_POLL="${FRONTIER_IMPL_READY_POLL:-15s}"
DEFAULT_IMPL_READY_COMMAND='git fetch -q origin +refs/heads/main:refs/remotes/origin/main +refs/heads/zigrun-${item_id}:refs/remotes/origin/zigrun-${item_id} && git merge-base --is-ancestor refs/remotes/origin/main refs/remotes/origin/zigrun-${item_id}'
IMPL_READY_COMMAND="${FRONTIER_IMPL_READY_COMMAND:-$DEFAULT_IMPL_READY_COMMAND}"
IMPL_WORKERS="${ZIGRUN_IMPL_WORKERS:-agent-b147cc87.native-actor-0,agent-b147cc87.native-actor-1,agent-b147cc87.native-actor-2,agent-b147cc87.native-actor-3,agent-b147cc87.native-actor-4,agent-b147cc87.native-actor-5}"
INTEGRATOR_WORKER="${ZIGRUN_INTEGRATOR_WORKER:-agent-b147cc87.local-integrator}"
FRONTIER_ROOT_TASK_ID="${FRONTIER_ROOT_TASK_ID:-zigrun-frontier-$(date -u +%Y%m%dT%H%M%SZ)}"
export FRONTIER_ROOT_TASK_ID

if [ -f "${HOME}/.nfltr_new_key" ]; then
  export NFLTR_API_KEY="$(tr -d '[:space:]' < "${HOME}/.nfltr_new_key")"
fi

FRONTIER_FLAGS=(
  --frontier-file zigrun/evolve/WAVES.md
  --skip-missing-under zigrun
  --impl-objective @zigrun/evolve/impl-objective.template.md
  --gate-command 'bash zigrun/evolve/gate_one.sh ${item_id}'
  --integrate-command 'LAND_ONE_WAVE_ONLY=1 bash zigrun/evolve/land_one.sh ${item_id}'
  --scheduler "$SCHEDULER"
  --impl-window "$IMPL_WINDOW"
  --impl-ready-command "$IMPL_READY_COMMAND"
  --impl-ready-after "$IMPL_READY_AFTER"
  --impl-ready-poll "$IMPL_READY_POLL"
  --impl-workers "$IMPL_WORKERS"
  --integrator-worker "$INTEGRATOR_WORKER"
  --root-task-id "$FRONTIER_ROOT_TASK_ID"
  --batch-size "$BATCH_SIZE"
  --concurrency "$CONCURRENCY"
  --acceptance-lapse-ms 600000
  --stuck-accepted-timeout-ms 1800000
  --impl-timeout-ms 1500000
  --continue-on-empty-batch
)

prune_stale_slots() {
  echo "frontier_run: reclaiming stale worker slots (older than 5m)…"
  "$NF" orch cancel-stale --older-than 5m --reason "zigrun frontier slot reclaim" 2>/dev/null || true
}

run_batch() {
  local rem="$1"
  "$NF" orch frontier-run "${FRONTIER_FLAGS[@]}" --budget "${rem}s"
}

deadline=$(( $(date +%s) + BUDGET_SEC ))
echo "frontier_run: lineage root ${FRONTIER_ROOT_TASK_ID}"
prune_stale_slots

while [ "$(date +%s)" -lt "$deadline" ]; do
  rem=$(( deadline - $(date +%s) ))
  [ "$rem" -le 30 ] && break

  with_spec=0
  while IFS= read -r id; do
    [ -f "zigrun/oracle/pending/${id}.zig" ] && with_spec=$((with_spec + 1))
  done < <(grep -oE '^- \[ \] [a-zA-Z0-9_]+' zigrun/evolve/WAVES.md | awk '{print $NF}')

  [ "$with_spec" -eq 0 ] && break

  echo "frontier_run: batch start (${with_spec} runnable, ${rem}s left)"
  run_batch "$rem" || { echo "frontier_run: batch rc=$? — continuing"; prune_stale_slots; }
  sleep 5
done

echo "frontier_run: done (budget or frontier exhausted)"
