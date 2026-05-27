#!/usr/bin/env bash
# Perpetual self-feeding loop — the orchestration keeps ITSELF busy, no operator:
#   frontier empty? -> self-author the next spec (spec_author.py; real zig validates it)
#   pending wave?    -> land it (land_wave.py: dispatch->recover->gate->merge->bookkeep->push)
# Skips waves it already failed (no infinite retry) and BACKS OFF after 2 consecutive
# land failures (likely a feature beyond worker capability) so it can't burn forever.
cd "$(dirname "$0")/.." || exit 2   # zigrun/
attempted=" "; fails=0; landed=""; failed=""
for round in $(seq 1 16); do
  next=""
  for id in $(grep -oE '^- \[ \] [a-zA-Z0-9_]+' evolve/WAVES.md | awk '{print $NF}'); do
    [ -f "oracle/pending/$id.zig" ] || continue
    case "$attempted" in *" $id "*) continue ;; esac
    next="$id"; break
  done
  if [ -z "$next" ]; then
    echo "===== AUTOLOOP round $round: frontier empty — SELF-AUTHORING next spec ====="
    if python3 evolve/spec_author.py; then continue
    else echo "AUTOLOOP: nothing left to author — stopping."; break; fi
  fi
  echo "===== AUTOLOOP round $round: landing '$next' ====="
  if python3 evolve/land_wave.py "$next"; then landed="$landed $next"; fails=0
  else failed="$failed $next"; fails=$((fails + 1)); fi
  attempted="$attempted$next "
  if [ "$fails" -ge 2 ]; then
    echo "AUTOLOOP: 2 consecutive land failures — backing off (likely beyond worker capability)."
    break
  fi
done
echo "===== AUTOLOOP DONE: landed=[$landed ] failed=[$failed ] ====="
