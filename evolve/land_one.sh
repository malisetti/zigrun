#!/usr/bin/env bash
# Integrator step for the frontier-driven orch graph run. The relay routes
# this slice to a worker pinned with role=integrator (max-tasks=1, runs on
# the operator's machine), so commit+push+WAVES.md flip is naturally
# serialized — no flock needed.
#
# Contract: the upstream verifier slice has already approved the worker's
# branch `zigrun-<wave_id>` against the operator's untampered oracle. This
# script merges that branch into main, re-verifies the FULL differential
# suite (real zig), flips the WAVES.md checkbox, and pushes origin/main.
# On any verify regression it hard-rolls-back so a failed land leaves no
# pollution behind (the same atomic semantics land_wave.py:land_on_main had).
#
# Usage: bash zigrun/evolve/land_one.sh <wave_id>

set -euo pipefail

if [ $# -lt 1 ]; then
  echo "land_one: usage: $0 <wave_id>" >&2
  exit 2
fi
WAVE_ID="$1"

# Pin to the repo root regardless of caller's cwd (workers may invoke from
# their own work_dir).
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO"

# --- Preflight: clean main, wave is pending, branch is fetched -------------

current_branch="$(git branch --show-current || true)"
if [ "$current_branch" != "main" ]; then
  echo "land_one: must run from main (currently on '$current_branch')" >&2
  exit 2
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "land_one: working tree dirty — aborting to protect uncommitted work" >&2
  git status --short >&2
  exit 2
fi

if ! grep -qE "^- \[ \] ${WAVE_ID} \|" zigrun/evolve/WAVES.md; then
  echo "land_one: '${WAVE_ID}' is not pending in zigrun/evolve/WAVES.md (already landed or unknown)" >&2
  exit 2
fi

pending_zig="zigrun/oracle/pending/${WAVE_ID}.zig"
if [ ! -f "$pending_zig" ]; then
  echo "land_one: spec file missing: $pending_zig" >&2
  exit 2
fi

echo "land_one[$WAVE_ID]: fetching origin + worker branch zigrun-${WAVE_ID}"
git fetch --prune origin >/dev/null
git fetch origin "+refs/heads/zigrun-${WAVE_ID}:refs/remotes/origin/zigrun-${WAVE_ID}" >/dev/null \
  || { echo "land_one: worker branch zigrun-${WAVE_ID} not found on origin" >&2; exit 2; }

# Snapshot pre-merge HEAD so we can fully roll back on regression.
pre_head="$(git rev-parse HEAD)"

# --- Merge the worker's verified branch into main --------------------------

# Use --no-ff so the merge commit is the recorded landing point — even when
# the worker branch is purely ahead of main, the dedicated merge commit
# carries the integrator's audit trail (land_one ran, gate passed).
echo "land_one[$WAVE_ID]: merging origin/zigrun-${WAVE_ID} into main"
if ! git merge --no-ff --no-edit \
       -m "feat(zigrun): ${WAVE_ID} landed via orch integrator" \
       "origin/zigrun-${WAVE_ID}"; then
  echo "land_one: merge produced conflicts — aborting" >&2
  git merge --abort 2>/dev/null || true
  exit 1
fi

# --- Anti-tamper: overlay operator's oracle from pre-merge main -----------
# A malicious or sloppy worker may have edited oracle/diff.sh or oracle/*.zig
# files to fake green. We discard whatever the merge brought in for oracle/
# and replace it with the operator's pre-merge oracle, then re-apply the
# specific files this wave is allowed to touch.

echo "land_one[$WAVE_ID]: overlaying operator oracle (anti-tamper)"
git checkout "$pre_head" -- zigrun/oracle/

# The wave is allowed to: (a) move pending/<id>.zig into oracle/<id>.zig,
# (b) add <id> to oracle/check.sh and oracle/diff.sh. Apply those by hand
# from the merged source, NOT by trusting the worker's oracle diff.

if [ -f "$pending_zig" ]; then
  git mv "$pending_zig" "zigrun/oracle/${WAVE_ID}.zig"
fi

# Add the wave id to the progs=(...) array in check.sh + diff.sh if absent.
for suite in zigrun/oracle/check.sh zigrun/oracle/diff.sh; do
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
git add zigrun/oracle/

# --- Re-verify on the merged tree (real zig is truth) ----------------------

echo "land_one[$WAVE_ID]: rebuilding zigrun + running full differential suite"
( cd zigrun && cargo build --quiet ) || {
  echo "land_one: cargo build failed after merge — rolling back" >&2
  git reset --hard "$pre_head"
  exit 1
}

if ! bash zigrun/oracle/diff.sh; then
  echo "land_one: differential suite RED after merge — rolling back" >&2
  git reset --hard "$pre_head"
  ( cd zigrun && cargo build --quiet ) || true
  exit 1
fi

# --- Flip WAVES.md [ ] → [x] and bump FEATURES.md coverage ----------------

echo "land_one[$WAVE_ID]: flipping WAVES.md + bumping FEATURES.md"
python3 - "$WAVE_ID" <<'PY'
import re, sys
from pathlib import Path
wid = sys.argv[1]
waves = Path("zigrun/evolve/WAVES.md")
lines = waves.read_text().splitlines()
out, moved = [], None
for ln in lines:
    m = re.match(rf"- \[ \] {re.escape(wid)} \| (\S+) \| (.+)", ln)
    if m and moved is None:
        moved = f"- [x] {wid} | oracle/{wid}.zig | {m.group(2)} (landed via orch integrator vs real zig)"
        continue
    out.append(ln)
if moved is None:
    sys.exit("WAVES.md flip: pending entry vanished mid-flight; aborting flip")
last_x = max((i for i, l in enumerate(out) if l.startswith("- [x] ")), default=len(out) - 1)
out.insert(last_x + 1, moved)
waves.write_text("\n".join(out) + "\n")

features = Path("zigrun/FEATURES.md")
if features.exists():
    t = features.read_text()
    m = re.search(r"~(\d+) of ~80", t)
    if m:
        t = t.replace(m.group(0), f"~{int(m.group(1)) + 1} of ~80", 1)
        features.write_text(t)
PY

# Amend WAVES.md + FEATURES.md flip into the merge commit so the audit trail
# is one atomic commit instead of two.
git add zigrun/evolve/WAVES.md zigrun/FEATURES.md
git commit --amend --no-edit

# --- Push to origin/main ---------------------------------------------------

echo "land_one[$WAVE_ID]: pushing to origin/main"
target_sha="$(git rev-parse HEAD)"
for attempt in 1 2 3 4 5 6; do
  if git push origin HEAD:refs/heads/main; then
    server_sha="$(git ls-remote origin main | awk '{print $1}')"
    if [ "$server_sha" = "$target_sha" ]; then
      echo "land_one[$WAVE_ID]: LANDED — origin/main now at $target_sha"
      exit 0
    fi
  fi
  echo "land_one[$WAVE_ID]: push attempt $attempt didn't stick; sleeping 10s"
  sleep 10
done

echo "land_one[$WAVE_ID]: pushed but origin/main didn't converge — manual investigation needed" >&2
exit 1
