#!/usr/bin/env bash
# Operator-side acceptance gate for an orch dag slice.
#
# Fetches the worker's pushed branch, checks out its code into a throwaway
# worktree, and runs the oracle subset against THAT code (build + run real .zig
# programs). Exit 0 iff the requested oracle programs run to their expected exit
# codes. If the branch isn't on origin (worker didn't push / didn't fulfill), the
# gate fails — which is exactly the acceptance gate doing its job.
#
# Usage: gate.sh <branch> <oracle-name...>
#   bash zigrun/oracle/gate.sh zigrun-add add
set -uo pipefail
branch="${1:?usage: gate.sh <branch> <oracle-name...>}"
shift
wt="$(mktemp -d /tmp/zigrun-gate.XXXXXX)"
cleanup() { git worktree remove --force "$wt" 2>/dev/null; rm -rf "$wt"; }
trap cleanup EXIT

if ! git fetch -q origin "$branch" 2>/dev/null; then
  echo "GATE: branch '$branch' not found on origin (worker did not push its work)"
  exit 3
fi
if ! git worktree add -q --detach "$wt" FETCH_HEAD 2>/dev/null; then
  echo "GATE: could not check out '$branch'"
  exit 3
fi
if [ ! -f "$wt/zigrun/oracle/check.sh" ]; then
  echo "GATE: '$branch' has no zigrun/oracle/check.sh"
  exit 3
fi
bash "$wt/zigrun/oracle/check.sh" "$@"
