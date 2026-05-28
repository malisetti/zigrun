#!/usr/bin/env bash
# Canonical launcher for zigrun frontier evolution via `nfltr orch frontier-run`.
# Keeps the cursor fleet saturated: batch size = worker count, loops until
# budget or frontier empty (continues after zero-land batches).
set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
cd "$REPO_ROOT"

NF="${NFLTR:-/Users/b/.local/bin/nfltr}"
BUDGET_SEC="${FRONTIER_BUDGET_SEC:-14400}"
BATCH_SIZE="${FRONTIER_BATCH_SIZE:-6}"
CONCURRENCY="${FRONTIER_CONCURRENCY:-6}"

# Six cursor implementers — one wave pinned per worker per batch.
IMPL_WORKERS="${ZIGRUN_IMPL_WORKERS:-agent-b147cc87.native-actor-0,agent-b147cc87.native-actor-1,agent-b147cc87.native-actor-2,agent-b147cc87.native-actor-3,agent-b147cc87.native-actor-4,agent-b147cc87.native-actor-5}"
INTEGRATOR_WORKER="${ZIGRUN_INTEGRATOR_WORKER:-agent-b147cc87.local-integrator}"

if [ -f "${HOME}/.nfltr_new_key" ]; then
  export NFLTR_API_KEY="$(tr -d '[:space:]' < "${HOME}/.nfltr_new_key")"
fi

EXTRA=()
if [ "${1:-}" = "--dry-run" ]; then
  EXTRA+=(--dry-run)
  exec "$NF" orch frontier-run \
    --frontier-file zigrun/evolve/WAVES.md \
    --skip-missing-under zigrun \
    --impl-objective @zigrun/evolve/impl-objective.template.md \
    --gate-command 'bash zigrun/evolve/gate_one.sh ${item_id}' \
    --integrate-command 'bash zigrun/evolve/land_one.sh ${item_id}' \
    --impl-workers "$IMPL_WORKERS" \
    --integrator-worker "$INTEGRATOR_WORKER" \
    --batch-size "$BATCH_SIZE" \
    --concurrency "$CONCURRENCY" \
    --budget "${BUDGET_SEC}s" \
    "${EXTRA[@]}"
fi

prune_stale_slots() {
  echo "frontier_run: reclaiming stale worker slots (older than 5m)…"
  "$NF" orch cancel-stale --older-than 5m --reason "zigrun frontier slot reclaim" 2>/dev/null || true
  # Force-cancel duplicate accepted tasks on cursor impl workers (max-tasks=1).
  python3 - "$NF" "$IMPL_WORKERS" <<'PY' || true
import json, subprocess, sys
nf, workers_csv = sys.argv[1], sys.argv[2]
workers = set(workers_csv.split(","))
raw = subprocess.run([nf, "orch", "list", "--active", "--output", "json"],
                     capture_output=True, text=True, timeout=120)
if raw.returncode != 0 or not raw.stdout.strip():
    sys.exit(0)
tasks = json.loads(raw.stdout)
by_worker = {}
for t in tasks:
    w = t.get("worker_id", "")
    if w in workers or w.replace("agent-b147cc87.", "") in {x.split(".")[-1] for x in workers}:
        key = w if w in workers else next((x for x in workers if x.endswith(w)), w)
        by_worker.setdefault(key, []).append(t)
for w, ts in by_worker.items():
    ts.sort(key=lambda x: x.get("created_at", ""), reverse=True)
    for dup in ts[1:]:
        tid = dup["task_id"]
        print(f"prune duplicate {tid} on {w}", flush=True)
        subprocess.run([nf, "orch", "cancel", "--worker", w, "--task", tid,
                        "--force", "--reason", "zigrun frontier slot dedup"],
                       capture_output=True, timeout=60)
PY
}

run_batch() {
  local rem="$1"
  "$NF" orch frontier-run \
    --frontier-file zigrun/evolve/WAVES.md \
    --skip-missing-under zigrun \
    --impl-objective @zigrun/evolve/impl-objective.template.md \
    --gate-command 'bash zigrun/evolve/gate_one.sh ${item_id}' \
    --integrate-command 'bash zigrun/evolve/land_one.sh ${item_id}' \
    --impl-workers "$IMPL_WORKERS" \
    --integrator-worker "$INTEGRATOR_WORKER" \
    --batch-size "$BATCH_SIZE" \
    --concurrency "$CONCURRENCY" \
    --budget "${rem}s"
}

deadline=$(( $(date +%s) + BUDGET_SEC ))
prune_stale_slots

while [ "$(date +%s)" -lt "$deadline" ]; do
  rem=$(( deadline - $(date +%s) ))
  [ "$rem" -le 30 ] && break

  pending=$(grep -cE '^- \[ \]' zigrun/evolve/WAVES.md 2>/dev/null || echo 0)
  with_spec=0
  while IFS= read -r id; do
    [ -f "zigrun/oracle/pending/${id}.zig" ] && with_spec=$((with_spec + 1))
  done < <(grep -oE '^- \[ \] [a-zA-Z0-9_]+' zigrun/evolve/WAVES.md | awk '{print $NF}')

  if [ "$with_spec" -eq 0 ]; then
    echo "frontier_run: no pending waves with oracle specs — done"
    break
  fi

  echo "frontier_run: batch start (${with_spec} runnable pending, ${rem}s left)"
  if run_batch "$rem"; then
    rc=0
  else
    rc=$?
    echo "frontier_run: batch exited rc=$rc — continuing (${rem}s left)"
    prune_stale_slots
  fi
  sleep 5
done

echo "frontier_run: budget elapsed or frontier exhausted"
