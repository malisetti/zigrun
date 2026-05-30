#!/usr/bin/env bash
# Land pending waves whose worker branches already exist on origin and pass
# wave-only gate_one. Bounded so supervise stall sweeps finish quickly.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO"

current_branch="$(git branch --show-current || true)"
[ "$current_branch" = "main" ] || { echo "integrate_ready: must run on main" >&2; exit 2; }

MAX="${INTEGRATE_READY_MAX:-5}"
landed=0
tried=0
while IFS= read -r wave; do
  [ -n "$wave" ] || continue
  [ "$tried" -ge "$MAX" ] && break
  git ls-remote --exit-code origin "refs/heads/zigrun-${wave}" >/dev/null 2>&1 || continue
  tried=$((tried + 1))
  echo "integrate_ready: trying $wave (branch on origin)"
  if GATE_ONE_WAVE_ONLY=1 bash zigrun/evolve/gate_one.sh "$wave" \
      && LAND_ONE_WAVE_ONLY=1 bash zigrun/evolve/land_one.sh "$wave"; then
    landed=$((landed + 1))
  else
    echo "integrate_ready: $wave not ready (gate or land failed)" >&2
  fi
done < <(grep -oE '^- \[ \] [a-zA-Z0-9_]+' zigrun/evolve/WAVES.md | awk '{print $NF}')

echo "integrate_ready: landed $landed wave(s) (tried $tried, max $MAX)"
exit 0
