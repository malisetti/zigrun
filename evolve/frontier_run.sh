#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
cd "$REPO_ROOT"

NF="${NFLTR:-$REPO_ROOT/out/nfltr}"
[ -x "$NF" ] || NF="${HOME}/.local/bin/nfltr}"

BUDGET_SEC="${FRONTIER_BUDGET_SEC:-18000}"
BATCH_SIZE="${FRONTIER_BATCH_SIZE:-3}"
CONCURRENCY="${FRONTIER_CONCURRENCY:-3}"
IMPL_WORKERS="${ZIGRUN_IMPL_WORKERS:-agent-b147cc87.native-actor-0,agent-b147cc87.native-actor-1,agent-b147cc87.native-actor-2,agent-b147cc87.native-actor-3,agent-b147cc87.native-actor-4,agent-b147cc87.native-actor-5}"
INTEGRATOR_WORKER="${ZIGRUN_INTEGRATOR_WORKER:-agent-b147cc87.local-integrator}"

if [ -f "${HOME}/.nfltr_new_key" ]; then
  export NFLTR_API_KEY="$(tr -d '[:space:]' < "${HOME}/.nfltr_new_key")"
fi

FRONTIER_FLAGS=(
  --frontier-file zigrun/evolve/WAVES.md
  --skip-missing-under zigrun
  --impl-objective @zigrun/evolve/impl-objective.template.md
  --gate-command 'bash zigrun/evolve/gate_one.sh ${item_id}'
  --integrate-command 'bash -lc '"'"'set -euo pipefail; cd "'"$REPO_ROOT"'"; git checkout main; git pull --ff-only origin main; bash zigrun/evolve/land_one.sh ${item_id}'"'"
  --impl-workers "$IMPL_WORKERS"
  --integrator-worker "$INTEGRATOR_WORKER"
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
  bash zigrun/evolve/integrate_ready.sh >> "$REPO_ROOT/out/frontier-run.log" 2>&1 || true
  run_batch "$rem" || { echo "frontier_run: batch rc=$? — continuing"; prune_stale_slots; }
  sleep 5
done

echo "frontier_run: done (budget or frontier exhausted)"
