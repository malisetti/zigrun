#!/usr/bin/env bash
# Canonical launcher for zigrun frontier evolution via `nfltr orch frontier-run`.
# Run from repo root or via supervise.sh (which sets REPO_ROOT).
set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
cd "$REPO_ROOT"

NF="${NFLTR:-/Users/b/.local/bin/nfltr}"
BUDGET="${FRONTIER_BUDGET:-4h}"
BATCH_SIZE="${FRONTIER_BATCH_SIZE:-4}"
CONCURRENCY="${FRONTIER_CONCURRENCY:-4}"

# Override with env or edit after `nfltr orch workers` if agent prefix changes.
IMPL_WORKERS="${ZIGRUN_IMPL_WORKERS:-agent-b147cc87.claude-sonnet-0,agent-b147cc87.claude-sonnet-1,agent-b147cc87.claude-haiku-0,agent-b147cc87.native-actor-0,agent-b147cc87.native-actor-1,agent-b147cc87.native-actor-2}"
INTEGRATOR_WORKER="${ZIGRUN_INTEGRATOR_WORKER:-agent-b147cc87.local-integrator}"

if [ -f "${HOME}/.nfltr_new_key" ]; then
  export NFLTR_API_KEY="$(tr -d '[:space:]' < "${HOME}/.nfltr_new_key")"
fi

EXTRA=()
if [ "${1:-}" = "--dry-run" ]; then
  EXTRA+=(--dry-run)
fi

exec "$NF" orch frontier-run \
  --frontier-file zigrun/evolve/WAVES.md \
  --skip-missing-under zigrun/oracle/pending \
  --impl-objective @zigrun/evolve/impl-objective.template.md \
  --gate-command 'bash zigrun/evolve/gate_one.sh ${item_id}' \
  --integrate-command 'bash zigrun/evolve/land_one.sh ${item_id}' \
  --impl-workers "$IMPL_WORKERS" \
  --integrator-worker "$INTEGRATOR_WORKER" \
  --batch-size "$BATCH_SIZE" \
  --concurrency "$CONCURRENCY" \
  --budget "$BUDGET" \
  "${EXTRA[@]}"
