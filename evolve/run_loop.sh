#!/usr/bin/env bash
# Continuous evolving loop: land pending waves (those with a spec) back-to-back,
# autonomously, until the frontier is exhausted or each remaining wave has been
# attempted. No operator between waves — that is the point.
cd "$(dirname "$0")/.." || exit 2   # zigrun/
attempted=" "; landed=""; failed=""
for round in $(seq 1 8); do
  next=""
  for id in $(grep -oE '^- \[ \] [a-zA-Z0-9_]+' evolve/WAVES.md | awk '{print $NF}'); do
    [ -f "oracle/pending/$id.zig" ] || continue          # only waves with a spec
    case "$attempted" in *" $id "*) continue ;; esac      # skip already-attempted
    next="$id"; break
  done
  [ -z "$next" ] && { echo "LOOP: frontier exhausted / all attempted"; break; }
  echo "===== LOOP round $round: landing '$next' ====="
  if python3 evolve/land_wave.py "$next"; then landed="$landed $next"; else failed="$failed $next"; fi
  attempted="$attempted$next "
done
echo "===== CONTINUOUS LOOP DONE: landed=[$landed ] failed=[$failed ] ====="
