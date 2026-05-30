#!/usr/bin/env bash
# Land pending waves whose worker branches already exist on origin and pass
# gate_one (GREEN). Skips impl dispatch — the usual path when integrator LLM
# completed tasks but land_one never ran.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO"

current_branch="$(git branch --show-current || true)"
[ "$current_branch" = "main" ] || { echo "integrate_ready: must run on main" >&2; exit 2; }

landed=0
while IFS= read -r wave; do
  [ -n "$wave" ] || continue
  git ls-remote --exit-code origin "refs/heads/zigrun-${wave}" >/dev/null 2>&1 || continue
  echo "integrate_ready: trying $wave (branch on origin)"
  if bash zigrun/evolve/gate_one.sh "$wave" && bash zigrun/evolve/land_one.sh "$wave"; then
    landed=$((landed + 1))
  else
    echo "integrate_ready: $wave not ready (gate or land failed)" >&2
  fi
done < <(grep -oE '^- \[ \] [a-zA-Z0-9_]+' zigrun/evolve/WAVES.md | awk '{print $NF}')

echo "integrate_ready: landed $landed wave(s)"
exit 0
