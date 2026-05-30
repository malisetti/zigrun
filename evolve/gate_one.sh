#!/usr/bin/env bash
# Operator-side acceptance gate for one wave, run as orch-dag Accept on the
# impl node. The implementer worker has finished and pushed branch
# `zigrun-<wave_id>` to origin; this script proves the work is real by
# overlaying it onto the operator's UN-TAMPERED oracle in a scratch tree
# and running the full differential suite against real zig. Exit 0 = OK to
# integrate. Exit non-zero = the orch-dag node fails and its integ child
# is skipped (a clean rejection — no operator tree mutation).
#
# Usage: bash zigrun/evolve/gate_one.sh <wave_id>

set -uo pipefail

if [ $# -lt 1 ]; then
  echo "gate_one: usage: $0 <wave_id>" >&2
  exit 2
fi
WAVE_ID="$1"

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO"

# 1. Fetch the worker's branch from origin.
echo "gate_one[$WAVE_ID]: fetching origin/zigrun-${WAVE_ID}"
if ! git fetch origin "+refs/heads/zigrun-${WAVE_ID}:refs/remotes/origin/zigrun-${WAVE_ID}" >/dev/null 2>&1; then
  echo "gate_one[$WAVE_ID]: worker branch zigrun-${WAVE_ID} not on origin — worker likely never pushed" >&2
  exit 1
fi

# 2. Make a scratch worktree from origin/zigrun-<id>.
SCRATCH="$(mktemp -d -t "zigrun-gate-${WAVE_ID}.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT

echo "gate_one[$WAVE_ID]: provisioning scratch worktree at $SCRATCH"
if ! git worktree add --force --detach "$SCRATCH" "origin/zigrun-${WAVE_ID}" >/dev/null 2>&1; then
  echo "gate_one[$WAVE_ID]: git worktree add failed" >&2
  exit 1
fi

# 3. Overlay the operator's UN-TAMPERED oracle (anti-tamper) on top of the
#    worker's branch. The worker may have edited oracle/diff.sh, oracle/*.zig,
#    or the suite arrays to fake green; replacing oracle/ entirely with the
#    operator's main-tree copy makes that impossible.
echo "gate_one[$WAVE_ID]: overlaying operator's oracle into scratch"
rm -rf "$SCRATCH/zigrun/oracle"
cp -R "$REPO/zigrun/oracle" "$SCRATCH/zigrun/oracle"

# 4. The wave is allowed to promote zigrun/oracle/pending/<id>.zig into
#    zigrun/oracle/<id>.zig and add <id> to the suite arrays. The operator's
#    oracle still has the pending file but not the landed one or the suite
#    update — apply both ourselves so the diff.sh suite includes the new wave
#    when scanning.
if [ -f "$SCRATCH/zigrun/oracle/pending/${WAVE_ID}.zig" ]; then
  cp "$SCRATCH/zigrun/oracle/pending/${WAVE_ID}.zig" "$SCRATCH/zigrun/oracle/${WAVE_ID}.zig"
fi
for suite in "$SCRATCH/zigrun/oracle/check.sh" "$SCRATCH/zigrun/oracle/diff.sh"; do
  if [ -f "$suite" ] && ! grep -qE "(^|[ (])${WAVE_ID}([ )]|$)" "$suite"; then
    python3 - "$suite" "$WAVE_ID" <<'PY'
import re, sys
p, wid = sys.argv[1], sys.argv[2]
src = open(p).read()
out = re.sub(r"(progs=\(add[^)]*?)\)", lambda m: m.group(1) + " " + wid + ")"
             if wid not in m.group(1) else m.group(0), src, count=1)
open(p, "w").write(out)
PY
  fi
done

# 5. Build zigrun in scratch + run the differential suite.
echo "gate_one[$WAVE_ID]: building zigrun in scratch"
if ! ( cd "$SCRATCH/zigrun" && cargo build --quiet ) ; then
  echo "gate_one[$WAVE_ID]: cargo build FAILED in scratch tree" >&2
  exit 1
fi

echo "gate_one[$WAVE_ID]: running differential suite (real zig)"
if ! ( cd "$SCRATCH/zigrun" && bash oracle/diff.sh "${WAVE_ID}" ); then
  echo "gate_one[$WAVE_ID]: differential gate RED — wave-specific diff.sh ${WAVE_ID} failed" >&2
  exit 1
fi

if [ "${GATE_ONE_WAVE_ONLY:-}" != "1" ]; then
  if ! ( cd "$SCRATCH/zigrun" && bash oracle/diff.sh ); then
    echo "gate_one[$WAVE_ID]: differential RED — full suite regressed" >&2
    exit 1
  fi
fi

echo "gate_one[$WAVE_ID]: GREEN — branch zigrun-${WAVE_ID} verified vs real zig"
exit 0
